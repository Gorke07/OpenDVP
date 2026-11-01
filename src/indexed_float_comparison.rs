//! PDF574/575 action and PDF758..760 contact modifiers. Actions convert K/H
//! integers; contacts use finite F literals or raw T/C/D binary32 sources.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::CalendarValue;
use crate::float_math::FloatBinaryError;
use crate::float_ops::{
    FloatComparisonInput, FloatOperand, evaluate_float_compare, evaluate_float_relation,
    evaluate_float_zone, parse_spec,
};
use crate::indexed_comparison::ComparisonTarget;
use crate::indexed_multiply::validate_model;
use crate::indexed_operand::{AddressedValue, has_modifiers};
use crate::plc::{Area, Device, Memory, Relation, comparison_destinations};
use crate::wide::WideOperand;
use std::collections::BTreeSet;
use std::fmt;

fn action_source(raw: &str) -> Result<AddressedValue, String> {
    let source = AddressedValue::parse(raw, true, false)?;
    let allowed = match &source {
        AddressedValue::Direct(CalendarValue::Additional(value)) => {
            value.base().is_none_or(|d| d.area == Area::D)
        }
        AddressedValue::Modified {
            address,
            digits: None,
            ..
        } => address.base.area == Area::D,
        _ => false,
    };
    if !allowed {
        return Err("DECMP/DEZCP kaynağı K/H/D olmalı; yalnız D adresi indekslenebilir".into());
    }
    Ok(source)
}

