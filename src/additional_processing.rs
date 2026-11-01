//! Additional word processing.
use crate::bit_index_operand::BitIndexValue;
use crate::calendar_operand::CalendarDestination;
use crate::data_processing::DataValue;
use crate::mask_random_operand::MaskRandomDestination;
use crate::plc::{Area, Device, Memory, Operand, word_range};
use crate::shift::bit_span;
use crate::wide::{WideOperand, double_devices};
use std::collections::BTreeSet;

/// CMPT/DCMPT behavior of the specifically audited installed simulators.
/// The offline default includes M1162 floating comparison from the manual;
/// its NaN and invalid-source behavior follows the current COMMGR ES2 image,
/// without claiming those edge cases for every physical PLC or firmware.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompareTableProfile {
    #[default]
    CurrentEs2,
    /// The exact current COMMGR SS2 image has distinct indexed source dispatch.
    CurrentSs2,
    InstalledLegacy,
}

/// A numeric operand including the manual's KnX/KnY/KnM/KnS packed-bit form.
#[derive(Debug, Clone, PartialEq)]
pub enum AdditionalValue {
    Word(Operand),
    Double(WideOperand),
    Group {
        base: Device,
        digits: u16,
        wide: bool,
    },
}
impl AdditionalValue {
    pub fn parse(raw: &str, wide: bool, write: bool) -> Result<Self, String> {
        let raw = raw.to_ascii_uppercase();
        if let Some(rest) = raw.strip_prefix('K')
            && let Some(offset) = rest.find(['X', 'Y', 'M', 'S'])
        {
            let digits: u16 = rest[..offset]
                .parse()
                .map_err(|_| "Kn bit grubu geçersiz")?;
            if !(1..=if wide { 8 } else { 4 }).contains(&digits) {
                return Err("Kn grubu kelimede K1..K4, çift kelimede K1..K8 olmalı".into());
            }
            let base = rest[offset..].parse()?;
            bit_span(base, digits * 4, write)?;
            return Ok(Self::Group { base, digits, wide });
        }
        let result = if wide {
            Self::Double(WideOperand::parse(&raw)?)
        } else {
            Self::Word(Operand::parse(&raw)?)
        };
        if write && result.base().is_none() {
            return Err("Hedef sayısal register veya KnY/KnM/KnS olmalı".into());
        }
        Ok(result)
    }
    pub fn base(&self) -> Option<Device> {
        match self {
            Self::Word(Operand::Register(d))
            | Self::Double(WideOperand::Register(d))
            | Self::Group { base: d, .. } => Some(*d),
            _ => None,
        }
    }
    pub fn wide(&self) -> bool {
        matches!(self, Self::Double(_) | Self::Group { wide: true, .. })
    }
    pub fn read(&self, m: &Memory) -> i32 {
        match self {
            Self::Word(v) => i32::from(v.read(m)),
            Self::Double(v) => v.read(m),
            Self::Group { base, digits, wide } => {
                let value = (0..digits * 4).fold(0u32, |v, n| {
                    v | (u32::from(m.bit(Device {
                        area: base.area,
                        index: base.index + n,
                    })) << n)
                });
                if *wide {
                    value as i32
                } else {
                    i32::from(value as i16)
                }
            }
        }
    }
    pub fn write(&self, m: &mut Memory, value: i32) {
        match self {
            Self::Word(Operand::Register(d)) => m.set_word(*d, value as i16),
            Self::Double(WideOperand::Register(d)) => m.set_double_word(*d, value),
            Self::Group { base, digits, .. } => {
                for n in 0..digits * 4 {
                    m.set_internal(
                        Device {
                            area: base.area,
                            index: base.index + n,
                        },
                        value as u32 & (1 << n) != 0,
                    );
                }
            }
            _ => unreachable!("validated writable operand"),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        match self {
            Self::Word(Operand::Register(d)) => {
                out.insert(*d);
            }
            Self::Double(v) => v.devices(out),
            Self::Group { base, digits, .. } => {
                out.extend(bit_span(*base, digits * 4, false).unwrap())
            }
            _ => {}
        }
    }
}
impl std::fmt::Display for AdditionalValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Word(v) => write!(f, "{v}"),
            Self::Double(v) => write!(f, "{v}"),
            Self::Group { base, digits, .. } => write!(f, "K{digits}{base}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AdditionalAction {
    Swap {
        wide: bool,
        dest: CalendarDestination,
    },
    MaskMove {
        source: BitIndexValue,
        mask: BitIndexValue,
        dest: MaskRandomDestination,
    },
    SignExtend {
        source: AdditionalValue,
        dest: Device,
    },
    WordSum {
        wide: bool,
        source: Device,
        count: DataValue,
        dest: Device,
    },
    Scale {
        source: Operand,
        slope: Operand,
        offset: Operand,
        dest: Device,
    },
    ScalePoints {
        wide: bool,
        source: DataValue,
        params: Device,
        dest: Device,
    },
    CompareTable {
        wide: bool,
        left: Device,
        right: Device,
        count: DataValue,
        dest: AdditionalValue,
    },
}

fn data_device(raw: &str) -> Result<Device, String> {
    let d: Device = raw.parse()?;
    if !matches!(d.area, Area::T | Area::C | Area::D) {
        return Err("Adres T/C/D alanında olmalı".into());
    }
    Ok(d)
}
fn d_device(raw: &str) -> Result<Device, String> {
    let d = data_device(raw)?;
    if d.area != Area::D {
        return Err("Operand D register olmalı".into());
    }
    Ok(d)
}
fn d_value(raw: &str, wide: bool) -> Result<DataValue, String> {
    let value = DataValue::parse(raw, wide)?;
    if matches!(&value, DataValue::Word(Operand::Register(d)) | DataValue::Double(WideOperand::Register(d)) if d.area != Area::D)
    {
        return Err("Sayı kaynağı K/H sabiti veya D register olmalı".into());
    }
    Ok(value)
}
fn word_d_value(raw: &str) -> Result<Operand, String> {
    match d_value(raw, false)? {
        DataValue::Word(v) => Ok(v),
        _ => unreachable!(),
    }
}
pub(crate) fn valid_slot(d: Device, wide: bool, limit: u16) -> bool {
    match d.area {
        Area::D => d.index <= limit,
        Area::T => d.index <= 255,
        Area::C => d.index <= if wide { 255 } else { 199 },
        _ => false,
    }
}
pub(crate) fn compare_table_matches(a: i32, b: i32, relation: u16, floating: bool) -> bool {
    let ordering = if floating {
        let (a, b) = (a as u32, b as u32);
        let is_nan = |v: u32| v & 0x7f80_0000 == 0x7f80_0000 && v & 0x007f_ffff != 0;
        if is_nan(a) || is_nan(b) {
            // The audited x87 unordered flags select EQ/default, LT and LE.
            return !matches!(relation, 3 | 4);
        }
        let ordered_bits = |mut v: u32| {
            if v & 0x7fff_ffff == 0 {
                v = 0;
            }
            if v & 0x8000_0000 != 0 {
                !v
            } else {
                v ^ 0x8000_0000
            }
        };
        // Bit ordering preserves subnormals and signed-zero equality without
        // inheriting the host's floating-point flush-to-zero settings.
        ordered_bits(a).cmp(&ordered_bits(b))
    } else {
        a.cmp(&b)
    };
    match relation {
        1 => ordering.is_lt(),
        2 => ordering.is_le(),
        3 => ordering.is_gt(),
        4 => ordering.is_ge(),
        _ => ordering.is_eq(),
    }
}
pub(crate) fn next_slot(d: Device, wide: bool) -> Device {
    Device {
        area: d.area,
        index: d
            .index
            .saturating_add(if wide && !(d.area == Area::C && d.index >= 199) {
                2
            } else {
                1
            }),
    }
}
pub(crate) fn slot_devices(d: Device, wide: bool, limit: u16) -> Vec<Device> {
    if !valid_slot(d, wide, limit) {
        return vec![];
    }
    let mut result = vec![d];
    let high = Device {
        area: d.area,
        index: d.index + 1,
    };
    if wide && !(d.area == Area::C && d.index >= 199) && valid_slot(high, false, limit) {
        result.push(high);
    }
    result
}
pub(crate) fn read_slot(m: &Memory, d: Device, wide: bool) -> i32 {
    if !wide {
        return i32::from(m.word(d));
    }
    if d.area == Area::C && d.index >= 199 {
        return m.double_word(d);
    }
    let high = Device {
        area: d.area,
        index: d.index + 1,
    };
    i32::from(m.word(d) as u16)
        | if valid_slot(high, false, m.data_register_limit.unwrap_or(9999)) {
            i32::from(m.word(high)) << 16
        } else {
            0
        }
}

impl AdditionalAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let wide = name.starts_with('D');
        let expected = match name {
            "SWAP" | "DSWAP" => 1,
            "MMOV" => 2,
            "MVM" | "DMVM" | "WSUM" | "DWSUM" | "SCLP" | "DSCLP" => 3,
            "SCAL" | "CMPT" | "DCMPT" => 4,
            _ => return Err("Bilinmeyen ek veri işleme komutu".into()),
        };
        if args.len() != expected {
            return Err(format!("{name}: {expected} operand gerekli"));
        }
        Ok(match name {
            "SWAP" | "DSWAP" => Self::Swap {
                wide,
                dest: CalendarDestination::parse(&args[0], wide)?,
            },
            "MVM" | "DMVM" => {
                let source = BitIndexValue::parse(&args[0], wide)?;
                if source.constant().is_some() {
                    return Err("MVM kaynağı register veya Kn bit grubu olmalı".into());
                }
                Self::MaskMove {
                    source,
                    mask: BitIndexValue::parse(&args[1], wide)?,
                    dest: MaskRandomDestination::parse(&args[2], wide)?,
                }
            }
            "MMOV" => {
                let dest = data_device(&args[1])?;
                double_devices(dest)?;
                Self::SignExtend {
                    source: AdditionalValue::parse(&args[0], false, false)?,
                    dest,
                }
            }
            "WSUM" | "DWSUM" => {
                let source = data_device(&args[0])?;
                if !valid_slot(source, wide, 9999) {
                    return Err("WSUM kaynak genişliği geçersiz".into());
                }
                Self::WordSum {
                    wide,
                    source,
                    count: d_value(&args[1], wide)?,
                    dest: data_device(&args[2])?,
                }
            }
            "SCAL" => Self::Scale {
                source: word_d_value(&args[0])?,
                slope: word_d_value(&args[1])?,
                offset: word_d_value(&args[2])?,
                dest: d_device(&args[3])?,
            },
            "SCLP" | "DSCLP" => {
                let dest = d_device(&args[2])?;
                if wide {
                    double_devices(dest)?;
                }
                Self::ScalePoints {
                    wide,
                    source: d_value(&args[0], wide)?,
                    params: d_device(&args[1])?,
                    dest,
                }
            }
            "CMPT" | "DCMPT" => {
                let left = data_device(&args[0])?;
                let right = data_device(&args[1])?;
                if [left, right]
                    .iter()
                    .any(|d| d.area == Area::C && d.index >= 200)
                {
                    return Err("CMPT kaynakları PDF'ye göre C0..C199 ile sınırlı".into());
                }
                Self::CompareTable {
                    wide,
                    left,
                    right,
                    count: d_value(&args[2], wide)?,
                    dest: AdditionalValue::parse(&args[3], wide, true)?,
                }
            }
            _ => unreachable!(),
        })
    }
    pub fn wide(&self) -> bool {
        match self {
            Self::Swap { wide, .. }
            | Self::WordSum { wide, .. }
            | Self::ScalePoints { wide, .. }
            | Self::CompareTable { wide, .. } => *wide,
            Self::MaskMove { source, .. } => source.wide(),
            _ => false,
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Swap { wide: false, .. } => "SWAP",
            Self::Swap { wide: true, .. } => "DSWAP",
            Self::MaskMove { .. } => {
                if self.wide() {
                    "DMVM"
                } else {
                    "MVM"
                }
            }
            Self::SignExtend { .. } => "MMOV",
            Self::WordSum { wide: false, .. } => "WSUM",
            Self::WordSum { wide: true, .. } => "DWSUM",
            Self::Scale { .. } => "SCAL",
            Self::ScalePoints { wide: false, .. } => "SCLP",
            Self::ScalePoints { wide: true, .. } => "DSCLP",
            Self::CompareTable { wide: false, .. } => "CMPT",
            Self::CompareTable { wide: true, .. } => "DCMPT",
        }
    }
    pub fn header(&self) -> u16 {
        (match self {
            Self::Swap { .. } => 0x4601,
            Self::MaskMove { .. } => 0x6e01,
            Self::SignExtend { .. } => 0x7e01,
            Self::WordSum { .. } => 0x8401,
            Self::Scale { .. } => 0xb201,
            Self::ScalePoints { .. } => 0xb401,
            Self::CompareTable { .. } => 0xb801,
        }) + if self.wide() { 0x100 } else { 0 }
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::Swap { dest, .. } => dest.base(),
            Self::MaskMove { dest, .. } => dest.base(),
            Self::CompareTable { dest, .. } => dest.base().expect("validated destination"),
            Self::SignExtend { dest, .. }
            | Self::WordSum { dest, .. }
            | Self::Scale { dest, .. }
            | Self::ScalePoints { dest, .. } => *dest,
        }
    }
    pub fn label(&self) -> String {
        let name = self.name();
        match self {
            Self::Swap { dest, .. } => format!("{name} {dest}"),
            Self::MaskMove { source, mask, dest } => format!("{name} {source} {mask} {dest}"),
            Self::SignExtend { source, dest } => format!("{name} {source} {dest}"),
            Self::WordSum {
                source,
                count,
                dest,
                ..
            } => format!("{name} {source} {count} {dest}"),
            Self::Scale {
                source,
                slope,
                offset,
                dest,
            } => format!("{name} {source} {slope} {offset} {dest}"),
            Self::ScalePoints {
                source,
                params,
                dest,
                ..
            } => format!("{name} {source} {params} {dest}"),
            Self::CompareTable {
                left,
                right,
                count,
                dest,
                ..
            } => format!("{name} {left} {right} {count} {dest}"),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        out.extend((1020..=1022).map(Memory::system_device));
        Memory::error_devices(out);
        match self {
            Self::Swap { dest, .. } => dest.devices(out),
            Self::MaskMove { source, mask, dest } => {
                source.devices(out);
                mask.devices(out);
                dest.devices(out);
            }
            Self::SignExtend { source, dest } => {
                source.devices(out);
                out.extend(double_devices(*dest).unwrap());
            }
            Self::WordSum {
                wide: _,
                source,
                count,
                dest,
            } => {
                count.devices(out);
                // Source spans are observed at execution with model boundaries.
                out.insert(*source);
                // WSUM writes its low destination word even when the high
                // word is beyond the selected model. Do not invent that
                // high address before a target's boundary is available.
                out.insert(*dest);
            }
            Self::Scale {
                source,
                slope,
                offset,
                dest,
            } => {
                for v in [source, slope, offset] {
                    if let Operand::Register(d) = v {
                        out.insert(*d);
                    }
                }
                out.insert(*dest);
            }
            Self::ScalePoints {
                wide,
                source,
                params,
                dest,
            } => {
                source.devices(out);
                let n = if *wide { 8 } else { 4 };
                if let Ok(block) = word_range(*params, n) {
                    out.extend(block);
                } else {
                    out.insert(*params);
                }
                out.extend(slot_devices(*dest, *wide, 9999));
                if *wide {
                    out.insert(Memory::system_device(1162));
                }
            }
            Self::CompareTable {
                wide,
                left,
                right,
                count,
                dest,
            } => {
                count.devices(out);
                dest.devices(out);
                out.extend([*left, *right]);
                if *wide {
                    out.insert(Memory::system_device(1162));
                }
            }
        }
    }
    pub fn display_value(&self, memory: &Memory) -> String {
        match self {
            Self::Swap { dest, .. } => dest.read(memory).to_string(),
            Self::MaskMove { dest, .. } => dest.read(memory).to_string(),
            Self::CompareTable { dest, .. } => dest.read(memory).to_string(),
            Self::SignExtend { dest, .. } => memory.double_word(*dest).to_string(),
            Self::WordSum { dest, .. } => read_slot(memory, *dest, true).to_string(),
            Self::Scale { dest, .. } => memory.word(*dest).to_string(),
            Self::ScalePoints { wide, dest, .. } => {
                let value = read_slot(memory, *dest, *wide);
                if *wide && memory.system_bit(1162) {
                    f32::from_bits(value as u32).to_string()
                } else {
                    value.to_string()
                }
            }
        }
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        let limit = m.data_register_limit.unwrap_or(9999);
        match self {
            Self::WordSum {
                wide,
                source,
                count,
                dest,
            } => {
                out.extend(slot_devices(*dest, true, limit));
                let mut next = *source;
                for _ in 0..count.read(m).clamp(1, 64) {
                    if !valid_slot(next, *wide, limit) {
                        break;
                    }
                    out.extend(slot_devices(next, *wide, limit));
                    next = next_slot(next, *wide);
                }
            }
            Self::CompareTable {
                wide,
                left,
                right,
                count,
                ..
            } => {
                let mut left = *left;
                let mut right = *right;
                for _ in 0..(count.read(m) as u8).clamp(1, if *wide { 32 } else { 16 }) {
                    if !valid_slot(left, *wide, limit) || !valid_slot(right, *wide, limit) {
                        break;
                    }
                    out.extend(slot_devices(left, *wide, limit));
                    out.extend(slot_devices(right, *wide, limit));
                    left = next_slot(left, *wide);
                    right = next_slot(right, *wide);
                }
            }
            _ => {}
        }
    }
    /// Called on active power only. `Memory` already holds the current native PC.
    pub fn execute(&self, m: &mut Memory) {
        if matches!(
            self,
            Self::WordSum { .. }
                | Self::Scale { .. }
                | Self::ScalePoints { .. }
                | Self::CompareTable { .. }
        ) {
            match crate::indexed_additional::IndexedAdditionalAction::from_legacy(self) {
                Ok(action) => {
                    action.execute(m, true, &mut BTreeSet::new());
                }
                Err(reason) => {
                    m.execution_limitations.insert(reason);
                }
            }
            return;
        }
        match self {
            Self::Swap { wide, dest } => {
                let raw = dest.read(m) as u32;
                let value = ((raw & 0x00ff_00ff) << 8) | ((raw & 0xff00_ff00) >> 8);
                dest.write(
                    m,
                    if *wide {
                        value as i32
                    } else {
                        i32::from(value as i16)
                    },
                );
            }
            Self::MaskMove { source, mask, dest } => {
                let source = source.read(m);
                let mask = mask.read(m);
                let old = dest.read(m);
                dest.write(m, (old & !mask) | (source & mask));
            }
            Self::SignExtend { source, dest } => m.set_double_word(*dest, source.read(m)),
            Self::WordSum { .. }
            | Self::Scale { .. }
            | Self::ScalePoints { .. }
            | Self::CompareTable { .. } => unreachable!("ordered additional handled above"),
        }
    }
}

