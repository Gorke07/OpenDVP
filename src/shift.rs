//! Rotation, range shifts, FIFO and digit transfer semantics.
use crate::additional_processing::AdditionalValue;
use crate::block_operand::{BlockDestination, BlockSource};
use crate::calendar_operand::{CalendarDestination, CalendarValue};
use crate::plc::{Area, Device, Memory, WordConversion};
use std::collections::BTreeSet;

fn count(raw: &str, min: u16, max: u16) -> Result<u16, String> {
    raw.strip_prefix('K')
        .and_then(|n| n.parse::<u16>().ok())
        .or_else(|| {
            raw.strip_prefix('H')
                .and_then(|n| u16::from_str_radix(n, 16).ok())
        })
        .filter(|n| (*n >= min) && (*n <= max))
        .ok_or_else(|| format!("Sayı K/H{min}..{max} sabiti olmalı"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plc::{Simulator, compile};
    fn d(s: &str) -> Device {
        s.parse().unwrap()
    }
    #[test]
    fn carry_rotations_match_a_closed_form_17_or_33_bit_ring() {
        for (wide, width) in [(false, 16u32), (true, 32u32)] {
            for left in [false, true] {
                for carry in [false, true] {
                    for value in [0u32, 1, 0x8123, 0x87654321, u32::MAX] {
                        for n in [1, width] {
                            let mask = (1u64 << width) - 1;
                            let value = u64::from(value) & mask;
                            let ring = (u64::from(carry) << width) | value;
                            let ring_bits = width + 1;
                            let all = (1u64 << ring_bits) - 1;
                            let expected = if left {
                                ((ring << n) | (ring >> (ring_bits - n))) & all
                            } else {
                                ((ring >> n) | (ring << (ring_bits - n))) & all
                            };
                            let mut m = Memory::default();
                            if wide {
                                m.set_double_word(d("D0"), value as i32);
                            } else {
                                m.set_word(d("D0"), value as i16);
                            }
                            m.set_system(1022, carry);
                            ShiftAction::Rotate {
                                op: if left {
                                    Rotation::CarryLeft
                                } else {
                                    Rotation::CarryRight
                                },
                                wide,
                                dest: CalendarDestination::parse("D0", wide).unwrap(),
                                n: n as u16,
                            }
                            .execute(&mut m);
                            let actual = if wide {
                                m.double_word(d("D0")) as u32 as u64
                            } else {
                                m.word(d("D0")) as u16 as u64
                            };
                            assert_eq!(actual, expected & mask);
                            assert_eq!(m.system_bit(1022), (expected >> width) != 0);
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn overlapping_word_shifts_follow_native_column_order() {
        for (name, expected) in [
            ("WSFR", [30, 40, 50, 30, 30]),
            ("WSFL", [10, 20, 10, 20, 30]),
        ] {
            let p = compile(&format!("LD X0\n{name} D2 D0 K5 K2\nEND")).unwrap();
            let mut s = Simulator::default();
            s.memory.set_input(d("X0"), true);
            for (i, value) in [10, 20, 30, 40, 50].into_iter().enumerate() {
                s.memory.set_word(d(&format!("D{i}")), value);
            }
            s.scan(&p, 100);
            for (i, value) in expected.into_iter().enumerate() {
                assert_eq!(s.memory.word(d(&format!("D{i}"))), value, "{name} D{i}");
            }
        }
    }
    #[test]
    fn bit_shift_injection_reads_the_already_shifted_source() {
        let p = compile("LD X0\nSFTR M0 M0 K5 K2\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        for name in ["M0", "M2", "M4"] {
            s.memory.set_internal(d(name), true);
        }
        s.scan(&p, 100);
        for (i, value) in [true, false, true, true, false].into_iter().enumerate() {
            assert_eq!(s.memory.bit(d(&format!("M{i}"))), value);
        }
    }
    #[test]
    fn fifo_full_empty_and_last_slot_retention_use_different_flags() {
        let mut m = Memory::default();
        let push = |v| ShiftAction::Push {
            source: CalendarValue::parse(&format!("K{v}"), false).unwrap(),
            dest: BlockDestination::parse("D0", false).unwrap(),
            total: 4,
        };
        for v in [11, 22, 33, 44] {
            push(v).execute(&mut m);
        }
        assert_eq!(m.word(d("D0")), 3);
        assert_eq!(m.word(d("D3")), 33);
        assert!(m.system_bit(1022));
        let pop = ShiftAction::Pop {
            source: BlockDestination::parse("D0", false).unwrap(),
            dest: CalendarDestination::parse("D10", false).unwrap(),
            total: 4,
        };
        for (value, empty) in [(11, false), (22, false), (33, true), (33, true)] {
            pop.execute(&mut m);
            assert_eq!(m.word(d("D10")), value);
            assert_eq!(m.system_bit(1020), empty);
            assert!(m.system_bit(1022));
            assert_eq!(m.word(d("D3")), 33);
        }
    }
    #[test]
    fn zero_reset_does_not_cancel_the_pending_timer_tick() {
        let p = compile("LD X0\nTMR T0 K5\nCNT C0 K3\nZRST T0 T0\nZRST C0 C0\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("T0")), 1);
        assert_eq!(s.memory.word(d("C0")), 0);
        assert!(!s.memory.bit(d("C0")));
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("T0")), 1);
        let p = compile("LD X0\nZRST M10 M5\nEND").unwrap();
        s.memory.set_internal(d("M5"), true);
        s.memory.set_internal(d("M10"), true);
        s.scan(&p, 100);
        assert!(!s.memory.bit(d("M5")));
        assert!(s.memory.bit(d("M10")));
    }
    #[test]
    fn digit_transfer_modes_and_error_pc_are_observable() {
        let p = compile("LD X0\nSMOV D0 K4 K2 D1 K3\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.memory.set_word(d("D0"), 1234);
        s.memory.set_word(d("D1"), 5678);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D1")), 5128);
        s.memory.set_system(1168, true);
        s.memory.set_word(d("D0"), 0x1234);
        s.memory.set_word(d("D1"), 0x5678);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D1")), 0x5128);
        s.memory.set_system(1168, false);
        s.memory.set_word(d("D0"), -1);
        s.memory.set_word(d("D1"), 77);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D1")), 77);
        assert!(s.memory.system_bit(1067));
        assert!(s.memory.system_bit(1068));
        assert_eq!(s.memory.word(d("D1067")), 0xe18);
        assert_eq!(s.memory.word(d("D1068")), 12);
    }
    #[test]
    fn ranges_counts_and_destinations_are_validated_before_execution() {
        let p = compile("LD X0\nSFTR X0 Y375 K3 K2\nZRST C200 C255\nDROR D9998 K32\nEND").unwrap();
        for name in [
            "X0", "X1", "Y375", "Y376", "Y377", "C200", "C255", "D9998", "D9999", "M1022",
        ] {
            assert!(p.devices.contains(&d(name)), "{name}");
        }
        for body in [
            "ROR D0 K0",
            "DROR D0 K33",
            "ROR C200 K1",
            "SFTR X377 Y0 K2 K2",
            "SFTL M0 X0 K2 K1",
            "WSFR D0 D9999 K2 K1",
            "WSFL D0 D0 K2 K3",
            "SFWR K1 D0 K1",
            "ZRST D0 M0",
            "ZRST X0 X1",
            "SMOV D0 K2 K3 D1 K4",
            "PLS S0",
        ] {
            assert!(compile(&format!("LD X0\n{body}\nEND")).is_err(), "{body}");
        }
        assert!(ShiftAction::parse("SMOV", &[]).is_err());
        assert!(!"M1000".parse::<Device>().unwrap().writable_bit());
    }
}
pub fn bit_span(base: Device, n: u16, writable: bool) -> Result<Vec<Device>, String> {
    if !(matches!(base.area, Area::X | Area::Y | Area::M | Area::S))
        || (writable && !base.writable_bit())
    {
        return Err("Bit aralığı kaynağı X/Y/M/S, hedefi Y/M/S olmalı".into());
    }
    (0..n)
        .map(|i| {
            let index = base.index.checked_add(i).ok_or("Bit aralığı taşıyor")?;
            if writable
                && !(Device {
                    area: base.area,
                    index,
                })
                .writable_bit()
            {
                return Err("Bit aralığı salt okunur sistem rölesine taşıyor".into());
            }
            Device {
                area: base.area,
                index,
            }
            .to_string()
            .parse()
        })
        .collect()
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Rotation {
    Right,
    Left,
    CarryRight,
    CarryLeft,
}
/// Shared value/carry kernel; instruction executors own read/write ordering.
pub(crate) fn rotation_result(
    value: i32,
    mut carry: bool,
    wide: bool,
    op: Rotation,
    n: u16,
) -> (i32, bool) {
    let width = if wide { 32 } else { 16 };
    let mask = (1u64 << width) - 1;
    let mut value = u64::from(value as u32) & mask;
    for _ in 0..n {
        let outgoing = if matches!(op, Rotation::Left | Rotation::CarryLeft) {
            value & (1 << (width - 1)) != 0
        } else {
            value & 1 != 0
        };
        let incoming = if matches!(op, Rotation::CarryLeft | Rotation::CarryRight) {
            carry
        } else {
            outgoing
        };
        value = if matches!(op, Rotation::Left | Rotation::CarryLeft) {
            ((value << 1) | u64::from(incoming)) & mask
        } else {
            (value >> 1) | (u64::from(incoming) << (width - 1))
        };
        carry = outgoing;
    }
    (value as i32, carry)
}

/// Shared digit-transfer calculation. Executors own snapshots, mode sampling,
/// scratch publication, late target lookup and the E18 error PC.
pub(crate) fn smov_result(
    source: i16,
    old: i16,
    hex: bool,
    m1: u16,
    m2: u16,
    n: u16,
) -> Option<i16> {
    let source = if hex {
        Some(source)
    } else {
        WordConversion::Bcd.apply(source)
    }?;
    let old = if hex {
        Some(old)
    } else {
        WordConversion::Bcd.apply(old)
    }?;
    let mask = (1u32 << (m2 * 4)) - 1;
    let selected = (u32::from(source as u16) >> ((m1 - m2) * 4)) & mask;
    let shift = (n - m2) * 4;
    let new = (u32::from(old as u16) & !(mask << shift)) | (selected << shift);
    if hex {
        Some(new as i16)
    } else {
        WordConversion::Bin.apply(new as i16)
    }
}

/// WSFR/WSFL retain the resolved native coordinate, not a packed group's
/// canonical PLC index. Numeric T/C/D coordinates are logical registers.
pub(crate) fn word_shift_scratch(base: Device) -> i16 {
    let coordinate = match base.area {
        Area::S | Area::T | Area::C | Area::D => base.index,
        Area::X => 0x400 + base.index,
        Area::Y => 0x500 + base.index,
        Area::M if base.index >= 1536 => 0x1000 + base.index - 1536,
        Area::M => 0x800 + base.index,
    };
    coordinate as i16
}

#[derive(Debug, Clone, PartialEq)]
pub enum ShiftAction {
    Rotate {
        op: Rotation,
        wide: bool,
        dest: CalendarDestination,
        n: u16,
    },
    Bits {
        left: bool,
        source: Device,
        dest: Device,
        total: u16,
        n: u16,
    },
    Words {
        left: bool,
        source: BlockSource,
        dest: BlockDestination,
        total: u16,
        n: u16,
    },
    Push {
        source: CalendarValue,
        dest: BlockDestination,
        total: u16,
    },
    Pop {
        source: BlockDestination,
        dest: CalendarDestination,
        total: u16,
    },
    Reset {
        start: Device,
        end: Device,
    },
    Digits {
        source: CalendarValue,
        m1: u16,
        m2: u16,
        dest: CalendarDestination,
        n: u16,
    },
}
impl ShiftAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let expected = match name {
            "ROR" | "ROL" | "RCR" | "RCL" | "DROR" | "DROL" | "DRCR" | "DRCL" | "ZRST" => 2,
            "SFWR" | "SFRD" => 3,
            "SFTR" | "SFTL" | "WSFR" | "WSFL" => 4,
            "SMOV" => 5,
            _ => return Err("Bilinmeyen kaydırma komutu".into()),
        };
        if args.len() != expected {
            return Err(format!("{name}: {expected} operand gerekli"));
        }
        let device = |i: usize| args[i].parse::<Device>();
        let result = match name {
            "ROR" | "ROL" | "RCR" | "RCL" | "DROR" | "DROL" | "DRCR" | "DRCL" => {
                let wide = name.starts_with('D');
                let op = match name.trim_start_matches('D') {
                    "ROR" => Rotation::Right,
                    "ROL" => Rotation::Left,
                    "RCR" => Rotation::CarryRight,
                    _ => Rotation::CarryLeft,
                };
                let dest = CalendarDestination::parse(&args[0], wide)?;
                if let CalendarValue::Additional(AdditionalValue::Group { digits, .. }) =
                    dest.value()
                    && *digits != if wide { 8 } else { 4 }
                {
                    return Err("Döndürme hedefi 16 bitte K4, 32 bitte K8 grubu olmalı".into());
                }
                Self::Rotate {
                    op,
                    wide,
                    dest,
                    n: count(&args[1], 1, if wide { 32 } else { 16 })?,
                }
            }
            "SFTR" | "SFTL" => {
                let source = device(0)?;
                let dest = device(1)?;
                let total = count(&args[2], 1, 1024)?;
                let n = count(&args[3], 1, total)?;
                bit_span(source, n, false)?;
                bit_span(dest, total, true)?;
                Self::Bits {
                    left: name == "SFTL",
                    source,
                    dest,
                    total,
                    n,
                }
            }
            "WSFR" | "WSFL" => {
                let source = BlockSource::parse(&args[0])?;
                let dest = BlockDestination::parse(&args[1], false)?;
                let total = count(&args[2], 1, 512)?;
                let n = count(&args[3], 1, total)?;
                if source.numeric_kind().is_some() != dest.numeric_kind().is_some() {
                    return Err(
                        "WSFR/WSFL kaynak ve hedefi aynı Kn veya T/C/D kategorisinde olmalı".into(),
                    );
                }
                if let (
                    AdditionalValue::Group { digits: a, .. },
                    AdditionalValue::Group { digits: b, .. },
                ) = (source.value(), dest.value())
                    && a != b
                {
                    return Err("WSFR/WSFL kaynak ve hedef Kn genişlikleri aynı olmalı".into());
                }
                source.devices(n)?;
                dest.devices(total)?;
                Self::Words {
                    left: name == "WSFL",
                    source,
                    dest,
                    total,
                    n,
                }
            }
            "SFWR" => {
                let source = CalendarValue::parse(&args[0], false)?;
                let dest = BlockDestination::parse(&args[1], false)?;
                let total = count(&args[2], 2, 512)?;
                dest.devices(total)?;
                Self::Push {
                    source,
                    dest,
                    total,
                }
            }
            "SFRD" => {
                let source = BlockDestination::parse(&args[0], false)?;
                let dest = CalendarDestination::parse(&args[1], false)?;
                let total = count(&args[2], 2, 512)?;
                source.devices(total)?;
                Self::Pop {
                    source,
                    dest,
                    total,
                }
            }
            "ZRST" => {
                let start = device(0)?;
                let end = device(1)?;
                if start.area != end.area
                    || (!start.writable_bit() && !matches!(start.area, Area::T | Area::C | Area::D))
                {
                    return Err("ZRST hedefleri aynı Y/M/S/T/C/D alanında olmalı".into());
                }
                let s = Self::Reset { start, end };
                s.reset_devices()?;
                s
            }
            "SMOV" => {
                let source = CalendarValue::parse(&args[0], false)?;
                if source.constant().is_some() {
                    return Err("SMOV kaynağı Kn grubu, T/C/D veya E/F olmalı".into());
                }
                let dest = CalendarDestination::parse(&args[3], false)?;
                let m1 = count(&args[1], 1, 4)?;
                let m2 = count(&args[2], 1, m1)?;
                let n = count(&args[4], m2, 4)?;
                Self::Digits {
                    source,
                    m1,
                    m2,
                    dest,
                    n,
                }
            }
            _ => return Err("Bilinmeyen kaydırma komutu".into()),
        };
        Ok(result)
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::Rotate { dest, .. } | Self::Pop { dest, .. } | Self::Digits { dest, .. } => {
                dest.base()
            }
            Self::Words { dest, .. } | Self::Push { dest, .. } => dest.base(),
            Self::Bits { dest, .. } => *dest,
            Self::Reset { end, .. } => *end,
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Rotate { op, wide, .. } => match (op, wide) {
                (Rotation::Right, false) => "ROR",
                (Rotation::Left, false) => "ROL",
                (Rotation::CarryRight, false) => "RCR",
                (Rotation::CarryLeft, false) => "RCL",
                (Rotation::Right, true) => "DROR",
                (Rotation::Left, true) => "DROL",
                (Rotation::CarryRight, true) => "DRCR",
                (Rotation::CarryLeft, true) => "DRCL",
            },
            Self::Bits { left: true, .. } => "SFTL",
            Self::Bits { .. } => "SFTR",
            Self::Words { left: true, .. } => "WSFL",
            Self::Words { .. } => "WSFR",
            Self::Push { .. } => "SFWR",
            Self::Pop { .. } => "SFRD",
            Self::Reset { .. } => "ZRST",
            Self::Digits { .. } => "SMOV",
        }
    }
    pub fn label(&self) -> String {
        let name = self.name();
        match self {
            Self::Rotate { dest, n, .. } => format!("{name} {dest} K{n}"),
            Self::Bits {
                source,
                dest,
                total,
                n,
                ..
            } => format!("{name} {source} {dest} K{total} K{n}"),
            Self::Words {
                source,
                dest,
                total,
                n,
                ..
            } => format!("{name} {source} {dest} K{total} K{n}"),
            Self::Push {
                source,
                dest,
                total,
            } => format!("{name} {source} {dest} K{total}"),
            Self::Pop {
                source,
                dest,
                total,
            } => format!("{name} {source} {dest} K{total}"),
            Self::Reset { start, end } => format!("{name} {start} {end}"),
            Self::Digits {
                source,
                m1,
                m2,
                dest,
                n,
            } => format!("{name} {source} K{m1} K{m2} {dest} K{n}"),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        match self {
            Self::Rotate { dest, .. } => {
                dest.devices(out);
                out.insert(Memory::system_device(1022));
            }
            Self::Bits {
                source,
                dest,
                total,
                n,
                ..
            } => {
                out.extend(bit_span(*source, *n, false).unwrap());
                out.extend(bit_span(*dest, *total, true).unwrap());
            }
            Self::Words {
                source,
                dest,
                total,
                n,
                ..
            } => {
                out.extend(
                    source
                        .devices(*n)
                        .expect("validated WSFR/WSFL source range"),
                );
                out.extend(
                    dest.devices(*total)
                        .expect("validated WSFR/WSFL destination range"),
                );
            }
            Self::Push {
                source,
                dest,
                total,
            } => {
                Memory::error_devices(out);
                source.devices(out);
                out.extend(dest.devices(*total).expect("validated SFWR queue range"));
                out.insert(Memory::system_device(1022));
            }
            Self::Pop {
                source,
                dest,
                total,
            } => {
                Memory::error_devices(out);
                out.extend(source.devices(*total).expect("validated SFRD queue range"));
                dest.devices(out);
                out.insert(Memory::system_device(1020));
            }
            Self::Reset { .. } => {
                out.extend(self.reset_devices().unwrap());
            }
            Self::Digits { source, dest, .. } => {
                source.devices(out);
                dest.devices(out);
                out.insert(Memory::system_device(1168));
                Memory::error_devices(out);
            }
        }
    }
    pub fn reset_devices(&self) -> Result<Vec<Device>, String> {
        if let Self::Reset { start, end } = self {
            let first = start.index.min(end.index);
            (first..=end.index)
                .map(|index| {
                    let device = Device {
                        area: end.area,
                        index,
                    }
                    .to_string()
                    .parse::<Device>()?;
                    if matches!(device.area, Area::Y | Area::M | Area::S) && !device.writable_bit()
                    {
                        return Err("ZRST aralığı salt okunur röle içeriyor".into());
                    }
                    Ok(device)
                })
                .collect()
        } else {
            Ok(Vec::new())
        }
    }
    /// ZRST clears values without canceling a timer tick already scheduled by TMR.
    pub fn execute(&self, m: &mut Memory) {
        match self {
            Self::Rotate { op, wide, dest, n } => {
                let (value, carry) =
                    rotation_result(dest.read(m), m.system_bit(1022), *wide, *op, *n);
                // Native loops update carry before the final operand store.
                // A destination that covers M1022 can therefore overwrite it.
                m.set_system(1022, carry);
                dest.write(m, value);
            }
            Self::Bits {
                left,
                source,
                dest,
                total,
                n,
            } => {
                let mut dst = bit_span(*dest, *total, true).unwrap();
                let mut src = bit_span(*source, *n, false).unwrap();
                if *left {
                    dst.reverse();
                    src.reverse();
                }
                for i in 0..usize::from(total - n) {
                    m.set_internal(dst[i], m.bit(dst[i + usize::from(*n)]));
                }
                for (i, source) in src.iter().enumerate() {
                    m.set_internal(dst[usize::from(total - n) + i], m.bit(*source));
                }
            }
            Self::Words {
                left,
                source,
                dest,
                total,
                n,
            } => {
                m.native_numeric_scratch = Some(word_shift_scratch(source.base()));
                if let AdditionalValue::Group { digits, .. } = dest.value() {
                    let dst = bit_span(dest.base(), total * digits * 4, true)
                        .expect("validated packed word-shift destination");
                    let src = bit_span(source.base(), n * digits * 4, false)
                        .expect("validated packed word-shift source");
                    // Native packed shifts move the complete destination
                    // before source injection. Both directions inject source
                    // bits in ascending order, including overlapping ranges.
                    if *left {
                        for i in (src.len()..dst.len()).rev() {
                            m.set_internal(dst[i], m.bit(dst[i - src.len()]));
                        }
                    } else {
                        for i in 0..dst.len() - src.len() {
                            m.set_internal(dst[i], m.bit(dst[i + src.len()]));
                        }
                    }
                    let start = if *left { 0 } else { dst.len() - src.len() };
                    for (i, source) in src.iter().enumerate() {
                        m.set_internal(dst[start + i], m.bit(*source));
                    }
                    return;
                }
                let mut dst: Vec<_> = (0..*total)
                    .map(|i| dest.element(i).expect("validated WSFR/WSFL destination"))
                    .collect();
                let mut src: Vec<_> = (0..*n)
                    .map(|i| source.element(i).expect("validated WSFR/WSFL source"))
                    .collect();
                if *left {
                    dst.reverse();
                    src.reverse();
                }
                let rem = usize::from(total % n);
                let n = usize::from(*n);
                let len = dst.len();
                for i in 0..rem {
                    let value = dst[i + n].read(m);
                    dst[i].write(m, value);
                }
                for (col, source) in src.iter().enumerate() {
                    let mut i = rem + col;
                    while i + n < len {
                        let value = dst[i + n].read(m);
                        dst[i].write(m, value);
                        i += n;
                    }
                    let value = source.read(m);
                    dst[i].write(m, value);
                }
            }
            Self::Push {
                source,
                dest,
                total,
            } => {
                let value = source.read(m);
                m.native_numeric_scratch = Some(value as i16);
                let pointer = dest.read(m);
                if pointer < 0 {
                    m.operation_error(0xe1a, 3);
                    return;
                }
                let next = (pointer as i16).wrapping_add(1);
                m.set_system(1022, false);
                if next >= *total as i16 {
                    m.set_system(1022, true);
                } else {
                    dest.write(m, i32::from(next));
                    // A corrupt positive header32767 wraps in the installed
                    // WORD temporary. Its signed advance loop visits slot0.
                    dest.element(next.max(0) as u16)
                        .expect("validated SFWR payload slot")
                        .write(m, value);
                }
            }
            Self::Pop {
                source,
                dest,
                total,
            } => {
                let pointer = source.read(m);
                m.set_system(1020, false);
                if pointer < 0 {
                    m.operation_error(0xe1a, 1);
                    return;
                }
                if pointer == 0 {
                    m.set_system(1020, true);
                } else if pointer > 0 {
                    let next = pointer - 1;
                    if next == 0 {
                        m.set_system(1020, true);
                    }
                    source.write(m, next);
                    let value = source
                        .element(1)
                        .expect("validated SFRD first slot")
                        .read(m);
                    dest.write(m, value);
                    for i in 1..*total - 1 {
                        let value = source
                            .element(i + 1)
                            .expect("validated SFRD source slot")
                            .read(m);
                        source
                            .element(i)
                            .expect("validated SFRD destination slot")
                            .write(m, value);
                    }
                }
            }
            Self::Reset { .. } => {
                let range = self.reset_devices().unwrap();
                for d in range.iter().rev() {
                    m.reset_value(*d);
                }
                // Native ZRST does not perform RST's pending-tick suppression.
            }
            Self::Digits {
                source,
                m1,
                m2,
                dest,
                n,
            } => {
                let source = source.read(m) as i16;
                m.native_numeric_scratch = Some(source);
                let old = dest.read(m) as i16;
                let hex = m.system_bit(1168);
                if !hex && (0..=9999).contains(&source) && (0..=9999).contains(&old) {
                    m.native_numeric_scratch = Some(source / 1000);
                }
                if let Some(value) = smov_result(source, old, hex, *m1, *m2, *n) {
                    dest.write(m, i32::from(value));
                } else {
                    m.operation_error(0xe18, 11);
                }
            }
        }
    }
    pub fn display_value(&self, m: &Memory) -> String {
        match self {
            Self::Rotate { dest, .. } => {
                format!("{} C:{}", dest.read(m), u8::from(m.system_bit(1022)))
            }
            Self::Bits { dest, total, .. } => format!(
                "ON {}/{}",
                bit_span(*dest, *total, true)
                    .unwrap()
                    .iter()
                    .filter(|d| m.bit(**d))
                    .count(),
                total
            ),
            Self::Push { dest, .. } => {
                format!("N:{} C:{}", dest.read(m), u8::from(m.system_bit(1022)))
            }
            Self::Pop { dest, .. } => {
                format!("{} Z:{}", dest.read(m), u8::from(m.system_bit(1020)))
            }
            Self::Words { dest, .. } => dest.read(m).to_string(),
            Self::Digits { dest, .. } => dest.read(m).to_string(),
            _ => m.word(self.dest()).to_string(),
        }
    }
}
