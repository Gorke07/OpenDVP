//! SER/DSER stack operands and ALT bit modifiers. Source and result cursors
//! are deliberately distinct: DSER source C199 continues, result C199 stops.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::{CalendarDestination, CalendarValue};
use crate::data_processing::DataValue;
use crate::indexed_address::IndexedAddress;
use crate::indexed_multiply::{qualify_group, validate_model};
use crate::indexed_operand::{AddressedValue, has_modifier, has_modifiers, read_resolved_value};
use crate::plc::{Area, Device, Memory, Operand, word_range};
use crate::search_ops::search_result;
use crate::shift::bit_span;
use crate::wide::{WideOperand, double_devices};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
struct StackBinding {
    base: Device,
    address: Option<IndexedAddress>,
    digits: Option<u16>,
    wide: bool,
}
impl StackBinding {
    fn parse(raw: &str, wide: bool, write: bool) -> Result<Self, String> {
        let upper = raw.trim().to_ascii_uppercase();
        if upper.starts_with('K') {
            let v = AddressedValue::parse(&upper, wide, write)?;
            let (base, address, digits) = match v {
                AddressedValue::Direct(CalendarValue::Additional(AdditionalValue::Group {
                    base,
                    digits,
                    ..
                })) => (base, None, digits),
                AddressedValue::Modified {
                    address,
                    digits: Some(digits),
                    ..
                } => (address.base, Some(address), digits),
                _ => return Err("SER yığını sabit olamaz".into()),
            };
            return Ok(Self {
                base,
                address,
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
            return Err("SER yığını Kn veya T/C/D olmalı; doğrudan E/F desteklenmez".into());
        }
        if base.area == Area::C && base.index >= 200 {
            if !wide {
                return Err("SER word temel sayacı C0..C199 olmalı".into());
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
    fn resolve(&self, m: &Memory, write: bool) -> Result<StackCursor, String> {
        let base = match self.address {
            Some(a) => {
                let index = u16::try_from(a.base.index.wrapping_add(a.index.read(m) as u16) as i16)
                    .map_err(|_| "SER yığın indeksi negatif")?;
                Device { index, ..a.base }
            }
            None => self.base,
        };
        base.to_string().parse::<Device>()?;
        validate_model(m, &BTreeSet::from([base]))?;
        if let Some(digits) = self.digits {
            if self.address.is_some() {
                qualify_group(self.base, base, digits * 4, true)?;
            }
            validate_model(m, &bit_span(base, digits * 4, write)?.into_iter().collect())?;
        } else if self.wide && base.area == Area::C && base.index >= 200 {
            double_devices(base)?;
        } else {
            word_range(base, 1)?;
        }
        Ok(StackCursor {
            base,
            digits: self.digits,
            wide: self.wide,
            tag: self.descriptor_tag()?,
        })
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
        crate::es2_encoder::calendar_operand(&CalendarValue::Additional(value))
    }
    fn descriptor_tag(&self) -> Result<u8, String> {
        Ok((self.native_words()?[1] & 255) as u8)
    }
    fn devices(&self, out: &mut BTreeSet<Device>) {
        out.insert(self.modifier().unwrap_or(self.base));
    }
}
impl std::fmt::Display for StackBinding {
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
struct StackCursor {
    base: Device,
    digits: Option<u16>,
    wide: bool,
    tag: u8,
}
#[derive(Clone)]
enum SourceSlot {
    Scalar(CalendarValue),
    Counter199(Device),
}
impl SourceSlot {
    fn devices(&self) -> BTreeSet<Device> {
        let mut ds = BTreeSet::new();
        match self {
            Self::Scalar(v) => v.devices(&mut ds),
            Self::Counter199(d) => {
                ds.insert(*d);
            }
        }
        ds
    }
}
#[derive(Clone)]
enum ResultField {
    Scalar(CalendarDestination),
    PartialLow(Device),
}
impl ResultField {
    fn devices(&self) -> BTreeSet<Device> {
        let mut ds = BTreeSet::new();
        match self {
            Self::Scalar(v) => v.devices(&mut ds),
            Self::PartialLow(d) => {
                ds.insert(*d);
            }
        }
        ds
    }
}
struct ResultPlan {
    fields: Vec<ResultField>,
    terminated: bool,
}
impl StackCursor {
    fn group_value(&self, index: u16, write: bool, m: &Memory) -> Result<CalendarValue, String> {
        let base = Device { index, ..self.base };
        let digits = self.digits.expect("group cursor");
        let ds = bit_span(base, digits * 4, write)?;
        validate_model(m, &ds.iter().copied().collect())?;
        if self.base.area == Area::M && self.base.index < 1536 && ds.iter().any(|d| d.index >= 1536)
        {
            return Err("SER ham grup cursor M1535 fiziksel geçişi doğrulanmadı".into());
        }
        Ok(CalendarValue::Additional(AdditionalValue::Group {
            base,
            digits,
            wide: self.wide,
        }))
    }
    fn numeric_value(&self, base: Device, m: &Memory) -> Result<CalendarValue, String> {
        let ds = if self.wide {
            double_devices(base)?
        } else {
            word_range(base, 1)?
        };
        validate_model(m, &ds.into_iter().collect())?;
        Ok(CalendarValue::Additional(if self.wide {
            AdditionalValue::Double(WideOperand::Register(base))
        } else {
            AdditionalValue::Word(Operand::Register(base))
        }))
    }
    fn sources(&self, m: &Memory, n: u16) -> Result<Vec<SourceSlot>, String> {
        let mut out = Vec::new();
        let mut index = self.base.index;
        for _ in 0..n {
            let base = Device { index, ..self.base };
            let slot = if self.digits.is_some() {
                SourceSlot::Scalar(self.group_value(index, false, m)?)
            } else if self.wide && base.area == Area::C && base.index == 199 {
                validate_model(m, &BTreeSet::from([base]))?;
                SourceSlot::Counter199(base)
            } else {
                SourceSlot::Scalar(self.numeric_value(base, m)?)
            };
            out.push(slot);
            let step = self.digits.map(|n| n * 4).unwrap_or(
                if self.wide && !(base.area == Area::C && index >= 199) {
                    2
                } else {
                    1
                },
            );
            index = index.checked_add(step).ok_or("SER kaynak yığını taşıyor")?;
        }
        Ok(out)
    }
    fn results(&self, m: &Memory) -> Result<ResultPlan, String> {
        let mut fields = Vec::new();
        let mut index = self.base.index;
        for _ in 0..5 {
            let base = Device { index, ..self.base };
            if let Some(digits) = self.digits {
                fields.push(ResultField::Scalar(CalendarDestination::from_value(
                    self.group_value(index, true, m)?,
                )?));
                index = index
                    .checked_add(digits * 4)
                    .ok_or("SER sonuç yığını taşıyor")?;
            } else if self.wide && self.tag == 0x84 && base.area == Area::C && index == 199 {
                word_range(base, 1)?;
                validate_model(m, &BTreeSet::from([base]))?;
                fields.push(ResultField::PartialLow(base));
                return Ok(ResultPlan {
                    fields,
                    terminated: true,
                });
            } else {
                fields.push(ResultField::Scalar(CalendarDestination::from_value(
                    self.numeric_value(base, m)?,
                )?));
                // Native tests C199 after EVERY half-store. C198's high store
                // completes this field then terminates the entire instruction.
                if self.wide && self.tag == 0x84 && base.area == Area::C && index == 198 {
                    let terminated = fields.len() < 5;
                    return Ok(ResultPlan { fields, terminated });
                }
                let step = if self.wide && !(base.area == Area::C && index >= 200) {
                    2
                } else {
                    1
                };
                index = index.checked_add(step).ok_or("SER sonuç yığını taşıyor")?;
            }
        }
        Ok(ResultPlan {
            fields,
            terminated: false,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
struct ToggleBinding {
    base: Device,
    address: Option<IndexedAddress>,
}
impl ToggleBinding {
    fn parse(raw: &str) -> Result<Self, String> {
        let address = if has_modifier(raw) {
            Some(IndexedAddress::parse(raw)?)
        } else {
            None
        };
        let base = match address {
            Some(a) => a.base,
            None => raw.to_ascii_uppercase().parse()?,
        };
        if !matches!(base.area, Area::Y | Area::M | Area::S) {
            return Err("ALT hedefi tek Y/M/S biti olmalı".into());
        }
        if address.is_none() && !base.writable_bit() {
            return Err("ALT hedefi salt okunur".into());
        }
        Ok(Self { base, address })
    }
    fn modifier(&self) -> Option<Device> {
        self.address.map(|a| a.index.device())
    }
    fn resolve(&self, m: &Memory) -> Result<Device, String> {
        let base = match self.address {
            Some(a) => {
                let index = u16::try_from(a.base.index.wrapping_add(a.index.read(m) as u16) as i16)
                    .map_err(|_| "ALT adresi negatif")?;
                let d = Device { index, ..a.base };
                qualify_group(a.base, d, 1, true)?;
                d
            }
            None => self.base,
        };
        bit_span(base, 1, true)?;
        validate_model(m, &BTreeSet::from([base]))?;
        Ok(base)
    }
    fn native_words(&self) -> Result<Vec<u16>, String> {
        if let Some(a) = self.address {
            let mut words = a.native_words(false, Some(1))?;
            words[1] &= !15;
            return Ok(words);
        }
        let raw = match self.base.area {
            Area::S => self.base.index,
            Area::Y => 0x500 + self.base.index,
            Area::M if self.base.index >= 1536 => 0x1000 + self.base.index - 1536,
            Area::M => 0x800 + self.base.index,
            _ => unreachable!(),
        };
        Ok(vec![((raw & 255) << 8) | 0x84, (raw & 0xff00) | 0x80])
    }
}
impl std::fmt::Display for ToggleBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.address {
            Some(a) => a.fmt(f),
            None => self.base.fmt(f),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Search {
        source: StackBinding,
        needle: AddressedValue,
        target: StackBinding,
        count: DataValue,
        wide: bool,
    },
    Toggle(ToggleBinding),
}
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedSearchToggleAction {
    kind: Kind,
    pulse_label: bool,
}

fn count_value(value: &DataValue) -> CalendarValue {
    CalendarValue::Additional(match value {
        DataValue::Word(v) => AdditionalValue::Word(v.clone()),
        DataValue::Double(v) => AdditionalValue::Double(v.clone()),
    })
}
fn scalar_resolved(binding: &AddressedValue, m: &Memory) -> Result<CalendarValue, String> {
    let v = binding.resolve(m, false)?;
    if let CalendarValue::Additional(AdditionalValue::Group { base, digits, .. }) = &v {
        qualify_group(binding.base().unwrap(), *base, digits * 4, false)?;
    }
    let mut ds = BTreeSet::new();
    v.devices(&mut ds);
    validate_model(m, &ds)?;
    Ok(v)
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
    fn base(&mut self, v: &StackBinding, write: bool) -> Result<StackCursor, String> {
        self.capture(v.modifier().into_iter().collect())?;
        v.resolve(self.m, write)
    }
    fn read(&mut self, v: &CalendarValue) -> Result<i32, String> {
        let mut ds = BTreeSet::new();
        v.devices(&mut ds);
        validate_model(self.m, &ds)?;
        self.capture(ds)?;
        read_resolved_value(v, self.m, self.observed)
    }
    fn needle(&mut self, v: &AddressedValue) -> Result<i32, String> {
        self.capture(v.modifier_devices().into_iter().collect())?;
        let value = scalar_resolved(v, self.m)?;
        self.read(&value)
    }
    fn source(&mut self, v: &SourceSlot) -> Result<i32, String> {
        match v {
            SourceSlot::Scalar(v) => self.read(v),
            SourceSlot::Counter199(d) => {
                self.capture(BTreeSet::from([*d]))?;
                Ok(i32::from(self.m.word(*d) as u16))
            }
        }
    }
    fn write(&mut self, field: &ResultField, value: u8) {
        let ds = field.devices();
        match field {
            ResultField::Scalar(v) => v.write(self.m, i32::from(value)),
            ResultField::PartialLow(d) => self.m.set_word(*d, i16::from(value)),
        }
        self.written.extend(&ds);
        self.observed.extend(ds);
    }
    fn error(&mut self, offset: u16) -> Result<(), String> {
        let mut ds = BTreeSet::new();
        Memory::error_devices(&mut ds);
        self.capture(ds.clone())?;
        self.m.operation_error(0xe1a, offset);
        self.written.extend(ds);
        Ok(())
    }
    fn toggle(&mut self, v: &ToggleBinding) -> Result<(), String> {
        self.capture(v.modifier().into_iter().collect())?;
        let d = v.resolve(self.m)?;
        self.capture(BTreeSet::from([d]))?;
        if d.area == Area::M
            && (1011..=1014).contains(&d.index)
            && self.m.system_relays.clock_levels().is_none()
        {
            return Err("ALT saat biti için kurulu ilk seviye gerekli".into());
        }
        self.m.set_internal(d, !self.m.bit(d));
        self.written.insert(d);
        Ok(())
    }
}

impl IndexedSearchToggleAction {
    fn spec(name: &str) -> Option<(&'static str, bool)> {
        let upper = name.to_ascii_uppercase();
        let p = upper.ends_with('P');
        Some((
            match upper.strip_suffix('P').unwrap_or(&upper) {
                "SER" => "SER",
                "DSER" => "DSER",
                "ALT" => "ALT",
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
        let (stem, pulse_label) =
            Self::spec(name).ok_or("İndeksli arama/değiştirme komutu geçersiz")?;
        let kind = if stem == "ALT" {
            if args.len() != 1 {
                return Err("ALT bir operand gerektirir".into());
            }
            Kind::Toggle(ToggleBinding::parse(&args[0])?)
        } else {
            if args.len() != 4 {
                return Err("SER S1 S2 D n gerektirir".into());
            }
            let wide = stem == "DSER";
            if has_modifier(&args[3]) {
                return Err("SER n beyaz K/H/D rolüdür; E/F suffix kabul etmez".into());
            }
            let count = DataValue::parse(&args[3].to_ascii_uppercase(), wide)?;
            if matches!(&count,DataValue::Word(Operand::Register(d))|DataValue::Double(WideOperand::Register(d)) if d.area!=Area::D)
            {
                return Err("SER n yalnız K/H veya D olmalı".into());
            }
            Kind::Search {
                source: StackBinding::parse(&args[0], wide, false)?,
                needle: AddressedValue::parse(&args[1], wide, false)?,
                target: StackBinding::parse(&args[2], wide, true)?,
                count,
                wide,
            }
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
            Kind::Toggle(v) => c.toggle(v)?,
            Kind::Search {
                source,
                needle,
                target,
                count,
                wide,
            } => {
                let source = c.base(source, false)?;
                if *wide {
                    // A single known byte cannot turn an unknown old scratch
                    // word into a falsely known value after a failed S2 read.
                    if let Some(old) = c.m.native_numeric_scratch {
                        c.m.native_numeric_scratch =
                            Some(((u16::from(source.tag) << 8) | (old as u16 & 255)) as i16);
                    }
                }
                let needle = c.needle(needle)?;
                if !*wide {
                    c.m.native_numeric_scratch = Some(needle as i16);
                }
                let target = c.base(target, true)?;
                if *wide {
                    c.m.native_numeric_scratch =
                        Some(((u16::from(source.tag) << 8) | u16::from(target.tag)) as i16);
                }
                let n = c.read(&count_value(count))?;
                if !(1..=if *wide { 128 } else { 256 }).contains(&n) {
                    c.error(if *wide { 13 } else { 7 })?;
                    return Ok(());
                }
                let slots = source.sources(c.m, n as u16)?;
                let mut values = Vec::with_capacity(slots.len());
                for slot in &slots {
                    values.push(c.source(slot)?);
                }
                let plan = target.results(c.m)?;
                if c.native && plan.terminated {
                    return Err("DSER C199'da biten kısmi sonuç yığını ayrı diagnostic gerektirir; tam beş-field native kapsamı değil".into());
                }
                let result = search_result(&values, needle);
                for (field, value) in plan.fields.iter().zip(result) {
                    c.write(field, value);
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
    pub fn modifier_devices(&self) -> Vec<Device> {
        let ds: BTreeSet<Device> = match &self.kind {
            Kind::Toggle(v) => v.modifier().into_iter().collect(),
            Kind::Search {
                source,
                needle,
                target,
                ..
            } => source
                .modifier()
                .into_iter()
                .chain(needle.modifier_devices())
                .chain(target.modifier())
                .collect(),
        };
        ds.into_iter().collect()
    }
    pub fn control_devices(&self) -> Vec<Device> {
        vec![]
    }
    pub fn dest(&self) -> Device {
        match &self.kind {
            Kind::Toggle(v) => v.base,
            Kind::Search { target, .. } => target.base,
        }
    }
    fn static_spans(&self) -> Result<BTreeSet<Device>, String> {
        let m = Memory::default();
        let mut ds = BTreeSet::new();
        match &self.kind {
            Kind::Toggle(v) => {
                if v.address.is_none() {
                    ds.insert(v.resolve(&m)?);
                }
            }
            Kind::Search {
                source,
                target,
                count,
                wide,
                ..
            } => {
                if source.address.is_none() {
                    let cursor = source.resolve(&m, false)?;
                    if let Some(n) = count
                        .constant()
                        .filter(|n| (1..=if *wide { 128 } else { 256 }).contains(n))
                    {
                        for slot in cursor.sources(&m, n as u16)? {
                            ds.extend(slot.devices());
                        }
                    } else {
                        ds.insert(cursor.base);
                        if let Some(digits) = cursor.digits {
                            ds.extend(bit_span(cursor.base, digits * 4, false)?);
                        }
                    }
                }
                if target.address.is_none() {
                    let cursor = target.resolve(&m, true)?;
                    if count
                        .constant()
                        .is_some_and(|n| (1..=if *wide { 128 } else { 256 }).contains(&n))
                    {
                        for field in cursor.results(&m)?.fields {
                            ds.extend(field.devices());
                        }
                    } else {
                        // A dynamic/invalid count may take the PLC-error path.
                        // Only the initial special base is visited before n.
                        ds.insert(cursor.base);
                        if let Some(digits) = cursor.digits {
                            ds.extend(bit_span(cursor.base, digits * 4, true)?);
                        }
                    }
                }
            }
        }
        Ok(ds)
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        match &self.kind {
            Kind::Toggle(v) => {
                out.insert(v.modifier().unwrap_or(v.base));
            }
            Kind::Search {
                source,
                needle,
                target,
                count,
                ..
            } => {
                source.devices(out);
                target.devices(out);
                needle.devices(out);
                count.devices(out);
            }
        }
        if let Ok(ds) = self.static_spans() {
            out.extend(ds);
        }
        out.extend((1020..=1022).map(Memory::system_device));
        Memory::error_devices(out);
    }
    /// Populate suffix aliases first; these rows additionally contain the
    /// entire plain-D count (two words for DSER), never a default guessed n.
    pub fn monitor_input_devices(&self, m: &Memory) -> Result<BTreeSet<Device>, String> {
        let mut ds: BTreeSet<Device> = self.modifier_devices().into_iter().collect();
        if let Kind::Search { count, .. } = &self.kind {
            count.devices(&mut ds);
        }
        validate_model(m, &ds)?;
        Ok(ds)
    }
    /// Potential current display spans. Not an executed trace or safety API.
    pub fn capture_devices(&self, m: &Memory) -> Result<BTreeSet<Device>, String> {
        let mut ds = self.monitor_input_devices(m)?;
        match &self.kind {
            Kind::Toggle(v) => {
                ds.insert(v.resolve(m)?);
            }
            Kind::Search {
                source,
                needle,
                target,
                count,
                wide,
            } => {
                let source = source.resolve(m, false)?;
                let target = target.resolve(m, true)?;
                scalar_resolved(needle, m)?.devices(&mut ds);
                let n = read_resolved_value(&count_value(count), m, &mut ds)?;
                if (1..=if *wide { 128 } else { 256 }).contains(&n) {
                    for slot in source.sources(m, n as u16)? {
                        ds.extend(slot.devices());
                    }
                    for field in target.results(m)?.fields {
                        ds.extend(field.devices());
                    }
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
        let value = match &self.kind {
            Kind::Toggle(v) => v.resolve(m).map(|d| i32::from(m.bit(d))),
            Kind::Search { target, .. } => target
                .resolve(m, true)
                .and_then(|v| v.results(m))
                .and_then(|p| match &p.fields[0] {
                    ResultField::Scalar(v) => {
                        read_resolved_value(v.value(), m, &mut BTreeSet::new())
                    }
                    ResultField::PartialLow(d) => Ok(i32::from(m.word(*d))),
                }),
        };
        match value {
            Ok(v) => v.to_string(),
            Err(e) => format!("İndeks: {e}"),
        }
    }
    pub fn label(&self) -> String {
        let p = if self.pulse_label { "P" } else { "" };
        match &self.kind {
            Kind::Toggle(v) => format!("ALT{p} {v}"),
            Kind::Search {
                source,
                needle,
                target,
                count,
                wide,
            } => format!(
                "{}SER{p} {source} {needle} {target} {count}",
                if *wide { "D" } else { "" }
            ),
        }
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let header = match self.kind {
            Kind::Toggle(_) => 0x9400,
            Kind::Search { wide: true, .. } => 0x8b00,
            _ => 0x8a00,
        };
        let mut words = vec![header | if self.pulse_label { 0x10 } else { 0 }];
        match &self.kind {
            Kind::Toggle(v) => words.extend(v.native_words()?),
            Kind::Search {
                source,
                needle,
                target,
                count,
                ..
            } => {
                words.extend(source.native_words()?);
                words.extend(needle.native_words()?);
                words.extend(target.native_words()?);
                words.extend(crate::es2_encoder::calendar_operand(&count_value(count))?);
            }
        }
        Ok(words)
    }
}
