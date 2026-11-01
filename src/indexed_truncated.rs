//! E/F address modifiers for the eight truncated MUL/DIV spellings. The
//! existing arithmetic executor owns carry and quotient behavior; this wrapper
//! freezes source values and validates exactly one instruction-width result.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::{CalendarDestination, CalendarValue};
use crate::indexed_multiply::{PreparedMultiply, qualify_group, validate_model};
use crate::indexed_operand::{AddressedValue, has_modifier};
use crate::plc::{Action, Device, Memory, Operand};
use crate::wide::{WideAction, WideDestination, WideOp, WideOperand, WideSource};
use std::collections::BTreeSet;

fn binding(raw: &str, wide: bool, write: bool) -> Result<AddressedValue, String> {
    let value = AddressedValue::parse(raw, wide, write)?;
    // The truncated tables have no direct-F exception, even in word sources.
    // An F suffix on a permitted T/C/D/Kn address remains an index WORD.
    if matches!(
        &value,
        AddressedValue::Direct(CalendarValue::Index {
            high_bank: true,
            ..
        })
    ) {
        return Err("MUL16/MUL32/DIV16/DIV32 doğrudan F operandı kullanamaz".into());
    }
    Ok(value)
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedTruncatedAction {
    divide: bool,
    wide: bool,
    pulse_label: bool,
    sources: [AddressedValue; 2],
    target: AddressedValue,
}

impl IndexedTruncatedAction {
    pub fn supports(name: &str) -> bool {
        matches!(
            name.to_ascii_uppercase().as_str(),
            "MUL16" | "MUL16P" | "MUL32" | "MUL32P" | "DIV16" | "DIV16P" | "DIV32" | "DIV32P"
        )
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        args.iter().any(|raw| has_modifier(raw))
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if !Self::supports(name) || args.len() != 3 {
            return Err(
                "İndeksli MUL16/MUL32/DIV16/DIV32: iki kaynak ve tek sonuç hedefi gerekli".into(),
            );
        }
        let name = name.to_ascii_uppercase();
        let wide = name.contains("32");
        Ok(Self {
            divide: name.starts_with("DIV"),
            wide,
            pulse_label: name.ends_with('P'),
            sources: [
                binding(&args[0], wide, false)?,
                binding(&args[1], wide, false)?,
            ],
            target: binding(&args[2], wide, true)?,
        })
    }
    pub fn sources(&self) -> &[AddressedValue] {
        &self.sources
    }
    /// Validate actual storage before a native preflight reads that source.
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
                binding.base().expect("group base"),
                *base,
                digits * 4,
                false,
            )?;
        }
        Ok(value)
    }
    fn resolved_target(&self, memory: &Memory) -> Result<CalendarDestination, String> {
        let value = self.target.resolve(memory, true)?;
        let target = CalendarDestination::from_value(value)?;
        let mut devices = BTreeSet::new();
        target.devices(&mut devices);
        validate_model(memory, &devices)?;
        if let CalendarValue::Additional(AdditionalValue::Group { base, digits, .. }) =
            target.value()
        {
            qualify_group(self.dest(), *base, digits * 4, true)?;
        }
        Ok(target)
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
        observed.extend(self.target.modifier_devices());
        let target = self.resolved_target(memory)?;
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
                    WideOp::DivideQuotient
                } else {
                    WideOp::MultiplyTruncated
                },
                sources: vec![WideSource::Read(a), WideSource::Read(b)],
                dest: WideDestination::Numeric(target),
                count: 0,
            })
        } else if self.divide {
            Action::DivideQuotient(a, b, target)
        } else {
            Action::MultiplyTruncated(a, b, target)
        };
        Ok(PreparedMultiply::Ready(Box::new(action)))
    }
    /// Original base is a label only; use resolved_target for the live address.
    pub fn dest(&self) -> Device {
        self.target.base().expect("validated target")
    }
    pub fn label(&self) -> String {
        format!(
            "{}{}{} {} {} {}",
            if self.divide { "DIV" } else { "MUL" },
            if self.wide { "32" } else { "16" },
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
        if !self.divide {
            out.insert(Memory::system_device(1022));
        }
        Memory::error_devices(out);
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        self.sources
            .iter()
            .flat_map(AddressedValue::modifier_devices)
            .chain(self.target.modifier_devices())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    /// UI snapshot collection includes held results independently of whether a
    /// zero divisor would prevent access during an actual instruction visit.
    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        for index in 0..2 {
            if let Ok(source) = self.resolved_source(index, memory) {
                source.devices(out);
            }
        }
        if let Ok(target) = self.resolved_target(memory) {
            target.devices(out);
        }
    }
    pub fn display_value(&self, memory: &Memory) -> String {
        match self.resolved_target(memory) {
            Ok(target) if self.divide => format!("Q:{}", target.read(memory)),
            Ok(target) => format!(
                "{} C:{}",
                target.read(memory),
                u8::from(memory.system_bit(1022))
            ),
            Err(reason) => format!("İndeks: {reason}"),
        }
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let mut header = match (self.divide, self.wide) {
            (false, false) => 0xf400,
            (false, true) => 0xf500,
            (true, false) => 0xf600,
            (true, true) => 0xf700,
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
    use crate::plc::{Area, Simulator, compile_il};

    fn d(raw: &str) -> Device {
        raw.parse().unwrap()
    }
    fn parse(source: &str) -> IndexedTruncatedAction {
        let parts = source
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        IndexedTruncatedAction::parse(&parts[0], &parts[1..]).unwrap()
    }
    fn prepared(action: &IndexedTruncatedAction, memory: &Memory) -> Action {
        let PreparedMultiply::Ready(action) = action.prepare(memory, &mut BTreeSet::new()).unwrap()
        else {
            panic!("ready action")
        };
        *action
    }
    fn run_frozen(action: Action, memory: Memory) -> Memory {
        // Run the returned ordinary action through an independently compiled
        // same-width ordinary site; no indexed production parser is needed.
        let mut p = compile_il(&format!("LD X0\n{}\nEND", action.label())).unwrap();
        p.networks[0].actions[0] = action;
        let mut s = Simulator::default();
        s.memory = memory;
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 0);
        assert!(s.memory.execution_limitations.is_empty());
        s.memory
    }
    const FORMS: [(&str, u16, bool); 8] = [
        ("MUL16", 0xf400, false),
        ("MUL16P", 0xf410, false),
        ("MUL32", 0xf500, true),
        ("MUL32P", 0xf510, true),
        ("DIV16", 0xf600, false),
        ("DIV16P", 0xf610, false),
        ("DIV32", 0xf700, true),
        ("DIV32P", 0xf710, true),
    ];

    #[test]
    fn all_eight_headers_use_literal_single_result_descriptors() {
        for (name, header, wide) in FORMS {
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
            assert_eq!(action.label(), format!("{name} D5E1 D10F7 K1M128E1"));
            assert_eq!(parse(&action.label()), action);
        }
        for name in ["MUL", "DMUL16", "DMUL32", "DDIV16", "DIV32PP", "DIV"] {
            assert!(!IndexedTruncatedAction::supports(name));
        }
    }

    #[test]
    fn direct_f_is_forbidden_in_every_role_but_suffix_f_stays_word_sized() {
        for (name, _, _) in FORMS {
            for args in [
                ["F0", "K3", "D0E0"],
                ["D0E0", "F7", "D20"],
                ["D0E0", "K3", "F1"],
            ] {
                assert!(
                    IndexedTruncatedAction::parse(name, &args.map(str::to_owned)).is_err(),
                    "{name} {args:?}"
                );
            }
            assert!(
                IndexedTruncatedAction::parse(name, &["E1".into(), "D5F7".into(), "E2".into()])
                    .is_ok()
            );
        }
        for raw in [
            "MUL16 C200E0 K3 D0",
            "DIV16 D0E0 K3 C200",
            "DIV16 D0E0 K3 C255E0",
            "MUL16 D0E0 K3 K5M0",
            "MUL32 D0E0 K3 K9M0",
            "DIV32 D0E0 K3 K1X0",
            "MUL16 X0E0 K3 D0",
            "MUL16 K1E0 K3 D0",
            "MUL16 E0E1 K3 D0",
            "DIV16 D0E8 K3 D0",
        ] {
            let parts = raw
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            assert!(
                IndexedTruncatedAction::parse(&parts[0], &parts[1..]).is_err(),
                "{raw}"
            );
        }
    }

    #[test]
    fn all_sixteen_index_aliases_read_one_signed_word_with_wide_value_sources() {
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
            let action = parse(&format!("DIV32 D5{bank} K3 D20"));
            assert_eq!(
                &action.native_words().unwrap()[1..5],
                &[first, second, 0x0080, 0x0080]
            );
            assert_eq!(action.modifier_devices(), [d(alias)]);
            let mut m = Memory::default();
            m.set_word(d(alias), -2);
            m.set_word(
                Device {
                    index: d(alias).index + 1,
                    ..d(alias)
                },
                12345,
            );
            m.set_double_word(d("D3"), -70000);
            let Action::Wide(result) = prepared(&action, &m) else {
                panic!("wide result")
            };
            assert_eq!(result.sources[0].value().constant(), Some(-70000));
            assert_eq!(result.sources[1].value().constant(), Some(3));
        }
    }

    #[test]
    fn zero_divisor_bypasses_target_and_observation_preserves_source_order() {
        for name in ["DIV16", "DIV16P", "DIV32", "DIV32P"] {
            let action = parse(&format!("{name} D0E0 D10F0 D9999E1"));
            let mut m = Memory::default();
            m.set_word(d("D1028"), 2);
            m.set_word(d("D1029"), 2);
            m.set_word(d("D1182"), 100);
            m.set_double_word(d("D2"), 17);
            let mut observed = BTreeSet::new();
            assert_eq!(
                action.prepare(&m, &mut observed),
                Ok(PreparedMultiply::ZeroDivisor {
                    offset: if name.contains("32") { 9 } else { 5 }
                })
            );
            assert!(observed.contains(&d("D2")) && observed.contains(&d("D12")));
            assert!(!observed.contains(&d("D1182")) && !observed.contains(&d("D9999")));
            m.set_word(d("D1028"), -100);
            observed.clear();
            assert!(action.prepare(&m, &mut observed).is_err());
            assert_eq!(observed, BTreeSet::from([d("D1028")]));
        }
        let action = parse("DIV16 D0E0 K0 D20F0");
        let mut display = BTreeSet::new();
        action.observed_devices(&Memory::default(), &mut display);
        assert!(display.contains(&d("D20")) && !display.contains(&d("D21")));
    }

    #[test]
    fn source_snapshots_and_single_target_survive_index_alias_overwrites() {
        let action = parse("MUL32 D20E0 D30F0 D1182E1");
        let mut m = Memory::default();
        m.set_word(d("D1028"), 1);
        m.set_word(d("D1029"), 2);
        m.set_double_word(d("D21"), 100000);
        m.set_double_word(d("D32"), 100000);
        m.set_double_word(d("D1184"), 77);
        let Action::Wide(result) = prepared(&action, &m) else {
            panic!("wide result")
        };
        m.set_double_word(d("D21"), 7);
        m.set_double_word(d("D32"), 9);
        m.set_word(d("D1182"), 500);
        result.execute(&mut m);
        assert_eq!(m.double_word(d("D1182")), 0x540be400);
        assert_eq!(m.double_word(d("D1184")), 77); // No full-result high chunk.
        assert!(m.system_bit(1022));
    }

    #[test]
    fn short_group_write_is_one_chunk_and_carry_uses_full_instruction_width() {
        for (name, expected, carry) in [
            ("MUL16", 0xad, false),
            ("DIV16", 0xab, false),
            ("MUL32", 0xad, false),
            ("DIV32", 0xab, false),
        ] {
            let action = parse(&format!("{name} K-17 K3 K1M128E0"));
            let mut m = Memory::default();
            for n in 0..8 {
                m.set_bit(
                    Device {
                        area: Area::M,
                        index: 128 + n,
                    },
                    0xa0 & (1 << n) != 0,
                );
            }
            m = run_frozen(prepared(&action, &m), m);
            let actual = (0..8).fold(0u8, |bits, n| {
                bits | (u8::from(m.bit(Device {
                    area: Area::M,
                    index: 128 + n,
                })) << n)
            });
            assert_eq!(actual, expected, "{name}");
            assert_eq!(m.system_bit(1022), carry);
        }
        let action = parse("MUL16 K300 K300 K1M128E0");
        let mut m = Memory::default();
        m.set_system(1067, true);
        let group = run_frozen(prepared(&action, &m), m.clone());
        let numeric = run_frozen(prepared(&parse("MUL16 K300 K300 D20E0"), &m), m);
        assert!(group.system_bit(1022));
        assert!(!numeric.system_bit(1022));
        assert_eq!(numeric.word(d("D20")), 24464);
    }

    #[test]
    fn short_sources_zero_extend_and_full_width_sources_retain_their_sign() {
        for (name, group, value) in [
            ("MUL16", "K1M0E0", 15),
            ("MUL16", "K4M0E0", -1),
            ("MUL32", "K4M0E0", 65535),
            ("MUL32", "K8M0E0", -1),
        ] {
            let action = parse(&format!("{name} {group} K1 D20"));
            let mut m = Memory::default();
            for n in 0..32 {
                m.set_bit(
                    Device {
                        area: Area::M,
                        index: n,
                    },
                    true,
                );
            }
            let frozen = prepared(&action, &m);
            let source = match frozen {
                Action::MultiplyTruncated(a, _, _) => a,
                Action::Wide(w) => w.sources[0].value().clone(),
                _ => panic!("multiply"),
            };
            assert_eq!(source.constant(), Some(value), "{name} {group}");
        }
    }

    #[test]
    fn model_endpoints_validate_one_result_and_allow_negative_offset_rescue() {
        let mut m = Memory::default();
        m.data_register_limit = Some(4999);
        for text in [
            "MUL16 K3 K4 D4999E0",
            "MUL16 K3 K4 T255E0",
            "MUL32 K3 K4 D4998E0",
            "MUL32 K3 K4 T254E0",
            "MUL32 K3 K4 C255E0",
            "MUL16 K3 K4 K1S1020E0",
            "MUL32 K3 K4 K8M4064E0",
        ] {
            assert!(
                parse(text).prepare(&m, &mut BTreeSet::new()).is_ok(),
                "{text}"
            );
        }
        for text in [
            "MUL32 K3 K4 D4999E0",
            "MUL32 K3 K4 T255E0",
            "MUL16 D5000 K3 D0E0",
            "MUL32 D4999 K3 D0E0",
            "MUL16 K3 K4 K1M998E0",
        ] {
            assert!(
                parse(text).prepare(&m, &mut BTreeSet::new()).is_err(),
                "{text}"
            );
        }
        m.set_word(d("D1028"), -1);
        assert!(
            parse("MUL32 K3 K4 D4999E0")
                .prepare(&m, &mut BTreeSet::new())
                .is_ok()
        );
        m.set_word(d("D1028"), 1280);
        assert!(
            parse("MUL16 K3 K4 K1M0E0")
                .prepare(&m, &mut BTreeSet::new())
                .is_err()
        );
        m.set_word(d("D1028"), 0);
        m.relay_limit = Some(255);
        assert!(
            parse("MUL32 K3 K4 K2M252E0")
                .prepare(&m, &mut BTreeSet::new())
                .is_err()
        );
    }

    #[test]
    fn wide_counter_crossings_and_direct_e_keep_canonical_single_result_storage() {
        for (base, offset, effective) in [
            ("C199E0", 1, "C200"),
            ("C200E0", -1, "C199"),
            ("C255E0", 0, "C255"),
        ] {
            let action = parse(&format!("DIV32 K-17 K3 {base}"));
            let mut m = Memory::default();
            m.set_word(d("D1028"), offset);
            m.set_double_word(d("C200"), 77);
            let Action::Wide(result) = prepared(&action, &m) else {
                panic!("wide result")
            };
            result.execute(&mut m);
            assert!(
                matches!(&result.dest, WideDestination::Numeric(target) if target.base() == d(effective))
            );
            assert_eq!(
                if effective == "C199" {
                    i32::from(m.word(d("C199")))
                } else {
                    m.double_word(d(effective))
                },
                -5
            );
            if effective == "C199" {
                assert_eq!(m.double_word(d("C200")), 77);
            }
        }
        let action = parse("MUL32 D0E0 K3 E1");
        let mut m = Memory::default();
        m.set_double_word(d("D0"), 17);
        m.set_double_word(d("D182"), 77);
        let Action::Wide(result) = prepared(&action, &m) else {
            panic!("wide result")
        };
        assert!(
            matches!(&result.dest, WideDestination::Numeric(target) if matches!(target.value(), CalendarValue::Index { index:1, wide:true, high_bank:false }))
        );
        result.execute(&mut m);
        assert_eq!(m.double_word(d("D1182")), 51);
        assert_eq!(m.double_word(d("D182")), 77); // Native mismatch is a separate guard/diagnostic.
    }

    #[test]
    fn static_dependencies_dynamic_accesses_and_display_use_the_real_single_span() {
        let action = parse("MUL32 D5E1 K3 D20F0");
        let mut m = Memory::default();
        m.set_word(d("D1182"), -2);
        m.set_word(d("D1029"), 3);
        m.set_double_word(d("D3"), 7);
        let mut static_devices = BTreeSet::new();
        action.devices(&mut static_devices);
        assert!(static_devices.contains(&d("D1182")) && static_devices.contains(&d("D1029")));
        assert!(!static_devices.contains(&d("D5")) && !static_devices.contains(&d("D20")));
        let mut actual = BTreeSet::new();
        action.prepare(&m, &mut actual).unwrap();
        for raw in ["D3", "D4", "D23", "D24"] {
            assert!(actual.contains(&d(raw)), "{raw}");
        }
        assert!(!actual.contains(&d("D25")));
        m.set_double_word(d("D23"), -51);
        assert_eq!(action.display_value(&m), "-51 C:0");
        assert_eq!(parse("DIV16 D5E1 K3 D20F0").display_value(&m), "Q:-51");
    }
}
