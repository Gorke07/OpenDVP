//! Explicit clock domains for M1011..M1015, not host-time or hardware emulation.
//!
//! Documented clocks use a named power-on phase (OFF at elapsed0) and continue
//! across RUN/STOP. The installed fixed-scan clock has independent END phases,
//! including its observed minute-clock threshold defect. Its first body levels
//! are an external one-time input, never guessed from scan number or outputs.
use crate::plc::{Area, Device, Memory};
use std::collections::BTreeSet;

const PERIODS_US: [u64; 4] = [10_000, 100_000, 1_000_000, 60_000_000];
const FIXED_PERIODS_MS: [u32; 4] = [10, 100, 1_000, 60_000];
const UNSEEDED: &str = "Installed fixed clock: first-body M1011..M1014 levels require a one-time external seed before observation";
const HIGH_SPEED_UNVERIFIED: &str = "M1015/D1015 installed high-speed timing is unverified; select the explicit local virtual backend or provide native capability evidence";
const STORAGE_SCOPE: &str = "M1015/D1015 installed fixed-scan storage-only capability: bounded original ES2/SS2 observations do not implement or validate the documented100us timer";
const STORAGE_OUTSIDE_FIXED: &str = "M1015/D1015 InstalledFixedStorage is qualified only with M1039 ON; uncontrolled host-clock/STOP behavior is unverified";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ClockProfile {
    /// Ideal50%duty virtualpowerclock, phase0/OFF at explicitpowerorigin.
    #[default]
    Documented,
    /// Original ES2/SS2 M1039 fixed-scan producer, not its QPC callback.
    InstalledFixed,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StopPolicy {
    /// Explicit local choice: an END-armed high-speed timer continues in STOP.
    #[default]
    ContinueWhileStopped,
    /// Explicit alternate choice; pulse relays still continue across STOP.
    FreezeWhileStopped,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FractionPolicy {
    #[default]
    Retain,
    DiscardOnDisable,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReenablePolicy {
    /// Every observed OFF unarms; a new enabled END must arm again.
    #[default]
    RequireEnd,
    /// After the first enabled END only, later ON resumes immediately.
    ResumeAfterFirstEnd,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ResetPolicy {
    ValueOnly,
    #[default]
    ValueAndFraction,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HighSpeedPolicy {
    pub stop: StopPolicy,
    pub fraction: FractionPolicy,
    pub reenable: ReenablePolicy,
    /// Applies only to explicit reset_high_speed(), not arbitrary D writes.
    pub reset: ResetPolicy,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HighSpeedBackend {
    /// PDF-based deterministic candidate; unknown physical facets are policies.
    #[default]
    Virtual100Us,
    /// Does not quietly replace an unverified original writer by the candidate.
    InstalledUnverified,
    /// Explicitly selected only for the digest-bound old fixed-scan targets.
    /// Bounded runs hold D1015; that is not a100us timer implementation.
    InstalledFixedStorage,
}

#[derive(Debug, Clone)]
pub struct ClockState {
    profile: ClockProfile,
    power_elapsed_us: u128,
    virtual_phase_us: [u64; 4],
    fixed_phase_ms: [u32; 4],
    installed_levels: Option<[bool; 4]>,
    installed_seeded: bool,
    fixed_end_seen: bool,
    high_speed_backend: HighSpeedBackend,
    high_speed_policy: HighSpeedPolicy,
    high_speed_noticed_policy: Option<HighSpeedPolicy>,
    high_speed_armed: bool,
    high_speed_ever_armed: bool,
    high_speed_last_enable: bool,
    high_speed_remainder_us: u8,
    running: bool,
    /// Informational profile qualifications, not invented PLC operation errors.
    pub notices: BTreeSet<String>,
}
impl Default for ClockState {
    fn default() -> Self {
        Self {
            profile: ClockProfile::Documented,
            power_elapsed_us: 0,
            virtual_phase_us: [0; 4],
            fixed_phase_ms: [0; 4],
            installed_levels: None,
            installed_seeded: false,
            fixed_end_seen: false,
            high_speed_backend: HighSpeedBackend::Virtual100Us,
            high_speed_policy: HighSpeedPolicy::default(),
            high_speed_noticed_policy: None,
            high_speed_armed: false,
            high_speed_ever_armed: false,
            high_speed_last_enable: false,
            high_speed_remainder_us: 0,
            running: false,
            notices: BTreeSet::new(),
        }
    }
}
fn d(index: u16) -> Device {
    Device {
        area: Area::D,
        index,
    }
}

impl ClockState {
    pub fn profile(&self) -> ClockProfile {
        self.profile
    }
    pub fn power_elapsed_us(&self) -> u128 {
        self.power_elapsed_us
    }
    pub fn high_speed_armed(&self) -> bool {
        self.high_speed_armed
    }
    pub fn high_speed_remainder_us(&self) -> u8 {
        self.high_speed_remainder_us
    }
    pub fn high_speed_policy(&self) -> HighSpeedPolicy {
        self.high_speed_policy
    }
    pub fn high_speed_backend(&self) -> HighSpeedBackend {
        self.high_speed_backend
    }
    pub fn fixed_phases_ms(&self) -> [u32; 4] {
        self.fixed_phase_ms
    }
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Configuration changes do not reset elapsed time, phase, D value or END
    /// ownership. A fresh ClockState represents a new explicit power origin.
    pub fn set_profile(&mut self, profile: ClockProfile) {
        self.profile = profile;
        self.high_speed_backend = match profile {
            ClockProfile::Documented => HighSpeedBackend::Virtual100Us,
            ClockProfile::InstalledFixed => HighSpeedBackend::InstalledUnverified,
        };
    }
    pub fn set_high_speed_backend(&mut self, backend: HighSpeedBackend) {
        self.high_speed_backend = backend;
        if backend != HighSpeedBackend::Virtual100Us {
            self.high_speed_armed = false;
        }
    }
    pub fn set_high_speed_policy(&mut self, policy: HighSpeedPolicy) {
        self.high_speed_policy = policy;
    }
    /// RUN initialization in the installed binary resets fixed phases, while
    /// power elapsed and its host-produced clock bit image remain independent.
    /// Call only at an actual fresh RUN boundary, not on every scan or pause.
    pub fn restart_installed_run(&mut self) {
        self.fixed_phase_ms = [0; 4];
        self.installed_levels = None;
        self.installed_seeded = false;
        self.fixed_end_seen = false;
    }
    pub fn clock_levels(&self) -> Option<[bool; 4]> {
        match self.profile {
            ClockProfile::Documented => Some(std::array::from_fn(|n| {
                self.virtual_phase_us[n] >= PERIODS_US[n] / 2
            })),
            ClockProfile::InstalledFixed => self.installed_levels,
        }
    }

    /// Seed only M1011..14 input levels once BEFORE the first installed END.
    /// Phases remain at independently initialized zero; observed levels are not
    /// used to infer hidden phase or repeatedly overwrite predicted endpoints.
    pub fn seed_installed_levels(m: &mut Memory, levels: [bool; 4]) -> Result<(), String> {
        let state = &mut m.system_relays;
        if state.profile != ClockProfile::InstalledFixed {
            return Err("Clock level seed requires InstalledFixed profile".into());
        }
        if state.installed_seeded || state.fixed_end_seen {
            return Err("Installed clock seed is allowed only once before the first END".into());
        }
        state.installed_seeded = true;
        state.installed_levels = Some(levels);
        m.execution_limitations.remove(UNSEEDED);
        for (n, value) in levels.into_iter().enumerate() {
            m.set_system(1011 + n as u16, value);
        }
        Ok(())
    }
    /// Optional integration seam for an actual source clock read. Merely
    /// servicing/uploading an unseeded session is not itself a false error.
    pub fn require_clock_observation(m: &mut Memory) -> Result<(), String> {
        if m.system_relays.clock_levels().is_none() {
            m.execution_limitations.insert(UNSEEDED.into());
            Err(UNSEEDED.into())
        } else {
            Ok(())
        }
    }
    /// Clock-domain lifecycle only; does not implement general PLC STOP
    /// retention, output latching, memory clearing or real host scheduling.
    pub fn set_running(m: &mut Memory, running: bool) {
        m.system_relays.running = running;
    }

    fn sync(&mut self, m: &mut Memory) {
        if let Some(levels) = self.clock_levels() {
            for (n, value) in levels.into_iter().enumerate() {
                m.set_system(1011 + n as u16, value);
            }
        }
        let enabled = m.system_bit(1015);
        if !enabled {
            if self.high_speed_last_enable
                && self.high_speed_policy.fraction == FractionPolicy::DiscardOnDisable
            {
                self.high_speed_remainder_us = 0;
            }
            self.high_speed_armed = false;
        } else {
            if !self.high_speed_last_enable
                && self.high_speed_ever_armed
                && self.high_speed_backend == HighSpeedBackend::Virtual100Us
                && self.high_speed_policy.reenable == ReenablePolicy::ResumeAfterFirstEnd
            {
                self.high_speed_armed = true;
            }
            match self.high_speed_backend {
                HighSpeedBackend::InstalledUnverified => {
                    m.execution_limitations.insert(HIGH_SPEED_UNVERIFIED.into());
                }
                HighSpeedBackend::InstalledFixedStorage => {
                    self.high_speed_armed = false;
                    if !self.notices.contains(STORAGE_SCOPE) {
                        self.notices.insert(STORAGE_SCOPE.into());
                    }
                    if !m.system_bit(1039) {
                        m.execution_limitations.insert(STORAGE_OUTSIDE_FIXED.into());
                    }
                }
                HighSpeedBackend::Virtual100Us => {
                    if self.high_speed_noticed_policy != Some(self.high_speed_policy) {
                        self.notices.insert(format!("M1015/D1015 local virtual100us: {:?}; fraction {:?}; re-enable {:?}; explicit reset {:?}. STOP/fraction/reset choices are not installed-native or physical timing proof.",self.high_speed_policy.stop,self.high_speed_policy.fraction,self.high_speed_policy.reenable,self.high_speed_policy.reset));
                        self.high_speed_noticed_policy = Some(self.high_speed_policy);
                    }
                }
            }
        }
        self.high_speed_last_enable = enabled;
    }
    /// Apply relay ownership and observe OFF immediately at each actual action
    /// visit. No time elapses and no D1015 reset is invented on an ON edge.
    pub fn service(m: &mut Memory) {
        let mut state = std::mem::take(&mut m.system_relays);
        state.sync(m);
        m.system_relays = state;
    }
    /// Deterministic POWER time supplied by the caller, including explicit STOP
    /// advancement. Never call host now/sleep or infer dt from instruction count.
    pub fn advance(m: &mut Memory, delta_us: u64) {
        let mut state = std::mem::take(&mut m.system_relays);
        state.sync(m);
        state.power_elapsed_us = state.power_elapsed_us.saturating_add(u128::from(delta_us));
        for (phase, period) in state.virtual_phase_us.iter_mut().zip(PERIODS_US) {
            *phase = ((u128::from(*phase) + u128::from(delta_us)) % u128::from(period)) as u64;
        }
        let can_tick = state.high_speed_armed
            && m.system_bit(1015)
            && state.high_speed_backend == HighSpeedBackend::Virtual100Us
            && (state.running || state.high_speed_policy.stop == StopPolicy::ContinueWhileStopped);
        if can_tick {
            let elapsed = u128::from(delta_us) + u128::from(state.high_speed_remainder_us);
            let ticks = elapsed / 100;
            state.high_speed_remainder_us = (elapsed % 100) as u8;
            if ticks != 0 {
                let value = u128::from(m.word(d(1015)) as u16 & 0x7fff);
                m.set_word(d(1015), ((value + ticks) % 32768) as i16);
            }
        }
        state.sync(m);
        m.system_relays = state;
    }
    /// Actual completed END/FEND only. Arming takes effect AFTER this boundary,
    /// never retroactively for the elapsed scan or an aborted execution body.
    pub fn end_scan(m: &mut Memory) {
        let mut state = std::mem::take(&mut m.system_relays);
        state.sync(m);
        if state.profile == ClockProfile::InstalledFixed {
            if !m.system_bit(1039) {
                m.execution_limitations.insert("InstalledFixed clock requires M1039 ON; the uncontrolled QPC clock producer is not modeled by fixed END phases".into());
            } else {
                let quantum = m.word(d(1039));
                if quantum < 0 {
                    m.execution_limitations
                        .insert("InstalledFixed negative D1039 clock quantum is unverified".into());
                } else {
                    for (phase, period) in state.fixed_phase_ms.iter_mut().zip(FIXED_PERIODS_MS) {
                        *phase = (*phase + quantum as u32) % period;
                    }
                    state.installed_levels = Some([
                        state.fixed_phase_ms[0] >= 5,
                        state.fixed_phase_ms[1] >= 50,
                        state.fixed_phase_ms[2] >= 500,
                        // Original remainder<60000 can never reach300000.
                        state.fixed_phase_ms[3] >= 300_000,
                    ]);
                    state.fixed_end_seen = true;
                }
            }
        }
        if m.system_bit(1015) && state.high_speed_backend == HighSpeedBackend::Virtual100Us {
            state.high_speed_armed = true;
            state.high_speed_ever_armed = true;
        }
        state.sync(m);
        m.system_relays = state;
    }
    /// Explicit local reset event. Ordinary source MOV D1015 changes its value
    /// without silently changing elapsed remainder, enable or END ownership.
    pub fn reset_high_speed(m: &mut Memory) {
        m.set_word(d(1015), 0);
        if m.system_relays.high_speed_policy.reset == ResetPolicy::ValueAndFraction {
            m.system_relays.high_speed_remainder_us = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn levels(m: &Memory) -> [bool; 4] {
        std::array::from_fn(|n| m.system_bit(1011 + n as u16))
    }
    fn fixed(quantum: i16) -> Memory {
        let mut m = Memory::default();
        m.system_relays.set_profile(ClockProfile::InstalledFixed);
        m.set_system(1039, true);
        m.set_word(d(1039), quantum);
        m
    }
    #[test]
    fn documented_exact_half_periods_and_full_periods_start_off() {
        for (slot, period) in PERIODS_US.into_iter().enumerate() {
            let mut m = Memory::default();
            ClockState::service(&mut m);
            assert!(!levels(&m)[slot]);
            ClockState::advance(&mut m, period / 2 - 1);
            assert!(!levels(&m)[slot]);
            ClockState::advance(&mut m, 1);
            assert!(levels(&m)[slot]);
            ClockState::advance(&mut m, period / 2);
            assert!(!levels(&m)[slot]);
        }
    }
    #[test]
    fn power_clock_continues_stopped_without_run_phase_reset() {
        let mut m = Memory::default();
        ClockState::set_running(&mut m, false);
        ClockState::advance(&mut m, 4999);
        ClockState::set_running(&mut m, true);
        ClockState::advance(&mut m, 1);
        assert!(m.system_bit(1011));
        ClockState::set_running(&mut m, false);
        ClockState::advance(&mut m, 5000);
        assert!(!m.system_bit(1011));
        assert_eq!(m.system_relays.power_elapsed_us(), 10_000);
    }
    #[test]
    fn splitting_elapsed_time_preserves_phase_including_u64_max_delta() {
        let mut a = Memory::default();
        let mut b = Memory::default();
        for dt in [u64::MAX, 17, 89, 7_654_321] {
            ClockState::advance(&mut a, dt);
        }
        ClockState::advance(&mut b, u64::MAX);
        ClockState::advance(&mut b, 17 + 89 + 7_654_321);
        assert_eq!(levels(&a), levels(&b));
        assert_eq!(
            a.system_relays.power_elapsed_us(),
            b.system_relays.power_elapsed_us()
        );
    }
    #[test]
    fn installed_first_body_seed_once_then_independent_end_prediction() {
        let mut m = fixed(100);
        ClockState::service(&mut m);
        assert!(m.execution_limitations.is_empty());
        assert!(ClockState::require_clock_observation(&mut m).is_err());
        ClockState::seed_installed_levels(&mut m, [true, true, true, true]).unwrap();
        assert!(m.execution_limitations.is_empty());
        ClockState::advance(&mut m, 30_000_000);
        assert_eq!(levels(&m), [true; 4]);
        for scan in 1..=10 {
            ClockState::end_scan(&mut m);
            assert_eq!(levels(&m), [false, false, (5..10).contains(&scan), false]);
        }
        assert!(ClockState::seed_installed_levels(&mut m, [false; 4]).is_err());
    }
    #[test]
    fn installed_minute_defect_and_fixed_phase_are_distinct_from_power_clock() {
        let mut m = fixed(7);
        ClockState::seed_installed_levels(&mut m, [false; 4]).unwrap();
        ClockState::end_scan(&mut m);
        assert_eq!(levels(&m), [true, false, false, false]);
        ClockState::end_scan(&mut m);
        assert_eq!(levels(&m), [false, false, false, false]);
        for _ in 0..5000 {
            ClockState::end_scan(&mut m);
        }
        assert!(!m.system_bit(1014));
        assert_eq!(m.system_relays.power_elapsed_us(), 0);
        m.system_relays.set_profile(ClockProfile::Documented);
        ClockState::advance(&mut m, 30_000_000);
        assert!(m.system_bit(1014));
    }
    #[test]
    fn high_speed_starts_after_end_and_off_freezes_without_zeroing() {
        let mut m = Memory::default();
        m.set_word(d(1015), 19);
        m.set_system(1015, true);
        ClockState::service(&mut m);
        ClockState::advance(&mut m, 300);
        assert_eq!(m.word(d(1015)), 19);
        ClockState::end_scan(&mut m);
        ClockState::advance(&mut m, 199);
        assert_eq!(m.word(d(1015)), 20);
        assert_eq!(m.system_relays.high_speed_remainder_us(), 99);
        m.set_system(1015, false);
        ClockState::service(&mut m);
        ClockState::advance(&mut m, 1000);
        assert_eq!(m.word(d(1015)), 20);
        m.set_system(1015, true);
        ClockState::service(&mut m);
        ClockState::advance(&mut m, 1);
        assert_eq!(m.word(d(1015)), 20);
        ClockState::end_scan(&mut m);
        ClockState::advance(&mut m, 1);
        assert_eq!(m.word(d(1015)), 21);
        assert!(!m.system_relays.notices.is_empty());
    }
    #[test]
    fn high_speed_wrap_user_write_and_explicit_reset_fraction_policy() {
        let mut m = Memory::default();
        m.set_word(d(1015), 32767);
        m.set_system(1015, true);
        ClockState::end_scan(&mut m);
        ClockState::advance(&mut m, 150);
        assert_eq!(m.word(d(1015)), 0);
        m.set_word(d(1015), 10);
        ClockState::advance(&mut m, 50);
        assert_eq!(m.word(d(1015)), 11);
        ClockState::advance(&mut m, 75);
        ClockState::reset_high_speed(&mut m);
        ClockState::advance(&mut m, 25);
        assert_eq!(m.word(d(1015)), 0);
        m.system_relays.set_high_speed_policy(HighSpeedPolicy {
            reset: ResetPolicy::ValueOnly,
            ..HighSpeedPolicy::default()
        });
        ClockState::advance(&mut m, 50);
        ClockState::reset_high_speed(&mut m);
        ClockState::advance(&mut m, 25);
        assert_eq!(m.word(d(1015)), 1);
    }
    #[test]
    fn explicit_stop_freeze_and_disable_discard_and_immediate_reenable_choices() {
        let mut m = Memory::default();
        m.system_relays.set_high_speed_policy(HighSpeedPolicy {
            stop: StopPolicy::FreezeWhileStopped,
            fraction: FractionPolicy::DiscardOnDisable,
            reenable: ReenablePolicy::ResumeAfterFirstEnd,
            ..HighSpeedPolicy::default()
        });
        m.set_system(1015, true);
        ClockState::end_scan(&mut m);
        ClockState::advance(&mut m, 1000);
        assert_eq!(m.word(d(1015)), 0);
        ClockState::set_running(&mut m, true);
        ClockState::advance(&mut m, 175);
        assert_eq!(m.word(d(1015)), 1);
        m.set_system(1015, false);
        ClockState::service(&mut m);
        assert_eq!(m.system_relays.high_speed_remainder_us(), 0);
        m.set_system(1015, true);
        ClockState::service(&mut m);
        ClockState::advance(&mut m, 25);
        assert_eq!(m.word(d(1015)), 1);
        ClockState::advance(&mut m, 75);
        assert_eq!(m.word(d(1015)), 2);
    }
    #[test]
    fn installed_high_speed_is_explicitly_unverified_not_fabricated_parity() {
        let mut m = fixed(100);
        ClockState::seed_installed_levels(&mut m, [false; 4]).unwrap();
        m.set_word(d(1015), 123);
        m.set_system(1015, true);
        ClockState::end_scan(&mut m);
        ClockState::advance(&mut m, 1_000_000);
        assert_eq!(m.word(d(1015)), 123);
        assert!(m.execution_limitations.contains(HIGH_SPEED_UNVERIFIED));
        assert!(!m.system_bit(1067));
    }
    #[test]
    fn explicit_installed_storage_preserves_values_through_elapsed_end_and_source_writes() {
        let mut m = fixed(100);
        ClockState::seed_installed_levels(&mut m, [false; 4]).unwrap();
        m.system_relays
            .set_high_speed_backend(HighSpeedBackend::InstalledFixedStorage);
        for seed in [0, 123, 32760, 32767, -1] {
            m.set_word(d(1015), seed);
            m.set_system(1015, true);
            ClockState::service(&mut m);
            for _ in 0..7 {
                ClockState::advance(&mut m, 1_000_175);
                ClockState::end_scan(&mut m);
            }
            assert_eq!(m.word(d(1015)), seed);
            assert!(!m.system_relays.high_speed_armed());
            m.set_system(1015, false);
            ClockState::service(&mut m);
            ClockState::advance(&mut m, 999);
            m.set_system(1015, true);
            ClockState::service(&mut m);
            assert_eq!(m.word(d(1015)), seed);
        }
        assert!(m.execution_limitations.is_empty());
        assert!(m.system_relays.notices.contains(STORAGE_SCOPE));
        assert!(!m.system_bit(1067));
    }
    #[test]
    fn installed_storage_cannot_claim_a_backend_outside_fixed_scan() {
        let mut m = fixed(100);
        m.system_relays
            .set_high_speed_backend(HighSpeedBackend::InstalledFixedStorage);
        m.set_system(1039, false);
        m.set_system(1015, true);
        m.set_word(d(1015), 42);
        ClockState::service(&mut m);
        ClockState::advance(&mut m, 10_000);
        assert!(m.execution_limitations.contains(STORAGE_OUTSIDE_FIXED));
        assert_eq!(m.word(d(1015)), 42);
        assert!(!m.system_relays.high_speed_armed());
    }
}