pub(crate) fn scale_result(source: i16, slope: i16, offset: i16) -> i16 {
    let result = (i32::from(source) * i32::from(slope)) / 1000 + i32::from(offset);
    result.clamp(i16::MIN.into(), i16::MAX.into()) as i16
}
pub(crate) fn scale_points_result(
    source: i32,
    values: [i32; 4],
    wide: bool,
    floating: bool,
) -> Option<i32> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    let _control = NativeControl::enter();
    if wide && floating {
        let [max, min, high, low] = values.map(|v| f32::from_bits(v as u32));
        let source = f32::from_bits(source as u32);
        let clipped = if source > max {
            max
        } else if source < min {
            min
        } else {
            source
        };
        let delta = clipped - min;
        let span = max - min;
        if span == 0.0 || span.is_nan() {
            return None;
        }
        let ratio = delta / span;
        let dest_span = high - low;
        Some(float_product_add(ratio, dest_span, low).to_bits() as i32)
    } else {
        let [max, min, high, low] = values;
        let source = if source > max {
            max
        } else if source < min {
            min
        } else {
            source
        };
        let span = max.wrapping_sub(min);
        if span == 0 {
            return None;
        }
        let delta = source.wrapping_sub(min);
        let dest_span = high.wrapping_sub(low);
        let result = if wide {
            let product = integer_product_float(delta, dest_span);
            let quotient = float_divide_integer(product, span);
            (quotient.trunc() as i64 as i32).wrapping_add(low)
        } else {
            let product = delta.wrapping_mul(dest_span);
            ((i64::from(product) / i64::from(span)) as i32).wrapping_add(low)
        };
        Some(if wide {
            result
        } else {
            i32::from(result as i16)
        })
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
struct NativeControl(u16);
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
impl NativeControl {
    fn enter() -> Self {
        let mut original = 0u16;
        unsafe {
            std::arch::asm!("fnstcw word ptr [{word}]", word = in(reg) &mut original, options(nostack, preserves_flags));
        }
        let native = (original & !0x0f00) | 0x0300;
        unsafe {
            std::arch::asm!("fldcw word ptr [{word}]", word = in(reg) &native, options(nostack, preserves_flags));
        }
        Self(original)
    }
}
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
impl Drop for NativeControl {
    fn drop(&mut self) {
        unsafe {
            std::arch::asm!("fldcw word ptr [{word}]", word = in(reg) &self.0, options(nostack, preserves_flags));
        }
    }
}

fn integer_product_float(a: i32, b: i32) -> f32 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let mut result = 0f32;
        unsafe {
            std::arch::asm!("fild dword ptr [{a}]", "fild dword ptr [{b}]", "fmulp st(1), st(0)", "fstp dword ptr [{out}]",
            a = in(reg) &a, b = in(reg) &b, out = in(reg) &mut result, options(nostack));
        }
        result
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        (f64::from(a) * f64::from(b)) as f32
    }
}
fn float_divide_integer(product: f32, span: i32) -> f32 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let mut result = 0f32;
        unsafe {
            std::arch::asm!("fild dword ptr [{span}]", "fdivr dword ptr [{product}]", "fstp dword ptr [{out}]",
            span = in(reg) &span, product = in(reg) &product, out = in(reg) &mut result, options(nostack));
        }
        result
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        (f64::from(product) / f64::from(span)) as f32
    }
}
fn float_product_add(a: f32, b: f32, c: f32) -> f32 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let mut result = 0f32;
        unsafe {
            std::arch::asm!("fld dword ptr [{a}]", "fmul dword ptr [{b}]", "fadd dword ptr [{c}]", "fstp dword ptr [{out}]",
            a = in(reg) &a, b = in(reg) &b, c = in(reg) &c, out = in(reg) &mut result, options(nostack));
        }
        result
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        (f64::from(a) * f64::from(b) + f64::from(c)) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d(raw: &str) -> Device {
        raw.parse().unwrap()
    }
    fn action(raw: &str) -> AdditionalAction {
        let mut args = raw.split_whitespace();
        AdditionalAction::parse(
            args.next().unwrap(),
            &args.map(str::to_owned).collect::<Vec<_>>(),
        )
        .unwrap()
    }
    #[test]
    fn swap_is_per_word_and_mask_move_snapshots_aliases() {
        let mut m = Memory::default();
        m.set_double_word(d("D0"), 0x1234_abcd);
        action("DSWAP D0").execute(&mut m);
        assert_eq!(m.double_word(d("D0")) as u32, 0x3412_cdab);
        action("DMVM D0 HFF00FF00 D1").execute(&mut m);
        assert_eq!(m.double_word(d("D1")) as u32, 0x3400_cd12);
    }
    #[test]
    fn grouped_reads_zero_extend_short_groups_and_writes_preserve_neighbors() {
        let mut m = Memory::default();
        for i in 0..20 {
            m.set_internal(
                Device {
                    area: Area::M,
                    index: i,
                },
                true,
            );
        }
        let dest = AdditionalValue::parse("K2M0", false, true).unwrap();
        assert_eq!(dest.read(&m), 255);
        dest.write(&mut m, 0x12);
        assert!(m.bit(d("M8")));
        assert_eq!(dest.read(&m), 0x12);
        assert!(AdditionalValue::parse("K4X0", false, true).is_err());
    }
    #[test]
    fn word_sum_sign_extends_and_clips_last_low_word_before_overlapping_write() {
        let mut m = Memory::default();
        m.set_word(d("D9998"), -2);
        m.set_word(d("D9999"), -3);
        action("WSUM D9998 K64 D9998").execute(&mut m);
        assert_eq!(m.double_word(d("D9998")), -5);
        m.set_word(d("D9999"), -1);
        action("DWSUM D9999 K2 D0").execute(&mut m);
        assert_eq!(m.double_word(d("D0")), 65535);
    }
    #[test]
    fn word_sum_destination_tracking_uses_the_selected_model_boundary() {
        for name in ["WSUM", "DWSUM"] {
            let act = action(&format!("{name} D0 K1 D4999"));
            let mut static_devices = BTreeSet::new();
            act.devices(&mut static_devices);
            assert!(static_devices.contains(&d("D4999")));
            assert!(!static_devices.contains(&d("D5000")));
            for (limit, expected_high) in [(4999, false), (9999, true)] {
                let mut m = Memory::default();
                m.data_register_limit = Some(limit);
                m.set_double_word(d("D0"), -2);
                m.set_word(d("D5000"), 123);
                let mut observed = BTreeSet::new();
                act.observed_devices(&m, &mut observed);
                assert_eq!(observed.contains(&d("D5000")), expected_high);
                act.execute(&mut m);
                assert_eq!(m.word(d("D4999")), -2);
                assert_eq!(m.word(d("D5000")), if expected_high { -1 } else { 123 });
            }
            let interior = action(&format!("{name} D0 K1 D4998"));
            let mut m = Memory::default();
            m.data_register_limit = Some(4999);
            let mut observed = BTreeSet::new();
            interior.observed_devices(&m, &mut observed);
            assert!(observed.contains(&d("D4999")));
        }
    }
    #[test]
    fn scale_truncates_before_offset_and_saturates_final_word() {
        let mut m = Memory::default();
        action("SCAL K-1 K500 K2 D0").execute(&mut m);
        assert_eq!(m.word(d("D0")), 2);
        action("SCAL K32767 K32767 K32767 D0").execute(&mut m);
        assert_eq!(m.word(d("D0")), i16::MAX);
        action("SCAL K-32768 K32767 K-32768 D0").execute(&mut m);
        assert_eq!(m.word(d("D0")), i16::MIN);
    }
    #[test]
    fn scaling_has_integer_truncation_and_preserves_fractional_float_results() {
        let mut m = Memory::default();
        for (i, v) in [2, 0, 3, 0].into_iter().enumerate() {
            m.set_word(
                Device {
                    area: Area::D,
                    index: i as u16,
                },
                v,
            );
        }
        action("SCLP K1 D0 D10").execute(&mut m);
        assert_eq!(m.word(d("D10")), 1);
        for (i, v) in [3000f32, 200., 500., 30.].into_iter().enumerate() {
            m.set_double_word(
                Device {
                    area: Area::D,
                    index: i as u16 * 2,
                },
                v.to_bits() as i32,
            );
        }
        m.set_double_word(d("D20"), 500f32.to_bits() as i32);
        m.set_system(1162, true);
        action("DSCLP D20 D0 D10").execute(&mut m);
        let value = f32::from_bits(m.double_word(d("D10")) as u32);
        assert!((value - 80.35714).abs() < 0.00001);
        m.set_double_word(d("D0"), 200f32.to_bits() as i32);
        action("DSCLP D20 D0 D10").execute(&mut m);
        assert!(m.system_bit(1067));
        assert_eq!(m.word(d("D1067")), 0xe1a);
    }
    #[test]
    fn legacy_comparison_modes_clear_unused_bits_and_ignore_float_mode() {
        let mut m = Memory::default();
        m.compare_table_profile = CompareTableProfile::InstalledLegacy;
        for (i, v) in [-1, 2, 3].into_iter().enumerate() {
            m.set_word(
                Device {
                    area: Area::D,
                    index: i as u16,
                },
                v,
            );
        }
        for (i, v) in [1, 2, 1].into_iter().enumerate() {
            m.set_word(
                Device {
                    area: Area::D,
                    index: 10 + i as u16,
                },
                v,
            );
        }
        for (relation, expected) in [(0, 2), (1, 1), (2, 3), (3, 4), (4, 6), (255, 2)] {
            action(&format!("CMPT D0 D10 H{:04X} D20", (relation << 8) | 3)).execute(&mut m);
            assert_eq!(m.word(d("D20")), expected);
        }
        m.set_system(1162, true);
        m.set_double_word(d("D0"), (-2f32).to_bits() as i32);
        m.set_double_word(d("D10"), (-1f32).to_bits() as i32);
        action("DCMPT D0 D10 H0301 D20").execute(&mut m);
        assert_eq!(m.double_word(d("D20")), 1);
        assert!(
            AdditionalAction::parse(
                "CMPT",
                &"C200 D0 K1 D20"
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            )
            .is_err()
        );
    }
    #[test]
    fn legacy_comparison_unavailable_later_source_holds_the_entire_destination() {
        for name in ["CMPT", "DCMPT"] {
            let mut m = Memory::default();
            m.compare_table_profile = CompareTableProfile::InstalledLegacy;
            m.set_word(d("D9999"), 123);
            m.set_word(d("T255"), 123);
            m.set_double_word(d("D100"), -1);
            for flag in 1020..=1022 {
                m.set_system(flag, true);
            }
            action(&format!("{name} D9999 T255 K3 D100")).execute(&mut m);
            assert_eq!(m.double_word(d("D100")), -1, "{name}");
            assert!((1020..=1022).all(|flag| m.system_bit(flag)));
            assert!(!m.system_bit(1067));
            assert!(!m.system_bit(1068));
            // A missing upper word on the first double source alone is
            // zero-filled; only the subsequent missing low element blocks it.
            action(&format!("{name} D9999 T255 K1 D100")).execute(&mut m);
            assert_eq!(m.word(d("D100")), 1);
            assert_eq!(m.word(d("D101")), if name == "DCMPT" { 0 } else { -1 });
        }
    }
    #[test]
    fn mmov_sign_extension_preserves_flags_and_all_matrix_rejections_are_explicit() {
        let mut m = Memory::default();
        m.set_system(1022, true);
        action("MMOV H8001 D10").execute(&mut m);
        assert_eq!(m.double_word(d("D10")), -32767);
        assert!(m.system_bit(1022));
        for raw in [
            "MVM K1 K2 D0",
            "SCAL T0 K1 K2 D0",
            "SCLP D0 T0 D10",
            "WSUM D0 T0 D10",
            "SWAP K1",
            "MVM D0 K1 K4X0",
        ] {
            let mut args = raw.split_whitespace();
            assert!(
                AdditionalAction::parse(
                    args.next().unwrap(),
                    &args.map(str::to_owned).collect::<Vec<_>>()
                )
                .is_err(),
                "{raw}"
            );
        }
    }
}
