//! PDF235..238 addressed ROR/ROL/RCR/RCL and their D/P forms.
//! Uses the shared pure `shift::rotation_result` kernel with ordinary rotations.
//! Only full K4/K8 groups are documented. Native permissive counts/groups do
//! not widen the public syntax. Address faults remain explicit qualifications.
//! In particular, a native faulted first read may still reach the carry loop
//! after the count reader clears transient status. Local checked-read failures
//! are safety limitations, not a native error/flags-hold equivalence claim.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::{CalendarDestination, CalendarValue};
use crate::indexed_multiply::{qualify_group, validate_model};
use crate::indexed_operand::{AddressedValue, frozen_value, has_modifiers, read_resolved_value};
use crate::plc::{Device, Memory};
use crate::shift::{Rotation, rotation_result};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedRotation {
    value: i32,
    carry: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedRotationAction {
    kind: Rotation,
    wide: bool,
    pulse_label: bool,
    target: AddressedValue,
    count: u8,
}

impl IndexedRotationAction {
    fn spec(name: &str) -> Option<(Rotation, bool, bool)> {
        let name = name.to_ascii_uppercase();
        let pulse = name.ends_with('P');
        let stem = name.strip_suffix('P').unwrap_or(&name);
        let wide = stem.starts_with('D');
        let plain = stem.strip_prefix('D').unwrap_or(stem);
        let kind = match plain {
            "ROR" => Rotation::Right,
            "ROL" => Rotation::Left,
            "RCR" => Rotation::CarryRight,
            "RCL" => Rotation::CarryLeft,
            _ => return None,
        };
        Some((kind, wide, pulse))
    }
    pub fn supports(name: &str) -> bool {
        Self::spec(name).is_some()
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        has_modifiers(args)
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let (kind, wide, pulse_label) =
            Self::spec(name).ok_or("İndeksli döndürme komutu geçersiz")?;
        if args.len() != 2 {
            return Err(format!("{name}: hedef ve K/H döndürme sayısı gerekli"));
        }
        let target = AddressedValue::parse(&args[0], wide, true)?;
        let digits = match &target {
            AddressedValue::Direct(CalendarValue::Additional(AdditionalValue::Group {
                digits,
                ..
            })) => Some(*digits),
            AddressedValue::Modified { digits, .. } => *digits,
            _ => None,
        };
        if digits.is_some_and(|digits| digits != if wide { 8 } else { 4 }) {
            return Err("Döndürme hedefi 16 bitte K4, 32 bitte K8 grubu olmalı".into());
        }
        let raw = args[1].to_ascii_uppercase();
        let count = raw
            .strip_prefix('K')
            .and_then(|n| n.parse::<u8>().ok())
            .or_else(|| {
                raw.strip_prefix('H')
                    .and_then(|n| u8::from_str_radix(n, 16).ok())
            })
            .filter(|n| (1..=if wide { 32 } else { 16 }).contains(n))
            .ok_or("Döndürme sayısı 16 bitte K/H1..16, 32 bitte K/H1..32 sabiti olmalı")?;
        Ok(Self {
            kind,
            wide,
            pulse_label,
            target,
            count,
        })
    }
    pub fn wide(&self) -> bool {
        self.wide
    }
    pub fn source_count(&self) -> usize {
        1
    }
    pub fn source_modifier_devices(&self, index: usize) -> Vec<Device> {
        if index == 0 {
            self.target.modifier_devices()
        } else {
            Vec::new()
        }
    }
    pub fn target_modifier_devices(&self) -> Vec<Device> {
        self.target.modifier_devices()
    }
    fn value(&self, memory: &Memory, write: bool) -> Result<CalendarValue, String> {
        let value = self.target.resolve(memory, write)?;
        if let CalendarValue::Additional(AdditionalValue::Group { base, digits, .. }) = &value {
            qualify_group(
                self.target.base().expect("rotation group"),
                *base,
                digits * 4,
                false,
            )?;
        }
        let mut devices = BTreeSet::new();
        value.devices(&mut devices);
        validate_model(memory, &devices)?;
        Ok(value)
    }
    /// Address-only source qualification remains usable before native capture.
    pub fn resolved_source(&self, index: usize, memory: &Memory) -> Result<CalendarValue, String> {
        if index != 0 {
            return Err("Döndürme yalnız kaynak 0'ı okur".into());
        }
        self.value(memory, false)
    }
    fn resolved_target(&self, memory: &Memory) -> Result<CalendarDestination, String> {
        CalendarDestination::from_value(self.value(memory, true)?)
    }
    /// Separate pure bridge check. This never performs a numerical source read
    /// or publishes carry and must not replace execute's late writer lookup.
    pub fn qualify_target(&self, memory: &Memory) -> Result<(), String> {
        self.resolved_target(memory).map(|_| ())
    }
    /// Pure source snapshot and calculation ONLY: target resolution is later.
    /// The source is checked before a carry input can be observed. Normal ROR/
    /// ROL ignore the incoming carry; RCR/RCL use the existing 17/33-bit ring.
    pub fn prepare(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<PreparedRotation, String> {
        observed.extend(self.source_modifier_devices(0));
        let source = self.resolved_source(0, memory)?;
        let value = read_resolved_value(&source, memory, observed)?;
        let old_carry = if matches!(self.kind, Rotation::CarryLeft | Rotation::CarryRight) {
            observed.insert(Memory::system_device(1022));
            memory.system_bit(1022)
        } else {
            false
        };
        let (value, carry) = rotation_result(
            value,
            old_carry,
            self.wide,
            self.kind,
            u16::from(self.count),
        );
        Ok(PreparedRotation { value, carry })
    }
    /// Caller owns the normal/P gate. The native loop publishes carry BEFORE
    /// the generic writer rereads the target modifier and caches its full span.
    /// Structural failures are limitations, not invented native PLC errors.
    pub fn execute(&self, memory: &mut Memory, power: bool, observed: &mut BTreeSet<Device>) {
        if !power {
            return;
        }
        let prepared = match self.prepare(memory, observed) {
            Ok(prepared) => prepared,
            Err(reason) => {
                memory.execution_limitations.insert(reason);
                return;
            }
        };
        memory.set_system(1022, prepared.carry);
        observed.insert(Memory::system_device(1022));
        observed.extend(self.target_modifier_devices());
        match self.resolved_target(memory) {
            Ok(target) => {
                target.devices(observed);
                target.write(memory, prepared.value);
            }
            Err(reason) => {
                memory.execution_limitations.insert(reason);
            }
        }
    }
    pub fn dest(&self) -> Device {
        self.target.base().expect("rotation destination")
    }
    pub fn label(&self) -> String {
        let name = match self.kind {
            Rotation::Right => "ROR",
            Rotation::Left => "ROL",
            Rotation::CarryRight => "RCR",
            Rotation::CarryLeft => "RCL",
        };
        format!(
            "{}{name}{} {} K{}",
            if self.wide { "D" } else { "" },
            if self.pulse_label { "P" } else { "" },
            self.target,
            self.count
        )
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        self.target.devices(out);
        out.extend((1020..=1022).map(Memory::system_device));
        Memory::error_devices(out);
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        self.target.modifier_devices()
    }
    /// Held-monitor inventory, distinct from actual prepare/execute visits.
    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Ok(value) = self.resolved_source(0, memory) {
            value.devices(out);
        }
    }
    pub fn display_value(&self, memory: &Memory) -> String {
        match self.resolved_target(memory) {
            Ok(target) => format!(
                "{} C:{}",
                target.read(memory),
                u8::from(memory.system_bit(1022))
            ),
            Err(reason) => format!("İndeks: {reason}"),
        }
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let mut header = match self.kind {
            Rotation::Right => 0x4c00,
            Rotation::Left => 0x4e00,
            Rotation::CarryRight => 0x5000,
            Rotation::CarryLeft => 0x5200,
        };
        if self.wide {
            header += 0x100;
        }
        if self.pulse_label {
            header |= 0x10;
        }
        let mut words = vec![header];
        words.extend(self.target.native_words()?);
        words.extend(crate::es2_encoder::calendar_operand(&frozen_value(
            i32::from(self.count),
            self.wide,
        ))?);
        Ok(words)
    }
}
