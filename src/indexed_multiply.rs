//! Opt-in E/F modifiers for full MUL/DIV results. Source snapshots and zero
//! division precede result resolution; result storage is not a scalar write.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::CalendarValue;
use crate::indexed_address::IndexedAddress;
use crate::indexed_operand::{AddressedValue, has_modifier};
use crate::multiply_destination::MultiplyDestination;
use crate::plc::{Action, Area, Device, Memory, Operand};
use crate::wide::{WideAction, WideDestination, WideOp, WideOperand, WideSource};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub enum PreparedMultiply {
    Ready(Box<Action>),
    ZeroDivisor { offset: u16 },
}

#[derive(Debug, Clone, PartialEq)]
enum IndexedMultiplyDestination {
    Direct(MultiplyDestination),
    Modified {
        address: IndexedAddress,
        digits: Option<u16>,
        instruction_wide: bool,
    },
}

pub(crate) fn validate_model(memory: &Memory, devices: &BTreeSet<Device>) -> Result<(), String> {
    if devices.iter().any(|d| match d.area {
        Area::D => d.index > memory.data_register_limit.unwrap_or(9999),
        Area::M => d.index > memory.relay_limit.unwrap_or(4095),
        _ => false,
    }) {
        return Err("İndeksli sayısal operandın tam aralığı seçilen model belleğini aşıyor".into());
    }
    Ok(())
}

pub(crate) fn qualify_group(
    original: Device,
    effective: Device,
    bits: u16,
    result: bool,
) -> Result<(), String> {
    if effective.area != Area::M {
        return Ok(());
    }
    // The arithmetic result resolver uses raw high-byte >=0x0d, so even M1280
    // differs from the generic scalar reader's strictly >0x0d00 remap.
    let first_unqualified = if result { 1280 } else { 1281 };
    if original.index < 1536 && (first_unqualified..=1535).contains(&effective.index) {
        return Err(
            "İndeksli M başlangıcının native fiziksel eşlemesi kanonik aralıkla uyuşmuyor".into(),
        );
    }
    if (original.index < 1536) != (effective.index < 1536)
        || (effective.index < 1536 && u32::from(effective.index) + u32::from(bits) > 1536)
    {
        return Err("İndeksli M sonuç/kaynak aralığının fiziksel alan geçişi doğrulanmadı".into());
    }
    Ok(())
}

