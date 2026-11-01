//! Indexed R arithmetic (PDF606/608/610/612). F literals and D pairs carry
//! raw binary32; ADDR/SUBR/MULR validate left before visiting right.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::CalendarValue;
use crate::float_math::{
    FloatBinaryError, FloatBinaryResult, MathOp, evaluate_real_binary, validate_real_left,
    validate_real_pair,
};
use crate::float_ops::FloatOperand;
use crate::indexed_multiply::validate_model;
use crate::indexed_operand::{AddressedValue, has_modifier};
use crate::plc::{Area, Device, Memory};
use crate::wide::WideOperand;
use std::collections::BTreeSet;
use std::fmt;

fn data_binding(raw: &str, write: bool) -> Result<AddressedValue, String> {
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
        return Err(
            "R kayan nokta yazmacı D çifti olmalı; yalnız D adresine E/F son eki eklenebilir"
                .into(),
        );
    }
    Ok(value)
}

#[derive(Debug, Clone, PartialEq)]
enum RealSource {
    Literal { bits: u32, text: String },
    Register(AddressedValue),
}

impl RealSource {
    fn parse(raw: &str) -> Result<Self, String> {
        let upper = raw.trim().to_ascii_uppercase();
        // F1.200E+0 is a floating literal, not an E index suffix. F1 itself
        // is literal1; an F modifier appears only after the D base address.
        if upper.starts_with('F') {
            let FloatOperand::Literal { bits, text } = FloatOperand::parse(&upper)? else {
                unreachable!("F parser returns literal")
            };
            Ok(Self::Literal { bits, text })
        } else {
            Ok(Self::Register(data_binding(&upper, false)?))
        }
    }

    fn devices(&self, out: &mut BTreeSet<Device>) {
        if let Self::Register(value) = self {
            value.devices(out);
        }
    }

    fn modifier_devices(&self) -> Vec<Device> {
        match self {
            Self::Literal { .. } => Vec::new(),
            Self::Register(value) => value.modifier_devices(),
        }
    }

    fn native_words(&self) -> Result<Vec<u16>, String> {
        match self {
            Self::Literal { bits, .. } => {
                crate::es2_encoder::calendar_operand(&CalendarValue::Additional(
                    AdditionalValue::Double(WideOperand::Constant(*bits as i32)),
                ))
            }
            Self::Register(value) => value.native_words(),
        }
    }
}

