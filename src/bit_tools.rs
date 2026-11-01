//! Absolute value, Gray code conversion and indexed writes to numeric words.
use crate::bit_index_operand::BitIndexValue;
use crate::calendar_operand::{CalendarDestination, CalendarValue};
use crate::plc::{Device, Memory};
use std::collections::BTreeSet;

pub(crate) fn absolute_result(value: i32, wide: bool) -> i32 {
    if wide {
        value.wrapping_abs()
    } else {
        i32::from((value as i16).wrapping_abs())
    }
}

pub(crate) fn gray_result(value: i32, decode: bool) -> Option<i32> {
    if value < 0 {
        return None;
    }
    let mut result = value as u32;
    if decode {
        for shift in [1, 2, 4, 8, 16] {
            result ^= result >> shift;
        }
    } else {
        result ^= result >> 1;
    }
    Some(result as i32)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BitWriteKind {
    Out,
    Set,
    Reset,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BitAction {
    Absolute {
        dest: CalendarDestination,
    },
    Gray {
        source: CalendarValue,
        dest: CalendarDestination,
        decode: bool,
    },
    IndexedWrite {
        kind: BitWriteKind,
        dest: CalendarDestination,
        index: BitIndexValue,
    },
}

impl BitAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let wide = name.starts_with('D');
        let expected = if matches!(name, "ABS" | "DABS") { 1 } else { 2 };
        if args.len() != expected {
            return Err(format!("{name}: {expected} operand gerekli"));
        }
        Ok(match name {
            "ABS" | "DABS" => {
                let dest = CalendarDestination::parse(&args[0], wide)?;
                Self::Absolute { dest }
            }
            "GRY" | "DGRY" | "GBIN" | "DGBIN" => {
                let dest = CalendarDestination::parse(&args[1], wide)?;
                Self::Gray {
                    source: CalendarValue::parse(&args[0], wide)?,
                    dest,
                    decode: matches!(name, "GBIN" | "DGBIN"),
                }
            }
            "BOUT" | "DBOUT" | "BSET" | "DBSET" | "BRST" | "DBRST" => {
                let dest = CalendarDestination::parse(&args[0], wide)?;
                // Indexed writers have a narrower destination matrix than
                // ABS/Gray: direct E/F are only permitted in operand n.
                if matches!(dest.value(), CalendarValue::Index { .. }) {
                    return Err("İndeksli bit hedefi KnY/KnM/KnS veya T/C/D olmalı".into());
                }
                let index = BitIndexValue::parse(&args[1], wide)?;
                let max = if wide { 31 } else { 15 };
                if index.constant().is_some_and(|n| !(0..=max).contains(&n)) {
                    return Err(format!("Bit numarası 0..{max} aralığında olmalı"));
                }
                Self::IndexedWrite {
                    kind: match name {
                        "BOUT" | "DBOUT" => BitWriteKind::Out,
                        "BSET" | "DBSET" => BitWriteKind::Set,
                        _ => BitWriteKind::Reset,
                    },
                    dest,
                    index,
                }
            }
            _ => return Err("Bilinmeyen bit işleme komutu".into()),
        })
    }
    pub fn dest(&self) -> Device {
        self.destination().base()
    }
    fn destination(&self) -> &CalendarDestination {
        match self {
            Self::Absolute { dest, .. }
            | Self::Gray { dest, .. }
            | Self::IndexedWrite { dest, .. } => dest,
        }
    }
    pub fn wide(&self) -> bool {
        self.destination().wide()
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Absolute { .. } => {
                if self.wide() {
                    "DABS"
                } else {
                    "ABS"
                }
            }
            Self::Gray { decode: false, .. } => {
                if self.wide() {
                    "DGRY"
                } else {
                    "GRY"
                }
            }
            Self::Gray { decode: true, .. } => {
                if self.wide() {
                    "DGBIN"
                } else {
                    "GBIN"
                }
            }
            Self::IndexedWrite {
                kind: BitWriteKind::Out,
                ..
            } => {
                if self.wide() {
                    "DBOUT"
                } else {
                    "BOUT"
                }
            }
            Self::IndexedWrite {
                kind: BitWriteKind::Set,
                ..
            } => {
                if self.wide() {
                    "DBSET"
                } else {
                    "BSET"
                }
            }
            Self::IndexedWrite {
                kind: BitWriteKind::Reset,
                ..
            } => {
                if self.wide() {
                    "DBRST"
                } else {
                    "BRST"
                }
            }
        }
    }
    pub fn header(&self) -> u16 {
        let word = match self {
            Self::Absolute { .. } => 0xbe00,
            Self::Gray { decode: false, .. } => 0x7201,
            Self::Gray { decode: true, .. } => 0x7401,
            Self::IndexedWrite {
                kind: BitWriteKind::Out,
                ..
            } => 0x3402,
            Self::IndexedWrite {
                kind: BitWriteKind::Set,
                ..
            } => 0x3602,
            Self::IndexedWrite {
                kind: BitWriteKind::Reset,
                ..
            } => 0x3802,
        };
        word + if self.wide() { 0x100 } else { 0 }
    }
    pub fn label(&self) -> String {
        let name = self.name();
        match self {
            Self::Absolute { dest, .. } => format!("{name} {dest}"),
            Self::Gray { source, dest, .. } => format!("{name} {source} {dest}"),
            Self::IndexedWrite { dest, index, .. } => format!("{name} {dest} {index}"),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        self.destination().devices(out);
        out.extend((1020..=1022).map(Memory::system_device));
        Memory::error_devices(out);
        match self {
            Self::Gray { source, .. } => source.devices(out),
            Self::IndexedWrite { index, .. } => index.devices(out),
            _ => {}
        }
    }
    /// BOUT is an output coil: it writes the selected bit on false power too.
    pub fn execute(&self, m: &mut Memory, power: bool) {
        if !power
            && !matches!(
                self,
                Self::IndexedWrite {
                    kind: BitWriteKind::Out,
                    ..
                }
            )
        {
            return;
        }
        let dest = self.destination();
        match self {
            Self::Absolute { .. } => {
                let value = dest.read(m);
                let result = absolute_result(value, self.wide());
                dest.write(m, result);
            }
            Self::Gray { source, decode, .. } => {
                let value = source.read(m);
                let Some(result) = gray_result(value, *decode) else {
                    m.operation_error(0xe1a, 1);
                    return;
                };
                dest.write(m, result);
            }
            Self::IndexedWrite { kind, index, .. } => {
                // Snapshot both reads before a destination alias can change
                // either value. Native operates on the zero-extended group
                // value, then truncates its write to that group's low bits.
                // Native dynamic indices wrap through the operand's low 4/5
                // bits, including negative indices, without a range error.
                let old = dest.read(m) as u32;
                let index = index.read(m) as u32 & if self.wide() { 31 } else { 15 };
                let mask = 1u32 << index;
                let on = match kind {
                    BitWriteKind::Out => power,
                    BitWriteKind::Set => true,
                    BitWriteKind::Reset => false,
                };
                dest.write(m, if on { old | mask } else { old & !mask } as i32);
            }
        }
    }
    pub fn display_value(&self, m: &Memory) -> String {
        self.destination().read(m).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plc::{Simulator, compile};
    fn d(name: &str) -> Device {
        name.parse().unwrap()
    }
    fn action(name: &str, args: &[&str]) -> BitAction {
        BitAction::parse(
            name,
            &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        )
        .unwrap()
    }
    #[test]
    fn absolute_retains_signed_minimum_and_flags_in_both_widths() {
        let mut m = Memory::default();
        for flag in 1020..=1022 {
            m.set_system(flag, true);
        }
        for value in [i16::MIN, -1234, 0, 1234, i16::MAX] {
            m.set_word(d("T255"), value);
            action("ABS", &["T255"]).execute(&mut m, true);
            assert_eq!(m.word(d("T255")), value.wrapping_abs());
        }
        for value in [i32::MIN, -32768, -123456789, 0, i32::MAX] {
            m.set_double_word(d("C255"), value);
            action("DABS", &["C255"]).execute(&mut m, true);
            assert_eq!(m.double_word(d("C255")), value.wrapping_abs());
        }
        assert!((1020..=1022).all(|flag| m.system_bit(flag)));
    }
    #[test]
    fn gray_roundtrip_matches_known_patterns_and_negative_input_holds_destination() {
        let mut m = Memory::default();
        for (bin, gray) in [
            (0, 0),
            (1, 1),
            (2, 3),
            (3, 2),
            (6513, 0x15c9),
            (32767, 0x4000),
        ] {
            m.set_word(d("D0"), bin);
            action("GRY", &["D0", "D0"]).execute(&mut m, true);
            assert_eq!(m.word(d("D0")), gray);
            action("GBIN", &["D0", "D0"]).execute(&mut m, true);
            assert_eq!(m.word(d("D0")), bin);
        }
        for bin in [0, 1, 65535, 123456789, i32::MAX] {
            m.set_double_word(d("D0"), bin);
            action("DGRY", &["D0", "D0"]).execute(&mut m, true);
            action("DGBIN", &["D0", "D0"]).execute(&mut m, true);
            assert_eq!(m.double_word(d("D0")), bin);
        }
        m.set_word(d("D2"), 77);
        action("GRY", &["H8000", "D2"]).execute(&mut m, true);
        assert_eq!(m.word(d("D2")), 77);
        assert_eq!(m.word(d("D1067")), 0xe1a);
        assert_eq!(m.word(d("D1068")), 1);
    }
    #[test]
    fn indexed_output_clears_only_selected_bit_on_false_and_sets_hold() {
        let mut m = Memory::default();
        m.set_word(d("D0"), 0x000a);
        let out = action("BOUT", &["D0", "K1"]);
        out.execute(&mut m, false);
        assert_eq!(m.word(d("D0")), 8);
        out.execute(&mut m, true);
        assert_eq!(m.word(d("D0")), 10);
        action("BSET", &["D0", "K2"]).execute(&mut m, false);
        assert_eq!(m.word(d("D0")), 10);
        action("BSET", &["D0", "K2"]).execute(&mut m, true);
        assert_eq!(m.word(d("D0")), 14);
        action("BRST", &["D0", "K3"]).execute(&mut m, true);
        assert_eq!(m.word(d("D0")), 6);
    }
    #[test]
    fn indexed_write_snapshots_aliased_index_and_dynamic_indices_wrap() {
        let mut m = Memory::default();
        m.set_word(d("D0"), 1);
        action("BSET", &["D0", "D0"]).execute(&mut m, true);
        assert_eq!(m.word(d("D0")), 3);
        m.set_double_word(d("D10"), 31);
        action("DBSET", &["D0", "D10"]).execute(&mut m, true);
        assert_eq!(m.double_word(d("D0")) as u32, 0x80000003);
        m.set_word(d("D11"), 1);
        action("DBRST", &["D0", "D10"]).execute(&mut m, true);
        assert_eq!(m.double_word(d("D0")) as u32, 3);
        m.set_word(d("D10"), -1);
        action("BSET", &["D0", "D10"]).execute(&mut m, true);
        assert_eq!(m.word(d("D0")) as u16, 0x8003);
        assert!(!m.system_bit(1067));
        assert!(!m.system_bit(1068));
    }
    #[test]
    fn output_false_does_not_change_following_action_power_and_pulses_rearm() {
        let p = compile("LD X0\nBOUT D0 K0\nOUT Y0\nABSP D1\nGRYP D2 D3\nGBINP D3 D4\nDABSP D10\nDGRYP D12 D14\nDGBINP D14 D16\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_word(d("D1"), -10);
        s.memory.set_word(d("D2"), 3);
        s.memory.set_double_word(d("D10"), -100000);
        s.memory.set_double_word(d("D12"), 123456789);
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D1")), 10);
        assert_eq!(s.memory.word(d("D4")), 3);
        assert_eq!(s.memory.double_word(d("D16")), 123456789);
        s.memory.set_word(d("D1"), -20);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D1")), -20);
        s.memory.set_input(d("X0"), false);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D0")) & 1, 0);
        assert!(!s.memory.bit(d("Y0")));
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D1")), 20);
        assert!(s.memory.bit(d("Y0")));
    }
    #[test]
    fn parser_bounds_and_no_indexed_pulses_are_enforced() {
        for (name, args) in [
            ("ABS", vec!["C200"]),
            ("DABS", vec!["D9999"]),
            ("BSET", vec!["D0", "K16"]),
            ("DBOUT", vec!["D0", "K32"]),
            ("BOUT", vec!["Y0", "K1"]),
            ("GRY", vec!["K7", "X0"]),
        ] {
            assert!(
                BitAction::parse(
                    name,
                    &args.into_iter().map(String::from).collect::<Vec<_>>()
                )
                .is_err()
            );
        }
        for name in ["BOUTP", "DBSETP", "BRSTP"] {
            assert!(compile(&format!("LD X0\n{name} D0 K1\nEND")).is_err());
        }
    }
}
