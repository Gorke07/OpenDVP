//! Legacy direct SER/DSER constructor and ordinary ALT coils. Public IL DSER
//! uses indexed_search_toggle's distinct source/result counter cursors.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::CalendarValue;
use crate::data_processing::DataValue;
use crate::plc::{Area, Device, Memory, Operand, word_range};
use crate::shift::bit_span;
use crate::wide::WideOperand;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub enum SearchAction {
    Search {
        source: AdditionalValue,
        value: CalendarValue,
        dest: AdditionalValue,
        count: DataValue,
    },
    Toggle {
        dest: Device,
    },
}

/// Retained bounded cursor for the legacy direct SearchAction constructor.
/// Public IL DSER handles C199/C200 using separate source/result cursors in
/// indexed_search_toggle; widening this shared cursor would conflate them.
/// Groups advance by their actual number of bits, not by the instruction width.
pub fn stack_slots(value: &AdditionalValue, count: u16) -> Result<Vec<AdditionalValue>, String> {
    if count == 0 {
        return Err("SER yığın uzunluğu sıfır olamaz".into());
    }
    match value {
        AdditionalValue::Word(Operand::Register(base)) => Ok(word_range(*base, count)?
            .into_iter()
            .map(|d| AdditionalValue::Word(Operand::Register(d)))
            .collect()),
        AdditionalValue::Double(WideOperand::Register(base)) => {
            let width = count.checked_mul(2).ok_or("DSER yığın aralığı taşıyor")?;
            word_range(*base, width)?;
            Ok((0..count)
                .map(|i| {
                    AdditionalValue::Double(WideOperand::Register(Device {
                        area: base.area,
                        index: base.index + i * 2,
                    }))
                })
                .collect())
        }
        AdditionalValue::Group { base, digits, wide } => {
            let stride = digits.checked_mul(4).ok_or("SER bit grubu taşıyor")?;
            bit_span(
                *base,
                stride.checked_mul(count).ok_or("SER bit aralığı taşıyor")?,
                false,
            )?;
            Ok((0..count)
                .map(|i| AdditionalValue::Group {
                    base: Device {
                        area: base.area,
                        index: base.index + i * stride,
                    },
                    digits: *digits,
                    wide: *wide,
                })
                .collect())
        }
        _ => Err("SER yığını KnX/KnY/KnM/KnS veya T/C/D adresi olmalı".into()),
    }
}

fn within_model(slots: &[AdditionalValue], m: &Memory) -> bool {
    let limit = m.data_register_limit.unwrap_or(9999);
    slots.iter().all(|v| {
        let mut devices = BTreeSet::new();
        v.devices(&mut devices);
        devices
            .iter()
            .all(|d| d.area != Area::D || d.index <= limit)
    })
}

/// Byte-sized search results, signed comparisons, and last-index extrema ties.
/// Callers sample the complete source before publishing any result.
pub(crate) fn search_result(values: &[i32], needle: i32) -> [u8; 5] {
    debug_assert!(!values.is_empty());
    let mut result = [0u8; 5];
    let mut minimum = values[0];
    let mut maximum = values[0];
    for (index, current) in values.iter().copied().enumerate() {
        let index = index as u8;
        if current == needle {
            result[0] = result[0].wrapping_add(1);
            if result[0] == 1 {
                result[1] = index;
            }
            result[2] = index;
        }
        if current <= minimum {
            minimum = current;
            result[3] = index;
        }
        if current >= maximum {
            maximum = current;
            result[4] = index;
        }
    }
    result
}

