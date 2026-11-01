//! Installed matrix bit operations. Raw tables stream; grouped tables stage.
//! and the PDF's K4-only prose versus its explicit K2, three-row example.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::{CalendarIndices, CalendarValue};
use crate::data_processing::DataValue;
use crate::plc::{Area, Device, Memory, Operand};
use crate::shift::bit_span;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatrixBinary {
    And,
    Or,
    Xor,
    Xnor,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MatrixAction {
    Binary {
        op: MatrixBinary,
        left: AdditionalValue,
        right: AdditionalValue,
        dest: AdditionalValue,
        count: DataValue,
    },
    Invert {
        source: AdditionalValue,
        dest: AdditionalValue,
        count: DataValue,
    },
    Compare {
        left: AdditionalValue,
        right: AdditionalValue,
        count: DataValue,
        pointer: CalendarValue,
    },
    Read {
        source: AdditionalValue,
        count: DataValue,
        pointer: CalendarValue,
    },
    Write {
        source: AdditionalValue,
        count: DataValue,
        pointer: CalendarValue,
    },
    Shift {
        source: AdditionalValue,
        dest: AdditionalValue,
        count: DataValue,
    },
    Rotate {
        source: AdditionalValue,
        dest: AdditionalValue,
        count: DataValue,
    },
    Count {
        source: AdditionalValue,
        count: DataValue,
        dest: CalendarValue,
    },
}

fn block(raw: &str, write: bool) -> Result<AdditionalValue, String> {
    // Word C200+ remains spellable; its raw halfword layout is checked at use.
    let value = if raw.starts_with(['T', 'C', 'D']) {
        AdditionalValue::Word(Operand::Register(raw.parse()?))
    } else {
        AdditionalValue::parse(raw, false, write)?
    };
    match &value {
        AdditionalValue::Word(Operand::Register(d))
            if matches!(d.area, Area::T | Area::C | Area::D) =>
        {
            Ok(value)
        }
        AdditionalValue::Group { base, .. } if !write || base.writable_bit() => Ok(value),
        _ => {
            Err("Matris tablosu KnX/KnY/KnM/KnS veya T/C/D olmalı; hedef KnY/KnM/KnS olmalı".into())
        }
    }
}
fn scalar(raw: &str) -> Result<CalendarValue, String> {
    let value = if raw.starts_with(['T', 'C', 'D']) {
        CalendarValue::Additional(AdditionalValue::Word(Operand::Register(raw.parse()?)))
    } else {
        CalendarValue::parse(raw, false)?
    };
    match &value {
        CalendarValue::Index { .. } => Ok(value),
        CalendarValue::Additional(AdditionalValue::Word(Operand::Register(_))) => Ok(value),
        CalendarValue::Additional(AdditionalValue::Group { base, .. }) if base.writable_bit() => {
            Ok(value)
        }
        _ => Err("Matris işaretçisi/sonucu KnY/KnM/KnS, T/C/D veya E/F olmalı".into()),
    }
}
fn count(raw: &str) -> Result<DataValue, String> {
    let value = DataValue::parse(raw, false)?;
    if matches!(
        &value,
        DataValue::Word(Operand::Constant(_) | Operand::Register(Device { area: Area::D, .. }))
    ) {
        Ok(value)
    } else {
        Err("Matris n operandı K/H/D olmalı".into())
    }
}
fn write_scalar(v: &CalendarValue, m: &mut Memory, n: i16) {
    if matches!(v,CalendarValue::Additional(v) if natural_counter(v)) {
        return;
    }
    match v {
        CalendarValue::Additional(v) => v.write(m, i32::from(n)),
        CalendarValue::Index {
            index, high_bank, ..
        } => {
            CalendarIndices::write_word(m, *index, *high_bank, n).expect("validated index");
        }
    }
}
fn natural_counter(v: &AdditionalValue) -> bool {
    matches!(v,AdditionalValue::Word(Operand::Register(d)) if d.area==Area::C && d.index>=200)
}
fn read_scalar(v: &CalendarValue, m: &mut Memory, pc_offset: u16) -> i16 {
    if matches!(v,CalendarValue::Additional(v) if natural_counter(v)) {
        // The installed numeric16 reader rejects natural C200+, returns zero
        // and reports E1A; its paired writer silently leaves the counter held.
        m.operation_error(0xe1a, pc_offset);
        0
    } else {
        v.read(m) as i16
    }
}
fn group(v: &AdditionalValue) -> bool {
    matches!(v, AdditionalValue::Group { .. })
}
fn scratch_byte(m: &mut Memory, high: bool, value: u8) {
    // Production initializes the whole word from uploaded program length. A
    // direct unseeded fixture cannot fabricate the unknown preserved byte.
    m.native_numeric_scratch = m.native_numeric_scratch.map(|old| {
        let old = old as u16;
        if high {
            ((old & 0xff) | u16::from(value) << 8) as i16
        } else {
            ((old & 0xff00) | u16::from(value)) as i16
        }
    });
}
fn group_offset(v: &AdditionalValue) -> Option<u8> {
    if let AdditionalValue::Group { base, .. } = v {
        Some((base.index & 7) as u8)
    } else {
        None
    }
}
fn check_device(m: &Memory, d: Device) -> Result<(), String> {
    let max = match d.area {
        Area::X | Area::Y | Area::T | Area::C => 255,
        Area::S => 1023,
        Area::M => m.relay_limit.unwrap_or(4095),
        Area::D => m.data_register_limit.unwrap_or(9999),
    };
    if d.index > max {
        Err(format!("{d} model sınırını aşıyor"))
    } else {
        Ok(())
    }
}
fn block_devices(v: &AdditionalValue, n: u16, physical_write: bool) -> Result<Vec<Device>, String> {
    match v {
        AdditionalValue::Group { base, digits, .. } => bit_span(
            *base,
            n.checked_mul(if physical_write { 16 } else { digits * 4 })
                .ok_or("Matris bit aralığı taşıyor")?,
            false,
        ),
        AdditionalValue::Word(Operand::Register(base)) => {
            (0..n).map(|i| raw_cell(*base, i).map(|(d, _)| d)).collect()
        }
        _ => Err("Geçersiz matris tablosu".into()),
    }
}

/// Original decoder C0..199 occupies 200 WORDs immediately followed by the
/// 112 halves of C200..255. Matrix loops advance two bytes, even across C199.
/// A span reaching the following physical D bank is rejected before execution.
fn raw_cell(base: Device, row: u16) -> Result<(Device, bool), String> {
    if base.area != Area::C {
        return Ok((
            Device {
                area: base.area,
                index: base
                    .index
                    .checked_add(row)
                    .ok_or("Matris kelime aralığı taşıyor")?,
            },
            false,
        ));
    }
    let offset = if base.index < 200 {
        base.index
    } else {
        200 + 2 * (base.index - 200)
    };
    let offset = offset
        .checked_add(row)
        .ok_or("Matris sayaç aralığı taşıyor")?;
    if offset >= 312 {
        return Err("Ham matris C255 üst yarım-kelimesinden sonraki fiziksel alanı aşamaz".into());
    }
    Ok(if offset < 200 {
        (
            Device {
                area: Area::C,
                index: offset,
            },
            false,
        )
    } else {
        (
            Device {
                area: Area::C,
                index: 200 + (offset - 200) / 2,
            },
            (offset - 200) % 2 != 0,
        )
    })
}

/// A grouped view snapshots low-width rows with zero padding; a raw view
/// performs every read from current memory. Destination commit is delayed
/// only for groups, preserving the installed overlap and flag-alias order.
struct Table {
    value: AdditionalValue,
    rows: Option<Vec<u16>>,
}
impl Table {
    fn new(value: &AdditionalValue, n: u16, m: &Memory) -> Self {
        let rows = if let AdditionalValue::Group { base, digits, .. } = value {
            Some(
                (0..n)
                    .map(|row| {
                        (0..digits * 4).fold(0, |v, bit| {
                            v | u16::from(m.bit(Device {
                                area: base.area,
                                index: base.index + row * digits * 4 + bit,
                            })) << bit
                        })
                    })
                    .collect(),
            )
        } else {
            None
        };
        Self {
            value: value.clone(),
            rows,
        }
    }
    fn read_word(&self, m: &Memory, row: u16) -> u16 {
        if let Some(rows) = &self.rows {
            rows[usize::from(row)]
        } else {
            let (d, high) = raw_cell(self.value.base().unwrap(), row).expect("validated raw span");
            if d.area == Area::C && d.index >= 200 {
                (m.double_word(d) as u32 >> if high { 16 } else { 0 }) as u16
            } else {
                m.word(d) as u16
            }
        }
    }
    fn write_word(&mut self, m: &mut Memory, row: u16, value: u16) {
        if let Some(rows) = &mut self.rows {
            rows[usize::from(row)] = value;
        } else {
            let (d, high) = raw_cell(self.value.base().unwrap(), row).expect("validated raw span");
            if d.area == Area::C && d.index >= 200 {
                let old = m.double_word(d) as u32;
                m.set_double_word(
                    d,
                    if high {
                        (old & 0xffff) | (u32::from(value) << 16)
                    } else {
                        (old & 0xffff0000) | u32::from(value)
                    } as i32,
                );
            } else {
                m.set_word(d, value as i16);
            }
        }
    }
    fn bit(&self, m: &Memory, bit: u16) -> bool {
        self.read_word(m, bit / 16) & (1 << (bit % 16)) != 0
    }
    fn set_bit(&mut self, m: &mut Memory, bit: u16, value: bool) {
        let mask = 1 << (bit % 16);
        let old = self.read_word(m, bit / 16);
        self.write_word(m, bit / 16, if value { old | mask } else { old & !mask });
    }
    fn commit(&self, m: &mut Memory) {
        if let (Some(rows), AdditionalValue::Group { base, digits, .. }) = (&self.rows, &self.value)
        {
            for (row, value) in rows.iter().enumerate() {
                for bit in 0..digits * 4 {
                    m.set_internal(
                        Device {
                            area: base.area,
                            index: base.index + row as u16 * digits * 4 + bit,
                        },
                        value & (1 << bit) != 0,
                    );
                }
            }
        }
    }
}

impl MatrixAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let arity = match name {
            "MAND" | "MOR" | "MXOR" | "MXNR" | "MCMP" => 4,
            "MINV" | "MBRD" | "MBWR" | "MBS" | "MBR" | "MBC" => 3,
            _ => return Err("Bilinmeyen matris komutu".into()),
        };
        if args.len() != arity {
            return Err(format!("{name}: {arity} operand gerekli"));
        }
        Ok(match name {
            "MAND" | "MOR" | "MXOR" | "MXNR" => Self::Binary {
                op: match name {
                    "MAND" => MatrixBinary::And,
                    "MOR" => MatrixBinary::Or,
                    "MXOR" => MatrixBinary::Xor,
                    _ => MatrixBinary::Xnor,
                },
                left: block(&args[0], false)?,
                right: block(&args[1], false)?,
                dest: block(&args[2], true)?,
                count: count(&args[3])?,
            },
            "MINV" => Self::Invert {
                source: block(&args[0], false)?,
                dest: block(&args[1], true)?,
                count: count(&args[2])?,
            },
            "MCMP" => Self::Compare {
                left: block(&args[0], false)?,
                right: block(&args[1], false)?,
                count: count(&args[2])?,
                pointer: scalar(&args[3])?,
            },
            "MBRD" => Self::Read {
                source: block(&args[0], false)?,
                count: count(&args[1])?,
                pointer: scalar(&args[2])?,
            },
            // KnX is explicitly writable here, and uses process-image writes.
            "MBWR" => Self::Write {
                source: block(&args[0], false)?,
                count: count(&args[1])?,
                pointer: scalar(&args[2])?,
            },
            "MBS" => Self::Shift {
                source: block(&args[0], false)?,
                dest: block(&args[1], true)?,
                count: count(&args[2])?,
            },
            "MBR" => Self::Rotate {
                source: block(&args[0], false)?,
                dest: block(&args[1], true)?,
                count: count(&args[2])?,
            },
            _ => Self::Count {
                source: block(&args[0], false)?,
                count: count(&args[1])?,
                dest: scalar(&args[2])?,
            },
        })
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Binary { op, .. } => match op {
                MatrixBinary::And => "MAND",
                MatrixBinary::Or => "MOR",
                MatrixBinary::Xor => "MXOR",
                MatrixBinary::Xnor => "MXNR",
            },
            Self::Invert { .. } => "MINV",
            Self::Compare { .. } => "MCMP",
            Self::Read { .. } => "MBRD",
            Self::Write { .. } => "MBWR",
            Self::Shift { .. } => "MBS",
            Self::Rotate { .. } => "MBR",
            Self::Count { .. } => "MBC",
        }
    }
    pub fn header(&self) -> u16 {
        match self.name() {
            "MAND" => 0x8601,
            "MOR" => 0x8801,
            "MXOR" => 0x8a01,
            "MXNR" => 0x8c01,
            "MINV" => 0x8e01,
            "MCMP" => 0x9001,
            "MBRD" => 0x9201,
            "MBWR" => 0x9401,
            "MBS" => 0x9601,
            "MBR" => 0x9801,
            _ => 0x9a01,
        }
    }
    pub fn wide(&self) -> bool {
        false
    }
    pub fn count(&self) -> &DataValue {
        match self {
            Self::Binary { count, .. }
            | Self::Invert { count, .. }
            | Self::Compare { count, .. }
            | Self::Read { count, .. }
            | Self::Write { count, .. }
            | Self::Shift { count, .. }
            | Self::Rotate { count, .. }
            | Self::Count { count, .. } => count,
        }
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::Binary { dest, .. }
            | Self::Invert { dest, .. }
            | Self::Shift { dest, .. }
            | Self::Rotate { dest, .. } => dest.base().unwrap(),
            Self::Write { source, .. } => source.base().unwrap(),
            Self::Compare { pointer, .. } | Self::Read { pointer, .. } => pointer.base().unwrap(),
            Self::Count { dest, .. } => dest.base().unwrap(),
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::Binary {
                left,
                right,
                dest,
                count,
                ..
            } => format!("{} {left} {right} {dest} {count}", self.name()),
            Self::Compare {
                left,
                right,
                count,
                pointer,
            } => format!("MCMP {left} {right} {count} {pointer}"),
            Self::Invert {
                source,
                dest,
                count,
            }
            | Self::Shift {
                source,
                dest,
                count,
            }
            | Self::Rotate {
                source,
                dest,
                count,
            } => format!("{} {source} {dest} {count}", self.name()),
            Self::Read {
                source,
                count,
                pointer,
            }
            | Self::Write {
                source,
                count,
                pointer,
            } => format!("{} {source} {count} {pointer}", self.name()),
            Self::Count {
                source,
                count,
                dest,
            } => format!("MBC {source} {count} {dest}"),
        }
    }
    fn blocks(&self) -> Vec<(&AdditionalValue, bool)> {
        match self {
            Self::Binary {
                left, right, dest, ..
            } => vec![(left, false), (right, false), (dest, false)],
            Self::Compare { left, right, .. } => vec![(left, false), (right, false)],
            Self::Invert { source, dest, .. }
            | Self::Shift { source, dest, .. }
            | Self::Rotate { source, dest, .. } => vec![(source, false), (dest, false)],
            Self::Write { source, .. } => vec![(source, true)],
            Self::Read { source, .. } | Self::Count { source, .. } => vec![(source, false)],
        }
    }
    fn scalar(&self) -> Option<&CalendarValue> {
        match self {
            Self::Compare { pointer, .. }
            | Self::Read { pointer, .. }
            | Self::Write { pointer, .. } => Some(pointer),
            Self::Count { dest, .. } => Some(dest),
            _ => None,
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        Memory::error_devices(out);
        out.extend((1088..=1099).map(Memory::system_device));
        self.count().devices(out);
        for (v, _) in self.blocks() {
            v.devices(out);
        }
        if let Some(v) = self.scalar() {
            v.devices(out);
        }
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        let n = self.count().read(m);
        if (1..=256).contains(&n) {
            for (v, physical) in self.blocks() {
                if let Ok(devices) = block_devices(v, n as u16, physical) {
                    out.extend(devices.into_iter().filter(|d| check_device(m, *d).is_ok()));
                }
            }
        }
    }
    /// Always invoke BEFORE target execution: binary group setup may overrun
    /// native fixed buffers even though its invalid-count error is already set.
    pub fn validate_native_span(&self, m: &Memory) -> Result<(), String> {
        if self.installed_unsafe_natural_start() {
            return Err("Matris ikili işleminde doğrudan C200+ başlangıcı kurulu simülatörde eski tampon/pointer durumuyla devam edip kilitlenebilir; native yükleme engellendi".into());
        }
        if self.silent_natural_rejection() {
            return Ok(());
        }
        let n = self.count().read(m);
        if !(1..=256).contains(&n) {
            return Err(
                "Matris n değeri 1..256 dışında; yerel E1A, güvensiz native tampon yolu engellendi"
                    .into(),
            );
        }
        self.validate_spans(m, n as u16)
    }
    pub fn installed_unsafe_natural_start(&self) -> bool {
        matches!(self,Self::Binary{left,right,dest,..} if [left,right,dest].into_iter().any(natural_counter))
    }
    fn validate_spans(&self, m: &Memory, n: u16) -> Result<(), String> {
        for (v, physical) in self.blocks() {
            for d in block_devices(v, n, physical)? {
                check_device(m, d)?;
            }
        }
        let mut devices = BTreeSet::new();
        self.count().devices(&mut devices);
        if let Some(v) = self.scalar() {
            v.devices(&mut devices);
        }
        for d in devices {
            check_device(m, d)?;
        }
        Ok(())
    }
    fn silent_natural_rejection(&self) -> bool {
        match self {
            Self::Binary {
                left, right, dest, ..
            } => [left, right, dest].into_iter().any(natural_counter),
            Self::Invert { source, dest, .. } => natural_counter(source) || natural_counter(dest),
            _ => false,
        }
    }
    fn count_offset(&self) -> u16 {
        match self {
            Self::Binary { .. } => 7,
            Self::Invert { .. }
            | Self::Compare { .. }
            | Self::Shift { .. }
            | Self::Rotate { .. } => 5,
            _ => 3,
        }
    }
    /// Active execution only. The caller owns ordinary power and P edge gates;
    /// disabled instructions leave all shared relays and numeric scratch held.
    pub fn execute(&self, m: &mut Memory) {
        if self.installed_unsafe_natural_start() {
            m.execution_limitations.insert("Matris ikili işleminde doğrudan C200+ başlangıcının kurulu yordamı güvenilir biçimde çalışmıyor; eski tampon/PC kusuru yürütülmedi".into());
            return;
        }
        // These decoder scratch writes precede even an invalid n check.
        match self {
            Self::Binary {
                left, right, dest, ..
            } => {
                if natural_counter(left) {
                    return;
                }
                scratch_byte(m, true, u8::from(group(left)));
                if natural_counter(right) || natural_counter(dest) {
                    return;
                }
                if let Some(offset) = group_offset(dest) {
                    scratch_byte(m, false, offset);
                }
            }
            Self::Invert { source, dest, .. } => {
                if natural_counter(source) || natural_counter(dest) {
                    return;
                }
                if let Some(offset) = group_offset(dest) {
                    scratch_byte(m, false, offset);
                }
            }
            _ => {}
        }
        let n = self.count().read(m);
        if !(1..=256).contains(&n) {
            m.operation_error(0xe1a, self.count_offset());
            // Valid local semantics stop before unsafe >256 native staging.
            if let Self::Binary { left, .. } = self
                && group(left)
            {
                scratch_byte(m, true, 0);
            }
            return;
        }
        let n = n as u16;
        if let Err(reason) = self.validate_spans(m, n) {
            m.execution_limitations
                .insert(format!("{}: {reason}", self.name()));
            return;
        }
        let max = 16 * n - 1;
        match self {
            Self::Binary {
                op,
                left,
                right,
                dest,
                ..
            } => {
                let left = Table::new(left, n, m);
                let right = Table::new(right, n, m);
                let mut dest = Table::new(dest, n, m);
                scratch_byte(m, true, 0);
                for row in 0..n {
                    let a = left.read_word(m, row);
                    let b = right.read_word(m, row);
                    dest.write_word(
                        m,
                        row,
                        match op {
                            MatrixBinary::And => a & b,
                            MatrixBinary::Or => a | b,
                            MatrixBinary::Xor => a ^ b,
                            MatrixBinary::Xnor => !(a ^ b),
                        },
                    );
                }
                dest.commit(m);
            }
            Self::Invert { source, dest, .. } => {
                let source = Table::new(source, n, m);
                let mut dest = Table::new(dest, n, m);
                for row in 0..n {
                    let v = source.read_word(m, row);
                    dest.write_word(m, row, !v);
                }
                dest.commit(m);
            }
            Self::Compare {
                left,
                right,
                pointer,
                ..
            } => {
                let left = Table::new(left, n, m);
                let right = Table::new(right, n, m);
                m.native_numeric_scratch = Some(max as i16);
                let mut start = read_scalar(pointer, m, 7);
                if m.system_bit(1090) {
                    start = -1;
                }
                if start > max as i16 || (start < 0 && !m.system_bit(1090)) {
                    m.set_system(1092, true);
                    return;
                }
                m.set_system(1089, false);
                m.set_system(1091, false);
                if start == max as i16 {
                    start = -1;
                }
                let mut result = max;
                let mut found = false;
                for bit in (start + 1) as u16..=max {
                    if (left.bit(m, bit) == right.bit(m, bit)) == m.system_bit(1088) {
                        result = bit;
                        found = true;
                        break;
                    }
                }
                m.set_system(1091, found);
                m.set_system(1089, result == max);
                write_scalar(pointer, m, result as i16);
            }
            Self::Read {
                source, pointer, ..
            }
            | Self::Write {
                source, pointer, ..
            } => {
                m.native_numeric_scratch = Some(n as i16);
                // MBRD snapshots groups before the pointer write; MBWR writes
                // physical bits directly and deliberately does not pad them.
                let source_table = Table::new(source, n, m);
                let mut index = read_scalar(pointer, m, 5);
                m.set_system(1089, false);
                if m.system_bit(1094) {
                    index = 0;
                    write_scalar(pointer, m, 0);
                }
                if index < 0 || index > max as i16 {
                    m.set_system(1092, true);
                    return;
                }
                m.native_numeric_scratch = Some(index);
                if index == max as i16 {
                    m.set_system(1089, true);
                } else if m.system_bit(1093) {
                    write_scalar(pointer, m, index + 1);
                }
                if matches!(self, Self::Read { .. }) {
                    let value = source_table.bit(m, index as u16);
                    m.set_system(1095, value);
                } else {
                    let value = m.system_bit(1096);
                    if let AdditionalValue::Group { base, .. } = source {
                        let d = Device {
                            area: base.area,
                            index: base.index + index as u16,
                        };
                        if d.area == Area::X {
                            m.set_input(d, value);
                        } else {
                            m.set_internal(d, value);
                        }
                    } else {
                        let mut raw = source_table;
                        raw.set_bit(m, index as u16, value);
                    }
                }
            }
            Self::Shift { source, dest, .. } | Self::Rotate { source, dest, .. } => {
                let source = Table::new(source, n, m);
                let mut dest = Table::new(dest, n, m);
                let right = m.system_bit(1097);
                let rotate = matches!(self, Self::Rotate { .. });
                if right {
                    let outgoing = source.bit(m, 0);
                    let insert = if rotate { outgoing } else { m.system_bit(1096) };
                    m.set_system(1095, outgoing);
                    scratch_byte(m, true, u8::from(insert));
                    for bit in 0..max {
                        let value = source.bit(m, bit + 1);
                        dest.set_bit(m, bit, value);
                    }
                    dest.set_bit(m, max, insert);
                } else {
                    let insert = if rotate { false } else { m.system_bit(1096) };
                    dest.set_bit(m, 0, insert);
                    for bit in 0..max {
                        let value = source.bit(m, bit);
                        dest.set_bit(m, bit + 1, value);
                    }
                    let outgoing = source.bit(m, max);
                    m.set_system(1095, outgoing);
                    if rotate && outgoing {
                        dest.set_bit(m, 0, true);
                    }
                }
                dest.commit(m);
            }
            Self::Count { source, dest, .. } => {
                let source = Table::new(source, n, m);
                let ones = (0..n)
                    .map(|row| source.read_word(m, row).count_ones() as u16)
                    .sum::<u16>();
                let value = if m.system_bit(1098) {
                    ones
                } else {
                    16 * n - ones
                };
                m.set_system(1099, value == 0);
                write_scalar(dest, m, value as i16);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d(s: &str) -> Device {
        s.parse().unwrap()
    }
    fn action(s: &str) -> MatrixAction {
        let mut words = s.split_whitespace();
        MatrixAction::parse(
            words.next().unwrap(),
            &words.map(str::to_string).collect::<Vec<_>>(),
        )
        .unwrap()
    }
    fn memory() -> Memory {
        let mut m = Memory::default();
        m.native_numeric_scratch = Some(0x1234);
        m
    }
    #[test]
    fn raw_counter_rows_cross_into_independent_low_and_high_halves() {
        let mut m = memory();
        m.set_word(d("C199"), 0x1234);
        m.set_double_word(d("C200"), 0x56789abc);
        action("MINV C199 D0 K3").execute(&mut m);
        assert_eq!(
            [
                m.word(d("D0")) as u16,
                m.word(d("D1")) as u16,
                m.word(d("D2")) as u16
            ],
            [!0x1234u16, !0x9abcu16, !0x5678u16]
        );
        action("MINV C199 C199 K3").execute(&mut m);
        assert_eq!(m.word(d("C199")) as u16, !0x1234u16);
        assert_eq!(m.double_word(d("C200")) as u32, !0x56789abcu32);
        assert!(action("MBC C255 K2 D10").validate_native_span(&m).is_ok());
        assert!(action("MBC C255 K3 D10").validate_native_span(&m).is_err());
        m.set_system(1098, true);
        action("MBC C200 K2 D10").execute(&mut m);
        assert_eq!(m.word(d("D10")), (!0x56789abcu32).count_ones() as i16);
    }
    #[test]
    fn natural_counter_rejections_preserve_exact_partial_decoder_scratch() {
        let mut m = memory();
        m.set_double_word(d("C200"), 0x12345678);
        m.set_word(d("D10"), 77);
        action("MINV C200 D10 K1").execute(&mut m);
        assert_eq!(m.native_numeric_scratch, Some(0x1234));
        assert_eq!(m.word(d("D10")), 77);
        let unsafe_action = action("MAND K4M0 C200 D10 K1");
        assert!(unsafe_action.validate_native_span(&m).is_err());
        unsafe_action.execute(&mut m);
        assert_eq!(m.native_numeric_scratch, Some(0x1234));
        assert!(!m.execution_limitations.is_empty());
        assert!(!m.system_bit(1067));
        action("MBRD D0 K1 C200").execute(&mut m);
        assert!(m.system_bit(1067));
        assert_eq!(m.word(d("D1067")), 0xe1a);
        assert_eq!(m.word(d("D1068")), 5);
        assert_eq!(m.double_word(d("C200")), 0x12345678);
    }
    #[test]
    fn parser_preserves_matrices_operand_order_and_ef_aliases() {
        for (name, len) in [
            ("MAND", 4),
            ("MOR", 4),
            ("MXOR", 4),
            ("MXNR", 4),
            ("MINV", 3),
            ("MCMP", 4),
            ("MBRD", 3),
            ("MBWR", 3),
            ("MBS", 3),
            ("MBR", 3),
            ("MBC", 3),
        ] {
            assert!(MatrixAction::parse(name, &vec!["D0".into(); len - 1]).is_err());
        }
        assert!(MatrixAction::parse("DMAND", &[]).is_err());
        for bad in [
            "MAND K1 D0 D1 K3",
            "MINV D0 K4X0 K1",
            "MBC E0 K1 D0",
            "MBC D0 T0 D1",
            "MBC D0 K1 K5M0",
        ] {
            let mut words = bad.split_whitespace();
            let name = words.next().unwrap();
            assert!(
                MatrixAction::parse(name, &words.map(str::to_string).collect::<Vec<_>>()).is_err()
            );
        }
        assert_eq!(action("MCMP D0 D10 K3 F1").label(), "MCMP D0 D10 K3 F1");
        let mut m = memory();
        m.set_system(1098, true);
        m.set_word(d("D0"), 3);
        action("MBC D0 K1 F1").execute(&mut m);
        assert_eq!(m.word(d("D1183")), 2);
    }
    #[test]
    fn raw_overlap_streams_but_groups_snapshot_and_compact() {
        let mut m = memory();
        m.set_word(d("D0"), 0x5555);
        m.set_word(d("D1"), 0x1234);
        action("MINV D0 D1 K2").execute(&mut m);
        assert_eq!(m.word(d("D1")) as u16, 0xaaaa);
        assert_eq!(m.word(d("D2")) as u16, 0x5555);
        for bit in [0, 3, 4, 6] {
            m.set_internal(
                Device {
                    area: Area::M,
                    index: bit,
                },
                true,
            );
        }
        action("MINV K1M0 K1M4 K2").execute(&mut m);
        assert_eq!(
            AdditionalValue::parse("K1M4", false, false)
                .unwrap()
                .read(&m),
            6
        );
        assert_eq!(
            AdditionalValue::parse("K1M8", false, false)
                .unwrap()
                .read(&m),
            10
        );
        assert_eq!(m.native_numeric_scratch, Some(0x1204));
    }
    #[test]
    fn read_pointer_write_precedes_raw_access_but_not_group_snapshot() {
        let mut m = memory();
        m.set_word(d("D0"), 0);
        m.set_system(1093, true);
        action("MBRD D0 K1 D0").execute(&mut m);
        assert_eq!(m.word(d("D0")), 1);
        assert!(m.system_bit(1095));
        m.set_word(d("D10"), 0);
        m.set_system(1095, true);
        action("MBRD K1M0 K1 D10").execute(&mut m);
        assert!(!m.system_bit(1095));
        assert_eq!(m.native_numeric_scratch, Some(0));
    }
    #[test]
    fn short_groups_read_padded_bits_but_write_physical_bits() {
        let mut m = memory();
        m.set_word(d("D0"), 8);
        m.set_input(d("X10"), true);
        action("MBRD K1X0 K1 D0").execute(&mut m);
        assert!(!m.system_bit(1095));
        m.set_system(1096, false);
        action("MBWR K1X0 K1 D0").execute(&mut m);
        assert!(!m.bit(d("X10")));
        action("MBC K1M0 K2 D10").execute(&mut m);
        assert_eq!(m.word(d("D10")), 32);
        m.set_system(1098, true);
        action("MBC K1M0 K2 D10").execute(&mut m);
        assert!(m.system_bit(1099));
        m.set_internal(d("M0"), true);
        action("MBC K1M0 K2 D10").execute(&mut m);
        assert!(!m.system_bit(1099));
    }
    #[test]
    fn compare_restart_end_found_and_sticky_error() {
        let mut m = memory();
        m.set_word(d("D0"), 0x20);
        m.set_word(d("D20"), 2);
        action("MCMP D0 D10 K1 D20").execute(&mut m);
        assert_eq!(m.word(d("D20")), 5);
        assert!(m.system_bit(1091));
        action("MCMP D0 D10 K1 D20").execute(&mut m);
        assert_eq!(m.word(d("D20")), 15);
        assert!(m.system_bit(1089));
        assert!(!m.system_bit(1091));
        m.set_word(d("D20"), -1);
        action("MCMP D0 D10 K1 D20").execute(&mut m);
        assert!(m.system_bit(1092));
        assert!(m.system_bit(1089));
        m.set_system(1090, true);
        action("MCMP D0 D10 K1 D20").execute(&mut m);
        assert_eq!(m.word(d("D20")), 5);
        assert!(m.system_bit(1090));
        assert!(m.system_bit(1092));
        assert_eq!(m.native_numeric_scratch, Some(15));
    }
    #[test]
    fn left_in_place_installed_streaming_quirk_and_right_scratch() {
        let mut m = memory();
        m.set_word(d("D0"), 0x5555);
        m.set_system(1096, true);
        action("MBS D0 D0 K1").execute(&mut m);
        assert_eq!(m.word(d("D0")), -1);
        assert!(m.system_bit(1095));
        action("MBR D0 D0 K1").execute(&mut m);
        assert_eq!(m.word(d("D0")), 0);
        assert!(!m.system_bit(1095));
        m.set_system(1097, true);
        action("MBS D0 D1 K1").execute(&mut m);
        assert_eq!(m.word(d("D1")) as u16, 0x8000);
        assert_eq!(m.native_numeric_scratch, Some(0x0134));
    }
    #[test]
    fn invalid_count_has_exact_error_offset_and_unsafe_native_preflight() {
        let mut m = memory();
        m.set_word(d("D20"), 257);
        m.set_word(d("D10"), 99);
        let a = action("MAND K1M0 D0 D10 D20");
        assert!(a.validate_native_span(&m).is_err());
        a.execute(&mut m);
        assert_eq!(m.word(d("D10")), 99);
        assert_eq!(m.word(d("D1067")), 0xe1a);
        assert_eq!(m.word(d("D1068")), 7);
        let mut m = memory();
        m.data_register_limit = Some(4999);
        let a = action("MINV D0 D4999 K2");
        assert!(a.validate_native_span(&m).is_err());
        a.execute(&mut m);
        assert!(!m.execution_limitations.is_empty());
        assert!(!m.system_bit(1067));
        assert!(action("MINV C199 D0 K2").validate_native_span(&m).is_ok());
    }
    #[test]
    fn binary_truth_tables_and_staged_control_alias_commit() {
        for (name, want) in [
            ("MAND", 0x1030u16),
            ("MOR", 0xf3ff),
            ("MXOR", 0xe3cf),
            ("MXNR", 0x1c30),
        ] {
            let mut m = memory();
            m.set_word(d("D0"), 0x1234);
            m.set_word(d("D1"), 0xf1fb_u16 as i16);
            action(&format!("{name} D0 D1 D2 K1")).execute(&mut m);
            assert_eq!(m.word(d("D2")) as u16, want);
            assert_eq!(m.native_numeric_scratch, Some(0x0034));
        }
        let mut m = memory();
        m.set_word(d("D0"), 0x40);
        MatrixAction::Shift {
            source: AdditionalValue::Word(Operand::Register(d("D0"))),
            dest: AdditionalValue::Group {
                base: Device {
                    area: Area::M,
                    index: 1088,
                },
                digits: 4,
                wide: false,
            },
            count: DataValue::Word(Operand::Constant(1)),
        }
        .execute(&mut m);
        // Helper first sets outgoing carry false; the delayed grouped output
        // then writes bit7=true over that very same M1095 carry relay.
        assert!(m.system_bit(1095));
    }
}
