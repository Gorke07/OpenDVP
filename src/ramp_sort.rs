//! Stateful ramping and column-major sorting.
use crate::data_processing::DataValue;
use crate::plc::{Area, Device, Memory, Operand, word_range};
use crate::wide::{WideOperand, double_devices};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Default)]
pub struct RampSortState {
    active_ramps: BTreeSet<usize>,
    sort: Option<SortJob>,
}

#[derive(Debug, Clone)]
struct SortJob {
    owner: usize,
    wide: bool,
    rows: u8,
    columns: u8,
    dest: Device,
    key: Device,
    progress: u8,
    one_dimensional: bool,
    elements: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RampSortAction {
    Ramp {
        wide: bool,
        start: Device,
        end: Device,
        dest: Device,
        count: DataValue,
    },
    Sort {
        wide: bool,
        source: Device,
        rows: u8,
        columns: u8,
        dest: Device,
        count: DataValue,
    },
}

fn data(raw: &str) -> Result<Device, String> {
    let d: Device = raw.parse()?;
    if d.area != Area::D {
        return Err("Operand D register olmalı".into());
    }
    Ok(d)
}
fn number(raw: &str, wide: bool) -> Result<DataValue, String> {
    let v = DataValue::parse(raw, wide)?;
    if matches!(&v, DataValue::Word(Operand::Register(d)) | DataValue::Double(WideOperand::Register(d)) if d.area != Area::D)
    {
        return Err("Sayı operandı K/H sabiti veya D register olmalı".into());
    }
    Ok(v)
}
fn constant(raw: &str, wide: bool, maximum: i32) -> Result<u8, String> {
    number(raw, wide)?
        .constant()
        .filter(|n| (1..=maximum).contains(n))
        .map(|n| n as u8)
        .ok_or_else(|| format!("Boyut K/H1..{maximum} sabiti olmalı"))
}
fn offset(d: Device, n: u16) -> Device {
    Device {
        area: d.area,
        index: d.index + n,
    }
}
fn width(wide: bool) -> u16 {
    if wide { 2 } else { 1 }
}
fn read(m: &Memory, d: Device, wide: bool) -> i32 {
    if wide {
        m.double_word(d)
    } else {
        i32::from(m.word(d))
    }
}
fn write(m: &mut Memory, d: Device, wide: bool, value: i32) {
    if wide {
        m.set_double_word(d, value);
    } else {
        m.set_word(d, value as i16);
    }
}
fn span_fits(d: Device, words: u16, limit: u16) -> bool {
    words == 0
        || d.index
            .checked_add(words - 1)
            .is_some_and(|end| end <= limit)
}

impl RampSortAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let wide = name.starts_with('D');
        let n = match name {
            "RAMP" | "DRAMP" => 4,
            "SORT" | "DSORT" => 5,
            _ => return Err("Bilinmeyen rampa/sıralama komutu".into()),
        };
        if args.len() != n {
            return Err(format!("{name}: {n} operand gerekli"));
        }
        Ok(match name {
            "RAMP" | "DRAMP" => {
                let start = data(&args[0])?;
                let end = data(&args[1])?;
                let dest = data(&args[2])?;
                if wide {
                    double_devices(start)?;
                    double_devices(end)?;
                }
                word_range(dest, width(wide) * 2)?;
                let count = number(&args[3], wide)?;
                if count.constant().is_some_and(|n| !(1..=32767).contains(&n)) {
                    return Err("RAMP sabit tarama sayısı 1..32767 olmalı".into());
                }
                Self::Ramp {
                    wide,
                    start,
                    end,
                    dest,
                    count,
                }
            }
            _ => {
                let source = data(&args[0])?;
                let rows = constant(&args[1], wide, 32)?;
                let columns = constant(&args[2], wide, 6)?;
                let dest = data(&args[3])?;
                let count = number(&args[4], wide)?;
                if rows != 1 || columns != 1 {
                    let words = u16::from(rows) * u16::from(columns) * width(wide);
                    word_range(source, words)?;
                    word_range(dest, words)?;
                }
                // A dynamic 1D length must be validated at execution. In
                // particular, a near-end valid length is not rejected using
                // an invented maximum-size32 span. Native length0 is accepted
                // as a bounded compatibility extension (no data, completionON).
                Self::Sort {
                    wide,
                    source,
                    rows,
                    columns,
                    dest,
                    count,
                }
            }
        })
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Ramp { wide: false, .. } => "RAMP",
            Self::Ramp { wide: true, .. } => "DRAMP",
            Self::Sort { wide: false, .. } => "SORT",
            Self::Sort { wide: true, .. } => "DSORT",
        }
    }
    pub fn wide(&self) -> bool {
        match self {
            Self::Ramp { wide, .. } | Self::Sort { wide, .. } => *wide,
        }
    }
    pub fn header(&self) -> u16 {
        (if matches!(self, Self::Ramp { .. }) {
            0x9600
        } else {
            0x9a00
        }) + if self.wide() { 0x100 } else { 0 }
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::Ramp { dest, .. } | Self::Sort { dest, .. } => *dest,
        }
    }
    pub fn label(&self) -> String {
        let name = self.name();
        match self {
            Self::Ramp {
                start,
                end,
                dest,
                count,
                ..
            } => format!("{name} {start} {end} {dest} {count}"),
            Self::Sort {
                source,
                rows,
                columns,
                dest,
                count,
                ..
            } => format!("{name} {source} K{rows} K{columns} {dest} {count}"),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        out.extend((1020..=1022).map(Memory::system_device));
        out.insert(Memory::system_device(1029));
        Memory::error_devices(out);
        match self {
            Self::Ramp {
                wide,
                start,
                end,
                dest,
                count,
            } => {
                for base in [start, end] {
                    out.extend(word_range(*base, width(*wide)).unwrap());
                }
                out.extend(word_range(*dest, width(*wide) * 2).unwrap());
                count.devices(out);
                out.insert(Memory::system_device(1026));
            }
            Self::Sort {
                wide,
                source,
                rows,
                columns,
                dest,
                count,
            } => {
                count.devices(out);
                out.extend([*source, *dest]);
                if *rows != 1 || *columns != 1 {
                    let words = u16::from(*rows) * u16::from(*columns) * width(*wide);
                    out.extend(word_range(*source, words).unwrap());
                    out.extend(word_range(*dest, words).unwrap());
                }
            }
        }
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        if let Self::Sort {
            wide,
            source,
            rows: 1,
            columns: 1,
            dest,
            count,
        } = self
        {
            let count = count.read(m) as u32;
            if count <= 32 {
                let words = count as u16 * width(*wide);
                let limit = m.data_register_limit.unwrap_or(9999);
                if span_fits(*source, words, limit) && span_fits(*dest, words, limit) && words != 0
                {
                    out.extend(word_range(*source, words).unwrap());
                    out.extend(word_range(*dest, words).unwrap());
                }
            }
        }
        // A completed 1D job keeps its captured span even if the count source
        // subsequently changes. Native parameter snapshots are not refreshed.
        if let Some(job) = &m.ramp_sort.sort {
            let words = job.elements * width(job.wide);
            if words != 0 && span_fits(job.dest, words, m.data_register_limit.unwrap_or(9999)) {
                out.extend(word_range(job.dest, words).unwrap());
            }
            if !job.wide
                && !job.one_dimensional
                && m.native_numeric_scratch
                    .is_some_and(|n| i16::from(job.progress) < n)
                && job.progress >= job.rows
            {
                let mut addresses = vec![offset(job.key, u16::from(job.progress))];
                addresses.extend((0..job.columns).map(|column| {
                    offset(
                        job.dest,
                        u16::from(column) * u16::from(job.rows) + u16::from(job.progress),
                    )
                }));
                out.extend(
                    addresses
                        .into_iter()
                        .filter(|d| d.index <= m.data_register_limit.unwrap_or(9999)),
                );
            }
        }
    }
    pub fn execute(&self, m: &mut Memory, power: bool, id: usize) {
        match self {
            Self::Ramp {
                wide,
                start,
                end,
                dest,
                count,
            } => {
                if !power {
                    if m.ramp_sort.active_ramps.remove(&id) {
                        m.set_system(1029, false);
                    }
                    return;
                }
                let initial = read(m, *start, *wide);
                let endpoint = read(m, *end, *wide);
                if !*wide {
                    //419FF8/41A187: the word start operand lives in shared
                    //scratch53A42E, also (incorrectly) consulted by SORT.
                    m.native_numeric_scratch = Some(initial as i16);
                }
                let counter = offset(*dest, width(*wide));
                if !m.ramp_sort.active_ramps.contains(&id) {
                    m.ramp_sort.active_ramps.insert(id);
                    write(m, *dest, *wide, initial);
                    write(m, counter, *wide, 0);
                    m.set_system(1029, false);
                    return;
                }
                let count = count.read(m);
                let elapsed = read(m, counter, *wide);
                if elapsed >= count {
                    if !m.system_bit(1026) && m.system_bit(1029) {
                        write(m, *dest, *wide, initial);
                        write(m, counter, *wide, 0);
                        m.set_system(1029, false);
                    } else {
                        m.set_system(1029, true);
                        write(m, *dest, *wide, endpoint);
                        write(m, counter, *wide, count);
                    }
                    return;
                }
                let elapsed = if *wide {
                    elapsed.wrapping_add(1)
                } else {
                    i32::from((elapsed as i16).wrapping_add(1))
                };
                let float = interpolate(initial, endpoint, elapsed, count);
                let raw = truncate_native(float) as i32;
                let candidate = if *wide { raw } else { i32::from(raw as i16) };
                let crossed = if endpoint >= initial {
                    candidate > endpoint
                } else {
                    candidate < endpoint
                };
                let result = if crossed || elapsed == count {
                    endpoint
                } else {
                    candidate
                };
                write(m, *dest, *wide, result);
                write(m, counter, *wide, elapsed);
            }
            Self::Sort {
                wide,
                source,
                rows,
                columns,
                dest,
                count,
            } => {
                if !power {
                    if m.ramp_sort.sort.as_ref().is_some_and(|job| job.owner == id) {
                        m.ramp_sort.sort = None;
                    }
                    return;
                }
                if m.ramp_sort.sort.as_ref().is_some_and(|job| job.owner != id) {
                    return;
                }
                if !*wide && (*rows != 1 || *columns != 1) && m.native_numeric_scratch.is_none() {
                    m.execution_limitations.insert("SORT shared scratch must be initialized by program loading before direct execution".into());
                    return;
                }
                let mut job = if let Some(job) = m.ramp_sort.sort.take() {
                    job
                } else {
                    let n = count.read(m) as u32;
                    let one_dimensional = *rows == 1 && *columns == 1;
                    let elements = if one_dimensional {
                        n as u16
                    } else {
                        u16::from(*rows) * u16::from(*columns)
                    };
                    if (one_dimensional
                        && (n > 32
                            || !span_fits(*source, elements * width(*wide), 9999)
                            || !span_fits(*dest, elements * width(*wide), 9999)))
                        || (!one_dimensional && (n == 0 || n > u32::from(*columns)))
                    {
                        m.operation_error(0xe1a, 0);
                        return;
                    }
                    let words = elements * width(*wide);
                    let limit = m.data_register_limit.unwrap_or(9999);
                    if !span_fits(*source, words, limit) || !span_fits(*dest, words, limit) {
                        m.execution_limitations.insert(
                            "SORT logical model boundary beyond the verified raw native span"
                                .into(),
                        );
                        return;
                    }
                    // Deliberately forward, preserving native overlapping-copy
                    // effects. This is not a snapshot or memmove.
                    for index in 0..words {
                        // DSORT also writes each low half before loading its
                        // high half. A one-word overlapping shift therefore
                        // propagates the just-written word through the copy.
                        let value = m.word(offset(*source, index));
                        m.set_word(offset(*dest, index), value);
                    }
                    if one_dimensional {
                        for _ in 0..elements {
                            for index in 0..elements.saturating_sub(1) {
                                let a = offset(*dest, index * width(*wide));
                                let b = offset(a, width(*wide));
                                let left = read(m, a, *wide);
                                let right = read(m, b, *wide);
                                if left > right {
                                    write(m, a, *wide, right);
                                    write(m, b, *wide, left);
                                }
                            }
                        }
                        m.set_system(1029, true);
                    } else {
                        m.set_system(1029, false);
                    }
                    SortJob {
                        owner: id,
                        wide: *wide,
                        rows: *rows,
                        columns: *columns,
                        dest: *dest,
                        key: offset(
                            *dest,
                            if one_dimensional {
                                0
                            } else {
                                (n as u16 - 1) * u16::from(*rows) * width(*wide)
                            },
                        ),
                        progress: 0,
                        one_dimensional,
                        elements,
                    }
                };
                let threshold = if job.wide {
                    i32::from(job.rows)
                } else {
                    i32::from(m.native_numeric_scratch.unwrap_or(0))
                };
                if !job.one_dimensional && i32::from(job.progress) < threshold {
                    //41A87C reads shared53A42E instead of m1. SER/RAMP/DTM
                    //can throttle or stop a word SORT between row steps.
                    //Past the row count, native selects only the same row
                    //and performs idempotent self-swaps, then increments its
                    //BYTE progress. Preserve that state and eventual wrap.
                    if job.progress >= job.rows {
                        let stride = width(job.wide);
                        let limit = m.data_register_limit.unwrap_or(9999);
                        let key = offset(job.key, u16::from(job.progress) * stride);
                        let last = offset(
                            job.dest,
                            (u16::from(job.columns - 1) * u16::from(job.rows)
                                + u16::from(job.progress))
                                * stride,
                        );
                        if !span_fits(key, stride, limit) || !span_fits(last, stride, limit) {
                            m.execution_limitations.insert("SORT shared scratch allowed a post-completion raw read beyond the logical model boundary".into());
                            m.ramp_sort.sort = Some(job);
                            return;
                        }
                        job.progress = job.progress.wrapping_add(1);
                        if job.progress >= job.rows {
                            m.set_system(1029, true);
                        }
                        m.ramp_sort.sort = Some(job);
                        return;
                    }
                    let stride = width(job.wide);
                    let mut best = job.progress;
                    let mut best_value =
                        read(m, offset(job.key, u16::from(best) * stride), job.wide);
                    for candidate in job.progress..job.rows {
                        let value =
                            read(m, offset(job.key, u16::from(candidate) * stride), job.wide);
                        if value < best_value {
                            best = candidate;
                            best_value = value;
                        }
                    }
                    for column in 0..job.columns {
                        let base = u16::from(column) * u16::from(job.rows);
                        let a = offset(job.dest, (base + u16::from(job.progress)) * stride);
                        let b = offset(job.dest, (base + u16::from(best)) * stride);
                        let temporary = read(m, b, job.wide);
                        let value = read(m, a, job.wide);
                        write(m, b, job.wide, value);
                        write(m, a, job.wide, temporary);
                    }
                    job.progress = job.progress.wrapping_add(1);
                    if job.progress >= job.rows {
                        m.set_system(1029, true);
                    }
                }
                m.ramp_sort.sort = Some(job);
            }
        }
    }
}

