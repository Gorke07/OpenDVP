//! Scoped PDF623/627 SWAP/MVM modifiers. Direct wide F belongs to MVM's
//! operand matrix only; global CalendarValue and other commands stay narrow.
use crate::additional_processing::AdditionalValue;
use crate::bit_index_operand::BitIndexValue;
use crate::calendar_operand::{CalendarDestination, CalendarValue};
use crate::indexed_multiply::{qualify_group, validate_model};
use crate::indexed_operand::{AddressedValue, has_modifier, has_modifiers};
use crate::mask_random_operand::MaskRandomDestination;
use crate::plc::{Device, Memory};
use crate::wide::WideOperand;
use std::collections::BTreeSet;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
enum Binding {
    Direct(BitIndexValue),
    Modified(AddressedValue),
}
impl Binding {
    fn parse(raw: &str, wide: bool, write: bool, swap: bool) -> Result<Self, String> {
        if has_modifier(raw) {
            return AddressedValue::parse(raw, wide, write).map(Self::Modified);
        }
        if write {
            if swap {
                CalendarDestination::parse(raw, wide)?;
            } else {
                MaskRandomDestination::parse(raw, wide)?;
            }
        }
        BitIndexValue::parse(raw, wide).map(Self::Direct)
    }
    fn constant(&self) -> Option<i32> {
        match self {
            Self::Direct(value) => value.constant(),
            Self::Modified(_) => None,
        }
    }
    fn value(&self, memory: &Memory, write: bool) -> Result<CalendarValue, String> {
        let value = match self {
            Self::Modified(binding) => binding.resolve(memory, write)?,
            Self::Direct(value) => match value.calendar_value() {
                Some(value) => value.clone(),
                None => {
                    // The scoped direct-wide-F exception has two canonical D
                    // words. Projection is for actual storage only; encoding
                    // and labels retain the original F descriptor.
                    let mut devices = BTreeSet::new();
                    value.devices(&mut devices);
                    let base = *devices.first().expect("wide F has canonical D storage");
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
                    value.calendar_value().expect("ordinary calendar operand"),
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
pub struct PreparedSwapMask {
    target: CalendarDestination,
    value: i32,
}
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedSwapMaskAction {
    swap: bool,
    wide: bool,
    pulse_label: bool,
    // SWAP has only its in-place target; MVM contains S1, mask, then target.
    operands: Vec<Binding>,
}
impl IndexedSwapMaskAction {
    fn spec(name: &str) -> Option<(bool, bool, bool)> {
        Some(match name.to_ascii_uppercase().as_str() {
            "SWAP" => (true, false, false),
            "SWAPP" => (true, false, true),
            "DSWAP" => (true, true, false),
            "DSWAPP" => (true, true, true),
            "MVM" => (false, false, false),
            "MVMP" => (false, false, true),
            "DMVM" => (false, true, false),
            "DMVMP" => (false, true, true),
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
        let (swap, wide, pulse_label) =
            Self::spec(name).ok_or("İndeksli SWAP/MVM komutu geçersiz")?;
        let count = if swap { 1 } else { 3 };
        if args.len() != count {
            return Err(format!("{name}: {count} operand gerekli"));
        }
        let operands = args
            .iter()
            .enumerate()
            .map(|(i, raw)| Binding::parse(raw, wide, i == count - 1, swap))
            .collect::<Result<Vec<_>, _>>()?;
        if !swap && operands[0].constant().is_some() {
            return Err("MVM ilk kaynağı Kn veya T/C/D/E/F olmalı; K/H sabiti kullanılamaz".into());
        }
        Ok(Self {
            swap,
            wide,
            pulse_label,
            operands,
        })
    }
    pub fn source_count(&self) -> usize {
        self.operands.len()
    }
    pub fn source_modifier_devices(&self, index: usize) -> Vec<Device> {
        self.operands
            .get(index)
            .map(Binding::modifier_devices)
            .unwrap_or_default()
    }
    pub fn target_modifier_devices(&self) -> Vec<Device> {
        self.operands.last().expect("target").modifier_devices()
    }
    pub fn resolved_source(&self, index: usize, memory: &Memory) -> Result<CalendarValue, String> {
        let value = self
            .operands
            .get(index)
            .ok_or("SWAP/MVM kaynak indeksi geçersiz")?;
        value.value(memory, index == self.operands.len() - 1)
    }
    fn resolved_target(&self, memory: &Memory) -> Result<CalendarDestination, String> {
        CalendarDestination::from_value(self.operands.last().expect("target").value(memory, true)?)
    }
    fn snapshot(
        &self,
        index: usize,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<i32, String> {
        observed.extend(self.source_modifier_devices(index));
        let source = self.resolved_source(index, memory)?;
        crate::indexed_operand::read_resolved_value(&source, memory, observed)
    }
    pub fn prepare(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<PreparedSwapMask, String> {
        let value = if self.swap {
            let raw = self.snapshot(0, memory, observed)? as u32;
            // Permute bytes inside each WORD; do not exchange the two WORDs.
            let swapped = ((raw & 0x00ff_00ff) << 8) | ((raw & 0xff00_ff00) >> 8);
            if self.wide {
                swapped as i32
            } else {
                i32::from(swapped as i16)
            }
        } else {
            let source = self.snapshot(0, memory, observed)?;
            let mask = self.snapshot(1, memory, observed)?;
            let old = self.snapshot(2, memory, observed)?;
            (old & !mask) | (source & mask)
        };
        // Native generic writer rereads the target modifier after all reads;
        // once resolved, its low/high writes use this fixed destination.
        observed.extend(self.target_modifier_devices());
        let target = self.resolved_target(memory)?;
        target.devices(observed);
        Ok(PreparedSwapMask { target, value })
    }
    pub fn execute(&self, memory: &mut Memory, power: bool, observed: &mut BTreeSet<Device>) {
        if !power {
            return;
        }
        match self.prepare(memory, observed) {
            Ok(prepared) => prepared.target.write(memory, prepared.value),
            Err(reason) => {
                memory.execution_limitations.insert(reason);
            }
        }
    }
    pub fn dest(&self) -> Device {
        self.operands
            .last()
            .expect("target")
            .base()
            .expect("writable target")
    }
    pub fn label(&self) -> String {
        let mut parts = vec![format!(
            "{}{}{}",
            if self.wide { "D" } else { "" },
            if self.swap { "SWAP" } else { "MVM" },
            if self.pulse_label { "P" } else { "" }
        )];
        parts.extend(self.operands.iter().map(ToString::to_string));
        parts.join(" ")
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        for value in &self.operands {
            value.devices(out);
        }
        out.extend((1020..=1022).map(Memory::system_device));
        Memory::error_devices(out);
    }
    /// First-use order is source, mask, old target; direct E/F values are data,
    /// not address modifiers. Staged bridge reads use source_modifier_devices.
    pub fn modifier_devices(&self) -> Vec<Device> {
        let mut devices = Vec::new();
        for d in self.operands.iter().flat_map(Binding::modifier_devices) {
            if !devices.contains(&d) {
                devices.push(d);
            }
        }
        devices
    }
    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        for i in 0..self.source_count() {
            if let Ok(source) = self.resolved_source(i, memory) {
                source.devices(out);
            }
        }
    }
    pub fn display_value(&self, memory: &Memory) -> String {
        match self.resolved_target(memory) {
            Ok(target) => target.read(memory).to_string(),
            Err(reason) => format!("İndeks: {reason}"),
        }
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let header = (if self.swap { 0x4601 } else { 0x6e01 })
            + if self.wide { 0x100 } else { 0 }
            + if self.pulse_label { 0x10 } else { 0 };
        let mut words = vec![header];
        for value in &self.operands {
            words.extend(value.native_words()?);
        }
        Ok(words)
    }
}
