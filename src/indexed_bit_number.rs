//! E/F address suffixes for six numeric bit writers and twelve numeric
//! bit contacts (PDF748..756). This is not the bare-bit indexed instruction
//! family. No P forms, no global CalendarValue wide-F exception.
//! Qualified reads snapshot the first scalar before n; writers then revisit
//! the original target and cache its effective span for the final RMW store.
use crate::additional_processing::AdditionalValue;
use crate::bit_index_operand::BitIndexValue;
use crate::bit_tools::BitWriteKind;
use crate::calendar_operand::{CalendarDestination, CalendarValue};
use crate::indexed_multiply::{qualify_group, validate_model};
use crate::indexed_operand::{AddressedValue, has_modifier, has_modifiers};
use crate::numeric_contacts::{ContactKind, parse_spec};
use crate::plc::{Area, Device, Memory, Operand};
use crate::wide::WideOperand;
use std::collections::BTreeSet;
use std::fmt;

fn first_binding(raw: &str, wide: bool, write: bool) -> Result<AddressedValue, String> {
    let binding = AddressedValue::parse(raw, wide, write)?;
    let allowed = match &binding {
        AddressedValue::Direct(CalendarValue::Additional(value)) => match value {
            AdditionalValue::Group { base, .. } => matches!(base.area, Area::Y | Area::M | Area::S),
            AdditionalValue::Word(Operand::Register(base))
            | AdditionalValue::Double(WideOperand::Register(base)) => {
                matches!(base.area, Area::T | Area::C | Area::D)
            }
            _ => false,
        },
        AddressedValue::Modified {
            address, digits, ..
        } => match digits {
            Some(_) => matches!(address.base.area, Area::Y | Area::M | Area::S),
            None => matches!(address.base.area, Area::T | Area::C | Area::D),
        },
        _ => false,
    };
    if !allowed {
        return Err("Bit numaralı komutun ilk operandı KnY/KnM/KnS veya T/C/D olmalı".into());
    }
    Ok(binding)
}

