//! Scoped addressed shifts/FIFO/digits/reset with per-family operand roles.
//! Native qualification replays captured inputs on a clone; it is not a
//! numerical oracle and never commits native data into the local simulator.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::{CalendarDestination, CalendarValue};
use crate::indexed_address::IndexedAddress;
use crate::indexed_multiply::{qualify_group, validate_model};
use crate::indexed_operand::{
    AddressedValue, frozen_value, has_modifier, has_modifiers, read_resolved_value,
};
use crate::plc::{Area, Device, Memory, Operand};
use crate::shift::{bit_span, smov_result, word_shift_scratch};
use std::collections::BTreeSet;

fn count(raw: &str, min: u16, max: u16) -> Result<u16, String> {
    match Operand::parse(&raw.to_ascii_uppercase())? {
        Operand::Constant(n)
            if i32::from(n) >= i32::from(min) && i32::from(n) <= i32::from(max) =>
        {
            Ok(n as u16)
        }
        _ => Err(format!("Sayı K/H{min}..{max} sabiti olmalı")),
    }
}
#[derive(Debug, Clone, PartialEq)]
enum RawAddress {
    Direct(Device),
    Modified(IndexedAddress),
}
impl RawAddress {
    fn parse(raw: &str, reset: bool, write: bool) -> Result<Self, String> {
        let binding = if has_modifier(raw) {
            Self::Modified(IndexedAddress::parse(raw)?)
        } else {
            Self::Direct(raw.to_ascii_uppercase().parse()?)
        };
        let base = binding.base();
        if if reset {
            !matches!(
                base.area,
                Area::Y | Area::M | Area::S | Area::T | Area::C | Area::D
            )
        } else {
            !matches!(base.area, Area::X | Area::Y | Area::M | Area::S)
                || (write && base.area == Area::X)
        } {
            return Err("Ham adresin alanı bu kaydırma/sıfırlama rolünde kullanılamaz".into());
        }
        Ok(binding)
    }
    fn base(&self) -> Device {
        match self {
            Self::Direct(d) => *d,
            Self::Modified(a) => a.base,
        }
    }
    fn modifier(&self) -> Option<Device> {
        match self {
            Self::Direct(_) => None,
            Self::Modified(a) => Some(a.index.device()),
        }
    }
    fn resolve(&self, m: &Memory, write: bool, reset: bool) -> Result<Device, String> {
        let d = match self {
            Self::Direct(d) => *d,
            Self::Modified(a) => {
                let index = u16::try_from(a.base.index.wrapping_add(a.index.read(m) as u16) as i16)
                    .map_err(|_| "İndeksli ham adres negatif")?;
                let d = Device { index, ..a.base };
                if d.area == Area::M {
                    qualify_group(a.base, d, 1, true)?;
                }
                d
            }
        };
        d.to_string().parse::<Device>()?;
        validate_model(m, &BTreeSet::from([d]))?;
        if !reset {
            bit_span(d, 1, write)?;
        } else if d.bit() && !matches!(d.area, Area::T | Area::C) && write && !d.writable_bit() {
            return Err("ZRST başlangıcı salt okunur".into());
        }
        Ok(d)
    }
    fn native_words(&self, reset: bool) -> Result<Vec<u16>, String> {
        match self {
            Self::Modified(a) => {
                let numeric = matches!(a.base.area, Area::T | Area::C | Area::D);
                let mut words = a.native_words(
                    numeric && a.base.area == Area::C && a.base.index >= 200,
                    if numeric { None } else { Some(1) },
                )?;
                if !numeric {
                    words[1] &= !0x000f;
                }
                words.truncate(2);
                Ok(words)
            }
            Self::Direct(d) => {
                if reset && matches!(d.area, Area::T | Area::C) {
                    return Ok(vec![
                        (d.index << 8) | 0x80,
                        if d.area == Area::T { 0x82 } else { 0x84 },
                    ]);
                }
                if d.area == Area::D {
                    return crate::es2_encoder::calendar_operand(&CalendarValue::Additional(
                        AdditionalValue::Word(Operand::Register(*d)),
                    ));
                }
                let raw = match d.area {
                    Area::S => d.index,
                    Area::X => 0x400 + d.index,
                    Area::Y => 0x500 + d.index,
                    Area::M if d.index >= 1536 => 0x1000 + d.index - 1536,
                    Area::M => 0x800 + d.index,
                    _ => return Err("Ham bit descriptor alanı geçersiz".into()),
                };
                Ok(vec![((raw & 255) << 8) | 0x84, (raw & 0xff00) | 0x84])
            }
        }
    }
}
impl std::fmt::Display for RawAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Direct(d) => d.fmt(f),
            Self::Modified(a) => a.fmt(f),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Bits {
        left: bool,
        source: RawAddress,
        target: RawAddress,
        total: u16,
        n: u16,
    },
    Words {
        left: bool,
        source: AddressedValue,
        target: AddressedValue,
        total: u16,
        n: u16,
    },
    Push {
        source: AddressedValue,
        head: AddressedValue,
        total: u16,
    },
    Pop {
        head: AddressedValue,
        target: AddressedValue,
        total: u16,
    },
    Digits {
        source: AddressedValue,
        target: AddressedValue,
        m1: u16,
        m2: u16,
        n: u16,
    },
    Reset {
        start: RawAddress,
        end: RawAddress,
    },
}
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedShiftFifoAction {
    kind: Kind,
    pulse_label: bool,
}

