//! Pure, exact-image RTC settled-boundary admission. No process I/O.
//!
//! This is a conditional comparison policy, not lossless native event capture.
//! Runtime qualification is separate.
use crate::plc::{Area, Device, Memory};
use crate::rtc_clock::{self, RtcProfile, RtcServiceSchedule};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum CurrentImage {
    CommgrEs2,
    CommgrSs2,
}

impl CurrentImage {
    pub fn from_sha256(digest: &str) -> Result<Self, AdmissionError> {
        match digest.to_ascii_lowercase().as_str() {
            "5570751341e73b46846f36d99ab476c0f40b9636a8d784d9a3781377e9eb0ea7" => {
                Ok(Self::CommgrEs2)
            }
            "915c96a9ad252cb64773b539290e66af9dcbb4a04af3c860b6d654f7aaad3525" => {
                Ok(Self::CommgrSs2)
            }
            _ => Err(inconclusive("unqualified native image digest")),
        }
    }
}

/// All budgets are caller-declared policy values, NOT measured guarantees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct SettledSameSecond {
    pub image: CurrentImage,
    pub max_capture_us: u64,
    pub max_window_us: u64,
    pub max_between_us: u64,
}

impl SettledSameSecond {
    pub fn validate(self) -> Result<Self, AdmissionError> {
        // One second is a dimensional policy ceiling, not a proven read budget.
        // With the explicit continuity assumption a shorter gap has at most one
        // second boundary. Equality/repeated second bytes alone prove nothing.
        if self.max_capture_us == 0
            || self.max_capture_us > self.max_window_us
            || self.max_window_us >= 1_000_000
            || self.max_between_us == 0
            || self.max_between_us >= 1_000_000
        {
            return Err(inconclusive("invalid explicitly declared timing budgets"));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct RtcControlFlags {
    pub m1016: bool,
    pub m1017: bool,
    pub m1082: bool,
}

impl RtcControlFlags {
    fn read(m: &Memory) -> Self {
        Self {
            m1016: m.system_bit(1016),
            m1017: m.system_bit(1017),
            m1082: m.system_bit(1082),
        }
    }
    fn write(self, m: &mut Memory) {
        m.set_system(1016, self.m1016);
        m.set_system(1017, self.m1017);
        m.set_system(1082, self.m1082);
    }
}

/// Fixture-authored, one-time external input. Never derive this from TRD,
/// public RTC words, or native private output after the program has run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct RtcInitialInput {
    /// Native wire order: year, weekday, month, day, hour, minute, second.
    /// Raw BYTE seeds are deliberate; no invented Gregorian validation here.
    pub wire_seed: [u8; 7],
    pub controls: RtcControlFlags,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct TimeEnvelope {
    pub monotonic_begin_us: u64,
    pub monotonic_end_us: u64,
    /// Caller-observed absolute host seconds bracketing the complete capture.
    /// Both use the same epoch; second-of-minute is rem_euclid(60).
    pub host_second_begin: i64,
    pub host_second_end: i64,
    /// Changes whenever the caller knows its continuity assumption was broken.
    pub continuity_epoch: u64,
    /// Explicit external assumption: no undetected host clock adjustment,
    /// suspension, or jump-and-return during this bounded interval.
    pub continuous_host_clock_assumed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RtcBoundaryMonitor {
    pub host_stamp: [u8; 7],
    pub shadow: [u8; 7],
    pub previous_second: u8,
    /// YEAR FIRST: D1319, D1318, ..., D1313, not ascending register order.
    pub public: [u16; 7],
    pub controls: RtcControlFlags,
    pub pc: u16,
    pub time: TimeEnvelope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum PhaseWait {
    HostSampleNotServiced,
    PublicNotPublished,
    CorrectionNotSettled,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum AdmissionError {
    /// Input/phase/race/qualification failed. No numerical parity conclusion.
    Inconclusive(String),
    /// A qualified, settled observation disagrees with an independent result.
    ConformanceFailure {
        field: String,
        expected: String,
        observed: String,
    },
}

fn inconclusive(reason: &str) -> AdmissionError {
    AdmissionError::Inconclusive(reason.into())
}
fn mismatch(
    field: &str,
    expected: impl std::fmt::Debug,
    observed: impl std::fmt::Debug,
) -> AdmissionError {
    AdmissionError::ConformanceFailure {
        field: field.into(),
        expected: format!("{expected:?}"),
        observed: format!("{observed:?}"),
    }
}

fn data(index: u16) -> Device {
    Device {
        area: Area::D,
        index,
    }
}
fn display(shadow: [u8; 7], four_digit_year: bool) -> [u16; 7] {
    let mut words = shadow.map(u16::from);
    if four_digit_year {
        words[0] += 2000;
    }
    words
}

/// Native-only phase classification. It cannot create or update local state.
/// A caller may wait ONLY for this phase, never for arbitrary output equality.
pub fn phase_status(monitor: &RtcBoundaryMonitor) -> Result<(), PhaseWait> {
    if monitor.previous_second != monitor.host_stamp[6] {
        return Err(PhaseWait::HostSampleNotServiced);
    }
    if monitor.public != display(monitor.shadow, monitor.controls.m1016) {
        return Err(PhaseWait::PublicNotPublished);
    }
    if monitor.controls.m1017 && (monitor.shadow[6] != 0 || !monitor.controls.m1082) {
        return Err(PhaseWait::CorrectionNotSettled);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum RtcInputEvent {
    Initial {
        declared: RtcInitialInput,
        environmental_host_stamp: [u8; 7],
        environmental_previous_second: u8,
        pc: u16,
        time: TimeEnvelope,
    },
    /// Exactly one accepted environmental second transition BETWEEN windows.
    AdjacentSecond {
        from: u8,
        to: u8,
        host_stamp: [u8; 7],
        pc: u16,
        time: TimeEnvelope,
    },
}

/// Non-RTC durable observations must also be captured inside the post envelope.
/// expected comes from the independent local execution, never native results.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DurableComparison {
    pub field: String,
    pub expected: u64,
    pub observed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum AdmissionStep {
    Ready,
    Wait(PhaseWait),
}

#[derive(Debug, Clone)]
pub struct RtcAdmission {
    policy: SettledSameSecond,
    initialization_attempted: bool,
    terminal: bool,
    last_boundary: Option<RtcBoundaryMonitor>,
    active: Option<RtcBoundaryMonitor>,
    inputs: Vec<RtcInputEvent>,
    accepted_windows: u64,
}

impl RtcAdmission {
    pub fn new(policy: SettledSameSecond) -> Result<Self, AdmissionError> {
        Ok(Self {
            policy: policy.validate()?,
            initialization_attempted: false,
            terminal: false,
            last_boundary: None,
            active: None,
            inputs: Vec::new(),
            accepted_windows: 0,
        })
    }
    pub fn input_ledger(&self) -> &[RtcInputEvent] {
        &self.inputs
    }
    pub fn accepted_windows(&self) -> u64 {
        self.accepted_windows
    }
    pub fn is_terminal(&self) -> bool {
        self.terminal
    }
    pub fn reject_external(&mut self, error: AdmissionError) -> AdmissionError {
        self.terminal = true;
        error
    }
    fn live(&self) -> Result<(), AdmissionError> {
        if self.terminal {
            Err(inconclusive(
                "admission is terminal; no result-based retry or resynchronization",
            ))
        } else {
            Ok(())
        }
    }
    fn conclude<T>(&mut self, result: Result<T, AdmissionError>) -> Result<T, AdmissionError> {
        if result.is_err() {
            self.terminal = true;
        }
        result
    }

    /// Call once after the owned native process has received the declared seed
    /// and controls, BEFORE IL execution. A failed attempt cannot be reseeded.
    /// Use phase_status externally for bounded INITIAL phase-only waiting.
    pub fn initialize(
        &mut self,
        local: &mut Memory,
        declared: RtcInitialInput,
        monitor: &RtcBoundaryMonitor,
        independently_predicted_pc: u16,
    ) -> Result<(), AdmissionError> {
        let result = self.initialize_inner(local, declared, monitor, independently_predicted_pc);
        self.conclude(result)
    }

    fn initialize_inner(
        &mut self,
        local: &mut Memory,
        declared: RtcInitialInput,
        monitor: &RtcBoundaryMonitor,
        expected_pc: u16,
    ) -> Result<(), AdmissionError> {
        self.live()?;
        if self.initialization_attempted {
            return Err(inconclusive("one-time initial seed already attempted"));
        }
        self.initialization_attempted = true;
        validate_capture(self.policy, monitor)?;
        if monitor.pc != expected_pc || phase_status(monitor).is_err() {
            return Err(inconclusive("initial PC/service phase is not admitted"));
        }
        let mut staged = local.clone();
        // ONLY fixture-declared bytes seed local arithmetic. Monitor.shadow is
        // never an argument of configure/replace_installed_shadow anywhere.
        staged
            .rtc_clock
            .configure_installed_shadow(declared.wire_seed, monitor.previous_second)
            .map_err(AdmissionError::Inconclusive)?;
        staged.rtc_service_schedule = RtcServiceSchedule::ExplicitBoundaries;
        declared.controls.write(&mut staged);
        settle(&mut staged, monitor.previous_second)?;
        compare_rtc(&staged, monitor)
            .map_err(|e| inconclusive(&format!("initial declared seed/setup mismatch: {e:?}")))?;
        *local = staged;
        self.inputs.push(RtcInputEvent::Initial {
            declared,
            environmental_host_stamp: monitor.host_stamp,
            environmental_previous_second: monitor.previous_second,
            pc: expected_pc,
            time: monitor.time,
        });
        self.last_boundary = Some(monitor.clone());
        Ok(())
    }

    /// Before predicting/executing the next local node. Admits zero events, or
    /// ONE adjacent event in the gap. Never acts on a mid-instruction change.
    /// Ready commits only independently computed service and pins this window.
    pub fn begin_window(
        &mut self,
        local: &mut Memory,
        monitor: &RtcBoundaryMonitor,
        independently_predicted_pc: u16,
    ) -> Result<AdmissionStep, AdmissionError> {
        let result = self.begin_inner(local, monitor, independently_predicted_pc);
        self.conclude(result)
    }

    fn begin_inner(
        &mut self,
        local: &mut Memory,
        monitor: &RtcBoundaryMonitor,
        expected_pc: u16,
    ) -> Result<AdmissionStep, AdmissionError> {
        self.live()?;
        if self.active.is_some() {
            return Err(inconclusive("a node window is already active"));
        }
        let before = self
            .last_boundary
            .as_ref()
            .ok_or_else(|| inconclusive("initial seed not admitted"))?;
        validate_capture(self.policy, monitor)?;
        let adjacent = validate_between(self.policy, before, monitor)?;
        if monitor.pc != expected_pc || before.pc != expected_pc {
            return Err(mismatch("paused PC", expected_pc, monitor.pc));
        }
        validate_local(local, before.previous_second)?;
        compare_stable_controls(local, monitor)?;
        if let Err(reason) = phase_status(monitor) {
            return Ok(AdmissionStep::Wait(reason));
        }
        let mut staged = local.clone();
        settle(&mut staged, monitor.previous_second)?;
        compare_rtc(&staged, monitor)?;
        if adjacent {
            self.inputs.push(RtcInputEvent::AdjacentSecond {
                from: before.previous_second,
                to: monitor.previous_second,
                host_stamp: monitor.host_stamp,
                pc: expected_pc,
                time: monitor.time,
            });
        }
        *local = staged;
        self.active = Some(monitor.clone());
        Ok(AdmissionStep::Ready)
    }

    /// After executing ONE node in a caller-owned Simulator clone and stepping
    /// native once. expected_next_pc is independently predicted, never observed.
    /// Wait leaves the staged Memory unchanged. On Ready the caller may commit
    /// its whole Simulator clone (including cursor/history), not just Memory.
    pub fn finish_window(
        &mut self,
        staged_local: &mut Memory,
        monitor: &RtcBoundaryMonitor,
        independently_predicted_next_pc: u16,
        durable: &[DurableComparison],
    ) -> Result<AdmissionStep, AdmissionError> {
        let result = self.finish_inner(
            staged_local,
            monitor,
            independently_predicted_next_pc,
            durable,
        );
        self.conclude(result)
    }

    fn finish_inner(
        &mut self,
        staged_local: &mut Memory,
        monitor: &RtcBoundaryMonitor,
        expected_pc: u16,
        durable: &[DurableComparison],
    ) -> Result<AdmissionStep, AdmissionError> {
        self.live()?;
        let before = self
            .active
            .as_ref()
            .ok_or_else(|| inconclusive("no active node window"))?;
        validate_capture(self.policy, monitor)?;
        validate_within(self.policy, before, monitor)?;
        if monitor.pc != expected_pc {
            return Err(mismatch("next PC", expected_pc, monitor.pc));
        }
        validate_local(staged_local, before.previous_second)?;
        compare_stable_controls(staged_local, monitor)?;
        if let Err(reason) = phase_status(monitor) {
            return Ok(AdmissionStep::Wait(reason));
        }
        let mut candidate = staged_local.clone();
        // Uses the PRE-window admitted sample. There is no path for replaying a
        // newly observed post-window input around an already executed opcode.
        settle(&mut candidate, before.previous_second)?;
        compare_rtc(&candidate, monitor)?;
        for check in durable {
            if check.expected != check.observed {
                return Err(mismatch(&check.field, check.expected, check.observed));
            }
        }
        *staged_local = candidate;
        self.active = None;
        self.last_boundary = Some(monitor.clone());
        self.accepted_windows += 1;
        Ok(AdmissionStep::Ready)
    }
}

fn validate_capture(
    policy: SettledSameSecond,
    m: &RtcBoundaryMonitor,
) -> Result<(), AdmissionError> {
    let t = m.time;
    let duration = t
        .monotonic_end_us
        .checked_sub(t.monotonic_begin_us)
        .ok_or_else(|| inconclusive("capture monotonic interval reversed"))?;
    if !t.continuous_host_clock_assumed
        || duration > policy.max_capture_us
        || t.host_second_begin != t.host_second_end
        || t.host_second_begin.rem_euclid(60) as u8 != m.host_stamp[6]
        || m.previous_second > 59
    {
        return Err(inconclusive(
            "capture budget, clock continuity, or host sample inconsistent",
        ));
    }
    let [year, weekday, month, day, hour, minute, second] = m.host_stamp;
    if year > 99
        || !(1..=7).contains(&weekday)
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return Err(inconclusive(
            "host sample outside qualified calendar field ranges",
        ));
    }
    Ok(())
}

fn validate_ordered_interval(
    before: &RtcBoundaryMonitor,
    after: &RtcBoundaryMonitor,
    budget: u64,
) -> Result<(), AdmissionError> {
    if after.time.monotonic_begin_us < before.time.monotonic_end_us
        || after.time.continuity_epoch != before.time.continuity_epoch
        || after
            .time
            .monotonic_end_us
            .checked_sub(before.time.monotonic_begin_us)
            .is_none_or(|duration| duration > budget)
    {
        return Err(inconclusive(
            "ordered interval budget or continuity epoch failed",
        ));
    }
    Ok(())
}

fn validate_between(
    policy: SettledSameSecond,
    before: &RtcBoundaryMonitor,
    after: &RtcBoundaryMonitor,
) -> Result<bool, AdmissionError> {
    validate_ordered_interval(before, after, policy.max_between_us)?;
    let delta = after
        .time
        .host_second_begin
        .checked_sub(before.time.host_second_end)
        .ok_or_else(|| inconclusive("host epoch difference overflow"))?;
    match delta {
        0 if after.host_stamp == before.host_stamp => Ok(false),
        1 if after.host_stamp[6] == (before.host_stamp[6] + 1) % 60 => Ok(true),
        _ => Err(inconclusive(
            "between boundaries requires zero or exactly one adjacent second",
        )),
    }
}

fn validate_within(
    policy: SettledSameSecond,
    before: &RtcBoundaryMonitor,
    after: &RtcBoundaryMonitor,
) -> Result<(), AdmissionError> {
    validate_ordered_interval(before, after, policy.max_window_us)?;
    if after.time.host_second_begin != before.time.host_second_end
        || after.host_stamp != before.host_stamp
        || after.previous_second != before.previous_second
    {
        return Err(inconclusive(
            "host/sample changed WITHIN node window; no retroactive replay",
        ));
    }
    Ok(())
}

fn validate_local(m: &Memory, previous: u8) -> Result<(), AdmissionError> {
    if m.rtc_service_schedule != RtcServiceSchedule::ExplicitBoundaries
        || m.rtc_clock.profile() != Some(RtcProfile::InstalledShadow)
        || m.rtc_clock.shadow().is_none()
        || m.rtc_clock.previous_native_second() != Some(previous)
    {
        return Err(inconclusive(
            "local RTC profile/schedule/admitted sample changed",
        ));
    }
    Ok(())
}

fn settle(m: &mut Memory, sample: u8) -> Result<(), AdmissionError> {
    if m.rtc_service_schedule != RtcServiceSchedule::ExplicitBoundaries
        || m.rtc_clock.profile() != Some(RtcProfile::InstalledShadow)
        || m.rtc_clock.shadow().is_none()
        || m.rtc_clock.previous_native_second().is_none_or(|s| s > 59)
        || sample > 59
    {
        return Err(inconclusive("cannot settle an uninitialized installed RTC"));
    }
    // Independent fixed-point arithmetic; never monitor-derived shadow/public.
    // Events are intentionally not forwarded to HOUR in this TRD/TWR policy.
    rtc_clock::settle_installed_service(m, sample).map_err(AdmissionError::Inconclusive)?;
    Ok(())
}

fn compare_stable_controls(
    m: &Memory,
    observed: &RtcBoundaryMonitor,
) -> Result<(), AdmissionError> {
    let expected = RtcControlFlags::read(m);
    // Background service can set M1082, but cannot alter these two controls.
    if (expected.m1016, expected.m1017) != (observed.controls.m1016, observed.controls.m1017) {
        return Err(mismatch(
            "M1016/M1017",
            (expected.m1016, expected.m1017),
            (observed.controls.m1016, observed.controls.m1017),
        ));
    }
    Ok(())
}

fn compare_rtc(m: &Memory, observed: &RtcBoundaryMonitor) -> Result<(), AdmissionError> {
    if m.rtc_clock.shadow() != Some(observed.shadow) {
        return Err(mismatch(
            "private RTC shadow",
            m.rtc_clock.shadow(),
            observed.shadow,
        ));
    }
    if m.rtc_clock.previous_native_second() != Some(observed.previous_second) {
        return Err(mismatch(
            "previous host second",
            m.rtc_clock.previous_native_second(),
            observed.previous_second,
        ));
    }
    let expected: [u16; 7] = std::array::from_fn(|i| m.word(data(1319 - i as u16)) as u16);
    if expected != observed.public {
        return Err(mismatch("public RTC words", expected, observed.public));
    }
    let flags = RtcControlFlags::read(m);
    if flags != observed.controls {
        return Err(mismatch("RTC control/ack flags", flags, observed.controls));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: [u8; 7] = [26, 5, 10, 30, 12, 34, 20];
    fn policy() -> SettledSameSecond {
        // Test fixture choices only, not recommended/proven operational budgets.
        SettledSameSecond {
            image: CurrentImage::CommgrEs2,
            max_capture_us: 20,
            max_window_us: 500,
            max_between_us: 500,
        }
    }
    fn monitor(shadow: [u8; 7], sample: u8, pc: u16, start: u64) -> RtcBoundaryMonitor {
        RtcBoundaryMonitor {
            host_stamp: [26, 5, 10, 30, 12, 0, sample],
            shadow,
            previous_second: sample,
            public: shadow.map(u16::from),
            controls: RtcControlFlags::default(),
            pc,
            time: TimeEnvelope {
                monotonic_begin_us: start,
                monotonic_end_us: start + 10,
                host_second_begin: i64::from(sample),
                host_second_end: i64::from(sample),
                continuity_epoch: 1,
                continuous_host_clock_assumed: true,
            },
        }
    }
    fn setup() -> (RtcAdmission, Memory) {
        let mut a = RtcAdmission::new(policy()).unwrap();
        let mut m = Memory::default();
        a.initialize(
            &mut m,
            RtcInitialInput {
                wire_seed: SEED,
                controls: RtcControlFlags::default(),
            },
            &monitor(SEED, 10, 0, 0),
            0,
        )
        .unwrap();
        (a, m)
    }
    fn begin(a: &mut RtcAdmission, m: &mut Memory) {
        assert_eq!(
            a.begin_window(m, &monitor(SEED, 10, 0, 20), 0),
            Ok(AdmissionStep::Ready)
        );
    }

    #[test]
    fn rtc_admission_exact_images_and_explicit_budget_only() {
        assert_eq!(
            CurrentImage::from_sha256(
                "5570751341e73b46846f36d99ab476c0f40b9636a8d784d9a3781377e9eb0ea7"
            ),
            Ok(CurrentImage::CommgrEs2)
        );
        assert_eq!(
            CurrentImage::from_sha256(
                "915c96a9ad252cb64773b539290e66af9dcbb4a04af3c860b6d654f7aaad3525"
            ),
            Ok(CurrentImage::CommgrSs2)
        );
        assert!(CurrentImage::from_sha256("old-WPL-or-unknown").is_err());
        let mut p = policy();
        p.max_between_us = 1_000_000;
        assert!(RtcAdmission::new(p).is_err());
    }

    #[test]
    fn rtc_admission_seed_is_one_time_and_never_native_shadow() {
        let mut a = RtcAdmission::new(policy()).unwrap();
        let mut m = Memory::default();
        let mut wrong = SEED;
        wrong[5] = 59;
        let initial = RtcInitialInput {
            wire_seed: SEED,
            controls: RtcControlFlags::default(),
        };
        assert!(matches!(
            a.initialize(&mut m, initial, &monitor(wrong, 10, 0, 0), 0),
            Err(AdmissionError::Inconclusive(_))
        ));
        assert_eq!(m.rtc_clock.shadow(), None);
        assert!(
            a.initialize(&mut m, initial, &monitor(SEED, 10, 0, 20), 0)
                .is_err()
        );
        assert!(a.input_ledger().is_empty());
        let (mut a, mut m) = setup();
        assert!(
            a.initialize(&mut m, initial, &monitor(SEED, 10, 0, 20), 0)
                .is_err()
        );
        assert_eq!(m.rtc_clock.shadow(), Some(SEED));
    }

    #[test]
    fn rtc_admission_fixed_point_includes_correction_ack_and_public_refresh() {
        let mut a = RtcAdmission::new(policy()).unwrap();
        let mut m = Memory::default();
        let mut raw = SEED;
        raw[6] = 45;
        let mut corrected = raw;
        corrected[5] = 35;
        corrected[6] = 0;
        let controls = RtcControlFlags {
            m1016: true,
            m1017: true,
            m1082: false,
        };
        let mut seen = monitor(corrected, 10, 0, 0);
        seen.controls = RtcControlFlags {
            m1082: true,
            ..controls
        };
        seen.public[0] = 2026;
        a.initialize(
            &mut m,
            RtcInitialInput {
                wire_seed: raw,
                controls,
            },
            &seen,
            0,
        )
        .unwrap();
        assert_eq!(m.rtc_clock.shadow(), Some(corrected));
        assert_eq!(m.word(data(1319)), 2026);
        assert!(m.system_bit(1082));
        seen.controls.m1082 = false;
        assert_eq!(phase_status(&seen), Err(PhaseWait::CorrectionNotSettled));
    }

    #[test]
    fn rtc_admission_same_second_node_commits_independent_twr_then_settles() {
        let (mut a, mut m) = setup();
        begin(&mut a, &mut m);
        let written = [27, 2, 4, 7, 3, 2, 1];
        for (i, value) in written.into_iter().enumerate() {
            m.set_word(data(100 + i as u16), value as i16);
        }
        assert_eq!(
            rtc_clock::RtcAction::Write { source: data(100) }.execute(&mut m),
            rtc_clock::RtcOutcome::Complete
        );
        let mut seen = monitor(written, 10, 3, 40);
        seen.controls.m1082 = true;
        assert_eq!(
            a.finish_window(&mut m, &seen, 3, &[]),
            Ok(AdmissionStep::Ready)
        );
        assert_eq!(m.rtc_clock.shadow(), Some(written));
        assert_eq!(a.accepted_windows(), 1);
        assert_eq!(a.input_ledger().len(), 1);
    }

    #[test]
    fn rtc_admission_within_window_change_rejects_before_service_or_ledger_event() {
        let (mut a, mut m) = setup();
        begin(&mut a, &mut m);
        m.set_word(data(1313), 777);
        let result = a.finish_window(&mut m, &monitor(SEED, 11, 3, 40), 3, &[]);
        assert!(matches!(result, Err(AdmissionError::Inconclusive(_))));
        assert_eq!(m.rtc_clock.shadow(), Some(SEED));
        assert_eq!(m.word(data(1313)), 777);
        assert_eq!(a.input_ledger().len(), 1);
        assert!(a.is_terminal());
    }

    #[test]
    fn rtc_admission_wait_is_phase_only_and_does_not_mutate_staged_memory() {
        let (mut a, mut m) = setup();
        begin(&mut a, &mut m);
        m.set_word(data(1313), 777);
        let mut seen = monitor(SEED, 10, 3, 40);
        seen.public[6] = 999;
        assert_eq!(
            a.finish_window(&mut m, &seen, 3, &[]),
            Ok(AdmissionStep::Wait(PhaseWait::PublicNotPublished))
        );
        assert_eq!(m.word(data(1313)), 777);
        seen.public[6] = 20;
        seen.time.monotonic_begin_us = 60;
        seen.time.monotonic_end_us = 70;
        assert_eq!(
            a.finish_window(&mut m, &seen, 3, &[]),
            Ok(AdmissionStep::Ready)
        );
        assert_eq!(m.word(data(1313)), 20);
    }

    #[test]
    fn rtc_admission_coherent_but_wrong_shadow_is_durable_failure_not_retry() {
        let (mut a, mut m) = setup();
        begin(&mut a, &mut m);
        let mut wrong = SEED;
        wrong[5] = 40;
        assert!(matches!(
            a.finish_window(&mut m, &monitor(wrong, 10, 3, 40), 3, &[]),
            Err(AdmissionError::ConformanceFailure { .. })
        ));
        assert_eq!(m.rtc_clock.shadow(), Some(SEED));
        assert!(
            a.finish_window(&mut m, &monitor(SEED, 10, 3, 60), 3, &[])
                .is_err()
        );
    }

    #[test]
    fn rtc_admission_other_durable_output_failure_cannot_be_polled_until_pass() {
        let (mut a, mut m) = setup();
        begin(&mut a, &mut m);
        let check = DurableComparison {
            field: "TRD D100".into(),
            expected: 26,
            observed: 27,
        };
        assert!(matches!(
            a.finish_window(&mut m, &monitor(SEED, 10, 3, 40), 3, &[check]),
            Err(AdmissionError::ConformanceFailure { .. })
        ));
        assert_eq!(a.accepted_windows(), 0);
        assert!(a.is_terminal());
    }

    #[test]
    fn rtc_admission_single_adjacent_event_only_between_windows() {
        let (mut a, mut m) = setup();
        begin(&mut a, &mut m);
        a.finish_window(&mut m, &monitor(SEED, 10, 3, 40), 3, &[])
            .unwrap();
        let mut advanced = SEED;
        advanced[6] = 21;
        assert_eq!(
            a.begin_window(&mut m, &monitor(advanced, 11, 3, 60), 3),
            Ok(AdmissionStep::Ready)
        );
        assert_eq!(m.rtc_clock.shadow(), Some(advanced));
        assert!(matches!(
            a.input_ledger()[1],
            RtcInputEvent::AdjacentSecond {
                from: 10,
                to: 11,
                ..
            }
        ));
        a.finish_window(&mut m, &monitor(advanced, 11, 6, 80), 6, &[])
            .unwrap();
        let mut jump = advanced;
        jump[6] = 23;
        assert!(matches!(
            a.begin_window(&mut m, &monitor(jump, 13, 6, 100), 6),
            Err(AdmissionError::Inconclusive(_))
        ));
        assert_eq!(m.rtc_clock.shadow(), Some(advanced));
        assert_eq!(a.input_ledger().len(), 2);
    }

    #[test]
    fn rtc_admission_adjacent_second_wrap_keeps_installed_no_date_carry() {
        let mut a = RtcAdmission::new(policy()).unwrap();
        let mut m = Memory::default();
        let seed = [26, 5, 10, 30, 23, 59, 59];
        a.initialize(
            &mut m,
            RtcInitialInput {
                wire_seed: seed,
                controls: RtcControlFlags::default(),
            },
            &monitor(seed, 59, 0, 0),
            0,
        )
        .unwrap();
        let expected = [26, 5, 10, 30, 0, 0, 0];
        let mut seen = monitor(expected, 0, 0, 20);
        seen.host_stamp[5] = 1;
        seen.time.host_second_begin = 60;
        seen.time.host_second_end = 60;
        assert_eq!(a.begin_window(&mut m, &seen, 0), Ok(AdmissionStep::Ready));
        assert_eq!(m.rtc_clock.shadow(), Some(expected));
    }

    #[test]
    fn rtc_admission_timeout_and_continuity_break_are_inconclusive() {
        for kind in 0..4 {
            let (mut a, mut m) = setup();
            begin(&mut a, &mut m);
            let mut seen = monitor(SEED, 10, 3, 40);
            match kind {
                0 => {
                    seen.time.monotonic_begin_us = 600;
                    seen.time.monotonic_end_us = 610;
                }
                1 => seen.time.continuity_epoch = 2,
                2 => seen.time.continuous_host_clock_assumed = false,
                _ => seen.time.monotonic_end_us = 39,
            }
            assert!(matches!(
                a.finish_window(&mut m, &seen, 3, &[]),
                Err(AdmissionError::Inconclusive(_))
            ));
            assert_eq!(m.rtc_clock.shadow(), Some(SEED));
        }
    }

    #[test]
    fn rtc_admission_full_stamp_and_wall_epoch_not_only_second_byte() {
        for kind in 0..2 {
            let (mut a, mut m) = setup();
            begin(&mut a, &mut m);
            let mut seen = monitor(SEED, 10, 3, 40);
            if kind == 0 {
                seen.host_stamp[5] = 1;
            } else {
                seen.time.host_second_begin = 70;
                seen.time.host_second_end = 70;
            }
            assert!(matches!(
                a.finish_window(&mut m, &seen, 3, &[]),
                Err(AdmissionError::Inconclusive(_))
            ));
        }
    }

    #[test]
    fn rtc_admission_native_pc_cannot_redirect_local_prediction() {
        let (mut a, mut m) = setup();
        begin(&mut a, &mut m);
        assert!(matches!(
            a.finish_window(&mut m, &monitor(SEED, 10, 99, 40), 3, &[]),
            Err(AdmissionError::ConformanceFailure { .. })
        ));
        assert_eq!(m.rtc_clock.shadow(), Some(SEED));
        assert_eq!(a.accepted_windows(), 0);
    }

    #[test]
    fn rtc_admission_schedule_or_sample_replacement_rejects() {
        for kind in 0..2 {
            let (mut a, mut m) = setup();
            begin(&mut a, &mut m);
            if kind == 0 {
                m.rtc_service_schedule = RtcServiceSchedule::EndOfScan;
            } else {
                m.rtc_clock.sample_installed_second(11).unwrap();
            }
            assert!(matches!(
                a.finish_window(&mut m, &monitor(SEED, 10, 3, 40), 3, &[]),
                Err(AdmissionError::Inconclusive(_))
            ));
        }
    }
}
