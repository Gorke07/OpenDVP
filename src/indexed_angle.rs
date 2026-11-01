//! DRAD/DDEG indexed D operands, PDF577/578. K/H sources are signed
//! integers; D pairs contain raw binary32. Both fields occupy four words.
use crate::calendar_operand::CalendarValue;
use crate::float_math::{FloatBinaryInput, FloatBinaryResult, MathOp, evaluate_angle};
use crate::indexed_multiply::validate_model;
use crate::indexed_operand::{AddressedValue, has_modifier};
use crate::plc::{Area, Device, Memory};
use std::collections::BTreeSet;

fn binding(raw: &str, write: bool) -> Result<AddressedValue, String> {
    let value = AddressedValue::parse(raw, true, write)?;
    let allowed = match &value {
        AddressedValue::Direct(CalendarValue::Additional(value)) => {
            (!write && value.base().is_none()) || value.base().is_some_and(|d| d.area == Area::D)
        }
        AddressedValue::Modified {
            address,
            digits: None,
            ..
        } => address.base.area == Area::D,
        _ => false,
    };
    if !allowed {
        return Err(
            "DRAD/DDEG kaynağı K/H/D, hedefi D olmalı; yalnız D adresine E/F son eki eklenebilir"
                .into(),
        );
    }
    Ok(value)
}

/// A pure preflight snapshot. The fixed target cannot be redirected by
/// writing the low half when that location is also an index register.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreparedAngle {
    target: Device,
    result: FloatBinaryResult,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedAngleAction {
    op: MathOp,
    pulse_label: bool,
    source: AddressedValue,
    target: AddressedValue,
}

impl IndexedAngleAction {
    pub fn supports(name: &str) -> bool {
        matches!(
            name.to_ascii_uppercase().as_str(),
            "DRAD" | "DRADP" | "DDEG" | "DDEGP"
        )
    }

    pub fn has_modifiers(args: &[String]) -> bool {
        args.iter().any(|raw| has_modifier(raw))
    }

    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if !Self::supports(name) || args.len() != 2 {
            return Err("İndeksli açı dönüşümü bir kaynak ve bir hedef gerektirir".into());
        }
        let name = name.to_ascii_uppercase();
        Ok(Self {
            op: MathOp::parse(name.strip_suffix('P').unwrap_or(&name))
                .expect("supported angle operation"),
            pulse_label: name.ends_with('P'),
            source: binding(&args[0], false)?,
            target: binding(&args[1], true)?,
        })
    }

    pub fn source(&self) -> &AddressedValue {
        &self.source
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

    fn result(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<FloatBinaryResult, String> {
        observed.extend(self.source.modifier_devices());
        let source = self.resolved_source(memory)?;
        source.devices(observed);
        let value = match source.constant() {
            Some(integer) => FloatBinaryInput::Integer(integer),
            None => FloatBinaryInput::RawBinary32(source.read(memory) as u32),
        };
        Ok(evaluate_angle(self.op, value))
    }

    /// Pure validation/capture. Angles have no separate R-style input NaN
    /// error path: any structurally valid input proceeds to normalization.
    pub fn prepare(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<PreparedAngle, String> {
        let result = self.result(memory, observed)?;
        observed.extend(self.target.modifier_devices());
        let target = self.resolved_target(memory)?;
        target.devices(observed);
        Ok(PreparedAngle {
            target: target.base().expect("D result"),
            result,
        })
    }

    /// Caller applies the P gate. Source failures preserve flags; a valid
    /// calculation publishes normalized flags before resolving the target.
    pub fn execute(&self, memory: &mut Memory, power: bool, observed: &mut BTreeSet<Device>) {
        if !power {
            return;
        }
        let result = match self.result(memory, observed) {
            Ok(result) => result,
            Err(reason) => {
                memory.execution_limitations.insert(reason);
                return;
            }
        };
        result.publish_flags(memory);
        observed.extend(self.target.modifier_devices());
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
        self.source
            .modifier_devices()
            .into_iter()
            .chain(self.target.modifier_devices())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// Monitoring resolves the held result independently of source validity.
    /// Only execute/prepare footprints describe actual instruction accesses.
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
            Ok(target) => f32::from_bits(target.read(memory) as u32).to_string(),
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
