//! Ordered WSUM/SCAL/SCLP/CMPT data bindings. Secondary-P state is VM-owned.
use crate::additional_processing::{
    AdditionalAction, AdditionalValue, CompareTableProfile, compare_table_matches, next_slot,
    read_slot, scale_points_result, scale_result, slot_devices, valid_slot,
};
use crate::calendar_operand::CalendarValue;
use crate::data_processing::DataValue;
use crate::indexed_address::IndexedAddress;
use crate::indexed_multiply::{qualify_group, validate_model};
use crate::indexed_operand::{AddressedValue, has_modifier, has_modifiers, read_resolved_value};
use crate::plc::{Area, Device, Memory, Operand, word_range};
use crate::wide::{WideOperand, double_devices};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdditionalOutcome {
    Nominal,
    Complete,
    PlcError { offset: u16 },
    Limitation(String),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdditionalVisit {
    pub outcome: AdditionalOutcome,
    pub observed: BTreeSet<Device>,
}

fn scalar(d: Device, wide: bool) -> CalendarValue {
    CalendarValue::Additional(if wide {
        AdditionalValue::Double(WideOperand::Register(d))
    } else {
        AdditionalValue::Word(Operand::Register(d))
    })
}
fn max_index(area: Area, m: &Memory) -> u16 {
    match area {
        Area::D => m.data_register_limit.unwrap_or(9999),
        Area::T | Area::C => 255,
        Area::X | Area::Y => 255,
        Area::M => m.relay_limit.unwrap_or(4095),
        Area::S => 1023,
    }
}

/// Numeric low-address binding. Pair/table completeness is a visited role
/// policy; WSUM's documented partial lows must not be rejected by construction.
#[derive(Debug, Clone, PartialEq)]
struct Address {
    base: Device,
    index: Option<IndexedAddress>,
}
impl Address {
    fn parse(raw: &str, d_only: bool, c_max: u16) -> Result<Self, String> {
        let upper = raw.trim().to_ascii_uppercase();
        let index = if has_modifier(&upper) {
            Some(IndexedAddress::parse(&upper)?)
        } else {
            None
        };
        let base = match index {
            Some(a) => a.base,
            None => upper.parse()?,
        };
        Self::check(base, d_only, c_max)?;
        Ok(Self { base, index })
    }
    fn direct(base: Device, d_only: bool, c_max: u16) -> Result<Self, String> {
        Self::check(base, d_only, c_max)?;
        Ok(Self { base, index: None })
    }
    fn check(base: Device, d_only: bool, c_max: u16) -> Result<(), String> {
        if (d_only && base.area != Area::D)
            || !matches!(base.area, Area::T | Area::C | Area::D)
            || (base.area == Area::C && base.index > c_max)
        {
            return Err("Bu sayısal rolün temel alanı/genişliği geçersiz".into());
        }
        base.to_string().parse::<Device>()?;
        Ok(())
    }
    fn modifier(&self) -> Option<Device> {
        self.index.map(|a| a.index.device())
    }
    fn raw(&self, m: &Memory) -> i16 {
        self.index
            .map(|a| a.base.index.wrapping_add(a.index.read(m) as u16) as i16)
            .unwrap_or(self.base.index as i16)
    }
    fn at(&self, m: &Memory) -> Option<Device> {
        let index = u16::try_from(self.raw(m)).ok()?;
        (index <= max_index(self.base.area, m)).then_some(Device { index, ..self.base })
    }
    fn devices(&self, out: &mut BTreeSet<Device>) {
        out.insert(self.modifier().unwrap_or(self.base));
    }
    fn native_words(&self, wide: bool) -> Result<Vec<u16>, String> {
        let natural = self.base.area == Area::C && self.base.index >= 200;
        let mut result = match self.index {
            Some(a) => a.native_words(wide || natural, None)?,
            None => crate::es2_encoder::calendar_operand(&scalar(self.base, wide || natural))?,
        };
        result.truncate(if wide { 4 } else { 2 });
        Ok(result)
    }
}
impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.index {
            Some(a) => a.fmt(f),
            None => self.base.fmt(f),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Constant(i32, bool),
    Data(Address, bool),
}
impl Value {
    fn parse(raw: &str, wide: bool) -> Result<Self, String> {
        let upper = raw.trim().to_ascii_uppercase();
        if upper.starts_with(['K', 'H']) {
            let value = DataValue::parse(&upper, wide)?;
            return value
                .constant()
                .map(|v| Self::Constant(v, wide))
                .ok_or("K/H sabiti gerekli".into());
        }
        Ok(Self::Data(Address::parse(&upper, true, 199)?, wide))
    }
    fn from_data(value: &DataValue) -> Result<Self, String> {
        match value {
            DataValue::Word(Operand::Constant(v)) => Ok(Self::Constant(i32::from(*v), false)),
            DataValue::Double(WideOperand::Constant(v)) => Ok(Self::Constant(*v, true)),
            DataValue::Word(Operand::Register(d)) => {
                Ok(Self::Data(Address::direct(*d, true, 199)?, false))
            }
            DataValue::Double(WideOperand::Register(d)) => {
                Ok(Self::Data(Address::direct(*d, true, 199)?, true))
            }
        }
    }
    fn word(value: &Operand) -> Result<Self, String> {
        Self::from_data(&DataValue::Word(value.clone()))
    }
    fn devices(&self, out: &mut BTreeSet<Device>) {
        if let Self::Data(a, wide) = self {
            a.devices(out);
            if a.index.is_none() && *wide {
                if let Ok(ds) = double_devices(a.base) {
                    out.extend(ds);
                }
            }
        }
    }
    fn modifier(&self) -> Option<Device> {
        match self {
            Self::Data(a, _) => a.modifier(),
            _ => None,
        }
    }
    fn native_words(&self) -> Result<Vec<u16>, String> {
        match self {
            Self::Data(a, wide) => a.native_words(*wide),
            Self::Constant(v, wide) => {
                crate::es2_encoder::calendar_operand(&CalendarValue::Additional(if *wide {
                    AdditionalValue::Double(WideOperand::Constant(*v))
                } else {
                    AdditionalValue::Word(Operand::Constant(*v as i16))
                }))
            }
        }
    }
}
impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Constant(v, _) => write!(f, "K{v}"),
            Self::Data(a, _) => a.fmt(f),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum TableTarget {
    Numeric(Address),
    Group(AddressedValue),
}
impl TableTarget {
    fn parse(raw: &str, wide: bool) -> Result<Self, String> {
        if raw.trim().to_ascii_uppercase().starts_with('K') {
            let v = AddressedValue::parse(raw, wide, true)?;
            if !matches!(
                &v,
                AddressedValue::Modified {
                    digits: Some(_),
                    ..
                } | AddressedValue::Direct(CalendarValue::Additional(
                    AdditionalValue::Group { .. }
                ))
            ) {
                return Err("CMPT Kn hedefi gerekli".into());
            }
            Ok(Self::Group(v))
        } else {
            Ok(Self::Numeric(Address::parse(
                raw,
                false,
                if wide { 255 } else { 199 },
            )?))
        }
    }
    fn from_value(value: &AdditionalValue, wide: bool) -> Result<Self, String> {
        if value.wide() != wide {
            return Err("CMPT typed hedef genişliği komutla uyuşmuyor".into());
        }
        match value {
            AdditionalValue::Word(Operand::Register(d))
            | AdditionalValue::Double(WideOperand::Register(d)) => Ok(Self::Numeric(
                Address::direct(*d, false, if wide { 255 } else { 199 })?,
            )),
            AdditionalValue::Group {
                base,
                digits,
                wide: actual,
            } if *actual == wide
                && base.writable_bit()
                && (1..=if wide { 8 } else { 4 }).contains(digits) =>
            {
                crate::shift::bit_span(*base, digits * 4, true)?;
                Ok(Self::Group(AddressedValue::Direct(
                    CalendarValue::Additional(value.clone()),
                )))
            }
            _ => Err("CMPT yazılabilir hedefi gerekli".into()),
        }
    }
    fn base(&self) -> Device {
        match self {
            Self::Numeric(a) => a.base,
            Self::Group(v) => v.base().unwrap(),
        }
    }
    fn modifier(&self) -> Option<Device> {
        match self {
            Self::Numeric(a) => a.modifier(),
            Self::Group(v) => v.modifier_devices().first().copied(),
        }
    }
    fn devices(&self, out: &mut BTreeSet<Device>) {
        match self {
            Self::Numeric(a) => a.devices(out),
            Self::Group(v) => v.devices(out),
        }
    }
    fn native_words(&self, wide: bool) -> Result<Vec<u16>, String> {
        match self {
            Self::Numeric(a) => a.native_words(wide),
            Self::Group(v) => v.native_words(),
        }
    }
}
impl std::fmt::Display for TableTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Numeric(a) => a.fmt(f),
            Self::Group(v) => v.fmt(f),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Sum {
        wide: bool,
        source: Address,
        count: Value,
        target: Address,
    },
    Scale {
        source: Value,
        slope: Value,
        offset: Value,
        target: Address,
    },
    Points {
        wide: bool,
        source: Value,
        params: Address,
        target: Address,
    },
    Table {
        wide: bool,
        left: Address,
        right: Address,
        count: Value,
        target: TableTarget,
    },
}
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedAdditionalAction {
    kind: Kind,
    pulse_label: bool,
}

