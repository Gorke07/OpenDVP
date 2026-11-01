//! Decode/encode, population count, bit test, mean and integer square root.
use crate::additional_processing::AdditionalValue;
use crate::bit_index_operand::BitIndexValue;
use crate::calendar_operand::{CalendarDestination, CalendarValue};
use crate::plc::{Area, Device, Memory, Operand, word_range};
use crate::shift::bit_span;
use crate::wide::{WideOperand, double_devices};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub enum DataValue {
    Word(Operand),
    Double(WideOperand),
}
impl DataValue {
    pub fn parse(raw: &str, wide: bool) -> Result<Self, String> {
        if wide {
            Ok(Self::Double(WideOperand::parse(raw)?))
        } else {
            Ok(Self::Word(Operand::parse(raw)?))
        }
    }
    pub fn wide(&self) -> bool {
        matches!(self, Self::Double(_))
    }
    pub fn read(&self, m: &Memory) -> i32 {
        match self {
            Self::Word(v) => i32::from(v.read(m)),
            Self::Double(v) => v.read(m),
        }
    }
    pub fn constant(&self) -> Option<i32> {
        match self {
            Self::Word(Operand::Constant(v)) => Some(i32::from(*v)),
            Self::Double(WideOperand::Constant(v)) => Some(*v),
            _ => None,
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        match self {
            Self::Word(Operand::Register(d)) => {
                out.insert(*d);
            }
            Self::Double(v) => v.devices(out),
            _ => {}
        }
    }
}
impl std::fmt::Display for DataValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Word(v) => write!(f, "{v}"),
            Self::Double(v) => write!(f, "{v}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DecodeSource {
    Bits(Device),
    Value(CalendarValue),
}
impl DecodeSource {
    fn parse(raw: &str, constants: bool) -> Result<Self, String> {
        if let Ok(d) = raw.parse::<Device>()
            && bits(d)
        {
            return Ok(Self::Bits(d));
        }
        let value = CalendarValue::parse(raw, false)?;
        if matches!(
            value,
            CalendarValue::Additional(AdditionalValue::Group { .. })
        ) || (!constants && value.constant().is_some())
        {
            return Err("DECO/ENCO kaynak rolü Kn grubu veya bu sabiti desteklemiyor".into());
        }
        Ok(Self::Value(value))
    }
}
impl std::fmt::Display for DecodeSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bits(d) => write!(f, "{d}"),
            Self::Value(v) => write!(f, "{v}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DecodeDestination {
    Bits(Device),
    Value(CalendarDestination),
}
impl DecodeDestination {
    fn base(&self) -> Device {
        match self {
            Self::Bits(d) => *d,
            Self::Value(value) => value.base(),
        }
    }
}
impl std::fmt::Display for DecodeDestination {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bits(d) => d.fmt(f),
            Self::Value(value) => value.fmt(f),
        }
    }
}

