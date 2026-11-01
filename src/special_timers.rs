//! Full timer banks and installed teaching/special timer instruction state.
//! Clock inputs are explicit. Virtual instruction time is a local simulation
//! profile; installed fixed-scan teaching ticks must be supplied independently.
use crate::plc::{Area, Device, Memory};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimerPreset {
    Constant(i16),
    Register(Device),
}
impl fmt::Display for TimerPreset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Constant(n) => write!(f, "K{n}"),
            Self::Register(d) => write!(f, "{d}"),
        }
    }
}
impl TimerPreset {
    fn parse(s: &str, hex: bool) -> Result<Self, String> {
        let s = s.to_ascii_uppercase();
        if let Some(s) = s.strip_prefix('K') {
            return s
                .parse::<i16>()
                .ok()
                .filter(|n| *n >= 0)
                .map(Self::Constant)
                .ok_or_else(|| "Timer preset must be K0..32767".into());
        }
        if hex && let Some(s) = s.strip_prefix('H') {
            return u16::from_str_radix(s, 16)
                .ok()
                .filter(|n| *n <= 32767)
                .map(|n| Self::Constant(n as i16))
                .ok_or_else(|| "Timer preset must be H0..7FFF".into());
        }
        let d: Device = s.parse()?;
        if d.area != Area::D {
            return Err("Timer preset register must be D".into());
        }
        Ok(Self::Register(d))
    }
    fn read(&self, m: &Memory) -> i16 {
        match self {
            Self::Constant(n) => *n,
            Self::Register(d) => m.word(*d),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimerAction {
    Timer {
        timer: Device,
        preset: TimerPreset,
        contact: bool,
    },
    Teach {
        dest: Device,
        scale: u8,
    },
    Special {
        timer: Device,
        preset: u16,
        dest: Device,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TeachingTickProfile {
    /// Local monotonic 100ms prescaler, not a native fixed-scan clock claim.
    #[default]
    VirtualElapsed,
    /// Native fixed-scan mode does not generate TTMR ticks automatically.
    InstalledControlled,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SpecialHoldProfile {
    /// Intended one timer/one hold bit semantics, pending physical validation.
    #[default]
    DocumentedBit,
    /// Exact bytes indexed without &7 in both shipped handlers. Static candidate
    /// until independently exercised; this can inhibit neighbouring timers.
    InstalledTableCandidate,
}
#[derive(Debug, Clone, Copy, Default)]
struct TeachingSlot {
    owner: u16,
    ticks: i16,
}
#[derive(Debug, Clone, Default)]
pub struct TimerState {
    enabled: BTreeSet<u16>,
    hold: [u8; 32],
    subroutine_presets: [i16; 16],
    teaching: [TeachingSlot; 8],
    teaching_active: BTreeSet<u16>,
    teaching_pending: u8,
    teaching_count: u8,
    teaching_profile: TeachingTickProfile,
    special_profile: SpecialHoldProfile,
    /// Native activation bit header+1 and phase bit header+5, by real word PC.
    special: BTreeMap<u16, bool>,
}

fn at(area: Area, index: u16) -> Device {
    Device { area, index }
}
fn offset(d: Device, n: u16) -> Device {
    at(d.area, d.index + n)
}
// The installed four-output writer increments the physical bit pointer. The
// low M bank ends at internal 0x0dff; its successor is C0's contact at 0x0e00.
// M1536 itself is separately mapped at 0x1000 and is not that successor.
fn special_output(d: Device, n: u16) -> Device {
    if d.area == Area::M && d.index < 1536 && d.index + n >= 1536 {
        at(Area::C, d.index + n - 1536)
    } else {
        offset(d, n)
    }
}
fn limitation(m: &mut Memory, message: &str) {
    m.execution_limitations.insert(message.into());
}

/// Resolution and retention are deliberately independent classifications.
pub fn timer_base_us(m: &Memory, index: u16) -> u64 {
    match index {
        64..=126 if m.system_bit(1028) => 10_000,
        127 | 246..=249 => 1_000,
        200..=245 if m.system_bit(1038) => 1_000,
        200..=245 => 10_000,
        _ => 100_000,
    }
}

impl TimerState {
    /// ANS has a distinct false/incomplete visit contract from TMR. It shares
    /// the same timer value/contact/enable and class scheduler; no private
    /// elapsed timer or global 100ms override is introduced here.
    pub(crate) fn visit_alarm(m: &mut Memory, timer: Device, preset: u16, power: bool) -> bool {
        let mut state = std::mem::take(&mut m.special_timers);
        let reached = if power {
            let reached = m.word(timer) >= preset as i16;
            if reached {
                m.set_bit(timer, true);
                m.set_word(timer, preset as i16);
            }
            // Below preset, the old done contact is held, not recomputed false.
            state.enable(m, timer, true);
            reached
        } else {
            m.set_bit(timer, false);
            m.set_word(timer, 0);
            // Unlike a false TMR, false ANS leaves any existing enable/hold
            // bits alone, including another same-scan timer owner's enable.
            let armed = state.timer_runs(m, &timer);
            m.timing_io.arm_plc_timer(timer, armed);
            false
        };
        m.special_timers = state;
        reached
    }
    pub fn set_teaching_tick_profile(&mut self, installed_fixed: bool) {
        self.teaching_profile = if installed_fixed {
            TeachingTickProfile::InstalledControlled
        } else {
            TeachingTickProfile::VirtualElapsed
        };
    }
    pub fn teaching_tick_profile(&self) -> TeachingTickProfile {
        self.teaching_profile
    }
    pub fn set_special_hold_profile(&mut self, profile: SpecialHoldProfile) {
        self.special_profile = profile;
    }
    pub fn special_hold_profile(&self) -> SpecialHoldProfile {
        self.special_profile
    }
    pub fn supply_teaching_ticks(&mut self, ticks: u8) {
        self.teaching_pending = self.teaching_pending.wrapping_add(ticks);
    }
    pub fn teaching_pending_ticks(&self) -> u8 {
        self.teaching_pending
    }
    pub fn teaching_owners(&self) -> [u16; 8] {
        self.teaching.map(|s| s.owner)
    }
    pub fn teaching_counters(&self) -> [i16; 8] {
        self.teaching.map(|s| s.ticks)
    }
    pub fn is_enabled(&self, timer: Device) -> bool {
        self.enabled.contains(&timer.index)
    }
    pub fn is_held(&self, timer: Device) -> bool {
        timer.index < 256 && self.hold[usize::from(timer.index / 8)] & (1 << (timer.index & 7)) != 0
    }
    pub fn timer_runs(&self, m: &Memory, timer: &Device) -> bool {
        timer.area == Area::T && self.is_enabled(*timer) && !m.bit(*timer) && !self.is_held(*timer)
    }
    fn enable(&mut self, m: &mut Memory, timer: Device, yes: bool) {
        if yes {
            self.enabled.insert(timer.index);
        } else {
            self.enabled.remove(&timer.index);
        }
        let armed = yes && !m.bit(timer) && !self.is_held(timer);
        m.timing_io.arm_plc_timer(timer, armed);
    }
    /// Original RST T is a visited level on the SAME inhibit bitmap used by
    /// STMR. A false visit releases it; a skipped RST retains it. Timer-enable
    /// is deliberately preserved, even when a later TMR/STMR re-enables it.
    pub(crate) fn visit_reset(m: &mut Memory, timer: Device, power: bool) {
        let byte = usize::from(timer.index / 8);
        let mask = 1 << (timer.index & 7);
        if power {
            m.special_timers.hold[byte] |= mask;
            m.set_word(timer, 0);
            m.set_bit(timer, false);
        } else {
            m.special_timers.hold[byte] &= !mask;
        }
        let armed = m.special_timers.timer_runs(m, &timer);
        m.timing_io.arm_plc_timer(timer, armed);
    }
    /// The virtual DELAY profile clears its continuous coil on ZRST. This is
    /// separate from installed RST's level inhibit and scan-end ZRST service.
    pub(crate) fn reset_virtual_timer(m: &mut Memory, timer: Device) {
        m.special_timers.enabled.remove(&timer.index);
        m.timing_io.arm_plc_timer(timer, false);
        m.set_word(timer, 0);
        m.set_bit(timer, false);
    }
    /// END/FEND service; never call for a VM budget fault or each FOR iteration.
    /// In ordered mode timing_io has already advanced eligible WORDs, so there
    /// is no second elapsed addition here.
    pub(crate) fn end_scan(m: &mut Memory, start_us: u64, end_us: u64, ordered: bool) {
        let mut state = std::mem::take(&mut m.special_timers);
        for index in 0..256 {
            let timer = at(Area::T, index);
            if !state.timer_runs(m, &timer) {
                continue;
            }
            if !ordered {
                let base = timer_base_us(m, index);
                let ticks = end_us / base - start_us / base;
                m.set_word(timer, m.word(timer).wrapping_add(ticks as i16));
            }
            if (184..=199).contains(&index) {
                let preset = state.subroutine_presets[usize::from(index & 15)];
                if m.word(timer) >= preset {
                    m.set_word(timer, preset);
                    m.set_bit(timer, true);
                }
            } else if !ordered {
                state.enabled.remove(&index);
            }
        }
        // Installed scan-end service consumes ordinary enables. The explicitly
        // virtual DELAY profile instead retains powered coils across elapsed
        // instruction time, including a delay preceding the next TMR visit.
        for index in 0..256 {
            let timer = at(Area::T, index);
            let armed = state.timer_runs(m, &timer);
            m.timing_io.arm_plc_timer(timer, armed);
        }
        if state.teaching_profile == TeachingTickProfile::VirtualElapsed
            && state.teaching_count != 0
        {
            let ticks = end_us / 100_000 - start_us / 100_000;
            state.supply_teaching_ticks(ticks as u8);
        }
        let pending = std::mem::take(&mut state.teaching_pending);
        for slot in &mut state.teaching {
            if slot.owner != 0 {
                slot.ticks = slot.ticks.wrapping_add(i16::from(pending));
            }
        }
        m.special_timers = state;
    }
}

impl TimerAction {
    pub fn parse(op: &str, args: &[&str]) -> Result<Self, String> {
        let op = op.to_ascii_uppercase();
        match op.as_str() {
            "TMR" | "ATMR" if args.len() == 2 => {
                let timer: Device = args[0].parse()?;
                if timer.area != Area::T {
                    return Err("Timer target must be T0..255".into());
                }
                Ok(Self::Timer {
                    timer,
                    preset: TimerPreset::parse(args[1], op == "ATMR")?,
                    contact: op == "ATMR",
                })
            }
            "TTMR" if args.len() == 2 => {
                let dest: Device = args[0].parse()?;
                if dest.area != Area::D || dest.index >= 9999 {
                    return Err("TTMR requires a two-word D destination".into());
                }
                let scale = constant(args[1])
                    .filter(|n| *n <= 2)
                    .ok_or("TTMR scale must be K/H 0..2")? as u8;
                Ok(Self::Teach { dest, scale })
            }
            "STMR" if args.len() == 3 => {
                let timer: Device = args[0].parse()?;
                if timer.area != Area::T || timer.index > 183 {
                    return Err("STMR timer must be T0..183".into());
                }
                let preset = constant(args[1])
                    .filter(|n| (1..=32767).contains(n))
                    .ok_or("STMR preset must be K/H 1..32767")?;
                let dest: Device = args[2].parse()?;
                let max = match dest.area {
                    Area::Y => 255,
                    Area::M => 4095,
                    Area::S => 1023,
                    _ => return Err("STMR output must be Y/M/S".into()),
                };
                if dest.index > max - 3 {
                    return Err("STMR four-bit destination exceeds its bank".into());
                }
                Ok(Self::Special {
                    timer,
                    preset,
                    dest,
                })
            }
            _ => Err(format!("Invalid timer instruction/arity: {op}")),
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Timer { contact: true, .. } => "ATMR",
            Self::Timer { .. } => "TMR",
            Self::Teach { .. } => "TTMR",
            Self::Special { .. } => "STMR",
        }
    }
    pub fn header(&self) -> u16 {
        match self {
            Self::Timer { contact: true, .. } => 0x1c02,
            Self::Timer { .. } => 0x0300,
            Self::Teach { .. } => 0x9000,
            Self::Special { .. } => 0x9200,
        }
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::Timer { timer, .. } => *timer,
            Self::Teach { dest, .. } | Self::Special { dest, .. } => *dest,
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::Timer { timer, preset, .. } => format!("{} {timer} {preset}", self.name()),
            Self::Teach { dest, scale } => format!("TTMR {dest} K{scale}"),
            Self::Special {
                timer,
                preset,
                dest,
            } => format!("STMR {timer} K{preset} {dest}"),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        match self {
            Self::Timer { timer, preset, .. } => {
                out.insert(*timer);
                if let TimerPreset::Register(d) = preset {
                    out.insert(*d);
                }
            }
            Self::Teach { dest, .. } => {
                out.extend([*dest, offset(*dest, 1)]);
            }
            Self::Special { timer, dest, .. } => {
                out.insert(*timer);
                out.extend((0..4).map(|n| special_output(*dest, n)));
            }
        }
    }
    pub fn observed_devices(&self, _m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
    }
    pub fn validate_native_span(&self, m: &Memory) -> Result<(), String> {
        let limit = m.data_register_limit.unwrap_or(9999);
        match self {
            Self::Timer {
                preset: TimerPreset::Register(d),
                ..
            } if d.index > limit => Err("Timer preset outside model D bank".into()),
            Self::Teach { dest, .. } if dest.index >= limit => {
                Err("TTMR two-word destination outside model D bank".into())
            }
            Self::Special { dest, .. }
                if dest.area == Area::M && dest.index + 3 > m.relay_limit.unwrap_or(4095) =>
            {
                Err("STMR four-bit output outside model M bank".into())
            }
            _ => Ok(()),
        }
    }
    pub fn execute(&self, m: &mut Memory, power: bool, pc: u16) -> Option<bool> {
        if let Err(reason) = self.validate_native_span(m) {
            limitation(m, &reason);
            return matches!(self, Self::Timer { contact: true, .. }).then_some(false);
        }
        let mut state = std::mem::take(&mut m.special_timers);
        let result = match self {
            Self::Timer {
                timer,
                preset,
                contact,
            } => {
                if power {
                    let preset = preset.read(m);
                    if (184..=199).contains(&timer.index) {
                        state.subroutine_presets[usize::from(timer.index & 15)] = preset;
                    } else {
                        let done = m.word(*timer) >= preset;
                        if done {
                            m.set_word(*timer, preset);
                        }
                        m.set_bit(*timer, done);
                    }
                    state.enable(m, *timer, true);
                } else {
                    state.enable(m, *timer, false);
                    if timer.index < 240 {
                        m.set_word(*timer, 0);
                        m.set_bit(*timer, false);
                    }
                }
                contact.then_some(power && m.bit(*timer))
            }
            Self::Teach { dest, scale } => {
                state.teach(m, *dest, *scale, power, pc);
                None
            }
            Self::Special {
                timer,
                preset,
                dest,
            } => {
                state.special(m, *timer, *preset as i16, *dest, power, pc);
                None
            }
        };
        m.special_timers = state;
        result
    }
}
fn constant(s: &str) -> Option<u16> {
    let s = s.to_ascii_uppercase();
    if let Some(n) = s.strip_prefix('K') {
        n.parse().ok()
    } else {
        s.strip_prefix('H')
            .and_then(|n| u16::from_str_radix(n, 16).ok())
    }
}

impl TimerState {
    fn teach(&mut self, m: &mut Memory, dest: Device, scale: u8, power: bool, pc: u16) {
        if power && !self.teaching_active.contains(&pc) {
            if pc == 0 {
                limitation(
                    m,
                    "TTMR header PC0 aliases the installed free-slot sentinel; execution requires a proven nonzero PC",
                );
                return;
            }
            let Some(slot) = self.teaching.iter_mut().find(|slot| slot.owner == 0) else {
                return;
            };
            *slot = TeachingSlot {
                owner: pc,
                ticks: 0,
            };
            self.teaching_active.insert(pc);
            if self.teaching_count == 0 {
                self.teaching_pending = 0;
            }
            self.teaching_count = self.teaching_count.wrapping_add(1);
            m.set_word(dest, 0);
            m.set_word(offset(dest, 1), 0);
        } else if self.teaching_active.contains(&pc) {
            let Some(slot) = self.teaching.iter_mut().find(|slot| slot.owner == pc) else {
                limitation(
                    m,
                    "TTMR activation without its installed owner is unresolved",
                );
                return;
            };
            if power {
                m.native_numeric_scratch = Some(dest.index as i16);
                let value = match scale {
                    0 => slot.ticks / 10,
                    1 => slot.ticks,
                    _ => slot.ticks.wrapping_mul(10),
                };
                m.set_word(dest, value);
                m.set_word(offset(dest, 1), slot.ticks);
            } else {
                self.teaching_active.remove(&pc);
                slot.owner = 0;
                self.teaching_count = self.teaching_count.saturating_sub(1);
                m.set_word(offset(dest, 1), 0);
            }
        }
    }
    fn special(
        &mut self,
        m: &mut Memory,
        timer: Device,
        preset: i16,
        dest: Device,
        power: bool,
        pc: u16,
    ) {
        let phase = self.special.get(&pc).copied();
        if !power && phase.is_none() {
            return;
        }
        m.native_numeric_scratch = Some(timer.index as i16);
        if power && (phase.is_none() || phase == Some(true)) {
            self.special.insert(pc, false);
            m.set_word(timer, 0);
            self.enable(m, timer, true);
            for (n, value) in [true, false, true, false].into_iter().enumerate() {
                m.set_bit(special_output(dest, n as u16), value);
            }
        } else if power {
            self.enable(m, timer, true);
            if m.word(timer) >= preset {
                let mask = match self.special_profile {
                    SpecialHoldProfile::DocumentedBit => 1 << (timer.index & 7),
                    SpecialHoldProfile::InstalledTableCandidate => {
                        INSTALLED_HOLD_TABLE[usize::from(timer.index)]
                    }
                };
                self.hold[usize::from(timer.index / 8)] |= mask;
                self.enable(m, timer, false);
                m.set_bit(special_output(dest, 2), false);
                m.set_bit(special_output(dest, 3), true);
            }
        } else {
            self.enable(m, timer, true);
            if phase == Some(false) {
                self.special.insert(pc, true);
                self.hold[usize::from(timer.index / 8)] &= !(1 << (timer.index & 7));
                m.set_word(timer, 0);
                // Enable again after clearing the inhibit bit.
                self.enable(m, timer, true);
                m.set_bit(special_output(dest, 1), true);
                m.set_bit(special_output(dest, 2), false);
                m.set_bit(special_output(dest, 3), true);
            } else if m.word(timer) >= preset {
                self.special.remove(&pc);
                self.enable(m, timer, false);
                m.set_word(timer, 0);
                for n in 0..4 {
                    m.set_bit(special_output(dest, n), false);
                }
            }
        }
    }
}

// Original ES2/SS2 bytes VA521C2C, file offset1183276, length184,
// SHA256 c4841642eca9b2285401b08a5db33c532839b3c41d76a54d80743d964747ea71.
// The out-of-bit-table indexing is intentional and explicitly profiled.
const INSTALLED_HOLD_TABLE: [u8; 184] = hold_table();
const fn hold_table() -> [u8; 184] {
    let hex = b"0102040810204080fefdfbf7efdfbf7f0100020004000800100020004000800000010002000400080010002000400080fefffdfffbfff7ffefffdfffbfff7ffffffefffdfffbfff7ffefffdfffbfff7f0100000002000000040000000800000010000000200000004000000080000000000100000002000000040000000800000010000000200000004000000080000000000100000002000000040000000800000010000000200000004000000080000000000100000002";
    let mut result = [0; 184];
    let mut i = 0;
    while i < 184 {
        result[i] = (nibble(hex[2 * i]) << 4) | nibble(hex[2 * i + 1]);
        i += 1;
    }
    result
}
const fn nibble(c: u8) -> u8 {
    if c <= b'9' { c - b'0' } else { c - b'a' + 10 }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn action(s: &str) -> TimerAction {
        let v: Vec<_> = s.split_whitespace().collect();
        TimerAction::parse(v[0], &v[1..]).unwrap()
    }
    fn end(m: &mut Memory, n: u64) {
        TimerState::end_scan(m, (n - 1) * 100_000, n * 100_000, false);
    }
    #[test]
    fn parse_matrices_and_real_headers() {
        for s in [
            "TMR T255 K0",
            "ATMR T128 H10",
            "TTMR D9998 H2",
            "STMR T183 K32767 Y374",
        ] {
            let a = action(s);
            assert_eq!(action(&a.label()), a);
        }
        for (op, args) in [
            ("TMR", vec!["T0", "H1"]),
            ("TTMR", vec!["T0", "K1"]),
            ("STMR", vec!["T184", "K1", "M0"]),
            ("TTMRP", vec!["D0", "K1"]),
            ("TTMR", vec!["D9999", "K1"]),
        ] {
            assert!(TimerAction::parse(op, &args).is_err());
        }
        assert_eq!(action("ATMR T0 K1").header(), 0x1c02);
    }
    #[test]
    fn ordinary_done_is_next_visit_and_skipped_stops() {
        let mut m = Memory::default();
        let a = action("TMR T0 K1");
        let t = a.dest();
        a.execute(&mut m, true, 1);
        end(&mut m, 1);
        assert_eq!(m.word(t), 1);
        assert!(!m.bit(t));
        end(&mut m, 2);
        assert_eq!(m.word(t), 1);
        a.execute(&mut m, true, 1);
        assert!(m.bit(t));
        a.execute(&mut m, false, 1);
        assert_eq!(m.word(t), 0);
        assert!(!m.bit(t));
    }
    #[test]
    fn subroutine_completes_while_skipped_and_accumulative_false_retains() {
        let mut m = Memory::default();
        let a = action("TMR T184 K2");
        a.execute(&mut m, true, 1);
        end(&mut m, 1);
        end(&mut m, 2);
        assert_eq!(m.word(a.dest()), 2);
        assert!(m.bit(a.dest()));
        let a = action("ATMR T250 K1");
        assert_eq!(a.execute(&mut m, true, 9), Some(false));
        end(&mut m, 3);
        assert_eq!(a.execute(&mut m, true, 9), Some(true));
        assert_eq!(a.execute(&mut m, false, 9), Some(false));
        assert_eq!(m.word(a.dest()), 1);
        assert!(m.bit(a.dest()));
    }
    #[test]
    fn all_bases_controls_and_signed_live_preset() {
        let mut m = Memory::default();
        for (n, b) in [
            (63, 100_000),
            (64, 100_000),
            (127, 1_000),
            (183, 100_000),
            (199, 100_000),
            (200, 10_000),
            (245, 10_000),
            (246, 1_000),
            (249, 1_000),
            (250, 100_000),
        ] {
            assert_eq!(timer_base_us(&m, n), b);
        }
        m.set_system(1028, true);
        m.set_system(1038, true);
        assert_eq!(timer_base_us(&m, 64), 10_000);
        assert_eq!(timer_base_us(&m, 245), 1_000);
        assert_eq!(timer_base_us(&m, 250), 100_000);
        m.set_word(at(Area::D, 0), -2);
        let a = action("ATMR T200 D0");
        assert_eq!(a.execute(&mut m, true, 3), Some(true));
        assert_eq!(m.word(a.dest()), -2);
    }
    #[test]
    fn controlled_teaching_ticks_publish_on_next_visit_and_false_holds_output() {
        let mut m = Memory::default();
        m.special_timers.set_teaching_tick_profile(true);
        let a = action("TTMR D0 K2");
        a.execute(&mut m, true, 1);
        end(&mut m, 1);
        a.execute(&mut m, true, 1);
        assert_eq!(m.word(a.dest()), 0);
        m.special_timers.supply_teaching_ticks(3);
        end(&mut m, 2);
        assert_eq!(m.word(a.dest()), 0);
        a.execute(&mut m, true, 1);
        assert_eq!(m.word(a.dest()), 30);
        assert_eq!(m.word(at(Area::D, 1)), 3);
        assert_eq!(m.native_numeric_scratch, Some(0));
        a.execute(&mut m, false, 1);
        assert_eq!(m.word(a.dest()), 30);
        assert_eq!(m.word(at(Area::D, 1)), 0);
    }
    #[test]
    fn teaching_eight_owners_skipped_ticks_rearm_and_byte_wrap() {
        let mut m = Memory::default();
        m.special_timers.set_teaching_tick_profile(true);
        for n in 0..9 {
            action(&format!("TTMR D{} K1", n * 2)).execute(&mut m, true, n + 1);
        }
        assert_eq!(m.special_timers.teaching_owners(), [1, 2, 3, 4, 5, 6, 7, 8]);
        m.special_timers.supply_teaching_ticks(255);
        m.special_timers.supply_teaching_ticks(2);
        end(&mut m, 1);
        assert_eq!(m.special_timers.teaching_counters(), [1; 8]);
        action("TTMR D0 K1").execute(&mut m, false, 1);
        action("TTMR D16 K1").execute(&mut m, true, 9);
        assert_eq!(m.special_timers.teaching_owners()[0], 9);
        assert_eq!(m.special_timers.teaching_counters()[0], 0);
    }
    #[test]
    fn teaching_signed_counter_and_scale_wrap() {
        let mut m = Memory::default();
        let a = action("TTMR D0 K0");
        a.execute(&mut m, true, 1);
        m.special_timers.teaching[0].ticks = -11;
        a.execute(&mut m, true, 1);
        assert_eq!(m.word(a.dest()), -1);
        let a = action("TTMR D2 K2");
        a.execute(&mut m, true, 2);
        m.special_timers.teaching[1].ticks = 32767;
        a.execute(&mut m, true, 2);
        assert_eq!(m.word(a.dest()), -10);
    }
    #[test]
    fn special_two_phases_and_four_output_rollover() {
        let mut m = Memory::default();
        let a = action("STMR T0 K1 Y7");
        let d = a.dest();
        a.execute(&mut m, false, 1);
        assert!(!m.bit(d));
        a.execute(&mut m, true, 1);
        assert!(m.bit(d));
        assert!(m.bit(offset(d, 2)));
        end(&mut m, 1);
        a.execute(&mut m, true, 1);
        assert!(m.special_timers.is_held(at(Area::T, 0)));
        assert!(!m.bit(offset(d, 2)));
        assert!(m.bit(offset(d, 3)));
        a.execute(&mut m, false, 1);
        assert!(m.bit(offset(d, 1)));
        end(&mut m, 2);
        a.execute(&mut m, false, 1);
        for n in 0..4 {
            assert!(!m.bit(offset(d, n)));
        }
    }
    #[test]
    fn installed_hold_table_is_explicit_and_can_inhibit_neighbours() {
        let mut m = Memory::default();
        m.special_timers
            .set_special_hold_profile(SpecialHoldProfile::InstalledTableCandidate);
        let a = action("STMR T8 K1 M0");
        a.execute(&mut m, true, 1);
        end(&mut m, 1);
        a.execute(&mut m, true, 1);
        assert!(!m.special_timers.is_held(at(Area::T, 8)));
        assert!(m.special_timers.is_held(at(Area::T, 9)));
        assert_eq!(INSTALLED_HOLD_TABLE[8], 0xfe);
    }
    #[test]
    fn model_bounds_and_pc_zero_are_explicit_limitations_without_plc_error() {
        let mut m = Memory::default();
        m.data_register_limit = Some(4999);
        action("TTMR D4999 K1").execute(&mut m, true, 1);
        assert!(!m.execution_limitations.is_empty());
        assert!(!m.system_bit(1067));
        m.execution_limitations.clear();
        action("TTMR D0 K1").execute(&mut m, true, 0);
        assert!(!m.execution_limitations.is_empty());
        assert_eq!(m.special_timers.teaching_owners(), [0; 8]);
    }
}
