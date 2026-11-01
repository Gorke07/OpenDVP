//! Scoped E/F suffixes for PDF429/713/714 ABS/GRY/GBIN roles.
//! Shared pure integer kernels preserve ordinary behavior and snapshot order.
use crate::additional_processing::AdditionalValue;
use crate::bit_tools::{absolute_result, gray_result};
use crate::calendar_operand::{CalendarDestination, CalendarValue};
use crate::float_math::FloatBinaryError;
use crate::indexed_multiply::{qualify_group, validate_model};
use crate::indexed_operand::{AddressedValue, has_modifier};
use crate::plc::{Device, Memory};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operation {
    Absolute,
    GrayEncode,
    GrayDecode,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedAbsGrayResult {
    target: CalendarDestination,
    value: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PreparedAbsGray {
    Ready(PreparedAbsGrayResult),
    /// Negative Gray source: E1A at header+1. No target modifier was accessed.
    SemanticError {
        code: i16,
        offset: u16,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedAbsGrayAction {
    operation: Operation,
    wide: bool,
    pulse_label: bool,
    // ABS has only one operand. Keeping its binding here avoids a separate
    // source projection that could disagree with its actual destination.
    source: Option<AddressedValue>,
    target: AddressedValue,
}

impl IndexedAbsGrayAction {
    pub fn supports(name: &str) -> bool {
        matches!(
            name.to_ascii_uppercase().as_str(),
            "ABS"
                | "ABSP"
                | "DABS"
                | "DABSP"
                | "GRY"
                | "GRYP"
                | "DGRY"
                | "DGRYP"
                | "GBIN"
                | "GBINP"
                | "DGBIN"
                | "DGBINP"
        )
    }

    pub fn has_modifiers(args: &[String]) -> bool {
        args.iter().any(|raw| has_modifier(raw))
    }

    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if !Self::supports(name) {
            return Err("İndeksli ABS/GRY/GBIN komutu bekleniyor".into());
        }
        let name = name.to_ascii_uppercase();
        let operation = if name.contains("ABS") {
            Operation::Absolute
        } else if name.contains("GBIN") {
            Operation::GrayDecode
        } else {
            Operation::GrayEncode
        };
        let absolute = operation == Operation::Absolute;
        let expected = if absolute { 1 } else { 2 };
        if args.len() != expected {
            return Err(format!("{name}: {expected} operand gerekli"));
        }
        let wide = name.starts_with('D');
        // AddressedValue admits precisely these grey roles: source KnX/Y/M/S
        // or T/C/D; writable KnY/M/S or T/C/D. Its direct scalar path preserves
        // K/H sources, E word/pair and F word-only without widening other APIs.
        let target = AddressedValue::parse(&args[expected - 1], wide, true)?;
        let source = if absolute {
            None
        } else {
            Some(AddressedValue::parse(&args[0], wide, false)?)
        };
        Ok(Self {
            operation,
            wide,
            pulse_label: name.ends_with('P'),
            source,
            target,
        })
    }

    pub fn wide(&self) -> bool {
        self.wide
    }

    pub fn source_count(&self) -> usize {
        1
    }

    fn source_binding(&self, index: usize) -> Result<&AddressedValue, String> {
        if index != 0 {
            return Err("ABS/GRY/GBIN yalnız bir kaynak okur".into());
        }
        Ok(self.source.as_ref().unwrap_or(&self.target))
    }

    pub fn source_modifier_devices(&self, index: usize) -> Vec<Device> {
        self.source_binding(index)
            .map(AddressedValue::modifier_devices)
            .unwrap_or_default()
    }

    pub fn target_modifier_devices(&self) -> Vec<Device> {
        self.target.modifier_devices()
    }

    fn qualify(binding: &AddressedValue, value: &CalendarValue) -> Result<(), String> {
        if let CalendarValue::Additional(AdditionalValue::Group { base, digits, .. }) = value {
            // Both readers and writers are generic scalar helpers: strict
            // low-bank M start >1280, not the MUL resolver's >=1280 rule.
            qualify_group(
                binding.base().expect("group base"),
                *base,
                digits * 4,
                false,
            )?;
        }
        Ok(())
    }

    pub fn resolved_source(&self, index: usize, memory: &Memory) -> Result<CalendarValue, String> {
        let binding = self.source_binding(index)?;
        let source = binding.resolve(memory, false)?;
        Self::qualify(binding, &source)?;
        let mut devices = BTreeSet::new();
        source.devices(&mut devices);
        validate_model(memory, &devices)?;
        Ok(source)
    }

    fn resolved_target(&self, memory: &Memory) -> Result<CalendarDestination, String> {
        let value = self.target.resolve(memory, true)?;
        Self::qualify(&self.target, &value)?;
        let target = CalendarDestination::from_value(value)?;
        let mut devices = BTreeSet::new();
        target.devices(&mut devices);
        validate_model(memory, &devices)?;
        Ok(target)
    }

    fn snapshot(&self, memory: &Memory, observed: &mut BTreeSet<Device>) -> Result<i32, String> {
        observed.extend(self.source_modifier_devices(0));
        let source = self.resolved_source(0, memory)?;
        crate::indexed_operand::read_resolved_value(&source, memory, observed)
    }

    fn result(&self, snapshot: i32) -> Result<i32, FloatBinaryError> {
        match self.operation {
            Operation::Absolute => Ok(absolute_result(snapshot, self.wide)),
            Operation::GrayEncode | Operation::GrayDecode => {
                gray_result(snapshot, self.operation == Operation::GrayDecode).ok_or(
                    FloatBinaryError {
                        code: 0xe1a,
                        offset: 1,
                    },
                )
            }
        }
    }

    /// Native bridge calls this only after capturing the real source. The
    /// historical error type is a plain code/offset carrier; no float math is
    /// involved, and no target address is resolved by this method.
    pub fn semantic_error(&self, memory: &Memory) -> Result<Option<FloatBinaryError>, String> {
        Ok(self
            .result(self.snapshot(memory, &mut BTreeSet::new())?)
            .err())
    }

    /// Read-only qualification; actual access records include a target only
    /// after successful conversion. Prepared values never borrow live storage.
    pub fn prepare(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<PreparedAbsGray, String> {
        let value = match self.result(self.snapshot(memory, observed)?) {
            Ok(value) => value,
            Err(error) => {
                return Ok(PreparedAbsGray::SemanticError {
                    code: error.code,
                    offset: error.offset,
                });
            }
        };
        // ABS's native writer rereads its modifier here; no intervening public
        // store changes it. Cache the resulting target for the entire write,
        // even when its first word overwrites that modifier's own alias.
        observed.extend(self.target_modifier_devices());
        let target = self.resolved_target(memory)?;
        target.devices(observed);
        Ok(PreparedAbsGray::Ready(PreparedAbsGrayResult {
            target,
            value,
        }))
    }

    /// The VM's ordinary P dispatcher owns edge history and skips held visits.
    pub fn execute(&self, memory: &mut Memory, power: bool, observed: &mut BTreeSet<Device>) {
        if !power {
            return;
        }
        match self.prepare(memory, observed) {
            Ok(PreparedAbsGray::Ready(result)) => result.target.write(memory, result.value),
            Ok(PreparedAbsGray::SemanticError { code, offset }) => {
                memory.operation_error(code, offset)
            }
            Err(reason) => {
                memory.execution_limitations.insert(reason);
            }
        }
    }

    pub fn dest(&self) -> Device {
        self.target.base().expect("validated ABS/Gray destination")
    }

    pub fn label(&self) -> String {
        let name = match self.operation {
            Operation::Absolute => "ABS",
            Operation::GrayEncode => "GRY",
            Operation::GrayDecode => "GBIN",
        };
        let prefix = if self.wide { "D" } else { "" };
        let suffix = if self.pulse_label { "P" } else { "" };
        match &self.source {
            Some(source) => format!("{prefix}{name}{suffix} {source} {}", self.target),
            None => format!("{prefix}{name}{suffix} {}", self.target),
        }
    }

    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        if let Some(source) = &self.source {
            source.devices(out);
        }
        self.target.devices(out);
        out.extend((1020..=1022).map(Memory::system_device));
        Memory::error_devices(out);
    }

    /// Ordered by first possible read, with duplicate alias words removed.
    pub fn modifier_devices(&self) -> Vec<Device> {
        let mut out = self.source_modifier_devices(0);
        for device in self.target_modifier_devices() {
            if !out.contains(&device) {
                out.push(device);
            }
        }
        out
    }

    /// Monitoring includes a held result independently of a Gray domain error.
    /// The execution path above, in contrast, only records actual accesses.
    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Ok(source) = self.resolved_source(0, memory) {
            source.devices(out);
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

    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let mut header = match self.operation {
            Operation::Absolute => 0xbe00,
            Operation::GrayEncode => 0x7201,
            Operation::GrayDecode => 0x7401,
        };
        if self.wide {
            header += 0x100;
        }
        if self.pulse_label {
            header |= 0x10;
        }
        let mut words = vec![header];
        if let Some(source) = &self.source {
            words.extend(source.native_words()?);
        }
        words.extend(self.target.native_words()?);
        Ok(words)
    }
}
