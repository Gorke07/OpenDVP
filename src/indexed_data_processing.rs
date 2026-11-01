//! Addressed DECO/ENCO/SUM/BON/MEAN with ordered, checked reads.
//! Native qualification hydrates a private clone in handler order. Partial
//! MEAN source ranges are a local documented policy, never native clipping.
use crate::additional_processing::AdditionalValue;
use crate::bit_index_operand::BitIndexValue;
use crate::calendar_operand::{CalendarDestination, CalendarValue};
use crate::data_processing::mean_result;
use crate::indexed_address::IndexedAddress;
use crate::indexed_multiply::{qualify_group, validate_model};
use crate::indexed_operand::{
    AddressedValue, frozen_value, has_modifier, has_modifiers, read_resolved_value,
};
use crate::plc::{Area, Device, Memory, Operand, word_range};
use crate::shift::bit_span;
use crate::wide::{WideOperand, double_devices};
use std::collections::BTreeSet;

fn group_digits(v: &AddressedValue) -> Option<u16> {
    match v {
        AddressedValue::Modified { digits, .. } => *digits,
        AddressedValue::Direct(CalendarValue::Additional(AdditionalValue::Group {
            digits,
            ..
        })) => Some(*digits),
        _ => None,
    }
}
fn numeric(raw: &str, wide: bool, write: bool, constants: bool) -> Result<AddressedValue, String> {
    let v = AddressedValue::parse(raw, wide, write)?;
    if group_digits(&v).is_some() || (!constants && v.base().is_none()) {
        return Err("Bu rol yalnız T/C/D veya doğrudan E/F kabul eder".into());
    }
    Ok(v)
}
fn literal(raw: &str, max: u16) -> Result<u16, String> {
    match Operand::parse(&raw.to_ascii_uppercase())? {
        Operand::Constant(n) if (1..=max as i16).contains(&n) => Ok(n as u16),
        _ => Err(format!("n K/H1..{max} sabiti olmalı")),
    }
}

#[derive(Debug, Clone, PartialEq)]
enum RawBinding {
    Direct(Device),
    Modified(IndexedAddress),
}
impl RawBinding {
    fn parse(raw: &str, write: bool) -> Result<Self, String> {
        let v = if has_modifier(raw) {
            Self::Modified(IndexedAddress::parse(raw)?)
        } else {
            Self::Direct(raw.to_ascii_uppercase().parse()?)
        };
        if !matches!(v.base().area, Area::X | Area::Y | Area::M | Area::S)
            || (write && v.base().area == Area::X)
        {
            return Err("Ham bit rolü X/Y/M/S, hedef Y/M/S olmalı".into());
        }
        Ok(v)
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
    fn resolve(
        &self,
        m: &Memory,
        count: u16,
        write: bool,
        special: bool,
        native: bool,
    ) -> Result<Vec<Device>, String> {
        let base = match self {
            Self::Direct(d) => *d,
            Self::Modified(a) => {
                let index = u16::try_from(a.base.index.wrapping_add(a.index.read(m) as u16) as i16)
                    .map_err(|_| "İndeksli ham bit adresi negatif")?;
                let d = Device { index, ..a.base };
                qualify_group(a.base, d, count, special)?;
                d
            }
        };
        // Direct generic packed reads also carry the existing low-M qualifier;
        // direct special84 bases do not use the indexed special remapping.
        if !special && matches!(self, Self::Direct(_)) {
            qualify_group(base, base, count, false)?;
        }
        let ds = bit_span(base, count, write)?;
        validate_model(m, &ds.iter().copied().collect())?;
        if native && base.area == Area::M && base.index < 1536 && ds.iter().any(|d| d.index >= 1536)
        {
            return Err(
                "Ham veri işleme cursor M1535 fiziksel geçişi native olarak doğrulanmadı".into(),
            );
        }
        Ok(ds)
    }
    fn native_words(&self, wide: bool) -> Result<Vec<u16>, String> {
        let mut words = match self {
            Self::Modified(a) => {
                let mut w = a.native_words(false, Some(1))?;
                w[1] &= !15;
                w
            }
            Self::Direct(d) => {
                let raw = match d.area {
                    Area::S => d.index,
                    Area::X => 0x400 + d.index,
                    Area::Y => 0x500 + d.index,
                    Area::M if d.index >= 1536 => 0x1000 + d.index - 1536,
                    Area::M => 0x800 + d.index,
                    _ => unreachable!(),
                };
                vec![((raw & 255) << 8) | 0x84, (raw & 0xff00) | 0x80]
            }
        };
        if wide {
            words.extend([0x0080, 0x0080]);
        }
        Ok(words)
    }
    fn devices(&self, out: &mut BTreeSet<Device>) {
        out.insert(self.modifier().unwrap_or_else(|| self.base()));
    }
}
impl std::fmt::Display for RawBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Direct(d) => d.fmt(f),
            Self::Modified(a) => a.fmt(f),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum DecodeBinding {
    Raw(RawBinding),
    Scalar(AddressedValue),
}
impl DecodeBinding {
    fn parse(raw: &str, write: bool, constants: bool, allow_raw: bool) -> Result<Self, String> {
        if allow_raw && let Ok(v) = RawBinding::parse(raw, write) {
            return Ok(Self::Raw(v));
        }
        numeric(raw, false, write, constants).map(Self::Scalar)
    }
    fn base(&self) -> Option<Device> {
        match self {
            Self::Raw(v) => Some(v.base()),
            Self::Scalar(v) => v.base(),
        }
    }
    fn native_words(&self) -> Result<Vec<u16>, String> {
        match self {
            Self::Raw(v) => v.native_words(false),
            Self::Scalar(v) => v.native_words(),
        }
    }
}
impl std::fmt::Display for DecodeBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Raw(v) => v.fmt(f),
            Self::Scalar(v) => v.fmt(f),
        }
    }
}