fn resolve_source(source: &AddressedValue, memory: &Memory) -> Result<CalendarValue, String> {
    let source = source.resolve(memory, false)?;
    let mut devices = BTreeSet::new();
    source.devices(&mut devices);
    validate_model(memory, &devices)?;
    Ok(source)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreparedFloatComparisonResult {
    target: Device,
    values: [bool; 3],
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreparedFloatComparison {
    Ready(PreparedFloatComparisonResult),
    SemanticError { code: i16, offset: u16 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedFloatComparisonAction {
    zone: bool,
    pulse_label: bool,
    sources: Vec<AddressedValue>,
    target: ComparisonTarget,
}
impl IndexedFloatComparisonAction {
    fn spec(name: &str) -> Option<(bool, bool)> {
        Some(match name.to_ascii_uppercase().as_str() {
            "DECMP" => (false, false),
            "DECMPP" => (false, true),
            "DEZCP" => (true, false),
            "DEZCPP" => (true, true),
            _ => return None,
        })
    }
    pub fn supports(name: &str) -> bool {
        Self::spec(name).is_some()
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        has_modifiers(args)
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let (zone, pulse_label) =
            Self::spec(name).ok_or("İndeksli kayan karşılaştırma geçersiz")?;
        let count = if zone { 3 } else { 2 };
        if args.len() != count + 1 {
            return Err(format!("{name}: {} operand gerekli", count + 1));
        }
        Ok(Self {
            zone,
            pulse_label,
            sources: args[..count]
                .iter()
                .map(|s| action_source(s))
                .collect::<Result<_, _>>()?,
            target: ComparisonTarget::parse(&args[count])?,
        })
    }
    pub fn sources(&self) -> &[AddressedValue] {
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
        resolve_source(
            self.sources
                .get(index)
                .ok_or("Kayan karşılaştırma kaynak indeksi geçersiz")?,
            memory,
        )
    }
    fn snapshot(
        &self,
        index: usize,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<FloatComparisonInput, String> {
        observed.extend(self.source_modifier_devices(index));
        let source = self.resolved_source(index, memory)?;
        source.devices(observed);
        Ok(match source.constant() {
            Some(value) => FloatComparisonInput::Integer(value),
            None => FloatComparisonInput::RawBinary32(source.read(memory) as u32),
        })
    }
    fn outcome(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<Result<[bool; 3], FloatBinaryError>, String> {
        // DECMP always reads both before classifying NaNs. DEZCP reads all
        // three before its ordered/unordered branches; neither is R arithmetic.
        let first = self.snapshot(0, memory, observed)?;
        let second = self.snapshot(1, memory, observed)?;
        if self.zone {
            let value = self.snapshot(2, memory, observed)?;
            Ok(Ok(evaluate_float_zone(first, second, value)))
        } else {
            Ok(evaluate_float_compare(first, second))
        }
    }
    pub fn semantic_error(&self, memory: &Memory) -> Result<Option<FloatBinaryError>, String> {
        Ok(self.outcome(memory, &mut BTreeSet::new())?.err())
    }
    pub fn prepare(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<PreparedFloatComparison, String> {
        let values = match self.outcome(memory, observed)? {
            Ok(values) => values,
            Err(error) => {
                return Ok(PreparedFloatComparison::SemanticError {
                    code: error.code,
                    offset: error.offset,
                });
            }
        };
        observed.extend(self.target_modifier_devices());
        let target = self.target.resolve(memory)?;
        observed.extend(comparison_destinations(target)?);
        Ok(PreparedFloatComparison::Ready(
            PreparedFloatComparisonResult { target, values },
        ))
    }
    /// Caller applies P gating. All source values precede the once-resolved
    /// result span; successful bit writes do not independently change ALU flags.
    pub fn execute(&self, memory: &mut Memory, power: bool, observed: &mut BTreeSet<Device>) {
        if !power {
            return;
        }
        match self.prepare(memory, observed) {
            Ok(PreparedFloatComparison::Ready(result)) => {
                for (device, value) in comparison_destinations(result.target)
                    .expect("resolved result span")
                    .into_iter()
                    .zip(result.values)
                {
                    memory.set_internal(device, value);
                }
            }
            Ok(PreparedFloatComparison::SemanticError { code, offset }) => {
                memory.operation_error(code, offset)
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
        let mut parts = vec![format!(
            "{}{}",
            if self.zone { "DEZCP" } else { "DECMP" },
            if self.pulse_label { "P" } else { "" }
        )];
        parts.extend(self.sources.iter().map(ToString::to_string));
        parts.push(self.target.to_string());
        parts.join(" ")
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
    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        for index in 0..self.sources.len() {
            if let Ok(source) = self.resolved_source(index, memory) {
                source.devices(out);
            }
        }
        if let Ok(target) = self.target.resolve(memory) {
            out.extend(comparison_destinations(target).expect("resolved result"));
        }
    }
    pub fn display_value(&self, memory: &Memory) -> String {
        match self.target.resolve(memory) {
            Ok(target) => comparison_destinations(target)
                .expect("resolved result")
                .iter()
                .map(|d| if memory.bit(*d) { "1" } else { "0" })
                .collect::<Vec<_>>()
                .join(" / "),
            Err(reason) => format!("İndeks: {reason}"),
        }
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let mut words = vec![
            (if self.zone { 0xef00 } else { 0xed00 }) | if self.pulse_label { 0x10 } else { 0 },
        ];
        for source in &self.sources {
            words.extend(source.native_words()?);
        }
        words.extend(self.target.native_words(true)?);
        Ok(words)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ContactSource {
    Literal { bits: u32, text: String },
    Register(AddressedValue),
}
impl ContactSource {
    fn parse(raw: &str) -> Result<Self, String> {
        let upper = raw.trim().to_ascii_uppercase();
        if upper.starts_with('F') {
            let FloatOperand::Literal { bits, text } = FloatOperand::parse(&upper)? else {
                unreachable!("F literal")
            };
            return Ok(Self::Literal { bits, text });
        }
        let source = AddressedValue::parse(&upper, true, false)?;
        let allowed = match &source {
            AddressedValue::Direct(CalendarValue::Additional(value)) => value
                .base()
                .is_some_and(|d| matches!(d.area, Area::T | Area::C | Area::D)),
            AddressedValue::Modified {
                address,
                digits: None,
                ..
            } => matches!(address.base.area, Area::T | Area::C | Area::D),
            _ => false,
        };
        if !allowed {
            return Err("Kayan karşılaştırma kontağı F sabiti veya T/C/D çifti gerektirir".into());
        }
        Ok(Self::Register(source))
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
impl fmt::Display for ContactSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Literal { text, .. } => f.write_str(text),
            Self::Register(value) => value.fmt(f),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedFloatPredicate {
    relation: Relation,
    sources: [ContactSource; 2],
}
impl IndexedFloatPredicate {
    pub fn supports(name: &str) -> bool {
        parse_spec(&name.to_ascii_uppercase()).is_some()
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        has_modifiers(args)
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let (_, relation) =
            parse_spec(&name.to_ascii_uppercase()).ok_or("İndeksli kayan kontak geçersiz")?;
        if args.len() != 2 {
            return Err(format!("{name}: 2 operand gerekli"));
        }
        Ok(Self {
            relation,
            sources: [
                ContactSource::parse(&args[0])?,
                ContactSource::parse(&args[1])?,
            ],
        })
    }
    pub fn source_modifier_devices(&self, index: usize) -> Vec<Device> {
        self.sources
            .get(index)
            .map(ContactSource::modifier_devices)
            .unwrap_or_default()
    }
    pub fn resolved_source(
        &self,
        index: usize,
        memory: &Memory,
    ) -> Result<Option<CalendarValue>, String> {
        match self
            .sources
            .get(index)
            .ok_or("Kayan kontak kaynak indeksi 0 veya 1 olmalı")?
        {
            ContactSource::Literal { .. } => Ok(None),
            ContactSource::Register(value) => resolve_source(value, memory).map(Some),
        }
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
            .ok_or("Kayan kontak kaynak indeksi 0 veya 1 olmalı")?
        {
            ContactSource::Literal { bits, .. } => Ok(*bits),
            ContactSource::Register(value) => {
                observed.extend(value.modifier_devices());
                let source = self
                    .resolved_source(index, memory)?
                    .expect("register source");
                source.devices(observed);
                Ok(source.read(memory) as u32)
            }
        }
    }
    pub fn evaluate(&self, memory: &Memory) -> Result<bool, String> {
        self.visit(memory, &mut BTreeSet::new())
    }
    /// Current FAND/FOR test the previous accumulator before either source;
    /// the caller gates visits. FLD always visits both, in source order.
    pub fn short_circuits(&self) -> bool {
        true
    }
    pub fn visit(&self, memory: &Memory, observed: &mut BTreeSet<Device>) -> Result<bool, String> {
        let left = self.snapshot(0, memory, observed)?;
        let right = self.snapshot(1, memory, observed)?;
        Ok(evaluate_float_relation(&self.relation, left, right))
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        for source in &self.sources {
            source.devices(out);
        }
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        self.sources
            .iter()
            .flat_map(ContactSource::modifier_devices)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        for index in 0..2 {
            if let Ok(Some(source)) = self.resolved_source(index, memory) {
                source.devices(out);
            }
        }
    }
    pub fn label(&self, prefix: &str) -> String {
        let prefix = match prefix {
            "LD" | "FLD" => "FLD",
            "AND" | "FAND" => "FAND",
            "OR" | "FOR" => "FOR",
            _ => prefix,
        };
        format!(
            "{prefix}{} {} {}",
            self.relation.symbol(),
            self.sources[0],
            self.sources[1]
        )
    }
    pub fn native_words(&self, prefix: &str) -> Result<Vec<u16>, String> {
        let base = match prefix {
            "LD" | "FLD" => 0x47,
            "AND" | "FAND" => 0x53,
            "OR" | "FOR" => 0x5f,
            _ => return Err("Kayan kontak öneki LD/AND/OR olmalı".into()),
        };
        let offset = match self.relation {
            Relation::Eq => 0,
            Relation::Gt => 2,
            Relation::Lt => 4,
            Relation::Ne => 6,
            Relation::Le => 8,
            Relation::Ge => 10,
        };
        let mut words = vec![((base + offset) << 8) | 2];
        for source in &self.sources {
            words.extend(source.native_words()?);
        }
        Ok(words)
    }
}