impl IndexedMultiplyDestination {
    fn parse(raw: &str, wide: bool) -> Result<Self, String> {
        if !has_modifier(raw) {
            return MultiplyDestination::parse(raw, wide).map(Self::Direct);
        }
        // Numeric word full results use the 32-bit writer, including C200+.
        // This is an existing native extension; vendor validators reject that
        // word target even though their emitter and native writer support it.
        let group = raw.trim().to_ascii_uppercase().starts_with('K');
        let binding = AddressedValue::parse(raw, wide || !group, true)?;
        let AddressedValue::Modified {
            address, digits, ..
        } = binding
        else {
            unreachable!("modifier was present")
        };
        Ok(Self::Modified {
            address,
            digits,
            instruction_wide: wide,
        })
    }
    fn base(&self) -> Device {
        match self {
            Self::Direct(target) => target.base(),
            Self::Modified { address, .. } => address.base,
        }
    }
    fn modifiers(&self) -> Vec<Device> {
        match self {
            Self::Direct(_) => Vec::new(),
            Self::Modified { address, .. } => vec![address.index.device()],
        }
    }
    fn devices(&self, out: &mut BTreeSet<Device>) {
        match self {
            Self::Direct(target) => target.devices(out),
            Self::Modified { address, .. } => {
                out.insert(address.index.device());
            }
        }
    }
    fn resolve(&self, memory: &Memory) -> Result<MultiplyDestination, String> {
        let target = match self {
            Self::Direct(target) => target.clone(),
            Self::Modified {
                address,
                digits,
                instruction_wide,
            } => {
                let value = address.resolve(
                    memory,
                    *instruction_wide || digits.is_none(),
                    *digits,
                    true,
                )?;
                MultiplyDestination::from_value(
                    CalendarValue::Additional(value),
                    *instruction_wide,
                )?
            }
        };
        let mut devices = BTreeSet::new();
        target.devices(&mut devices);
        validate_model(memory, &devices)?;
        if let CalendarValue::Additional(AdditionalValue::Group { base, digits, .. }) =
            target.value()
        {
            qualify_group(self.base(), *base, digits * 8, true)?;
        }
        Ok(target)
    }
    fn native_words(&self) -> Result<Vec<u16>, String> {
        let (mut words, wide) = match self {
            Self::Direct(target) => (
                crate::es2_encoder::calendar_operand(target.value())?,
                target.wide(),
            ),
            Self::Modified {
                address,
                digits,
                instruction_wide,
            } => (
                address.native_words(*instruction_wide || digits.is_none(), *digits)?,
                *instruction_wide,
            ),
        };
        words.truncate(if wide { 4 } else { 2 });
        Ok(words)
    }
}
impl std::fmt::Display for IndexedMultiplyDestination {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Direct(target) => target.fmt(f),
            Self::Modified {
                address, digits, ..
            } => {
                if let Some(digits) = digits {
                    write!(f, "K{digits}")?;
                }
                address.fmt(f)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedMultiplyAction {
    divide: bool,
    wide: bool,
    pulse_label: bool,
    sources: [AddressedValue; 2],
    target: IndexedMultiplyDestination,
}
impl IndexedMultiplyAction {
    pub fn supports(name: &str) -> bool {
        matches!(
            name.to_ascii_uppercase().as_str(),
            "MUL" | "MULP" | "DMUL" | "DMULP" | "DIV" | "DIVP" | "DDIV" | "DDIVP"
        )
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        args.iter().any(|raw| has_modifier(raw))
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if !Self::supports(name) || args.len() != 3 {
            return Err("İndeksli MUL/DIV: iki kaynak ve tam sonuç hedefi gerekli".into());
        }
        let name = name.to_ascii_uppercase();
        let wide = matches!(name.as_str(), "DMUL" | "DMULP" | "DDIV" | "DDIVP");
        Ok(Self {
            divide: name.contains("DIV"),
            wide,
            pulse_label: name.ends_with('P'),
            sources: [
                AddressedValue::parse(&args[0], wide, false)?,
                AddressedValue::parse(&args[1], wide, false)?,
            ],
            target: IndexedMultiplyDestination::parse(&args[2], wide)?,
        })
    }
    pub fn sources(&self) -> &[AddressedValue] {
        &self.sources
    }
    /// Validate before a native preflight reads the actual resolved source.
    pub fn resolved_source(&self, index: usize, memory: &Memory) -> Result<CalendarValue, String> {
        let binding = self
            .sources
            .get(index)
            .ok_or("MUL/DIV kaynak numarası 0 veya 1 olmalı")?;
        let value = binding.resolve(memory, false)?;
        let mut devices = BTreeSet::new();
        value.devices(&mut devices);
        validate_model(memory, &devices)?;
        if let CalendarValue::Additional(AdditionalValue::Group { base, digits, .. }) = &value {
            qualify_group(
                binding.base().expect("group has a base"),
                *base,
                digits * 4,
                false,
            )?;
        }
        Ok(value)
    }
    pub fn prepare(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<PreparedMultiply, String> {
        let mut snapshot = [0; 2];
        for (index, slot) in snapshot.iter_mut().enumerate() {
            observed.extend(self.sources[index].modifier_devices());
            let value = self.resolved_source(index, memory)?;
            *slot = crate::indexed_operand::read_resolved_value(&value, memory, observed)?;
        }
        if self.divide && snapshot[1] == 0 {
            return Ok(PreparedMultiply::ZeroDivisor {
                offset: if self.wide { 9 } else { 5 },
            });
        }
        observed.extend(self.target.modifiers());
        let target = self.target.resolve(memory)?;
        target.devices(observed);
        let frozen = |value| {
            CalendarValue::Additional(if self.wide {
                AdditionalValue::Double(WideOperand::Constant(value))
            } else {
                AdditionalValue::Word(Operand::Constant(value as i16))
            })
        };
        let [a, b] = snapshot.map(frozen);
        let action = if self.wide {
            Action::Wide(WideAction {
                op: if self.divide {
                    WideOp::Divide
                } else {
                    WideOp::Multiply
                },
                sources: vec![WideSource::Read(a), WideSource::Read(b)],
                dest: WideDestination::Product(target),
                count: 0,
            })
        } else if self.divide {
            Action::Divide(a, b, target)
        } else {
            Action::Multiply(a, b, target)
        };
        Ok(PreparedMultiply::Ready(Box::new(action)))
    }
    /// Static source-layout label only; never an effective execution address.
    pub fn dest(&self) -> Device {
        self.target.base()
    }
    pub fn label(&self) -> String {
        let name = match (self.divide, self.wide) {
            (false, false) => "MUL",
            (false, true) => "DMUL",
            (true, false) => "DIV",
            (true, true) => "DDIV",
        };
        format!(
            "{name}{} {} {} {}",
            if self.pulse_label { "P" } else { "" },
            self.sources[0],
            self.sources[1],
            self.target
        )
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        for source in &self.sources {
            source.devices(out);
        }
        self.target.devices(out);
        Memory::error_devices(out);
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        self.sources
            .iter()
            .flat_map(AddressedValue::modifier_devices)
            .chain(self.target.modifiers())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    /// Snapshot display addresses, including a held result; this is deliberately
    /// distinct from prepare's record of actual execution accesses.
    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        for index in 0..2 {
            if let Ok(value) = self.resolved_source(index, memory) {
                value.devices(out);
            }
        }
        if let Ok(target) = self.target.resolve(memory) {
            target.devices(out);
        }
    }
    pub fn display_value(&self, memory: &Memory) -> String {
        match self.target.resolve(memory) {
            Ok(target) => {
                let [low, high] = target.read_pair(memory);
                if self.divide {
                    return format!("Q:{low} R:{high}");
                }
                if matches!(
                    target.value(),
                    CalendarValue::Additional(AdditionalValue::Group { .. })
                ) {
                    return format!("L:{low} H:{high}");
                }
                if self.wide {
                    ((u64::from(low as u32) | (u64::from(high as u32) << 32)) as i64).to_string()
                } else {
                    ((u32::from(low as u16) | (u32::from(high as u16) << 16)) as i32).to_string()
                }
            }
            Err(error) => format!("İndeks: {error}"),
        }
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let mut header = match (self.divide, self.wide) {
            (false, false) => 0x3c00,
            (false, true) => 0x3d00,
            (true, false) => 0x3e00,
            (true, true) => 0x3f00,
        };
        if self.pulse_label {
            header |= 0x10;
        }
        let mut words = vec![header];
        for source in &self.sources {
            words.extend(source.native_words()?);
        }
        words.extend(self.target.native_words()?);
        Ok(words)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(raw: &str) -> Device {
        raw.parse().unwrap()
    }
    fn parse(source: &str) -> IndexedMultiplyAction {
        let mut parts = source.split_whitespace();
        let name = parts.next().unwrap();
        IndexedMultiplyAction::parse(name, &parts.map(str::to_owned).collect::<Vec<_>>()).unwrap()
    }
    fn target(prepared: PreparedMultiply) -> MultiplyDestination {
        let PreparedMultiply::Ready(action) = prepared else {
            panic!("prepared full result")
        };
        match *action {
            Action::Multiply(_, _, target) | Action::Divide(_, _, target) => target,
            Action::Wide(WideAction {
                dest: WideDestination::Product(target),
                ..
            }) => target,
            _ => panic!("prepared full result"),
        }
    }
    fn prepare(
        action: &IndexedMultiplyAction,
        memory: &Memory,
    ) -> Result<PreparedMultiply, String> {
        action.prepare(memory, &mut BTreeSet::new())
    }

    #[test]
    fn literal_headers_and_original_descriptors_cover_all_eight_spellings() {
        for (name, header, wide) in [
            ("MUL", 0x3c00, false),
            ("MULP", 0x3c10, false),
            ("DMUL", 0x3d00, true),
            ("DMULP", 0x3d10, true),
            ("DIV", 0x3e00, false),
            ("DIVP", 0x3e10, false),
            ("DDIV", 0x3f00, true),
            ("DDIVP", 0x3f10, true),
        ] {
            let action = parse(&format!("{name} D5E1 D10F7 K1M128E1"));
            let expected = if wide {
                vec![
                    header, 0x0ae6, 0x8086, 0x0080, 0x0080, 0x14f6, 0x8089, 0x0080, 0x0080, 0x80e4,
                    0x2882, 0x0080, 0x0080,
                ]
            } else {
                vec![header, 0x0ae6, 0x8086, 0x14f6, 0x8089, 0x80e4, 0x2882]
            };
            assert_eq!(action.native_words().unwrap(), expected, "{name}");
            assert_eq!(action, parse(&action.label()));
            assert!(IndexedMultiplyAction::has_modifiers(
                &action
                    .sources
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            ));
        }
        for unsupported in ["MUL16", "MUL32P", "DIV16", "DIV32P", "DMULPP", "XCH"] {
            assert!(!IndexedMultiplyAction::supports(unsupported));
        }
    }

    #[test]
    fn every_bank_uses_its_actual_single_word_alias_even_for_wide_sources() {
        let banks = [
            ("E0", "D1028", 0x0ae6, 0x0086),
            ("E1", "D1182", 0x0ae6, 0x8086),
            ("E2", "D1184", 0x0ae6, 0x0087),
            ("E3", "D1186", 0x0ae6, 0x8087),
            ("E4", "D1188", 0x0ae6, 0x0088),
            ("E5", "D1190", 0x0ae6, 0x8088),
            ("E6", "D1192", 0x0ae6, 0x0089),
            ("E7", "D1194", 0x0ae6, 0x8089),
            ("F0", "D1029", 0x0af6, 0x0086),
            ("F1", "D1183", 0x0af6, 0x8086),
            ("F2", "D1185", 0x0af6, 0x0087),
            ("F3", "D1187", 0x0af6, 0x8087),
            ("F4", "D1189", 0x0af6, 0x0088),
            ("F5", "D1191", 0x0af6, 0x8088),
            ("F6", "D1193", 0x0af6, 0x0089),
            ("F7", "D1195", 0x0af6, 0x8089),
        ];
        for (bank, alias, first, second) in banks {
            let action = parse(&format!("DMUL D5{bank} K3 D20"));
            assert_eq!(
                &action.native_words().unwrap()[1..5],
                &[first, second, 0x0080, 0x0080]
            );
            assert_eq!(action.modifier_devices(), vec![d(alias)]);
            let mut memory = Memory::default();
            memory.set_word(d(alias), -2);
            memory.set_word(
                Device {
                    index: d(alias).index + 1,
                    ..d(alias)
                },
                12345,
            );
            memory.set_double_word(d("D3"), -70000);
            let PreparedMultiply::Ready(result) = prepare(&action, &memory).unwrap() else {
                panic!("ready result")
            };
            let Action::Wide(result) = *result else {
                panic!("wide result")
            };
            assert_eq!(result.sources[0].value().constant(), Some(-70000));
            assert_eq!(result.sources[1].value().constant(), Some(3));
        }
    }

    #[test]
    fn source_and_result_roles_keep_direct_ef_and_counter_rules_separate() {
        for source in [
            "MUL F7 D0E0 E1",
            "DIV E7 K3 D0F7",
            "DMUL E7 D0F7 D20E1",
            "DDIV K8X0E0 K8S0F0 K8Y0E1",
            "MUL K1X0E0 K4M0F1 K4S0E2",
            "MUL D0E0 K1 C255F7",
        ] {
            assert_eq!(parse(source).label(), source);
        }
        for source in [
            "DMUL F1 K3 D0E0",
            "DDIV E1 K3 E1",
            "MUL D0E0 K3 F1",
            "MUL C200E0 K3 D0",
            "MUL K1E0 K3 D0",
            "MUL E0E1 K3 D0",
            "MUL D0E0 K3 K1X0F0",
            "MUL D0E0 K3 Y0F0",
            "MUL D0E0 K3 K5M0F0",
            "DMUL D0E0 K3 K9M0F0",
            "MUL D0E8 K3 D0",
            "MUL D0E0 K3 K1E0M0",
        ] {
            let parts = source
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            assert!(
                IndexedMultiplyAction::parse(&parts[0], &parts[1..]).is_err(),
                "{source}"
            );
        }
        let natural = parse("MUL K3 K4 C200E1");
        assert_eq!(
            natural.native_words().unwrap(),
            [0x3c00, 0x0380, 0x0080, 0x0480, 0x0080, 0x00e6, 0x2284]
        );
    }

    #[test]
    fn zero_divisor_precedes_target_index_access_and_display_is_independent() {
        for (name, offset) in [("DIV", 5), ("DIVP", 5), ("DDIV", 9), ("DDIVP", 9)] {
            let action = parse(&format!("{name} D0E0 D10F0 D9999E1"));
            let mut memory = Memory::default();
            memory.set_word(d("D1028"), 2);
            memory.set_word(d("D1029"), 2);
            memory.set_word(d("D1182"), 100);
            memory.set_double_word(d("D2"), 17);
            let mut observed = BTreeSet::new();
            assert_eq!(
                action.prepare(&memory, &mut observed),
                Ok(PreparedMultiply::ZeroDivisor { offset })
            );
            assert!(observed.contains(&d("D1028")) && observed.contains(&d("D1029")));
            assert!(observed.contains(&d("D2")) && observed.contains(&d("D12")));
            assert!(!observed.contains(&d("D1182")) && !observed.contains(&d("D9999")));
            memory.set_double_word(d("D12"), 1);
            assert!(prepare(&action, &memory).is_err());
        }
        let action = parse("DIV D0E0 K0 D20F0");
        let mut memory = Memory::default();
        memory.set_word(d("D1029"), 2);
        let mut display = BTreeSet::new();
        action.observed_devices(&memory, &mut display);
        assert!(display.contains(&d("D22")) && display.contains(&d("D23")));
        assert_eq!(action.display_value(&memory), "Q:0 R:0");
    }

    #[test]
    fn both_sources_and_result_address_are_frozen_before_alias_writes() {
        let action = parse("DMUL D20E0 D30F0 D1182E1");
        let mut memory = Memory::default();
        memory.set_word(d("D1028"), 1);
        memory.set_word(d("D1029"), 2);
        memory.set_double_word(d("D21"), 100000);
        memory.set_double_word(d("D32"), 100000);
        memory.set_double_word(d("D1682"), 77);
        let mut observed = BTreeSet::new();
        let PreparedMultiply::Ready(result) = action.prepare(&memory, &mut observed).unwrap()
        else {
            panic!("ready result")
        };
        let Action::Wide(result) = *result else {
            panic!("wide result")
        };
        assert_eq!(result.sources[0].value().constant(), Some(100000));
        assert_eq!(result.sources[1].value().constant(), Some(100000));
        for name in [
            "D21", "D22", "D32", "D33", "D1182", "D1183", "D1184", "D1185",
        ] {
            assert!(observed.contains(&d(name)), "{name}");
        }
        assert!(!observed.contains(&d("D20")) && !observed.contains(&d("D30")));
        memory.set_double_word(d("D21"), 7);
        memory.set_double_word(d("D32"), 9);
        memory.set_word(d("D1182"), 500);
        result.execute(&mut memory);
        assert_eq!(memory.double_word(d("D1182")), 0x540be400);
        assert_eq!(memory.double_word(d("D1184")), 2);
        assert_eq!(memory.double_word(d("D1682")), 77);
    }

    #[test]
    fn complete_secondary_spans_and_model_limits_are_checked_for_direct_and_modified_targets() {
        let mut memory = Memory::default();
        memory.data_register_limit = Some(4999);
        for source in [
            "MUL K3 K4 D4999E0",
            "DMUL K3 K4 D4997E0",
            "MUL D0E0 K3 D4999",
            "DMUL D0E0 K3 D4997",
            "MUL K3 K4 K1S1020E0",
            "MUL K3 K4 K1M996E0",
            "DDIV K3 K4 C255E0",
            "DMUL K3 K4 T253E0",
        ] {
            assert!(prepare(&parse(source), &memory).is_err(), "{source}");
        }
        memory.set_word(d("D1028"), -3);
        assert_eq!(
            target(prepare(&parse("DMUL K3 K4 D4999E0"), &memory).unwrap()).base(),
            d("D4996")
        );
        memory.set_word(d("D1028"), -1);
        assert_eq!(
            target(prepare(&parse("MUL K3 K4 T255E0"), &memory).unwrap()).base(),
            d("T254")
        );
        memory.set_word(d("D1028"), 0);
        for source in ["MUL D5000 K3 D0E0", "DMUL D4999 K3 D0E0"] {
            assert!(
                parse(source).resolved_source(0, &memory).is_err(),
                "{source}"
            );
        }
        memory.relay_limit = Some(255);
        assert!(prepare(&parse("MUL K3 K4 K1M252E0"), &memory).is_err());
        assert!(
            parse("MUL K1M256 K3 D0E0")
                .resolved_source(0, &memory)
                .is_err()
        );
    }

    #[test]
    fn result_mapping_qualification_is_stricter_than_source_at_m1280() {
        assert!(qualify_group(d("M0"), d("M1280"), 4, false).is_ok());
        assert!(qualify_group(d("M0"), d("M1280"), 8, true).is_err());
        assert!(qualify_group(d("M0"), d("M1284"), 8, true).is_err());
        assert!(qualify_group(d("M0"), d("M1536"), 8, true).is_err());
        assert!(qualify_group(d("M2000"), d("M1280"), 8, true).is_err());
        let mut memory = Memory::default();
        memory.set_word(d("D1028"), 1284);
        assert!(prepare(&parse("MUL K1 K2 K1M0E0"), &memory).is_err());
        memory.set_word(d("D1028"), 100);
        assert!(prepare(&parse("MUL K1 K2 K1M2000E0"), &memory).is_ok());
    }

    #[test]
    fn word_natural_counter_extension_and_wide_boundary_layout_are_typed() {
        let mut memory = Memory::default();
        let action = parse("MUL K300 K300 C200E0");
        memory.set_word(d("D1028"), -1);
        let low_only = target(prepare(&action, &memory).unwrap());
        memory.set_double_word(d("C200"), 77);
        low_only.write_pair(&mut memory, [24464, 1]);
        assert_eq!(memory.word(d("C199")), 24464);
        assert_eq!(memory.double_word(d("C200")), 77);
        memory.set_word(d("D1028"), 1);
        let natural = target(prepare(&parse("MUL K300 K300 C199E0"), &memory).unwrap());
        natural.write_pair(&mut memory, [24464, 1]);
        assert_eq!(memory.double_word(d("C200")), 90000);

        for (offset, first, second) in [
            (0, "C197", "C199"),
            (1, "C198", "C200"),
            (2, "C199", "C200"),
            (3, "C200", "C201"),
        ] {
            let mut m = Memory::default();
            m.set_word(d("D1028"), offset);
            let result = target(prepare(&parse("DDIV K-17 K3 C197E0"), &m).unwrap());
            result.write_pair(&mut m, [-5, -2]);
            assert_eq!(result.base(), d(first));
            assert_eq!(
                if d(second).index == 199 {
                    i32::from(m.word(d(second)))
                } else {
                    m.double_word(d(second))
                },
                -2
            );
            let mut spans = BTreeSet::new();
            result.devices(&mut spans);
            assert!(spans.contains(&d(first)) && spans.contains(&d(second)));
        }
    }

    #[test]
    fn typed_result_constructor_keeps_two_short_chunks_and_neighbor_bits() {
        let mut memory = Memory::default();
        memory.set_bit(d("M127"), true);
        memory.set_bit(d("M136"), true);
        let result = MultiplyDestination::from_value(
            CalendarValue::Additional(AdditionalValue::Group {
                base: d("M128"),
                digits: 1,
                wide: false,
            }),
            false,
        )
        .unwrap();
        result.write_pair(&mut memory, [-51, -1]);
        let actual = (0..8).fold(0u8, |v, n| {
            v | (u8::from(memory.bit(Device {
                area: Area::M,
                index: 128 + n,
            })) << n)
        });
        assert_eq!(actual, 0xfd);
        assert_eq!(result.read_pair(&memory), [13, 15]);
        assert!(memory.bit(d("M127")) && memory.bit(d("M136")));
        assert!(
            MultiplyDestination::from_value(
                CalendarValue::Additional(AdditionalValue::Group {
                    base: d("M0"),
                    digits: 5,
                    wide: true,
                }),
                false
            )
            .is_err()
        );
        assert!(
            MultiplyDestination::from_value(
                CalendarValue::Index {
                    index: 1,
                    high_bank: true,
                    wide: false
                },
                false
            )
            .is_err()
        );
        assert!(
            MultiplyDestination::from_value(
                CalendarValue::Index {
                    index: 1,
                    high_bank: false,
                    wide: true
                },
                true
            )
            .is_err()
        );
    }
}
