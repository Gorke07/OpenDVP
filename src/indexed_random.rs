//! Addressed RAND/P and DRAND/P numerical body (PDF626).
//! DRAND operand-tail control flow belongs to the
//! execution plan; this module preserves its original seven/thirteen WORDs.
use crate::additional_processing::AdditionalValue;
use crate::bit_index_operand::BitIndexValue;
use crate::calendar_operand::{CalendarDestination, CalendarValue};
use crate::float_math::FloatBinaryError;
use crate::indexed_multiply::{qualify_group, validate_model};
use crate::indexed_operand::{AddressedValue, has_modifier, has_modifiers};
use crate::mask_random_operand::MaskRandomDestination;
use crate::plc::{Device, Memory};
use crate::random_ops::sample_with_state;
use crate::wide::WideOperand;
use std::collections::BTreeSet;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
enum Binding {
    Direct(BitIndexValue),
    Modified(AddressedValue),
}
impl Binding {
    fn parse(raw: &str, wide: bool, write: bool) -> Result<Self, String> {
        if has_modifier(raw) {
            return AddressedValue::parse(raw, wide, write).map(Self::Modified);
        }
        if write {
            MaskRandomDestination::parse(raw, wide)?;
        }
        BitIndexValue::parse(raw, wide).map(Self::Direct)
    }
    fn value(&self, memory: &Memory, write: bool) -> Result<CalendarValue, String> {
        let value = match self {
            Self::Modified(value) => value.resolve(memory, write)?,
            Self::Direct(value) => match value.calendar_value() {
                Some(value) => value.clone(),
                None => {
                    let mut devices = BTreeSet::new();
                    value.devices(&mut devices);
                    let base = *devices.first().expect("scoped wide F has canonical D pair");
                    CalendarValue::Additional(AdditionalValue::Double(WideOperand::Register(base)))
                }
            },
        };
        if let CalendarValue::Additional(AdditionalValue::Group { base, digits, .. }) = &value {
            qualify_group(self.base().expect("group base"), *base, digits * 4, false)?;
        }
        let mut devices = BTreeSet::new();
        value.devices(&mut devices);
        validate_model(memory, &devices)?;
        Ok(value)
    }
    fn base(&self) -> Option<Device> {
        match self {
            Self::Modified(value) => value.base(),
            Self::Direct(value) => match value.calendar_value() {
                Some(value) => value.base(),
                None => {
                    let mut devices = BTreeSet::new();
                    value.devices(&mut devices);
                    devices.first().copied()
                }
            },
        }
    }
    fn devices(&self, out: &mut BTreeSet<Device>) {
        match self {
            Self::Direct(value) => value.devices(out),
            Self::Modified(value) => value.devices(out),
        }
    }
    fn modifier_devices(&self) -> Vec<Device> {
        match self {
            Self::Direct(_) => Vec::new(),
            Self::Modified(value) => value.modifier_devices(),
        }
    }
    fn native_words(&self) -> Result<Vec<u16>, String> {
        match self {
            Self::Modified(value) => value.native_words(),
            Self::Direct(value) => match value.native_words() {
                Some(words) => Ok(words),
                None => crate::es2_encoder::calendar_operand(
                    value.calendar_value().expect("calendar operand"),
                ),
            },
        }
    }
}
impl fmt::Display for Binding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Direct(value) => value.fmt(f),
            Self::Modified(value) => value.fmt(f),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedRandomTarget {
    bounds: [i32; 2],
    target: CalendarDestination,
}
#[derive(Debug, Clone, PartialEq)]
pub enum PreparedRandom {
    Ready(PreparedRandomTarget),
    SemanticError { code: i16, offset: u16 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedRandomAction {
    wide: bool,
    pulse_label: bool,
    sources: [Binding; 2],
    target: Binding,
}
impl IndexedRandomAction {
    pub fn supports(name: &str) -> bool {
        matches!(
            name.to_ascii_uppercase().as_str(),
            "RAND" | "RANDP" | "DRAND" | "DRANDP"
        )
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        has_modifiers(args)
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if !Self::supports(name) || args.len() != 3 {
            return Err("İndeksli RAND iki sınır ve bir hedef gerektirir".into());
        }
        let name = name.to_ascii_uppercase();
        let wide = name.starts_with('D');
        Ok(Self {
            wide,
            pulse_label: name.ends_with('P'),
            sources: [
                Binding::parse(&args[0], wide, false)?,
                Binding::parse(&args[1], wide, false)?,
            ],
            target: Binding::parse(&args[2], wide, true)?,
        })
    }
    pub fn wide(&self) -> bool {
        self.wide
    }
    pub fn source_count(&self) -> usize {
        2
    }
    pub fn source_modifier_devices(&self, index: usize) -> Vec<Device> {
        self.sources
            .get(index)
            .map(Binding::modifier_devices)
            .unwrap_or_default()
    }
    pub fn target_modifier_devices(&self) -> Vec<Device> {
        self.target.modifier_devices()
    }
    pub fn resolved_source(&self, index: usize, memory: &Memory) -> Result<CalendarValue, String> {
        self.sources
            .get(index)
            .ok_or("RAND kaynak indeksi 0 veya 1 olmalı")?
            .value(memory, false)
    }
    fn resolved_target(&self, memory: &Memory) -> Result<CalendarDestination, String> {
        CalendarDestination::from_value(self.target.value(memory, true)?)
    }
    fn bounds(&self, memory: &Memory, observed: &mut BTreeSet<Device>) -> Result<[i32; 2], String> {
        let mut bounds = [0; 2];
        for (index, bound) in bounds.iter_mut().enumerate() {
            observed.extend(self.source_modifier_devices(index));
            let source = self.resolved_source(index, memory)?;
            *bound = crate::indexed_operand::read_resolved_value(&source, memory, observed)?;
        }
        Ok(bounds)
    }
    fn error(&self) -> FloatBinaryError {
        FloatBinaryError {
            code: 0xe1a,
            offset: if self.wide { 5 } else { 3 },
        }
    }
    fn invalid_bounds(bounds: [i32; 2]) -> bool {
        bounds[0] < 0 || bounds[1] < 0 || bounds[0] > bounds[1]
    }
    /// Pure domain decision. It intentionally does not run the sampling
    /// kernel: reversed nonnegative bounds mutate state only on execution.
    pub fn semantic_error(&self, memory: &Memory) -> Result<Option<FloatBinaryError>, String> {
        Ok(Self::invalid_bounds(self.bounds(memory, &mut BTreeSet::new())?).then(|| self.error()))
    }
    /// Native preflight only: source snapshots, domain, then target bounds.
    /// No seed initialization, sampling, pulse/history or PLC error mutation.
    pub fn prepare(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<PreparedRandom, String> {
        let bounds = self.bounds(memory, observed)?;
        if Self::invalid_bounds(bounds) {
            let error = self.error();
            return Ok(PreparedRandom::SemanticError {
                code: error.code,
                offset: error.offset,
            });
        }
        observed.extend(self.target_modifier_devices());
        let target = self.resolved_target(memory)?;
        target.devices(observed);
        Ok(PreparedRandom::Ready(PreparedRandomTarget {
            bounds,
            target,
        }))
    }
    /// Caller owns normal/P power and DRAND's separate operand-tail flow.
    /// Sampling deliberately precedes destination qualification, so an invalid
    /// target cannot undo a state update made by a valid numerical visit.
    pub fn execute(&self, memory: &mut Memory, power: bool, observed: &mut BTreeSet<Device>) {
        if !power {
            return;
        }
        let [lower, upper] = match self.bounds(memory, observed) {
            Ok(bounds) => bounds,
            Err(reason) => {
                memory.execution_limitations.insert(reason);
                return;
            }
        };
        if lower < 0 || upper < 0 {
            let error = self.error();
            memory.operation_error(error.code, error.offset);
            return;
        }
        let state = memory.random_state.get_or_insert(1);
        let value = match sample_with_state(lower, upper, self.wide, state) {
            Ok(value) => value,
            Err(_) => {
                let error = self.error();
                memory.operation_error(error.code, error.offset);
                return;
            }
        };
        observed.extend(self.target_modifier_devices());
        match self.resolved_target(memory) {
            Ok(target) => {
                target.devices(observed);
                target.write(memory, value);
            }
            Err(reason) => {
                memory.execution_limitations.insert(reason);
            }
        }
    }
    pub fn dest(&self) -> Device {
        self.target.base().expect("validated RAND result")
    }
    pub fn label(&self) -> String {
        format!(
            "{}RAND{} {} {} {}",
            if self.wide { "D" } else { "" },
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
        let mut result = Vec::new();
        for d in self
            .sources
            .iter()
            .chain(std::iter::once(&self.target))
            .flat_map(Binding::modifier_devices)
        {
            if !result.contains(&d) {
                result.push(d);
            }
        }
        result
    }
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
            Ok(target) => target.read(memory).to_string(),
            Err(reason) => format!("İndeks: {reason}"),
        }
    }
    /// Original operands are preserved, including E and colliding/noncolliding
    /// F descriptors. The execution-plan layer classifies DRAND's real tail.
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let mut words = vec![
            (if self.wide { 0x5501 } else { 0x5401 }) | if self.pulse_label { 0x10 } else { 0 },
        ];
        for source in &self.sources {
            words.extend(source.native_words()?);
        }
        words.extend(self.target.native_words()?);
        Ok(words)
    }
}