struct Context<'a, F> {
    m: &'a mut Memory,
    observed: &'a mut BTreeSet<Device>,
    fetch: F,
    captured: BTreeSet<Device>,
    written: BTreeSet<Device>,
    transient: bool,
    first_error: Option<u16>,
    native: bool,
}
impl<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>> Context<'_, F> {
    fn capture(&mut self, ds: BTreeSet<Device>) -> Result<(), String> {
        let missing = ds
            .difference(&self.captured)
            .copied()
            .filter(|d| !self.written.contains(d))
            .collect::<BTreeSet<_>>();
        if !missing.is_empty() {
            (self.fetch)(self.m, &missing)?;
        }
        self.captured.extend(&ds);
        self.observed.extend(ds);
        Ok(())
    }
    fn error(&mut self, offset: u16) -> Result<(), String> {
        let mut ds = BTreeSet::new();
        Memory::error_devices(&mut ds);
        self.capture(ds.clone())?;
        self.m.operation_error(0xe1a, offset);
        self.written.extend(ds);
        self.transient = true;
        self.first_error.get_or_insert(offset);
        Ok(())
    }
    fn special(&mut self, a: &Address, offset: u16) -> Result<(Option<Device>, i16), String> {
        self.transient = false;
        self.capture(a.modifier().into_iter().collect())?;
        let raw = a.raw(self.m);
        let resolved = a.at(self.m);
        if resolved.is_none() {
            self.error(offset)?;
        }
        Ok((resolved, raw))
    }
    fn read_slot(&mut self, d: Device, wide: bool) -> Result<i32, String> {
        let ds = slot_devices(d, wide, self.m.data_register_limit.unwrap_or(9999))
            .into_iter()
            .collect();
        self.capture(ds)?;
        Ok(read_slot(self.m, d, wide))
    }
    fn read(&mut self, value: &Value, offset: u16) -> Result<i32, String> {
        // Every generic reader clears status10; sticky public errors are separate.
        self.transient = false;
        match value {
            Value::Constant(v, _) => Ok(*v),
            Value::Data(a, wide) => {
                self.capture(a.modifier().into_iter().collect())?;
                let Some(d) = a.at(self.m) else {
                    self.error(offset)?;
                    return Ok(0);
                };
                let span = if *wide {
                    double_devices(d)
                } else {
                    word_range(d, 1)
                };
                let Ok(ds) = span else {
                    self.error(offset)?;
                    return Ok(0);
                };
                if validate_model(self.m, &ds.iter().copied().collect()).is_err() {
                    self.error(offset)?;
                    return Ok(0);
                }
                self.capture(ds.into_iter().collect())?;
                read_resolved_value(&scalar(d, *wide), self.m, self.observed)
            }
        }
    }
    fn mode(&mut self) -> Result<bool, String> {
        self.capture(BTreeSet::from([Memory::system_device(1162)]))?;
        Ok(self.m.system_bit(1162))
    }
    fn read_comparison_count(&mut self, value: &Value, offset: u16) -> Result<i32, String> {
        if let Value::Data(address, true) = value {
            // The complete DCMPT count reader observes a valid low WORD
            // before detecting an unavailable high WORD. Do not fabricate
            // high storage or move the fault ahead of the actual low read.
            self.transient = false;
            self.capture(address.modifier().into_iter().collect())?;
            let Some(low) = address.at(self.m) else {
                self.error(offset)?;
                return Ok(0);
            };
            self.capture(BTreeSet::from([low]))?;
            let _ = read_resolved_value(&scalar(low, false), self.m, self.observed)?;
        }
        // Repeated modifier/low capture is deduplicated. No public write
        // intervened; this retains the generic reader's existing error rule.
        self.read(value, offset)
    }
    fn put_partial(&mut self, d: Device, value: i32) -> Result<(), String> {
        let ds = slot_devices(d, true, self.m.data_register_limit.unwrap_or(9999));
        if d.area == Area::C && d.index >= 200 {
            self.m.set_double_word(d, value);
        } else {
            self.m.set_word(d, value as i16);
            if ds.len() > 1 {
                self.m.set_word(ds[1], (value >> 16) as i16);
            }
        }
        self.written.extend(&ds);
        self.observed.extend(ds);
        Ok(())
    }
    fn put_numeric(
        &mut self,
        a: &Address,
        wide: bool,
        value: i32,
        offset: u16,
    ) -> Result<(), String> {
        if self.transient {
            return Ok(());
        } // Native writer skips before target modifier.
        self.capture(a.modifier().into_iter().collect())?;
        let Some(d) = a.at(self.m) else {
            self.error(offset)?;
            return Ok(());
        };
        let span = if wide {
            double_devices(d)
        } else {
            word_range(d, 1)
        };
        let Ok(ds) = span else {
            self.error(offset)?;
            return Ok(());
        };
        if validate_model(self.m, &ds.iter().copied().collect()).is_err() {
            self.error(offset)?;
            return Ok(());
        }
        if wide {
            self.m.set_double_word(d, value);
        } else {
            self.m.set_word(d, value as i16);
        }
        self.written.extend(&ds);
        self.observed.extend(ds);
        Ok(())
    }
    fn group_target(
        &mut self,
        binding: &AddressedValue,
        offset: u16,
    ) -> Result<Option<(Device, u16)>, String> {
        self.capture(binding.modifier_devices().into_iter().collect())?;
        let (original, base, digits) = match binding {
            AddressedValue::Direct(CalendarValue::Additional(AdditionalValue::Group {
                base,
                digits,
                ..
            })) => (*base, Some(*base), *digits),
            AddressedValue::Modified {
                address,
                digits: Some(digits),
                ..
            } => {
                let raw = address
                    .base
                    .index
                    .wrapping_add(address.index.read(self.m) as u16)
                    as i16;
                let base = u16::try_from(raw).ok().map(|index| Device {
                    index,
                    ..address.base
                });
                (address.base, base, *digits)
            }
            _ => return Err("CMPT Kn hedef bağlayıcısı geçersiz".into()),
        };
        let Some(base) = base else {
            self.error(offset)?;
            return Ok(None);
        };
        if base.index > max_index(base.area, self.m) {
            self.error(offset)?;
            return Ok(None);
        }
        if self.m.compare_table_profile != CompareTableProfile::CurrentEs2 {
            qualify_group(original, base, digits * 4, false)?;
        }
        // Current ES2's custom >raw0DFF mapping is canonical across M1535;
        // do not import generic >raw0D00 exclusions into this profile.
        Ok(Some((base, digits)))
    }
    fn put_table(
        &mut self,
        target: &TableTarget,
        wide: bool,
        value: i32,
        offset: u16,
    ) -> Result<(), String> {
        if self.transient {
            return Ok(());
        }
        if let TableTarget::Numeric(a) = target {
            return self.put_numeric(a, wide, value, offset);
        }
        let TableTarget::Group(binding) = target else {
            unreachable!()
        };
        let Some((base, digits)) = self.group_target(binding, offset)? else {
            return Ok(());
        };
        let mut ds = Vec::new();
        for i in 0..digits * 4 {
            let Some(index) = base.index.checked_add(i) else {
                self.error(offset)?;
                return Ok(());
            };
            let d = Device { index, ..base };
            if index > max_index(base.area, self.m) {
                if self.m.compare_table_profile == CompareTableProfile::CurrentEs2 {
                    self.error(offset)?;
                    return Ok(());
                }
                break;
            }
            d.to_string()
                .parse::<Device>()
                .map_err(|_| "CMPT hedefi uygulanmamış özel biti içeriyor")?;
            if !d.writable_bit() {
                return Err("CMPT hedefi salt okunur bit içeriyor".into());
            }
            if self.m.compare_table_profile != CompareTableProfile::CurrentEs2
                && base.area == Area::M
                && base.index < 1536
                && index >= 1536
            {
                return Err("Legacy CMPT generic M1535 hedef geçişi nitelendirilmedi".into());
            }
            ds.push(d);
        }
        for (i, d) in ds.iter().copied().enumerate() {
            self.m.set_internal(d, value as u32 & (1 << i) != 0);
            self.written.insert(d);
            self.observed.insert(d);
        }
        if ds.len() != usize::from(digits * 4) {
            self.error(offset)?;
        }
        Ok(())
    }
    fn finish(&self) -> AdditionalOutcome {
        self.first_error
            .map(|offset| AdditionalOutcome::PlcError { offset })
            .unwrap_or(AdditionalOutcome::Complete)
    }

    /// CMPT reads/advances the left cursor before validating the right one.
    /// A failed double high preserves the already advanced left low cursor.
    fn comparison_source_supported(
        &mut self,
        address: &Address,
        wide: bool,
    ) -> Result<bool, String> {
        if self.m.compare_table_profile != CompareTableProfile::CurrentEs2 {
            // Both old WPL images and current SS2 dispatch the ORIGINAL
            // source category, not just the resolved T/C/D storage area.
            // Wire BYTE3 is the low byte of the second encoded WORD.
            // Higher D modifier banks encode 87..89 and take a silent
            // transient-status10 branch before reading this source payload.
            let category = address.native_words(wide)?[1] as u8;
            if !matches!(category, 0x82 | 0x84 | 0x86) {
                self.transient = true;
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn comparison_value(
        &mut self,
        cursor: &mut Device,
        wide: bool,
        left: bool,
        offset: u16,
    ) -> Result<Option<i32>, String> {
        let limit = self.m.data_register_limit.unwrap_or(9999);
        let modern = self.m.compare_table_profile == CompareTableProfile::CurrentEs2;
        if !valid_slot(*cursor, wide, limit) {
            if modern {
                self.error(offset)?;
            }
            return Ok(None);
        }
        if !wide {
            let value = self.read_slot(*cursor, false)?;
            *cursor = next_slot(*cursor, false);
            if left {
                self.m.native_numeric_scratch = Some(cursor.index as i16);
            }
            return Ok(Some(value));
        }
        if cursor.area == Area::C && cursor.index >= 199 {
            let value = self.read_slot(*cursor, true)?;
            *cursor = next_slot(*cursor, true);
            if left {
                self.m.native_numeric_scratch = Some(cursor.index as i16);
            }
            return Ok(Some(value));
        }
        let low = self.read_slot(*cursor, false)? as u16;
        cursor.index += 1;
        if left {
            self.m.native_numeric_scratch = Some(cursor.index as i16);
        }
        let high = if valid_slot(*cursor, false, limit) {
            self.read_slot(*cursor, false)? as i16
        } else {
            if modern {
                self.error(offset)?;
                return Ok(None);
            }
            0
        };
        cursor.index += 1;
        if left {
            self.m.native_numeric_scratch = Some(cursor.index as i16);
        }
        Ok(Some(i32::from(low) | (i32::from(high) << 16)))
    }
}

impl IndexedAdditionalAction {
    fn spec(name: &str) -> Option<(&'static str, bool, bool)> {
        Some(match name.to_ascii_uppercase().as_str() {
            "WSUM" => ("WSUM", false, false),
            "WSUMP" => ("WSUM", false, true),
            "DWSUM" => ("WSUM", true, false),
            "DWSUMP" => ("WSUM", true, true),
            "SCAL" => ("SCAL", false, false),
            "SCALP" => ("SCAL", false, true),
            "SCLP" => ("SCLP", false, false),
            "SCLPP" => ("SCLP", false, true),
            "DSCLP" => ("SCLP", true, false),
            "DSCLPP" => ("SCLP", true, true),
            "CMPT" => ("CMPT", false, false),
            "CMPTP" => ("CMPT", false, true),
            "DCMPT" => ("CMPT", true, false),
            "DCMPTP" => ("CMPT", true, true),
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
        let (stem, wide, pulse_label) =
            Self::spec(name).ok_or("WSUM/SCAL/SCLP/CMPT ailesi gerekli")?;
        if args.len()
            != if matches!(stem, "SCAL" | "CMPT") {
                4
            } else {
                3
            }
        {
            return Err(format!("{name}: operand sayısı yanlış"));
        }
        let kind = match stem {
            "WSUM" => Kind::Sum {
                wide,
                source: Address::parse(&args[0], false, if wide { 255 } else { 199 })?,
                count: Value::parse(&args[1], wide)?,
                target: Address::parse(&args[2], false, 255)?,
            },
            "SCAL" => Kind::Scale {
                source: Value::parse(&args[0], false)?,
                slope: Value::parse(&args[1], false)?,
                offset: Value::parse(&args[2], false)?,
                target: Address::parse(&args[3], true, 199)?,
            },
            "SCLP" => Kind::Points {
                wide,
                source: Value::parse(&args[0], wide)?,
                params: Address::parse(&args[1], true, 199)?,
                target: Address::parse(&args[2], true, 199)?,
            },
            "CMPT" => Kind::Table {
                wide,
                left: Address::parse(&args[0], false, 199)?,
                right: Address::parse(&args[1], false, 199)?,
                count: Value::parse(&args[2], wide)?,
                target: TableTarget::parse(&args[3], wide)?,
            },
            _ => unreachable!(),
        };
        Ok(Self { kind, pulse_label })
    }
    pub fn from_legacy(action: &AdditionalAction) -> Result<Self, String> {
        match action {
            AdditionalAction::WordSum { wide, count, .. }
            | AdditionalAction::CompareTable { wide, count, .. }
                if *wide != count.wide() =>
            {
                return Err("Typed sayaç genişliği komutla uyuşmuyor".into());
            }
            AdditionalAction::ScalePoints { wide, source, .. } if *wide != source.wide() => {
                return Err("Typed SCLP kaynak genişliği komutla uyuşmuyor".into());
            }
            _ => {}
        }
        let kind = match action {
            AdditionalAction::WordSum {
                wide,
                source,
                count,
                dest,
            } => Kind::Sum {
                wide: *wide,
                source: Address::direct(*source, false, if *wide { 255 } else { 199 })?,
                count: Value::from_data(count)?,
                target: Address::direct(*dest, false, 255)?,
            },
            AdditionalAction::Scale {
                source,
                slope,
                offset,
                dest,
            } => Kind::Scale {
                source: Value::word(source)?,
                slope: Value::word(slope)?,
                offset: Value::word(offset)?,
                target: Address::direct(*dest, true, 199)?,
            },
            AdditionalAction::ScalePoints {
                wide,
                source,
                params,
                dest,
            } => Kind::Points {
                wide: *wide,
                source: Value::from_data(source)?,
                params: Address::direct(*params, true, 199)?,
                target: Address::direct(*dest, true, 199)?,
            },
            AdditionalAction::CompareTable {
                wide,
                left,
                right,
                count,
                dest,
            } => Kind::Table {
                wide: *wide,
                left: Address::direct(*left, false, if *wide { 255 } else { 199 })?,
                right: Address::direct(*right, false, if *wide { 255 } else { 199 })?,
                count: Value::from_data(count)?,
                target: TableTarget::from_value(dest, *wide)?,
            },
            _ => return Err("Bu legacy eylem WSUM/SCAL/SCLP/CMPT değil".into()),
        };
        Ok(Self {
            kind,
            pulse_label: false,
        })
    }
    fn run<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>>(
        &self,
        c: &mut Context<'_, F>,
    ) -> Result<(), String> {
        match &self.kind {
            Kind::Sum {
                wide,
                source,
                count,
                target,
            } => {
                let (source, raw) = c.special(source, 1)?;
                c.m.native_numeric_scratch = Some(raw);
                let Some(mut next) = source else {
                    return Ok(());
                };
                if !wide && next.area == Area::C && next.index >= 200 {
                    c.error(1)?;
                    return Ok(());
                }
                let n = c.read(count, if *wide { 5 } else { 3 })?.clamp(1, 64);
                let (target, _) = c.special(target, if *wide { 9 } else { 5 })?;
                let Some(target) = target else {
                    return Ok(());
                };
                let mut result = 0i32;
                for _ in 0..n {
                    if !valid_slot(next, *wide, c.m.data_register_limit.unwrap_or(9999)) {
                        break;
                    }
                    result = result.wrapping_add(c.read_slot(next, *wide)?);
                    next = next_slot(next, *wide);
                    c.m.native_numeric_scratch = Some(next.index as i16);
                }
                c.put_partial(target, result)?;
            }
            Kind::Scale {
                source,
                slope,
                offset,
                target,
            } => {
                let a = c.read(source, 1)? as i16;
                c.m.native_numeric_scratch = Some(a);
                let b = c.read(slope, 3)? as i16;
                let d = c.read(offset, 5)? as i16;
                c.put_numeric(target, false, i32::from(scale_result(a, b, d)), 7)?;
            }
            Kind::Points {
                wide,
                source,
                params,
                target,
            } => {
                let source = c.read(source, 1)?;
                if *wide && c.transient {
                    return Ok(());
                }
                let (base, _) = c.special(params, if *wide { 5 } else { 3 })?;
                let Some(base) = base else {
                    return Ok(());
                };
                let count = if *wide { 8 } else { 4 };
                if u32::from(base.index) + count
                    > u32::from(c.m.data_register_limit.unwrap_or(9999)) + 1
                {
                    c.error(if *wide { 5 } else { 3 })?;
                    return Ok(());
                }
                let values = if *wide {
                    let mut values = [0; 4];
                    for (i, v) in values.iter_mut().enumerate() {
                        *v = c.read_slot(
                            Device {
                                index: base.index + i as u16 * 2,
                                ..base
                            },
                            true,
                        )?;
                    }
                    values
                } else {
                    // The word body validates the span before reading result limits.
                    let min = c.read_slot(
                        Device {
                            index: base.index + 1,
                            ..base
                        },
                        false,
                    )?;
                    let max = c.read_slot(base, false)?;
                    if max == min {
                        c.error(3)?;
                        return Ok(());
                    }
                    let high = c.read_slot(
                        Device {
                            index: base.index + 2,
                            ..base
                        },
                        false,
                    )?;
                    let low = c.read_slot(
                        Device {
                            index: base.index + 3,
                            ..base
                        },
                        false,
                    )?;
                    [max, min, high, low]
                };
                let floating = *wide && c.mode()?;
                let Some(result) = scale_points_result(source, values, *wide, floating) else {
                    c.error(if *wide { 5 } else { 3 })?;
                    return Ok(());
                };
                c.put_numeric(target, *wide, result, if *wide { 9 } else { 5 })?;
            }
            Kind::Table {
                wide,
                left,
                right,
                count,
                target,
            } => {
                let modern = c.m.compare_table_profile == CompareTableProfile::CurrentEs2;
                let (left_base, left_raw) = c.special(left, 1)?;
                c.m.native_numeric_scratch = Some(left_raw);
                if modern && left_base.is_none() {
                    return Ok(());
                }
                if !modern && left_base.is_none() && (c.native || left_raw < 0) {
                    return Err(
                        "Legacy CMPT geçersiz ilk kaynak cursor'u native olarak güvenli değil"
                            .into(),
                    );
                }
                let mut left_cursor = left_base.unwrap_or(Device {
                    index: left_raw as u16,
                    ..left.base
                });
                let (right_base, right_raw) = c.special(right, if *wide { 5 } else { 3 })?;
                if modern && right_base.is_none() {
                    return Ok(());
                }
                if !modern && right_base.is_none() && (c.native || right_raw < 0) {
                    return Err(
                        "Legacy CMPT geçersiz ikinci kaynak cursor'u native olarak güvenli değil"
                            .into(),
                    );
                }
                let mut right_cursor = right_base.unwrap_or(Device {
                    index: right_raw as u16,
                    ..right.base
                });
                let count_pc = if *wide { 9 } else { 5 };
                let raw = c.read_comparison_count(count, count_pc)? as u16;
                if modern && c.transient {
                    return Ok(());
                }
                // Legacy inline count dispatch clears10 before comparisons,
                // permitting a result after the count reader's sticky error.
                c.transient = false;
                let n = (raw as u8).clamp(1, if *wide { 32 } else { 16 });
                let relation = raw >> 8;
                let mut result = 0u32;
                for bit in 0..n {
                    if !c.comparison_source_supported(left, *wide)? {
                        return Ok(());
                    }
                    let Some(a) = c.comparison_value(&mut left_cursor, *wide, true, count_pc)?
                    else {
                        return Ok(());
                    };
                    if !c.comparison_source_supported(right, *wide)? {
                        return Ok(());
                    }
                    let Some(b) = c.comparison_value(&mut right_cursor, *wide, false, count_pc)?
                    else {
                        return Ok(());
                    };
                    let floating = *wide && modern && c.mode()?;
                    if compare_table_matches(a, b, relation, floating) {
                        result |= 1 << bit;
                    }
                }
                c.put_table(target, *wide, result as i32, if *wide { 13 } else { 7 })?;
            }
        }
        Ok(())
    }
    pub fn execute(
        &self,
        m: &mut Memory,
        power: bool,
        observed: &mut BTreeSet<Device>,
    ) -> AdditionalOutcome {
        if !power {
            return AdditionalOutcome::Nominal;
        }
        let mut c = Context {
            m,
            observed,
            fetch: |_: &mut Memory, _: &BTreeSet<Device>| Ok(()),
            captured: BTreeSet::new(),
            written: BTreeSet::new(),
            transient: false,
            first_error: None,
            native: false,
        };
        let outcome = match self.run(&mut c) {
            Ok(()) => c.finish(),
            Err(reason) => AdditionalOutcome::Limitation(reason),
        };
        if let AdditionalOutcome::Limitation(reason) = &outcome {
            c.m.execution_limitations.insert(reason.clone());
        }
        outcome
    }
    pub fn qualify_native_with<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>>(
        &self,
        m: &Memory,
        fetch: F,
    ) -> AdditionalVisit {
        let mut clone = m.clone();
        let mut observed = BTreeSet::new();
        let mut c = Context {
            m: &mut clone,
            observed: &mut observed,
            fetch,
            captured: BTreeSet::new(),
            written: BTreeSet::new(),
            transient: false,
            first_error: None,
            native: true,
        };
        let outcome = match self.run(&mut c) {
            Ok(()) => c.finish(),
            Err(reason) => AdditionalOutcome::Limitation(reason),
        };
        AdditionalVisit { outcome, observed }
    }
    pub fn qualify_native(&self, m: &Memory) -> AdditionalVisit {
        self.qualify_native_with(m, |_, _| Ok(()))
    }
    pub fn wide(&self) -> bool {
        match self.kind {
            Kind::Sum { wide, .. } | Kind::Points { wide, .. } | Kind::Table { wide, .. } => wide,
            Kind::Scale { .. } => false,
        }
    }
    pub fn header(&self) -> u16 {
        (match self.kind {
            Kind::Sum { .. } => 0x8401,
            Kind::Scale { .. } => 0xb201,
            Kind::Points { .. } => 0xb401,
            Kind::Table { .. } => 0xb801,
        }) + if self.wide() { 0x100 } else { 0 }
            + if self.pulse_label { 0x10 } else { 0 }
    }
    pub fn nominal_words(&self) -> u16 {
        match self.kind {
            Kind::Sum { wide, .. } | Kind::Points { wide, .. } => {
                if wide {
                    13
                } else {
                    7
                }
            }
            Kind::Scale { .. } => 9,
            Kind::Table { wide, .. } => {
                if wide {
                    17
                } else {
                    9
                }
            }
        }
    }
    pub fn dest(&self) -> Device {
        match &self.kind {
            Kind::Sum { target, .. } | Kind::Scale { target, .. } | Kind::Points { target, .. } => {
                target.base
            }
            Kind::Table { target, .. } => target.base(),
        }
    }
    pub fn label(&self) -> String {
        let p = if self.pulse_label { "P" } else { "" };
        let d = if self.wide() { "D" } else { "" };
        match &self.kind {
            Kind::Sum {
                source,
                count,
                target,
                ..
            } => format!("{d}WSUM{p} {source} {count} {target}"),
            Kind::Scale {
                source,
                slope,
                offset,
                target,
            } => format!("SCAL{p} {source} {slope} {offset} {target}"),
            Kind::Points {
                source,
                params,
                target,
                ..
            } => format!("{d}SCLP{p} {source} {params} {target}"),
            Kind::Table {
                left,
                right,
                count,
                target,
                ..
            } => format!("{d}CMPT{p} {left} {right} {count} {target}"),
        }
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let mut words = vec![self.header()];
        match &self.kind {
            Kind::Sum {
                wide,
                source,
                count,
                target,
            } => {
                words.extend(source.native_words(*wide)?);
                words.extend(count.native_words()?);
                words.extend(target.native_words(*wide)?);
            }
            Kind::Scale {
                source,
                slope,
                offset,
                target,
            } => {
                for v in [source, slope, offset] {
                    words.extend(v.native_words()?);
                }
                words.extend(target.native_words(false)?);
            }
            Kind::Points {
                wide,
                source,
                params,
                target,
            } => {
                words.extend(source.native_words()?);
                words.extend(params.native_words(*wide)?);
                words.extend(target.native_words(*wide)?);
            }
            Kind::Table {
                wide,
                left,
                right,
                count,
                target,
            } => {
                words.extend(left.native_words(*wide)?);
                words.extend(right.native_words(*wide)?);
                words.extend(count.native_words()?);
                words.extend(target.native_words(*wide)?);
            }
        }
        Ok(words)
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        let values = match &self.kind {
            Kind::Sum {
                source,
                count,
                target,
                ..
            } => vec![source.modifier(), count.modifier(), target.modifier()],
            Kind::Scale {
                source,
                slope,
                offset,
                target,
            } => vec![
                source.modifier(),
                slope.modifier(),
                offset.modifier(),
                target.modifier(),
            ],
            Kind::Points {
                source,
                params,
                target,
                ..
            } => vec![source.modifier(), params.modifier(), target.modifier()],
            Kind::Table {
                left,
                right,
                count,
                target,
                ..
            } => vec![
                left.modifier(),
                right.modifier(),
                count.modifier(),
                target.modifier(),
            ],
        };
        values
            .into_iter()
            .flatten()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    pub fn control_devices(&self) -> Vec<Device> {
        if matches!(
            self.kind,
            Kind::Points { wide: true, .. } | Kind::Table { wide: true, .. }
        ) {
            vec![Memory::system_device(1162)]
        } else {
            vec![]
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        match &self.kind {
            Kind::Sum {
                source,
                count,
                target,
                ..
            } => {
                source.devices(out);
                count.devices(out);
                target.devices(out);
            }
            Kind::Scale {
                source,
                slope,
                offset,
                target,
            } => {
                for v in [source, slope, offset] {
                    v.devices(out);
                }
                target.devices(out);
            }
            Kind::Points {
                wide,
                source,
                params,
                target,
            } => {
                source.devices(out);
                params.devices(out);
                target.devices(out);
                if params.index.is_none() {
                    if let Ok(ds) = word_range(params.base, if *wide { 8 } else { 4 }) {
                        out.extend(ds);
                    }
                }
                if target.index.is_none() {
                    out.extend(slot_devices(target.base, *wide, 9999));
                }
            }
            Kind::Table {
                wide,
                left,
                right,
                count,
                target,
            } => {
                left.devices(out);
                right.devices(out);
                count.devices(out);
                target.devices(out);
                if let TableTarget::Numeric(a) = target {
                    if a.index.is_none() {
                        out.extend(slot_devices(a.base, *wide, 9999));
                    }
                }
            }
        }
        out.extend(self.control_devices());
        Memory::error_devices(out);
        // Sum source/result and table source inventory deliberately retain only
        // their initial low words: later partial accesses are model-dependent.
    }
    fn count_input(value: &Value, m: &Memory, out: &mut BTreeSet<Device>) -> Result<(), String> {
        if let Value::Data(a, wide) = value {
            let d = a.at(m).ok_or("İzleme sayacı etkin adresi geçersiz")?;
            let ds = if *wide {
                double_devices(d)?
            } else {
                word_range(d, 1)?
            };
            validate_model(m, &ds.iter().copied().collect())?;
            out.extend(ds);
        }
        Ok(())
    }
    fn monitor_value(value: &Value, m: &Memory) -> Result<i32, String> {
        match value {
            Value::Constant(v, _) => Ok(*v),
            Value::Data(a, wide) => {
                let mut ds = BTreeSet::new();
                Self::count_input(value, m, &mut ds)?;
                let d = a.at(m).ok_or("İzleme adresi geçersiz")?;
                Ok(if *wide {
                    m.double_word(d)
                } else {
                    i32::from(m.word(d))
                })
            }
        }
    }
    /// Require real modifier/count/mode rows before expanding display spans.
    /// No source values, scratch, status10 or stores are synthesized for UI.
    pub fn monitor_input_devices(&self, m: &Memory) -> Result<BTreeSet<Device>, String> {
        let mut out = self.modifier_devices().into_iter().collect::<BTreeSet<_>>();
        match &self.kind {
            Kind::Sum { count, .. } | Kind::Table { count, .. } => {
                Self::count_input(count, m, &mut out)?
            }
            _ => {}
        }
        if matches!(self.kind, Kind::Points { wide: true, .. })
            || matches!(self.kind, Kind::Table { wide: true, .. })
                && m.compare_table_profile == CompareTableProfile::CurrentEs2
        {
            out.insert(Memory::system_device(1162));
        }
        validate_model(m, &out)?;
        Ok(out)
    }
    pub fn capture_devices(&self, m: &Memory) -> Result<BTreeSet<Device>, String> {
        let mut out = self.monitor_input_devices(m)?;
        let add_address =
            |a: &Address, wide: bool, n: u16, out: &mut BTreeSet<Device>| -> Result<(), String> {
                let mut next = a.at(m).ok_or("İzleme sayısal adresi geçersiz")?;
                for _ in 0..n {
                    if !valid_slot(next, wide, m.data_register_limit.unwrap_or(9999)) {
                        break;
                    }
                    out.extend(slot_devices(
                        next,
                        wide,
                        m.data_register_limit.unwrap_or(9999),
                    ));
                    next = next_slot(next, wide);
                }
                Ok(())
            };
        let add_value = |v: &Value, out: &mut BTreeSet<Device>| -> Result<(), String> {
            if let Value::Data(a, wide) = v {
                add_address(a, *wide, 1, out)?;
            }
            Ok(())
        };
        match &self.kind {
            Kind::Sum {
                wide,
                source,
                count,
                target,
            } => {
                let n = Self::monitor_value(count, m)?.clamp(1, 64) as u16;
                add_address(source, *wide, n, &mut out)?;
                add_address(target, true, 1, &mut out)?;
            }
            Kind::Scale {
                source,
                slope,
                offset,
                target,
            } => {
                for v in [source, slope, offset] {
                    add_value(v, &mut out)?;
                }
                add_address(target, false, 1, &mut out)?;
            }
            Kind::Points {
                wide,
                source,
                params,
                target,
            } => {
                add_value(source, &mut out)?;
                add_address(params, false, if *wide { 8 } else { 4 }, &mut out)?;
                add_address(target, *wide, 1, &mut out)?;
            }
            Kind::Table {
                wide,
                left,
                right,
                count,
                target,
            } => {
                let n =
                    (Self::monitor_value(count, m)? as u8).clamp(1, if *wide { 32 } else { 16 });
                add_address(left, *wide, u16::from(n), &mut out)?;
                add_address(right, *wide, u16::from(n), &mut out)?;
                match target {
                    TableTarget::Numeric(a) => add_address(a, *wide, 1, &mut out)?,
                    TableTarget::Group(v) => {
                        let (base, digits) = match v {
                            AddressedValue::Direct(CalendarValue::Additional(
                                AdditionalValue::Group { base, digits, .. },
                            )) => (*base, *digits),
                            AddressedValue::Modified {
                                address,
                                digits: Some(digits),
                                ..
                            } => {
                                let index = u16::try_from(
                                    address
                                        .base
                                        .index
                                        .wrapping_add(address.index.read(m) as u16)
                                        as i16,
                                )
                                .map_err(|_| "İzleme Kn adresi negatif")?;
                                (
                                    Device {
                                        index,
                                        ..address.base
                                    },
                                    *digits,
                                )
                            }
                            _ => return Err("İzleme Kn binding geçersiz".into()),
                        };
                        for i in 0..digits * 4 {
                            let Some(index) = base.index.checked_add(i) else {
                                break;
                            };
                            if index > max_index(base.area, m) {
                                break;
                            }
                            let d = Device { index, ..base };
                            if d.to_string().parse::<Device>().is_ok() {
                                out.insert(d);
                            }
                        }
                    }
                }
            }
        }
        Ok(out)
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Ok(ds) = self.capture_devices(m) {
            out.extend(ds);
        }
    }
    pub fn display_value(&self, m: &Memory) -> String {
        match &self.kind {
            Kind::Sum { target, .. } => target
                .at(m)
                .map(|d| read_slot(m, d, true).to_string())
                .unwrap_or_else(|| "Geçersiz hedef".into()),
            Kind::Scale { target, .. } => target
                .at(m)
                .map(|d| m.word(d).to_string())
                .unwrap_or_else(|| "Geçersiz hedef".into()),
            Kind::Points { wide, target, .. } => target
                .at(m)
                .map(|d| read_slot(m, d, *wide).to_string())
                .unwrap_or_else(|| "Geçersiz hedef".into()),
            Kind::Table {
                wide,
                target: TableTarget::Numeric(a),
                ..
            } => a
                .at(m)
                .map(|d| read_slot(m, d, *wide).to_string())
                .unwrap_or_else(|| "Geçersiz hedef".into()),
            Kind::Table {
                target: TableTarget::Group(_),
                ..
            } => "Kn karşılaştırma sonucu".into(),
        }
    }
}

/// Both parsed and manually constructed legacy programs need the ordered VM.
pub(crate) fn is_instruction(action: &crate::plc::Action) -> bool {
    match action {
        crate::plc::Action::Pulse(inner) => is_instruction(inner),
        crate::plc::Action::IndexedAdditional(_) => true,
        crate::plc::Action::Additional(
            AdditionalAction::WordSum { .. }
            | AdditionalAction::Scale { .. }
            | AdditionalAction::ScalePoints { .. }
            | AdditionalAction::CompareTable { .. },
        ) => true,
        _ => false,
    }
}