/// MEAN alone permits partial source ranges. Numeric sources retain the
/// established model-end and counter-boundary rules; packed sources retain
/// their declared element width instead of becoming projected word devices.
#[derive(Debug, Clone, PartialEq)]
pub enum MeanSource {
    Numeric(Device),
    Group(AdditionalValue),
}
impl MeanSource {
    fn parse(raw: &str, wide: bool) -> Result<Self, String> {
        if raw.trim().to_ascii_uppercase().starts_with('K') {
            let value = AdditionalValue::parse(raw.trim(), wide, false)?;
            if matches!(value, AdditionalValue::Group { .. }) {
                return Ok(Self::Group(value));
            }
            return Err("MEAN kaynağı KnX/Y/M/S veya T/C/D olmalı".into());
        }
        let source: Device = raw.parse()?;
        // A valid final low word remains usable even for DMEAN. Do not
        // replace this with generic double-operand full-pair validation.
        if wide && source.area == Area::C && source.index >= 200 {
            double_devices(source)?;
        } else {
            word_range(source, 1)?;
        }
        Ok(Self::Numeric(source))
    }
    fn devices(&self, out: &mut BTreeSet<Device>) {
        match self {
            Self::Numeric(d) => {
                out.insert(*d);
            }
            Self::Group(value) => value.devices(out),
        }
    }
    fn group_slots(&self, memory: &Memory, count: u16) -> Vec<Vec<(u16, Device)>> {
        let Self::Group(AdditionalValue::Group { base, digits, .. }) = self else {
            unreachable!("validated packed MEAN source");
        };
        let width = digits * 4;
        (0..count)
            .map(|slot| {
                (0..width)
                    .filter_map(|bit| {
                        let index = base.index.checked_add(slot * width)?.checked_add(bit)?;
                        let d = Device { index, ..*base };
                        let in_model =
                            d.area != Area::M || d.index <= memory.relay_limit.unwrap_or(4095);
                        (in_model && d.to_string().parse::<Device>().is_ok()).then_some((bit, d))
                    })
                    .collect()
            })
            .collect()
    }
    fn observe(&self, memory: &Memory, count: u16, wide: bool, out: &mut BTreeSet<Device>) {
        match self {
            Self::Numeric(source) => observe_slots(
                *source,
                count,
                wide,
                memory.data_register_limit.unwrap_or(9999),
                out,
            ),
            Self::Group(_) => out.extend(
                self.group_slots(memory, count)
                    .into_iter()
                    .flatten()
                    .map(|(_, d)| d),
            ),
        }
    }
    fn values(&self, memory: &Memory, count: u16, wide: bool) -> Vec<i32> {
        match self {
            Self::Numeric(source) => slots(
                *source,
                count,
                wide,
                memory.data_register_limit.unwrap_or(9999),
            )
            .into_iter()
            .map(|d| slot_value(memory, d, wide))
            .collect(),
            Self::Group(_) => self
                .group_slots(memory, count)
                .into_iter()
                .map(|bits| {
                    // Apply the manual's valid-address policy locally: unavailable
                    // bits are zero and the denominator remains n.
                    // Raw native pointer spills across areas are not emulated.
                    let value = bits.into_iter().fold(0u32, |value, (bit, d)| {
                        value | (u32::from(memory.bit(d)) << bit)
                    });
                    if wide {
                        value as i32
                    } else {
                        i32::from(value as i16)
                    }
                })
                .collect(),
        }
    }
}
impl std::fmt::Display for MeanSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Numeric(d) => d.fmt(f),
            Self::Group(value) => value.fmt(f),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DataAction {
    Decode {
        source: DecodeSource,
        dest: DecodeDestination,
        n: u16,
    },
    Encode {
        source: DecodeSource,
        dest: CalendarDestination,
        n: u16,
    },
    Sum {
        source: CalendarValue,
        dest: CalendarDestination,
    },
    BitOn {
        source: CalendarValue,
        dest: Device,
        n: BitIndexValue,
    },
    Mean {
        wide: bool,
        source: MeanSource,
        dest: CalendarDestination,
        n: BitIndexValue,
    },
    Root {
        source: DataValue,
        dest: Device,
    },
}

/// Shared installed mean arithmetic. Executors own cursor snapshots, checked
/// payload reads, the requested denominator and late destination resolution.
pub(crate) fn mean_result(values: &[i32], n: i32, wide: bool) -> i32 {
    let value = if wide {
        // Per-term truncation is observable with mixed signs: [4,-1]/2 ->2.
        let quotient: i64 = values.iter().map(|v| i64::from(*v / n)).sum();
        let remainder: i64 = values.iter().map(|v| i64::from(*v % n)).sum();
        quotient + remainder / i64::from(n)
    } else {
        values.iter().map(|v| i64::from(*v)).sum::<i64>() / i64::from(n)
    };
    value as i32
}

/// Shared numeric result for ordinary and indexed integer square root. The
/// executor owns access order; no second sqrt or flag algorithm is required.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RootResult {
    value: i32,
    wide: bool,
    zero: bool,
    borrow: bool,
}

impl RootResult {
    pub(crate) fn begin(m: &mut Memory) {
        m.set_system(1020, false);
        m.set_system(1021, false);
    }

