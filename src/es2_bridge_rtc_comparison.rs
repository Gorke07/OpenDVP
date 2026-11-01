//! Explicit settled RTC comparison. Native outputs never initialize or repair
//! the local clock after its fixture-authored initial input has been admitted.
use super::{Comparison, RtcDecoderObservation, RtcNativeObservation, Session};
use crate::{
    plc::{self, Device, Simulator},
    rtc_admission::{
        AdmissionError, AdmissionStep, CurrentImage, DurableComparison, RtcAdmission,
        RtcBoundaryMonitor, RtcControlFlags, RtcInitialInput, RtcInputEvent, SettledSameSecond,
        TimeEnvelope,
    },
};
use serde::Serialize;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, Serialize)]
pub struct RtcComparisonBudgets {
    /// Entire qualified read batch, including all durable operand/output reads.
    pub max_capture_us: u64,
    /// One raw owned-process observation; distinct from the complete batch.
    pub max_observation_us: u64,
    pub max_window_us: u64,
    pub max_between_us: u64,
    /// Start outside the late part of the host second. This is an admission
    /// margin, not a guarantee about subsequent OS scheduling.
    pub latest_start_phase_us: u32,
    pub continuous_host_clock_assumed: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct RtcWindowEvidence {
    pub before: RtcBoundaryMonitor,
    pub after: RtcBoundaryMonitor,
    pub independently_predicted_next_pc: u16,
    pub durable: Vec<DurableComparison>,
    pub error: Option<AdmissionError>,
}
#[derive(Debug, Clone, Serialize)]
pub enum RtcDecoderExpectation {
    Active {
        header_pc: u16,
        outcome: crate::rtc_clock::RtcOutcome,
    },
    Skipped {
        header_pc: u16,
    },
}
#[derive(Debug, Clone, Serialize)]
pub struct RtcDecoderEvidence {
    pub expectation: RtcDecoderExpectation,
    pub before: RtcDecoderObservation,
    pub after: RtcDecoderObservation,
    pub idle_repeat: RtcDecoderObservation,
    pub expected_error_bit: bool,
    pub error: Option<AdmissionError>,
}
#[derive(Debug, Clone, Serialize)]
pub struct RtcComparisonEvidence {
    pub declared: RtcInitialInput,
    pub policy: SettledSameSecond,
    pub budgets: RtcComparisonBudgets,
    pub observations: Vec<RtcNativeObservation>,
    pub input_phase_waits: Vec<RtcInputPhaseWait>,
    pub input_ledger: Vec<RtcInputEvent>,
    pub windows: Vec<RtcWindowEvidence>,
    pub accepted_windows: u64,
    pub error: Option<AdmissionError>,
    pub decoder_diagnostic_mode: bool,
    pub decoder_diagnostics: Vec<RtcDecoderEvidence>,
    pub decoder_observations: Vec<RtcDecoderObservation>,
    pub scope: &'static str,
}
#[derive(Debug, Clone, Serialize)]
pub struct RtcInputPhaseWait {
    pub site: &'static str,
    pub independently_predicted_pc: u16,
    pub observation_index: usize,
    pub awaiting_wall_second: u8,
    pub retained_sampled_second: u8,
}
pub(super) struct RtcSessionState {
    origin: Instant,
    admission: RtcAdmission,
    evidence: RtcComparisonEvidence,
}

fn inconclusive(s: impl Into<String>) -> AdmissionError {
    AdmissionError::Inconclusive(s.into())
}
fn transport(s: String) -> AdmissionError {
    inconclusive(s)
}
fn render(error: &AdmissionError) -> String {
    format!("RTC comparison {error:?}")
}
fn epoch() -> Result<Duration, AdmissionError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| inconclusive(e.to_string()))
}
/// Only a stable, exactly one-second-old environmental sample may be awaited
/// BEFORE seed delivery or BEFORE the next instruction. No numerical output is
/// used to choose the wait, and this is never called inside an active window.
fn stable_stale_host_sample(c: &RtcNativeObservation) -> Result<bool, AdmissionError> {
    if !c.stable_fields() {
        return Err(inconclusive(
            "Unstable RTC capture cannot become an input-phase wait",
        ));
    }
    let wall = (c.host_epoch_second_end % 60) as u8;
    let sampled = c.clock_after[6];
    if sampled == wall {
        return Ok(false);
    }
    if sampled != (wall + 59) % 60 || c.clock_after[14] != sampled {
        return Err(inconclusive(
            "RTC environmental sample is not a coherent one-second-old input",
        ));
    }
    Ok(true)
}
fn check_independent_paused_pc(
    c: &RtcNativeObservation,
    expected: u16,
) -> Result<(), AdmissionError> {
    if c.pc_before != expected || c.pc_after != expected {
        return Err(AdmissionError::ConformanceFailure {
            field: "paused PC before RTC input-phase wait".into(),
            expected: expected.to_string(),
            observed: format!("{}..{}", c.pc_before, c.pc_after),
        });
    }
    Ok(())
}
fn decoder_expected_bit(
    expectation: &RtcDecoderExpectation,
    before: &RtcDecoderObservation,
) -> Result<bool, AdmissionError> {
    use crate::rtc_clock::RtcOutcome;
    match expectation {
        RtcDecoderExpectation::Skipped { .. } => Ok(before.status_after & 0x10 != 0),
        RtcDecoderExpectation::Active { outcome, .. } => match outcome {
            RtcOutcome::Complete => Ok(false),
            RtcOutcome::PartialRead { .. } | RtcOutcome::PlcError { .. } => Ok(true),
            _ => Err(inconclusive(
                "RTC decoder diagnostic has no fresh active outcome",
            )),
        },
    }
}
fn validate_decoder_evidence(
    expectation: &RtcDecoderExpectation,
    before: &RtcDecoderObservation,
    after: &RtcDecoderObservation,
    idle: &RtcDecoderObservation,
    next_pc: u16,
) -> Result<(), AdmissionError> {
    let header = match expectation {
        RtcDecoderExpectation::Active { header_pc, .. }
        | RtcDecoderExpectation::Skipped { header_pc } => *header_pc,
    };
    if before.pc_before != header
        || before.pc_after != header
        || after.pc_before != next_pc
        || after.pc_after != next_pc
        || idle.pc_before != next_pc
        || idle.pc_after != next_pc
    {
        return Err(inconclusive(
            "RTC decoder status was not captured at its independently predicted paused PC",
        ));
    }
    let expected = decoder_expected_bit(expectation, before)?;
    for capture in [before, after, idle] {
        if (capture.status_before ^ capture.status_after) & 0x10 != 0 {
            return Err(inconclusive(
                "RTC internal decoder bit changed during paired RPM; handler-exit status unqualified",
            ));
        }
    }
    if [
        after.status_before,
        after.status_after,
        idle.status_before,
        idle.status_after,
    ]
    .into_iter()
    .any(|s| (s & 0x10 != 0) != expected)
    {
        return Err(inconclusive(
            "RTC internal decoder status did not persist as statically predicted through observation; no diagnostic parity claim",
        ));
    }
    if matches!(
        expectation,
        RtcDecoderExpectation::Active {
            outcome: crate::rtc_clock::RtcOutcome::PartialRead { written: 0 },
            ..
        }
    ) && [
        after.status_before,
        after.status_after,
        idle.status_before,
        idle.status_after,
    ]
    .into_iter()
    .any(|s| s & 0x70 != 0x50)
    {
        return Err(inconclusive(
            "RTC C200+ zero-store natural32 decoder classification was not observed",
        ));
    }
    Ok(())
}
fn check_controls(
    expected: &Simulator,
    observed: &RtcNativeObservation,
) -> Result<(), AdmissionError> {
    let want = [
        expected.memory.system_bit(1016),
        expected.memory.system_bit(1017),
    ];
    let flags = observed.controls();
    let actual = [flags[0], flags[1]];
    if want != actual {
        return Err(AdmissionError::ConformanceFailure {
            field: "M1016/M1017 before phase wait".into(),
            expected: format!("{want:?}"),
            observed: format!("{actual:?}"),
        });
    }
    Ok(())
}
fn monitor(
    observation: &RtcNativeObservation,
    budget: RtcComparisonBudgets,
) -> Result<RtcBoundaryMonitor, AdmissionError> {
    let [m1016, m1017, m1082] = observation.controls();
    Ok(RtcBoundaryMonitor {
        host_stamp: observation.host_stamp(),
        shadow: observation.shadow(),
        previous_second: observation.clock_after[14],
        public: observation.public_year_first,
        controls: RtcControlFlags {
            m1016,
            m1017,
            m1082,
        },
        pc: observation.pc_after,
        time: TimeEnvelope {
            monotonic_begin_us: observation.monotonic_begin_us,
            monotonic_end_us: observation.monotonic_end_us,
            host_second_begin: i64::try_from(observation.host_epoch_second_begin)
                .map_err(|_| inconclusive("Host epoch overflow"))?,
            host_second_end: i64::try_from(observation.host_epoch_second_end)
                .map_err(|_| inconclusive("Host epoch overflow"))?,
            continuity_epoch: 0,
            continuous_host_clock_assumed: budget.continuous_host_clock_assumed,
        },
    })
}
impl RtcSessionState {
    fn sync(&mut self) {
        self.evidence.input_ledger = self.admission.input_ledger().to_vec();
        self.evidence.accepted_windows = self.admission.accepted_windows();
    }
    fn reject(&mut self, error: AdmissionError) -> String {
        let error = self.admission.reject_external(error);
        self.evidence.error = Some(error.clone());
        self.sync();
        render(&error)
    }
}
impl Session {
    pub(super) fn preflight_rtc_action(
        &mut self,
        action: &crate::indexed_rtc::IndexedRtcAction,
    ) -> Result<(), String> {
        let memory = self.local.memory.clone();
        let native = if self.rtc_decoder_diagnostics {
            action.qualify_native_decoder_with(&memory, |captured, devices| {
                self.capture_operand_storage(captured, devices)
            })
        } else {
            action.qualify_native_with(&memory, |captured, devices| {
                self.capture_operand_storage(captured, devices)
            })
        };
        let mut independent = memory;
        let local = action.execute(&mut independent, true, &mut Default::default());
        if let crate::rtc_clock::RtcOutcome::Limitation(reason) = &native.outcome {
            return Err(format!("Native RTC operands unqualified: {reason}"));
        }
        if let crate::rtc_clock::RtcOutcome::Limitation(reason) = &local {
            return Err(format!("Independent RTC operands unqualified: {reason}"));
        }
        if local != native.outcome {
            return Err(format!(
                "Independent RTC operand disposition {local:?} differs from captured-input safety replay {:?}",
                native.outcome
            ));
        }
        self.local.observed_devices.extend(native.observed);
        if self.rtc_decoder_diagnostics {
            self.rtc_decoder_expected = Some(RtcDecoderExpectation::Active {
                header_pc: self
                    .local
                    .current_plan_pc()
                    .ok_or("RTC diagnostic source cursor missing")?,
                outcome: local,
            });
        }
        Ok(())
    }
    pub fn rtc_comparison_evidence(&self) -> Option<&RtcComparisonEvidence> {
        self.rtc_comparison.as_ref().map(|s| &s.evidence)
    }
    /// Enables guarded partial/invalid-resolver cases ONLY as separately
    /// labeled decoder diagnostics. Does not promote the normal native domain.
    pub fn configure_rtc_decoder_diagnostics(
        &mut self,
        declared: RtcInitialInput,
        budgets: RtcComparisonBudgets,
    ) -> Result<(), String> {
        if self.rtc_comparison.is_some() || self.rtc_mode_active {
            return Err("RTC diagnostic input already attempted".into());
        }
        self.rtc_decoder_diagnostics = true;
        let result = self.configure_rtc_comparison(declared, budgets);
        // Early validation can fail before an admission attempt exists. Such a
        // failure must not leak the diagnostic domain into a later normal call.
        if result.is_err() && self.rtc_comparison.is_none() {
            self.rtc_decoder_diagnostics = false;
        }
        result
    }
    /// Enable once, AFTER upload and BEFORE any PLC body. Budgets and the
    /// continuity assumption must be explicit; they have no implicit defaults.
    pub fn configure_rtc_comparison(
        &mut self,
        declared: RtcInitialInput,
        budgets: RtcComparisonBudgets,
    ) -> Result<(), String> {
        if self.rtc_comparison.is_some() || self.rtc_mode_active {
            return Err("RTC initial input already attempted; upload a fresh program".into());
        }
        let image = CurrentImage::from_sha256(&self.digest).map_err(|e| render(&e))?;
        let policy = SettledSameSecond {
            image,
            max_capture_us: budgets.max_capture_us,
            max_window_us: budgets.max_window_us,
            max_between_us: budgets.max_between_us,
        };
        let admission = RtcAdmission::new(policy).map_err(|e| render(&e))?;
        if !budgets.continuous_host_clock_assumed
            || budgets.max_observation_us == 0
            || budgets.max_observation_us > budgets.max_capture_us
            || budgets.latest_start_phase_us == 0
            || u64::from(budgets.latest_start_phase_us) + budgets.max_window_us >= 1_000_000
        {
            return Err("RTC configuration needs an explicit continuity assumption and a positive clock margin".into());
        }
        if self.faulted
            || self.online_active
            || self.teaching_tick_replay.is_some()
            || self.local.scans != 0
            || self.local.current_plan_pc().is_some()
            || !self.program.as_ref().is_some_and(|p| {
                p.networks
                    .iter()
                    .flat_map(|n| &n.actions)
                    .any(crate::rtc_clock::is_instruction)
            })
        {
            return Err("RTC input must be declared after a successful RTC program upload and before its first body".into());
        }
        let mut state = RtcSessionState {
            origin: Instant::now(),
            admission,
            evidence: RtcComparisonEvidence {
                declared,
                policy,
                budgets,
                observations: Vec::new(),
                input_phase_waits: Vec::new(),
                input_ledger: Vec::new(),
                windows: Vec::new(),
                accepted_windows: 0,
                error: None,
                decoder_diagnostic_mode: self.rtc_decoder_diagnostics,
                decoder_diagnostics: Vec::new(),
                decoder_observations: Vec::new(),
                scope: "Conditional bounded settled endpoints of owned unchanged images; no lossless/atomic clock capture or transient handler-store claim",
            },
        };
        let result = self.configure_rtc_inner(&mut state);
        let result = result.map_err(|e| state.reject(e));
        state.sync();
        self.rtc_comparison = Some(state);
        if result.is_err() {
            self.faulted = true;
        } else {
            self.rtc_mode_active = true;
        }
        result
    }
    fn rtc_capture(
        &mut self,
        state: &mut RtcSessionState,
    ) -> Result<RtcNativeObservation, AdmissionError> {
        if state.evidence.observations.len() >= 100_000 {
            return Err(inconclusive("RTC observation ledger capacity reached"));
        }
        let capture = self.observe_rtc_boundary(state.origin).map_err(transport)?;
        state.evidence.observations.push(capture.clone());
        if !capture.stable_fields()
            || capture
                .monotonic_end_us
                .saturating_sub(capture.monotonic_begin_us)
                > state.evidence.budgets.max_observation_us
        {
            return Err(inconclusive(
                "RTC capture is torn, crosses a host second, or exceeds its declared budget",
            ));
        }
        Ok(capture)
    }
    fn rtc_wait_start_phase(&self, state: &RtcSessionState) -> Result<(), AdmissionError> {
        let start = Instant::now();
        while epoch()?.subsec_micros() > state.evidence.budgets.latest_start_phase_us {
            if start.elapsed() >= Duration::from_secs(1) {
                return Err(inconclusive("RTC clock phase admission timed out"));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    }
    fn rtc_await_input_phase(
        state: &mut RtcSessionState,
        capture: &RtcNativeObservation,
        site: &'static str,
        pc: u16,
    ) -> Result<bool, AdmissionError> {
        if !stable_stale_host_sample(capture)? {
            return Ok(false);
        }
        state.evidence.input_phase_waits.push(RtcInputPhaseWait {
            site,
            independently_predicted_pc: pc,
            observation_index: state.evidence.observations.len() - 1,
            awaiting_wall_second: (capture.host_epoch_second_end % 60) as u8,
            retained_sampled_second: capture.clock_after[6],
        });
        Ok(true)
    }
    fn configure_rtc_inner(&mut self, state: &mut RtcSessionState) -> Result<(), AdmissionError> {
        self.rtc_wait_start_phase(state)?;
        let deadline = Instant::now();
        let before = loop {
            let capture = self.rtc_capture(state)?;
            if deadline.elapsed().as_micros() > u128::from(state.evidence.policy.max_window_us) {
                return Err(inconclusive(
                    "RTC initial service capture exceeded setup deadline",
                ));
            }
            if capture.pc_after != 0 {
                return Err(inconclusive(
                    "RTC initialization requires independently declared PC0",
                ));
            }
            if Self::rtc_await_input_phase(state, &capture, "before_seed", 0)? {
                continue;
            }
            if capture.service_phase_settled() {
                break capture;
            }
            if deadline.elapsed().as_micros() > u128::from(state.evidence.policy.max_window_us) {
                return Err(inconclusive("RTC initial service phase did not settle"));
            }
        };
        // All setup writes are explicit fixture inputs, before any instruction.
        self.bit_write(
            super::bit_address(plc::Memory::system_device(1017)).map_err(transport)?,
            false,
        )
        .map_err(transport)?;
        self.write_rtc_shadow(state.evidence.declared.wire_seed)
            .map_err(transport)?;
        let flags = state.evidence.declared.controls;
        for (bit, value) in [
            (1016, flags.m1016),
            (1082, flags.m1082),
            (1017, flags.m1017),
        ] {
            self.bit_write(
                super::bit_address(plc::Memory::system_device(bit)).map_err(transport)?,
                value,
            )
            .map_err(transport)?;
        }
        loop {
            let capture = self.rtc_capture(state)?;
            if deadline.elapsed().as_micros() > u128::from(state.evidence.policy.max_window_us) {
                return Err(inconclusive(
                    "RTC initial seed delivery exceeded setup deadline",
                ));
            }
            if capture.host_epoch_second_begin != before.host_epoch_second_end
                || capture.host_stamp() != before.host_stamp()
                || capture.clock_before[14] != before.clock_after[14]
                || capture.clock_after[14] != before.clock_after[14]
            {
                return Err(inconclusive(
                    "RTC input changed during one-time initial seed delivery",
                ));
            }
            if capture.service_phase_settled() {
                return state.admission.initialize(
                    &mut self.local.memory,
                    state.evidence.declared,
                    &monitor(&capture, state.evidence.budgets)?,
                    0,
                );
            }
            if deadline.elapsed().as_micros() > u128::from(state.evidence.policy.max_window_us) {
                return Err(inconclusive("RTC seeded service phase timed out"));
            }
        }
    }
    fn rtc_begin_node(
        &mut self,
        state: &mut RtcSessionState,
        pc: u16,
    ) -> Result<RtcBoundaryMonitor, AdmissionError> {
        self.rtc_wait_start_phase(state)?;
        let deadline = Instant::now();
        loop {
            let capture = self.rtc_capture(state)?;
            if deadline.elapsed().as_micros() > u128::from(state.evidence.policy.max_window_us) {
                return Err(inconclusive("RTC pre-node service phase timed out"));
            }
            check_independent_paused_pc(&capture, pc)?;
            check_controls(&self.local, &capture)?;
            if Self::rtc_await_input_phase(state, &capture, "before_node", pc)? {
                continue;
            }
            // Admission checks durable controls before its native-only phase
            // predicate; wrong M1016/M1017 values cannot become a phase retry.
            let m = monitor(&capture, state.evidence.budgets)?;
            match state
                .admission
                .begin_window(&mut self.local.memory, &m, pc)?
            {
                AdmissionStep::Ready => return Ok(m),
                AdmissionStep::Wait(_) => {}
            }
        }
    }
    fn rtc_check_window(
        state: &RtcSessionState,
        before: &RtcBoundaryMonitor,
        c: &RtcNativeObservation,
    ) -> Result<(), AdmissionError> {
        if c.monotonic_end_us
            .saturating_sub(before.time.monotonic_begin_us)
            > state.evidence.policy.max_window_us
            || c.host_epoch_second_begin as i128 != i128::from(before.time.host_second_begin)
            || c.host_epoch_second_end != c.host_epoch_second_begin
            || c.clock_before[..7] != before.host_stamp
            || c.host_stamp() != before.host_stamp
            || c.clock_before[14] != before.previous_second
            || c.clock_after[14] != before.previous_second
        {
            return Err(inconclusive(
                "RTC host sample changed or budget expired WITHIN node window; no retroactive replay",
            ));
        }
        Ok(())
    }
    fn rtc_finish_node(
        &mut self,
        state: &mut RtcSessionState,
        before: &RtcBoundaryMonitor,
        staged: &mut Simulator,
        next_pc: u16,
    ) -> Result<Comparison, AdmissionError> {
        let mut expected = staged.clone();
        crate::rtc_clock::settle_installed_service(&mut expected.memory, before.previous_second)
            .map_err(transport)?;
        loop {
            let first = self.rtc_capture(state)?;
            Self::rtc_check_window(state, before, &first)?;
            check_controls(&expected, &first)?;
            if !first.service_phase_settled() {
                continue;
            }
            let comparison = self.snapshot_for(&expected).map_err(transport)?;
            let last = self.rtc_capture(state)?;
            Self::rtc_check_window(state, before, &last)?;
            check_controls(&expected, &last)?;
            if !last.service_phase_settled() {
                continue;
            }
            if first.clock_after != last.clock_before || first.control_after != last.control_before
            {
                return Err(inconclusive("RTC changed across durable read batch"));
            }
            let mut after = monitor(&last, state.evidence.budgets)?;
            // COMPLETE durable batch is covered, not just the final RPM read.
            after.time.monotonic_begin_us = first.monotonic_begin_us;
            after.time.host_second_begin = i64::try_from(first.host_epoch_second_begin)
                .map_err(|_| inconclusive("Host epoch overflow"))?;
            let durable = comparison
                .rows
                .iter()
                .map(|r| DurableComparison {
                    field: format!("{}:{}", r.device, r.kind),
                    expected: u64::from(r.local),
                    observed: u64::from(r.original),
                })
                .collect::<Vec<_>>();
            let result =
                state
                    .admission
                    .finish_window(&mut staged.memory, &after, next_pc, &durable);
            state.evidence.windows.push(RtcWindowEvidence {
                before: before.clone(),
                after,
                independently_predicted_next_pc: next_pc,
                durable,
                error: result.as_ref().err().cloned(),
            });
            match result? {
                AdmissionStep::Ready => return Ok(comparison),
                AdmissionStep::Wait(_) => {}
            }
        }
    }
    fn rtc_finish_decoder_diagnostic(
        &mut self,
        state: &mut RtcSessionState,
        expectation: RtcDecoderExpectation,
        before: RtcDecoderObservation,
        next_pc: u16,
    ) -> Result<(), AdmissionError> {
        let after = self.observe_rtc_decoder(state.origin).map_err(transport)?;
        state.evidence.decoder_observations.push(after.clone());
        // No next IL instruction, memory write, or debugger stop is introduced.
        // The unchanged worker remains idle at the independently predicted PC.
        std::thread::sleep(Duration::from_millis(1));
        let idle = self.observe_rtc_decoder(state.origin).map_err(transport)?;
        state.evidence.decoder_observations.push(idle.clone());
        let expected = decoder_expected_bit(&expectation, &before)?;
        let result = validate_decoder_evidence(&expectation, &before, &after, &idle, next_pc);
        state.evidence.decoder_diagnostics.push(RtcDecoderEvidence {
            expectation,
            before,
            after,
            idle_repeat: idle,
            expected_error_bit: expected,
            error: result.as_ref().err().cloned(),
        });
        result
    }
    pub(super) fn step_rtc_inner(
        &mut self,
        inputs: &[(Device, bool)],
    ) -> Result<Comparison, String> {
        let mut state = self
            .rtc_comparison
            .take()
            .ok_or("RTC admission state missing")?;
        let result = self
            .step_rtc_qualified(&mut state, inputs)
            .map_err(|e| state.reject(e));
        state.sync();
        self.rtc_comparison = Some(state);
        result
    }
    fn step_rtc_qualified(
        &mut self,
        state: &mut RtcSessionState,
        inputs: &[(Device, bool)],
    ) -> Result<Comparison, AdmissionError> {
        if state.admission.is_terminal() {
            return Err(inconclusive("RTC admission is terminal"));
        }
        if inputs.len() > 256
            || inputs
                .iter()
                .any(|(d, _)| d.area != plc::Area::X || d.index > 255)
        {
            return Err(inconclusive("RTC step accepts only valid X inputs"));
        }
        for (d, value) in inputs {
            self.bit_write(super::bit_address(*d).map_err(transport)?, *value)
                .map_err(transport)?;
            self.local.memory.set_input(*d, *value);
        }
        let program = self
            .program
            .clone()
            .ok_or_else(|| inconclusive("No uploaded RTC program"))?;
        let encoded = self
            .encoded
            .clone()
            .ok_or_else(|| inconclusive("No encoded RTC program"))?;
        self.local
            .begin_plan_scan(&program, super::SCAN_MS, true)
            .map_err(transport)?;
        let mut last = None;
        while let Some(pc) = self.local.current_plan_pc() {
            let before = self.rtc_begin_node(state, pc)?;
            let index = *encoded
                .execution_plan
                .by_pc
                .get(&pc)
                .ok_or_else(|| inconclusive("Independent RTC PC outside plan"))?;
            self.rtc_decoder_expected = None;
            if let Some(check) = encoded.preflight.get(&index) {
                self.preflight(*check).map_err(transport)?;
            }
            let mut staged = self.local.clone();
            let prediction = staged.execute_plan_step(&program).map_err(transport)?;
            if !staged.memory.execution_limitations.is_empty() {
                return Err(inconclusive(format!(
                    "Independent RTC VM limitation: {:?}",
                    staged.memory.execution_limitations
                )));
            }
            let decoder_before = if let Some(expectation) = self.rtc_decoder_expected.clone() {
                let observation = self.observe_rtc_decoder(state.origin).map_err(transport)?;
                state
                    .evidence
                    .decoder_observations
                    .push(observation.clone());
                if observation.pc_before != prediction.pc || observation.pc_after != prediction.pc {
                    return Err(inconclusive(
                        "RTC decoder pre-observation PC moved before STEP",
                    ));
                }
                Some((expectation, observation))
            } else {
                None
            };
            self.bit_write(super::STEP, true).map_err(transport)?;
            let actual = self.word_read(super::PC).map_err(transport)?;
            if actual != prediction.next_pc {
                return Err(AdmissionError::ConformanceFailure {
                    field: format!("PC after {}", prediction.pc),
                    expected: prediction.next_pc.to_string(),
                    observed: actual.to_string(),
                });
            }
            if let Some((expectation, before_status)) = decoder_before {
                self.rtc_finish_decoder_diagnostic(
                    state,
                    expectation,
                    before_status,
                    prediction.next_pc,
                )?;
            }
            let snapshot = self.rtc_finish_node(state, &before, &mut staged, prediction.next_pc)?;
            self.local = staged;
            self.verified_steps += 1;
            last = Some(snapshot);
            if prediction.scan_boundary {
                break;
            }
        }
        if self.word_read(0x140f).map_err(transport)? != super::SCAN_MS as u16
            || !self
                .bit_read(plc::Memory::system_device(1039))
                .map_err(transport)?
        {
            return Err(inconclusive("RTC program changed fixed scan controls"));
        }
        last.ok_or_else(|| inconclusive("RTC scan contained no independently predicted node"))
    }
}

#[cfg(test)]
mod decoder_tests {
    use super::*;
    fn clock_observation(wall: u64, sample: u8) -> RtcNativeObservation {
        let clock = [26, 6, 10, 30, 0, 0, sample, 24, 4, 2, 29, 12, 30, 7, sample];
        RtcNativeObservation {
            owned_pid: 1,
            image_sha256: String::new(),
            monotonic_begin_us: 10,
            monotonic_end_us: 20,
            host_epoch_second_begin: wall,
            host_epoch_second_end: wall,
            clock_before: clock,
            clock_after: clock,
            public_year_first: [24, 4, 2, 29, 12, 30, 7],
            control_before: [0, 0],
            control_after: [0, 0],
            pc_before: 0,
            pc_after: 0,
        }
    }
    fn budgets() -> RtcComparisonBudgets {
        RtcComparisonBudgets {
            max_capture_us: 200_000,
            max_observation_us: 100_000,
            max_window_us: 250_000,
            max_between_us: 950_000,
            latest_start_phase_us: 600_000,
            continuous_host_clock_assumed: true,
        }
    }
    fn state() -> RtcSessionState {
        let b = budgets();
        let policy = SettledSameSecond {
            image: CurrentImage::CommgrEs2,
            max_capture_us: b.max_capture_us,
            max_window_us: b.max_window_us,
            max_between_us: b.max_between_us,
        };
        RtcSessionState {
            origin: Instant::now(),
            admission: RtcAdmission::new(policy).unwrap(),
            evidence: RtcComparisonEvidence {
                declared: RtcInitialInput {
                    wire_seed: [24, 4, 2, 29, 12, 30, 7],
                    controls: RtcControlFlags::default(),
                },
                policy,
                budgets: b,
                observations: vec![],
                input_phase_waits: vec![],
                input_ledger: vec![],
                windows: vec![],
                accepted_windows: 0,
                error: None,
                decoder_diagnostic_mode: false,
                decoder_diagnostics: vec![],
                decoder_observations: vec![],
                scope: "Synthetic unit inputs only",
            },
        }
    }
    #[test]
    fn rtc_input_phase_wait_is_only_coherent_one_second_old_environment_not_numeric_output() {
        let mut c = clock_observation(10, 9);
        assert_eq!(stable_stale_host_sample(&c), Ok(true));
        c.clock_before[7] = 99;
        c.clock_after[7] = 99;
        c.public_year_first[0] = 88;
        assert_eq!(stable_stale_host_sample(&c), Ok(true));
        assert_eq!(
            stable_stale_host_sample(&clock_observation(9, 9)),
            Ok(false)
        );
        assert!(stable_stale_host_sample(&clock_observation(11, 9)).is_err());
        let mut c = clock_observation(10, 9);
        c.clock_before[14] = 8;
        c.clock_after[14] = 8;
        assert!(stable_stale_host_sample(&c).is_err());
        let mut c = clock_observation(10, 9);
        c.clock_before[6] = 8;
        assert!(stable_stale_host_sample(&c).is_err());
    }
    #[test]
    fn rtc_input_phase_wait_is_never_an_excuse_for_a_change_inside_node_window() {
        let s = state();
        let before = monitor(&clock_observation(9, 9), budgets()).unwrap();
        let c = clock_observation(10, 9);
        assert_eq!(stable_stale_host_sample(&c), Ok(true));
        assert!(Session::rtc_check_window(&s, &before, &c).is_err());
    }
    #[test]
    fn rtc_input_phase_stale_sample_cannot_turn_wrong_paused_pc_into_a_wait() {
        let c = clock_observation(10, 9);
        assert_eq!(stable_stale_host_sample(&c), Ok(true));
        assert!(matches!(
            check_independent_paused_pc(&c, 3),
            Err(AdmissionError::ConformanceFailure { .. })
        ));
        assert_eq!(check_independent_paused_pc(&c, 0), Ok(()));
    }
    #[test]
    fn rtc_input_phase_raw_and_whole_durable_batch_budgets_are_distinct() {
        let mut s = state();
        let mut memory = plc::Memory::default();
        let mut first = monitor(&clock_observation(9, 9), budgets()).unwrap();
        first.time.monotonic_begin_us = 0;
        first.time.monotonic_end_us = 1000;
        s.admission
            .initialize(&mut memory, s.evidence.declared, &first, 0)
            .unwrap();
        let mut before = first.clone();
        before.time.monotonic_begin_us = 2000;
        before.time.monotonic_end_us = 3000;
        assert_eq!(
            s.admission.begin_window(&mut memory, &before, 0),
            Ok(AdmissionStep::Ready)
        );
        let mut after = before.clone();
        after.pc = 3;
        after.time.monotonic_begin_us = 20_000;
        after.time.monotonic_end_us = 154_888;
        assert!(
            after.time.monotonic_end_us - after.time.monotonic_begin_us
                > s.evidence.budgets.max_observation_us
        );
        assert_eq!(
            s.admission.finish_window(&mut memory, &after, 3, &[]),
            Ok(AdmissionStep::Ready)
        );
        assert_eq!(
            memory.rtc_clock.shadow(),
            Some(s.evidence.declared.wire_seed)
        );
    }
    fn observation(pc: u16, status: u8) -> RtcDecoderObservation {
        RtcDecoderObservation {
            owned_pid: 1,
            image_sha256: String::new(),
            monotonic_begin_us: 0,
            monotonic_end_us: 1,
            pc_before: pc,
            pc_after: pc,
            status_before: status,
            status_after: status,
        }
    }
    #[test]
    fn rtc_decoder_contract_distinguishes_fresh_error_clear_and_skipped_preservation() {
        use crate::rtc_clock::RtcOutcome;
        for (outcome, status) in [
            (RtcOutcome::Complete, 0),
            (RtcOutcome::PartialRead { written: 1 }, 0x10),
            (RtcOutcome::PartialRead { written: 0 }, 0x50),
            (RtcOutcome::PlcError { offset: 1 }, 0x10),
            (RtcOutcome::PlcError { offset: 3 }, 0x10),
        ] {
            let expected = RtcDecoderExpectation::Active {
                header_pc: 1,
                outcome,
            };
            assert!(
                validate_decoder_evidence(
                    &expected,
                    &observation(1, 0x10),
                    &observation(4, status),
                    &observation(4, status),
                    4
                )
                .is_ok()
            );
        }
        for incoming in [0, 0x10] {
            let expected = RtcDecoderExpectation::Skipped { header_pc: 1 };
            assert!(
                validate_decoder_evidence(
                    &expected,
                    &observation(1, incoming),
                    &observation(4, incoming),
                    &observation(4, incoming),
                    4
                )
                .is_ok()
            );
        }
    }
    #[test]
    fn rtc_decoder_persistence_and_natural32_classification_cannot_be_assumed() {
        let expected = RtcDecoderExpectation::Active {
            header_pc: 1,
            outcome: crate::rtc_clock::RtcOutcome::PartialRead { written: 0 },
        };
        for (after, idle) in [(0x10, 0x10), (0x50, 0), (0, 0)] {
            assert!(matches!(
                validate_decoder_evidence(
                    &expected,
                    &observation(1, 0),
                    &observation(4, after),
                    &observation(4, idle),
                    4
                ),
                Err(AdmissionError::Inconclusive(_))
            ));
        }
        let mut torn = observation(4, 0x50);
        torn.status_after = 0x40;
        assert!(
            validate_decoder_evidence(
                &expected,
                &observation(1, 0),
                &torn,
                &observation(4, 0x50),
                4
            )
            .is_err()
        );
        assert!(
            validate_decoder_evidence(
                &expected,
                &observation(1, 0),
                &observation(5, 0x50),
                &observation(5, 0x50),
                4
            )
            .is_err()
        );
    }
}