fn qualified_value(
    binding: &AddressedValue,
    memory: &Memory,
    write: bool,
) -> Result<CalendarValue, String> {
    let value = binding.resolve(memory, write)?;
    if let CalendarValue::Additional(AdditionalValue::Group { base, digits, .. }) = &value {
        qualify_group(
            binding.base().expect("group base"),
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

#[derive(Debug, Clone, PartialEq)]
enum NumberBinding {
    Direct(BitIndexValue),
    Modified(AddressedValue),
}
impl NumberBinding {
    fn parse(raw: &str, wide: bool) -> Result<Self, String> {
        if has_modifier(raw) {
            return AddressedValue::parse(raw, wide, false).map(Self::Modified);
        }
        let value = BitIndexValue::parse(raw, wide)?;
        let limit = if wide { 31 } else { 15 };
        if value.constant().is_some_and(|n| !(0..=limit).contains(&n)) {
            return Err(format!("Sabit bit numarası 0..{limit} aralığında olmalı"));
        }
        Ok(Self::Direct(value))
    }
    fn resolve(&self, memory: &Memory) -> Result<CalendarValue, String> {
        match self {
            Self::Modified(value) => qualified_value(value, memory, false),
            Self::Direct(value) => {
                let value = match value.calendar_value() {
                    Some(value) => value.clone(),
                    None => {
                        // Runtime-qualified compatibility: shipped WPL/ISP validators reject
                        // double-width direct F, although the emitter and current
                        // native runtime support this existing scoped extension.
                        // This n-only F-double descriptor reads its canonical
                        // F alias AND the following D word, including F7.
                        // Storage projection never replaces the encoded F tag.
                        let mut devices = BTreeSet::new();
                        value.devices(&mut devices);
                        let low = *devices.first().expect("wide F pair");
                        CalendarValue::Additional(AdditionalValue::Double(WideOperand::Register(
                            low,
                        )))
                    }
                };
                qualified_value(&AddressedValue::Direct(value), memory, false)
            }
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
                    value.calendar_value().expect("ordinary n"),
                ),
            },
        }
    }
}
impl fmt::Display for NumberBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Direct(value) => value.fmt(f),
            Self::Modified(value) => value.fmt(f),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Operands {
    first: AddressedValue,
    number: NumberBinding,
    wide: bool,
}
impl Operands {
    fn parse(args: &[String], wide: bool, write: bool) -> Result<Self, String> {
        if args.len() != 2 {
            return Err("Bit numaralı komut iki operand gerektirir".into());
        }
        Ok(Self {
            first: first_binding(&args[0], wide, write)?,
            number: NumberBinding::parse(&args[1], wide)?,
            wide,
        })
    }
    fn source_modifier_devices(&self, index: usize) -> Vec<Device> {
        match index {
            0 => self.first.modifier_devices(),
            1 => self.number.modifier_devices(),
            _ => Vec::new(),
        }
    }
    fn resolved_source(&self, index: usize, memory: &Memory) -> Result<CalendarValue, String> {
        match index {
            0 => qualified_value(&self.first, memory, false),
            1 => self.number.resolve(memory),
            _ => Err("Bit numaralı komut kaynak indeksi 0 veya 1 olmalı".into()),
        }
    }
    fn snapshot(
        &self,
        index: usize,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<i32, String> {
        observed.extend(self.source_modifier_devices(index));
        let value = self.resolved_source(index, memory)?;
        crate::indexed_operand::read_resolved_value(&value, memory, observed)
    }
    fn snapshot_pair(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<(u32, u32), String> {
        let old = self.snapshot(0, memory, observed)? as u32;
        let number = self.snapshot(1, memory, observed)? as u32 & if self.wide { 31 } else { 15 };
        Ok((old, number))
    }
    fn devices(&self, out: &mut BTreeSet<Device>) {
        self.first.devices(out);
        self.number.devices(out);
        Memory::error_devices(out);
    }
    fn modifier_devices(&self) -> Vec<Device> {
        let mut devices = self.first.modifier_devices();
        for device in self.number.modifier_devices() {
            if !devices.contains(&device) {
                devices.push(device);
            }
        }
        devices
    }
    fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        for index in 0..2 {
            if let Ok(value) = self.resolved_source(index, memory) {
                value.devices(out);
            }
        }
    }
    fn native_words(&self, header: u16) -> Result<Vec<u16>, String> {
        let mut words = vec![header];
        words.extend(self.first.native_words()?);
        words.extend(self.number.native_words()?);
        Ok(words)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedBitNumber {
    target: CalendarDestination,
    old: u32,
    number: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedBitNumberAction {
    kind: BitWriteKind,
    operands: Operands,
}
impl IndexedBitNumberAction {
    pub fn supports(name: &str) -> bool {
        matches!(
            name.to_ascii_uppercase().as_str(),
            "BOUT" | "DBOUT" | "BSET" | "DBSET" | "BRST" | "DBRST"
        )
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        has_modifiers(args)
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let upper = name.to_ascii_uppercase();
        let kind = match upper.as_str() {
            "BOUT" | "DBOUT" => BitWriteKind::Out,
            "BSET" | "DBSET" => BitWriteKind::Set,
            "BRST" | "DBRST" => BitWriteKind::Reset,
            _ => {
                return Err(
                    "İndeksli bit yazıcı BOUT/BSET/BRST veya D biçimi olmalı; P biçimi yok".into(),
                );
            }
        };
        Ok(Self {
            kind,
            operands: Operands::parse(args, upper.starts_with('D'), true)?,
        })
    }
    pub fn wide(&self) -> bool {
        self.operands.wide
    }
    pub fn is_out(&self) -> bool {
        self.kind == BitWriteKind::Out
    }
    pub fn should_resolve(&self, power: bool) -> bool {
        self.is_out() || power
    }
    pub fn source_count(&self) -> usize {
        2
    }
    pub fn source_modifier_devices(&self, index: usize) -> Vec<Device> {
        self.operands.source_modifier_devices(index)
    }
    pub fn target_modifier_devices(&self) -> Vec<Device> {
        self.operands.first.modifier_devices()
    }
    pub fn resolved_source(&self, index: usize, memory: &Memory) -> Result<CalendarValue, String> {
        self.operands.resolved_source(index, memory)
    }
    fn resolved_target(&self, memory: &Memory) -> Result<CalendarDestination, String> {
        CalendarDestination::from_value(qualified_value(&self.operands.first, memory, true)?)
    }
    /// Pure qualification after the bridge captures each real source in order.
    /// Power is deliberately absent: BOUT false still accesses both operands.
    pub fn prepare(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<PreparedBitNumber, String> {
        let (old, number) = self.operands.snapshot_pair(memory, observed)?;
        observed.extend(self.target_modifier_devices());
        let target = self.resolved_target(memory)?;
        target.devices(observed);
        Ok(PreparedBitNumber {
            target,
            old,
            number,
        })
    }
    pub fn execute(&self, memory: &mut Memory, power: bool, observed: &mut BTreeSet<Device>) {
        if !self.should_resolve(power) {
            return;
        }
        let prepared = match self.prepare(memory, observed) {
            Ok(prepared) => prepared,
            Err(reason) => {
                memory.execution_limitations.insert(reason);
                return;
            }
        };
        let on = match self.kind {
            BitWriteKind::Out => power,
            BitWriteKind::Set => true,
            BitWriteKind::Reset => false,
        };
        let mask = 1u32 << prepared.number;
        let result = if on {
            prepared.old | mask
        } else {
            prepared.old & !mask
        };
        // Full scalar RMW is truncated by the descriptor width, not a physical
        // base+n bit store. Both halves keep this cached effective destination.
        prepared.target.write(memory, result as i32);
    }
    pub fn dest(&self) -> Device {
        self.operands.first.base().expect("numeric bit target")
    }
    pub fn label(&self) -> String {
        let name = match self.kind {
            BitWriteKind::Out => "BOUT",
            BitWriteKind::Set => "BSET",
            BitWriteKind::Reset => "BRST",
        };
        format!(
            "{}{name} {} {}",
            if self.wide() { "D" } else { "" },
            self.operands.first,
            self.operands.number
        )
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        self.operands.devices(out);
        out.extend((1020..=1022).map(Memory::system_device));
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        self.operands.modifier_devices()
    }
    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        self.operands.observed_devices(memory, out);
        out.extend((1020..=1022).map(Memory::system_device));
    }
    pub fn display_value(&self, memory: &Memory) -> String {
        match self.resolved_target(memory) {
            Ok(target) => target.read(memory).to_string(),
            Err(reason) => format!("İndeks: {reason}"),
        }
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let base = match self.kind {
            BitWriteKind::Out => 0x3402,
            BitWriteKind::Set => 0x3602,
            BitWriteKind::Reset => 0x3802,
        };
        self.operands
            .native_words(base + if self.wide() { 0x100 } else { 0 })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedBitNumberPredicate {
    operands: Operands,
    inverted: bool,
}
impl IndexedBitNumberPredicate {
    pub fn supports(name: &str) -> bool {
        parse_spec(&name.to_ascii_uppercase())
            .is_some_and(|spec| matches!(spec.kind, ContactKind::Indexed(_)))
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        has_modifiers(args)
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let spec = parse_spec(&name.to_ascii_uppercase()).ok_or("Bit numaralı kontak geçersiz")?;
        let ContactKind::Indexed(inverted) = spec.kind else {
            return Err("BLD/BLDI/BAND/BANI/BOR/BORI veya D biçimi gerekli; P biçimi yok".into());
        };
        Ok(Self {
            operands: Operands::parse(args, spec.wide, false)?,
            inverted,
        })
    }
    pub fn wide(&self) -> bool {
        self.operands.wide
    }
    pub fn source_count(&self) -> usize {
        2
    }
    pub fn source_modifier_devices(&self, index: usize) -> Vec<Device> {
        self.operands.source_modifier_devices(index)
    }
    pub fn resolved_source(&self, index: usize, memory: &Memory) -> Result<CalendarValue, String> {
        self.operands.resolved_source(index, memory)
    }
    pub fn short_circuits(&self) -> bool {
        true
    }
    pub fn evaluate(&self, memory: &Memory) -> Result<bool, String> {
        self.visit(memory, &mut BTreeSet::new())
    }
    /// Caller gates AND on true and OR on false. A failed qualified source
    /// stays Err through NC and outer INV; it must never become an ON result.
    /// BOR's separate malformed-native-address continuation is not emulated.
    pub fn visit(&self, memory: &Memory, observed: &mut BTreeSet<Device>) -> Result<bool, String> {
        let (value, number) = self.operands.snapshot_pair(memory, observed)?;
        Ok((value & (1u32 << number) != 0) ^ self.inverted)
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        self.operands.devices(out);
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        self.operands.modifier_devices()
    }
    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        self.operands.observed_devices(memory, out);
    }
    pub fn label(&self, prefix: &str) -> String {
        let name = match (prefix, self.inverted) {
            ("LD", false) => "BLD",
            ("LD", true) => "BLDI",
            ("AND", false) => "BAND",
            ("AND", true) => "BANI",
            ("OR", false) => "BOR",
            ("OR", true) => "BORI",
            _ => prefix,
        };
        format!(
            "{}{name} {} {}",
            if self.wide() { "D" } else { "" },
            self.operands.first,
            self.operands.number
        )
    }
    pub fn display_value(&self, memory: &Memory) -> String {
        match self.evaluate(memory) {
            Ok(value) => u8::from(value).to_string(),
            Err(reason) => format!("İndeks: {reason}"),
        }
    }
    pub fn native_words(&self, prefix: &str) -> Result<Vec<u16>, String> {
        let base = match (prefix, self.inverted) {
            ("LD", false) => 0x3a02,
            ("LD", true) => 0x3c02,
            ("AND", false) => 0x3e02,
            ("AND", true) => 0x4002,
            ("OR", false) => 0x4202,
            ("OR", true) => 0x4402,
            _ => return Err("Bit numaralı kontak öneki LD/AND/OR olmalı".into()),
        };
        self.operands
            .native_words(base + if self.wide() { 0x100 } else { 0 })
    }
}
