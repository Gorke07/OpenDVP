//! DLOG/DPOW indexed D roles, PDF588/591. Validate the base before visiting
//! the argument/exponent; preserve integer versus raw-binary32 provenance.
use crate::calendar_operand::CalendarValue;
use crate::float_advanced::{
    AdvancedOp, AdvancedResult, LogPowInput, evaluate_log_pow, validate_log_pow_left,
    validate_log_pow_pair,
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
        return Err("DLOG/DPOW kaynakları K/H/D, hedefi D olmalı; yalnız D adresine E/F son eki eklenebilir".into());
    }
    Ok(value)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreparedLogPowResult {
    target: Device,
    result: AdvancedResult,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreparedLogPow {
    Ready(PreparedLogPowResult),
    SemanticError { code: i16, offset: u16 },
}
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedLogPowAction {
    op: AdvancedOp,
    pulse_label: bool,
    sources: [AddressedValue; 2],
    target: AddressedValue,
}

impl IndexedLogPowAction {
    pub fn supports(name: &str) -> bool {
        matches!(
            name.to_ascii_uppercase().as_str(),
            "DLOG" | "DLOGP" | "DPOW" | "DPOWP"
        )
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        args.iter().any(|raw| has_modifier(raw))
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if !Self::supports(name) || args.len() != 3 {
            return Err("İndeksli DLOG/DPOW iki kaynak ve bir hedef gerektirir".into());
        }
        let name = name.to_ascii_uppercase();
        Ok(Self {
            op: AdvancedOp::parse(name.strip_suffix('P').unwrap_or(&name))
                .expect("supported Log/Pow"),
            pulse_label: name.ends_with('P'),
            sources: [binding(&args[0], false)?, binding(&args[1], false)?],
            target: binding(&args[2], true)?,
        })
    }
    pub fn sources(&self) -> &[AddressedValue; 2] {
        &self.sources
    }
    pub fn source_modifier_devices(&self, index: usize) -> Vec<Device> {
        self.sources
            .get(index)
            .map(AddressedValue::modifier_devices)
            .unwrap_or_default()
    }
    pub fn target_modifier_devices(&self) -> Vec<Device> {
        self.target.modifier_devices()
    }
    pub fn resolved_source(&self, index: usize, memory: &Memory) -> Result<CalendarValue, String> {
        let source = self
            .sources
            .get(index)
            .ok_or("DLOG/DPOW kaynak indeksi 0 veya 1 olmalı")?
            .resolve(memory, false)?;
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
        index: usize,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<LogPowInput, String> {
        observed.extend(self.source_modifier_devices(index));
        let source = self.resolved_source(index, memory)?;
        source.devices(observed);
        Ok(match source.constant() {
            Some(value) => LogPowInput::Integer(value),
            None => LogPowInput::RawBinary32(source.read(memory) as u32),
        })
    }
    pub fn needs_right_source(&self, memory: &Memory) -> Result<bool, String> {
        let left = self.snapshot(0, memory, &mut BTreeSet::new())?;
        Ok(validate_log_pow_left(self.op, left).is_ok())
    }
    fn inputs(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<Result<[LogPowInput; 2], FloatBinaryError>, String> {
        let left = self.snapshot(0, memory, observed)?;
        if let Err(error) = validate_log_pow_left(self.op, left) {
            return Ok(Err(error));
        }
        let right = self.snapshot(1, memory, observed)?;
        Ok(validate_log_pow_pair(self.op, left, right).map(|()| [left, right]))
    }
    /// Staged domain checks only; native preflight does not run the kernels
    /// merely to decide which right/target modifiers it may safely capture.
    pub fn semantic_error(&self, memory: &Memory) -> Result<Option<FloatBinaryError>, String> {
        Ok(self.inputs(memory, &mut BTreeSet::new())?.err())
    }
    fn outcome(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<Result<AdvancedResult, FloatBinaryError>, String> {
        Ok(match self.inputs(memory, observed)? {
            Ok([left, right]) => evaluate_log_pow(self.op, left, right),
            Err(error) => Err(error),
        })
    }
    pub fn prepare(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<PreparedLogPow, String> {
        let result = match self.outcome(memory, observed)? {
            Ok(result) => result,
            Err(error) => {
                return Ok(PreparedLogPow::SemanticError {
                    code: error.code,
                    offset: error.offset,
                });
            }
        };
        observed.extend(self.target_modifier_devices());
        let target = self.resolved_target(memory)?;
        target.devices(observed);
        Ok(PreparedLogPow::Ready(PreparedLogPowResult {
            target: target.base().expect("D result"),
            result,
        }))
    }
    /// Caller owns P gating. Errors hold all flags; successful normalization
    /// precedes one target resolution, including self-indexed result pairs.
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
            "{}{} {} {} {}",
            self.op.name(),
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
        out.extend((1020..=1022).map(Memory::system_device));
        Memory::error_devices(out);
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        self.sources
            .iter()
            .flat_map(AddressedValue::modifier_devices)
            .chain(self.target_modifier_devices())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    /// Monitoring may show held operands/results independently of domain
    /// validity; execution footprints above include only visited stages.
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
            Ok(target) => f32::from_bits(target.read(memory) as u32).to_string(),
            Err(reason) => format!("İndeks: {reason}"),
        }
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let mut words = vec![self.op.header() | if self.pulse_label { 0x10 } else { 0 }];
        for source in &self.sources {
            words.extend(source.native_words()?);
        }
        words.extend(self.target.native_words()?);
        Ok(words)
    }
}