fn block(raw: &str, write: bool) -> Result<AddressedValue, String> {
    let value = AddressedValue::parse(raw, false, write)?;
    if value.base().is_none()
        || matches!(value, AddressedValue::Direct(CalendarValue::Index { .. }))
    {
        return Err("Blok adresi Kn veya T/C/D olmalı".into());
    }
    Ok(value)
}
fn group_digits(value: &AddressedValue) -> Option<u16> {
    match value {
        AddressedValue::Modified { digits, .. } => *digits,
        AddressedValue::Direct(CalendarValue::Additional(AdditionalValue::Group {
            digits,
            ..
        })) => Some(*digits),
        _ => None,
    }
}

/// A frozen word/group cursor. It contains no modifier to reread on advance.
#[derive(Debug, Clone)]
struct Span {
    base: Device,
    digits: Option<u16>,
}
impl Span {
    fn from(value: CalendarValue) -> Result<Self, String> {
        match value {
            CalendarValue::Additional(AdditionalValue::Word(Operand::Register(base))) => {
                Ok(Self { base, digits: None })
            }
            CalendarValue::Additional(AdditionalValue::Group { base, digits, .. }) => Ok(Self {
                base,
                digits: Some(digits),
            }),
            _ => Err("FIFO/kaydırma cursor Kn veya T/C/D olmalı".into()),
        }
    }
    fn at(&self, n: u16) -> Result<CalendarValue, String> {
        let step = self.digits.map(|d| d * 4).unwrap_or(1);
        let index = self
            .base
            .index
            .checked_add(n.checked_mul(step).ok_or("Kaydırma aralığı taşıyor")?)
            .ok_or("Kaydırma aralığı taşıyor")?;
        let base = Device { index, ..self.base };
        Ok(CalendarValue::Additional(
            if let Some(digits) = self.digits {
                AdditionalValue::Group {
                    base,
                    digits,
                    wide: false,
                }
            } else {
                AdditionalValue::Word(Operand::Register(base))
            },
        ))
    }
    fn devices(
        &self,
        m: &Memory,
        count: u16,
        write: bool,
        native: bool,
    ) -> Result<BTreeSet<Device>, String> {
        let mut result = BTreeSet::new();
        for i in 0..count {
            let value = self.at(i)?;
            match &value {
                CalendarValue::Additional(AdditionalValue::Group { base, digits, .. }) => {
                    result.extend(bit_span(*base, digits * 4, write)?);
                }
                CalendarValue::Additional(AdditionalValue::Word(Operand::Register(base))) => {
                    result.extend(crate::plc::word_range(*base, 1)?);
                }
                _ => unreachable!(),
            }
        }
        validate_model(m, &result)?;
        if native
            && self.base.area == Area::M
            && self.base.index < 1536
            && result.iter().any(|d| d.index >= 1536)
        {
            return Err(
                "Kaydırma/FIFO cursor M1535 geçişinin native raw pointer döngüsü kanonik değil"
                    .into(),
            );
        }
        Ok(result)
    }
}