impl SearchAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        match name {
            "ALT" => {
                if args.len() != 1 {
                    return Err("ALT: bir Y/M/S bit hedefi gerekli".into());
                }
                let dest: Device = args[0].parse()?;
                if !dest.writable_bit() {
                    return Err("ALT hedefi Y/M/S biti olmalı".into());
                }
                Ok(Self::Toggle { dest })
            }
            "SER" | "DSER" => {
                if args.len() != 4 {
                    return Err(format!("{name}: S1 S2 D n gerekli"));
                }
                let wide = name == "DSER";
                let source = AdditionalValue::parse(&args[0], wide, false)?;
                let value = CalendarValue::parse(&args[1], wide)?;
                let dest = AdditionalValue::parse(&args[2], wide, true)?;
                let count = DataValue::parse(&args[3], wide)?;
                if matches!(&count, DataValue::Word(Operand::Register(d)) | DataValue::Double(WideOperand::Register(d)) if d.area != Area::D)
                {
                    return Err("SER n operandı K/H sabiti veya D register olmalı".into());
                }
                let action = Self::Search {
                    source,
                    value,
                    dest,
                    count,
                };
                // Dynamic counts remain valid near an endpoint; their actual
                // span is resolved before reading at execution time.
                action.source_slots(action.search_span_count())?;
                action.result_slots()?;
                Ok(action)
            }
            _ => Err("Bilinmeyen arama/değiştirme komutu".into()),
        }
    }
    pub fn wide(&self) -> bool {
        matches!(self, Self::Search { source, .. } if source.wide())
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Toggle { .. } => "ALT",
            _ if self.wide() => "DSER",
            _ => "SER",
        }
    }
    pub fn header(&self) -> u16 {
        match self {
            Self::Toggle { .. } => 0x9400,
            _ if self.wide() => 0x8b00,
            _ => 0x8a00,
        }
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::Search { dest, .. } => dest.base().expect("validated SER destination"),
            Self::Toggle { dest } => *dest,
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::Search {
                source,
                value,
                dest,
                count,
            } => format!("{} {source} {value} {dest} {count}", self.name()),
            Self::Toggle { dest } => format!("ALT {dest}"),
        }
    }
    pub fn max_count(&self) -> u16 {
        if self.wide() { 128 } else { 256 }
    }
    /// Statically known source span; runtime count values are never guessed.
    pub fn search_span_count(&self) -> u16 {
        match self {
            Self::Search { count, .. } => count
                .constant()
                .filter(|n| (1..=i32::from(self.max_count())).contains(n))
                .map(|n| n as u16)
                .unwrap_or(1),
            _ => 1,
        }
    }
    pub fn source_slots(&self, count: u16) -> Result<Vec<AdditionalValue>, String> {
        match self {
            Self::Search { source, .. } => stack_slots(source, count),
            _ => Ok(Vec::new()),
        }
    }
    pub fn result_slots(&self) -> Result<Vec<AdditionalValue>, String> {
        match self {
            Self::Search { dest, .. } => stack_slots(dest, 5),
            _ => Ok(Vec::new()),
        }
    }
    /// A bridge may call this before native execution. Invalid count domains
    /// are valid PLC error cases; unsafe logical pointer spans are limitations.
    pub fn validate_runtime_span(&self, m: &Memory) -> Result<(), String> {
        if let Self::Search { count, .. } = self {
            let n = count.read(m);
            if (1..=i32::from(self.max_count())).contains(&n) {
                let source = self.source_slots(n as u16)?;
                let dest = self.result_slots()?;
                if !within_model(&source, m) || !within_model(&dest, m) {
                    return Err("SER yığın aralığı modelin D sınırını aşıyor".into());
                }
            }
        }
        Ok(())
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        out.extend((1020..=1022).map(Memory::system_device));
        Memory::error_devices(out);
        match self {
            Self::Toggle { dest } => {
                out.insert(*dest);
            }
            Self::Search { value, count, .. } => {
                value.devices(out);
                count.devices(out);
                for slot in self
                    .source_slots(self.search_span_count())
                    .expect("validated source")
                    .into_iter()
                    .chain(self.result_slots().expect("validated destination"))
                {
                    slot.devices(out);
                }
            }
        }
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        if let Self::Search { count, .. } = self {
            let n = count.read(m);
            if (1..=i32::from(self.max_count())).contains(&n)
                && self.validate_runtime_span(m).is_ok()
                && let Ok(slots) = self.source_slots(n as u16)
            {
                for slot in slots {
                    slot.devices(out);
                }
            }
        }
    }
    /// Called only with enabled ordinary or occurrence-specific pulse power.
    pub fn execute(&self, m: &mut Memory) {
        match self {
            Self::Toggle { dest } => m.set_internal(*dest, !m.bit(*dest)),
            Self::Search {
                source,
                value,
                dest,
                count,
            } => {
                // Native SER stores its search value before checking n.
                // DSER uses these same two scratch bytes for operand tags.
                // Legacy word SORT's progress bug observes this shared word.
                let needle = value.read(m);
                m.native_numeric_scratch = Some(if self.wide() {
                    let tag = |value: &AdditionalValue| -> u16 {
                        match value {
                            AdditionalValue::Group { digits, .. } => 0x80 | (digits * 2),
                            AdditionalValue::Word(Operand::Register(d))
                            | AdditionalValue::Double(WideOperand::Register(d)) => match d.area {
                                Area::T => 0x82,
                                Area::C => 0x84,
                                _ => 0x86,
                            },
                            _ => unreachable!("validated search stack"),
                        }
                    };
                    ((tag(source) << 8) | tag(dest)) as i16
                } else {
                    needle as i16
                });
                let n = count.read(m);
                if !(1..=i32::from(self.max_count())).contains(&n) {
                    m.operation_error(0x0e1a, if self.wide() { 13 } else { 7 });
                    return;
                }
                if let Err(error) = self.validate_runtime_span(m) {
                    m.execution_limitations.insert(format!(
                        "{}: {error}; raw-pointer boundary semantics are unverified",
                        self.name()
                    ));
                    return;
                }
                // Search value, complete input and all result addresses are
                // resolved before any write, including overlapping destinations.
                let values: Vec<i32> = self
                    .source_slots(n as u16)
                    .expect("checked span")
                    .iter()
                    .map(|slot| slot.read(m))
                    .collect();
                let slots = self.result_slots().expect("validated destination");
                let result = search_result(&values, needle);
                for (slot, value) in slots.into_iter().zip(result) {
                    slot.write(m, i32::from(value));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d(raw: &str) -> Device {
        raw.parse().unwrap()
    }
    fn action(name: &str, args: &[&str]) -> SearchAction {
        SearchAction::parse(
            name,
            &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        )
        .unwrap()
    }
    fn results(m: &Memory, a: &SearchAction) -> Vec<i32> {
        a.result_slots()
            .unwrap()
            .iter()
            .map(|slot| slot.read(m))
            .collect()
    }
    #[test]
    fn manual_stack_signed_extrema_last_ties_and_no_match() {
        let mut m = Memory::default();
        for (i, value) in [88, 100, 110, 150, 100, 300, 100, 5, 100, 500]
            .into_iter()
            .enumerate()
        {
            m.set_word(d(&format!("D{}", i + 10)), value);
        }
        let a = action("SER", &["D10", "K100", "D50", "K10"]);
        a.execute(&mut m);
        assert_eq!(results(&m, &a), [4, 1, 8, 7, 9]);
        for (i, value) in [-2, 7, -2, 7].into_iter().enumerate() {
            m.set_word(d(&format!("D{i}")), value);
        }
        let a = action("SER", &["D0", "K11", "D50", "K4"]);
        a.execute(&mut m);
        assert_eq!(results(&m, &a), [0, 0, 0, 2, 3]);
    }
    #[test]
    fn byte_count_wrap_and_double_result_width() {
        let mut m = Memory::default();
        for i in 0..256 {
            m.set_word(d(&format!("D{i}")), 7);
        }
        let a = action("SER", &["D0", "K7", "D500", "K256"]);
        a.execute(&mut m);
        assert_eq!(results(&m, &a), [0, 0, 255, 255, 255]);
        for i in 0..128 {
            m.set_double_word(d(&format!("D{}", i * 2)), -1);
        }
        let a = action("DSER", &["D0", "K-1", "D500", "K128"]);
        a.execute(&mut m);
        assert_eq!(results(&m, &a), [128, 0, 127, 127, 127]);
        for i in 0..5 {
            assert_eq!(m.word(d(&format!("D{}", 501 + i * 2))), 0);
        }
    }
    #[test]
    fn source_needle_and_destination_alias_are_snapshotted() {
        let mut m = Memory::default();
        for (i, value) in [4, 2, 4, -7].into_iter().enumerate() {
            m.set_word(d(&format!("D{i}")), value);
        }
        let a = action("SER", &["D0", "D3", "D0", "K4"]);
        a.execute(&mut m);
        assert_eq!(results(&m, &a), [1, 3, 3, 3, 2]);
    }
    #[test]
    fn packed_groups_use_group_stride_and_clear_full_result_groups() {
        let mut m = Memory::default();
        let a = action("SER", &["K1M0", "K3", "K1Y0", "K4"]);
        for (slot, value) in a.source_slots(4).unwrap().into_iter().zip([3, 1, 3, 0]) {
            slot.write(&mut m, value);
        }
        a.execute(&mut m);
        assert_eq!(results(&m, &a), [2, 0, 2, 3, 2]);
        let a = action("DSER", &["K8M2000", "K-1", "K8S0", "K3"]);
        for (slot, value) in a.source_slots(3).unwrap().into_iter().zip([-1, 0, -1]) {
            slot.write(&mut m, value);
        }
        for slot in a.result_slots().unwrap() {
            slot.write(&mut m, -1);
        }
        a.execute(&mut m);
        assert_eq!(results(&m, &a), [2, 0, 2, 2, 1]);
        for i in 0..5 {
            for bit in 8..32 {
                assert!(!m.bit(Device {
                    area: Area::S,
                    index: i * 32 + bit
                }));
            }
        }
    }
    #[test]
    fn invalid_count_holds_results_flags_and_reports_native_operand_pc() {
        for (op, invalid, offset) in [
            ("SER", 0, 7),
            ("SER", 257, 7),
            ("SER", -1, 7),
            ("DSER", 129, 13),
        ] {
            let mut m = Memory::default();
            let a = action(op, &["D0", "K0", "D500", &format!("K{invalid}")]);
            for slot in a.result_slots().unwrap() {
                slot.write(&mut m, 55);
            }
            m.set_system(1020, true);
            m.set_system(1022, true);
            a.execute(&mut m);
            assert_eq!(results(&m, &a), [55; 5]);
            assert!(m.system_bit(1020));
            assert!(!m.system_bit(1021));
            assert!(m.system_bit(1022));
            assert_eq!(m.word(d("D1067")), 0x0e1a);
            assert_eq!(m.word(d("D1068")), offset);
        }
    }
    #[test]
    fn dynamic_near_endpoint_valid_count_and_explicit_unverified_span() {
        let a = action("SER", &["D9999", "K8", "D20", "D10"]);
        let mut m = Memory::default();
        m.set_word(d("D9999"), 8);
        m.set_word(d("D10"), 1);
        a.execute(&mut m);
        assert_eq!(results(&m, &a), [1, 0, 0, 0, 0]);
        let mut observed = BTreeSet::new();
        a.observed_devices(&m, &mut observed);
        assert!(observed.contains(&d("D9999")));
        m.set_word(d("D10"), 2);
        a.execute(&mut m);
        assert_eq!(results(&m, &a), [1, 0, 0, 0, 0]);
        assert!(!m.execution_limitations.is_empty());
        assert!(!m.system_bit(1067));
        let a = action("SER", &["D4999", "K8", "D20", "D10"]);
        m.data_register_limit = Some(4999);
        assert!(a.validate_runtime_span(&m).is_err());
    }
    #[test]
    fn operand_matrices_and_unproven_area_crossings_are_explicit() {
        for args in [
            ["K1", "K1", "D0", "K1"],
            ["D0", "K1", "K4X0", "K1"],
            ["D0", "K1", "D0", "T0"],
            ["D0", "K1", "D9996", "K1"],
            ["C199", "K1", "D0", "K2"],
        ] {
            assert!(SearchAction::parse("SER", &args.map(str::to_string)).is_err());
        }
        assert!(
            SearchAction::parse("DSER", &["C200", "K1", "D0", "K1"].map(str::to_string)).is_err()
        );
        assert!(
            SearchAction::parse("DSER", &["D0", "K1", "D9991", "K1"].map(str::to_string)).is_err()
        );
        assert!(
            SearchAction::parse("DSER", &["D0", "C200", "D9990", "K1"].map(str::to_string)).is_ok()
        );
        for bad in ["X0", "T0", "C0", "D0", "K4Y0"] {
            assert!(SearchAction::parse("ALT", &[bad.to_string()]).is_err());
        }
    }
    #[test]
    fn alternate_changes_exactly_its_bare_bit_each_enabled_call() {
        let mut m = Memory::default();
        for target in ["Y0", "M2000", "S1023"] {
            let a = action("ALT", &[target]);
            a.execute(&mut m);
            assert!(m.bit(d(target)));
            a.execute(&mut m);
            assert!(!m.bit(d(target)));
            assert_eq!(a.header(), 0x9400);
        }
    }
}
