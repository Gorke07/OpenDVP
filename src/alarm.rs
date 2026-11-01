//! ANS/ANR annunciators, with the installed timer-class and monitor ordering.
use crate::plc::{Area, Device, Memory};
use crate::special_timers::TimerState;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub enum AlarmAction {
    Set {
        timer: Device,
        preset: u16,
        alarm: Device,
    },
    /// ANRP uses this same action behind the controller's per-site pulse gate.
    Reset,
}
fn at(area: Area, index: u16) -> Device {
    Device { area, index }
}
impl AlarmAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        match name {
            "ANS" if args.len() == 3 => {
                let timer: Device = args[0].parse()?;
                if timer.area != Area::T || timer.index > 183 {
                    return Err("ANS zamanlayıcısı T0..T183 olmalı".into());
                }
                let preset = args[1]
                    .strip_prefix('K')
                    .and_then(|v| v.parse::<u16>().ok())
                    .filter(|v| (1..=32767).contains(v))
                    .ok_or("ANS süre sabiti K1..K32767 olmalı; H/register desteklenmiyor")?;
                let alarm: Device = args[2].parse()?;
                if alarm.area != Area::S || !(912..=1023).contains(&alarm.index) {
                    return Err("ANS alarmı S912..S1023 olmalı".into());
                }
                Ok(Self::Set {
                    timer,
                    preset,
                    alarm,
                })
            }
            "ANR" if args.is_empty() => Ok(Self::Reset),
            _ => Err(format!(
                "{name}: ANS üç operand, ANR sıfır operand gerektirir; ANSP/geniş biçimler desteklenmiyor"
            )),
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Set { .. } => "ANS",
            Self::Reset => "ANR",
        }
    }
    pub fn header(&self) -> u16 {
        match self {
            Self::Set { .. } => 0x6c00,
            Self::Reset => 0x6e00,
        }
    }
    /// ANR has no encoded destination; M1048 is its UI/status anchor.
    pub fn dest(&self) -> Device {
        match self {
            Self::Set { alarm, .. } => *alarm,
            Self::Reset => Memory::system_device(1048),
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::Set {
                timer,
                preset,
                alarm,
            } => format!("ANS {timer} K{preset} {alarm}"),
            Self::Reset => "ANR".into(),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        out.extend((912..=1023).map(|n| at(Area::S, n)));
        out.extend([
            Memory::system_device(1048),
            Memory::system_device(1049),
            at(Area::D, 1049),
        ]);
        if let Self::Set { timer, .. } = self {
            out.insert(*timer);
            out.insert(Memory::system_device(1028));
        }
    }
    pub fn observed_devices(&self, _m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
    }
    /// Call for a genuinely visited handler, including normal false visits.
    /// ANRP's held-active and first false-after-active dispatch SKIP this handler
    /// entirely. Calling execute(false) for those skipped visits is incorrect,
    /// because even a false ANR visit refreshes the global alarm monitor.
    pub fn execute(&self, m: &mut Memory, power: bool, _pc: u16) {
        match self {
            Self::Set {
                timer,
                preset,
                alarm,
            } => {
                if timer.area != Area::T
                    || timer.index > 183
                    || !(1..=32767).contains(preset)
                    || alarm.area != Area::S
                    || !(912..=1023).contains(&alarm.index)
                {
                    m.execution_limitations
                        .insert("ANS: doğrulanmamış operand aralığı".into());
                    return;
                }
                m.native_numeric_scratch = Some(timer.index as i16);
                if TimerState::visit_alarm(m, *timer, *preset, power) {
                    m.set_bit(*alarm, true);
                    m.native_numeric_scratch = Some(alarm.index as i16);
                } else if !power {
                    // Native false path leaves the contact BYTE offset in its
                    // shared numeric scratch (0xC0 is T0's byte within raw S).
                    m.native_numeric_scratch = Some((0xc0 + timer.index / 8) as i16);
                }
            }
            Self::Reset => {
                if power
                    && let Some(alarm) = (912..=1023).map(|n| at(Area::S, n)).find(|d| m.bit(*d))
                {
                    m.set_bit(alarm, false);
                }
            }
        }
        Self::refresh_monitor(m);
    }
    /// Exact visited-instruction service, not an END service. Direct SET/RST of
    /// an alarm bit does not invoke it automatically in the installed profile.
    pub fn refresh_monitor(m: &mut Memory) {
        if !m.system_bit(1049) {
            return;
        }
        let lowest = (912..=1023).find(|&n| m.bit(at(Area::S, n)));
        m.set_system(1048, lowest.is_some());
        if let Some(n) = lowest {
            m.set_word(at(Area::D, 1049), n as i16);
        }
        // No active alarm: D1049 holds its last value, rather than an invented
        // zero/-1 sentinel. Monitor disabled: BOTH outputs remain unchanged.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn set(timer: u16, preset: u16, alarm: u16) -> AlarmAction {
        AlarmAction::Set {
            timer: at(Area::T, timer),
            preset,
            alarm: at(Area::S, alarm),
        }
    }
    #[test]
    fn forms_and_complete_operand_matrix_are_strict() {
        assert_eq!(
            AlarmAction::parse("ANS", &["T183".into(), "K32767".into(), "S1023".into()])
                .unwrap()
                .header(),
            0x6c00
        );
        assert_eq!(AlarmAction::parse("ANR", &[]).unwrap().header(), 0x6e00);
        for args in [
            ["T184", "K1", "S912"],
            ["T0", "H1", "S912"],
            ["T0", "D0", "S912"],
            ["T0", "K0", "S912"],
            ["T0", "K32768", "S912"],
            ["T0", "K1", "S911"],
            ["T0", "K1", "S1024"],
            ["D0", "K1", "S912"],
        ] {
            assert!(
                AlarmAction::parse("ANS", &args.map(String::from)).is_err(),
                "{args:?}"
            );
        }
        for name in ["ANSP", "DANS", "DANSP", "DANR", "DANRP"] {
            assert!(AlarmAction::parse(name, &[]).is_err());
        }
        assert!(AlarmAction::parse("ANR", &["S912".into()]).is_err());
    }
    #[test]
    fn timed_set_latches_alarm_and_false_clears_timer_without_clearing_enable() {
        let mut m = Memory::default();
        let a = set(0, 5, 912);
        m.set_word(at(Area::T, 0), 5);
        a.execute(&mut m, true, 10);
        assert!(m.bit(at(Area::S, 912)));
        assert!(m.bit(at(Area::T, 0)));
        assert!(m.special_timers.is_enabled(at(Area::T, 0)));
        a.execute(&mut m, false, 10);
        assert!(m.bit(at(Area::S, 912)));
        assert!(!m.bit(at(Area::T, 0)));
        assert_eq!(m.word(at(Area::T, 0)), 0);
        assert!(m.special_timers.is_enabled(at(Area::T, 0)));
        assert_eq!(m.native_numeric_scratch, Some(0xc0));
        TimerState::end_scan(&mut m, 0, 100_000, false);
        assert_eq!(m.word(at(Area::T, 0)), 1);
        assert!(!m.special_timers.is_enabled(at(Area::T, 0)));
    }
    #[test]
    fn incomplete_active_set_preserves_old_done_contact_and_alarm_latch() {
        let mut m = Memory::default();
        m.set_bit(at(Area::T, 10), true);
        m.set_word(at(Area::T, 10), 1);
        m.set_bit(at(Area::S, 999), true);
        set(10, 20, 999).execute(&mut m, true, 1);
        assert!(m.bit(at(Area::T, 10)));
        assert!(m.bit(at(Area::S, 999)));
        assert_eq!(m.word(at(Area::T, 10)), 1);
        assert_eq!(m.native_numeric_scratch, Some(10));
    }
    #[test]
    fn reset_removes_exactly_lowest_live_alarm_independently_of_monitor() {
        let mut m = Memory::default();
        for n in [1023, 920, 912] {
            m.set_bit(at(Area::S, n), true);
        }
        m.set_system(1048, true);
        m.set_word(at(Area::D, 1049), 999);
        AlarmAction::Reset.execute(&mut m, true, 1);
        assert!(!m.bit(at(Area::S, 912)));
        assert!(m.bit(at(Area::S, 920)));
        assert!(m.system_bit(1048));
        assert_eq!(m.word(at(Area::D, 1049)), 999);
        AlarmAction::Reset.execute(&mut m, true, 1);
        assert!(!m.bit(at(Area::S, 920)));
        assert!(m.bit(at(Area::S, 1023)));
        AlarmAction::Reset.execute(&mut m, false, 1);
        assert!(m.bit(at(Area::S, 1023)));
    }
    #[test]
    fn monitor_requires_visit_and_enable_clears_only_status_when_empty() {
        let mut m = Memory::default();
        m.set_bit(at(Area::S, 1023), true);
        m.set_system(1049, true);
        assert!(!m.system_bit(1048));
        assert_eq!(m.word(at(Area::D, 1049)), 0);
        AlarmAction::Reset.execute(&mut m, false, 1);
        assert!(m.system_bit(1048));
        assert_eq!(m.word(at(Area::D, 1049)), 1023);
        AlarmAction::Reset.execute(&mut m, true, 1);
        assert!(!m.system_bit(1048));
        assert_eq!(m.word(at(Area::D, 1049)), 1023);
        m.set_system(1049, false);
        m.set_system(1048, true);
        m.set_word(at(Area::D, 1049), 777);
        AlarmAction::Reset.execute(&mut m, false, 1);
        assert!(m.system_bit(1048));
        assert_eq!(m.word(at(Area::D, 1049)), 777);
    }
    #[test]
    fn ordinary_timer_classes_and_global_prescaler_are_not_overridden() {
        for (timer, fine, expected) in [
            (0, false, 1),
            (64, false, 1),
            (64, true, 10),
            (126, true, 10),
            (127, false, 100),
            (128, false, 1),
            (183, false, 1),
        ] {
            let mut m = Memory::default();
            m.set_system(1028, fine);
            set(timer, 32767, 912).execute(&mut m, true, 1);
            TimerState::end_scan(&mut m, 0, 100_000, false);
            assert_eq!(m.word(at(Area::T, timer)), expected, "T{timer},M1028{fine}");
            assert!(!m.bit(at(Area::S, 912)));
        }
    }
    #[test]
    fn source_order_controls_reassertion_and_sfc_shared_status_writes() {
        let mut m = Memory::default();
        m.set_system(1049, true);
        m.set_word(at(Area::T, 0), 5);
        let a = set(0, 5, 912);
        a.execute(&mut m, true, 1);
        AlarmAction::Reset.execute(&mut m, true, 8);
        assert!(!m.bit(at(Area::S, 912)));
        assert!(!m.system_bit(1048));
        a.execute(&mut m, true, 1);
        assert!(m.bit(at(Area::S, 912)));
        assert!(m.system_bit(1048));
        m.set_bit(at(Area::S, 912), false);
        m.set_system(1048, true);
        AlarmAction::Reset.execute(&mut m, false, 8);
        assert!(!m.system_bit(1048));
        m.set_system(1048, true);
        assert!(m.system_bit(1048));
    }
    #[test]
    fn all_alarm_bits_are_observed_even_without_local_ans_owners() {
        let mut out = BTreeSet::new();
        AlarmAction::Reset.devices(&mut out);
        assert_eq!(out.iter().filter(|d| d.area == Area::S).count(), 112);
        assert!(out.contains(&at(Area::S, 912)) && out.contains(&at(Area::S, 1023)));
        assert!(out.contains(&at(Area::D, 1049)));
        assert!(out.contains(&Memory::system_device(1049)));
    }
}
