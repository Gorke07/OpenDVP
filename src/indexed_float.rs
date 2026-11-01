//! Opt-in FLT/INT modifiers with separate descriptor and data-access widths.
//! The mode is sampled once; exceptional sources never resolve the target.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::CalendarValue;
use crate::float_ops::{FloatConversionMode, FloatConversionResult};
use crate::indexed_address::IndexedAddress;
use crate::indexed_multiply::validate_model;
use crate::indexed_operand::has_modifier;
use crate::plc::{Area, Device, Memory, Operand, word_range};
use crate::wide::{WideOperand, double_devices};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FloatAddress {
    Direct(Device),
    Modified(IndexedAddress),
}

impl FloatAddress {
    fn parse(raw: &str, float_family: bool) -> Result<Self, String> {
        let value = if has_modifier(raw) {
            Self::Modified(IndexedAddress::parse(raw)?)
        } else {
            Self::Direct(raw.parse()?)
        };
        if if float_family {
            value.base().area != Area::D
        } else {
            !matches!(value.base().area, Area::T | Area::C | Area::D)
        } {
            return Err("FLT indeksli rolleri yalnız D; INT rolleri yalnız T/C/D olabilir".into());
        }
        Ok(value)
    }

    fn base(self) -> Device {
        match self {
            Self::Direct(base) => base,
            Self::Modified(address) => address.base,
        }
    }

    fn modifier(self) -> Option<Device> {
        match self {
            Self::Direct(_) => None,
            Self::Modified(address) => Some(address.index.device()),
        }
    }

    fn direct_span(self, wide: bool) -> Result<(), String> {
        if let Self::Direct(base) = self {
            if wide {
                double_devices(base)?;
            } else {
                word_range(base, 1)?;
            }
        }
        Ok(())
    }

    fn resolve(self, memory: &Memory, wide: bool, write: bool) -> Result<CalendarValue, String> {
        let value = match self {
            Self::Modified(address) => address.resolve(memory, wide, None, write)?,
            Self::Direct(base) => {
                self.direct_span(wide)?;
                if wide {
                    AdditionalValue::Double(WideOperand::Register(base))
                } else {
                    AdditionalValue::Word(Operand::Register(base))
                }
            }
        };
        let mut devices = BTreeSet::new();
        value.devices(&mut devices);
        validate_model(memory, &devices)?;
        Ok(CalendarValue::Additional(value))
    }

    fn static_devices(self, maximum_wide: bool, out: &mut BTreeSet<Device>) {
        if let Some(modifier) = self.modifier() {
            out.insert(modifier);
        } else {
            let base = self.base();
            out.insert(base);
            // A FLT word endpoint may be legal in only one M1081 mode. Static
            // declarations must not panic or fabricate its unavailable high word.
            if maximum_wide && let Ok(span) = double_devices(base) {
                out.extend(span);
            }
        }
    }

    fn native_words(self, descriptor_wide: bool, source_wide: bool) -> Result<Vec<u16>, String> {
        // INT's word instruction still needs the natural-C wide-source marker.
        // Generate a four-word numeric descriptor, then retain the actual field
        // length. This does not turn the index register into a double operand.
        let wide = descriptor_wide || source_wide;
        let mut words = match self {
            Self::Modified(address) => address.native_words(wide, None)?,
            Self::Direct(base) => {
                crate::es2_encoder::calendar_operand(&CalendarValue::Additional(if wide {
                    AdditionalValue::Double(WideOperand::Register(base))
                } else {
                    AdditionalValue::Word(Operand::Register(base))
                }))?
            }
        };
        words.truncate(if descriptor_wide { 4 } else { 2 });
        Ok(words)
    }
}

impl std::fmt::Display for FloatAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Direct(base) => base.fmt(f),
            Self::Modified(address) => address.fmt(f),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreparedFloatResult {
    target: Device,
    conversion: FloatConversionResult,
}