/// Fetch is called only for initial storage which has not already been
/// captured or overwritten by this instruction's clone replay. Even equal
/// stores and explicit flag writes mark their storage as written.
struct Context<'a, F> {
    m: &'a mut Memory,
    observed: &'a mut BTreeSet<Device>,
    fetch: F,
    captured: BTreeSet<Device>,
    written: BTreeSet<Device>,
    native: bool,
}
impl<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>> Context<'_, F> {
    fn capture(&mut self, devices: BTreeSet<Device>) -> Result<(), String> {
        let missing = devices
            .difference(&self.captured)
            .copied()
            .filter(|d| !self.written.contains(d))
            .collect();
        (self.fetch)(self.m, &missing)?;
        self.captured.extend(&devices);
        self.observed.extend(devices);
        Ok(())
    }
    fn flag(&mut self, index: u16) -> Result<bool, String> {
        self.capture(BTreeSet::from([Memory::system_device(index)]))?;
        Ok(self.m.system_bit(index))
    }
    fn set_flag(&mut self, index: u16, value: bool) {
        let d = Memory::system_device(index);
        self.m.set_system(index, value);
        self.written.insert(d);
        self.observed.insert(d);
    }
    fn error(&mut self, code: i16, offset: u16) -> Result<(), String> {
        let mut devices = BTreeSet::new();
        Memory::error_devices(&mut devices);
        self.capture(devices.clone())?;
        self.m.operation_error(code, offset);
        self.written.extend(devices);
        Ok(())
    }
    fn raw(&mut self, binding: &RawAddress, write: bool, reset: bool) -> Result<Device, String> {
        if let Some(d) = binding.modifier() {
            self.capture(BTreeSet::from([d]))?;
        }
        binding.resolve(self.m, write, reset)
    }
    fn resolve(
        &mut self,
        binding: &AddressedValue,
        write: bool,
        special: bool,
    ) -> Result<CalendarValue, String> {
        self.capture(binding.modifier_devices().into_iter().collect())?;
        let value = binding.resolve(self.m, write)?;
        if (!special || matches!(binding, AddressedValue::Modified { .. }))
            && let CalendarValue::Additional(AdditionalValue::Group { base, digits, .. }) = &value
        {
            qualify_group(binding.base().unwrap(), *base, digits * 4, special)?;
        }
        let mut ds = BTreeSet::new();
        value.devices(&mut ds);
        validate_model(self.m, &ds)?;
        Ok(value)
    }
    fn read(&mut self, value: &CalendarValue) -> Result<i32, String> {
        let mut ds = BTreeSet::new();
        value.devices(&mut ds);
        self.capture(ds)?;
        read_resolved_value(value, self.m, self.observed)
    }
    fn bit(&mut self, d: Device) -> Result<bool, String> {
        self.capture(BTreeSet::from([d]))?;
        if d.area == Area::M
            && (1011..=1014).contains(&d.index)
            && self.m.system_relays.clock_levels().is_none()
        {
            return Err("İndeksli saat biti için kurulu ilk seviye gerekli".into());
        }
        Ok(self.m.bit(d))
    }
    fn put_bit(&mut self, d: Device, value: bool) {
        self.m.set_internal(d, value);
        self.written.insert(d);
        self.observed.insert(d);
    }
    fn write(&mut self, value: CalendarValue, v: i32) -> Result<(), String> {
        let dest = CalendarDestination::from_value(value)?;
        let mut ds = BTreeSet::new();
        dest.devices(&mut ds);
        validate_model(self.m, &ds)?;
        dest.write(self.m, v);
        self.written.extend(&ds);
        self.observed.extend(ds);
        Ok(())
    }
    fn raw_span(&self, base: Device, n: u16, write: bool) -> Result<Vec<Device>, String> {
        let ds = bit_span(base, n, write)?;
        validate_model(self.m, &ds.iter().copied().collect())?;
        if self.native
            && base.area == Area::M
            && base.index < 1536
            && ds.iter().any(|d| d.index >= 1536)
        {
            return Err("Ham kaydırma M1535 cursor geçişi native olarak güvenli değil".into());
        }
        Ok(ds)
    }
}