fn truncate_native(value: f32) -> i64 {
    let value = f64::from(value);
    if !value.is_finite()
        || !(-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&value)
    {
        i64::MIN
    } else {
        value.trunc() as i64
    }
}

fn interpolate(start: i32, end: i32, elapsed: i32, count: i32) -> f32 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    let _control = NativeControl::enter();
    let first = start as f32;
    let last = end as f32;
    let ascending = end >= start;
    let difference = if ascending {
        last - first
    } else {
        first - last
    };
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let mut step = 0f32;
        let mut result = 0f32;
        unsafe {
            std::arch::asm!("fild dword ptr [{count}]", "fdivr dword ptr [{difference}]", "fstp dword ptr [{step}]",
                count = in(reg) &count, difference = in(reg) &difference, step = in(reg) &mut step, options(nostack));
            if ascending {
                std::arch::asm!("fild dword ptr [{elapsed}]", "fmul dword ptr [{step}]", "fadd dword ptr [{first}]", "fstp dword ptr [{out}]",
                    elapsed = in(reg) &elapsed, step = in(reg) &step, first = in(reg) &first, out = in(reg) &mut result, options(nostack));
            } else {
                std::arch::asm!("fild dword ptr [{elapsed}]", "fmul dword ptr [{step}]", "fsubr dword ptr [{first}]", "fstp dword ptr [{out}]",
                    elapsed = in(reg) &elapsed, step = in(reg) &step, first = in(reg) &first, out = in(reg) &mut result, options(nostack));
            }
        }
        result
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        let step = (f64::from(difference) / f64::from(count)) as f32;
        let movement = f64::from(elapsed) * f64::from(step);
        (if ascending {
            f64::from(first) + movement
        } else {
            f64::from(first) - movement
        }) as f32
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
        let scoped = (original & !0x0f00) | 0x0300;
        unsafe {
            std::arch::asm!("fldcw word ptr [{word}]", word = in(reg) &scoped, options(nostack, preserves_flags));
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

#[cfg(test)]
mod tests {
    use super::*;
    fn d(raw: &str) -> Device {
        raw.parse().unwrap()
    }
    fn action(raw: &str) -> RampSortAction {
        let mut tokens = raw.split_whitespace();
        RampSortAction::parse(
            tokens.next().unwrap(),
            &tokens.map(str::to_owned).collect::<Vec<_>>(),
        )
        .unwrap()
    }
    fn shared_scratch_fixture(predecessor: &str) -> crate::plc::Simulator {
        use crate::plc::{Simulator, compile};
        // Same retained source ordering as the three initial native mismatches,
        // including the following MOV marker that must still execute.
        let source = format!(
            "LD X1\nMOV K30 D0\nMOV K10 D1\nMOV K20 D2\nMOV K3 D3\nMOV K1 D4\nMOV K2 D5\nMOV K-777 D100\nMOV K-777 D101\nMOV K-777 D102\nMOV K-777 D103\nMOV K-777 D104\nMOV K-777 D105\nSET M1020\nSET M1021\nSET M1022\nRST M1029\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\nMOV K2 D200\nMOV K9 D201\nMOV K65 D202\nLD X0\n{predecessor}\nOUT Y1\nLD X0\nSORT D0 K3 K2 D100 K2\nOUT Y0\nLD X3\nMOV K1234 D300\nEND"
        );
        let program = compile(&source).unwrap();
        let mut simulator = Simulator::default();
        simulator.memory.set_input(d("X1"), true);
        simulator.memory.set_input(d("X3"), true);
        simulator.scan(&program, 100);
        simulator.memory.set_input(d("X1"), false);
        simulator.memory.set_input(d("X0"), true);
        for _ in 0..6 {
            simulator.scan(&program, 100);
            assert_eq!(simulator.memory.word(d("D300")), 1234);
        }
        assert!(simulator.memory.execution_limitations.is_empty());
        simulator
    }
    #[test]
    fn shared_zero_ser_scratch_prevents_every_word_sort_row_step() {
        let simulator = shared_scratch_fixture("SER D200 K0 D220 K1");
        assert_eq!(
            (100..106)
                .map(|i| simulator.memory.word(offset(d("D0"), i)))
                .collect::<Vec<_>>(),
            [30, 10, 20, 3, 1, 2]
        );
        assert!(!simulator.memory.system_bit(1029));
    }
    #[test]
    fn packed_lrc_invalid_zero_preserves_the_native_unsorted_table() {
        // Both unchanged current simulators retain this unsorted table. LRC
        // publishes the zero raw WORD before rejecting its ASCII characters.
        let simulator = shared_scratch_fixture("LRC D210 K2 D220");
        assert_eq!(simulator.memory.native_numeric_scratch, Some(0));
        assert_eq!(
            (100..106)
                .map(|i| simulator.memory.word(d(&format!("D{i}"))))
                .collect::<Vec<_>>(),
            [30, 10, 20, 3, 1, 2]
        );
        assert!(!simulator.memory.system_bit(1029));
        assert_eq!(simulator.memory.word(d("D1067")) as u16, 0xe1a);
        assert!(simulator.memory.system_bit(1067) && simulator.memory.system_bit(1068));
    }
    #[test]
    fn shared_one_ser_scratch_allows_only_the_first_word_sort_row() {
        let simulator = shared_scratch_fixture("SER D200 K1 D220 K1");
        assert_eq!(
            (100..106)
                .map(|i| simulator.memory.word(offset(d("D0"), i)))
                .collect::<Vec<_>>(),
            [10, 30, 20, 1, 3, 2]
        );
        assert!(!simulator.memory.system_bit(1029));
    }
    #[test]
    fn shared_ramp_start_scratch_preserves_ramp_completion_cadence() {
        use crate::plc::{Simulator, compile};
        let program = compile("LD X1\nMOV K2 D200\nMOV K9 D201\nMOV K30 D0\nMOV K10 D1\nMOV K20 D2\nMOV K3 D3\nMOV K1 D4\nMOV K2 D5\nLD X0\nRAMP D200 D201 D220 K2\nOUT Y1\nLD X0\nSORT D0 K3 K2 D100 K2\nOUT Y0\nLD X3\nMOV K1234 D300\nEND").unwrap();
        let mut simulator = Simulator::default();
        simulator.memory.set_input(d("X1"), true);
        simulator.memory.set_input(d("X3"), true);
        simulator.scan(&program, 100);
        simulator.memory.set_input(d("X1"), false);
        simulator.memory.set_input(d("X0"), true);
        for (value, done) in [
            (2, false),
            (5, false),
            (9, false),
            (9, true),
            (2, false),
            (5, false),
        ] {
            simulator.scan(&program, 100);
            assert_eq!(simulator.memory.word(d("D220")), value);
            assert_eq!(simulator.memory.system_bit(1029), done);
            assert_eq!(simulator.memory.word(d("D300")), 1234);
        }
        assert_eq!(
            (100..106)
                .map(|i| simulator.memory.word(offset(d("D0"), i)))
                .collect::<Vec<_>>(),
            [10, 20, 30, 1, 2, 3]
        );
    }
    #[test]
    fn ramp_completion_happens_after_the_interpolated_endpoint_then_restarts() {
        let mut m = Memory::default();
        m.set_word(d("D0"), 10);
        m.set_word(d("D1"), 20);
        let ramp = action("RAMP D0 D1 D10 K2");
        for (value, elapsed, done) in [
            (10, 0, false),
            (15, 1, false),
            (20, 2, false),
            (20, 2, true),
            (10, 0, false),
        ] {
            ramp.execute(&mut m, true, 3);
            assert_eq!(m.word(d("D10")), value);
            assert_eq!(m.word(d("D11")), elapsed);
            assert_eq!(m.system_bit(1029), done);
        }
    }
    #[test]
    fn held_mode_and_false_power_preserve_devices_but_rearm_occurrences() {
        let mut m = Memory::default();
        m.set_system(1026, true);
        m.set_word(d("D0"), 12);
        m.set_word(d("D1"), -12);
        let ramp = action("RAMP D0 D1 D10 K1");
        for _ in 0..4 {
            ramp.execute(&mut m, true, 1);
        }
        assert_eq!(m.word(d("D10")), -12);
        assert!(m.system_bit(1029));
        ramp.execute(&mut m, false, 99);
        assert!(m.system_bit(1029));
        ramp.execute(&mut m, false, 1);
        assert!(!m.system_bit(1029));
        assert_eq!(m.word(d("D10")), -12);
        assert_eq!(m.word(d("D11")), 1);
        ramp.execute(&mut m, true, 1);
        assert_eq!(m.word(d("D10")), 12);
        assert_eq!(m.word(d("D11")), 0);
    }
    #[test]
    fn double_ramp_uses_a_separate_counter_pair_and_live_endpoints() {
        let mut m = Memory::default();
        m.set_double_word(d("D0"), 100_000);
        m.set_double_word(d("D2"), 100_100);
        let ramp = action("DRAMP D0 D2 D10 K4");
        ramp.execute(&mut m, true, 1);
        ramp.execute(&mut m, true, 1);
        assert_eq!(m.double_word(d("D10")), 100_025);
        assert_eq!(m.double_word(d("D12")), 1);
        m.set_double_word(d("D2"), 100_200);
        ramp.execute(&mut m, true, 1);
        assert_eq!(m.double_word(d("D10")), 100_100);
        assert_eq!(m.double_word(d("D12")), 2);
    }
    #[test]
    fn dynamic_invalid_ramp_count_is_native_completion_not_an_invented_error() {
        let mut m = Memory::default();
        m.set_word(d("D0"), 10);
        m.set_word(d("D1"), 30);
        m.set_word(d("D5"), -1);
        let ramp = action("RAMP D0 D1 D10 D5");
        ramp.execute(&mut m, true, 1);
        assert_eq!(m.word(d("D10")), 10);
        ramp.execute(&mut m, true, 1);
        assert_eq!(m.word(d("D10")), 30);
        assert_eq!(m.word(d("D11")), -1);
        assert!(m.system_bit(1029));
        assert!(!m.system_bit(1067));
    }
    #[test]
    fn one_dimensional_sort_captures_parameters_and_preserves_forward_copy_aliasing() {
        let mut m = Memory::default();
        for (i, v) in [3, -1, 2].into_iter().enumerate() {
            m.set_word(offset(d("D0"), i as u16), v);
        }
        let sort = action("SORT D0 K1 K1 D1 K3");
        sort.execute(&mut m, true, 1);
        assert_eq!(
            (0..3)
                .map(|i| m.word(offset(d("D1"), i)))
                .collect::<Vec<_>>(),
            vec![3, 3, 3]
        );
        m.set_word(d("D1"), 9);
        sort.execute(&mut m, true, 1);
        assert_eq!(m.word(d("D1")), 9);
        sort.execute(&mut m, false, 1);
        assert!(m.system_bit(1029));
    }
    #[test]
    fn double_sort_copy_preserves_low_before_high_word_alias_order() {
        let mut m = Memory::default();
        for (index, value) in [1, 2, 3, 4].into_iter().enumerate() {
            m.set_word(offset(d("D0"), index as u16), value);
        }
        action("DSORT D0 K1 K1 D1 K2").execute(&mut m, true, 1);
        assert_eq!(m.double_word(d("D1")), 0x0001_0001);
        assert_eq!(m.double_word(d("D3")), 0x0001_0001);
    }
    #[test]
    fn double_column_sort_moves_rows_across_columns_one_row_per_call() {
        let mut m = Memory::default();
        for (i, v) in [30, 10, 20, 3, 1, 2].into_iter().enumerate() {
            m.set_double_word(offset(d("D0"), i as u16 * 2), v);
        }
        let sort = action("DSORT D0 K3 K2 D20 K2");
        sort.execute(&mut m, true, 1);
        assert_eq!(m.double_word(d("D20")), 10);
        assert!(!m.system_bit(1029));
        sort.execute(&mut m, true, 1);
        assert_eq!(m.double_word(d("D22")), 20);
        assert!(!m.system_bit(1029));
        sort.execute(&mut m, true, 1);
        assert!(m.system_bit(1029));
        assert_eq!(
            (0..6)
                .map(|i| m.double_word(offset(d("D20"), i * 2)))
                .collect::<Vec<_>>(),
            vec![10, 20, 30, 1, 2, 3]
        );
    }
    #[test]
    fn sorting_owner_is_shared_and_only_its_false_call_releases_it() {
        let mut m = Memory::default();
        m.set_double_word(d("D0"), 5);
        m.set_double_word(d("D2"), 2);
        let a = action("DSORT D0 K1 K1 D10 K2");
        let b = action("SORT D0 K1 K1 D20 K1");
        a.execute(&mut m, true, 1);
        b.execute(&mut m, true, 2);
        assert_eq!(m.word(d("D20")), 0);
        b.execute(&mut m, false, 2);
        b.execute(&mut m, true, 2);
        assert_eq!(m.word(d("D20")), 0);
        a.execute(&mut m, false, 1);
        b.execute(&mut m, true, 2);
        assert_eq!(m.word(d("D20")), 5);
    }
    #[test]
    fn one_dimensional_dynamic_length_uses_actual_span_and_invalid_count_holds_data() {
        let mut m = Memory::default();
        m.set_word(d("D9999"), 7);
        m.set_word(d("D5"), 1);
        let valid = action("SORT D9999 K1 K1 D9998 D5");
        valid.execute(&mut m, true, 1);
        assert_eq!(m.word(d("D9998")), 7);
        valid.execute(&mut m, false, 1);
        m.set_word(d("D5"), 2);
        valid.execute(&mut m, true, 1);
        assert_eq!(m.word(d("D9998")), 7);
        assert_eq!(m.word(d("D1067")), 0xe1a);
        let empty = action("SORT D0 K1 K1 D10 K0");
        let mut m = Memory::default();
        empty.execute(&mut m, true, 1);
        assert!(m.system_bit(1029));
        assert!(!m.system_bit(1067));
    }
    #[test]
    fn completion_relay_reset_distinguishes_word_and_double_owned_jobs() {
        for wide in [false, true] {
            let mut m = Memory::default();
            m.native_numeric_scratch = Some(300);
            let stride = if wide { 2 } else { 1 };
            for (index, value) in [30, 10, 20, 3, 1, 2].into_iter().enumerate() {
                write(&mut m, offset(d("D0"), index as u16 * stride), wide, value);
            }
            let sort = action(if wide {
                "DSORT D0 K3 K2 D100 K2"
            } else {
                "SORT D0 K3 K2 D100 K2"
            });
            for _ in 0..4 {
                sort.execute(&mut m, true, 1);
            }
            assert!(m.system_bit(1029));
            m.set_system(1029, false);
            sort.execute(&mut m, true, 1);
            assert_eq!(m.system_bit(1029), !wide);
        }
    }
    #[test]
    fn word_sort_byte_progress_wrap_restarts_selection_after_the_256th_call() {
        let mut m = Memory::default();
        m.native_numeric_scratch = Some(300);
        for (index, value) in [30, 10, 20, 3, 1, 2].into_iter().enumerate() {
            m.set_word(offset(d("D0"), index as u16), value);
        }
        let sort = action("SORT D0 K3 K2 D100 K2");
        for _ in 0..255 {
            sort.execute(&mut m, true, 1);
        }
        for (index, value) in [30, 10, 20, 3, 1, 2].into_iter().enumerate() {
            m.set_word(offset(d("D100"), index as u16), value);
        }
        m.set_system(1029, false);
        sort.execute(&mut m, true, 1);
        assert_eq!(
            (0..6)
                .map(|i| m.word(offset(d("D100"), i)))
                .collect::<Vec<_>>(),
            [30, 10, 20, 3, 1, 2]
        );
        assert!(!m.system_bit(1029));
        sort.execute(&mut m, true, 1);
        assert_eq!(
            (0..6)
                .map(|i| m.word(offset(d("D100"), i)))
                .collect::<Vec<_>>(),
            [10, 30, 20, 1, 3, 2]
        );
        assert!(!m.system_bit(1029));
        sort.execute(&mut m, true, 1);
        sort.execute(&mut m, true, 1);
        assert_eq!(
            (0..6)
                .map(|i| m.word(offset(d("D100"), i)))
                .collect::<Vec<_>>(),
            [10, 20, 30, 1, 2, 3]
        );
        assert!(m.system_bit(1029));
        assert!(m.execution_limitations.is_empty());
    }
}
