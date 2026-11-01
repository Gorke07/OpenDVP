//! Explicit external-supply classifications for the documented M1019 control.
//!
//! PDF physical pages 56 and 137/138 describe a nonlatched, power-on-cleared
//! control, an error LED, and recovery after MORE than two normal seconds.
//! They do not define an instability detector, hysteresis, LED blink period,
//! or the recovery timer's behavior when M1019 changes during normal supply.
//! This local model therefore takes classifications and elapsed time as inputs
//! and names that recovery choice. It does not generate M1009 or inhibit RUN.
//! InstalledStorageOnly describes the bounded installed-image storage proof;
//! its LED is unavailable, rather than silently using the virtual model.
//! The documented feature requires ES2/EX2 >=3.60 or SS2 >=3.50; this virtual
//! facility does not derive a CPU firmware version from expansion-bus profiles.

use crate::plc::Memory;
use std::collections::BTreeSet;

const RECOVERY_US: u128 = 2_000_000;
const VIRTUAL_SCOPE: &str = "M1019 virtual power monitor: explicit external supply classifications and elapsed time; no physical voltage detector, hysteresis, blink-frequency or CPU/RUN-inhibition emulation. Normal with no known fault is a local LED-Off convention.";
const STORAGE_SCOPE: &str = "M1019 InstalledStorageOnly: installed-image control-bit storage is supported; supply detection and the error LED are unavailable and are not replaced by virtual voltage emulation.";
const UNKNOWN_SCOPE: &str = "M1019: no explicit supply classification is available; Unknown breaks normal continuity and cannot clear a known flashing indication.";
const UNSTABLE_OFF_SCOPE: &str = "M1019 OFF with Unstable supply is unspecified by the PDF: this local model reports Unknown unless a previously known fault keeps the LED Flashing.";
const RESTART_POLICY: &str = "M1019 local recovery policy RestartAfterControlOff: an observed ON-to-OFF transition during Normal restarts the >2 s recovery interval; this transition timing is not specified by the PDF.";
const REUSE_POLICY: &str = "M1019 local recovery policy ReuseContinuousNormal: an observed ON-to-OFF transition reuses continuous Normal time; this transition timing is not specified by the PDF.";

