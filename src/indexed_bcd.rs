//! Opt-in E/F address modifiers for BCD/BIN and their double/pulse forms.
//! Conversion validity precedes destination resolution: all four installed
//! handlers report E18 at header+1 without reading the target index on failure.
//! The ordinary conversion action remains the owner of result/flag semantics.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::{CalendarDestination, CalendarValue};
use crate::indexed_multiply::{qualify_group, validate_model};
use crate::indexed_operand::{AddressedValue, has_modifier};
use crate::plc::{Action, Device, Memory, Operand, WordConversion};
use crate::wide::{WideAction, WideDestination, WideOp, WideOperand, WideSource};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub enum PreparedBcd {
    Ready(Box<Action>),
    /// The caller records E18 at native instruction header+1. The target and
    /// its modifier were not accessed; this is not an address limitation.
    InvalidSource,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedBcdAction {
    conversion: WordConversion,
    wide: bool,
    pulse_label: bool,
    source: AddressedValue,
    target: AddressedValue,
}

impl IndexedBcdAction {
    pub fn supports(name: &str) -> bool {
        matches!(
            name.to_ascii_uppercase().as_str(),
            "BCD" | "BCDP" | "DBCD" | "DBCDP" | "BIN" | "BINP" | "DBIN" | "DBINP"
        )
    }

    pub fn has_modifiers(args: &[String]) -> bool {
        args.iter().any(|raw| has_modifier(raw))
    }

    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if !Self::supports(name) || args.len() != 2 {
            return Err("İndeksli BCD/BIN: bir kaynak ve bir sonuç hedefi gerekli".into());
        }
        let name = name.to_ascii_uppercase();
        let wide = name.starts_with('D');
        let source = AddressedValue::parse(&args[0], wide, false)?;
        if matches!(&source, AddressedValue::Direct(value) if value.constant().is_some()) {
            return Err(
                "BCD/BIN kaynağı Kn grubu, T/C/D veya izinli E/F olmalı; K/H sabiti kullanılamaz"
                    .into(),
            );
        }
        Ok(Self {
            conversion: if name.contains("BCD") {
                WordConversion::Bcd
            } else {
                WordConversion::Bin
            },
            wide,
            pulse_label: name.ends_with('P'),
            source,
            target: AddressedValue::parse(&args[1], wide, true)?,
        })
    }

    pub fn source(&self) -> &AddressedValue {
        &self.source
    }

    /// Resolve and validate real storage before a native preflight captures it.
    pub fn resolved_source(&self, memory: &Memory) -> Result<CalendarValue, String> {
        let value = self.source.resolve(memory, false)?;
        let mut devices = BTreeSet::new();
        value.devices(&mut devices);
        validate_model(memory, &devices)?;
        self.qualify_group(&self.source, &value)?;
        Ok(value)
    }

    fn qualify_group(&self, binding: &AddressedValue, value: &CalendarValue) -> Result<(), String> {
        if let CalendarValue::Additional(AdditionalValue::Group { base, digits, .. }) = value {
            // BCD/BIN use the generic scalar reader AND writer, not the
            // multiplication result resolver's different M1280 threshold.
            qualify_group(
                binding.base().expect("group base"),
                *base,
                digits * 4,
                false,
            )?;
        }
        Ok(())
    }

    fn resolved_target(&self, memory: &Memory) -> Result<CalendarDestination, String> {
        let value = self.target.resolve(memory, true)?;
        self.qualify_group(&self.target, &value)?;
        let target = CalendarDestination::from_value(value)?;
        let mut devices = BTreeSet::new();
        target.devices(&mut devices);
        validate_model(memory, &devices)?;
        Ok(target)
    }

    fn source_is_valid(&self, value: i32) -> bool {
        if !self.wide {
            return self.conversion.apply(value as i16).is_some();
        }
        match self.conversion {
            WordConversion::Bcd => (0..=99_999_999).contains(&value),
            WordConversion::Bin => (0..8).all(|digit| ((value as u32 >> (digit * 4)) & 15) <= 9),
        }
    }

    pub fn prepare(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<PreparedBcd, String> {
        observed.extend(self.source.modifier_devices());
        let source = self.resolved_source(memory)?;
        let snapshot = crate::indexed_operand::read_resolved_value(&source, memory, observed)?;
        if !self.source_is_valid(snapshot) {
            return Ok(PreparedBcd::InvalidSource);
        }
        observed.extend(self.target.modifier_devices());
        let target = self.resolved_target(memory)?;
        target.devices(observed);
        let frozen = CalendarValue::Additional(if self.wide {
            AdditionalValue::Double(WideOperand::Constant(snapshot))
        } else {
            AdditionalValue::Word(Operand::Constant(snapshot as i16))
        });
        let action = if self.wide {
            Action::Wide(WideAction {
                op: match self.conversion {
                    WordConversion::Bcd => WideOp::Bcd,
                    WordConversion::Bin => WideOp::Bin,
                },
                sources: vec![WideSource::Read(frozen)],
                dest: WideDestination::Numeric(target),
                count: 0,
            })
        } else {
            Action::Convert(self.conversion.clone(), frozen, target)
        };
        Ok(PreparedBcd::Ready(Box::new(action)))
    }

    /// Static source label only; execution and displays use the resolved target.
    pub fn dest(&self) -> Device {
        self.target.base().expect("validated BCD/BIN target")
    }

    pub fn label(&self) -> String {
        format!(
            "{}{}{} {} {}",
            if self.wide { "D" } else { "" },
            self.conversion.name(),
            if self.pulse_label { "P" } else { "" },
            self.source,
            self.target,
        )
    }

    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        self.source.devices(out);
        self.target.devices(out);
        Memory::error_devices(out);
    }

    pub fn modifier_devices(&self) -> Vec<Device> {
        self.source
            .modifier_devices()
            .into_iter()
            .chain(self.target.modifier_devices())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// UI collection includes the held target independently of source validity;
    /// prepare() instead records only accesses made by the current execution.
    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Ok(source) = self.resolved_source(memory) {
            source.devices(out);
        }
        if let Ok(target) = self.resolved_target(memory) {
            target.devices(out);
        }
    }

    pub fn display_value(&self, memory: &Memory) -> String {
        match self.resolved_target(memory) {
            Ok(target) => target.read(memory).to_string(),
            Err(reason) => format!("İndeks: {reason}"),
        }
    }

    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let mut header = match (&self.conversion, self.wide) {
            (WordConversion::Bcd, false) => 0x3400,
            (WordConversion::Bcd, true) => 0x3500,
            (WordConversion::Bin, false) => 0x3600,
            (WordConversion::Bin, true) => 0x3700,
        };
        if self.pulse_label {
            header |= 0x10;
        }
        let mut words = vec![header];
        words.extend(self.source.native_words()?);
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

    fn parse(source: &str) -> IndexedBcdAction {
        let parts = source
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        IndexedBcdAction::parse(&parts[0], &parts[1..]).unwrap()
    }

    #[test]
    fn eight_literal_headers_and_descriptors_preserve_original_indices() {
        for (name, header, wide) in [
            ("BCD", 0x3400, false),
            ("BCDP", 0x3410, false),
            ("DBCD", 0x3500, true),
            ("DBCDP", 0x3510, true),
            ("BIN", 0x3600, false),
            ("BINP", 0x3610, false),
            ("DBIN", 0x3700, true),
            ("DBINP", 0x3710, true),
        ] {
            let text = format!("{name} D5E1 K1M128F7");
            let action = parse(&text);
            let expected = if wide {
                vec![
                    header, 0x0ae6, 0x8086, 0x0080, 0x0080, 0x80f4, 0xe882, 0x0080, 0x0080,
                ]
            } else {
                vec![header, 0x0ae6, 0x8086, 0x80f4, 0xe882]
            };
            assert_eq!(action.native_words().unwrap(), expected, "{name}");
            assert_eq!(action.label(), text);
        }
        assert_eq!(
            parse("BIN F1 D20E0").native_words().unwrap(),
            [0x3600, 0x6e86, 0x0180, 0x28e6, 0x0086]
        );
    }

    #[test]
    fn matrix_keeps_constants_bare_bits_and_wide_direct_f_forbidden() {
        for text in [
            "BCD K1 D0E0",
            "DBIN H1234 D0E0",
            "BCD X0E0 D0",
            "BIN D0E0 Y0",
            "BCD D0E0 K1X0",
            "BIN D0E0 K1X0E1",
            "DBCD F0 D0E0",
            "DBIN D0E0 F1",
            "BCD C200E0 D0",
            "BIN D0E0 C200",
            "DBCD E0E1 D0",
            "BCDP D0E8 D0",
        ] {
            let parts = text
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            assert!(
                IndexedBcdAction::parse(&parts[0], &parts[1..]).is_err(),
                "{text}"
            );
        }
        for text in [
            "BCD E1 D0F7",
            "BIN F7 D0E0",
            "BCD D0E0 F0",
            "DBCD E1 D0F7",
            "DBIN D0F7 E1",
        ] {
            let _ = parse(text);
        }
        for name in ["BCDP", "DBINP"] {
            assert!(IndexedBcdAction::supports(name));
        }
        for name in ["BCDP P", "BCDPP", "BINPP", "DEBCD", "MUL16"] {
            assert!(!IndexedBcdAction::supports(name));
        }
    }

    #[test]
    fn invalid_source_never_observes_or_resolves_target_index() {
        for (name, value) in [
            ("BCD", -1),
            ("BIN", 0xa000),
            ("DBCD", 100_000_000),
            ("DBIN", 0xa000_0000u32 as i32),
        ] {
            let action = parse(&format!("{name} D10E0 D100F7"));
            let mut memory = Memory::default();
            memory.set_word(d("D1028"), 1);
            memory.set_word(d("D1195"), -30000);
            if name.starts_with('D') {
                memory.set_double_word(d("D11"), value);
            } else {
                memory.set_word(d("D11"), value as i16);
            }
            let mut observed = BTreeSet::new();
            assert_eq!(
                action.prepare(&memory, &mut observed).unwrap(),
                PreparedBcd::InvalidSource
            );
            assert!(observed.contains(&d("D1028")) && observed.contains(&d("D11")));
            assert!(!observed.contains(&d("D1195")) && !observed.contains(&d("D100")));
        }
    }

    #[test]
    fn validation_checks_all_digits_before_any_small_target_truncation() {
        let memory = Memory::default();
        for (text, valid, invalid) in [
            ("BCD D0E0 K1M128E1", 9999, 10000),
            ("BIN D0E0 K1M128E1", 0x9999, 0xa009),
            ("DBCD D0E0 K1M128E1", 99_999_999, 100_000_000),
            (
                "DBIN D0E0 K1M128E1",
                0x9999_9999u32 as i32,
                0xa000_0009u32 as i32,
            ),
        ] {
            let action = parse(text);
            for (value, expected) in [(valid, true), (invalid, false)] {
                let mut memory = memory.clone();
                if text.starts_with('D') {
                    memory.set_double_word(d("D0"), value);
                } else {
                    memory.set_word(d("D0"), value as i16);
                }
                let prepared = action.prepare(&memory, &mut BTreeSet::new()).unwrap();
                assert_eq!(
                    matches!(prepared, PreparedBcd::Ready(_)),
                    expected,
                    "{text} {value}"
                );
            }
        }
    }

    #[test]
    fn prepared_source_and_destination_are_frozen_while_ui_observes_held_result() {
        let action = parse("DBCD D10E0 D1027F0");
        let mut memory = Memory::default();
        memory.set_word(d("D1028"), 2);
        memory.set_word(d("D1029"), 1);
        memory.set_double_word(d("D12"), 12_345_678);
        let PreparedBcd::Ready(prepared) = action.prepare(&memory, &mut BTreeSet::new()).unwrap()
        else {
            panic!("ready");
        };
        memory.set_double_word(d("D12"), 99);
        memory.set_word(d("D1029"), 20);
        let Action::Wide(wide) = *prepared else {
            panic!("wide");
        };
        assert_eq!(wide.sources[0].value().constant(), Some(12_345_678));
        let WideDestination::Numeric(target) = wide.dest else {
            panic!("numeric");
        };
        assert_eq!(target.base(), d("D1028"));

        let held = parse("BCD D10E0 D100F0");
        memory.set_word(d("D1029"), 3);
        memory.set_word(d("D12"), -1);
        memory.set_word(d("D103"), 77);
        assert_eq!(held.display_value(&memory), "77");
        let mut observed = BTreeSet::new();
        held.observed_devices(&memory, &mut observed);
        assert!(observed.contains(&d("D103")));
        assert_eq!(
            held.prepare(&memory, &mut BTreeSet::new()).unwrap(),
            PreparedBcd::InvalidSource
        );
    }

    #[test]
    fn complete_model_spans_and_counter_widths_are_qualified() {
        let mut memory = Memory::default();
        memory.data_register_limit = Some(4999);
        memory.set_word(d("D0"), 12);
        for (text, valid) in [
            ("BCD D0E0 D4999", true),
            ("DBCD D0E0 D4999E1", false),
            ("DBCD D0E0 C199E1", true),
            ("DBCD D0E0 C255E1", true),
            ("BCD D0E0 K1M998E1", false),
            ("BCD D0E0 K1M1284E1", false),
            ("BCD D5000 D20E1", false),
            ("DBCD D0E0 T255E1", false),
        ] {
            assert_eq!(
                parse(text).prepare(&memory, &mut BTreeSet::new()).is_ok(),
                valid,
                "{text}"
            );
        }
        memory.set_word(d("D1182"), -1);
        assert!(
            parse("DBCD D0E0 D4999E1")
                .prepare(&memory, &mut BTreeSet::new())
                .is_ok()
        );
        memory.set_word(d("D1182"), 1);
        assert!(
            parse("BCD D0E0 C199E1")
                .prepare(&memory, &mut BTreeSet::new())
                .is_err()
        );
    }
}