/// Wide direct F is scoped to the n role, never scalar S/D.
#[derive(Debug, Clone, PartialEq)]
enum CountBinding {
    Direct(BitIndexValue),
    Modified(AddressedValue),
}
impl CountBinding {
    fn parse(raw: &str, wide: bool, mean: bool) -> Result<Self, String> {
        let v = if has_modifier(raw) {
            Self::Modified(AddressedValue::parse(raw, wide, false)?)
        } else {
            Self::Direct(BitIndexValue::parse(raw, wide)?)
        };
        if !mean && v.is_group() {
            return Err("BON n operandı Kn grubu olamaz".into());
        }
        let (min, max) = if mean {
            (1, 64)
        } else {
            (0, if wide { 31 } else { 15 })
        };
        if v.constant().is_some_and(|n| !(min..=max).contains(&n)) {
            return Err(format!("Sabit n {min}..{max} aralığında olmalı"));
        }
        Ok(v)
    }
    fn is_group(&self) -> bool {
        match self {
            Self::Modified(v) => group_digits(v).is_some(),
            Self::Direct(v) => matches!(
                v.calendar_value(),
                Some(CalendarValue::Additional(AdditionalValue::Group { .. }))
            ),
        }
    }
    fn constant(&self) -> Option<i32> {
        match self {
            Self::Direct(v) => v.constant(),
            Self::Modified(_) => None,
        }
    }
    fn modifiers(&self) -> Vec<Device> {
        match self {
            Self::Direct(_) => vec![],
            Self::Modified(v) => v.modifier_devices(),
        }
    }
    fn devices(&self, out: &mut BTreeSet<Device>) {
        match self {
            Self::Direct(v) => v.devices(out),
            Self::Modified(v) => v.devices(out),
        }
    }
    fn scalar(&self) -> AddressedValue {
        match self {
            Self::Modified(v) => v.clone(),
            Self::Direct(v) => AddressedValue::Direct(match v.calendar_value() {
                Some(v) => v.clone(),
                None => {
                    let mut ds = BTreeSet::new();
                    v.devices(&mut ds);
                    CalendarValue::Additional(AdditionalValue::Double(WideOperand::Register(
                        *ds.first().expect("wide F pair"),
                    )))
                }
            }),
        }
    }
    fn native_words(&self) -> Result<Vec<u16>, String> {
        match self {
            Self::Modified(v) => v.native_words(),
            Self::Direct(v) => match v.native_words() {
                Some(w) => Ok(w),
                None => crate::es2_encoder::calendar_operand(
                    v.calendar_value().expect("ordinary count"),
                ),
            },
        }
    }
}
impl std::fmt::Display for CountBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Direct(v) => v.fmt(f),
            Self::Modified(v) => v.fmt(f),
        }
    }
}

