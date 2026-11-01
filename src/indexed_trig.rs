//! Twelve trig/inverse modifier forms, PDF594..604. Only forward trig
//! observes M1018; inverse domains/outputs do not inherit degree mode.
use crate::calendar_operand::CalendarValue;
use crate::float_advanced::{
    AdvancedOp, AdvancedResult, TrigInput, evaluate_trig, trig_uses_degrees, validate_trig,
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
            "Trigonometrik kaynak K/H/D, hedef D olmalı; yalnız D adresine E/F son eki eklenebilir"
                .into(),
        );
    }
    Ok(value)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreparedTrigResult {
    target: Device,
    result: AdvancedResult,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreparedTrig {
    Ready(PreparedTrigResult),
    SemanticError { code: i16, offset: u16 },
}
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedTrigAction {
    op: AdvancedOp,
    pulse_label: bool,
    source: AddressedValue,
    target: AddressedValue,
}
impl IndexedTrigAction {
    pub fn supports(name: &str) -> bool {
        matches!(
            name.to_ascii_uppercase().as_str(),
            "DSIN"
                | "DSINP"
                | "DCOS"
                | "DCOSP"
                | "DTAN"
                | "DTANP"
                | "DASIN"
                | "DASINP"
                | "DACOS"
                | "DACOSP"
                | "DATAN"
                | "DATANP"
        )
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        args.iter().any(|raw| has_modifier(raw))
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if !Self::supports(name) || args.len() != 2 {
            return Err("İndeksli trigonometrik komut bir kaynak ve bir hedef gerektirir".into());
        }
        let name = name.to_ascii_uppercase();
        Ok(Self {
            op: AdvancedOp::parse(name.strip_suffix('P').unwrap_or(&name))
                .expect("supported trig operation"),
            pulse_label: name.ends_with('P'),
            source: binding(&args[0], false)?,
            target: binding(&args[1], true)?,
        })
    }
    pub fn mode_device(&self) -> Option<Device> {
        trig_uses_degrees(self.op).then(|| Memory::system_device(1018))
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
    ) -> Result<TrigInput, String> {
        observed.extend(self.source_modifier_devices());
        let source = self.resolved_source(memory)?;
        source.devices(observed);
        Ok(match source.constant() {
            Some(integer) => TrigInput::Integer(integer),
            None => TrigInput::RawBinary32(source.read(memory) as u32),
        })
    }
    /// Domain validation needs no degree mode or numerical kernel. Only the
    /// inverse operations can reject an otherwise valid source operand.
    pub fn semantic_error(&self, memory: &Memory) -> Result<Option<FloatBinaryError>, String> {
        Ok(validate_trig(self.op, self.snapshot(memory, &mut BTreeSet::new())?).err())
    }
    fn outcome(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<Result<AdvancedResult, FloatBinaryError>, String> {
        let source = self.snapshot(memory, observed)?;
        let degrees = if let Some(device) = self.mode_device() {
            observed.insert(device);
            memory.system_bit(1018)
        } else {
            false
        };
        Ok(evaluate_trig(self.op, source, degrees))
    }
    pub fn prepare(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<PreparedTrig, String> {
        let result = match self.outcome(memory, observed)? {
            Ok(result) => result,
            Err(error) => {
                return Ok(PreparedTrig::SemanticError {
                    code: error.code,
                    offset: error.offset,
                });
            }
        };
        observed.extend(self.target_modifier_devices());
        let target = self.resolved_target(memory)?;
        target.devices(observed);
        Ok(PreparedTrig::Ready(PreparedTrigResult {
            target: target.base().expect("D result"),
            result,
        }))
    }
    /// Caller owns P gating. Source then forward mode are sampled once;
    /// normalization precedes the single result-pair resolution.
    pub fn execute(&self, memory: &mut Memory, power: bool, observed: &mut BTreeSet<Device>) {
        if !power {
            return;
        }
        let result = match self.outcome(memory, observed) {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => {
                memory.operation_error(error.code, error.offset);
                return;
            }
            Err(reason) => {
                memory.execution_limitations.insert(reason);
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
        if let Some(mode) = self.mode_device() {
            out.insert(mode);
        }
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
    /// UI independently resolves held sources/results; only execution paths
    /// define actual visits to source/mode/destination storage.
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