impl fmt::Display for RealSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Literal { text, .. } => f.write_str(text),
            Self::Register(value) => value.fmt(f),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreparedRealBinaryResult {
    target: Device,
    result: FloatBinaryResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreparedRealBinary {
    Ready(PreparedRealBinaryResult),
    SemanticError { code: i16, offset: u16 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedRealBinaryAction {
    op: MathOp,
    pulse_label: bool,
    sources: [RealSource; 2],
    target: AddressedValue,
}

impl IndexedRealBinaryAction {
    pub fn supports(name: &str) -> bool {
        matches!(
            name.to_ascii_uppercase().as_str(),
            "DADDR" | "DADDRP" | "DSUBR" | "DSUBRP" | "DMULR" | "DMULRP" | "DDIVR" | "DDIVRP"
        )
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        args.iter().any(|raw| has_modifier(raw))
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if !Self::supports(name) || args.len() != 3 {
            return Err("İndeksli R aritmetiği iki kaynak ve bir hedef gerektirir".into());
        }
        let name = name.to_ascii_uppercase();
        Ok(Self {
            op: MathOp::parse(name.strip_suffix('P').unwrap_or(&name))
                .expect("supported R operation"),
            pulse_label: name.ends_with('P'),
            sources: [RealSource::parse(&args[0])?, RealSource::parse(&args[1])?],
            target: data_binding(&args[2], true)?,
        })
    }
    pub fn is_divide_real(&self) -> bool {
        self.op == MathOp::DivideReal
    }
    pub fn source_modifier_devices(&self, index: usize) -> Vec<Device> {
        self.sources
            .get(index)
            .map(RealSource::modifier_devices)
            .unwrap_or_default()
    }
    pub fn target_modifier_devices(&self) -> Vec<Device> {
        self.target.modifier_devices()
    }

    pub fn resolved_source(
        &self,
        index: usize,
        memory: &Memory,
    ) -> Result<Option<CalendarValue>, String> {
        match self
            .sources
            .get(index)
            .ok_or("R kayan nokta kaynak indeksi 0 veya 1 olmalı")?
        {
            RealSource::Literal { .. } => Ok(None),
            RealSource::Register(value) => {
                let source = value.resolve(memory, false)?;
                let mut devices = BTreeSet::new();
                source.devices(&mut devices);
                validate_model(memory, &devices)?;
                Ok(Some(source))
            }
        }
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
    ) -> Result<u32, String> {
        match self
            .sources
            .get(index)
            .ok_or("R kayan nokta kaynak indeksi 0 veya 1 olmalı")?
        {
            RealSource::Literal { bits, .. } => Ok(*bits),
            RealSource::Register(value) => {
                observed.extend(value.modifier_devices());
                let source = self
                    .resolved_source(index, memory)?
                    .expect("register source");
                source.devices(observed);
                Ok(source.read(memory) as u32)
            }
        }
    }

    /// Bridge calls this after capturing actual left storage. No right
    /// address or modifier is accessed when an ADDR/SUBR/MULR left is NaN.
    pub fn needs_right_source(&self, memory: &Memory) -> Result<bool, String> {
        let left = self.snapshot(0, memory, &mut BTreeSet::new())?;
        Ok(validate_real_left(self.op, left).is_ok())
    }

    fn inputs(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<Result<[u32; 2], FloatBinaryError>, String> {
        let left = self.snapshot(0, memory, observed)?;
        if let Err(error) = validate_real_left(self.op, left) {
            return Ok(Err(error));
        }
        let right = self.snapshot(1, memory, observed)?;
        Ok(validate_real_pair(self.op, left, right).map(|()| [left, right]))
    }

    /// Only staged validation, without arithmetic, flags or target access.
    pub fn semantic_error(&self, memory: &Memory) -> Result<Option<FloatBinaryError>, String> {
        Ok(self.inputs(memory, &mut BTreeSet::new())?.err())
    }

    fn outcome(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<Result<FloatBinaryResult, FloatBinaryError>, String> {
        Ok(match self.inputs(memory, observed)? {
            Ok([left, right]) => evaluate_real_binary(self.op, left, right),
            Err(error) => Err(error),
        })
    }

    pub fn prepare(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<PreparedRealBinary, String> {
        let result = match self.outcome(memory, observed)? {
            Ok(result) => result,
            Err(error) => {
                return Ok(PreparedRealBinary::SemanticError {
                    code: error.code,
                    offset: error.offset,
                });
            }
        };
        observed.extend(self.target_modifier_devices());
        let target = self.resolved_target(memory)?;
        target.devices(observed);
        Ok(PreparedRealBinary::Ready(PreparedRealBinaryResult {
            target: target.base().expect("D result"),
            result,
        }))
    }

    /// Caller applies P gating. Source/semantic failures hold existing flags;
    /// success publishes flags before resolving the one fixed result pair.
    pub fn execute(
        &self,
        memory: &mut Memory,
        power: bool,
        observed: &mut BTreeSet<Device>,
    ) -> bool {
        if !power {
            return false;
        }
        let result = match self.outcome(memory, observed) {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => {
                memory.operation_error(error.code, error.offset);
                return self.is_divide_real() && error.code == 0xe1a && error.offset == 9;
            }
            Err(reason) => {
                memory.execution_limitations.insert(reason);
                return false;
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
        false
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
            .flat_map(RealSource::modifier_devices)
            .chain(self.target_modifier_devices())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    /// UI collection is independent of execution and can show held sources
    /// and result; prepare/execute alone define actual instruction accesses.
    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        for index in 0..2 {
            if let Ok(Some(source)) = self.resolved_source(index, memory) {
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
    /// Exactly13 semantic words. The caller must add the four balanced INV
    /// words and skip metadata when is_divide_real() is true.
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let mut words = vec![self.op.header() | if self.pulse_label { 0x10 } else { 0 }];
        for source in &self.sources {
            words.extend(source.native_words()?);
        }
        words.extend(self.target.native_words()?);
        Ok(words)
    }
}
