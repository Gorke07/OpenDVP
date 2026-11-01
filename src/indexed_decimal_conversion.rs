//! DEBCD/DEBIN D-only modifiers (PDF579/580). Conversion effects precede
//! target resolution; invalid DEBIN input never visits the destination.
use crate::calendar_operand::CalendarValue;
use crate::float_math::{DecimalConversionResult, DecimalScratchProfile, FloatBinaryError, MathOp};
use crate::indexed_multiply::validate_model;
use crate::indexed_operand::{AddressedValue, has_modifier};
use crate::plc::{Area, Device, Memory};
use std::collections::BTreeSet;

fn binding(raw: &str, write: bool) -> Result<AddressedValue, String> {
    let value = AddressedValue::parse(raw, true, write)?;
    let allowed = match &value {
        AddressedValue::Direct(CalendarValue::Additional(value)) => {
            value.base().is_some_and(|d| d.area == Area::D)
        }
        AddressedValue::Modified {
            address,
            digits: None,
            ..
        } => address.base.area == Area::D,
        _ => false,
    };
    if !allowed {
        return Err("DEBCD/DEBIN kaynağı ve hedefi D çifti olmalı; yalnız D adresine E/F son eki eklenebilir".into());
    }
    Ok(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreparedDecimalConversionResult {
    target: Device,
    result: DecimalConversionResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreparedDecimalConversion {
    Ready(PreparedDecimalConversionResult),
    InvalidSource,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedDecimalConversionAction {
    op: MathOp,
    pulse_label: bool,
    source: AddressedValue,
    target: AddressedValue,
}

impl IndexedDecimalConversionAction {
    pub fn supports(name: &str) -> bool {
        matches!(
            name.to_ascii_uppercase().as_str(),
            "DEBCD" | "DEBCDP" | "DEBIN" | "DEBINP"
        )
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        args.iter().any(|raw| has_modifier(raw))
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if !Self::supports(name) || args.len() != 2 {
            return Err("İndeksli DEBCD/DEBIN bir kaynak ve bir hedef gerektirir".into());
        }
        let name = name.to_ascii_uppercase();
        Ok(Self {
            op: MathOp::parse(name.strip_suffix('P').unwrap_or(&name))
                .expect("supported conversion"),
            pulse_label: name.ends_with('P'),
            source: binding(&args[0], false)?,
            target: binding(&args[1], true)?,
        })
    }
    pub fn source(&self) -> &AddressedValue {
        &self.source
    }
    pub fn source_modifier_devices(&self) -> Vec<Device> {
        self.source.modifier_devices()
    }
    pub fn target_modifier_devices(&self) -> Vec<Device> {
        self.target.modifier_devices()
    }

    pub fn resolved_source(&self, memory: &Memory) -> Result<CalendarValue, String> {
        let source = self.source.resolve(memory, false)?;
        let mut devices = BTreeSet::new();
        source.devices(&mut devices);
        validate_model(memory, &devices)?;
        Ok(source)
    }
    fn resolved_target(&self, memory: &Memory) -> Result<CalendarValue, String> {
        let target = self.target.resolve(memory, true)?;
        let mut devices = BTreeSet::new();
        target.devices(&mut devices);
        validate_model(memory, &devices)?;
        Ok(target)
    }
    fn snapshot(&self, memory: &Memory, observed: &mut BTreeSet<Device>) -> Result<u32, String> {
        observed.extend(self.source_modifier_devices());
        let source = self.resolved_source(memory)?;
        source.devices(observed);
        Ok(source.read(memory) as u32)
    }

    /// Native preflight calls this after capturing the actual source, before
    /// it reads the target's index. No arithmetic or profile effects occur.
    pub fn semantic_error(&self, memory: &Memory) -> Result<Option<FloatBinaryError>, String> {
        let raw = self.snapshot(memory, &mut BTreeSet::new())?;
        Ok(DecimalConversionResult::validate(self.op, raw).err())
    }

    pub fn prepare(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<PreparedDecimalConversion, String> {
        let raw = self.snapshot(memory, observed)?;
        let result = match DecimalConversionResult::evaluate(self.op, raw) {
            Ok(result) => result,
            Err(_) => return Ok(PreparedDecimalConversion::InvalidSource),
        };
        result.observed_effects(memory, observed);
        // Legacy diagnostics cannot alias any E/F backing word. Their pure
        // preflight footprint therefore leaves this resolution unchanged.
        observed.extend(self.target_modifier_devices());
        let target = self.resolved_target(memory)?;
        target.devices(observed);
        Ok(PreparedDecimalConversion::Ready(
            PreparedDecimalConversionResult {
                target: target.base().expect("D result"),
                result,
            },
        ))
    }

    /// P gating is caller-owned. Clear flags before reading the source;
    /// apply conversion/profile effects before resolving one result pair.
    pub fn execute(&self, memory: &mut Memory, power: bool, observed: &mut BTreeSet<Device>) {
        if !power {
            return;
        }
        DecimalConversionResult::begin(memory);
        let raw = match self.snapshot(memory, observed) {
            Ok(raw) => raw,
            Err(reason) => {
                memory.execution_limitations.insert(reason);
                return;
            }
        };
        let result = match DecimalConversionResult::evaluate(self.op, raw) {
            Ok(result) => result,
            Err(error) => {
                memory.operation_error(error.code, error.offset);
                return;
            }
        };
        result.observed_effects(memory, observed);
        result.publish_before_target(memory);
        observed.extend(self.target_modifier_devices());
        match self.resolved_target(memory) {
            Ok(target) => {
                target.devices(observed);
                result.write_value(memory, target.base().expect("D result"));
            }
            Err(reason) => {
                memory.execution_limitations.insert(reason);
            }
        }
    }

    pub fn dest(&self) -> Device {
        self.target.base().expect("validated D result")
    }
    pub fn label(&self) -> String {
        format!(
            "{}{} {} {}",
            self.op.name(),
            if self.pulse_label { "P" } else { "" },
            self.source,
            self.target
        )
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        self.source.devices(out);
        self.target.devices(out);
        out.extend((1020..=1022).map(Memory::system_device));
        Memory::error_devices(out);
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        self.source_modifier_devices()
            .into_iter()
            .chain(self.target_modifier_devices())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    /// UI can observe a held result and legacy diagnostics independently of
    /// current source validity. Actual execution uses the footprint above.
    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Ok(source) = self.resolved_source(memory) {
            source.devices(out);
        }
        if let Ok(target) = self.resolved_target(memory) {
            target.devices(out);
        }
        if self.op == MathOp::Decimal
            && memory.decimal_scratch_profile == DecimalScratchProfile::LegacyVisible
            && memory.data_register_limit.unwrap_or(9999) >= 5001
        {
            out.extend([
                Device {
                    area: Area::D,
                    index: 5000,
                },
                Device {
                    area: Area::D,
                    index: 5001,
                },
            ]);
        }
    }
    pub fn display_value(&self, memory: &Memory) -> String {
        match self.resolved_target(memory) {
            Ok(target) => {
                let raw = target.read(memory) as u32;
                if self.op == MathOp::Decimal {
                    format!("{} × 10^{}", raw as i16, (raw >> 16) as i16)
                } else {
                    f32::from_bits(raw).to_string()
                }
            }
            Err(reason) => format!("İndeks: {reason}"),
        }
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let mut words = vec![self.op.header() | if self.pulse_label { 0x10 } else { 0 }];
        words.extend(self.source.native_words()?);
        words.extend(self.target.native_words()?);
        Ok(words)
    }
}
