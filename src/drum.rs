//! Absolute and incremental drum sequencers. Native memory-file stubs are not used.
use crate::additional_processing::AdditionalValue;
use crate::plc::{Area, Device, Memory, Operand, word_range};
use crate::shift::bit_span;
use crate::wide::{WideOperand, double_devices};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Default)]
pub struct DrumState {
    /// Installed activation ownership is indexed by the actual instruction header PC.
    pub active_pcs: BTreeSet<u16>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DrumAction {
    Absolute {
        source: AdditionalValue,
        value: AdditionalValue,
        dest: Device,
        count: u16,
    },
    Incremental {
        source: AdditionalValue,
        counter: Device,
        dest: Device,
        count: u16,
    },
}

fn register_value(raw: &str, wide: bool) -> Result<AdditionalValue, String> {
    let d: Device = raw.parse()?;
    if !matches!(d.area, Area::T | Area::C | Area::D) {
        return Err("Tambur sayısal operandı T/C/D olmalı".into());
    }
    if wide {
        double_devices(d)?;
        Ok(AdditionalValue::Double(WideOperand::Register(d)))
    } else {
        // Keep the matrix-valid spelling available; raw high-speed-counter word
        // traversal is explicitly checked before any execution below.
        Ok(AdditionalValue::Word(Operand::Register(d)))
    }
}
fn table_value(raw: &str, wide: bool) -> Result<AdditionalValue, String> {
    if raw.starts_with('K') {
        let v = AdditionalValue::parse(raw, wide, false)?;
        if matches!(&v, AdditionalValue::Group { digits, .. } if *digits == if wide {8} else {4}) {
            return Ok(v);
        }
        return Err("Tambur bit tablosu kelimede K4, çift kelimede K8 olmalı".into());
    }
    register_value(raw, wide)
}
fn count(raw: &str) -> Result<u16, String> {
    raw.strip_prefix('K')
        .and_then(|s| s.parse::<u16>().ok())
        .or_else(|| {
            raw.strip_prefix('H')
                .and_then(|s| u16::from_str_radix(s, 16).ok())
        })
        .filter(|v| (1..=64).contains(v))
        .ok_or_else(|| "Tambur n operandı K/H 1..64 sabiti olmalı".into())
}

/// Explicit C199 natural-width transition, unlike a blanket two-register stride.
/// Raw word traversal into the high-speed-counter bank remains an open facet.
pub fn table_slots(source: &AdditionalValue, count: u16) -> Result<Vec<AdditionalValue>, String> {
    match source {
        AdditionalValue::Group { base, digits, wide } => {
            let bits = digits * 4;
            bit_span(
                *base,
                bits.checked_mul(count)
                    .ok_or("Tambur bit aralığı taşıyor")?,
                false,
            )?;
            Ok((0..count)
                .map(|n| AdditionalValue::Group {
                    base: Device {
                        area: base.area,
                        index: base.index + n * bits,
                    },
                    digits: *digits,
                    wide: *wide,
                })
                .collect())
        }
        AdditionalValue::Word(Operand::Register(base)) => Ok(word_range(*base, count)?
            .into_iter()
            .map(|d| AdditionalValue::Word(Operand::Register(d)))
            .collect()),
        AdditionalValue::Double(WideOperand::Register(base)) => {
            let mut next = *base;
            let mut result = Vec::new();
            for _ in 0..count {
                double_devices(next)?;
                result.push(AdditionalValue::Double(WideOperand::Register(next)));
                let stride = if next.area == Area::C && next.index >= 199 {
                    1
                } else {
                    2
                };
                next.index = next
                    .index
                    .checked_add(stride)
                    .ok_or("Tambur çift kelime aralığı taşıyor")?;
            }
            Ok(result)
        }
        _ => Err("Tambur tablosu K4/K8 bit grubu veya T/C/D olmalı".into()),
    }
}

impl DrumAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if !matches!(name, "ABSD" | "DABSD" | "INCD") {
            return Err("Bilinmeyen tambur komutu".into());
        }
        if args.len() != 4 {
            return Err(format!("{name}: S1 S2 D n gerekli"));
        }
        let wide = name == "DABSD";
        let source = table_value(&args[0], wide)?;
        let dest: Device = args[2].parse()?;
        if !dest.writable_bit() {
            return Err("Tambur hedefi Y/M/S biti olmalı".into());
        }
        let count = count(&args[3])?;
        bit_span(dest, count, true)?;
        let action = if name == "INCD" {
            let counter: Device = args[1].parse()?;
            if counter.area != Area::C || counter.index > 198 {
                return Err("INCD S2 operandı C0..C198 olmalı; sonraki sayaç da kullanılır".into());
            }
            Self::Incremental {
                source,
                counter,
                dest,
                count,
            }
        } else {
            let value = register_value(&args[1], wide)?;
            Self::Absolute {
                source,
                value,
                dest,
                count,
            }
        };
        // Known full spans are validated at runtime too (including model limits).
        // An unproven logical-bank crossing remains a visible execution limitation,
        // rather than an unsafe fabricated raw-memory alias.
        Ok(action)
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Incremental { .. } => "INCD",
            Self::Absolute { source, .. } if source.wide() => "DABSD",
            _ => "ABSD",
        }
    }
    pub fn header(&self) -> u16 {
        match self.name() {
            "ABSD" => 0x8c00,
            "DABSD" => 0x8d00,
            _ => 0x8e00,
        }
    }
    pub fn wide(&self) -> bool {
        matches!(self, Self::Absolute {source,..} if source.wide())
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::Absolute { dest, .. } | Self::Incremental { dest, .. } => *dest,
        }
    }
    pub fn count(&self) -> u16 {
        match self {
            Self::Absolute { count, .. } | Self::Incremental { count, .. } => *count,
        }
    }
    pub fn source(&self) -> &AdditionalValue {
        match self {
            Self::Absolute { source, .. } | Self::Incremental { source, .. } => source,
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::Absolute {
                source,
                value,
                dest,
                count,
            } => format!("{} {source} {value} {dest} K{count}", self.name()),
            Self::Incremental {
                source,
                counter,
                dest,
                count,
            } => format!("INCD {source} {counter} {dest} K{count}"),
        }
    }
    pub fn source_slots(&self) -> Result<Vec<AdditionalValue>, String> {
        table_slots(
            self.source(),
            self.count()
                * if matches!(self, Self::Absolute { .. }) {
                    2
                } else {
                    1
                },
        )
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        Memory::error_devices(out);
        out.extend((1020..=1022).map(Memory::system_device));
        if let Ok(slots) = self.source_slots() {
            for slot in slots {
                slot.devices(out);
            }
        } else {
            self.source().devices(out);
        }
        if let Ok(bits) = bit_span(self.dest(), self.count(), true) {
            out.extend(bits);
        }
        match self {
            Self::Absolute { value, .. } => value.devices(out),
            Self::Incremental { counter, .. } => {
                out.insert(*counter);
                out.insert(Device {
                    area: Area::C,
                    index: counter.index + 1,
                });
                out.insert(Memory::system_device(1029));
            }
        }
    }
    pub fn observed_devices(&self, _m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
    }
    /// Conservative native preflight validates the complete declared table,
    /// including before INCD initialization/disabled paths that may not read it.
    pub fn validate_native_span(&self, m: &Memory) -> Result<(), String> {
        self.validate_model(m, true)
    }
    fn validate_model(&self, m: &Memory, source: bool) -> Result<(), String> {
        let mut devices = BTreeSet::new();
        devices.extend(bit_span(self.dest(), self.count(), true)?);
        if source {
            for slot in self.source_slots()? {
                slot.devices(&mut devices);
            }
            if let Self::Absolute { value, .. } = self {
                if matches!(value,AdditionalValue::Word(Operand::Register(d)) if d.area==Area::C && d.index>=200)
                {
                    return Err("16 bit C200+ sayısal okumanın ham sayaç eşlemesi doğrulanmadı; DABSD kullanımı ayrıca desteklenir".into());
                }
                value.devices(&mut devices);
            }
        }
        for d in devices {
            if (d.area == Area::D && d.index > m.data_register_limit.unwrap_or(9999))
                || (d.area == Area::M && d.index > m.relay_limit.unwrap_or(4095))
            {
                return Err(format!("{d} seçili modelin adres sınırını aşıyor"));
            }
        }
        Ok(())
    }
    /// Must receive false power as well; pc is the actual encoded HEADER PC.
    pub fn execute(&self, m: &mut Memory, power: bool, pc: u16) {
        let active = m.drum.active_pcs.contains(&pc);
        if !power && (!active || matches!(self, Self::Absolute { .. })) {
            return;
        }
        // INCD initialization and falling power do not read S1. Preserve that
        // native ordering even if a currently unused table span is unavailable.
        let need_source = matches!(self, Self::Absolute { .. }) || (power && active);
        if let Err(e) = self.validate_model(m, need_source) {
            m.execution_limitations
                .insert(format!("{} yürütülemedi: {e}", self.name()));
            return;
        }
        let outputs = bit_span(self.dest(), self.count(), true).expect("validated outputs");
        match self {
            Self::Absolute { source, value, .. } => {
                let value = value.read(m); // Snapshot before any output can alias S1.
                if self.wide() {
                    if let AdditionalValue::Double(WideOperand::Register(base)) = source {
                        let tag = match base.area {
                            Area::T => 0x82,
                            Area::C => 0x84,
                            _ => 0x86,
                        };
                        // The installed raw-table path writes only this byte;
                        // grouped tables skip it. Preserve unknown prior high bits.
                        m.native_numeric_scratch = m
                            .native_numeric_scratch
                            .map(|prior| ((prior as u16 & 0xff00) | tag) as i16);
                    }
                } else {
                    m.native_numeric_scratch = Some(value as i16);
                }
                let slots = self.source_slots().expect("validated source");
                for (pair, output) in slots.as_chunks::<2>().0.iter().zip(outputs) {
                    let a = pair[0].read(m);
                    let b = pair[1].read(m);
                    let on = if a <= b {
                        a <= value && value <= b
                    } else {
                        value < a.min(b) || value > a.max(b)
                    };
                    m.set_internal(output, on);
                }
            }
            Self::Incremental { counter, count, .. } => {
                let section_device = Device {
                    area: Area::C,
                    index: counter.index + 1,
                };
                if !power {
                    m.drum.active_pcs.remove(&pc);
                    m.set_word(*counter, 0);
                    m.set_word(section_device, 0);
                    for output in outputs {
                        m.set_internal(output, false);
                    }
                } else if !active {
                    m.set_word(*counter, 0);
                    m.set_word(section_device, 0);
                    for (n, output) in outputs.into_iter().enumerate() {
                        m.set_internal(output, n == 0);
                    }
                    m.drum.active_pcs.insert(pc);
                } else {
                    let present = m.word(*counter);
                    m.native_numeric_scratch = Some(present);
                    let section = (m.word(section_device) as u16 & 0xff) as usize;
                    if section >= usize::from(*count) {
                        return;
                    }
                    let threshold =
                        self.source_slots().expect("validated table")[section].read(m) as i16;
                    if section == 0 && m.system_bit(1029) {
                        m.set_system(1029, false);
                    }
                    if present < threshold {
                        m.set_internal(outputs[section], true);
                        return;
                    }
                    m.set_internal(outputs[section], false);
                    let next = section + 1;
                    let next = if next >= usize::from(*count) {
                        m.set_system(1029, true);
                        0
                    } else {
                        next
                    };
                    m.set_internal(outputs[next], true);
                    m.set_word(*counter, 0);
                    let high = m.word(section_device) as u16 & 0xff00;
                    m.set_word(section_device, (high | next as u16) as i16);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d(s: &str) -> Device {
        s.parse().unwrap()
    }
    fn action(name: &str, args: &str) -> DrumAction {
        DrumAction::parse(
            name,
            &args
                .split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>(),
        )
        .unwrap()
    }
    #[test]
    fn exact_pdf_matrices_and_counts() {
        for (name, args) in [
            ("ABSD", "D0 D20 M0 K1"),
            ("DABSD", "K8M0 C200 Y0 H40"),
            ("INCD", "K4S0 C198 M0 K64"),
        ] {
            action(name, args);
        }
        for (name, args) in [
            ("ABSDP", "D0 D20 M0 K1"),
            ("DINCD", "D0 C0 M0 K1"),
            ("ABSD", "K3M0 D20 M0 K1"),
            ("DABSD", "K4M0 D20 M0 K1"),
            ("ABSD", "D0 K1 M0 K1"),
            ("ABSD", "E0 D20 M0 K1"),
            ("INCD", "D0 D20 M0 K1"),
            ("INCD", "D0 C199 M0 K1"),
            ("ABSD", "D0 D20 M0 D30"),
            ("ABSD", "D0 D20 M0 K255"),
            ("ABSD", "D0 D20 M0 K0"),
        ] {
            assert!(
                DrumAction::parse(
                    name,
                    &args
                        .split_whitespace()
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                )
                .is_err(),
                "{name} {args}"
            );
        }
    }
    #[test]
    fn signed_inclusive_equal_and_strict_reversed_windows() {
        for wide in [false, true] {
            let a = action(if wide { "DABSD" } else { "ABSD" }, "D0 D100 M0 K3");
            let mut m = Memory::default();
            let pairs = [(-10, 10), (5, 5), (20, -20)];
            for (n, (lo, hi)) in pairs.into_iter().enumerate() {
                if wide {
                    m.set_double_word(d(&format!("D{}", n * 4)), lo);
                    m.set_double_word(d(&format!("D{}", n * 4 + 2)), hi);
                } else {
                    m.set_word(d(&format!("D{}", n * 2)), lo as i16);
                    m.set_word(d(&format!("D{}", n * 2 + 1)), hi as i16);
                }
            }
            for value in [-21, -20, -10, 0, 5, 10, 20, 21] {
                if wide {
                    m.set_double_word(d("D100"), value);
                } else {
                    m.set_word(d("D100"), value as i16);
                }
                a.execute(&mut m, true, 10);
                assert_eq!(m.bit(d("M0")), (-10..=10).contains(&value));
                assert_eq!(m.bit(d("M1")), value == 5);
                assert_eq!(m.bit(d("M2")), !(-20..=20).contains(&value));
            }
            m.set_internal(d("M0"), true);
            a.execute(&mut m, false, 10);
            assert!(m.bit(d("M0")));
        }
    }
    #[test]
    fn pair_reads_observe_prior_output_alias_and_snapshot_value() {
        let a = action("ABSD", "K4M0 D0 M32 K2");
        let mut m = Memory::default();
        m.set_word(d("D0"), 0);
        // Initially both pairs are [0,0]. Output0 aliases the low bit of
        // pair1's lower bound: its write makes pair1 [1,0] before that read.
        a.execute(&mut m, true, 0);
        assert!(m.bit(d("M32")));
        assert!(!m.bit(d("M33"))); // Sequential read gives reversed endpoint OFF.
    }
    #[test]
    fn incremental_lifecycle_wrap_shared_flags_and_section_high_byte() {
        let a = action("INCD", "D0 C10 M0 K2");
        let mut m = Memory::default();
        m.set_word(d("D0"), 3);
        m.set_word(d("D1"), -1);
        m.set_system(1029, true);
        a.execute(&mut m, true, 20);
        assert!(m.bit(d("M0")));
        assert!(m.system_bit(1029));
        m.set_word(d("C10"), 2);
        a.execute(&mut m, true, 20);
        assert!(!m.system_bit(1029));
        assert_eq!(m.word(d("C10")), 2);
        m.set_word(d("C11"), 0x7f00);
        m.set_word(d("C10"), 100);
        a.execute(&mut m, true, 20);
        assert_eq!(m.word(d("C11")), 0x7f01);
        assert_eq!(m.word(d("C10")), 0);
        assert!(m.bit(d("M1")));
        assert!(!m.bit(d("M0")));
        a.execute(&mut m, true, 20);
        assert!(m.system_bit(1029));
        assert_eq!(m.word(d("C11")), 0x7f00);
        a.execute(&mut m, false, 20);
        assert_eq!(m.word(d("C11")), 0);
        assert!(!m.bit(d("M0")));
        assert!(m.system_bit(1029));
        a.execute(&mut m, true, 20);
        assert!(m.bit(d("M0")));
        assert!(m.drum.active_pcs.contains(&20));
    }
    #[test]
    fn header_pc_owns_initialization_and_invalid_section_holds() {
        let a = action("INCD", "D0 C0 M0 K1");
        let mut m = Memory::default();
        m.set_word(d("D0"), 5);
        a.execute(&mut m, true, 10);
        m.set_word(d("C0"), 4);
        a.execute(&mut m, true, 30);
        assert_eq!(m.word(d("C0")), 0);
        assert_eq!(m.drum.active_pcs.len(), 2);
        m.set_word(d("C1"), 9);
        m.set_word(d("C0"), 10);
        a.execute(&mut m, true, 10);
        assert_eq!(m.word(d("C0")), 10);
        assert_eq!(m.word(d("C1")), 9);
    }
    #[test]
    fn c199_transition_is_single_zero_extended_then_natural_counter() {
        let a = action("DABSD", "C199 D0 M0 K1");
        let mut m = Memory::default();
        m.set_word(d("C199"), -1);
        m.set_double_word(d("C200"), 70000);
        m.set_double_word(d("D0"), 65535);
        a.execute(&mut m, true, 0);
        assert!(m.bit(d("M0")));
        let slots = table_slots(
            &AdditionalValue::Double(WideOperand::Register(d("C197"))),
            4,
        )
        .unwrap();
        assert_eq!(
            slots
                .iter()
                .map(|s| s.base().unwrap().index)
                .collect::<Vec<_>>(),
            vec![197, 199, 200, 201]
        );
    }
    #[test]
    fn unavailable_full_span_holds_and_reports_without_fabricated_error() {
        let a = action("ABSD", "D4998 D0 M0 K2");
        let mut m = Memory::default();
        m.data_register_limit = Some(4999);
        m.set_internal(d("M0"), true);
        a.execute(&mut m, true, 0);
        assert!(m.bit(d("M0")));
        assert!(!m.execution_limitations.is_empty());
        assert!(!m.system_bit(1067));
        let a = action("INCD", "D9999 C0 M10 K2");
        a.execute(&mut m, true, 30);
        assert!(m.bit(d("M10"))); // Initialization does not read unused S1.
        m.set_word(d("C0"), 20);
        a.execute(&mut m, true, 30);
        assert_eq!(m.word(d("C0")), 20);
        a.execute(&mut m, false, 30);
        assert!(!m.bit(d("M10")));
        assert_eq!(m.word(d("C0")), 0);
    }
    #[test]
    fn full_width_groups_and_sixty_four_outputs_preserve_other_flags() {
        let a = action("ABSD", "D0 T0 M200 K64");
        let mut m = Memory::default();
        for n in 0..64 {
            m.set_word(
                Device {
                    area: Area::D,
                    index: n * 2,
                },
                -2,
            );
            m.set_word(
                Device {
                    area: Area::D,
                    index: n * 2 + 1,
                },
                2,
            );
        }
        for flag in [1020, 1021, 1022, 1029] {
            m.set_system(flag, true);
        }
        a.execute(&mut m, true, 12);
        for n in 200..264 {
            assert!(m.bit(Device {
                area: Area::M,
                index: n
            }));
        }
        for flag in [1020, 1021, 1022, 1029] {
            assert!(m.system_bit(flag));
        }
        let b = action("INCD", "K4M0 C0 Y0 K2");
        table_slots(b.source(), 2).unwrap()[0].write(&mut m, 1);
        table_slots(b.source(), 2).unwrap()[1].write(&mut m, 2);
        b.execute(&mut m, true, 100);
        m.set_word(d("C0"), 1);
        b.execute(&mut m, true, 100);
        assert_eq!(m.word(d("C1")), 1);
        assert!(m.bit(d("Y1")));
        m.set_word(d("C0"), 1);
        b.execute(&mut m, true, 100);
        assert_eq!(m.word(d("C1")), 1);
        m.set_word(d("C0"), 2);
        b.execute(&mut m, true, 100);
        assert_eq!(m.word(d("C1")), 0);
        assert!(m.system_bit(1029));
    }
}