impl PreparedFloatResult {
    fn write(self, memory: &mut Memory) {
        self.conversion.write_value(memory, self.target);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreparedFloat {
    Ready(PreparedFloatResult),
    InvalidSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedFloatAction {
    float_family: bool,
    wide: bool,
    pulse_label: bool,
    source: FloatAddress,
    target: FloatAddress,
}

impl IndexedFloatAction {
    pub fn supports(name: &str) -> bool {
        matches!(
            name.to_ascii_uppercase().as_str(),
            "FLT" | "FLTP" | "DFLT" | "DFLTP" | "INT" | "INTP" | "DINT" | "DINTP"
        )
    }

    pub fn has_modifiers(args: &[String]) -> bool {
        args.iter().any(|raw| has_modifier(raw))
    }

    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if !Self::supports(name) || args.len() != 2 {
            return Err("İndeksli FLT/INT bir kaynak ve bir hedef gerektirir".into());
        }
        let name = name.to_ascii_uppercase();
        let float_family = name.contains("FLT");
        let wide = name.starts_with('D');
        let source = FloatAddress::parse(&args[0], float_family)?;
        let target = FloatAddress::parse(&args[1], float_family)?;
        if !float_family || wide {
            source.direct_span(true)?;
            target.direct_span(wide)?;
        }
        if !float_family && !wide && target.base().area == Area::C && target.base().index >= 200 {
            return Err(
                "INT hedefi 16 bit C0..C199 olmalı; doğal32 sayaç yalnız kaynak olabilir".into(),
            );
        }
        Ok(Self {
            float_family,
            wide,
            pulse_label: name.ends_with('P'),
            source,
            target,
        })
    }

    pub fn mode_device(&self) -> Option<Device> {
        self.float_family.then(|| Memory::system_device(1081))
    }

    fn mode(&self, memory: &Memory) -> FloatConversionMode {
        if self.float_family && !memory.system_bit(1081) {
            FloatConversionMode::Forward { wide: self.wide }
        } else {
            FloatConversionMode::Integer {
                wide: self.wide,
                flt_reverse: self.float_family,
            }
        }
    }

    pub fn resolved_source(&self, memory: &Memory) -> Result<CalendarValue, String> {
        self.source
            .resolve(memory, self.mode(memory).source_wide(), false)
    }

    fn read_conversion(
        &self,
        memory: &Memory,
        mode: FloatConversionMode,
        observed: &mut BTreeSet<Device>,
    ) -> Result<Option<FloatConversionResult>, String> {
        observed.extend(self.mode_device());
        observed.extend(self.source.modifier());
        let source = self.source.resolve(memory, mode.source_wide(), false)?;
        source.devices(observed);
        Ok(mode.convert(source.read(memory)))
    }

    fn resolved_target(
        &self,
        memory: &Memory,
        mode: FloatConversionMode,
    ) -> Result<CalendarValue, String> {
        self.target.resolve(memory, mode.result_wide(), true)
    }

    pub fn prepare(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<PreparedFloat, String> {
        let mode = self.mode(memory);
        let Some(conversion) = self.read_conversion(memory, mode, observed)? else {
            return Ok(PreparedFloat::InvalidSource);
        };
        observed.extend(self.target.modifier());
        let target = self.resolved_target(memory, mode)?;
        target.devices(observed);
        Ok(PreparedFloat::Ready(PreparedFloatResult {
            target: target.base().expect("numeric conversion target"),
            conversion,
        }))
    }

    /// The VM has already applied the instruction's P-edge gate. Disabled visits
    /// do not sample mode/indices, clear flags, or create observation footprints.
    pub fn execute(&self, memory: &mut Memory, power: bool, observed: &mut BTreeSet<Device>) {
        if !power {
            return;
        }
        let mode = self.mode(memory);
        mode.begin(memory);
        let conversion = match self.read_conversion(memory, mode, observed) {
            Ok(Some(conversion)) => conversion,
            Ok(None) => {
                memory.operation_error(0xe1a, 1);
                return;
            }
            Err(reason) => {
                memory.execution_limitations.insert(reason);
                return;
            }
        };
        conversion.publish_flags(memory);
        observed.extend(self.target.modifier());
        match self.resolved_target(memory, mode) {
            Ok(target) => {
                target.devices(observed);
                PreparedFloatResult {
                    target: target.base().expect("numeric target"),
                    conversion,
                }
                .write(memory);
            }
            Err(reason) => {
                memory.execution_limitations.insert(reason);
            }
        }
    }

    pub fn dest(&self) -> Device {
        self.target.base()
    }

    pub fn label(&self) -> String {
        format!(
            "{}{}{} {} {}",
            if self.wide { "D" } else { "" },
            if self.float_family { "FLT" } else { "INT" },
            if self.pulse_label { "P" } else { "" },
            self.source,
            self.target
        )
    }

    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        // Word FLT's possible pair is mode-dependent. Do not let a speculative
        // upper word reject a legal last-word operand during model validation.
        self.source
            .static_devices(!self.float_family || self.wide, out);
        self.target.static_devices(self.wide, out);
        out.extend(self.mode_device());
        out.extend((1020..=1022).map(Memory::system_device));
        Memory::error_devices(out);
    }

    pub fn modifier_devices(&self) -> Vec<Device> {
        self.source
            .modifier()
            .into_iter()
            .chain(self.target.modifier())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        let mode = self.mode(memory);
        out.extend(self.modifier_devices());
        out.extend(self.mode_device());
        out.extend((1020..=1022).map(Memory::system_device));
        Memory::error_devices(out);
        if let Ok(source) = self.source.resolve(memory, mode.source_wide(), false) {
            source.devices(out);
        }
        if let Ok(target) = self.resolved_target(memory, mode) {
            target.devices(out);
        }
    }

    pub fn display_value(&self, memory: &Memory) -> String {
        let mode = self.mode(memory);
        match self.resolved_target(memory, mode) {
            Ok(target) if matches!(mode, FloatConversionMode::Forward { .. }) => {
                f32::from_bits(target.read(memory) as u32).to_string()
            }
            Ok(target) => target.read(memory).to_string(),
            Err(reason) => format!("İndeks: {reason}"),
        }
    }

    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let mut header = match (self.float_family, self.wide) {
            (true, false) => 0x7200,
            (true, true) => 0x7300,
            (false, false) => 0x2201,
            (false, true) => 0x2301,
        };
        if self.pulse_label {
            header |= 0x10;
        }
        let mut words = vec![header];
        words.extend(self.source.native_words(self.wide, !self.float_family)?);
        words.extend(self.target.native_words(self.wide, false)?);
        Ok(words)
    }
}