impl IndexedShiftFifoAction {
    fn spec(name: &str) -> Option<(&'static str, bool)> {
        let upper = name.to_ascii_uppercase();
        let p = upper.ends_with('P');
        let stem = upper.strip_suffix('P').unwrap_or(&upper);
        Some((
            match stem {
                "SMOV" => "SMOV",
                "SFTR" => "SFTR",
                "SFTL" => "SFTL",
                "WSFR" => "WSFR",
                "WSFL" => "WSFL",
                "SFWR" => "SFWR",
                "SFRD" => "SFRD",
                "ZRST" => "ZRST",
                _ => return None,
            },
            p,
        ))
    }
    pub fn supports(name: &str) -> bool {
        Self::spec(name).is_some()
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        has_modifiers(args)
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let (stem, pulse_label) = Self::spec(name).ok_or("İndeksli kaydırma komutu geçersiz")?;
        let expected = match stem {
            "SMOV" => 5,
            "SFTR" | "SFTL" | "WSFR" | "WSFL" => 4,
            "ZRST" => 2,
            _ => 3,
        };
        if args.len() != expected {
            return Err(format!("{name}: {expected} operand gerekli"));
        }
        let kind = match stem {
            "SFTR" | "SFTL" => {
                let total = count(&args[2], 1, 1024)?;
                Kind::Bits {
                    left: stem == "SFTL",
                    source: RawAddress::parse(&args[0], false, false)?,
                    target: RawAddress::parse(&args[1], false, true)?,
                    total,
                    n: count(&args[3], 1, total)?,
                }
            }
            "WSFR" | "WSFL" => {
                let source = block(&args[0], false)?;
                let target = block(&args[1], true)?;
                if group_digits(&source) != group_digits(&target) {
                    return Err(
                        "WSFR/WSFL aynı sayısal/grup kategorisi ve Kn genişliği gerektirir".into(),
                    );
                }
                let total = count(&args[2], 1, 512)?;
                Kind::Words {
                    left: stem == "WSFL",
                    source,
                    target,
                    total,
                    n: count(&args[3], 1, total)?,
                }
            }
            "SFWR" => Kind::Push {
                source: AddressedValue::parse(&args[0], false, false)?,
                head: block(&args[1], true)?,
                total: count(&args[2], 2, 512)?,
            },
            "SFRD" => Kind::Pop {
                head: block(&args[0], true)?,
                target: AddressedValue::parse(&args[1], false, true)?,
                total: count(&args[2], 2, 512)?,
            },
            "SMOV" => {
                let source = AddressedValue::parse(&args[0], false, false)?;
                if source.base().is_none() {
                    return Err("SMOV kaynağı sabit olamaz".into());
                }
                let m1 = count(&args[1], 1, 4)?;
                let m2 = count(&args[2], 1, m1)?;
                Kind::Digits {
                    source,
                    target: AddressedValue::parse(&args[3], false, true)?,
                    m1,
                    m2,
                    n: count(&args[4], m2, 4)?,
                }
            }
            "ZRST" => {
                let start = RawAddress::parse(&args[0], true, true)?;
                let end = RawAddress::parse(&args[1], true, false)?;
                if start.base().area != end.base().area {
                    return Err("ZRST iki uç aynı alan olmalı".into());
                }
                Kind::Reset { start, end }
            }
            _ => unreachable!(),
        };
        let action = Self { kind, pulse_label };
        action.static_spans(&Memory::default())?;
        Ok(action)
    }
    fn run<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>>(
        &self,
        c: &mut Context<'_, F>,
    ) -> Result<(), String> {
        match &self.kind {
            Kind::Bits {
                left,
                source,
                target,
                total,
                n,
            } => {
                let source = c.raw(source, false, false)?;
                let target = c.raw(target, true, false)?;
                let mut src = c.raw_span(source, *n, false)?;
                let mut dst = c.raw_span(target, *total, true)?;
                if *left {
                    src.reverse();
                    dst.reverse();
                }
                for i in 0..usize::from(total - n) {
                    let bit = c.bit(dst[i + usize::from(*n)])?;
                    c.put_bit(dst[i], bit);
                }
                for (i, d) in src.into_iter().enumerate() {
                    let bit = c.bit(d)?;
                    c.put_bit(dst[usize::from(total - n) + i], bit);
                }
            }
            Kind::Words {
                left,
                source,
                target,
                total,
                n,
            } => {
                let source = Span::from(c.resolve(source, false, true)?)?;
                c.m.native_numeric_scratch = Some(word_shift_scratch(source.base));
                let target = Span::from(c.resolve(target, true, true)?)?;
                source.devices(c.m, *n, false, c.native)?;
                target.devices(c.m, *total, true, c.native)?;
                if let Some(digits) = source.digits {
                    let src = c.raw_span(source.base, n * digits * 4, false)?;
                    let dst = c.raw_span(target.base, total * digits * 4, true)?;
                    if *left {
                        for i in (src.len()..dst.len()).rev() {
                            let b = c.bit(dst[i - src.len()])?;
                            c.put_bit(dst[i], b);
                        }
                    } else {
                        for i in 0..dst.len() - src.len() {
                            let b = c.bit(dst[i + src.len()])?;
                            c.put_bit(dst[i], b);
                        }
                    }
                    let start = if *left { 0 } else { dst.len() - src.len() };
                    for (i, d) in src.into_iter().enumerate() {
                        let b = c.bit(d)?;
                        c.put_bit(dst[start + i], b);
                    }
                } else {
                    let mut dst = (0..*total)
                        .map(|i| target.at(i))
                        .collect::<Result<Vec<_>, _>>()?;
                    let mut src = (0..*n)
                        .map(|i| source.at(i))
                        .collect::<Result<Vec<_>, _>>()?;
                    if *left {
                        dst.reverse();
                        src.reverse();
                    }
                    let rem = usize::from(total % n);
                    let stride = usize::from(*n);
                    for i in 0..rem {
                        let value = c.read(&dst[i + stride])?;
                        c.write(dst[i].clone(), value)?;
                    }
                    for (col, source) in src.iter().enumerate() {
                        let mut i = rem + col;
                        while i + stride < dst.len() {
                            let value = c.read(&dst[i + stride])?;
                            c.write(dst[i].clone(), value)?;
                            i += stride;
                        }
                        let value = c.read(source)?;
                        c.write(dst[i].clone(), value)?;
                    }
                }
            }
            Kind::Push {
                source,
                head,
                total,
            } => {
                let source = c.resolve(source, false, false)?;
                let value = c.read(&source)? as i16;
                c.m.native_numeric_scratch = Some(value);
                let header = c.resolve(head, false, false)?;
                let pointer = c.read(&header)? as i16;
                if pointer < 0 {
                    c.error(0xe1a, 3)?;
                    return Ok(());
                }
                let next = pointer.wrapping_add(1);
                c.set_flag(1022, false);
                if i32::from(next) >= i32::from(*total) {
                    c.set_flag(1022, true);
                    return Ok(());
                }
                if c.native && next < 0 {
                    return Err(
                        "SFWR taşmış header ayrı malformed native qualification gerektirir".into(),
                    );
                }
                let header = c.resolve(head, true, false)?;
                c.write(header, i32::from(next))?;
                let queue = Span::from(c.resolve(head, true, true)?)?;
                queue.devices(c.m, *total, true, c.native)?;
                c.write(
                    queue.at(if next < 0 { 0 } else { next as u16 })?,
                    i32::from(value),
                )?;
            }
            Kind::Pop {
                head,
                target,
                total,
            } => {
                let header = c.resolve(head, false, false)?;
                let pointer = c.read(&header)? as i16;
                c.set_flag(1020, false);
                if pointer < 0 {
                    c.error(0xe1a, 1)?;
                    return Ok(());
                }
                if pointer == 0 {
                    c.set_flag(1020, true);
                    return Ok(());
                }
                let next = pointer - 1;
                if next == 0 {
                    c.set_flag(1020, true);
                }
                let header = c.resolve(head, true, false)?;
                c.write(header, i32::from(next))?;
                let queue = Span::from(c.resolve(head, true, true)?)?;
                queue.devices(c.m, *total, true, c.native)?;
                let value = c.read(&queue.at(1)?)?;
                let output = c.resolve(target, true, false)?;
                c.write(output, value)?;
                if let Some(digits) = queue.digits {
                    let bits = c.raw_span(queue.base, total * digits * 4, true)?;
                    let width = usize::from(digits * 4);
                    for i in width..bits.len() - width {
                        let b = c.bit(bits[i + width])?;
                        c.put_bit(bits[i], b);
                    }
                } else {
                    for i in 1..total - 1 {
                        let value = c.read(&queue.at(i + 1)?)?;
                        c.write(queue.at(i)?, value)?;
                    }
                }
            }
            Kind::Digits {
                source,
                target,
                m1,
                m2,
                n,
            } => {
                let source = c.resolve(source, false, false)?;
                let source = c.read(&source)? as i16;
                c.m.native_numeric_scratch = Some(source);
                let old = c.resolve(target, false, false)?;
                let old = c.read(&old)? as i16;
                let hex = c.flag(1168)?;
                if !hex && (0..=9999).contains(&source) && (0..=9999).contains(&old) {
                    c.m.native_numeric_scratch = Some(source / 1000);
                }
                if let Some(value) = smov_result(source, old, hex, *m1, *m2, *n) {
                    let target = c.resolve(target, true, false)?;
                    c.write(target, i32::from(value))?;
                } else {
                    c.error(0xe18, 11)?;
                }
            }
            Kind::Reset { start, end } => {
                let start = c.raw(start, true, true)?;
                let end = c.raw(end, false, true)?;
                let devices = reset_span(c.m, start, end)?;
                for d in devices.into_iter().rev() {
                    c.m.reset_value(d);
                    c.written.insert(d);
                    c.observed.insert(d);
                }
            }
        }
        Ok(())
    }
    pub fn execute(&self, m: &mut Memory, power: bool, observed: &mut BTreeSet<Device>) {
        if !power {
            return;
        }
        let mut c = Context {
            m,
            observed,
            fetch: |_: &mut Memory, _: &BTreeSet<Device>| Ok(()),
            captured: BTreeSet::new(),
            written: BTreeSet::new(),
            native: false,
        };
        if let Err(reason) = self.run(&mut c) {
            c.m.execution_limitations.insert(reason);
        }
    }
    /// Only an internal clone is modified. Caller fetches paused native initial
    /// storage into that clone; the independent local simulator is untouched.
    pub fn qualify_native_with<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>>(
        &self,
        m: &Memory,
        fetch: F,
    ) -> Result<(), String> {
        let mut clone = m.clone();
        let mut observed = BTreeSet::new();
        let mut c = Context {
            m: &mut clone,
            observed: &mut observed,
            fetch,
            captured: BTreeSet::new(),
            written: BTreeSet::new(),
            native: true,
        };
        self.run(&mut c)
    }
    pub fn qualify_native(&self, m: &Memory) -> Result<(), String> {
        self.qualify_native_with(m, |_, _| Ok(()))
    }
    fn values(&self) -> Vec<&AddressedValue> {
        match &self.kind {
            Kind::Words { source, target, .. } | Kind::Digits { source, target, .. } => {
                vec![source, target]
            }
            Kind::Push { source, head, .. } => vec![source, head],
            Kind::Pop { head, target, .. } => vec![head, target],
            _ => vec![],
        }
    }
    fn raws(&self) -> Vec<&RawAddress> {
        match &self.kind {
            Kind::Bits { source, target, .. } => vec![source, target],
            Kind::Reset { start, end } => vec![start, end],
            _ => vec![],
        }
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        self.values()
            .into_iter()
            .flat_map(AddressedValue::modifier_devices)
            .chain(self.raws().into_iter().filter_map(RawAddress::modifier))
            .collect()
    }
    pub fn control_devices(&self) -> Vec<Device> {
        if matches!(self.kind, Kind::Digits { .. }) {
            vec![Memory::system_device(1168)]
        } else {
            Vec::new()
        }
    }
    pub fn dest(&self) -> Device {
        match &self.kind {
            Kind::Bits { target, .. } => target.base(),
            Kind::Reset { end, .. } => end.base(),
            Kind::Words { target, .. } | Kind::Digits { target, .. } | Kind::Pop { target, .. } => {
                target.base().unwrap()
            }
            Kind::Push { head, .. } => head.base().unwrap(),
        }
    }
    fn spans(&self, m: &Memory, only_direct: bool) -> Result<BTreeSet<Device>, String> {
        let mut devices = BTreeSet::new();
        let value_span =
            |binding: &AddressedValue, n: u16, write: bool| -> Result<BTreeSet<Device>, String> {
                if only_direct && !matches!(binding, AddressedValue::Direct(_)) {
                    return Ok(BTreeSet::new());
                }
                let value = binding.resolve(m, write)?;
                if n == 0 {
                    let mut ds = BTreeSet::new();
                    value.devices(&mut ds);
                    validate_model(m, &ds)?;
                    Ok(ds)
                } else {
                    Span::from(value)?.devices(m, n, write, false)
                }
            };
        let raw_span =
            |binding: &RawAddress, n: u16, write: bool| -> Result<BTreeSet<Device>, String> {
                if only_direct && !matches!(binding, RawAddress::Direct(_)) {
                    return Ok(BTreeSet::new());
                }
                let base = binding.resolve(m, write, false)?;
                let ds = bit_span(base, n, write)?.into_iter().collect();
                validate_model(m, &ds)?;
                Ok(ds)
            };
        match &self.kind {
            Kind::Bits {
                source,
                target,
                total,
                n,
                ..
            } => {
                devices.extend(raw_span(source, *n, false)?);
                devices.extend(raw_span(target, *total, true)?);
            }
            Kind::Words {
                source,
                target,
                total,
                n,
                ..
            } => {
                devices.extend(value_span(source, *n, false)?);
                devices.extend(value_span(target, *total, true)?);
            }
            Kind::Push {
                source,
                head,
                total,
            } => {
                devices.extend(value_span(source, 0, false)?);
                devices.extend(value_span(head, *total, true)?);
            }
            Kind::Pop {
                head,
                target,
                total,
            } => {
                devices.extend(value_span(head, *total, true)?);
                devices.extend(value_span(target, 0, true)?);
            }
            Kind::Digits { source, target, .. } => {
                devices.extend(value_span(source, 0, false)?);
                devices.extend(value_span(target, 0, true)?);
            }
            Kind::Reset { start, end } => {
                if !only_direct
                    || (matches!(start, RawAddress::Direct(_))
                        && matches!(end, RawAddress::Direct(_)))
                {
                    devices.extend(reset_span(
                        m,
                        start.resolve(m, true, true)?,
                        end.resolve(m, false, true)?,
                    )?);
                } else {
                    for binding in [start, end] {
                        if let RawAddress::Direct(d) = binding {
                            devices.insert(*d);
                        }
                    }
                }
            }
        }
        Ok(devices)
    }
    fn static_spans(&self, m: &Memory) -> Result<BTreeSet<Device>, String> {
        self.spans(m, true)
    }
    /// Address-only monitor candidates at the captured current indices. This
    /// does not simulate a FIFO header store and MUST NOT be used as the native
    /// safety API; qualify_native_with is the staged actual-read contract.
    pub fn capture_devices(&self, m: &Memory) -> Result<BTreeSet<Device>, String> {
        self.spans(m, false)
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        for value in self.values() {
            value.devices(out);
        }
        for raw in self.raws() {
            if let Some(d) = raw.modifier() {
                out.insert(d);
            } else {
                out.insert(raw.base());
            }
        }
        if let Ok(ds) = self.static_spans(&Memory::default()) {
            out.extend(ds);
        }
        out.extend(self.control_devices());
        match self.kind {
            Kind::Push { .. } => {
                out.insert(Memory::system_device(1022));
            }
            Kind::Pop { .. } => {
                out.insert(Memory::system_device(1020));
            }
            _ => {}
        }
        Memory::error_devices(out);
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Ok(ds) = self.capture_devices(m) {
            out.extend(ds);
        }
    }
    pub fn display_value(&self, m: &Memory) -> String {
        let value = match &self.kind {
            Kind::Digits { target, .. } | Kind::Pop { target, .. } | Kind::Words { target, .. } => {
                target
                    .resolve(m, false)
                    .and_then(|v| read_resolved_value(&v, m, &mut BTreeSet::new()))
            }
            Kind::Push { head, .. } => head
                .resolve(m, false)
                .and_then(|v| read_resolved_value(&v, m, &mut BTreeSet::new())),
            Kind::Reset { start, end } => {
                return match (start.resolve(m, true, true), end.resolve(m, false, true)) {
                    (Ok(a), Ok(b)) => format!("{a}..{b}"),
                    (Err(e), _) | (_, Err(e)) => format!("İndeks: {e}"),
                };
            }
            Kind::Bits { target, total, .. } => {
                return match target
                    .resolve(m, true, false)
                    .and_then(|d| bit_span(d, *total, true))
                {
                    Ok(ds) => format!("ON {}/{}", ds.iter().filter(|d| m.bit(**d)).count(), total),
                    Err(e) => format!("İndeks: {e}"),
                };
            }
        };
        match value {
            Ok(v) => v.to_string(),
            Err(e) => format!("İndeks: {e}"),
        }
    }
    pub fn label(&self) -> String {
        let p = if self.pulse_label { "P" } else { "" };
        match &self.kind {
            Kind::Bits {
                left,
                source,
                target,
                total,
                n,
            } => format!(
                "SFT{}{p} {source} {target} K{total} K{n}",
                if *left { "L" } else { "R" }
            ),
            Kind::Words {
                left,
                source,
                target,
                total,
                n,
            } => format!(
                "WSF{}{p} {source} {target} K{total} K{n}",
                if *left { "L" } else { "R" }
            ),
            Kind::Push {
                source,
                head,
                total,
            } => format!("SFWR{p} {source} {head} K{total}"),
            Kind::Pop {
                head,
                target,
                total,
            } => format!("SFRD{p} {head} {target} K{total}"),
            Kind::Digits {
                source,
                target,
                m1,
                m2,
                n,
            } => format!("SMOV{p} {source} K{m1} K{m2} {target} K{n}"),
            Kind::Reset { start, end } => format!("ZRST{p} {start} {end}"),
        }
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let header = match self.kind {
            Kind::Bits { left: false, .. } => 0x5400,
            Kind::Bits { left: true, .. } => 0x5600,
            Kind::Words { left: false, .. } => 0x5800,
            Kind::Words { left: true, .. } => 0x5a00,
            Kind::Push { .. } => 0x5c00,
            Kind::Pop { .. } => 0x5e00,
            Kind::Digits { .. } => 0x2a00,
            Kind::Reset { .. } => 0x6000,
        };
        let mut words = vec![header | if self.pulse_label { 0x10 } else { 0 }];
        let constant =
            |n: u16| crate::es2_encoder::calendar_operand(&frozen_value(i32::from(n), false));
        match &self.kind {
            Kind::Bits {
                source,
                target,
                total,
                n,
                ..
            } => {
                words.extend(source.native_words(false)?);
                words.extend(target.native_words(false)?);
                words.extend(constant(*total)?);
                words.extend(constant(*n)?);
            }
            Kind::Words {
                source,
                target,
                total,
                n,
                ..
            } => {
                words.extend(source.native_words()?);
                words.extend(target.native_words()?);
                words.extend(constant(*total)?);
                words.extend(constant(*n)?);
            }
            Kind::Push {
                source,
                head,
                total,
            } => {
                words.extend(source.native_words()?);
                words.extend(head.native_words()?);
                words.extend(constant(*total)?);
            }
            Kind::Pop {
                head,
                target,
                total,
            } => {
                words.extend(head.native_words()?);
                words.extend(target.native_words()?);
                words.extend(constant(*total)?);
            }
            Kind::Digits {
                source,
                target,
                m1,
                m2,
                n,
            } => {
                words.extend(source.native_words()?);
                words.extend(constant(*m1)?);
                words.extend(constant(*m2)?);
                words.extend(target.native_words()?);
                words.extend(constant(*n)?);
            }
            Kind::Reset { start, end } => {
                words.extend(start.native_words(true)?);
                words.extend(end.native_words(true)?);
            }
        }
        Ok(words)
    }
}

fn reset_span(m: &Memory, start: Device, end: Device) -> Result<Vec<Device>, String> {
    if start.area != end.area {
        return Err("ZRST uçları aynı alan olmalı".into());
    }
    let devices = (start.index.min(end.index)..=end.index)
        .map(|index| {
            let d = Device { index, ..end };
            d.to_string().parse::<Device>()?;
            if matches!(d.area, Area::Y | Area::M | Area::S) && !d.writable_bit() {
                return Err("ZRST aralığında salt okunur bit var".into());
            }
            Ok(d)
        })
        .collect::<Result<Vec<Device>, String>>()?;
    validate_model(m, &devices.iter().copied().collect())?;
    Ok(devices)
}