/// Numeric source parsing deliberately validates only its low word. DMEAN
/// T255/Dlast can be partial locally, or rescued by a negative modifier.
#[derive(Debug, Clone, PartialEq)]
struct MeanBinding {
    base: Device,
    address: Option<IndexedAddress>,
    digits: Option<u16>,
    wide: bool,
}
impl MeanBinding {
    fn parse(raw: &str, wide: bool) -> Result<Self, String> {
        let upper = raw.trim().to_ascii_uppercase();
        if upper.starts_with('K') {
            let v = AddressedValue::parse(&upper, wide, false)?;
            let digits = group_digits(&v).ok_or("MEAN kaynağı sabit olamaz")?;
            return Ok(Self {
                base: v.base().unwrap(),
                address: match v {
                    AddressedValue::Modified { address, .. } => Some(address),
                    _ => None,
                },
                digits: Some(digits),
                wide,
            });
        }
        let address = if has_modifier(&upper) {
            Some(IndexedAddress::parse(&upper)?)
        } else {
            None
        };
        let base = match address {
            Some(a) => a.base,
            None => upper.parse()?,
        };
        if !matches!(base.area, Area::T | Area::C | Area::D) {
            return Err("MEAN kaynağı KnX/Y/M/S veya T/C/D olmalı".into());
        }
        if base.area == Area::C && base.index >= 200 {
            if !wide {
                return Err("MEAN word kaynağı C0..C199 olmalı".into());
            }
            double_devices(base)?;
        } else {
            word_range(base, 1)?;
        }
        Ok(Self {
            base,
            address,
            digits: None,
            wide,
        })
    }
    fn modifier(&self) -> Option<Device> {
        self.address.map(|a| a.index.device())
    }
    fn resolve(&self, m: &Memory) -> Result<MeanCursor, String> {
        let base = if let Some(a) = self.address {
            let index = u16::try_from(a.base.index.wrapping_add(a.index.read(m) as u16) as i16)
                .map_err(|_| "MEAN kaynak indeksi negatif")?;
            Device { index, ..a.base }
        } else {
            self.base
        };
        base.to_string().parse::<Device>()?;
        validate_model(m, &BTreeSet::from([base]))?;
        if let Some(digits) = self.digits {
            if self.address.is_some() {
                qualify_group(self.base, base, digits * 4, true)?;
            }
            let ds = bit_span(base, digits * 4, false)?;
            validate_model(m, &ds.into_iter().collect())?;
        } else if self.wide && base.area == Area::C && base.index >= 200 {
            double_devices(base)?;
        } else {
            word_range(base, 1)?;
        }
        Ok(MeanCursor {
            base,
            digits: self.digits,
            wide: self.wide,
        })
    }
    fn devices(&self, out: &mut BTreeSet<Device>) {
        if let Some(d) = self.modifier() {
            out.insert(d);
        } else {
            out.insert(self.base);
            if let Some(digits) = self.digits
                && let Ok(ds) = bit_span(self.base, digits * 4, false)
            {
                out.extend(ds);
            }
        }
    }
    fn native_words(&self) -> Result<Vec<u16>, String> {
        if let Some(a) = self.address {
            return a.native_words(self.wide, self.digits);
        }
        let value = if let Some(digits) = self.digits {
            AdditionalValue::Group {
                base: self.base,
                digits,
                wide: self.wide,
            }
        } else if self.wide {
            AdditionalValue::Double(WideOperand::Register(self.base))
        } else {
            AdditionalValue::Word(Operand::Register(self.base))
        };
        // The emitter accepts a lexical last-word source. Its payload span is
        // resolved/qualified separately; no generic full-pair parser is used.
        crate::es2_encoder::calendar_operand(&CalendarValue::Additional(value))
    }
}
impl std::fmt::Display for MeanBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(n) = self.digits {
            write!(f, "K{n}")?;
        }
        match self.address {
            Some(a) => a.fmt(f),
            None => self.base.fmt(f),
        }
    }
}
#[derive(Clone)]
struct MeanCursor {
    base: Device,
    digits: Option<u16>,
    wide: bool,
}
#[derive(Clone)]
enum MeanSlot {
    Bits(Vec<(u16, Device)>),
    Numeric {
        low: Device,
        high: Option<Device>,
        natural: bool,
    },
    Missing,
}
impl MeanCursor {
    fn slots(&self, m: &Memory, n: u16, native: bool) -> Result<Vec<MeanSlot>, String> {
        let valid = |d: Device| {
            d.to_string().parse::<Device>().is_ok()
                && validate_model(m, &BTreeSet::from([d])).is_ok()
        };
        let mut result = Vec::new();
        let mut index = self.base.index;
        for slot in 0..n {
            if let Some(digits) = self.digits {
                let mut ds = Vec::new();
                for bit in 0..digits * 4 {
                    let d = self
                        .base
                        .index
                        .checked_add(slot * digits * 4)
                        .and_then(|i| i.checked_add(bit))
                        .map(|index| Device { index, ..self.base });
                    if let Some(d) = d.filter(|d| valid(*d)) {
                        ds.push((bit, d));
                    }
                }
                if native && ds.len() != usize::from(digits * 4) {
                    return Err("MEAN native kaynak grubu tam desteklenen aralıkta olmalı; yerel kısmi adres politikası native değildir".into());
                }
                if native
                    && self.base.area == Area::M
                    && self.base.index < 1536
                    && ds.iter().any(|(_, d)| d.index >= 1536)
                {
                    return Err("MEAN native grup cursor M1535 geçişi doğrulanmadı".into());
                }
                result.push(MeanSlot::Bits(ds));
            } else {
                let low = Device { index, ..self.base };
                let low_valid = valid(low)
                    && if self.wide && low.area == Area::C && low.index >= 200 {
                        double_devices(low).is_ok()
                    } else {
                        word_range(low, 1).is_ok()
                    };
                if !low_valid {
                    if native {
                        return Err("MEAN native kaynak öğeleri alan/model sınırını aşamaz".into());
                    }
                    result.push(MeanSlot::Missing);
                } else {
                    let natural = self.wide && low.area == Area::C && low.index >= 200;
                    let high = if self.wide && !(low.area == Area::C && low.index >= 199) {
                        low.index
                            .checked_add(1)
                            .map(|index| Device { index, ..low })
                            .filter(|d| valid(*d) && word_range(*d, 1).is_ok())
                    } else {
                        None
                    };
                    if native
                        && self.wide
                        && !natural
                        && !(low.area == Area::C && low.index == 199)
                        && high.is_none()
                    {
                        return Err("DMEAN native son çiftin yüksek kelimesi eksik; yerel düşük-kelime politikası native değildir".into());
                    }
                    result.push(MeanSlot::Numeric { low, high, natural });
                }
                let step = if self.wide && !(self.base.area == Area::C && index >= 199) {
                    2
                } else {
                    1
                };
                index = index.saturating_add(step);
            }
        }
        Ok(result)
    }
}
impl MeanSlot {
    fn devices(&self) -> BTreeSet<Device> {
        match self {
            Self::Bits(ds) => ds.iter().map(|(_, d)| *d).collect(),
            Self::Numeric { low, high, .. } => std::iter::once(*low).chain(*high).collect(),
            Self::Missing => BTreeSet::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Decode {
        source: DecodeBinding,
        target: DecodeBinding,
        n: u16,
    },
    Encode {
        source: DecodeBinding,
        target: AddressedValue,
        n: u16,
    },
    Sum {
        source: AddressedValue,
        target: AddressedValue,
        wide: bool,
    },
    BitOn {
        source: AddressedValue,
        target: RawBinding,
        n: CountBinding,
        wide: bool,
    },
    Mean {
        source: MeanBinding,
        target: AddressedValue,
        n: CountBinding,
        wide: bool,
    },
}
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedDataProcessingAction {
    kind: Kind,
    pulse_label: bool,
}

struct Context<'a, F> {
    m: &'a mut Memory,
    observed: &'a mut BTreeSet<Device>,
    fetch: F,
    captured: BTreeSet<Device>,
    written: BTreeSet<Device>,
    native: bool,
}
impl<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>> Context<'_, F> {
    fn capture(&mut self, ds: BTreeSet<Device>) -> Result<(), String> {
        let missing = ds
            .difference(&self.captured)
            .copied()
            .filter(|d| !self.written.contains(d))
            .collect();
        (self.fetch)(self.m, &missing)?;
        self.captured.extend(&ds);
        self.observed.extend(ds);
        Ok(())
    }
    fn scalar(&mut self, v: &AddressedValue, write: bool) -> Result<CalendarValue, String> {
        self.capture(v.modifier_devices().into_iter().collect())?;
        let value = v.resolve(self.m, write)?;
        if let CalendarValue::Additional(AdditionalValue::Group { base, digits, .. }) = &value {
            qualify_group(v.base().unwrap(), *base, digits * 4, false)?;
        }
        let mut ds = BTreeSet::new();
        value.devices(&mut ds);
        validate_model(self.m, &ds)?;
        Ok(value)
    }
    fn read(&mut self, v: &CalendarValue) -> Result<i32, String> {
        let mut ds = BTreeSet::new();
        v.devices(&mut ds);
        self.capture(ds)?;
        read_resolved_value(v, self.m, self.observed)
    }
    fn snapshot(&mut self, v: &AddressedValue) -> Result<i32, String> {
        let v = self.scalar(v, false)?;
        self.read(&v)
    }
    fn count(&mut self, n: &CountBinding) -> Result<i32, String> {
        self.snapshot(&n.scalar())
    }
    fn raw(
        &mut self,
        v: &RawBinding,
        n: u16,
        write: bool,
        special: bool,
    ) -> Result<Vec<Device>, String> {
        self.capture(v.modifier().into_iter().collect())?;
        v.resolve(self.m, n, write, special, self.native)
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
    fn write(&mut self, v: CalendarValue, value: i32) -> Result<(), String> {
        let target = CalendarDestination::from_value(v)?;
        let mut ds = BTreeSet::new();
        target.devices(&mut ds);
        validate_model(self.m, &ds)?;
        target.write(self.m, value);
        self.written.extend(&ds);
        self.observed.extend(ds);
        Ok(())
    }
    fn put(&mut self, d: Device, on: bool) {
        self.m.set_internal(d, on);
        self.written.insert(d);
        self.observed.insert(d);
    }
    fn zero(&mut self) {
        let d = Memory::system_device(1020);
        self.m.set_system(1020, true);
        self.written.insert(d);
        self.observed.insert(d);
    }
    fn error(&mut self, offset: u16) -> Result<(), String> {
        let mut ds = BTreeSet::new();
        Memory::error_devices(&mut ds);
        self.capture(ds.clone())?;
        self.m.operation_error(0xe1a, offset);
        self.written.extend(ds);
        Ok(())
    }
    fn decode_source(&mut self, v: &DecodeBinding, n: u16, special: bool) -> Result<u32, String> {
        match v {
            DecodeBinding::Scalar(v) => Ok(self.snapshot(v)? as u16 as u32),
            DecodeBinding::Raw(v) => {
                let ds = self.raw(v, n, false, special)?;
                let mut value = 0u32;
                for (i, d) in ds.into_iter().enumerate() {
                    if self.bit(d)? {
                        value |= 1u32 << i;
                    }
                }
                Ok(value)
            }
        }
    }
    fn mean_base(&mut self, v: &MeanBinding) -> Result<MeanCursor, String> {
        self.capture(v.modifier().into_iter().collect())?;
        v.resolve(self.m)
    }
    fn mean_values(&mut self, source: &MeanCursor, n: u16) -> Result<Vec<i32>, String> {
        let slots = source.slots(self.m, n, self.native)?;
        let mut values = Vec::new();
        for slot in slots {
            let value = match slot {
                MeanSlot::Missing => 0,
                MeanSlot::Bits(ds) => {
                    let mut value = 0u32;
                    for (bit, d) in ds {
                        if self.bit(d)? {
                            value |= 1u32 << bit;
                        }
                    }
                    if source.wide {
                        value as i32
                    } else {
                        i32::from(value as i16)
                    }
                }
                MeanSlot::Numeric { low, high, natural } => {
                    self.capture(std::iter::once(low).chain(high).collect())?;
                    if natural {
                        self.m.double_word(low)
                    } else if source.wide {
                        let lo = u32::from(self.m.word(low) as u16);
                        let hi = high.map(|d| u32::from(self.m.word(d) as u16)).unwrap_or(0);
                        (lo | (hi << 16)) as i32
                    } else {
                        i32::from(self.m.word(low))
                    }
                }
            };
            values.push(value);
        }
        Ok(values)
    }
}

impl IndexedDataProcessingAction {
    fn spec(name: &str) -> Option<(&'static str, bool, bool)> {
        let upper = name.to_ascii_uppercase();
        let pulse = upper.ends_with('P');
        let stem = upper.strip_suffix('P').unwrap_or(&upper);
        let (stem, wide) = match stem {
            "DECO" => ("DECO", false),
            "ENCO" => ("ENCO", false),
            "SUM" => ("SUM", false),
            "DSUM" => ("SUM", true),
            "BON" => ("BON", false),
            "DBON" => ("BON", true),
            "MEAN" => ("MEAN", false),
            "DMEAN" => ("MEAN", true),
            _ => return None,
        };
        Some((stem, wide, pulse))
    }
    pub fn supports(name: &str) -> bool {
        Self::spec(name).is_some()
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        has_modifiers(args)
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let (stem, wide, pulse_label) =
            Self::spec(name).ok_or("İndeksli veri işleme komutu geçersiz")?;
        let expected = if stem == "SUM" { 2 } else { 3 };
        if args.len() != expected {
            return Err(format!("{name}: {expected} operand gerekli"));
        }
        let kind = match stem {
            "DECO" => {
                let target = DecodeBinding::parse(&args[1], true, false, true)?;
                let n = literal(
                    &args[2],
                    if matches!(target, DecodeBinding::Raw(_)) {
                        8
                    } else {
                        4
                    },
                )?;
                Kind::Decode {
                    source: DecodeBinding::parse(&args[0], false, true, true)?,
                    target,
                    n,
                }
            }
            "ENCO" => {
                let source = DecodeBinding::parse(&args[0], false, false, true)?;
                let n = literal(
                    &args[2],
                    if matches!(source, DecodeBinding::Raw(_)) {
                        8
                    } else {
                        4
                    },
                )?;
                Kind::Encode {
                    source,
                    target: numeric(&args[1], false, true, false)?,
                    n,
                }
            }
            "SUM" => Kind::Sum {
                source: AddressedValue::parse(&args[0], wide, false)?,
                target: numeric(&args[1], wide, true, false)?,
                wide,
            },
            "BON" => Kind::BitOn {
                source: AddressedValue::parse(&args[0], wide, false)?,
                target: RawBinding::parse(&args[1], true)?,
                n: CountBinding::parse(&args[2], wide, false)?,
                wide,
            },
            "MEAN" => Kind::Mean {
                source: MeanBinding::parse(&args[0], wide)?,
                target: AddressedValue::parse(&args[1], wide, true)?,
                n: CountBinding::parse(&args[2], wide, true)?,
                wide,
            },
            _ => unreachable!(),
        };
        let action = Self { kind, pulse_label };
        action.static_spans()?;
        Ok(action)
    }
    fn run<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>>(
        &self,
        c: &mut Context<'_, F>,
    ) -> Result<(), String> {
        match &self.kind {
            Kind::Decode { source, target, n } => {
                let value = c.decode_source(source, *n, false)? & ((1u32 << n) - 1);
                match target {
                    DecodeBinding::Raw(v) => {
                        for (i, d) in c.raw(v, 1 << n, true, true)?.into_iter().enumerate() {
                            c.put(d, i == value as usize);
                        }
                    }
                    DecodeBinding::Scalar(v) => {
                        let target = c.scalar(v, true)?;
                        c.write(target, (1u16 << value) as i16 as i32)?;
                    }
                }
            }
            Kind::Encode { source, target, n } => {
                let highest = match source {
                    DecodeBinding::Raw(v) => {
                        let ds = c.raw(v, 1 << n, false, true)?;
                        let mut highest = None;
                        for (i, d) in ds.into_iter().enumerate() {
                            if c.bit(d)? {
                                highest = Some(i as i32);
                            }
                        }
                        highest
                    }
                    DecodeBinding::Scalar(v) => {
                        let value = c.snapshot(v)? as u16;
                        (0..(1u16 << n))
                            .rev()
                            .find(|i| value & (1u16 << i) != 0)
                            .map(i32::from)
                    }
                };
                if let Some(value) = highest {
                    let target = c.scalar(target, true)?;
                    c.write(target, value)?;
                } else {
                    c.error(3)?;
                }
            }
            Kind::Sum {
                source,
                target,
                wide,
            } => {
                let value = c.snapshot(source)?;
                let count = if *wide {
                    (value as u32).count_ones()
                } else {
                    (value as u16).count_ones()
                };
                if count == 0 {
                    c.zero();
                }
                let target = c.scalar(target, true)?;
                c.write(target, count as i32)?;
            }
            Kind::BitOn {
                source,
                target,
                n,
                wide,
            } => {
                let value = c.snapshot(source)? as u32;
                let n = c.count(n)?;
                if !(0..if *wide { 32 } else { 16 }).contains(&n) {
                    c.error(if *wide { 9 } else { 5 })?;
                } else {
                    let d = c.raw(target, 1, true, false)?[0];
                    c.put(d, value & (1u32 << n) != 0);
                }
            }
            Kind::Mean {
                source,
                target,
                n,
                wide,
            } => {
                let source = c.mean_base(source)?;
                let n = c.count(n)?;
                if !(1..=64).contains(&n) {
                    c.error(if *wide { 9 } else { 5 })?;
                } else {
                    let values = c.mean_values(&source, n as u16)?;
                    let value = mean_result(&values, n, *wide);
                    let target = c.scalar(target, true)?;
                    c.write(target, value)?;
                }
            }
        }
        Ok(())
    }
    pub fn execute(&self, m: &mut Memory, power: bool, observed: &mut BTreeSet<Device>) {
        if !power {
            return;
        }
        let result = self.run(&mut Context {
            m,
            observed,
            fetch: |_: &mut Memory, _: &BTreeSet<Device>| Ok(()),
            captured: BTreeSet::new(),
            written: BTreeSet::new(),
            native: false,
        });
        if let Err(reason) = result {
            m.execution_limitations.insert(reason);
        }
    }
    pub fn qualify_native_with<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>>(
        &self,
        m: &Memory,
        fetch: F,
    ) -> Result<BTreeSet<Device>, String> {
        let mut clone = m.clone();
        let mut observed = BTreeSet::new();
        self.run(&mut Context {
            m: &mut clone,
            observed: &mut observed,
            fetch,
            captured: BTreeSet::new(),
            written: BTreeSet::new(),
            native: true,
        })?;
        Ok(observed)
    }
    pub fn qualify_native(&self, m: &Memory) -> Result<BTreeSet<Device>, String> {
        self.qualify_native_with(m, |_, _| Ok(()))
    }
    fn scalars(&self) -> Vec<&AddressedValue> {
        match &self.kind {
            Kind::Decode { source, target, .. } => [source, target]
                .into_iter()
                .filter_map(|v| match v {
                    DecodeBinding::Scalar(v) => Some(v),
                    _ => None,
                })
                .collect(),
            Kind::Encode { source, target, .. } => {
                let mut v = vec![target];
                if let DecodeBinding::Scalar(s) = source {
                    v.push(s);
                }
                v
            }
            Kind::Sum { source, target, .. } => vec![source, target],
            Kind::BitOn { source, .. } => vec![source],
            Kind::Mean { target, .. } => vec![target],
        }
    }
    fn raws(&self) -> Vec<&RawBinding> {
        match &self.kind {
            Kind::Decode { source, target, .. } => [source, target]
                .into_iter()
                .filter_map(|v| match v {
                    DecodeBinding::Raw(v) => Some(v),
                    _ => None,
                })
                .collect(),
            Kind::Encode {
                source: DecodeBinding::Raw(v),
                ..
            } => vec![v],
            Kind::BitOn { target, .. } => vec![target],
            _ => vec![],
        }
    }
    fn count_binding(&self) -> Option<&CountBinding> {
        match &self.kind {
            Kind::BitOn { n, .. } | Kind::Mean { n, .. } => Some(n),
            _ => None,
        }
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        let mut ds: BTreeSet<Device> = self
            .scalars()
            .into_iter()
            .flat_map(AddressedValue::modifier_devices)
            .chain(self.raws().into_iter().filter_map(RawBinding::modifier))
            .collect();
        if let Some(n) = self.count_binding() {
            ds.extend(n.modifiers());
        }
        if let Kind::Mean { source, .. } = &self.kind {
            ds.extend(source.modifier());
        }
        ds.into_iter().collect()
    }
    pub fn control_devices(&self) -> Vec<Device> {
        vec![]
    }
    pub fn dest(&self) -> Device {
        match &self.kind {
            Kind::Decode { target, .. } => target.base().unwrap(),
            Kind::Encode { target, .. } | Kind::Sum { target, .. } | Kind::Mean { target, .. } => {
                target.base().unwrap()
            }
            Kind::BitOn { target, .. } => target.base(),
        }
    }
    fn raw_spans(&self, m: &Memory, only_direct: bool) -> Result<BTreeSet<Device>, String> {
        let mut ds = BTreeSet::new();
        let mut add = |v: &RawBinding, n, write, special| -> Result<(), String> {
            if !only_direct || matches!(v, RawBinding::Direct(_)) {
                ds.extend(v.resolve(m, n, write, special, false)?);
            }
            Ok(())
        };
        match &self.kind {
            Kind::Decode { source, target, n } => {
                if let DecodeBinding::Raw(v) = source {
                    add(v, *n, false, false)?;
                }
                if let DecodeBinding::Raw(v) = target {
                    add(v, 1 << n, true, true)?;
                }
            }
            Kind::Encode {
                source: DecodeBinding::Raw(v),
                n,
                ..
            } => add(v, 1 << n, false, true)?,
            Kind::BitOn { target, .. } => add(target, 1, true, false)?,
            _ => {}
        }
        Ok(ds)
    }
    fn static_spans(&self) -> Result<BTreeSet<Device>, String> {
        self.raw_spans(&Memory::default(), true)
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        for v in self.scalars() {
            v.devices(out);
        }
        for v in self.raws() {
            v.devices(out);
        }
        if let Some(n) = self.count_binding() {
            n.devices(out);
        }
        if let Kind::Mean { source, .. } = &self.kind {
            source.devices(out);
        }
        if let Ok(ds) = self.static_spans() {
            out.extend(ds);
        }
        out.extend((1020..=1022).map(Memory::system_device));
        Memory::error_devices(out);
    }
    /// Required actual snapshot rows before a UI expands current candidates.
    /// Dynamic MEAN n must be present, never defaulted from absent rows. This
    /// is a monitor contract, not permission to preload native execution.
    pub fn monitor_input_devices(&self, m: &Memory) -> Result<BTreeSet<Device>, String> {
        let mut ds: BTreeSet<Device> = self.modifier_devices().into_iter().collect();
        if let Kind::Mean { n, .. } = &self.kind {
            let v = n.scalar().resolve(m, false)?;
            v.devices(&mut ds);
            validate_model(m, &ds)?;
        }
        Ok(ds)
    }
    /// Current address-only display candidates; execution observation uses run.
    /// The caller must supply monitor_input_devices from an actual snapshot.
    pub fn capture_devices(&self, m: &Memory) -> Result<BTreeSet<Device>, String> {
        let mut ds = self.raw_spans(m, false)?;
        for v in self.scalars() {
            v.resolve(m, false)?.devices(&mut ds);
        }
        if let Some(n) = self.count_binding() {
            n.scalar().resolve(m, false)?.devices(&mut ds);
        }
        if let Kind::Mean { source, n, .. } = &self.kind {
            let cursor = source.resolve(m)?;
            let value = n.scalar().resolve(m, false)?;
            let count = read_resolved_value(&value, m, &mut BTreeSet::new())?;
            if (1..=64).contains(&count) {
                for slot in cursor.slots(m, count as u16, false)? {
                    ds.extend(slot.devices());
                }
            }
        }
        validate_model(m, &ds)?;
        Ok(ds)
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Ok(ds) = self.capture_devices(m) {
            out.extend(ds);
        }
    }
    pub fn display_value(&self, m: &Memory) -> String {
        let result = match &self.kind {
            Kind::Decode {
                target: DecodeBinding::Raw(v),
                n,
                ..
            } => {
                return match v.resolve(m, 1 << n, true, true, false) {
                    Ok(ds) => format!("ON:{}", ds.iter().filter(|d| m.bit(**d)).count()),
                    Err(e) => format!("İndeks: {e}"),
                };
            }
            Kind::BitOn { target, .. } => {
                return match target.resolve(m, 1, true, false, false) {
                    Ok(ds) => u8::from(m.bit(ds[0])).to_string(),
                    Err(e) => format!("İndeks: {e}"),
                };
            }
            Kind::Decode {
                target: DecodeBinding::Scalar(v),
                ..
            }
            | Kind::Encode { target: v, .. }
            | Kind::Sum { target: v, .. }
            | Kind::Mean { target: v, .. } => v
                .resolve(m, false)
                .and_then(|v| read_resolved_value(&v, m, &mut BTreeSet::new())),
        };
        match result {
            Ok(v) => v.to_string(),
            Err(e) => format!("İndeks: {e}"),
        }
    }
    pub fn label(&self) -> String {
        let p = if self.pulse_label { "P" } else { "" };
        match &self.kind {
            Kind::Decode { source, target, n } => format!("DECO{p} {source} {target} K{n}"),
            Kind::Encode { source, target, n } => format!("ENCO{p} {source} {target} K{n}"),
            Kind::Sum {
                source,
                target,
                wide,
            } => format!("{}SUM{p} {source} {target}", if *wide { "D" } else { "" }),
            Kind::BitOn {
                source,
                target,
                n,
                wide,
            } => format!(
                "{}BON{p} {source} {target} {n}",
                if *wide { "D" } else { "" }
            ),
            Kind::Mean {
                source,
                target,
                n,
                wide,
            } => format!(
                "{}MEAN{p} {source} {target} {n}",
                if *wide { "D" } else { "" }
            ),
        }
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let header = match &self.kind {
            Kind::Decode { .. } => 0x6200,
            Kind::Encode { .. } => 0x6400,
            Kind::Sum { wide, .. } => {
                if *wide {
                    0x6700
                } else {
                    0x6600
                }
            }
            Kind::BitOn { wide, .. } => {
                if *wide {
                    0x6900
                } else {
                    0x6800
                }
            }
            Kind::Mean { wide, .. } => {
                if *wide {
                    0x6b00
                } else {
                    0x6a00
                }
            }
        };
        let mut words = vec![header | if self.pulse_label { 0x10 } else { 0 }];
        match &self.kind {
            Kind::Decode { source, target, n } => {
                words.extend(source.native_words()?);
                words.extend(target.native_words()?);
                words.extend(crate::es2_encoder::calendar_operand(&frozen_value(
                    i32::from(*n),
                    false,
                ))?);
            }
            Kind::Encode { source, target, n } => {
                words.extend(source.native_words()?);
                words.extend(target.native_words()?);
                words.extend(crate::es2_encoder::calendar_operand(&frozen_value(
                    i32::from(*n),
                    false,
                ))?);
            }
            Kind::Sum { source, target, .. } => {
                words.extend(source.native_words()?);
                words.extend(target.native_words()?);
            }
            Kind::BitOn {
                source,
                target,
                n,
                wide,
            } => {
                words.extend(source.native_words()?);
                words.extend(target.native_words(*wide)?);
                words.extend(n.native_words()?);
            }
            Kind::Mean {
                source, target, n, ..
            } => {
                words.extend(source.native_words()?);
                words.extend(target.native_words()?);
                words.extend(n.native_words()?);
            }
        }
        Ok(words)
    }
}