    pub(crate) fn convert(value: i32, wide: bool) -> Option<Self> {
        if value < 0 {
            return None;
        }
        let root = if wide {
            f64::from(value).sqrt() as i32
        } else {
            (value as f32).sqrt() as i32
        };
        Some(Self {
            value: root,
            wide,
            zero: value == 0,
            borrow: i64::from(root) * i64::from(root) != i64::from(value),
        })
    }

    pub(crate) fn publish_flags(self, m: &mut Memory) {
        m.set_system(1020, self.zero);
        m.set_system(1021, self.borrow);
    }

    pub(crate) fn write_value(self, m: &mut Memory, dest: Device) {
        if self.wide {
            m.set_double_word(dest, self.value);
        } else {
            m.set_word(dest, self.value as i16);
        }
    }
}

fn bits(d: Device) -> bool {
    matches!(d.area, Area::X | Area::Y | Area::M | Area::S)
}
fn validate_dest(d: Device, wide: bool) -> Result<(), String> {
    if wide {
        double_devices(d)?;
    } else {
        word_range(d, 1)?;
    }
    Ok(())
}
fn const_n(raw: &str, max: u16) -> Result<u16, String> {
    match Operand::parse(raw)? {
        Operand::Constant(v) if (1..=max as i16).contains(&v) => Ok(v as u16),
        _ => Err(format!("Sayı K/H1..{max} sabiti olmalı")),
    }
}
fn checked_count(n: &BitIndexValue, minimum: i32, maximum: i32) -> Result<(), String> {
    if n.constant()
        .is_some_and(|v| !(minimum..=maximum).contains(&v))
    {
        return Err(format!("Sayı {minimum}..{maximum} aralığında olmalı"));
    }
    Ok(())
}
fn numeric_destination(raw: &str, wide: bool) -> Result<CalendarDestination, String> {
    let value = CalendarDestination::parse(raw, wide)?;
    if matches!(
        value.value(),
        CalendarValue::Additional(AdditionalValue::Group { .. })
    ) {
        return Err("Bu hedef rolü yalnız T/C/D/E/F destekliyor; Kn grubu kullanılamaz".into());
    }
    Ok(value)
}
fn slots(source: Device, n: u16, wide: bool, data_limit: u16) -> Vec<Device> {
    let mut result = Vec::new();
    let mut next = source;
    for _ in 0..n {
        let valid = if wide && next.area == Area::C && next.index >= 200 {
            double_devices(next).is_ok()
        } else {
            word_range(next, 1).is_ok()
        } && (next.area != Area::D || next.index <= data_limit);
        if valid {
            result.push(next);
        }
        let stride = if wide && !(next.area == Area::C && next.index >= 199) {
            2
        } else {
            1
        };
        let Some(index) = next.index.checked_add(stride) else {
            break;
        };
        next.index = index;
    }
    result
}

fn slot_high(d: Device, data_limit: u16) -> Option<Device> {
    if d.area == Area::C && d.index >= 199 {
        return None;
    }
    let high = Device {
        area: d.area,
        index: d.index.checked_add(1)?,
    };
    (word_range(high, 1).is_ok() && (high.area != Area::D || high.index <= data_limit))
        .then_some(high)
}

fn slot_value(m: &Memory, d: Device, wide: bool) -> i32 {
    if !wide {
        return i32::from(m.word(d));
    }
    if d.area == Area::C && d.index >= 199
        || slot_high(d, m.data_register_limit.unwrap_or(9999)).is_some()
    {
        m.double_word(d)
    } else {
        // A valid last low word participates even when its upper word is
        // outside the model's range; that unavailable upper half is zero.
        i32::from(m.word(d) as u16)
    }
}

