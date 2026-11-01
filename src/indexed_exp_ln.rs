//! Indexed DEXP/DLN, PDF586/587: signed K/H or raw D-pair source,
//! D-pair target. Domain errors precede flags and all destination accesses.
use crate::calendar_operand::CalendarValue;
use crate::float_advanced::{
    AdvancedOp, AdvancedResult, ExpLnInput, evaluate_exp_ln, validate_exp_ln,
};
use crate::float_math::FloatBinaryError;
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
            "DEXP/DLN kaynağı K/H/D, hedefi D olmalı; yalnız D adresine E/F son eki eklenebilir"
                .into(),
        );
    }
    Ok(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreparedExpLnResult {
    target: Device,
    result: AdvancedResult,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreparedExpLn {
    Ready(PreparedExpLnResult),
    SemanticError { code: i16, offset: u16 },
}
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedExpLnAction {
    op: AdvancedOp,
    pulse_label: bool,
    source: AddressedValue,
    target: AddressedValue,
}
impl IndexedExpLnAction {
    pub fn supports(name: &str) -> bool {
        matches!(
            name.to_ascii_uppercase().as_str(),
            "DEXP" | "DEXPP" | "DLN" | "DLNP"
        )
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        args.iter().any(|raw| has_modifier(raw))
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if !Self::supports(name) || args.len() != 2 {
            return Err("İndeksli DEXP/DLN bir kaynak ve bir hedef gerektirir".into());
        }
        let name = name.to_ascii_uppercase();
        // DEXP's final P is part of the ordinary mnemonic; only DEXPP is
        // its pulse form. Do not apply generic suffix stripping here.
        let (op, pulse_label) = match name.as_str() {
            "DEXP" => (AdvancedOp::Exp, false),
            "DEXPP" => (AdvancedOp::Exp, true),
            "DLN" => (AdvancedOp::Ln, false),
            "DLNP" => (AdvancedOp::Ln, true),
            _ => unreachable!("supported Exp/Ln operation"),
        };
        Ok(Self {
            op,
            pulse_label,
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
    fn snapshot(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<ExpLnInput, String> {
        observed.extend(self.source_modifier_devices());
        let source = self.resolved_source(memory)?;
        source.devices(observed);
        Ok(match source.constant() {
            Some(integer) => ExpLnInput::Integer(integer),
            None => ExpLnInput::RawBinary32(source.read(memory) as u32),
        })
    }
    /// Domain validation only, after native source capture and before target
    /// modifier capture. It does not run the exponential/logarithm kernel.
    pub fn semantic_error(&self, memory: &Memory) -> Result<Option<FloatBinaryError>, String> {
        let source = self.snapshot(memory, &mut BTreeSet::new())?;
        Ok(validate_exp_ln(self.op, source).err())
    }
    pub fn prepare(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<PreparedExpLn, String> {
        let source = self.snapshot(memory, observed)?;
        let result = match evaluate_exp_ln(self.op, source) {
            Ok(result) => result,
            Err(error) => {
                return Ok(PreparedExpLn::SemanticError {
                    code: error.code,
                    offset: error.offset,
                });
            }
        };
        observed.extend(self.target_modifier_devices());
        let target = self.resolved_target(memory)?;
        target.devices(observed);
        Ok(PreparedExpLn::Ready(PreparedExpLnResult {
            target: target.base().expect("D result"),
            result,
        }))
    }
    /// P gating belongs to the caller. Errors keep all arithmetic flags;
    /// success normalizes before resolving the once-cached destination pair.
    pub fn execute(&self, memory: &mut Memory, power: bool, observed: &mut BTreeSet<Device>) {
        if !power {
            return;
        }
        let source = match self.snapshot(memory, observed) {
            Ok(source) => source,
            Err(reason) => {
                memory.execution_limitations.insert(reason);
                return;
            }
        };
        let result = match evaluate_exp_ln(self.op, source) {
            Ok(result) => result,
            Err(error) => {
                memory.operation_error(error.code, error.offset);
                return;
            }
        };
        result.publish_flags(memory);
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
    /// UI may inspect a held target independently of domain validity; actual
    /// execution observations come from the staged paths above.
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