/// Classifications supplied by a caller, never inferred from host time or PLC
/// storage. Unstable does not imply the separate below-17.8-V classification.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SupplyInput {
    #[default]
    Unknown,
    Normal,
    Below17800Mv,
    Unstable,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PowerBackend {
    #[default]
    DocumentedVirtual,
    InstalledStorageOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorLed {
    Unavailable,
    Unknown,
    Off,
    Flashing,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RecoveryPolicy {
    /// Named local choice: only normal time after an observed control-OFF edge
    /// can clear a previously latched indication.
    #[default]
    RestartAfterControlOff,
    /// Named alternative: normal time accumulated while ON counts after OFF.
    ReuseContinuousNormal,
}

/// Read-only display snapshot. M1019 itself is deliberately absent: Memory is
/// its sole storage owner, so this cannot publish a stale duplicate control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerStatus {
    pub input: SupplyInput,
    pub backend: PowerBackend,
    pub recovery_policy: RecoveryPolicy,
    pub led: ErrorLed,
    pub normal_for_us: u128,
    pub power_elapsed_us: u128,
}

#[derive(Debug, Clone, Default)]
pub struct PowerState {
    input: SupplyInput,
    backend: PowerBackend,
    recovery_policy: RecoveryPolicy,
    fault_latched: bool,
    normal_for_us: u128,
    power_elapsed_us: u128,
    // Transition history only, not a second writable/control-bit image.
    previous_control: Option<bool>,
    used: bool,
    /// Informational capability and local-policy qualifications, not PLC errors.
    pub notices: BTreeSet<String>,
}

impl PowerState {
    pub fn input(&self) -> SupplyInput {
        self.input
    }

    pub fn backend(&self) -> PowerBackend {
        self.backend
    }

    pub fn recovery_policy(&self) -> RecoveryPolicy {
        self.recovery_policy
    }

    pub fn led(&self) -> ErrorLed {
        if self.backend == PowerBackend::InstalledStorageOnly {
            ErrorLed::Unavailable
        } else if self.fault_latched {
            ErrorLed::Flashing
        } else if self.input == SupplyInput::Normal {
            // Explicit local initial-indicator convention; not a manufactured
            // healthy input and not an assertion about hardware startup LEDs.
            ErrorLed::Off
        } else {
            ErrorLed::Unknown
        }
    }

    /// Normal time credited by the selected recovery policy. RestartAfterControlOff
    /// may restart this interval even though the external input stayed Normal.
    /// Storage-only periods never accrue virtual recovery time.
    pub fn normal_for_us(&self) -> u128 {
        self.normal_for_us
    }

    pub fn power_elapsed_us(&self) -> u128 {
        self.power_elapsed_us
    }

    pub fn status(&self) -> PowerStatus {
        PowerStatus {
            input: self.input,
            backend: self.backend,
            recovery_policy: self.recovery_policy,
            led: self.led(),
            normal_for_us: self.normal_for_us,
            power_elapsed_us: self.power_elapsed_us,
        }
    }

    fn sync(&mut self, control: bool) {
        if self.backend == PowerBackend::DocumentedVirtual {
            if self.input != SupplyInput::Normal
                || (self.previous_control == Some(true)
                    && !control
                    && self.recovery_policy == RecoveryPolicy::RestartAfterControlOff)
            {
                self.normal_for_us = 0;
            }
            if self.input == SupplyInput::Below17800Mv
                || (control && self.input == SupplyInput::Unstable)
            {
                self.fault_latched = true;
            } else if !control
                && self.input == SupplyInput::Normal
                && self.normal_for_us > RECOVERY_US
            {
                self.fault_latched = false;
            }
        }
        self.previous_control = Some(control);
        if self.used {
            self.record_notices(control);
        }
    }

    fn record_notices(&mut self, control: bool) {
        if self.backend == PowerBackend::InstalledStorageOnly {
            self.notices.insert(STORAGE_SCOPE.into());
            return;
        }
        self.notices.insert(VIRTUAL_SCOPE.into());
        self.notices.insert(
            match self.recovery_policy {
                RecoveryPolicy::RestartAfterControlOff => RESTART_POLICY,
                RecoveryPolicy::ReuseContinuousNormal => REUSE_POLICY,
            }
            .into(),
        );
        if self.input == SupplyInput::Unknown {
            self.notices.insert(UNKNOWN_SCOPE.into());
        } else if !control && self.input == SupplyInput::Unstable {
            self.notices.insert(UNSTABLE_OFF_SCOPE.into());
        }
    }

    /// Observe canonical M1019 without modifying any PLC bit, word, RUN state,
    /// or time. Ordinary RUN/STOP servicing never resets the fault or timer.
    pub fn service(m: &mut Memory) {
        let control = m.system_bit(1019);
        m.power_monitor.sync(control);
    }

    /// Explicit POWER-domain elapsed time, including time supplied during STOP.
    /// No host-clock sampling or scan-count-to-time conversion occurs here.
    pub fn advance(m: &mut Memory, delta_us: u64) {
        let control = m.system_bit(1019);
        let state = &mut m.power_monitor;
        state.sync(control);
        state.power_elapsed_us = state.power_elapsed_us.saturating_add(u128::from(delta_us));
        if state.backend == PowerBackend::DocumentedVirtual && state.input == SupplyInput::Normal {
            state.normal_for_us = state.normal_for_us.saturating_add(u128::from(delta_us));
        }
        state.sync(control);
    }

    /// Publish a classification at the current explicit time. Repeated Normal
    /// samples preserve continuity; any different classification breaks it.
    pub fn set_input(m: &mut Memory, input: SupplyInput) {
        let control = m.system_bit(1019);
        let state = &mut m.power_monitor;
        state.sync(control);
        if state.input != input {
            state.normal_for_us = 0;
        }
        state.input = input;
        state.used = true;
        state.sync(control);
    }

    /// Mark an actual facility observation. Background service/advance alone
    /// deliberately does not produce notices for unrelated PLC programs.
    pub fn observe(m: &mut Memory) {
        m.power_monitor.used = true;
        Self::service(m);
    }

    /// Call after any actual M1019 write, including a same-value or group write.
    /// The write remains owned by Memory; only its consequences are serviced.
    pub fn control_written(m: &mut Memory) {
        m.power_monitor.used = true;
        Self::service(m);
    }

    /// A backend change is a local configuration boundary. It breaks recovery
    /// continuity, retains an already-known virtual fault, and does not turn
    /// storage-only elapsed time/input changes into a virtual fault history.
    /// Selecting a backend alone does not mark the facility as observed.
    pub fn set_backend(m: &mut Memory, backend: PowerBackend) {
        let control = m.system_bit(1019);
        let state = &mut m.power_monitor;
        state.sync(control);
        if state.backend != backend {
            state.backend = backend;
            state.normal_for_us = 0;
        }
        state.sync(control);
    }

    /// Changing the policy does not itself create a control transition or
    /// retroactively restart time; the new policy governs subsequent edges.
    pub fn set_recovery_policy(m: &mut Memory, policy: RecoveryPolicy) {
        let control = m.system_bit(1019);
        let state = &mut m.power_monitor;
        state.sync(control);
        state.recovery_policy = policy;
        state.sync(control);
    }

    /// Fresh explicit power origin: external configuration/input survive, while
    /// latch, timer and transition/observation history do not. The caller must
    /// separately clear canonical M1019 and service the retained input.
    pub fn restart_for_power_on(&self) -> Self {
        Self {
            input: self.input,
            backend: self.backend,
            recovery_policy: self.recovery_policy,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn control(m: &mut Memory, enabled: bool) {
        m.set_system(1019, enabled);
        PowerState::control_written(m);
    }

    fn fault_then_normal(m: &mut Memory) {
        PowerState::set_input(m, SupplyInput::Below17800Mv);
        assert_eq!(m.power_monitor.led(), ErrorLed::Flashing);
        PowerState::set_input(m, SupplyInput::Normal);
    }

    #[test]
    fn unknown_default_and_background_time_never_manufacture_health_or_notices() {
        let mut m = Memory::default();
        assert_eq!(m.power_monitor.input(), SupplyInput::Unknown);
        assert_eq!(m.power_monitor.led(), ErrorLed::Unknown);
        PowerState::service(&mut m);
        PowerState::advance(&mut m, u64::MAX);
        assert_eq!(m.power_monitor.led(), ErrorLed::Unknown);
        assert_eq!(m.power_monitor.normal_for_us(), 0);
        assert!(m.power_monitor.notices.is_empty());
        PowerState::observe(&mut m);
        assert!(m.power_monitor.notices.contains(UNKNOWN_SCOPE));
        let notices = m.power_monitor.notices.clone();
        PowerState::observe(&mut m);
        PowerState::service(&mut m);
        assert_eq!(m.power_monitor.notices, notices);
    }

    #[test]
    fn recovery_requires_strictly_more_than_two_normal_seconds() {
        let mut m = Memory::default();
        fault_then_normal(&mut m);
        PowerState::advance(&mut m, 1_999_999);
        assert_eq!(m.power_monitor.led(), ErrorLed::Flashing);
        PowerState::set_input(&mut m, SupplyInput::Normal);
        PowerState::advance(&mut m, 1);
        assert_eq!(m.power_monitor.normal_for_us(), 2_000_000);
        assert_eq!(m.power_monitor.led(), ErrorLed::Flashing);
        PowerState::advance(&mut m, 1);
        assert_eq!(m.power_monitor.led(), ErrorLed::Off);
        assert!(!m.system_bit(1019));
    }

    #[test]
    fn unknown_interrupts_recovery_and_retains_known_fault() {
        let mut m = Memory::default();
        fault_then_normal(&mut m);
        PowerState::advance(&mut m, 2_000_000);
        PowerState::set_input(&mut m, SupplyInput::Unknown);
        PowerState::advance(&mut m, 10_000_000);
        assert_eq!(m.power_monitor.normal_for_us(), 0);
        assert_eq!(m.power_monitor.led(), ErrorLed::Flashing);
        PowerState::set_input(&mut m, SupplyInput::Normal);
        PowerState::advance(&mut m, 2_000_000);
        assert_eq!(m.power_monitor.led(), ErrorLed::Flashing);
        PowerState::advance(&mut m, 1);
        assert_eq!(m.power_monitor.led(), ErrorLed::Off);
    }

    #[test]
    fn unstable_is_not_a_below_threshold_sample_when_control_is_off() {
        let mut m = Memory::default();
        PowerState::set_input(&mut m, SupplyInput::Normal);
        assert_eq!(m.power_monitor.led(), ErrorLed::Off);
        PowerState::set_input(&mut m, SupplyInput::Unstable);
        PowerState::advance(&mut m, 3_000_000);
        assert_eq!(m.power_monitor.led(), ErrorLed::Unknown);
        assert!(m.power_monitor.notices.contains(UNSTABLE_OFF_SCOPE));
        control(&mut m, true);
        assert_eq!(m.power_monitor.led(), ErrorLed::Flashing);
        control(&mut m, false);
        assert_eq!(m.power_monitor.led(), ErrorLed::Flashing);
        assert_eq!(m.power_monitor.normal_for_us(), 0);
    }

    #[test]
    fn on_latches_through_normal_and_default_off_edge_restarts_recovery() {
        let mut m = Memory::default();
        control(&mut m, true);
        PowerState::set_input(&mut m, SupplyInput::Unstable);
        PowerState::set_input(&mut m, SupplyInput::Normal);
        PowerState::advance(&mut m, 8_000_000);
        assert_eq!(m.power_monitor.led(), ErrorLed::Flashing);
        assert_eq!(m.power_monitor.normal_for_us(), 8_000_000);
        control(&mut m, false);
        assert_eq!(m.power_monitor.normal_for_us(), 0);
        PowerState::advance(&mut m, 2_000_000);
        control(&mut m, false); // Same-value writes are not new edges.
        assert_eq!(m.power_monitor.normal_for_us(), 2_000_000);
        assert_eq!(m.power_monitor.led(), ErrorLed::Flashing);
        PowerState::advance(&mut m, 1);
        assert_eq!(m.power_monitor.led(), ErrorLed::Off);
    }

    #[test]
    fn alternate_policy_reuses_normal_time_but_keeps_strict_boundary() {
        let mut m = Memory::default();
        PowerState::set_recovery_policy(&mut m, RecoveryPolicy::ReuseContinuousNormal);
        control(&mut m, true);
        fault_then_normal(&mut m);
        PowerState::advance(&mut m, 2_000_000);
        control(&mut m, false);
        assert_eq!(m.power_monitor.normal_for_us(), 2_000_000);
        assert_eq!(m.power_monitor.led(), ErrorLed::Flashing);
        PowerState::advance(&mut m, 1);
        assert_eq!(m.power_monitor.led(), ErrorLed::Off);

        control(&mut m, true);
        fault_then_normal(&mut m);
        PowerState::advance(&mut m, 2_000_001);
        assert_eq!(m.power_monitor.led(), ErrorLed::Flashing);
        control(&mut m, false);
        assert_eq!(m.power_monitor.led(), ErrorLed::Off);
    }

    #[test]
    fn changing_recovery_policy_does_not_invent_a_control_edge() {
        let mut m = Memory::default();
        control(&mut m, true);
        fault_then_normal(&mut m);
        PowerState::advance(&mut m, 3_000_000);
        PowerState::set_recovery_policy(&mut m, RecoveryPolicy::ReuseContinuousNormal);
        assert_eq!(m.power_monitor.normal_for_us(), 3_000_000);
        assert_eq!(m.power_monitor.led(), ErrorLed::Flashing);
        PowerState::set_recovery_policy(&mut m, RecoveryPolicy::RestartAfterControlOff);
        assert_eq!(m.power_monitor.normal_for_us(), 3_000_000);
        control(&mut m, false);
        assert_eq!(m.power_monitor.normal_for_us(), 0);
        assert_eq!(m.power_monitor.led(), ErrorLed::Flashing);
    }

    #[test]
    fn later_fault_retriggers_and_nonnormal_sample_breaks_continuity() {
        let mut m = Memory::default();
        fault_then_normal(&mut m);
        PowerState::advance(&mut m, 2_000_001);
        assert_eq!(m.power_monitor.led(), ErrorLed::Off);
        PowerState::set_input(&mut m, SupplyInput::Below17800Mv);
        assert_eq!(m.power_monitor.led(), ErrorLed::Flashing);
        assert_eq!(m.power_monitor.normal_for_us(), 0);
        PowerState::set_input(&mut m, SupplyInput::Normal);
        PowerState::advance(&mut m, 1_000_000);
        PowerState::set_input(&mut m, SupplyInput::Unstable);
        assert_eq!(m.power_monitor.normal_for_us(), 0);
        PowerState::set_input(&mut m, SupplyInput::Normal);
        PowerState::advance(&mut m, 2_000_000);
        assert_eq!(m.power_monitor.led(), ErrorLed::Flashing);
    }

    #[test]
    fn huge_elapsed_values_and_split_advances_use_wide_time_without_wrapping() {
        let mut whole = Memory::default();
        fault_then_normal(&mut whole);
        let mut split = whole.clone();
        PowerState::advance(&mut whole, u64::MAX);
        PowerState::advance(&mut split, u64::MAX - 1);
        PowerState::advance(&mut split, 1);
        assert_eq!(whole.power_monitor.status(), split.power_monitor.status());
        PowerState::advance(&mut whole, u64::MAX);
        assert_eq!(
            whole.power_monitor.power_elapsed_us(),
            u128::from(u64::MAX) * 2
        );
        assert_eq!(
            whole.power_monitor.normal_for_us(),
            u128::from(u64::MAX) * 2
        );
        assert_eq!(whole.power_monitor.led(), ErrorLed::Off);
    }

    #[test]
    fn storage_backend_is_unavailable_and_never_manufactures_voltage_history() {
        let mut m = Memory::default();
        PowerState::set_backend(&mut m, PowerBackend::InstalledStorageOnly);
        PowerState::advance(&mut m, 10_000_000);
        assert_eq!(m.power_monitor.led(), ErrorLed::Unavailable);
        assert!(m.power_monitor.notices.is_empty());
        control(&mut m, true);
        PowerState::set_input(&mut m, SupplyInput::Below17800Mv);
        PowerState::advance(&mut m, 10_000_000);
        PowerState::set_input(&mut m, SupplyInput::Normal);
        PowerState::advance(&mut m, 10_000_000);
        assert_eq!(m.power_monitor.normal_for_us(), 0);
        assert_eq!(m.power_monitor.led(), ErrorLed::Unavailable);
        assert!(m.power_monitor.notices.contains(STORAGE_SCOPE));
        assert!(!m.power_monitor.notices.contains(VIRTUAL_SCOPE));
        PowerState::set_backend(&mut m, PowerBackend::DocumentedVirtual);
        assert_eq!(m.power_monitor.led(), ErrorLed::Off);
        assert_eq!(m.power_monitor.normal_for_us(), 0);
        assert!(m.system_bit(1019));
    }

    #[test]
    fn backend_switch_preserves_known_fault_but_breaks_recovery_continuity() {
        let mut m = Memory::default();
        fault_then_normal(&mut m);
        PowerState::advance(&mut m, 1_999_999);
        PowerState::set_backend(&mut m, PowerBackend::InstalledStorageOnly);
        PowerState::advance(&mut m, 10_000_000);
        PowerState::set_input(&mut m, SupplyInput::Unknown);
        PowerState::set_backend(&mut m, PowerBackend::DocumentedVirtual);
        assert_eq!(m.power_monitor.led(), ErrorLed::Flashing);
        assert_eq!(m.power_monitor.normal_for_us(), 0);
        PowerState::set_input(&mut m, SupplyInput::Normal);
        PowerState::advance(&mut m, 2_000_000);
        assert_eq!(m.power_monitor.led(), ErrorLed::Flashing);
        PowerState::advance(&mut m, 1);
        assert_eq!(m.power_monitor.led(), ErrorLed::Off);
    }

    #[test]
    fn power_restart_preserves_external_configuration_and_resets_only_local_history() {
        let mut m = Memory::default();
        PowerState::set_recovery_policy(&mut m, RecoveryPolicy::ReuseContinuousNormal);
        control(&mut m, true);
        fault_then_normal(&mut m);
        PowerState::advance(&mut m, 5_000_000);
        let restarted = m.power_monitor.restart_for_power_on();
        assert_eq!(restarted.input(), SupplyInput::Normal);
        assert_eq!(restarted.backend(), PowerBackend::DocumentedVirtual);
        assert_eq!(
            restarted.recovery_policy(),
            RecoveryPolicy::ReuseContinuousNormal
        );
        assert_eq!(restarted.power_elapsed_us(), 0);
        assert_eq!(restarted.normal_for_us(), 0);
        assert_eq!(restarted.led(), ErrorLed::Off);
        assert!(restarted.notices.is_empty());
        assert!(m.system_bit(1019)); // Restart helper does not own the PLC bit.

        m.power_monitor = restarted;
        m.set_system(1019, false); // The parent reset integration owns this clear.
        PowerState::service(&mut m);
        PowerState::set_input(&mut m, SupplyInput::Below17800Mv);
        m.power_monitor = m.power_monitor.restart_for_power_on();
        PowerState::service(&mut m);
        assert_eq!(m.power_monitor.led(), ErrorLed::Flashing);

        PowerState::set_backend(&mut m, PowerBackend::InstalledStorageOnly);
        let restarted = m.power_monitor.restart_for_power_on();
        assert_eq!(restarted.backend(), PowerBackend::InstalledStorageOnly);
        assert_eq!(restarted.input(), SupplyInput::Below17800Mv);
        assert_eq!(restarted.led(), ErrorLed::Unavailable);
        assert!(restarted.notices.is_empty());
    }

    #[test]
    fn service_observation_and_elapsed_time_do_not_write_plc_storage_or_flags() {
        let mut m = Memory::default();
        for (index, value) in [
            (1009, true),
            (1019, true),
            (1020, true),
            (1021, false),
            (1022, true),
        ] {
            m.set_system(index, value);
        }
        PowerState::set_input(&mut m, SupplyInput::Below17800Mv);
        PowerState::observe(&mut m);
        PowerState::service(&mut m);
        PowerState::advance(&mut m, 9_000_000);
        for (index, expected) in [
            (1009, true),
            (1019, true),
            (1020, true),
            (1021, false),
            (1022, true),
        ] {
            assert_eq!(m.system_bit(index), expected);
        }
        assert!(m.execution_limitations.is_empty());
    }
}
