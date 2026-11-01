//! Snapshot-based SFC semantics recovered from the installed ES2/SS2 handlers.
use crate::plc::{Area, Device, Memory};
use crate::program_control::ControlState;
use std::collections::BTreeSet;

fn d(area: Area, index: u16) -> Device {
    Device { area, index }
}
fn s(index: u16) -> Device {
    d(Area::S, index)
}
fn flag(m: &Memory, index: u16) -> bool {
    m.bit(d(Area::M, index))
}
fn set_flag(m: &mut Memory, index: u16, value: bool) {
    m.set_bit(d(Area::M, index), value);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SfcProfile {
    /// Uses all 1024 documented S bits for the monitor; transitions/IST preserve
    /// the recovered installed ordering, including documented discrepancies.
    #[default]
    Documented,
    /// Installed old simulator monitor stops at S895; snapshots still cover1024.
    InstalledLegacy,
}
#[derive(Debug, Clone)]
pub struct SfcState {
    pub profile: SfcProfile,
    pub current: [bool; 1024],
    pub previous: [bool; 1024],
    /// Native list holds at most eight predecessors. Its order is significant.
    pub predecessors: Vec<Device>,
    /// Recovered byte486. Bit02 has no proven setter: only explicit caller
    /// injection may enable classification; executing IST never invents it.
    pub internal_flags: u8,
    /// Installed M1034 service saves the first inhibited output image once;
    /// its falling edge restores that image, discarding held-period writes.
    output_backup: Option<[bool; 256]>,
}
impl Default for SfcState {
    fn default() -> Self {
        Self {
            profile: SfcProfile::Documented,
            current: [false; 1024],
            previous: [false; 1024],
            predecessors: Vec::new(),
            internal_flags: 0,
            output_backup: None,
        }
    }
}
impl SfcState {
    pub(crate) fn service_output_inhibit(m: &mut Memory) {
        if flag(m, 1034) {
            if m.sfc.output_backup.is_none() {
                m.sfc.output_backup = Some(std::array::from_fn(|i| m.bit(d(Area::Y, i as u16))));
            }
            for i in 0..256 {
                m.set_bit(d(Area::Y, i), false);
            }
        } else if let Some(saved) = m.sfc.output_backup.take() {
            for (i, value) in saved.into_iter().enumerate() {
                m.set_bit(d(Area::Y, i as u16), value);
            }
        }
    }
    /// Adjacent means the immediately preceding encoded WORD is another STL.
    /// It must be computed by the immutable lowerer, not dynamic visit history.
    pub fn enter(m: &mut Memory, control: &mut ControlState, step: Device, adjacent: bool) {
        if step.area != Area::S || step.index > 1023 {
            m.execution_limitations
                .insert("STL: adım S0..S1023 olmalı".into());
            return;
        }
        let mut state = std::mem::take(&mut m.sfc);
        control.step_context_active = true;
        if adjacent {
            control.out_enabled = state.internal_flags & 1 != 0;
        } else {
            control.out_enabled = true;
            state.internal_flags &= !1;
            state.predecessors.clear();
        }
        if !control.out_enabled {
            control.logic_word &= !1;
            m.sfc = state;
            return;
        }
        if state.internal_flags & 2 != 0 && m.bit(step) {
            state.internal_flags |= if (10..20).contains(&step.index) { 4 } else { 8 };
        }
        if state.current[usize::from(step.index)] {
            control.logic_word |= 1;
            control.master_gate |= 1;
            let capacity = if state.profile == SfcProfile::Documented {
                16
            } else {
                8
            };
            if state.predecessors.len() < capacity {
                state.predecessors.push(step);
            } else if state.profile == SfcProfile::InstalledLegacy {
                set_flag(m, 1048, true);
            } else {
                m.execution_limitations
                    .insert("STL: belgeli 16 öncül sınırı aşıldı".into());
            }
        } else {
            if !state.previous[usize::from(step.index)] {
                control.out_enabled = false;
            }
            control.logic_word &= !1;
            control.master_gate &= !1;
        }
        state.internal_flags = (state.internal_flags & !1) | u8::from(control.out_enabled);
        m.sfc = state;
    }
    /// Returns true for S outputs handled here. Caller supplies effective power.
    pub fn transition(
        m: &mut Memory,
        control: &mut ControlState,
        target: Device,
        is_set: bool,
        power: bool,
    ) -> bool {
        if target.area != Area::S {
            return false;
        }
        if target.index > 1023 {
            m.execution_limitations
                .insert("SFC: hedef S0..S1023 olmalı".into());
            return true;
        }
        if !is_set && !control.out_enabled {
            return true;
        }
        let mut state = std::mem::take(&mut m.sfc);
        if power {
            if is_set {
                if !flag(m, 1040) {
                    m.set_bit(target, true);
                    while let Some(step) = state.predecessors.pop() {
                        m.set_bit(step, false);
                    }
                }
            } else {
                while let Some(step) = state.predecessors.pop() {
                    m.set_bit(step, false);
                }
                if !flag(m, 1040) {
                    m.set_bit(target, true);
                }
            }
        } else if !is_set && !control.step_context_active {
            m.set_bit(target, false);
        }
        m.sfc = state;
        true
    }
    /// Native RET does not clear flag485bit08 (step_context_active).
    pub fn ret(m: &mut Memory, control: &mut ControlState) {
        m.sfc.predecessors.clear();
        control.out_enabled = true;
        control.master_gate |= 1;
    }
    /// Exact installed order: live-step monitor, then previous/current rotation.
    pub fn end_scan(m: &mut Memory) {
        let mut state = std::mem::take(&mut m.sfc);
        if flag(m, 1047) {
            set_flag(m, 1046, false);
            let limit = if state.profile == SfcProfile::InstalledLegacy {
                896
            } else {
                1024
            };
            let mut slot = 0;
            for index in 0..limit {
                if m.bit(s(index)) {
                    set_flag(m, 1046, true);
                    m.set_word(d(Area::D, 1040 + slot), index as i16);
                    slot += 1;
                    if slot == 8 {
                        break;
                    }
                }
            }
        }
        state.previous = state.current;
        for (index, v) in state.current.iter_mut().enumerate() {
            *v = m.bit(s(index as u16));
        }
        m.sfc = state;
    }
}

/// Native IST reads eight physically consecutive bits, not eight logical M
/// names: M1535 is followed by C0, X377 by Y0, and Y377 by T0.
fn source_bits(source: Device) -> Result<[Device; 8], String> {
    if !matches!(source.area, Area::X | Area::Y | Area::M) {
        return Err("IST: kaynak X/Y/M olmalı".into());
    }
    let base = match source.area {
        Area::X if source.index <= 255 => 0x400 + u32::from(source.index),
        Area::Y if source.index <= 255 => 0x500 + u32::from(source.index),
        Area::M if source.index < 1536 => 0x800 + u32::from(source.index),
        Area::M if source.index <= 4095 => 0xb000 + u32::from(source.index) - 1536,
        _ => return Err("IST: kaynak adresi aralık dışında".into()),
    };
    let mut out = [source; 8];
    for (i, slot) in out.iter_mut().enumerate() {
        let address = base + i as u32;
        *slot = match address {
            0x400..=0x4ff => d(Area::X, (address - 0x400) as u16),
            0x500..=0x5ff => d(Area::Y, (address - 0x500) as u16),
            0x600..=0x6ff => d(Area::T, (address - 0x600) as u16),
            0x800..=0xdff => d(Area::M, (address - 0x800) as u16),
            0xe00..=0xeff => d(Area::C, (address - 0xe00) as u16),
            0xb000..=0xb9ff => d(Area::M, (address - 0xb000 + 1536) as u16),
            _ => return Err("IST: sekiz fiziksel kaynak bitinin tamamı modellenemiyor".into()),
        };
    }
    Ok(out)
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SfcAction {
    Ist {
        source: Device,
        first: Device,
        last: Device,
    },
}
impl SfcAction {
    pub fn parse(args: &[String]) -> Result<Self, String> {
        if args.len() != 3 {
            return Err("IST: üç operand gerekli".into());
        }
        let source = args[0].parse::<Device>()?;
        source_bits(source)?;
        let first = args[1].parse::<Device>()?;
        let last = args[2].parse::<Device>()?;
        if first.area != Area::S
            || last.area != Area::S
            || !(20..=911).contains(&first.index)
            || !(20..=911).contains(&last.index)
            || first.index >= last.index
        {
            return Err("IST: S20..S911 içinde artan iki adım sınırı gerekli".into());
        }
        Ok(Self::Ist {
            source,
            first,
            last,
        })
    }
    pub fn name(&self) -> &'static str {
        "IST"
    }
    pub fn header(&self) -> u16 {
        0x8800
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::Ist { first, .. } => *first,
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::Ist {
                source,
                first,
                last,
            } => format!("IST {source} {first} {last}"),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        let Self::Ist { source, .. } = self;
        if let Ok(bits) = source_bits(*source) {
            out.extend(bits);
        }
        out.extend((0..1024).map(s));
        out.extend((0..256).map(|i| d(Area::Y, i)));
        out.extend((1040..=1048).map(|i| d(Area::M, i)));
        out.insert(d(Area::M, 1002));
        out.extend((1040..=1047).map(|i| d(Area::D, i)));
    }
    pub fn observed_devices(&self, _m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
    }
    /// Must be visited with false effective power too, for pulse-history reset.
    pub fn execute(&self, m: &mut Memory, power: bool, _pc: u16) {
        let Self::Ist {
            source,
            first,
            last,
        } = *self;
        if !power {
            m.sfc.internal_flags &= !0x2c;
            return;
        }
        if first.area != Area::S
            || last.area != Area::S
            || first.index >= last.index
            || first.index < 20
            || last.index > 911
        {
            m.execution_limitations
                .insert("IST: güvenli adım sınırları S20..S911 ve ilk<son olmalı".into());
            return;
        }
        let bits = match source_bits(source) {
            Ok(v) => v,
            Err(e) => {
                m.execution_limitations.insert(e);
                return;
            }
        };
        if bits
            .iter()
            .any(|b| b.area == Area::M && b.index > m.relay_limit.unwrap_or(4095))
        {
            m.execution_limitations
                .insert("IST: kaynak modelin M alanını aşıyor".into());
            return;
        }
        let u = bits
            .into_iter()
            .enumerate()
            .fold(0u8, |v, (i, b)| v | (u8::from(m.bit(b)) << i));
        let mut state = std::mem::take(&mut m.sfc);
        let clear_y = |m: &mut Memory| {
            for i in 0..256 {
                m.set_bit(d(Area::Y, i), false);
            }
        };
        let clear_range = |state: &mut SfcState, m: &mut Memory| {
            for i in first.index..=last.index {
                m.set_bit(s(i), false);
                state.current[usize::from(i)] = false;
            }
        };
        if u & 1 != 0 {
            if state.internal_flags & 8 != 0 && !flag(m, 1045) {
                clear_y(m);
            }
            m.set_bit(s(0), true);
            m.set_bit(s(1), false);
            m.set_bit(s(2), false);
            state.current[1] = false;
            state.current[2] = false;
            let zero = flag(m, 1044);
            set_flag(m, 1043, zero);
            for i in 10..20 {
                m.set_bit(s(i), false);
            }
            clear_range(&mut state, m);
        } else if u & 2 != 0 {
            if state.internal_flags & 4 == 0 {
                if m.bit(s(0)) || state.internal_flags & 8 != 0 {
                    set_flag(m, 1043, false);
                    if !flag(m, 1045) {
                        clear_y(m);
                    }
                }
                m.set_bit(s(0), false);
                m.set_bit(s(1), true);
                m.set_bit(s(2), false);
                state.current[0] = false;
                state.current[2] = false;
                clear_range(&mut state, m);
            }
        } else if u & 0x1c != 0 {
            if m.bit(s(2)) {
                set_flag(m, 1043, false);
            } else if state.internal_flags & 8 == 0 && flag(m, 1043) {
                m.set_bit(s(0), false);
                m.set_bit(s(1), false);
                m.set_bit(s(2), true);
                if flag(m, 1041) {
                    set_flag(m, 1043, false);
                }
                for i in 10..18 {
                    m.set_bit(s(i), false);
                }
            }
        }
        let start = u & 0x80 == 0
            && (u & 0x40 != 0 && u & 0x1c != 0 || u & 0x10 != 0 && flag(m, 1041) && u & 0x1c != 0);
        set_flag(m, 1041, start);
        if u & 0x60 != 0 && u & 1 == 0 {
            let rising = state.internal_flags & 0x20 == 0;
            set_flag(m, 1042, rising);
            state.internal_flags |= 0x20;
        } else {
            state.internal_flags &= !0x20;
        }
        let inhibit = if u & 1 != 0 {
            true
        } else if u & 0x82 == 0x82
            || u & 4 != 0
            || u & 0x88 == 0x88
            || flag(m, 1002)
            || flag(m, 1040)
        {
            !flag(m, 1042)
        } else {
            false
        };
        set_flag(m, 1040, inhibit);
        set_flag(m, 1047, true);
        state.internal_flags &= !0x0c;
        m.sfc = state;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn action(source: Device) -> SfcAction {
        SfcAction::Ist {
            source,
            first: s(20),
            last: s(911),
        }
    }
    #[test]
    fn snapshots_delay_and_exit_cleanup_preserve_upper_logic() {
        let mut m = Memory::default();
        let mut c = ControlState::default();
        m.set_bit(s(20), true);
        c.logic_word = 0xa5a4;
        SfcState::enter(&mut m, &mut c, s(20), false);
        assert_eq!(c.logic_word, 0xa5a4);
        assert!(!c.out_enabled);
        SfcState::end_scan(&mut m);
        SfcState::enter(&mut m, &mut c, s(20), false);
        assert_eq!(c.logic_word, 0xa5a5);
        SfcState::transition(&mut m, &mut c, s(21), false, true);
        assert!(!m.bit(s(20)));
        assert!(m.bit(s(21)));
        assert!(!m.sfc.current[21]);
        SfcState::end_scan(&mut m);
        SfcState::enter(&mut m, &mut c, s(20), false);
        assert!(c.out_enabled);
        assert!(!c.effective_power());
        SfcState::end_scan(&mut m);
        SfcState::enter(&mut m, &mut c, s(20), false);
        assert!(!c.out_enabled);
    }
    #[test]
    fn set_and_out_self_alias_and_inhibit_differ() {
        for is_set in [false, true] {
            let mut m = Memory::default();
            let mut c = ControlState::default();
            m.sfc.predecessors.push(s(20));
            m.set_bit(s(20), true);
            SfcState::transition(&mut m, &mut c, s(20), is_set, true);
            assert_eq!(m.bit(s(20)), !is_set);
            m.sfc.predecessors.push(s(21));
            m.set_bit(s(21), true);
            set_flag(&mut m, 1040, true);
            SfcState::transition(&mut m, &mut c, s(22), is_set, true);
            assert_eq!(m.bit(s(21)), is_set);
            assert!(!m.bit(s(22)));
        }
    }
    #[test]
    fn predecessor_quota_ret_and_false_out_context() {
        let mut m = Memory::default();
        m.sfc.profile = SfcProfile::InstalledLegacy;
        let mut c = ControlState::default();
        m.sfc.current.fill(true);
        for i in 20..30 {
            SfcState::enter(&mut m, &mut c, s(i), i != 20);
        }
        assert_eq!(m.sfc.predecessors.len(), 8);
        assert!(flag(&m, 1048));
        c.logic_word = 0x8193;
        SfcState::ret(&mut m, &mut c);
        assert!(c.step_context_active);
        assert_eq!(c.logic_word, 0x8193);
        assert!(m.sfc.predecessors.is_empty());
        m.set_bit(s(30), true);
        SfcState::transition(&mut m, &mut c, s(30), false, false);
        assert!(m.bit(s(30)));
        c.step_context_active = false;
        SfcState::transition(&mut m, &mut c, s(30), false, false);
        assert!(!m.bit(s(30)));
    }
    #[test]
    fn monitor_profiles_tail_and_all_snapshot_bits() {
        let mut m = Memory::default();
        set_flag(&mut m, 1047, true);
        m.set_word(d(Area::D, 1041), 77);
        m.set_bit(s(911), true);
        m.sfc.profile = SfcProfile::InstalledLegacy;
        SfcState::end_scan(&mut m);
        assert!(!flag(&m, 1046));
        assert!(m.sfc.current[911]);
        m.sfc.profile = SfcProfile::Documented;
        SfcState::end_scan(&mut m);
        assert_eq!(m.word(d(Area::D, 1040)), 911);
        assert_eq!(m.word(d(Area::D, 1041)), 77);
    }
    #[test]
    fn ist_freezes_y_controls_and_retains_nonqualifying_start_pulse() {
        let mut m = Memory::default();
        m.sfc.internal_flags = 8;
        m.set_bit(d(Area::Y, 0), true);
        m.set_bit(d(Area::Y, 5), true);
        set_flag(&mut m, 1044, true);
        action(d(Area::Y, 0)).execute(&mut m, true, 0);
        assert!(m.bit(s(0)));
        assert!(flag(&m, 1043));
        assert!(flag(&m, 1040));
        assert!(!m.bit(d(Area::Y, 0)));
        m.set_input(d(Area::X, 5), true);
        action(d(Area::X, 0)).execute(&mut m, true, 0);
        assert!(flag(&m, 1042));
        m.set_input(d(Area::X, 5), false);
        action(d(Area::X, 0)).execute(&mut m, true, 0);
        assert!(flag(&m, 1042));
        m.set_input(d(Area::X, 5), true);
        action(d(Area::X, 0)).execute(&mut m, true, 0);
        assert!(flag(&m, 1042));
        action(d(Area::X, 0)).execute(&mut m, true, 0);
        assert!(!flag(&m, 1042));
    }
    #[test]
    fn physical_m_boundary_and_validation() {
        let bits = source_bits(d(Area::M, 1535)).unwrap();
        assert_eq!(bits[1], d(Area::C, 0));
        assert_eq!(bits[7], d(Area::C, 6));
        assert!(source_bits(d(Area::M, 4095)).is_err());
        assert!(SfcAction::parse(&["X0".into(), "S20".into(), "S20".into()]).is_err());
        let mut m = Memory::default();
        m.set_bit(d(Area::M, 1535), true);
        m.sfc.internal_flags = 0x2c;
        action(d(Area::M, 1535)).execute(&mut m, false, 0);
        assert_eq!(m.sfc.internal_flags, 0);
        assert!(!flag(&m, 1047));
    }
}
