//! Addressed BMOV/FMOV/XCH family. DFMOV also admits complete direct
//! C199 -> natural C200 cursor transitions. Incomplete T/D double endpoints,
//! unsafe native packed spans and malformed native fault continuation are not
//! claimed equivalent. Native qualification checks complete model spans.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::{CalendarDestination, CalendarValue};
use crate::indexed_multiply::{qualify_group, validate_model};
use crate::indexed_operand::{AddressedValue, frozen_value, has_modifiers, read_resolved_value};
use crate::plc::{Area, Device, Memory, Operand, word_range};
use crate::shift::bit_span;
use crate::wide::{WideOperand, double_devices};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Move {
        source: AddressedValue,
        target: AddressedValue,
        count: Operand,
    },
    Fill {
        source: AddressedValue,
        target: AddressedValue,
        count: u16,
        wide: bool,
    },
    Exchange {
        first: AddressedValue,
        second: AddressedValue,
        wide: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedBlockExchangeAction {
    kind: Kind,
    pulse_label: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct Cursor {
    value: AdditionalValue,
}

#[derive(Debug, Clone, PartialEq)]
enum Fault {
    Range(String),
    Scope(String),
}
impl Fault {
    fn reason(self) -> String {
        match self {
            Self::Range(s) | Self::Scope(s) => s,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
struct ScratchPatch {
    high: Option<u8>,
    low: Option<u8>,
}
impl ScratchPatch {
    fn apply(&self, memory: &mut Memory) {
        memory.native_numeric_scratch = match (self.high, self.low, memory.native_numeric_scratch) {
            (Some(h), Some(l), _) => Some(i16::from_be_bytes([h, l])),
            (Some(h), None, Some(old)) => Some(((u16::from(h) << 8) | (old as u16 & 0xff)) as i16),
            (None, Some(l), Some(old)) => Some(((old as u16 & 0xff00) | u16::from(l)) as i16),
            (_, _, old) => old,
        };
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Ready {
    Move {
        source: Cursor,
        target: Cursor,
        count: i16,
    },
    Fill {
        value: i32,
        target: Cursor,
        count: u16,
    },
    Exchange {
        first: i32,
        second: i32,
        swap_second: bool,
        wide: bool,
    },
}
#[derive(Debug, Clone, PartialEq)]
pub struct PreparedBlockExchange {
    ready: Ready,
}

fn numeric_kind(value: &AdditionalValue) -> Option<u8> {
    if matches!(value, AdditionalValue::Group { .. }) {
        return None;
    }
    Some(match value.base().expect("numeric block").area {
        Area::T => 0x82,
        Area::C => 0x84,
        Area::D => 0x86,
        _ => unreachable!(),
    })
}
fn digits(value: &AddressedValue) -> Option<u16> {
    match value {
        AddressedValue::Modified { digits, .. } => *digits,
        AddressedValue::Direct(CalendarValue::Additional(AdditionalValue::Group {
            digits,
            ..
        })) => Some(*digits),
        _ => None,
    }
}
fn resolve_value(
    binding: &AddressedValue,
    memory: &Memory,
    write: bool,
    special: bool,
) -> Result<CalendarValue, String> {
    let value = binding.resolve(memory, write)?;
    if (!special || matches!(binding, AddressedValue::Modified { .. }))
        && let CalendarValue::Additional(AdditionalValue::Group { base, digits, .. }) = &value
    {
        qualify_group(
            binding.base().expect("group address"),
            *base,
            digits * 4,
            special,
        )?;
    }
    let mut devices = BTreeSet::new();
    value.devices(&mut devices);
    validate_model(memory, &devices)?;
    Ok(value)
}
fn block_binding(raw: &str, wide: bool, write: bool) -> Result<AddressedValue, String> {
    let value = AddressedValue::parse(raw, wide, write)?;
    if matches!(&value, AddressedValue::Direct(CalendarValue::Index { .. }))
        || value.base().is_none()
    {
        return Err("Blok adresi Kn veya T/C/D olmalı; doğrudan E/F ve sabit olamaz".into());
    }
    Ok(value)
}

impl Cursor {
    fn resolve(binding: &AddressedValue, memory: &Memory, write: bool) -> Result<Self, String> {
        let CalendarValue::Additional(value) = resolve_value(binding, memory, write, true)? else {
            return Err("Blok adresi doğrudan E/F olamaz".into());
        };
        Ok(Self { value })
    }
    fn base(&self) -> Device {
        self.value.base().expect("block cursor")
    }
    fn raw_at(&self, offset: u16) -> Result<AdditionalValue, Fault> {
        let base = self.base();
        let mut index = base.index;
        for _ in 0..offset {
            let step = match &self.value {
                AdditionalValue::Group { digits, .. } => digits * 4,
                AdditionalValue::Double(_) if !(base.area == Area::C && index >= 199) => 2,
                _ => 1,
            };
            index = index
                .checked_add(step)
                .ok_or_else(|| Fault::Range("Blok adresi taşıyor".into()))?;
        }
        let next = Device { index, ..base };
        Ok(match &self.value {
            AdditionalValue::Group { digits, wide, .. } => AdditionalValue::Group {
                base: next,
                digits: *digits,
                wide: *wide,
            },
            AdditionalValue::Double(_) => AdditionalValue::Double(WideOperand::Register(next)),
            AdditionalValue::Word(_) => AdditionalValue::Word(Operand::Register(next)),
        })
    }
    fn validate_bit(&self, memory: &Memory, bit: Device, write: bool) -> Result<(), Fault> {
        bit_span(bit, 1, write).map_err(Fault::Range)?;
        validate_model(memory, &BTreeSet::from([bit])).map_err(Fault::Range)?;
        Ok(())
    }
    fn element(&self, memory: &Memory, offset: u16, write: bool) -> Result<CalendarValue, Fault> {
        let value = self.raw_at(offset)?;
        let devices = match &value {
            AdditionalValue::Group { base, digits, .. } => {
                let span = bit_span(*base, digits * 4, write).map_err(Fault::Range)?;
                for bit in &span {
                    self.validate_bit(memory, *bit, write)?;
                }
                span
            }
            AdditionalValue::Word(Operand::Register(base)) => {
                word_range(*base, 1).map_err(Fault::Range)?
            }
            AdditionalValue::Double(WideOperand::Register(base)) => {
                double_devices(*base).map_err(Fault::Range)?
            }
            _ => unreachable!("validated block cursor"),
        };
        validate_model(memory, &devices.into_iter().collect()).map_err(Fault::Range)?;
        Ok(CalendarValue::Additional(value))
    }
    fn span(&self, memory: &Memory, count: u16, write: bool) -> Result<BTreeSet<Device>, String> {
        let mut devices = BTreeSet::new();
        for i in 0..count {
            self.element(memory, i, write)
                .map_err(Fault::reason)?
                .devices(&mut devices);
        }
        Ok(devices)
    }
    fn read_move(
        &self,
        memory: &Memory,
        offset: u16,
        more: bool,
        observed: &mut BTreeSet<Device>,
    ) -> Result<i32, Fault> {
        let value = self.element(memory, offset, false)?;
        let snapshot = read_resolved_value(&value, memory, observed).map_err(Fault::Scope)?;
        if more
            && let CalendarValue::Additional(AdditionalValue::Group { base, digits, .. }) = value
        {
            self.validate_bit(
                memory,
                Device {
                    index: base.index + digits * 4,
                    ..base
                },
                false,
            )?;
        }
        Ok(snapshot)
    }
    fn write_move(
        &self,
        memory: &mut Memory,
        offset: u16,
        more: bool,
        value: i32,
        observed: &mut BTreeSet<Device>,
    ) -> Result<(), Fault> {
        if let AdditionalValue::Group { base, digits, .. } = self.raw_at(offset)? {
            for n in 0..digits * 4 {
                let bit = Device {
                    index: base.index + n,
                    ..base
                };
                self.validate_bit(memory, bit, true)?;
                observed.insert(bit);
                memory.set_internal(bit, value as u32 & (1 << n) != 0);
                if n + 1 < digits * 4 || more {
                    self.validate_bit(
                        memory,
                        Device {
                            index: bit.index + 1,
                            ..bit
                        },
                        true,
                    )?;
                }
            }
        } else {
            let target = CalendarDestination::from_value(self.element(memory, offset, true)?)
                .map_err(Fault::Scope)?;
            target.devices(observed);
            target.write(memory, value);
        }
        Ok(())
    }
}

impl IndexedBlockExchangeAction {
    fn spec(name: &str) -> Option<(&'static str, bool)> {
        let upper = name.to_ascii_uppercase();
        let pulse = upper.ends_with('P');
        let stem = upper.strip_suffix('P').unwrap_or(&upper);
        Some((
            match stem {
                "BMOV" => "BMOV",
                "FMOV" => "FMOV",
                "DFMOV" => "DFMOV",
                "XCH" => "XCH",
                "DXCH" => "DXCH",
                _ => return None,
            },
            pulse,
        ))
    }
    pub fn supports(name: &str) -> bool {
        Self::spec(name).is_some()
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        has_modifiers(args)
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let (stem, pulse_label) = Self::spec(name).ok_or("İndeksli blok/takas komutu geçersiz")?;
        let exchange = matches!(stem, "XCH" | "DXCH");
        if args.len() != if exchange { 2 } else { 3 } {
            return Err(format!("{name}: operand sayısı geçersiz"));
        }
        let wide = matches!(stem, "DFMOV" | "DXCH");
        let kind = if exchange {
            Kind::Exchange {
                first: AddressedValue::parse(&args[0], wide, true)?,
                second: AddressedValue::parse(&args[1], wide, true)?,
                wide,
            }
        } else if stem == "BMOV" {
            let source = block_binding(&args[0], false, false)?;
            let target = block_binding(&args[1], false, true)?;
            if let (Some(a), Some(b)) = (digits(&source), digits(&target))
                && a != b
            {
                return Err("BMOV kaynak/hedef Kn genişlikleri aynı olmalı".into());
            }
            let count = Operand::parse(&args[2].to_ascii_uppercase())?;
            if let Operand::Constant(n) = count
                && !(1..=512).contains(&n)
            {
                return Err("BMOV sayısı K/H1..512 olmalı".into());
            }
            Kind::Move {
                source,
                target,
                count,
            }
        } else {
            let count = match Operand::parse(&args[2].to_ascii_uppercase())? {
                Operand::Constant(n) if (1..=512).contains(&n) => n as u16,
                _ => return Err("FMOV/DFMOV sayısı K/H1..512 sabiti olmalı".into()),
            };
            Kind::Fill {
                source: AddressedValue::parse(&args[0], wide, false)?,
                target: block_binding(&args[1], wide, true)?,
                count,
                wide,
            }
        };
        let action = Self { kind, pulse_label };
        // Direct ranges retain static rejection. The new scoped cursor permits
        // complete C198/C199 -> C200 transitions without widening other blocks.
        let memory = Memory::default();
        match &action.kind {
            Kind::Fill {
                target: AddressedValue::Direct(_),
                count,
                ..
            } => {
                action.block_target(&memory)?.span(&memory, *count, true)?;
            }
            Kind::Move {
                source,
                target,
                count: Operand::Constant(n),
            } => {
                if matches!(source, AddressedValue::Direct(_)) {
                    Cursor::resolve(source, &memory, false)?.span(&memory, *n as u16, false)?;
                }
                if matches!(target, AddressedValue::Direct(_)) {
                    Cursor::resolve(target, &memory, true)?.span(&memory, *n as u16, true)?;
                }
            }
            _ => {}
        }
        Ok(action)
    }
    fn bindings(&self) -> [&AddressedValue; 2] {
        match &self.kind {
            Kind::Move { source, target, .. } | Kind::Fill { source, target, .. } => {
                [source, target]
            }
            Kind::Exchange { first, second, .. } => [first, second],
        }
    }
    pub fn wide(&self) -> bool {
        matches!(
            self.kind,
            Kind::Fill { wide: true, .. } | Kind::Exchange { wide: true, .. }
        )
    }
    pub fn control_devices(&self) -> Vec<Device> {
        if matches!(self.kind, Kind::Exchange { .. }) {
            vec![Memory::system_device(1303)]
        } else {
            Vec::new()
        }
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        self.bindings()
            .into_iter()
            .flat_map(AddressedValue::modifier_devices)
            .collect()
    }
    pub fn count_device(&self) -> Option<Device> {
        if let Kind::Move {
            count: Operand::Register(d),
            ..
        } = self.kind
        {
            Some(d)
        } else {
            None
        }
    }
    fn read_count(&self, memory: &Memory, observed: &mut BTreeSet<Device>) -> Result<i16, String> {
        let Kind::Move { count, .. } = &self.kind else {
            return Err("Dinamik blok sayısı yalnız BMOV için geçerli".into());
        };
        let value = CalendarValue::Additional(AdditionalValue::Word(count.clone()));
        let mut devices = BTreeSet::new();
        value.devices(&mut devices);
        validate_model(memory, &devices)?;
        Ok(read_resolved_value(&value, memory, observed)? as i16)
    }
    fn block_target(&self, memory: &Memory) -> Result<Cursor, String> {
        Cursor::resolve(self.bindings()[1], memory, true)
    }
    fn exchange_target(
        &self,
        index: usize,
        memory: &Memory,
    ) -> Result<CalendarDestination, String> {
        let binding = self
            .bindings()
            .get(index)
            .copied()
            .ok_or("Takas operandı 0/1 olmalı")?;
        CalendarDestination::from_value(resolve_value(binding, memory, true, false)?)
    }
    fn staged(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> (Result<Ready, String>, ScratchPatch) {
        let mut scratch = ScratchPatch::default();
        let result = (|| match &self.kind {
            Kind::Move { source, target, .. } => {
                observed.extend(source.modifier_devices());
                let source = Cursor::resolve(source, memory, false)?;
                scratch.high = numeric_kind(&source.value);
                observed.extend(target.modifier_devices());
                let target = Cursor::resolve(target, memory, true)?;
                scratch.low = numeric_kind(&target.value);
                let count = self.read_count(memory, observed)?;
                Ok(Ready::Move {
                    source,
                    target,
                    count,
                })
            }
            Kind::Fill {
                source,
                target,
                count,
                wide,
            } => {
                observed.extend(source.modifier_devices());
                let source = resolve_value(source, memory, false, false)?;
                let value = read_resolved_value(&source, memory, observed)?;
                observed.extend(target.modifier_devices());
                let target = Cursor::resolve(target, memory, true)?;
                if *wide {
                    scratch.high = numeric_kind(&target.value);
                } else {
                    scratch.low = Some(match &target.value {
                        AdditionalValue::Group { digits, .. } => (0x80 | (2 * digits) as u8) & 0x86,
                        value => numeric_kind(value).expect("numeric cursor"),
                    });
                }
                Ok(Ready::Fill {
                    value,
                    target,
                    count: *count,
                })
            }
            Kind::Exchange {
                first,
                second,
                wide,
            } => {
                observed.extend(first.modifier_devices());
                let first_value = resolve_value(first, memory, false, false)?;
                let a = read_resolved_value(&first_value, memory, observed)?;
                observed.insert(Memory::system_device(1303));
                let mode = memory.system_bit(1303);
                if mode {
                    resolve_value(first, memory, false, true)?;
                }
                observed.extend(second.modifier_devices());
                let second_value = resolve_value(second, memory, false, false)?;
                let b = read_resolved_value(&second_value, memory, observed)?;
                if mode {
                    resolve_value(second, memory, false, true)?;
                }
                let same = mode
                    && CalendarDestination::from_value(first_value)?
                        .same_exchange_address(&CalendarDestination::from_value(second_value)?);
                Ok(Ready::Exchange {
                    first: a,
                    second: b,
                    swap_second: same,
                    wide: *wide,
                })
            }
        })();
        (result, scratch)
    }
    pub fn prepare(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<PreparedBlockExchange, String> {
        let (result, _) = self.staged(memory, observed);
        Ok(PreparedBlockExchange { ready: result? })
    }
    /// Address-only collection for initial native storage capture. The bridge
    /// first captures control_devices, modifiers and count_device. Payload is
    /// never captured for invalid dynamic counts, but both bases qualify first.
    pub fn capture_devices(&self, memory: &Memory) -> Result<BTreeSet<Device>, String> {
        let mut devices = BTreeSet::new();
        match &self.kind {
            Kind::Move { source, target, .. } => {
                let source = Cursor::resolve(source, memory, false)?;
                let target = Cursor::resolve(target, memory, true)?;
                let n = self.read_count(memory, &mut BTreeSet::new())?;
                if (1..=512).contains(&n) {
                    devices.extend(source.span(memory, n as u16, false)?);
                    let targets = target.span(memory, n as u16, true)?;
                    if target.base().area == Area::M
                        && target.base().index <= 1535
                        && targets.iter().any(|d| d.index >= 1536)
                    {
                        return Err(
                            "BMOV hedefinin M1535 geçişinde native pointer hatası mevcut".into(),
                        );
                    }
                }
            }
            Kind::Fill {
                source,
                target,
                count,
                ..
            } => {
                resolve_value(source, memory, false, false)?.devices(&mut devices);
                Cursor::resolve(target, memory, true)?.span(memory, *count, true)?;
            }
            Kind::Exchange { first, second, .. } => {
                for binding in [first, second] {
                    resolve_value(binding, memory, false, false)?.devices(&mut devices);
                    if memory.system_bit(1303) {
                        resolve_value(binding, memory, false, true)?;
                    }
                }
            }
        }
        Ok(devices)
    }
    /// Pure native safety check. Exchange's second store is replayed ONLY into
    /// a clone, because it can change the first operand's modifier word.
    pub fn qualify_native(&self, memory: &Memory) -> Result<(), String> {
        self.capture_devices(memory)?;
        let prepared = self.prepare(memory, &mut BTreeSet::new())?;
        if let Ready::Exchange {
            first, swap_second, ..
        } = prepared.ready
        {
            let target = self.exchange_target(1, memory)?;
            if !swap_second {
                let mut clone = memory.clone();
                target.write(&mut clone, first);
                self.exchange_target(0, &clone)?;
            }
        }
        Ok(())
    }
    fn swapped(value: i32, wide: bool) -> i32 {
        if wide {
            (value as u32).rotate_left(16) as i32
        } else {
            i32::from((value as u16).swap_bytes() as i16)
        }
    }
    pub fn execute(&self, memory: &mut Memory, power: bool, observed: &mut BTreeSet<Device>) {
        if !power {
            return;
        }
        let (prepared, scratch) = self.staged(memory, observed);
        scratch.apply(memory);
        let prepared = match prepared {
            Ok(p) => p,
            Err(reason) => {
                memory.execution_limitations.insert(reason);
                return;
            }
        };
        match prepared {
            Ready::Move {
                source,
                target,
                count,
            } => {
                if !(1..=512).contains(&count) {
                    return;
                }
                for i in 0..count as u16 {
                    let more = i + 1 < count as u16;
                    let result = source
                        .read_move(memory, i, more, observed)
                        .and_then(|value| target.write_move(memory, i, more, value, observed));
                    match result {
                        Ok(()) => {}
                        Err(Fault::Range(_)) => {
                            memory.operation_error(0xe1a, 7);
                            break;
                        }
                        Err(Fault::Scope(reason)) => {
                            memory.execution_limitations.insert(reason);
                            break;
                        }
                    }
                }
            }
            Ready::Fill {
                value,
                target,
                count,
            } => {
                if matches!(target.value, AdditionalValue::Word(_))
                    && target.span(memory, count, true).is_err()
                {
                    return;
                }
                for i in 0..count {
                    let target = match target
                        .element(memory, i, true)
                        .map_err(Fault::reason)
                        .and_then(CalendarDestination::from_value)
                    {
                        Ok(target) => target,
                        Err(reason) => {
                            memory.execution_limitations.insert(reason);
                            break;
                        }
                    };
                    target.devices(observed);
                    target.write(memory, value);
                }
            }
            Ready::Exchange {
                first,
                second,
                swap_second,
                wide,
            } => {
                let result = (|| {
                    observed.extend(self.bindings()[1].modifier_devices());
                    let target = self.exchange_target(1, memory)?;
                    target.devices(observed);
                    target.write(
                        memory,
                        if swap_second {
                            Self::swapped(second, wide)
                        } else {
                            first
                        },
                    );
                    if !swap_second {
                        observed.extend(self.bindings()[0].modifier_devices());
                        let target = self.exchange_target(0, memory)?;
                        target.devices(observed);
                        target.write(memory, second);
                    }
                    Ok::<(), String>(())
                })();
                if let Err(reason) = result {
                    memory.execution_limitations.insert(reason);
                }
            }
        }
    }
    pub fn dest(&self) -> Device {
        self.bindings()[1].base().expect("block/exchange target")
    }
    pub fn label(&self) -> String {
        let p = if self.pulse_label { "P" } else { "" };
        match &self.kind {
            Kind::Move {
                source,
                target,
                count,
            } => format!("BMOV{p} {source} {target} {count}"),
            Kind::Fill {
                source,
                target,
                count,
                wide,
            } => format!(
                "{}FMOV{p} {source} {target} K{count}",
                if *wide { "D" } else { "" }
            ),
            Kind::Exchange {
                first,
                second,
                wide,
            } => format!("{}XCH{p} {first} {second}", if *wide { "D" } else { "" }),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        for value in self.bindings() {
            value.devices(out);
        }
        // A direct fill's complete constant span is statically known. Keep it
        // in Program.devices so model validation rejects e.g. D4998+2 DWORDs
        // on SS2 before upload. Modified bases still require captured indices.
        if let Kind::Fill {
            target: AddressedValue::Direct(_),
            count,
            ..
        } = &self.kind
            && let Ok(target) = self.block_target(&Memory::default())
            && let Ok(span) = target.span(&Memory::default(), *count, true)
        {
            out.extend(span);
        }
        if let Kind::Move {
            source,
            target,
            count: Operand::Constant(count),
        } = &self.kind
        {
            for (binding, write) in [(source, false), (target, true)] {
                if matches!(binding, AddressedValue::Direct(_))
                    && let Ok(cursor) = Cursor::resolve(binding, &Memory::default(), write)
                    && let Ok(span) = cursor.span(&Memory::default(), *count as u16, write)
                {
                    out.extend(span);
                }
            }
        }
        out.extend(self.control_devices());
        if let Some(count) = self.count_device() {
            out.insert(count);
        }
        Memory::error_devices(out);
    }
    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Ok(devices) = self.capture_devices(memory) {
            out.extend(devices);
        }
        match &self.kind {
            Kind::Move { .. } => {
                if let (Ok(target), Ok(n)) = (
                    self.block_target(memory),
                    self.read_count(memory, &mut BTreeSet::new()),
                ) && (1..=512).contains(&n)
                    && let Ok(span) = target.span(memory, n as u16, true)
                {
                    out.extend(span);
                }
            }
            Kind::Fill { count, .. } => {
                if let Ok(target) = self.block_target(memory)
                    && let Ok(span) = target.span(memory, *count, true)
                {
                    out.extend(span);
                }
            }
            Kind::Exchange { .. } => {
                if let Ok(target) = self.exchange_target(1, memory) {
                    target.devices(out);
                }
            }
        }
    }
    pub fn display_value(&self, memory: &Memory) -> String {
        let value = match &self.kind {
            Kind::Exchange { .. } => self.exchange_target(1, memory).map(|d| d.read(memory)),
            _ => self
                .block_target(memory)
                .and_then(|c| c.element(memory, 0, false).map_err(Fault::reason))
                .map(|v| v.read(memory)),
        };
        match value {
            Ok(value) => value.to_string(),
            Err(reason) => format!("İndeks: {reason}"),
        }
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let header = match self.kind {
            Kind::Move { .. } => 0x2e00,
            Kind::Fill { wide: false, .. } => 0x3000,
            Kind::Fill { wide: true, .. } => 0x3100,
            Kind::Exchange { wide: false, .. } => 0x3200,
            Kind::Exchange { wide: true, .. } => 0x3300,
        };
        let mut words = vec![header | if self.pulse_label { 0x10 } else { 0 }];
        for value in self.bindings() {
            words.extend(value.native_words()?);
        }
        match &self.kind {
            Kind::Move { count, .. } => words.extend(crate::es2_encoder::calendar_operand(
                &CalendarValue::Additional(AdditionalValue::Word(count.clone())),
            )?),
            Kind::Fill { count, wide, .. } => words.extend(crate::es2_encoder::calendar_operand(
                &frozen_value(i32::from(*count), *wide),
            )?),
            Kind::Exchange { .. } => {}
        }
        Ok(words)
    }
}