impl DataAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let wide = name.starts_with('D') && !matches!(name, "DECO");
        let expected = if matches!(name, "SUM" | "DSUM" | "SQR" | "DSQR") {
            2
        } else {
            3
        };
        if args.len() != expected {
            return Err(format!("{name}: {expected} operand gerekli"));
        }
        Ok(match name {
            "DECO" => {
                let dest = if let Ok(d) = args[1].parse::<Device>()
                    && bits(d)
                {
                    DecodeDestination::Bits(d)
                } else {
                    DecodeDestination::Value(numeric_destination(&args[1], false)?)
                };
                let n = const_n(
                    &args[2],
                    if matches!(dest, DecodeDestination::Bits(_)) {
                        8
                    } else {
                        4
                    },
                )?;
                if let DecodeDestination::Bits(d) = &dest {
                    bit_span(*d, 1 << n, true)?;
                }
                let source = DecodeSource::parse(&args[0], true)?;
                if let DecodeSource::Bits(d) = &source {
                    bit_span(*d, n, false)?;
                }
                Self::Decode { source, dest, n }
            }
            "ENCO" => {
                let source = DecodeSource::parse(&args[0], false)?;
                let n = const_n(
                    &args[2],
                    if matches!(source, DecodeSource::Bits(_)) {
                        8
                    } else {
                        4
                    },
                )?;
                if let DecodeSource::Bits(d) = &source {
                    bit_span(*d, 1 << n, false)?;
                }
                let dest = numeric_destination(&args[1], false)?;
                Self::Encode { source, dest, n }
            }
            "SUM" | "DSUM" => Self::Sum {
                source: CalendarValue::parse(&args[0], wide)?,
                dest: numeric_destination(&args[1], wide)?,
            },
            "BON" | "DBON" => {
                let dest: Device = args[1].parse()?;
                if !dest.writable_bit() {
                    return Err("BON hedefi Y/M/S olmalı".into());
                }
                let n = BitIndexValue::parse(&args[2], wide)?;
                if matches!(
                    n.calendar_value(),
                    Some(CalendarValue::Additional(AdditionalValue::Group { .. }))
                ) {
                    return Err(
                        "BON bit numarası K/H, T/C/D veya E/F olmalı; Kn grubu kullanılamaz".into(),
                    );
                }
                checked_count(&n, 0, if wide { 31 } else { 15 })?;
                Self::BitOn {
                    source: CalendarValue::parse(&args[0], wide)?,
                    dest,
                    n,
                }
            }
            "MEAN" | "DMEAN" => {
                let source = MeanSource::parse(&args[0], wide)?;
                let dest = CalendarDestination::parse(&args[1], wide)?;
                let n = BitIndexValue::parse(&args[2], wide)?;
                checked_count(&n, 1, 64)?;
                Self::Mean {
                    wide,
                    source,
                    dest,
                    n,
                }
            }
            "SQR" | "DSQR" => {
                let dest: Device = args[1].parse()?;
                let source = DataValue::parse(&args[0], wide)?;
                let register = match &source {
                    DataValue::Word(Operand::Register(d))
                    | DataValue::Double(WideOperand::Register(d)) => Some(*d),
                    _ => None,
                };
                if dest.area != Area::D || register.is_some_and(|d| d.area != Area::D) {
                    return Err("SQR kaynağı K/H/D, hedefi D olmalı".into());
                }
                validate_dest(dest, wide)?;
                Self::Root { source, dest }
            }
            _ => return Err("Bilinmeyen veri işleme komutu".into()),
        })
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::Decode { dest, .. } => dest.base(),
            Self::Encode { dest, .. } | Self::Sum { dest, .. } | Self::Mean { dest, .. } => {
                dest.base()
            }
            Self::BitOn { dest, .. } | Self::Root { dest, .. } => *dest,
        }
    }
    pub fn wide(&self) -> bool {
        match self {
            Self::Sum { source, .. } | Self::BitOn { source, .. } => source.wide(),
            Self::Root { source, .. } => source.wide(),
            Self::Mean { wide, .. } => *wide,
            _ => false,
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Decode { .. } => "DECO",
            Self::Encode { .. } => "ENCO",
            Self::Sum { .. } => {
                if self.wide() {
                    "DSUM"
                } else {
                    "SUM"
                }
            }
            Self::BitOn { .. } => {
                if self.wide() {
                    "DBON"
                } else {
                    "BON"
                }
            }
            Self::Mean { .. } => {
                if self.wide() {
                    "DMEAN"
                } else {
                    "MEAN"
                }
            }
            Self::Root { .. } => {
                if self.wide() {
                    "DSQR"
                } else {
                    "SQR"
                }
            }
        }
    }
    pub fn header(&self) -> u16 {
        match self {
            Self::Decode { .. } => 0x6200,
            Self::Encode { .. } => 0x6400,
            Self::Sum { .. } => {
                if self.wide() {
                    0x6700
                } else {
                    0x6600
                }
            }
            Self::BitOn { .. } => {
                if self.wide() {
                    0x6900
                } else {
                    0x6800
                }
            }
            Self::Mean { .. } => {
                if self.wide() {
                    0x6b00
                } else {
                    0x6a00
                }
            }
            Self::Root { .. } => {
                if self.wide() {
                    0x7100
                } else {
                    0x7000
                }
            }
        }
    }
    pub fn label(&self) -> String {
        let name = self.name();
        match self {
            Self::Decode { source, dest, n } => format!("{name} {source} {dest} K{n}"),
            Self::Encode { source, dest, n } => format!("{name} {source} {dest} K{n}"),
            Self::Sum { source, dest } => {
                format!("{name} {source} {dest}")
            }
            Self::Root { source, dest } => {
                format!("{name} {source} {dest}")
            }
            Self::BitOn { source, dest, n } => format!("{name} {source} {dest} {n}"),
            Self::Mean {
                source, dest, n, ..
            } => format!("{name} {source} {dest} {n}"),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        out.extend((1020..=1022).map(Memory::system_device));
        Memory::error_devices(out);
        match self {
            Self::Decode { source, dest, n } => {
                match source {
                    DecodeSource::Bits(d) => out.extend(bit_span(*d, *n, false).unwrap()),
                    DecodeSource::Value(value) => value.devices(out),
                }
                match dest {
                    DecodeDestination::Bits(d) => out.extend(bit_span(*d, 1 << n, true).unwrap()),
                    DecodeDestination::Value(value) => value.devices(out),
                }
            }
            Self::Encode { source, dest, n } => {
                match source {
                    DecodeSource::Bits(d) => out.extend(bit_span(*d, 1 << n, false).unwrap()),
                    DecodeSource::Value(value) => value.devices(out),
                }
                dest.devices(out);
            }
            Self::Sum { source, dest } => {
                source.devices(out);
                dest.devices(out);
            }
            Self::Root { source, dest } => {
                source.devices(out);
                destination_devices(*dest, self.wide(), out);
            }
            Self::BitOn { source, dest, n } => {
                source.devices(out);
                n.devices(out);
                out.insert(*dest);
            }
            Self::Mean {
                source, dest, n, ..
            } => {
                n.devices(out);
                dest.devices(out);
                // The executed source span is clipped to the model's memory
                // limit and recorded dynamically, even for constant counts.
                source.devices(out);
            }
        }
    }
    /// Capture dynamic mean reads before an aliased destination can overwrite n.
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        if let Self::Mean {
            source, n, wide, ..
        } = self
        {
            let count = n.read(m);
            if (1..=64).contains(&count) {
                source.observe(m, count as u16, *wide, out);
            }
        }
    }
    pub fn execute(&self, m: &mut Memory) {
        match self {
            Self::Decode { source, dest, n } => {
                let value = match source {
                    DecodeSource::Bits(d) => bit_span(*d, *n, false)
                        .unwrap()
                        .iter()
                        .enumerate()
                        .fold(0u16, |value, (i, d)| value | (u16::from(m.bit(*d)) << i)),
                    DecodeSource::Value(v) => v.read(m) as u16,
                } & ((1 << n) - 1);
                match dest {
                    DecodeDestination::Bits(d) => {
                        for (i, d) in bit_span(*d, 1 << n, true).unwrap().into_iter().enumerate() {
                            m.set_internal(d, i == usize::from(value));
                        }
                    }
                    DecodeDestination::Value(target) => {
                        target.write(m, i32::from((1u16 << value) as i16))
                    }
                }
            }
            Self::Encode { source, dest, n } => {
                let highest = match source {
                    DecodeSource::Bits(d) => bit_span(*d, 1 << n, false)
                        .unwrap()
                        .iter()
                        .rposition(|d| m.bit(*d)),
                    DecodeSource::Value(value) => {
                        let value = value.read(m) as u16;
                        (0..(1usize << n)).rev().find(|i| value & (1 << i) != 0)
                    }
                };
                if let Some(index) = highest {
                    dest.write(m, index as i32);
                } else {
                    m.operation_error(0xe1a, 3);
                }
            }
            Self::Sum { source, dest } => {
                let value = source.read(m);
                let count = if source.wide() {
                    (value as u32).count_ones()
                } else {
                    (value as u16).count_ones()
                };
                if count == 0 {
                    m.set_system(1020, true);
                }
                dest.write(m, count as i32);
            }
            Self::BitOn { source, dest, n } => {
                let value = source.read(m) as u32;
                let n = n.read(m);
                if (0..if self.wide() { 32 } else { 16 }).contains(&n) {
                    m.set_internal(*dest, value & (1u32 << n) != 0);
                } else {
                    m.operation_error(0xe1a, if self.wide() { 9 } else { 5 });
                }
            }
            Self::Mean {
                source,
                dest,
                wide,
                n,
            } => {
                let n = n.read(m);
                if !(1..=64).contains(&n) {
                    m.operation_error(0xe1a, if *wide { 9 } else { 5 });
                    return;
                }
                let values = source.values(m, n as u16, *wide);
                dest.write(m, mean_result(&values, n, *wide));
            }
            Self::Root { source, dest } => {
                RootResult::begin(m);
                let value = source.read(m);
                let Some(result) = RootResult::convert(value, source.wide()) else {
                    m.operation_error(0xe1b, 1);
                    return;
                };
                result.publish_flags(m);
                result.write_value(m, *dest);
            }
        }
    }
    pub fn display_value(&self, m: &Memory) -> String {
        match self {
            Self::Decode {
                dest: DecodeDestination::Bits(dest),
                n,
                ..
            } => {
                let on = bit_span(*dest, 1 << n, true)
                    .unwrap()
                    .iter()
                    .filter(|d| m.bit(**d))
                    .count();
                format!("ON:{on}")
            }
            Self::Decode {
                dest: DecodeDestination::Value(dest),
                ..
            }
            | Self::Encode { dest, .. }
            | Self::Sum { dest, .. }
            | Self::Mean { dest, .. } => dest.read(m).to_string(),
            Self::BitOn { dest, .. } => u8::from(m.bit(*dest)).to_string(),
            Self::Root { source, dest } => {
                if source.wide() {
                    m.double_word(*dest).to_string()
                } else {
                    m.word(*dest).to_string()
                }
            }
        }
    }
}
fn destination_devices(d: Device, wide: bool, out: &mut BTreeSet<Device>) {
    if wide {
        out.extend(double_devices(d).unwrap());
    } else {
        out.insert(d);
    }
}
fn observe_slots(source: Device, n: u16, wide: bool, data_limit: u16, out: &mut BTreeSet<Device>) {
    for d in slots(source, n, wide, data_limit) {
        out.insert(d);
        if wide && let Some(high) = slot_high(d, data_limit) {
            out.insert(high);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plc::{Simulator, compile};
    fn d(name: &str) -> Device {
        name.parse().unwrap()
    }
    fn action(name: &str, args: &[&str]) -> DataAction {
        DataAction::parse(
            name,
            &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        )
        .unwrap()
    }

    #[test]
    fn decode_reads_consecutive_bits_and_encode_selects_highest_with_mask() {
        let mut m = Memory::default();
        m.set_input(d("X0"), true);
        m.set_input(d("X1"), true);
        m.set_internal(d("M108"), true);
        action("DECO", &["X0", "M100", "K3"]).execute(&mut m);
        for index in 0..8 {
            assert_eq!(m.bit(d(&format!("M{}", 100 + index))), index == 3);
        }
        assert!(m.bit(d("M108")), "outside decode output span");
        m.set_internal(d("M107"), true);
        action("ENCO", &["M100", "D0", "K3"]).execute(&mut m);
        assert_eq!(m.word(d("D0")), 7);
        m.set_word(d("D10"), 0xff08u16 as i16);
        action("ENCO", &["D10", "D11", "K3"]).execute(&mut m);
        assert_eq!(m.word(d("D11")), 3, "high eight source bits ignored");
        action("DECO", &["HFFFB", "D12", "K3"]).execute(&mut m);
        assert_eq!(m.word(d("D12")), 8, "all unused destination bits cleared");
        m.set_word(d("D10"), 0xff00u16 as i16);
        action("ENCO", &["D10", "D11", "K3"]).execute(&mut m);
        assert_eq!(m.word(d("D11")), 3, "zero encode holds destination");
        assert_eq!(m.word(d("D1067")), 0xe1a);
        assert_eq!(m.word(d("D1068")), 3);
    }

    #[test]
    fn population_count_sticky_zero_and_double_destination_are_distinct() {
        let mut m = Memory::default();
        m.set_system(1021, true);
        m.set_system(1022, true);
        m.set_word(d("D3"), -1);
        action("DSUM", &["K0", "D2"]).execute(&mut m);
        assert_eq!(m.double_word(d("D2")), 0);
        assert!(m.system_bit(1020));
        action("DSUM", &["HFFFFFFFF", "D2"]).execute(&mut m);
        assert_eq!(m.double_word(d("D2")), 32);
        assert!(m.system_bit(1020), "nonzero count holds previous zero flag");
        action("SUM", &["H8001", "D4"]).execute(&mut m);
        assert_eq!(m.word(d("D4")), 2);
        assert!(m.system_bit(1021));
        assert!(m.system_bit(1022));
    }

    #[test]
    fn bit_test_dynamic_index_matches_source_width_and_holds_on_error() {
        let mut m = Memory::default();
        m.set_word(d("D0"), i16::MIN);
        m.set_word(d("D10"), 15);
        action("BON", &["D0", "M0", "D10"]).execute(&mut m);
        assert!(m.bit(d("M0")));
        m.set_word(d("D10"), -1);
        action("BON", &["D0", "M0", "D10"]).execute(&mut m);
        assert!(m.bit(d("M0")));
        assert_eq!(m.word(d("D1067")), 0xe1a);
        assert_eq!(m.word(d("D1068")), 5);
        m.set_double_word(d("D20"), i32::MIN);
        m.set_double_word(d("D10"), 31);
        action("DBON", &["D20", "M1", "D10"]).execute(&mut m);
        assert!(m.bit(d("M1")));
        m.set_word(d("D11"), 1);
        m.set_system(1067, false);
        m.set_system(1068, false);
        action("DBON", &["K0", "M1", "D10"]).execute(&mut m);
        assert!(m.bit(d("M1")), "upper index word makes index invalid");
        assert_eq!(m.word(d("D1068")), 9);
    }

    #[test]
    fn mean_signed_division_native_double_decomposition_and_count_alias() {
        let mut m = Memory::default();
        m.set_word(d("D0"), -5);
        m.set_word(d("D1"), 2);
        action("MEAN", &["D0", "D10", "K2"]).execute(&mut m);
        assert_eq!(m.word(d("D10")), -1);
        m.set_double_word(d("D20"), 4);
        m.set_double_word(d("D22"), -1);
        action("DMEAN", &["D20", "D30", "K2"]).execute(&mut m);
        assert_eq!(
            m.double_word(d("D30")),
            2,
            "native per-term division differs from mathematical mean"
        );
        let p = compile("LD X0\nMEAN D0 D4 D4\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        for (name, value) in [("D0", 10), ("D1", 20), ("D2", 30), ("D4", 3)] {
            s.memory.set_word(d(name), value);
        }
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D4")), 20);
        assert!(
            s.observed_devices.contains(&d("D2")),
            "count captured before aliased result write"
        );
        s.scan(&p, 100);
        assert!(
            s.observed_devices.contains(&d("D19")),
            "new dynamic count observes full later span"
        );
    }

    #[test]
    fn mean_partial_model_ranges_and_counter_boundary_use_requested_denominator() {
        let mut m = Memory::default();
        m.data_register_limit = Some(4999);
        m.set_word(d("D4999"), 100);
        let a = action("MEAN", &["D4999", "D0", "K3"]);
        let mut observed = BTreeSet::new();
        a.observed_devices(&m, &mut observed);
        assert!(observed.contains(&d("D4999")));
        assert!(!observed.contains(&d("D5000")));
        a.execute(&mut m);
        assert_eq!(m.word(d("D0")), 33);
        m.set_word(d("D4999"), 7);
        for name in ["MEAN", "DMEAN"] {
            let a = action(name, &["D4999", "D0", "K2"]);
            observed.clear();
            a.observed_devices(&m, &mut observed);
            assert!(observed.contains(&d("D4999")));
            assert!(!observed.contains(&d("D5000")));
            a.execute(&mut m);
            assert_eq!(
                if name == "DMEAN" {
                    m.double_word(d("D0"))
                } else {
                    i32::from(m.word(d("D0")))
                },
                3
            );
        }
        m.set_word(d("D4999"), -7);
        action("DMEAN", &["D4999", "D0", "K2"]).execute(&mut m);
        assert_eq!(
            m.double_word(d("D0")),
            32764,
            "partial pair zero-extends its low word"
        );
        m.set_word(d("T255"), 7);
        action("DMEAN", &["T255", "D0", "K2"]).execute(&mut m);
        assert_eq!(m.double_word(d("D0")), 3);
        m.set_word(d("C199"), -1);
        m.set_double_word(d("C200"), 5);
        action("DMEAN", &["C199", "D2", "K2"]).execute(&mut m);
        assert_eq!(m.double_word(d("D2")), 32770);
        assert!(compile("LD X0\nDMEAN D9999 D0 K64\nEND").is_ok());
    }

    #[test]
    fn root_clears_zero_borrow_before_error_and_preserves_carry() {
        let mut m = Memory::default();
        m.set_system(1020, true);
        m.set_system(1021, true);
        m.set_system(1022, true);
        m.set_word(d("D0"), -1);
        m.set_word(d("D2"), 77);
        action("SQR", &["D0", "D2"]).execute(&mut m);
        assert_eq!(m.word(d("D2")), 77);
        assert!(!m.system_bit(1020));
        assert!(!m.system_bit(1021));
        assert!(m.system_bit(1022));
        assert_eq!(m.word(d("D1067")), 0xe1b);
        assert_eq!(m.word(d("D1068")), 1);
        for (source, root, zero, borrow) in [
            (4, 2, false, false),
            (2, 1, false, true),
            (0, 0, true, false),
        ] {
            m.set_word(d("D0"), source);
            action("SQR", &["D0", "D0"]).execute(&mut m);
            assert_eq!(m.word(d("D0")), root);
            assert_eq!(m.system_bit(1020), zero);
            assert_eq!(m.system_bit(1021), borrow);
            assert!(m.system_bit(1022));
        }
        action("DSQR", &["K2147395600", "D10"]).execute(&mut m);
        assert_eq!(m.double_word(d("D10")), 46340);
        assert!(!m.system_bit(1021));
    }

    #[test]
    fn pulse_results_hold_and_rearm_and_documented_operand_matrix_is_enforced() {
        let p = compile("LD X0\nSUM P0 D0\nEND");
        assert!(p.is_err());
        let p = compile("LD X0\nDECOP D0 D10 K3\nENCOP D10 D11 K3\nSUMP D0 D12\nDSUMP D2 D14\nBONP D0 Y0 K0\nDBONP D2 Y1 K31\nMEANP D0 D16 K1\nDMEANP D2 D18 K1\nSQRP D0 D20\nDSQRP D2 D22\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.memory.set_word(d("D0"), 3);
        s.memory.set_double_word(d("D2"), 100);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D10")), 8);
        s.memory.set_word(d("D0"), 2);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D10")), 8);
        s.memory.set_input(d("X0"), false);
        s.scan(&p, 100);
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D10")), 4);
        for source in ["T0", "C0", "X0"] {
            assert!(DataAction::parse("SQR", &[source.into(), "D0".into()]).is_err());
        }
        for (name, args) in [
            ("DECO", ["D0", "D1", "K5"]),
            ("ENCO", ["D0", "D1", "K0"]),
            ("BON", ["D0", "X0", "K0"]),
            ("DBON", ["D0", "Y0", "K32"]),
            ("MEAN", ["D0", "D1", "K65"]),
        ] {
            assert!(
                DataAction::parse(name, &args.map(String::from)).is_err(),
                "{name}"
            );
        }
    }
}
