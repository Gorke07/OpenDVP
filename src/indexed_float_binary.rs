//! Opt-in DEADD/DESUB/DEMUL/DEDIV modifiers, PDF582..585. D operands
//! carry raw binary32; K/H sources remain signed integers converted to float.
use crate::calendar_operand::CalendarValue;
use crate::float_math::{
    FloatBinaryError, FloatBinaryInput, FloatBinaryResult, MathOp, evaluate_binary,
};
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
        return Err("DEADD/DESUB/DEMUL/DEDIV kaynakları K/H/D, hedefi D olmalı; yalnız D adresine E/F son eki eklenebilir".into());
    }
    Ok(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreparedFloatBinaryResult {
    target: Device,
    result: FloatBinaryResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreparedFloatBinary {
    Ready(PreparedFloatBinaryResult),
    SemanticError { code: i16, offset: u16 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedFloatBinaryAction {
    op: MathOp,
    pulse_label: bool,
    sources: [AddressedValue; 2],
    target: AddressedValue,
}

impl IndexedFloatBinaryAction {
    pub fn supports(name: &str) -> bool {
        matches!(
            name.to_ascii_uppercase().as_str(),
            "DEADD" | "DEADDP" | "DESUB" | "DESUBP" | "DEMUL" | "DEMULP" | "DEDIV" | "DEDIVP"
        )
    }

    pub fn has_modifiers(args: &[String]) -> bool {
        args.iter().any(|raw| has_modifier(raw))
    }

    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if !Self::supports(name) || args.len() != 3 {
            return Err("İndeksli DE aritmetiği iki kaynak ve bir hedef gerektirir".into());
        }
        let name = name.to_ascii_uppercase();
        let pulse_label = name.ends_with('P');
        let ordinary = name.strip_suffix('P').unwrap_or(&name);
        Ok(Self {
            op: MathOp::parse(ordinary).expect("supported binary operation"),
            pulse_label,
            sources: [binding(&args[0], false)?, binding(&args[1], false)?],
            target: binding(&args[2], true)?,
        })
    }

    pub fn sources(&self) -> &[AddressedValue; 2] {
        &self.sources
    }

    pub fn resolved_source(&self, index: usize, memory: &Memory) -> Result<CalendarValue, String> {
        let source = self
            .sources
            .get(index)
            .ok_or("Kayan nokta kaynak indeksi 0 veya 1 olmalı")?
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
    ) -> Result<FloatBinaryInput, String> {
        observed.extend(self.sources[index].modifier_devices());
        let source = self.resolved_source(index, memory)?;
        source.devices(observed);
        Ok(match source.constant() {
            Some(integer) => FloatBinaryInput::Integer(integer),
            None => FloatBinaryInput::RawBinary32(source.read(memory) as u32),
        })
    }

    fn outcome(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<Result<FloatBinaryResult, FloatBinaryError>, String> {
        let left = self.snapshot(0, memory, observed)?;
        let right = self.snapshot(1, memory, observed)?;
        Ok(evaluate_binary(self.op, left, right))
    }

    /// Pure preflight. DEDIV arithmetic failures preserve flags and never
    /// observe, resolve or qualify the target modifier or target pair.
    pub fn prepare(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<PreparedFloatBinary, String> {
        let result = match self.outcome(memory, observed)? {
            Ok(result) => result,
            Err(error) => {
                return Ok(PreparedFloatBinary::SemanticError {
                    code: error.code,
                    offset: error.offset,
                });
            }
        };
        observed.extend(self.target.modifier_devices());
        let target = self.resolved_target(memory)?;
        target.devices(observed);
        Ok(PreparedFloatBinary::Ready(PreparedFloatBinaryResult {
            target: target.base().expect("validated D result"),
            result,
        }))
    }

    /// Caller owns P qualification. There is no early arithmetic-flag clear;
    /// source failures and semantic division errors retain prior flags.
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
        observed.extend(self.target.modifier_devices());
        match self.resolved_target(memory) {
            Ok(target) => {
                target.devices(observed);
                // Resolving once keeps a self-indexed low-half write from
                // redirecting the high half to a newly selected address.
                result.write_value(memory, target.base().expect("validated D result"));
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
            .chain(self.target.modifier_devices())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// Monitoring resolves the held result independently of the arithmetic
    /// outcome; actual execution footprints come only from prepare/execute.
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
