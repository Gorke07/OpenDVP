//! DTM data conversions, with explicit services for identity and networking.
//! The installed legacy selector table and remaining qualifications are in
use crate::data_processing::DataValue;
use crate::plc::{Area, Device, Memory, Operand};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub struct TransformAction {
    pub source: Device,
    pub dest: Device,
    pub mode: DataValue,
    pub count: DataValue,
}

#[derive(Debug, Clone)]
pub struct SolarRequest {
    pub longitude: f32,
    pub latitude: f32,
    pub timezone_hours: i16,
    pub daylight_saving: i16,
    /// Original year/month/day words, without century conversion. An explicit
    /// callback may interpret these independently of the built-in ES2 helper.
    pub date: [i16; 3],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolarTimes {
    pub sunrise: [i16; 3],
    pub sunset: [i16; 3],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NtpRequest {
    pub server: [u8; 4],
    pub update_minutes: u16,
    pub timezone_hours: i16,
    pub timezone_minutes: i16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i16)]
pub enum NtpStatus {
    Successful = 0,
    InProgress = 1,
    Failed = 2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NtpReply {
    pub status: NtpStatus,
    pub unix_seconds: Option<i64>,
}

pub type SolarBackend = fn(&SolarRequest) -> Result<SolarTimes, String>;
pub type NtpBackend = fn(&NtpRequest) -> Result<NtpReply, String>;

/// Optional virtual-device services. Solar calculations have a deterministic
/// built-in implementation; this callback remains an explicit override.
#[derive(Debug, Clone, Default)]
pub struct TransformContext {
    pub serial_number: Option<Vec<u8>>,
    pub solar_backend: Option<SolarBackend>,
    pub ntp_backend: Option<NtpBackend>,
    pub last_ntp_request: Option<NtpRequest>,
    pub last_ntp_reply: Option<NtpReply>,
    /// Virtual clock state returned by the selected backend. Wiring this into
    /// future RTC instructions is an explicit remaining integration facet.
    pub synchronized_unix_seconds: Option<i64>,
}

/// Capabilities verified for the exact installed images selected by the bridge.
/// This does not identify a physical PLC firmware generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeTransformProfile {
    #[default]
    InstalledLegacy,
    CurrentEs2,
}
impl NativeTransformProfile {
    pub fn supports(self, mode: i32) -> bool {
        let mode = mode as u8;
        if self == Self::CurrentEs2 && matches!(mode, 15 | 30..=36) {
            return true;
        }
        // Selector 10 has an undocumented legacy algorithm. Selector 15 and
        // the queue/reverse selectors are verified only for current ES2.
        !matches!(mode, 10 | 15 | 30..=36 | 39 | 52)
    }
}

/// Backwards-compatible legacy capability query.
pub fn supports_installed_mode(mode: i32) -> bool {
    NativeTransformProfile::InstalledLegacy.supports(mode)
}

fn d(index: u16) -> Device {
    Device {
        area: Area::D,
        index,
    }
}

fn numeric(raw: &str) -> Result<DataValue, String> {
    let value = DataValue::parse(raw, false)?;
    if matches!(&value, DataValue::Word(Operand::Register(device)) if device.area != Area::D) {
        return Err("DTM sayısal operandı K/H sabiti veya D register olmalı".into());
    }
    Ok(value)
}

impl TransformAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if !matches!(name, "DTM" | "DTMP") || args.len() != 4 {
            return Err("DTM S D m n: dört operand gerekli".into());
        }
        let source: Device = args[0].parse()?;
        let dest: Device = args[1].parse()?;
        if source.area != Area::D || dest.area != Area::D {
            return Err("DTM kaynak ve hedefi D register olmalı".into());
        }
        Ok(Self {
            source,
            dest,
            mode: numeric(&args[2])?,
            count: numeric(&args[3])?,
        })
    }

    pub fn name(&self) -> &'static str {
        "DTM"
    }
    pub fn header(&self) -> u16 {
        0x9800
    }
    pub fn dest(&self) -> Device {
        self.dest
    }
    pub fn label(&self) -> String {
        format!(
            "DTM {} {} {} {}",
            self.source, self.dest, self.mode, self.count
        )
    }

    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        out.extend([self.source, self.dest]);
        self.mode.devices(out);
        self.count.devices(out);
        // Implicit spans depend on model limits, actual mode/count and string
        // terminators. Track them at execution, not as an unbounded worst-case
        // span that would reject valid terminated strings near an SS2 endpoint.
    }

    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        let mode = self.mode.read(memory) as u8;
        let n = self.count.read(memory).clamp(1, 256) as u16;
        self.footprint(mode, n, memory.data_register_limit.unwrap_or(9999), out);
        if matches!(mode, 31..=36) {
            // Replay on a private image to include live alias-derived targets,
            // notably mode 33's second header read and mode 36's decremented
            // pointer when D aliases S+1. No application memory is changed.
            let mut snapshot = memory.clone();
            let _ = self.walk_queue(&mut snapshot, mode, n, out);
        }
    }

    /// Validate the actual legacy pointer walk before running the original.
    /// The caller supplies a snapshot of the native mode/count and the bounded
    /// ranges returned by observed_devices(), at this instruction's PC.
    pub fn validate_installed_execution(&self, memory: &Memory) -> Result<(), String> {
        self.validate_for_profile(memory, NativeTransformProfile::InstalledLegacy)
    }

    /// Validate every access of the selected installed implementation before
    /// native execution. Alias-dependent queue and solar walks use a private image.
    pub fn validate_for_profile(
        &self,
        memory: &Memory,
        profile: NativeTransformProfile,
    ) -> Result<(), String> {
        let mode = self.mode.read(memory) as u8;
        if !profile.supports(i32::from(mode)) {
            return Err(format!(
                "DTM mode {mode} has no matching implemented {profile:?} semantics"
            ));
        }
        let count = self.count.read(memory).clamp(1, 256) as u16;
        let limit = memory.data_register_limit.unwrap_or(9999);
        let check = |start: u16, count: u16| {
            if fits(start, count, limit) {
                Ok(())
            } else {
                Err(format!(
                    "DTM mode {mode} native access D{start}..D{} exceeds model D{limit}",
                    start.saturating_add(count.saturating_sub(1))
                ))
            }
        };
        let source = self.source.index;
        let dest = self.dest.index;
        match mode {
            0..=7 => {
                let mut source = source;
                let mut dest = dest;
                for index in 0..count {
                    check(source, 1)?;
                    check(dest, 1)?;
                    match mode {
                        0 | 1 | 6 | 7 => {
                            if index & 1 != 0 {
                                dest += 1;
                            }
                        }
                        2..=5 => {
                            dest += 1;
                            if dest >= 10000 {
                                return Ok(());
                            }
                            check(dest, 1)?;
                            dest += 1;
                        }
                        _ => unreachable!(),
                    }
                    source += 1;
                    if source >= 10000 || dest >= 10000 || (mode == 3 && dest >= 9999) {
                        return Ok(());
                    }
                }
            }
            8 => {
                if source >= 9998 || dest >= 9997 {
                    return Ok(());
                }
                check(source, 11)?;
                check(dest, 4)?;
            }
            9 => {
                if source > 9997 || dest > 9997 {
                    return Ok(());
                }
                check(source, 5)?;
                check(dest, 3)?;
            }
            11 => {
                if source > 9991 || dest > 9994 {
                    return Ok(());
                }
                check(source, 9)?;
                check(dest, 6)?;
            }
            12..=14 => {
                let width = if mode == 12 { 1 } else { 2 };
                check(source, width)?; // Native reads input before validating n.
                if !(2..=50).contains(&count) {
                    return Ok(());
                }
                let input = if mode == 12 {
                    i32::from(word(memory, source))
                } else {
                    read_double(memory, source) as i32
                };
                for index in 1..count {
                    let s = source + index * width;
                    let target = dest + index * width;
                    check(s, width * 2)?;
                    let (lower, upper, in_segment) = if mode == 14 {
                        let value = f32::from_bits(input as u32);
                        let low = f32::from_bits(read_double(memory, s));
                        let high = f32::from_bits(read_double(memory, s + width));
                        if low >= high {
                            return Ok(());
                        }
                        let lower = value <= low || value.is_nan() || low.is_nan();
                        (
                            lower,
                            value > high,
                            !lower && (value <= high || high.is_nan()),
                        )
                    } else {
                        let low = if width == 1 {
                            i32::from(word(memory, s))
                        } else {
                            read_double(memory, s) as i32
                        };
                        let high = if width == 1 {
                            i32::from(word(memory, s + width))
                        } else {
                            read_double(memory, s + width) as i32
                        };
                        if low >= high {
                            return Ok(());
                        }
                        (input <= low, input > high, input > low && input <= high)
                    };
                    if lower || in_segment || (upper && index == count - 1) {
                        check(dest, width)?;
                        if lower {
                            check(target, width)?;
                        } else if in_segment {
                            check(target, width * 2)?;
                        } else {
                            check(target + width, width)?;
                        }
                        return Ok(());
                    }
                }
            }
            16 | 17 => {
                let length = count.min(10000 - source).min(10000 - dest);
                let mut snapshot = memory.clone();
                let mut offset = 0;
                if mode == 16 {
                    while offset < length {
                        check(dest + offset, 1)?;
                        if byte(&snapshot, dest + offset) == 0 {
                            break;
                        }
                        offset += 1;
                    }
                    let mut s = source;
                    while offset < length {
                        check(s, 1)?;
                        check(dest + offset, 1)?;
                        let value = byte(&snapshot, s);
                        set_byte(&mut snapshot, dest + offset, false, value);
                        if byte(&snapshot, s) == 0 {
                            return Ok(());
                        }
                        offset += 1;
                        s += 1;
                    }
                } else {
                    while offset < length {
                        check(source + offset, 1)?;
                        if byte(&snapshot, source + offset) == 0 {
                            break;
                        }
                        check(dest + offset, 1)?;
                        let value = byte(&snapshot, source + offset);
                        set_byte(&mut snapshot, dest + offset, false, value);
                        offset += 1;
                    }
                    check(dest + offset, 1)?; // The terminator is a real write.
                }
            }
            15 => {
                if memory.transform_context.solar_backend.is_some() {
                    return Err("DTM m=15 native replay requires the built-in solar calculation, not an injected callback".into());
                }
                let mut snapshot = memory.clone();
                self.walk_solar_builtin(&mut snapshot)?;
            }
            18 => {
                check(dest, 2)?;
                let mut remaining = count.min(8).min(10000 - source);
                let mut next = source;
                while remaining != 0 {
                    check(next, 1)?;
                    let value = byte(memory, next);
                    next += 1;
                    if value == b'.' {
                        break;
                    }
                    remaining -= 1;
                }
                while remaining != 0 {
                    check(next, 1)?;
                    let value = byte(memory, next);
                    next += 1;
                    if !value.is_ascii_digit() {
                        break;
                    }
                    remaining -= 1;
                }
            }
            19 => {
                if source > 9998 {
                    return Ok(());
                }
                check(source, 2)?;
                let value = f32::from_bits(read_double(memory, source));
                if value >= 100_000_000.0 {
                    check(dest, 1)?;
                } else {
                    let digits = i32::from(count)
                        .min(10000 - i32::from(dest) - 2)
                        .clamp(0, 8) as u16;
                    check(dest, digits + 2)?;
                }
            }
            30 => {
                let length = count.min(10000 - source).min(10000 - dest);
                check(source, length)?;
                check(dest, length)?;
            }
            31..=36 => {
                let mut snapshot = memory.clone();
                self.walk_queue(&mut snapshot, mode, count, &mut BTreeSet::new())?;
            }
            _ => {}
        }
        Ok(())
    }

    fn footprint(&self, mode: u8, n: u16, limit: u16, out: &mut BTreeSet<Device>) {
        let (source, dest) = match mode {
            0 | 1 | 6 | 7 => (n, n.div_ceil(2)),
            2..=5 => (n, n.saturating_mul(2)),
            8 => (11, 4),
            9 => (5, 3),
            11 | 15 => (9, 6),
            12 => (n.min(50) + 1, n.min(50) + 1),
            13 | 14 => ((n.min(50) + 1) * 2, (n.min(50) + 1) * 2),
            16 => (n + 1, n + 1),
            17 => (n + 1, n + 1),
            18 => (n.min(8) + 1, 2),
            19 => (2, n.min(8) + 2),
            30 => (n, n),
            31 => (1, n + 1),
            32 | 33 => (n + 1, 1),
            34 => (1, 2),
            35 | 36 => (2, 1),
            39 => (0, 10),
            52 => (5, 1),
            _ => (0, 0),
        };
        span(self.source.index, source, limit, out);
        span(self.dest.index, dest, limit, out);
    }

    pub fn execute(&self, memory: &mut Memory) {
        let _precision = Precision::nearest_extended();
        // The active handler stores its decoded S address into shared scratch
        // 53A42E, also read by the legacy word SORT continuation gate. It is
        // persistent across scans, so updates must follow the actual branches.
        memory.native_numeric_scratch = Some(self.source.index as i16);
        let mode = self.mode.read(memory) as u8;
        let count = self.count.read(memory).clamp(1, 256) as u16;
        let limit = memory.data_register_limit.unwrap_or(9999);
        if self.source.index > limit || self.dest.index > limit {
            limitation(
                memory,
                "DTM base register is outside the configured model range",
            );
            return;
        }
        match mode {
            0..=7 => self.bytes(memory, mode, count, limit),
            8 => self.gps(memory, limit),
            9 => self.positioning(memory, count, limit),
            10 => {} // Undocumented in the modern PDF; unsupported selectors are no-ops.
            11 => self.longitude_time(memory, limit),
            12..=14 => self.interpolate(memory, mode, count, limit),
            15 => self.solar(memory, limit),
            16 | 17 => self.string_copy(memory, mode, count, limit),
            18 => self.parse_decimal(memory, count, limit),
            19 => self.format_decimal(memory, count, limit),
            30 => {
                for offset in 0..count {
                    let (Some(source), Some(dest)) = (
                        self.source.index.checked_add(offset),
                        self.dest.index.checked_add(offset),
                    ) else {
                        break;
                    };
                    if source > limit || dest > limit {
                        break;
                    }
                    let value = word(memory, source) as u16;
                    memory.set_word(d(dest), value.reverse_bits() as i16);
                    memory.native_numeric_scratch = Some((source + 1) as i16);
                }
            }
            31..=36 => self.queue(memory, mode, count),
            39 => self.identity(memory, limit),
            52 => self.ntp(memory, limit),
            _ => {} // The manual explicitly specifies unsupported selectors as no-ops.
        }
    }

    // Cursor updates interleave partial byte writes and native boundary exits.
    #[allow(clippy::explicit_counter_loop)]
    fn bytes(&self, memory: &mut Memory, mode: u8, count: u16, limit: u16) {
        let mut source = self.source.index;
        let mut dest = self.dest.index;
        for index in 0..count {
            if source > limit || dest > limit {
                break;
            }
            match mode {
                0 | 1 => {
                    let high = (index & 1 == 0) == (mode == 0);
                    let value = byte(memory, source);
                    set_byte(memory, dest, high, value);
                    memory.native_numeric_scratch = Some((source + 1) as i16);
                    if index & 1 != 0 {
                        dest += 1;
                    }
                }
                2 | 3 => {
                    // Reload the second source byte after the first write: the
                    // original streams forward and exposes partial overlap.
                    let value = if mode == 2 {
                        (word(memory, source) as u16 >> 8) as u8
                    } else {
                        byte(memory, source)
                    };
                    set_byte(memory, dest, false, value);
                    dest += 1;
                    if dest > limit {
                        break;
                    }
                    let value = if mode == 2 {
                        byte(memory, source)
                    } else {
                        (word(memory, source) as u16 >> 8) as u8
                    };
                    set_byte(memory, dest, false, value);
                    dest += 1;
                    memory.native_numeric_scratch = Some((source + 1) as i16);
                    if mode == 3 && dest >= 9999 {
                        break;
                    }
                }
                4 | 5 => {
                    let value = byte(memory, source);
                    let first = if mode == 4 { value >> 4 } else { value & 15 };
                    set_byte(memory, dest, false, hex_ascii(first));
                    dest += 1;
                    if dest > limit {
                        break;
                    }
                    let value = byte(memory, source);
                    let second = if mode == 4 { value & 15 } else { value >> 4 };
                    set_byte(memory, dest, false, hex_ascii(second));
                    dest += 1;
                    memory.native_numeric_scratch = Some((source + 1) as i16);
                }
                6 | 7 => {
                    let digit = ascii_hex(byte(memory, source));
                    if index & 1 == 0 {
                        set_byte(
                            memory,
                            dest,
                            false,
                            if mode == 6 { digit << 4 } else { digit },
                        );
                    } else {
                        let digit = if mode == 7 { digit << 4 } else { digit };
                        let value = byte(memory, dest).wrapping_add(digit);
                        set_byte(memory, dest, false, value);
                        dest += 1;
                    }
                    memory.native_numeric_scratch = Some((source + 1) as i16);
                }
                _ => unreachable!(),
            }
            source += 1;
        }
    }

    fn gps(&self, memory: &mut Memory, limit: u16) {
        if !fits(self.source.index, 11, limit) || !fits(self.dest.index, 4, limit) {
            return;
        }
        let mut value = f32::from(byte(memory, self.source.index));
        for (offset, factor) in [(1, 0.01), (2, 0.0001), (3, 0.000001)] {
            value = mul_add_f64_f32(
                i32::from(byte(memory, self.source.index + offset)),
                factor,
                value,
            );
        }
        if byte(memory, self.source.index + 4) != b'N' {
            value = -value;
        }
        write_double(memory, self.dest.index, value.to_bits());
        // The first output is already written when the second input is read.
        let mut value = f32::from(byte(memory, self.source.index + 5)) * 100.0;
        value += f32::from(byte(memory, self.source.index + 6));
        for (offset, factor) in [(7, 0.01), (8, 0.0001), (9, 0.000001)] {
            value = mul_add_f64_f32(
                i32::from(byte(memory, self.source.index + offset)),
                factor,
                value,
            );
        }
        if byte(memory, self.source.index + 10) != b'E' {
            value = -value;
        }
        write_double(memory, self.dest.index + 2, value.to_bits());
    }

    fn string_copy(&self, memory: &mut Memory, mode: u8, count: u16, limit: u16) {
        let length = count
            .min(limit.saturating_sub(self.source.index) + 1)
            .min(limit.saturating_sub(self.dest.index) + 1);
        let mut offset = 0;
        if mode == 16 {
            while offset < length && byte(memory, self.dest.index + offset) != 0 {
                offset += 1;
            }
            let mut source = self.source.index;
            while offset < length {
                let value = byte(memory, source);
                set_byte(memory, self.dest.index + offset, false, value);
                if byte(memory, source) == 0 {
                    break;
                }
                offset += 1;
                source += 1;
            }
            // Legacy selector 16 does not append a forced terminator on a full
            // destination. This documented discrepancy remains explicit.
        } else {
            while offset < length && byte(memory, self.source.index + offset) != 0 {
                let value = byte(memory, self.source.index + offset);
                set_byte(memory, self.dest.index + offset, false, value);
                offset += 1;
            }
            if self.dest.index + offset <= limit {
                set_byte(memory, self.dest.index + offset, false, 0);
            }
        }
    }

    fn parse_decimal(&self, memory: &mut Memory, count: u16, limit: u16) {
        if !fits(self.dest.index, 2, limit) {
            return;
        }
        let mut remaining = count
            .min(8)
            .min(limit.saturating_sub(self.source.index) + 1);
        let mut source = self.source.index;
        let mut value = 0.0f32;
        while remaining != 0 && source <= limit {
            let character = byte(memory, source);
            source += 1;
            if character == b'.' {
                break;
            }
            let digit = if (b'1'..=b'9').contains(&character) {
                character - b'0'
            } else {
                0
            };
            value = (f64::from(value) * 10.0 + f64::from(digit)) as f32;
            remaining -= 1;
        }
        for factor in [
            0.1, 0.01, 0.001, 0.0001, 0.00001, 0.000001, 0.0000001, 0.00000001,
        ] {
            if remaining == 0 || source > limit {
                break;
            }
            let character = byte(memory, source);
            source += 1;
            if !character.is_ascii_digit() {
                break;
            }
            value = mul_add_f64_f32(i32::from(character - b'0'), factor, value);
            remaining -= 1;
        }
        write_double(memory, self.dest.index, value.to_bits());
    }

    fn format_decimal(&self, memory: &mut Memory, count: u16, limit: u16) {
        if !fits(self.source.index, 2, limit) {
            return;
        }
        let mut value = f32::from_bits(read_double(memory, self.source.index));
        if value >= 100_000_000.0 {
            set_byte(memory, self.dest.index, false, 0);
            return;
        }
        let room = limit.saturating_sub(self.dest.index) + 1;
        if room < 2 {
            return;
        }
        let mut remaining = count.min(8).min(room - 2) as i16;
        let mut dest = self.dest.index;
        if value >= 1.0 {
            let mut divisor = 10_000_000.0f32;
            for _ in 0..7 {
                if value / divisor >= 1.0 {
                    break;
                }
                divisor = tenth(divisor);
            }
            while remaining > 0 {
                let digit = trunc_i64(f64::from(value / divisor)) as i32;
                set_byte(memory, dest, false, digit.wrapping_add(0x30) as u8);
                dest += 1;
                remaining -= 1;
                value = subtract_product(value, digit as u8, divisor);
                if divisor <= 1.0 {
                    break;
                }
                divisor = tenth(divisor);
            }
        }
        set_byte(memory, dest, false, b'.');
        dest += 1;
        let mut divisor = 0.1f32;
        while remaining > 0 {
            let digit = trunc_i64(f64::from(value / divisor)) as i32;
            set_byte(memory, dest, false, digit.wrapping_add(0x30) as u8);
            dest += 1;
            remaining -= 1;
            value = subtract_product(value, digit as u8, divisor);
            divisor = tenth(divisor);
        }
        set_byte(memory, dest, false, 0);
    }

    fn interpolate(&self, memory: &mut Memory, mode: u8, count: u16, _limit: u16) {
        if !(2..=50).contains(&count) {
            return;
        }
        let width = if mode == 12 { 1 } else { 2 };
        if let Err(error) = self.validate_installed_execution(memory) {
            limitation(memory, &error);
            return;
        }
        let input = if mode == 12 {
            i32::from(word(memory, self.source.index))
        } else {
            read_double(memory, self.source.index) as i32
        };
        for index in 1..count {
            let source = self.source.index + index * width;
            let dest = self.dest.index + index * width;
            memory.native_numeric_scratch = Some(source as i16);
            if mode == 14 {
                let x = f32::from_bits(input as u32);
                let x1 = f32::from_bits(read_double(memory, source));
                let x2 = f32::from_bits(read_double(memory, source + 2));
                if x1 >= x2 {
                    return;
                }
                let output = if x <= x1 || x.is_nan() || x1.is_nan() {
                    f32::from_bits(read_double(memory, dest))
                } else if x <= x2 || x2.is_nan() {
                    let y1 = f32::from_bits(read_double(memory, dest));
                    let y2 = f32::from_bits(read_double(memory, dest + 2));
                    let delta = x - x1;
                    let span = x2 - x1;
                    let product = (y2 - y1) * delta;
                    let quotient = product / span;
                    quotient + y1
                } else if index == count - 1 {
                    f32::from_bits(read_double(memory, dest + 2))
                } else {
                    continue;
                };
                if x > x2 && index == count - 1 {
                    memory.native_numeric_scratch = Some((source + width) as i16);
                }
                write_double(memory, self.dest.index, output.to_bits());
            } else {
                let read = |index| {
                    if width == 1 {
                        i32::from(word(memory, index))
                    } else {
                        read_double(memory, index) as i32
                    }
                };
                let x1 = read(source);
                let x2 = read(source + width);
                if x1 >= x2 {
                    return;
                }
                let output = if input <= x1 {
                    read(dest)
                } else if input <= x2 {
                    let y1 = read(dest);
                    let y2 = read(dest + width);
                    let delta = if width == 1 {
                        (input as i16).wrapping_sub(x1 as i16) as i32
                    } else {
                        input.wrapping_sub(x1)
                    };
                    let span = if width == 1 {
                        x2 - x1
                    } else {
                        x2.wrapping_sub(x1)
                    } as f32;
                    let output_span = if width == 1 {
                        y2 - y1
                    } else {
                        y2.wrapping_sub(y1)
                    } as f32;
                    let product = int_mul_f32(delta, output_span);
                    let quotient = product / span;
                    let value = trunc_i64(f64::from(quotient)) as i32;
                    if width == 1 {
                        i32::from((value as i16).wrapping_add(y1 as i16))
                    } else {
                        value.wrapping_add(y1)
                    }
                } else if index == count - 1 {
                    read(dest + width)
                } else {
                    continue;
                };
                if input > x2 && index == count - 1 {
                    memory.native_numeric_scratch = Some((source + width) as i16);
                }
                if width == 1 {
                    memory.set_word(d(self.dest.index), output as i16);
                } else {
                    write_double(memory, self.dest.index, output as u32);
                }
            }
            return;
        }
    }

    fn longitude_time(&self, memory: &mut Memory, limit: u16) {
        if !fits(self.source.index, 9, limit) || !fits(self.dest.index, 6, limit) {
            return;
        }
        let mut longitude = f32::from_bits(read_double(memory, self.source.index));
        if longitude > 180.0 {
            longitude = 180.0;
            write_double(memory, self.source.index, longitude.to_bits());
        } else if longitude < -180.0 || longitude.is_nan() {
            longitude = -180.0;
            write_double(memory, self.source.index, longitude.to_bits());
        }
        let timezone = word(memory, self.source.index + 2);
        // Preserve the original's forward, unsnapshotted civil-time copy.
        for offset in 0..6 {
            let value = word(memory, self.source.index + 3 + offset);
            memory.set_word(d(self.dest.index + offset), value);
        }
        let base = self.dest.index;
        let hour = word(memory, base + 3).wrapping_sub(timezone);
        memory.set_word(d(base + 3), hour);
        normalize_hour(memory, base, 12);
        let mut offset = extended_scale(longitude, &ONE_FIFTEENTH);
        let hours = trunc_i64(f64::from(offset)) as i16;
        memory.native_numeric_scratch = Some(hours);
        offset -= f32::from(hours);
        offset *= 60.0;
        let minutes = trunc_i64(f64::from(offset)) as i16;
        offset -= f32::from(minutes);
        offset *= 60.0;
        let seconds = trunc_i64(f64::from(offset)) as i16;
        let seconds = word(memory, base + 5).wrapping_add(seconds);
        let (seconds, carry) = carry_sixty(seconds);
        memory.set_word(d(base + 5), seconds);
        let minutes = word(memory, base + 4)
            .wrapping_add(minutes)
            .wrapping_add(carry);
        let (minutes, carry) = carry_sixty(minutes);
        memory.set_word(d(base + 4), minutes);
        let hours = word(memory, base + 3)
            .wrapping_add(hours)
            .wrapping_add(carry);
        memory.set_word(d(base + 3), hours);
        normalize_hour(memory, base, 13);
    }

    fn positioning(&self, memory: &mut Memory, count: u16, limit: u16) {
        if !fits(self.source.index, 5, limit) || !fits(self.dest.index, 3, limit) {
            return;
        }
        let pulses = read_double(memory, self.source.index);
        let up = word(memory, self.source.index + 3);
        let down = word(memory, self.source.index + 4);
        let equal = up == down;
        let up_step = extended_scale(
            f32::from(up),
            if equal { &ONE_SIXTIETH } else { &ONE_THIRTIETH },
        );
        let down_step = extended_scale(f32::from(down), &ONE_THIRTIETH);
        let ramp_sum = up.wrapping_add(down);
        // The installed handler compares the ramp duration with the decoded n
        // scratch field, not its copy of S+2. This surprising behavior is kept
        // separate from the modern manual's reserved-n description.
        let mut total = count as i16;
        if ramp_sum > total && ramp_sum.wrapping_add(1) > 0 {
            total = ramp_sum.wrapping_add(1);
            memory.set_word(d(self.source.index + 2), total);
        }
        let mut frequency = pulses.wrapping_mul(1000) / total as u32;
        let mut start_frequency = 0u32;
        while frequency <= 200_000 {
            start_frequency = frequency / if equal { 61 } else { 31 } + 1;
            let mut sample_frequency = start_frequency;
            let mut ramp_pulses = 0u32;
            for _ in 0..if equal { 60 } else { 30 } {
                ramp_pulses = ramp_pulses.wrapping_add(scaled_pulses(
                    sample_frequency,
                    up_step,
                    if equal { &TWO_MILLI } else { &ONE_MILLI },
                ) as u32);
                if !equal {
                    ramp_pulses = ramp_pulses.wrapping_add(scaled_pulses(
                        sample_frequency,
                        down_step,
                        &ONE_MILLI,
                    ) as u32);
                }
                sample_frequency = sample_frequency.wrapping_add(start_frequency);
            }
            if ramp_pulses >= pulses {
                frequency = frequency.wrapping_add(10_000);
                continue;
            }
            let extra = remaining_milliseconds(frequency, pulses - ramp_pulses) as i32;
            let elapsed = (extra as i16).wrapping_add(ramp_sum);
            if elapsed <= total {
                break;
            }
            let excess = elapsed.wrapping_sub(total);
            frequency = frequency.wrapping_add(if excess > 10 {
                10_000
            } else if excess > 5 {
                5_000
            } else if !equal || excess > 3 {
                2_000
            } else {
                500
            });
        }
        if frequency > 200_000 {
            frequency = 0;
            start_frequency = 0;
        }
        write_double(memory, self.dest.index, frequency);
        memory.set_word(d(self.dest.index + 2), start_frequency as i16);
    }

    fn queue(&self, memory: &mut Memory, mode: u8, count: u16) {
        // Unknown model accesses hold the operation before mutation. Native
        // guard exits, in contrast, are verified successful no-ops.
        let mut snapshot = memory.clone();
        if let Err(error) = self.walk_queue(&mut snapshot, mode, count, &mut BTreeSet::new()) {
            limitation(memory, &error);
            return;
        }
        self.walk_queue(memory, mode, count, &mut BTreeSet::new())
            .expect("identical validated queue access sequence");
    }

    fn walk_queue(
        &self,
        memory: &mut Memory,
        mode: u8,
        count: u16,
        observed: &mut BTreeSet<Device>,
    ) -> Result<(), String> {
        let source = i32::from(self.source.index);
        let dest = i32::from(self.dest.index);
        let capacity = i32::from(count);
        let mut access = QueueAccess { memory, observed };
        match mode {
            31 => {
                if native_word_queue_guard(dest, capacity) {
                    return Ok(());
                }
                let mut index = access.word(dest)?;
                if index < 1 {
                    index = 1;
                    access.set_word(dest, 1)?;
                }
                if index > count as i16 {
                    access.set_word(dest, count as i16 + 1)?;
                    return Ok(());
                }
                let value = access.word(source)?;
                access.set_word(dest + i32::from(index), value)?;
                let next = access.word(dest)?.wrapping_add(1);
                access.set_word(dest, next)?;
            }
            32 | 33 => {
                if native_word_queue_guard(source, capacity) || (1000..2000).contains(&dest) {
                    return Ok(());
                }
                let index = access.word(source)?;
                if index < 2 || index > count as i16 + 1 {
                    return Ok(());
                }
                if mode == 32 {
                    let value = access.word(source + 1)?;
                    access.set_word(dest, value)?;
                    // The native FIFO shifts the whole capacity, even when
                    // the occupancy pointer says fewer items are present.
                    for offset in 1..capacity {
                        let value = access.word(source + offset + 1)?;
                        access.set_word(source + offset, value)?;
                    }
                    access.set_word(source + capacity, 0)?;
                    let next = access.word(source)?.wrapping_sub(1);
                    access.set_word(source, next)?;
                } else {
                    access.set_word(source, index.wrapping_sub(1))?;
                    let last = i32::from(access.word(source)?);
                    let value = access.word(source + last)?;
                    access.set_word(dest, value)?;
                    // Output may replace the header. The native clear uses
                    // that second, signed header value, not the saved index.
                    let clear = i32::from(access.word(source)?);
                    access.set_word(source + clear, 0)?;
                }
            }
            34..=36 => {
                if (mode == 34 && dest + 1 >= 10000) || (mode != 34 && source + 1 >= 10000) {
                    return Ok(());
                }
                let source_bit = i32::from(access.word(source)?);
                let target_bit = i32::from(access.word(dest)?);
                if !(0..=4095).contains(&source_bit) || !(0..=4095).contains(&target_bit) {
                    return Ok(());
                }
                if mode == 34 {
                    if native_bit_queue_guard(target_bit, capacity) {
                        return Ok(());
                    }
                    let mut index = access.word(dest + 1)?;
                    if index < 0 {
                        index = 0;
                        access.set_word(dest + 1, 0)?;
                    }
                    if index >= count as i16 {
                        access.set_word(dest + 1, count as i16)?;
                        return Ok(());
                    }
                    let source_bit = i32::from(access.word(source)?);
                    let target_bit = i32::from(access.word(dest)?);
                    let value = access.bit(source_bit)?;
                    access.set_bit(target_bit + i32::from(index), value)?;
                    let next = access.word(dest + 1)?.wrapping_add(1);
                    access.set_word(dest + 1, next)?;
                } else {
                    if native_bit_queue_guard(source_bit, capacity)
                        || (1000..2000).contains(&target_bit)
                    {
                        return Ok(());
                    }
                    let index = access.word(source + 1)?;
                    if index < 1 || index > count as i16 {
                        return Ok(());
                    }
                    if mode == 35 {
                        let value = access.bit(source_bit)?;
                        access.set_bit(target_bit, value)?;
                        for offset in 0..capacity - 1 {
                            let value = access.bit(source_bit + offset + 1)?;
                            access.set_bit(source_bit + offset, value)?;
                        }
                        access.set_bit(source_bit + capacity - 1, false)?;
                        let next = access.word(source + 1)?.wrapping_sub(1);
                        access.set_word(source + 1, next)?;
                    } else {
                        access.set_word(source + 1, index.wrapping_sub(1))?;
                        let source_bit = i32::from(access.word(source)?);
                        let selected = source_bit + i32::from(access.word(source + 1)?);
                        let target_bit = i32::from(access.word(dest)?);
                        let value = access.bit(selected)?;
                        access.set_bit(target_bit, value)?;
                        access.set_bit(selected, false)?;
                    }
                }
            }
            _ => unreachable!("queue selector"),
        }
        Ok(())
    }

    fn solar(&self, memory: &mut Memory, limit: u16) {
        if !fits(self.source.index, 9, limit) || !fits(self.dest.index, 6, limit) {
            limitation(
                memory,
                "DTM m=15 requires an available nine-word source and six-word destination",
            );
            return;
        }
        let Some(backend) = memory.transform_context.solar_backend else {
            // Validate the second date after simulated sunrise writes. An
            // unsafe alias must not enter the native date table or partially
            // publish a result before reporting the replay limitation.
            let mut snapshot = memory.clone();
            if let Err(error) = self.walk_solar_builtin(&mut snapshot) {
                limitation(memory, &error);
                return;
            }
            self.walk_solar_builtin(memory)
                .expect("the same deterministic solar walk was validated above");
            return;
        };
        let request = SolarRequest {
            longitude: f32::from_bits(read_double(memory, self.source.index)),
            latitude: f32::from_bits(read_double(memory, self.source.index + 2)),
            timezone_hours: word(memory, self.source.index + 4),
            daylight_saving: word(memory, self.source.index + 5),
            date: std::array::from_fn(|index| word(memory, self.source.index + 6 + index as u16)),
        };
        match backend(&request) {
            Ok(times) => {
                for (offset, value) in times.sunrise.into_iter().chain(times.sunset).enumerate() {
                    memory.set_word(d(self.dest.index + offset as u16), value);
                }
            }
            Err(error) => limitation(memory, &format!("DTM sunrise/sunset backend: {error}")),
        }
    }

    fn walk_solar_builtin(&self, memory: &mut Memory) -> Result<(), String> {
        let limit = memory.data_register_limit.unwrap_or(9999).min(9999);
        if !fits(self.source.index, 9, limit) || !fits(self.dest.index, 6, limit) {
            return Err(
                "DTM m=15 requires an available nine-word source and six-word destination".into(),
            );
        }
        // 42DB25 snapshots these six words once. Each event then reloads its
        // date, allowing sunrise's output to change the sunset calculation.
        let parameters = crate::solar_position::DtmSolarParameters {
            longitude: f32::from_bits(read_double(memory, self.source.index)),
            latitude: f32::from_bits(read_double(memory, self.source.index + 2)),
            timezone_hours: word(memory, self.source.index + 4),
            daylight_saving: word(memory, self.source.index + 5) != 0,
        };
        for sunset in [false, true] {
            let date =
                std::array::from_fn(|index| word(memory, self.source.index + 6 + index as u16));
            if let Some(times) = parameters.event(date, sunset)? {
                let base = self.dest.index + if sunset { 3 } else { 0 };
                for (offset, value) in times.into_iter().enumerate() {
                    memory.set_word(d(base + offset as u16), value);
                }
            }
        }
        Ok(())
    }

    fn identity(&self, memory: &mut Memory, limit: u16) {
        let Some(serial) = memory.transform_context.serial_number.clone() else {
            limitation(
                memory,
                "DTM m=39 requires a configured virtual PLC serial number",
            );
            return;
        };
        if serial.len() > 18 || !fits(self.dest.index, 1 + serial.len().div_ceil(2) as u16, limit) {
            limitation(
                memory,
                "DTM virtual serial number does not fit its documented destination span",
            );
            return;
        }
        memory.set_word(d(self.dest.index), serial.len() as i16);
        for (index, pair) in serial.chunks(2).enumerate() {
            let value = u16::from(pair[0]) << 8 | u16::from(*pair.get(1).unwrap_or(&0));
            memory.set_word(d(self.dest.index + 1 + index as u16), value as i16);
        }
    }

    fn ntp(&self, memory: &mut Memory, limit: u16) {
        if !fits(self.source.index, 5, limit) {
            return;
        }
        let Some(backend) = memory.transform_context.ntp_backend else {
            limitation(
                memory,
                "DTM m=52 requires an explicitly configured NTP and clock backend",
            );
            return;
        };
        let high = word(memory, self.source.index) as u16;
        let low = word(memory, self.source.index + 1) as u16;
        let request = NtpRequest {
            server: [(high >> 8) as u8, high as u8, (low >> 8) as u8, low as u8],
            update_minutes: word(memory, self.source.index + 2) as u16,
            timezone_hours: word(memory, self.source.index + 3),
            timezone_minutes: word(memory, self.source.index + 4),
        };
        memory.transform_context.last_ntp_request = Some(request.clone());
        match backend(&request) {
            Ok(reply) => {
                memory.set_word(self.dest, reply.status as i16);
                if reply.status == NtpStatus::Successful && reply.unix_seconds.is_some() {
                    memory.transform_context.synchronized_unix_seconds = reply.unix_seconds;
                }
                memory.transform_context.last_ntp_reply = Some(reply);
            }
            Err(error) => {
                memory.set_word(self.dest, NtpStatus::Failed as i16);
                limitation(memory, &format!("DTM NTP backend: {error}"));
            }
        }
    }
}

fn word(memory: &Memory, index: u16) -> i16 {
    memory.word(d(index))
}
fn byte(memory: &Memory, index: u16) -> u8 {
    word(memory, index) as u8
}
fn read_double(memory: &Memory, index: u16) -> u32 {
    u32::from(word(memory, index) as u16) | (u32::from(word(memory, index + 1) as u16) << 16)
}
fn write_double(memory: &mut Memory, index: u16, value: u32) {
    memory.set_word(d(index), value as i16);
    memory.set_word(d(index + 1), (value >> 16) as i16);
}
fn set_byte(memory: &mut Memory, index: u16, high: bool, value: u8) {
    let old = word(memory, index) as u16;
    let value = if high {
        old & 0xff | (u16::from(value) << 8)
    } else {
        old & 0xff00 | u16::from(value)
    };
    memory.set_word(d(index), value as i16);
}
fn fits(start: u16, count: u16, limit: u16) -> bool {
    start <= limit && (count == 0 || start.checked_add(count - 1).is_some_and(|end| end <= limit))
}
fn span(start: u16, count: u16, limit: u16, out: &mut BTreeSet<Device>) {
    out.extend(
        (start..start.saturating_add(count))
            .filter(|index| *index <= limit)
            .map(d),
    );
}
fn hex_ascii(value: u8) -> u8 {
    if value < 10 {
        b'0' + value
    } else {
        b'A' + value - 10
    }
}
fn ascii_hex(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        b'A'..=b'F' => value - b'A' + 10,
        _ => 0,
    }
}
fn limitation(memory: &mut Memory, message: &str) {
    memory.execution_limitations.insert(message.to_owned());
}
fn native_word_queue_guard(start: i32, count: i32) -> bool {
    (1000..2000).contains(&start)
        || (start < 1000 && start + count >= 1000)
        || start + count >= 10000
}
fn native_bit_queue_guard(start: i32, count: i32) -> bool {
    (1000..2000).contains(&start)
        || (start < 1000 && start + count >= 1000)
        || start + count >= 4096
}

/// Checked access shared by native preflight, observation and local execution.
/// Known native guard exits happen before these helpers. An error here marks
/// an unverified/model-inaccessible walk, rather than inventing a PLC error.
struct QueueAccess<'a> {
    memory: &'a mut Memory,
    observed: &'a mut BTreeSet<Device>,
}
impl QueueAccess<'_> {
    fn data_device(&mut self, index: i32) -> Result<Device, String> {
        let limit = self.memory.data_register_limit.unwrap_or(9999).min(9999);
        if !(0..=i32::from(limit)).contains(&index) {
            return Err(format!(
                "DTM queue native access D{index} exceeds configured D0..D{limit}"
            ));
        }
        let device = d(index as u16);
        self.observed.insert(device);
        Ok(device)
    }
    fn word(&mut self, index: i32) -> Result<i16, String> {
        let device = self.data_device(index)?;
        Ok(self.memory.word(device))
    }
    fn set_word(&mut self, index: i32, value: i16) -> Result<(), String> {
        let device = self.data_device(index)?;
        self.memory.set_word(device, value);
        Ok(())
    }
    fn relay_device(&mut self, index: i32) -> Result<Device, String> {
        let limit = self.memory.relay_limit.unwrap_or(4095).min(4095);
        if !(0..=i32::from(limit)).contains(&index) || (1000..2000).contains(&index) {
            return Err(format!(
                "DTM queue native access M{index} is outside the verified ordinary relay range"
            ));
        }
        let device = Memory::system_device(index as u16);
        self.observed.insert(device);
        Ok(device)
    }
    fn bit(&mut self, index: i32) -> Result<bool, String> {
        let device = self.relay_device(index)?;
        Ok(self.memory.bit(device))
    }
    fn set_bit(&mut self, index: i32, value: bool) -> Result<(), String> {
        let device = self.relay_device(index)?;
        self.memory.set_internal(device, value);
        Ok(())
    }
}
fn carry_sixty(value: i16) -> (i16, i16) {
    if value >= 60 {
        (value.wrapping_sub(60), 1)
    } else if value < 0 {
        (value.wrapping_add(60), -1)
    } else {
        (value, 0)
    }
}

/// The legacy calendar helper has no year parameter and always treats February
/// as 28 days. Preserve it for this selector rather than silently adding leaps.
fn month_day_adjust(month: i16, day: i16) -> i16 {
    match day {
        0 => match month {
            1 | 2 | 4 | 6 | 8 | 9 | 11 => 31,
            3 => 28,
            _ => 30,
        },
        29 if month == 2 => 1,
        31 if matches!(month, 4 | 6 | 9 | 11) => 1,
        32 => 1,
        _ => day,
    }
}
fn normalize_hour(memory: &mut Memory, base: u16, month_rollover: i16) {
    let hour = word(memory, base + 3);
    if hour < 0 {
        memory.set_word(d(base + 3), hour.wrapping_add(24));
        let day = word(memory, base + 2).wrapping_sub(1);
        memory.set_word(d(base + 2), day);
        if day == 0 {
            let month = word(memory, base + 1);
            memory.set_word(d(base + 2), month_day_adjust(month, day));
            let month = month.wrapping_sub(1);
            memory.set_word(d(base + 1), month);
            if month == 0 {
                memory.set_word(d(base + 1), 12);
                let year = word(memory, base).wrapping_sub(1);
                memory.set_word(d(base), year);
            }
        }
    } else if hour >= 24 {
        memory.set_word(d(base + 3), hour.wrapping_sub(24));
        let day = word(memory, base + 2).wrapping_add(1);
        memory.set_word(d(base + 2), day);
        if day > 28 {
            let adjusted = month_day_adjust(word(memory, base + 1), day);
            if adjusted != day {
                memory.set_word(d(base + 2), adjusted);
                let month = word(memory, base + 1).wrapping_add(1);
                memory.set_word(d(base + 1), month);
                if month == month_rollover {
                    memory.set_word(d(base + 1), 1);
                    let year = word(memory, base).wrapping_add(1);
                    memory.set_word(d(base), year);
                }
            }
        }
    }
}

fn trunc_i64(value: f64) -> i64 {
    if value.is_finite()
        && (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&value)
    {
        value.trunc() as i64
    } else {
        i64::MIN
    }
}

const ONE_THIRTIETH: [u8; 10] = [0x89, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0xfa, 0x3f];
const ONE_SIXTIETH: [u8; 10] = [0x89, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0xf9, 0x3f];
const ONE_FIFTEENTH: [u8; 10] = [0x89, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0xfb, 0x3f];
const ONE_TENTH: [u8; 10] = [0xcd, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xfb, 0x3f];
const ONE_MILLI: [u8; 10] = [0x3b, 0xdf, 0x4f, 0x8d, 0x97, 0x6e, 0x12, 0x83, 0xf5, 0x3f];
const TWO_MILLI: [u8; 10] = [0x3b, 0xdf, 0x4f, 0x8d, 0x97, 0x6e, 0x12, 0x83, 0xf6, 0x3f];

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
struct Precision(u16);
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
impl Precision {
    fn nearest_extended() -> Self {
        let mut previous = 0u16;
        unsafe {
            std::arch::asm!("fnstcw word ptr [{p}]", p=in(reg) &mut previous, options(nostack));
        }
        let current = (previous & !0x0f00) | 0x0300;
        unsafe {
            std::arch::asm!("fldcw word ptr [{p}]", p=in(reg) &current, options(nostack));
        }
        Self(previous)
    }
}
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
impl Drop for Precision {
    fn drop(&mut self) {
        unsafe {
            std::arch::asm!("fldcw word ptr [{p}]", p=in(reg) &self.0, options(nostack));
        }
    }
}
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
struct Precision;
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
impl Precision {
    fn nearest_extended() -> Self {
        Self
    }
}

fn mul_add_f64_f32(value: i32, factor: f64, sum: f32) -> f32 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let mut output = 0.0f32;
        unsafe {
            std::arch::asm!("fild dword ptr [{v}]", "fmul qword ptr [{f}]", "fadd dword ptr [{s}]", "fstp dword ptr [{o}]", v=in(reg)&value,f=in(reg)&factor,s=in(reg)&sum,o=in(reg)&mut output,options(nostack));
        }
        output
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        (f64::from(value) * factor + f64::from(sum)) as f32
    }
}
fn int_mul_f32(value: i32, factor: f32) -> f32 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let mut output = 0.0f32;
        unsafe {
            std::arch::asm!("fild dword ptr [{v}]","fmul dword ptr [{f}]","fstp dword ptr [{o}]",v=in(reg)&value,f=in(reg)&factor,o=in(reg)&mut output,options(nostack));
        }
        output
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        (f64::from(value) * f64::from(factor)) as f32
    }
}
fn extended_scale(value: f32, factor: &[u8; 10]) -> f32 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let mut output = 0.0f32;
        unsafe {
            std::arch::asm!("fld tbyte ptr [{f}]","fmul dword ptr [{v}]","fstp dword ptr [{o}]",v=in(reg)&value,f=in(reg)factor.as_ptr(),o=in(reg)&mut output,options(nostack));
        }
        output
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        let mantissa = u64::from_le_bytes(factor[..8].try_into().unwrap());
        let exponent = i32::from(u16::from_le_bytes([factor[8], factor[9]]) & 0x7fff) - 16383 - 63;
        (f64::from(value) * (mantissa as f64) * 2f64.powi(exponent)) as f32
    }
}
fn tenth(value: f32) -> f32 {
    extended_scale(value, &ONE_TENTH)
}
fn subtract_product(value: f32, digit: u8, factor: f32) -> f32 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let digit = i32::from(digit);
        let mut output = 0.0f32;
        unsafe {
            std::arch::asm!("fild dword ptr [{d}]","fmul dword ptr [{f}]","fsubr dword ptr [{v}]","fstp dword ptr [{o}]",d=in(reg)&digit,f=in(reg)&factor,v=in(reg)&value,o=in(reg)&mut output,options(nostack));
        }
        output
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        (f64::from(value) - f64::from(digit) * f64::from(factor)) as f32
    }
}
fn scaled_pulses(frequency: u32, duration: f32, scale: &[u8; 10]) -> i64 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let frequency = i64::from(frequency);
        let mut output = 0i64;
        let mut control = 0u16;
        unsafe {
            std::arch::asm!("fnstcw word ptr [{c}]",c=in(reg)&mut control,options(nostack));
        }
        let truncated = control | 0x0c00;
        unsafe {
            std::arch::asm!("fild qword ptr [{f}]","fmul dword ptr [{d}]","fld tbyte ptr [{s}]","fmulp st(1), st(0)","fldcw word ptr [{t}]","fistp qword ptr [{o}]","fldcw word ptr [{c}]",f=in(reg)&frequency,d=in(reg)&duration,s=in(reg)scale.as_ptr(),t=in(reg)&truncated,o=in(reg)&mut output,c=in(reg)&control,options(nostack));
        }
        output
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        trunc_i64(
            f64::from(frequency)
                * f64::from(duration)
                * if scale == &ONE_MILLI { 0.001 } else { 0.002 },
        )
    }
}
fn remaining_milliseconds(frequency: u32, remaining: u32) -> i64 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let frequency = i64::from(frequency);
        let remaining = i64::from(remaining);
        let thousand = 1000f32;
        let mut output = 0i64;
        let mut control = 0u16;
        unsafe {
            std::arch::asm!("fnstcw word ptr [{c}]",c=in(reg)&mut control,options(nostack));
        }
        let truncated = control | 0x0c00;
        unsafe {
            std::arch::asm!("fild qword ptr [{f}]","fdivr dword ptr [{k}]","fild qword ptr [{r}]","fmulp st(1), st(0)","fldcw word ptr [{t}]","fistp qword ptr [{o}]","fldcw word ptr [{c}]",f=in(reg)&frequency,k=in(reg)&thousand,r=in(reg)&remaining,t=in(reg)&truncated,o=in(reg)&mut output,c=in(reg)&control,options(nostack));
        }
        output
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        trunc_i64(1000.0 / f64::from(frequency) * f64::from(remaining))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn action(mode: i16, count: i16) -> TransformAction {
        TransformAction {
            source: d(0),
            dest: d(100),
            mode: DataValue::Word(Operand::Constant(mode)),
            count: DataValue::Word(Operand::Constant(count)),
        }
    }
    #[test]
    fn byte_stream_keeps_untouched_bytes_and_exposes_overlap() {
        let mut memory = Memory::default();
        memory.set_word(d(0), 0x1234);
        memory.set_word(d(1), 0x5678);
        memory.set_word(d(100), 0x55aa);
        memory.set_word(d(101), 0x66bb);
        action(0, 1).execute(&mut memory);
        assert_eq!(word(&memory, 100) as u16, 0x34aa);
        action(2, 1).execute(&mut memory);
        assert_eq!(word(&memory, 100) as u16, 0x3412);
        assert_eq!(word(&memory, 101) as u16, 0x6634);
        let mut overlap = action(4, 1);
        overlap.dest = d(0);
        overlap.execute(&mut memory);
        assert_eq!(byte(&memory, 0), b'3');
        assert_eq!(byte(&memory, 1), b'3');
    }
    #[test]
    fn ascii_pairs_are_nibbles_and_invalid_maps_to_zero() {
        let mut memory = Memory::default();
        memory.set_word(d(0), b'a' as i16);
        memory.set_word(d(1), b'Z' as i16);
        memory.set_word(d(100), 0x5501);
        action(6, 2).execute(&mut memory);
        assert_eq!(word(&memory, 100) as u16, 0x55a0);
        action(7, 2).execute(&mut memory);
        assert_eq!(word(&memory, 100) as u16, 0x550a);
    }
    #[test]
    fn piecewise_word_uses_wrapped_delta_before_float_interpolation() {
        let mut memory = Memory::default();
        memory.set_word(d(0), 1);
        memory.set_word(d(1), 0);
        memory.set_word(d(2), 2);
        memory.set_word(d(101), 0);
        memory.set_word(d(102), 3);
        action(12, 2).execute(&mut memory);
        assert_eq!(word(&memory, 100), 1);
        memory.set_word(d(2), 0);
        memory.set_word(d(100), 77);
        action(12, 2).execute(&mut memory);
        assert_eq!(word(&memory, 100), 77);
    }
    #[test]
    fn modern_queues_and_identity_require_explicit_context() {
        let mut memory = Memory::default();
        memory.set_word(d(0), 42);
        action(31, 2).execute(&mut memory);
        assert_eq!(word(&memory, 100), 2);
        assert_eq!(word(&memory, 101), 42);
        action(39, 1).execute(&mut memory);
        assert!(!memory.execution_limitations.is_empty());
        memory.transform_context.serial_number = Some(b"ABC".to_vec());
        action(39, 1).execute(&mut memory);
        assert_eq!(word(&memory, 100), 3);
        assert_eq!(word(&memory, 101) as u16, 0x4142);
        assert_eq!(word(&memory, 102) as u16, 0x4300);
    }
    #[test]
    fn callbacks_are_explicit_and_do_not_invent_solar_or_network_results() {
        fn solar(_: &SolarRequest) -> Result<SolarTimes, String> {
            Ok(SolarTimes {
                sunrise: [6, 39, 47],
                sunset: [17, 16, 42],
            })
        }
        fn ntp(_: &NtpRequest) -> Result<NtpReply, String> {
            Ok(NtpReply {
                status: NtpStatus::Successful,
                unix_seconds: Some(123),
            })
        }
        let mut memory = Memory::default();
        action(15, 1).execute(&mut memory);
        assert_eq!(word(&memory, 100), 0);
        memory.transform_context.solar_backend = Some(solar);
        action(15, 1).execute(&mut memory);
        assert_eq!(word(&memory, 100), 6);
        memory.transform_context.ntp_backend = Some(ntp);
        action(52, 1).execute(&mut memory);
        assert_eq!(
            memory.transform_context.synchronized_unix_seconds,
            Some(123)
        );
        assert!(!supports_installed_mode(15));
        assert!(!supports_installed_mode(52));
        assert!(supports_installed_mode(7));
    }

    #[test]
    fn native_preflight_distinguishes_terminated_strings_from_unsafe_walks() {
        let mut memory = Memory::default();
        memory.data_register_limit = Some(4999);
        let mut capture = action(17, 256);
        capture.dest = d(4999);
        assert!(capture.validate_installed_execution(&memory).is_ok());
        memory.set_word(d(0), b'A' as i16);
        assert!(capture.validate_installed_execution(&memory).is_err());
        let mut pack = action(0, 256);
        pack.source = d(4999);
        assert!(pack.validate_installed_execution(&memory).is_err());
        memory.data_register_limit = Some(9999);
        pack.source = d(9999);
        assert!(pack.validate_installed_execution(&memory).is_ok());
        let mut gps = action(8, 1);
        gps.source = d(9997);
        assert!(gps.validate_installed_execution(&memory).is_err());
    }

    #[test]
    fn formatting_large_value_needs_only_one_terminator_word() {
        let mut memory = Memory::default();
        memory.data_register_limit = Some(4999);
        write_double(&mut memory, 0, 100_000_000.0f32.to_bits());
        memory.set_word(d(4999), 0x55aa);
        let mut formatter = action(19, 8);
        formatter.dest = d(4999);
        assert!(formatter.validate_installed_execution(&memory).is_ok());
        formatter.execute(&mut memory);
        assert_eq!(word(&memory, 4999) as u16, 0x5500);
    }

    #[test]
    fn legacy_scratch_tracks_stream_progress_half_writes_and_scalar_modes() {
        let mut memory = Memory::default();
        memory.native_numeric_scratch = Some(777);
        let mut transform = action(0, 3);
        transform.source = d(20);
        transform.execute(&mut memory);
        assert_eq!(memory.native_numeric_scratch, Some(23));
        transform.mode = DataValue::Word(Operand::Constant(2));
        transform.dest = d(9999);
        transform.execute(&mut memory);
        assert_eq!(memory.native_numeric_scratch, Some(20));
        transform.mode = DataValue::Word(Operand::Constant(3));
        transform.dest = d(9997);
        transform.execute(&mut memory);
        assert_eq!(memory.native_numeric_scratch, Some(21));
        for mode in [8, 9, 16, 17, 18, 19, 20] {
            transform.mode = DataValue::Word(Operand::Constant(mode));
            transform.dest = d(100);
            transform.execute(&mut memory);
            assert_eq!(memory.native_numeric_scratch, Some(20), "mode{mode}");
        }
        write_double(&mut memory, 20, 45.0f32.to_bits());
        transform.mode = DataValue::Word(Operand::Constant(11));
        transform.execute(&mut memory);
        assert_eq!(memory.native_numeric_scratch, Some(3));
    }

    #[test]
    fn legacy_interpolation_scratch_records_selected_segment_and_final_clip() {
        for mode in [12, 13, 14] {
            let mut memory = Memory::default();
            let width = if mode == 12 { 1 } else { 2 };
            let mut transform = action(mode, 2);
            transform.source = d(20);
            if mode == 12 {
                memory.set_word(d(20), 5);
                memory.set_word(d(21), 0);
                memory.set_word(d(22), 10);
                memory.set_word(d(101), 0);
                memory.set_word(d(102), 100);
            } else {
                for (index, value) in [(20, 5), (22, 0), (24, 10), (102, 0), (104, 100)] {
                    write_double(
                        &mut memory,
                        index,
                        if mode == 14 {
                            (value as f32).to_bits()
                        } else {
                            value as u32
                        },
                    );
                }
            }
            transform.execute(&mut memory);
            assert_eq!(memory.native_numeric_scratch, Some(20 + width));
            if mode == 12 {
                memory.set_word(d(20), 11);
            } else {
                write_double(
                    &mut memory,
                    20,
                    if mode == 14 { 11.0f32.to_bits() } else { 11 },
                );
            }
            transform.execute(&mut memory);
            assert_eq!(memory.native_numeric_scratch, Some(20 + 2 * width));
            transform.count = DataValue::Word(Operand::Constant(1));
            transform.execute(&mut memory);
            assert_eq!(memory.native_numeric_scratch, Some(20));
        }
    }

    #[test]
    fn modern_bit_reverse_streams_forward_when_destination_overlaps_source() {
        let mut memory = Memory::default();
        memory.set_word(d(0), 1);
        memory.set_word(d(1), 2);
        action(30, 2).execute(&mut memory);
        assert_eq!(word(&memory, 100) as u16, 0x8000);
        assert_eq!(word(&memory, 101) as u16, 0x4000);
        assert_eq!(memory.native_numeric_scratch, Some(2));
        let mut overlap = action(30, 2);
        overlap.dest = d(1);
        overlap.execute(&mut memory);
        assert_eq!(word(&memory, 1) as u16, 0x8000);
        assert_eq!(word(&memory, 2), 1);
        assert_eq!(memory.native_numeric_scratch, Some(2));
    }

    #[test]
    fn modern_word_queue_full_empty_fifo_lifo_and_forward_alias_rules() {
        let mut memory = Memory::default();
        memory.set_word(d(100), -1);
        memory.set_word(d(0), 12);
        action(31, 2).execute(&mut memory);
        memory.set_word(d(0), 13);
        action(31, 2).execute(&mut memory);
        memory.set_word(d(0), 99);
        action(31, 2).execute(&mut memory);
        assert_eq!(
            [word(&memory, 100), word(&memory, 101), word(&memory, 102)],
            [3, 12, 13]
        );
        let pop = TransformAction {
            source: d(100),
            dest: d(200),
            ..action(32, 2)
        };
        pop.execute(&mut memory);
        assert_eq!(word(&memory, 200), 12);
        assert_eq!(
            [word(&memory, 100), word(&memory, 101), word(&memory, 102)],
            [2, 13, 0]
        );
        pop.execute(&mut memory);
        pop.execute(&mut memory);
        assert_eq!(word(&memory, 200), 13);
        assert_eq!(word(&memory, 100), 1);
        memory.set_word(d(100), 3);
        memory.set_word(d(101), 12);
        memory.set_word(d(102), 13);
        let stack = TransformAction {
            source: d(100),
            dest: d(200),
            ..action(33, 2)
        };
        stack.execute(&mut memory);
        assert_eq!(word(&memory, 200), 13);
        stack.execute(&mut memory);
        stack.execute(&mut memory);
        assert_eq!(word(&memory, 200), 12);
        assert_eq!(word(&memory, 100), 1);
        memory.set_word(d(100), 3);
        memory.set_word(d(101), 12);
        memory.set_word(d(102), 13);
        let alias = TransformAction {
            dest: d(101),
            ..pop
        };
        alias.execute(&mut memory);
        assert_eq!(word(&memory, 101), 13);
        assert_eq!(word(&memory, 100), 2);
        memory.set_word(d(100), 3);
        memory.set_word(d(102), 13);
        let alias = TransformAction {
            dest: d(102),
            ..stack
        };
        alias.execute(&mut memory);
        assert_eq!(word(&memory, 102), 0);
    }

    #[test]
    fn modern_queue_trigger_forms_rearm_and_respect_full_storage() {
        let program = crate::plc::compile("LD X0\nDTMP D0 D100 K31 K2\nEND").unwrap();
        let mut simulator = crate::plc::Simulator::default();
        simulator.memory.set_word(d(0), 12);
        let input = Device {
            area: Area::X,
            index: 0,
        };
        simulator.memory.set_input(input, true);
        simulator.scan(&program, 100);
        simulator.memory.set_word(d(0), 13);
        simulator.scan(&program, 100);
        assert_eq!(
            [
                word(&simulator.memory, 100),
                word(&simulator.memory, 101),
                word(&simulator.memory, 102)
            ],
            [2, 12, 0]
        );
        simulator.memory.set_input(input, false);
        simulator.scan(&program, 100);
        simulator.memory.set_input(input, true);
        simulator.scan(&program, 100);
        assert_eq!(
            [
                word(&simulator.memory, 100),
                word(&simulator.memory, 101),
                word(&simulator.memory, 102)
            ],
            [3, 12, 13]
        );
        simulator.memory.set_input(input, false);
        simulator.scan(&program, 100);
        simulator.memory.set_input(input, true);
        simulator.scan(&program, 100);
        assert_eq!(word(&simulator.memory, 100), 3);
    }

    #[test]
    fn modern_bit_queue_full_empty_and_alias_rules_follow_current_native_order() {
        let mut memory = Memory::default();
        memory.set_word(d(0), 50);
        memory.set_word(d(100), 200);
        memory.set_word(d(101), -1);
        memory.set_system(50, true);
        memory.set_system(202, true);
        action(34, 2).execute(&mut memory);
        memory.set_system(50, false);
        action(34, 2).execute(&mut memory);
        memory.set_system(50, true);
        action(34, 2).execute(&mut memory);
        assert!(memory.system_bit(200));
        assert!(!memory.system_bit(201));
        assert!(memory.system_bit(202));
        assert_eq!(word(&memory, 101), 2);
        memory.set_word(d(20), 70);
        let fifo = TransformAction {
            source: d(100),
            dest: d(20),
            ..action(35, 2)
        };
        fifo.execute(&mut memory);
        assert!(memory.system_bit(70));
        assert_eq!(word(&memory, 101), 1);
        fifo.execute(&mut memory);
        assert!(!memory.system_bit(70));
        fifo.execute(&mut memory);
        assert_eq!(word(&memory, 101), 0);
        memory.set_word(d(101), 2);
        memory.set_system(200, true);
        memory.set_system(201, false);
        let stack = TransformAction {
            source: d(100),
            dest: d(20),
            ..action(36, 2)
        };
        stack.execute(&mut memory);
        assert!(!memory.system_bit(70));
        stack.execute(&mut memory);
        assert!(memory.system_bit(70));
        stack.execute(&mut memory);
        assert_eq!(word(&memory, 101), 0);
        memory.set_word(d(101), 2);
        memory.set_word(d(20), 200);
        memory.set_system(200, true);
        memory.set_system(201, false);
        fifo.execute(&mut memory);
        assert!(!memory.system_bit(200));
        assert_eq!(word(&memory, 101), 1);
        memory.set_word(d(101), 2);
        memory.set_word(d(20), 201);
        memory.set_system(201, true);
        stack.execute(&mut memory);
        assert!(!memory.system_bit(201));
    }

    #[test]
    fn modern_native_guard_holds_are_distinct_from_unavailable_model_accesses() {
        let mut memory = Memory::default();
        memory.data_register_limit = Some(4999);
        memory.set_word(d(0), 42);
        memory.set_word(d(1000), 123);
        memory.set_word(d(1001), 77);
        let forbidden = TransformAction {
            dest: d(1000),
            ..action(31, 2)
        };
        forbidden.execute(&mut memory);
        assert_eq!(word(&memory, 1000), 123);
        assert_eq!(word(&memory, 1001), 77);
        assert!(memory.execution_limitations.is_empty());
        memory.execution_limitations.clear();
        let boundary = TransformAction {
            dest: d(4999),
            ..action(31, 2)
        };
        boundary.execute(&mut memory);
        assert!(!memory.execution_limitations.is_empty());
        memory.execution_limitations.clear();
        memory.set_word(d(0), 4095);
        memory.set_word(d(100), 4094);
        memory.set_word(d(101), 0);
        memory.set_system(4095, true);
        action(34, 1).execute(&mut memory);
        assert_eq!(word(&memory, 101), 1);
        assert!(memory.system_bit(4094));
        assert!(memory.execution_limitations.is_empty());
        memory.set_word(d(101), 0);
        action(34, 2).execute(&mut memory);
        assert_eq!(word(&memory, 101), 0);
        assert!(memory.execution_limitations.is_empty());
        let mut observed = BTreeSet::new();
        action(34, 2).observed_devices(&memory, &mut observed);
        assert!(
            !observed
                .iter()
                .any(|device| device.area == Area::M && device.index > 4095)
        );
        memory.execution_limitations.clear();
        memory.set_word(d(0), 50);
        memory.set_word(d(100), 999);
        action(34, 2).execute(&mut memory);
        assert!(memory.execution_limitations.is_empty());
        let mut observed = BTreeSet::new();
        action(34, 2).observed_devices(&memory, &mut observed);
        assert!(!observed.contains(&Memory::system_device(1000)));
        memory.execution_limitations.clear();
        memory.relay_limit = Some(8191);
        memory.set_word(d(0), 5000);
        memory.set_word(d(100), 8000);
        memory.set_system(5000, true);
        action(34, 1).execute(&mut memory);
        assert!(!memory.system_bit(8000));
        assert!(memory.execution_limitations.is_empty());
        // The exact native source guard allows special M reads for mode 34,
        // but this PDF-forbidden/raw-bank path remains a preflight limitation.
        memory.set_word(d(0), 1000);
        memory.set_word(d(100), 200);
        memory.set_word(d(101), 0);
        assert!(
            action(34, 1)
                .validate_for_profile(&memory, NativeTransformProfile::CurrentEs2)
                .is_err()
        );
        action(34, 1).execute(&mut memory);
        assert!(!memory.execution_limitations.is_empty());
        assert_eq!(word(&memory, 101), 0);
        assert!(!memory.system_bit(1067) && !memory.system_bit(1068));
    }

    #[test]
    fn current_profile_adds_only_verified_solar_queue_and_reverse_modes() {
        for mode in [15, 30, 31, 32, 33, 34, 35, 36] {
            assert!(!supports_installed_mode(mode));
            assert!(NativeTransformProfile::CurrentEs2.supports(mode));
            assert!(NativeTransformProfile::CurrentEs2.supports(mode | 0x100));
        }
        for mode in [10, 39, 52] {
            assert!(!NativeTransformProfile::CurrentEs2.supports(mode));
        }
        let mut memory = Memory::default();
        assert!(action(30, 1).validate_installed_execution(&memory).is_err());
        assert!(
            action(30, 1)
                .validate_for_profile(&memory, NativeTransformProfile::CurrentEs2)
                .is_ok()
        );
        for (count, expected_cursor) in [(-1, 1), (0, 1), (257, 256)] {
            let mut reverse = action(30, count);
            reverse.dest = d(500);
            reverse.execute(&mut memory);
            assert_eq!(memory.native_numeric_scratch, Some(expected_cursor));
        }
        let reverse = TransformAction {
            source: d(9999),
            ..action(30, 2)
        };
        reverse.execute(&mut memory);
        assert_eq!(memory.native_numeric_scratch, Some(10000));
    }

    #[test]
    fn current_fifo_shifts_capacity_even_when_only_one_item_is_occupied() {
        let mut memory = Memory::default();
        for (address, value) in [
            (100, 2),
            (101, 11),
            (102, 22),
            (103, 33),
            (104, 44),
            (105, 55),
        ] {
            memory.set_word(d(address), value);
        }
        let fifo = TransformAction {
            source: d(100),
            dest: d(200),
            ..action(32, 4)
        };
        fifo.execute(&mut memory);
        assert_eq!(word(&memory, 200), 11);
        assert_eq!(
            (100..=105).map(|i| word(&memory, i)).collect::<Vec<_>>(),
            [1, 22, 33, 44, 0, 55]
        );

        memory.set_word(d(100), 200);
        memory.set_word(d(101), 1);
        memory.set_word(d(20), 70);
        for (relay, value) in [
            (200, true),
            (201, false),
            (202, true),
            (203, true),
            (204, true),
        ] {
            memory.set_system(relay, value);
        }
        let fifo = TransformAction {
            source: d(100),
            dest: d(20),
            ..action(35, 4)
        };
        fifo.execute(&mut memory);
        assert!(memory.system_bit(70));
        assert_eq!(
            (200..=204)
                .map(|i| memory.system_bit(i))
                .collect::<Vec<_>>(),
            [false, true, true, false, true]
        );
        assert_eq!(word(&memory, 101), 0);
    }

    #[test]
    fn current_word_queue_reloads_headers_after_alias_writes() {
        let mut memory = Memory::default();
        memory.set_word(d(100), 9);
        memory.set_word(d(101), 12);
        action(31, 3).execute(&mut memory);
        assert_eq!(word(&memory, 100), 4, "full pointer is normalized");
        assert_eq!(word(&memory, 101), 12);
        memory.set_word(d(100), 0);
        let same = TransformAction {
            source: d(100),
            ..action(31, 3)
        };
        same.execute(&mut memory);
        assert_eq!([word(&memory, 100), word(&memory, 101)], [2, 1]);

        for (address, value) in [(100, 2), (101, i16::MIN), (102, 9), (103, 8)] {
            memory.set_word(d(address), value);
        }
        let fifo = TransformAction {
            source: d(100),
            dest: d(100),
            ..action(32, 3)
        };
        fifo.execute(&mut memory);
        assert_eq!(
            (100..=103).map(|i| word(&memory, i)).collect::<Vec<_>>(),
            [32767, 9, 8, 0]
        );

        for (address, value) in [(99, 55), (100, 3), (101, 11), (102, -1), (103, 66)] {
            memory.set_word(d(address), value);
        }
        let lifo = TransformAction {
            source: d(100),
            dest: d(100),
            ..action(33, 3)
        };
        let mut observed = BTreeSet::new();
        lifo.observed_devices(&memory, &mut observed);
        assert!(
            observed.contains(&d(99)),
            "signed second header controls clear"
        );
        assert!(
            lifo.validate_for_profile(&memory, NativeTransformProfile::CurrentEs2)
                .is_ok()
        );
        lifo.execute(&mut memory);
        assert_eq!(
            (99..=103).map(|i| word(&memory, i)).collect::<Vec<_>>(),
            [0, -1, 11, -1, 66]
        );
    }

    #[test]
    fn current_bit_lifo_decrements_an_aliased_destination_index_before_copying() {
        let mut memory = Memory::default();
        memory.set_word(d(100), 200);
        memory.set_word(d(101), 2);
        memory.set_system(200, true);
        memory.set_system(201, true);
        for flag in 1020..=1022 {
            memory.set_system(flag, true);
        }
        let lifo = TransformAction {
            source: d(100),
            dest: d(101),
            ..action(36, 3)
        };
        let mut observed = BTreeSet::new();
        lifo.observed_devices(&memory, &mut observed);
        assert!(observed.contains(&Memory::system_device(1)));
        lifo.execute(&mut memory);
        assert_eq!(word(&memory, 101), 1);
        assert!(memory.system_bit(1) && !memory.system_bit(2));
        assert!(memory.system_bit(200) && !memory.system_bit(201));
        assert!((1020..=1022).all(|flag| memory.system_bit(flag)));
        assert!(!memory.system_bit(1067) && !memory.system_bit(1068));
    }

    #[test]
    fn current_preflight_follows_guard_exits_and_alias_derived_accesses() {
        let profile = NativeTransformProfile::CurrentEs2;
        let mut memory = Memory::default();
        memory.data_register_limit = Some(100);
        memory.set_word(d(100), 9);
        let push = action(31, 3);
        assert!(push.validate_for_profile(&memory, profile).is_ok());
        push.execute(&mut memory);
        assert_eq!(word(&memory, 100), 4);
        assert!(memory.execution_limitations.is_empty());
        memory.set_word(d(100), 1);
        assert!(push.validate_for_profile(&memory, profile).is_err());
        push.execute(&mut memory);
        assert_eq!(word(&memory, 100), 1);
        assert!(!memory.execution_limitations.is_empty());

        memory.data_register_limit = Some(120);
        memory.set_word(d(100), 2);
        memory.set_word(d(101), 25);
        let lifo = TransformAction {
            source: d(100),
            dest: d(100),
            ..action(33, 3)
        };
        assert!(
            lifo.validate_for_profile(&memory, profile).is_err(),
            "alias would clear D125"
        );
        memory.set_word(d(101), -101);
        assert!(
            lifo.validate_for_profile(&memory, profile).is_err(),
            "alias would clear D-1"
        );

        memory.data_register_limit = Some(9999);
        memory.relay_limit = Some(70);
        memory.set_word(d(0), 50);
        memory.set_word(d(100), 70);
        memory.set_word(d(101), 0);
        assert!(action(34, 2).validate_for_profile(&memory, profile).is_ok());
        memory.set_word(d(101), 1);
        assert!(
            action(34, 2)
                .validate_for_profile(&memory, profile)
                .is_err()
        );
        memory.set_word(d(100), 68);
        memory.set_word(d(101), 1);
        memory.set_word(d(20), 50);
        let fifo = TransformAction {
            source: d(100),
            dest: d(20),
            ..action(35, 4)
        };
        assert!(
            fifo.validate_for_profile(&memory, profile).is_err(),
            "full capacity reads M71"
        );
    }

    #[test]
    fn unconfigured_model_services_hold_outputs_without_inventing_plc_errors() {
        let mut memory = Memory::default();
        memory.set_word(d(100), 12345);
        for mode in [39, 52] {
            memory.execution_limitations.clear();
            action(mode, 1).execute(&mut memory);
            assert_eq!(word(&memory, 100), 12345);
            assert!(!memory.execution_limitations.is_empty());
            assert!(!memory.system_bit(1067));
            assert!(!memory.system_bit(1068));
        }
        memory.transform_context.serial_number = Some(b"123456789012345678".to_vec());
        let identity = TransformAction {
            dest: d(9990),
            ..action(39, 1)
        };
        memory.data_register_limit = None;
        identity.execute(&mut memory);
        assert_eq!(word(&memory, 9990), 18);
        assert_eq!(word(&memory, 9999) as u16, 0x3738);
        memory.execution_limitations.clear();
        memory.transform_context.serial_number = Some(vec![b'A'; 19]);
        memory.set_word(d(100), 12345);
        action(39, 1).execute(&mut memory);
        assert_eq!(word(&memory, 100), 12345);
        assert!(!memory.execution_limitations.is_empty());
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    mod solar_tests {
        use super::*;

        fn input() -> Memory {
            let mut memory = Memory::default();
            write_double(&mut memory, 0, 121.301f32.to_bits());
            write_double(&mut memory, 2, 24.993f32.to_bits());
            for (index, value) in [(4, 8), (5, 0), (6, 18), (7, 1), (8, 1)] {
                memory.set_word(d(index), value);
            }
            memory
        }

        fn output(memory: &Memory, base: u16) -> [i16; 6] {
            std::array::from_fn(|offset| word(memory, base + offset as u16))
        }

        #[test]
        fn built_in_solar_matches_manual_preserves_flags_and_ignores_reserved_count() {
            let mut memory = input();
            for (flag, value) in [
                (1020, true),
                (1021, false),
                (1022, true),
                (1067, true),
                (1068, false),
            ] {
                memory.set_system(flag, value);
            }
            memory.set_word(d(1067), 123);
            memory.set_word(d(1068), 456);
            assert!(action(15, 1).validate_installed_execution(&memory).is_err());
            assert!(
                action(15, 1)
                    .validate_for_profile(&memory, NativeTransformProfile::CurrentEs2)
                    .is_ok()
            );
            action(15, 1).execute(&mut memory);
            let actual = output(&memory, 100);
            let seconds =
                |v: &[i16]| i32::from(v[0]) * 3600 + i32::from(v[1]) * 60 + i32::from(v[2]);
            assert!((seconds(&actual[..3]) - seconds(&[6, 39, 47])).abs() <= 3);
            assert!((seconds(&actual[3..]) - seconds(&[17, 16, 42])).abs() <= 3);
            for count in [-32768, 0, 256, 32767] {
                action(15, count).execute(&mut memory);
                assert_eq!(output(&memory, 100), actual);
            }
            for (flag, value) in [
                (1020, true),
                (1021, false),
                (1022, true),
                (1067, true),
                (1068, false),
            ] {
                assert_eq!(memory.system_bit(flag), value);
            }
            assert_eq!([word(&memory, 1067), word(&memory, 1068)], [123, 456]);
            assert_eq!(memory.native_numeric_scratch, Some(0));
            assert!(memory.execution_limitations.is_empty());
        }

        #[test]
        fn solar_snapshots_geography_before_overlapping_sunrise_stores() {
            let mut ordinary = input();
            action(15, 1).execute(&mut ordinary);
            let expected = output(&ordinary, 100);
            let mut aliased = input();
            let transform = TransformAction {
                dest: d(0),
                ..action(15, 1)
            };
            assert!(
                transform
                    .validate_for_profile(&aliased, NativeTransformProfile::CurrentEs2)
                    .is_ok()
            );
            transform.execute(&mut aliased);
            assert_eq!(output(&aliased, 0), expected);
            assert_eq!(
                [word(&aliased, 6), word(&aliased, 7), word(&aliased, 8)],
                [18, 1, 1]
            );
        }

        #[test]
        fn solar_reloads_sunset_date_after_sunrise_overwrites_day() {
            let mut ordinary = input();
            action(15, 1).execute(&mut ordinary);
            let first_date = output(&ordinary, 100);
            let mut changed_date = input();
            changed_date.set_word(d(8), first_date[0]);
            action(15, 1).execute(&mut changed_date);
            let second_date = output(&changed_date, 100);
            assert_ne!(&first_date[3..], &second_date[3..]);
            let mut aliased = input();
            let transform = TransformAction {
                dest: d(8),
                ..action(15, 1)
            };
            assert!(
                transform
                    .validate_for_profile(&aliased, NativeTransformProfile::CurrentEs2)
                    .is_ok()
            );
            transform.execute(&mut aliased);
            let actual = output(&aliased, 8);
            assert_eq!(&actual[..3], &first_date[..3]);
            assert_eq!(&actual[3..], &second_date[3..]);
        }

        #[test]
        fn solar_preflight_rejects_alias_created_invalid_month_without_partial_output() {
            let mut memory = input();
            for index in 9..12 {
                memory.set_word(d(index), 777);
            }
            let transform = TransformAction {
                dest: d(6),
                ..action(15, 1)
            };
            let before = output(&memory, 6);
            let error = transform
                .validate_for_profile(&memory, NativeTransformProfile::CurrentEs2)
                .unwrap_err();
            assert!(error.contains("month"));
            transform.execute(&mut memory);
            assert_eq!(output(&memory, 6), before);
            assert!(!memory.execution_limitations.is_empty());
            assert!(!memory.system_bit(1067) && !memory.system_bit(1068));
        }

        #[test]
        fn solar_preflight_bounds_spans_and_nonfinite_inputs() {
            let profile = NativeTransformProfile::CurrentEs2;
            for (source, dest, limit, valid) in [
                (9991, 9994, 9999, true),
                (9992, 100, 9999, false),
                (0, 9995, 9999, false),
                (4991, 4994, 4999, true),
                (4992, 100, 4999, false),
            ] {
                let seed = input();
                let mut memory = Memory::default();
                memory.data_register_limit = Some(limit);
                for offset in 0..9 {
                    memory.set_word(d(source + offset), word(&seed, offset));
                }
                let transform = TransformAction {
                    source: d(source),
                    dest: d(dest),
                    ..action(15, 1)
                };
                assert_eq!(
                    transform.validate_for_profile(&memory, profile).is_ok(),
                    valid
                );
            }
            for (longitude, latitude, month) in [
                (f32::NAN, 24.993, 1),
                (121.301, f32::INFINITY, 1),
                (181.0, 24.993, 1),
                (121.301, 91.0, 1),
                (121.301, 24.993, 0),
                (121.301, 24.993, 13),
            ] {
                let mut memory = input();
                write_double(&mut memory, 0, longitude.to_bits());
                write_double(&mut memory, 2, latitude.to_bits());
                memory.set_word(d(7), month);
                memory.set_word(d(100), 777);
                assert!(
                    action(15, 1)
                        .validate_for_profile(&memory, profile)
                        .is_err()
                );
                action(15, 1).execute(&mut memory);
                assert_eq!(word(&memory, 100), 777);
                assert!(!memory.system_bit(1067) && !memory.system_bit(1068));
            }
        }

        #[test]
        fn solar_polar_triplets_hold_independently() {
            for (latitude, month, rise_holds) in [(65.7289f32, 6, true), (67.3902f32, 12, false)] {
                let mut memory = input();
                write_double(&mut memory, 0, 0.0f32.to_bits());
                write_double(&mut memory, 2, latitude.to_bits());
                for (index, value) in [(4, 0), (6, 24), (7, month), (8, 21)] {
                    memory.set_word(d(index), value);
                }
                for offset in 0..6 {
                    memory.set_word(d(100 + offset), 777 + offset as i16);
                }
                action(15, 1).execute(&mut memory);
                let actual = output(&memory, 100);
                if rise_holds {
                    assert_eq!(&actual[..3], &[777, 778, 779]);
                    assert_ne!(&actual[3..], &[780, 781, 782]);
                } else {
                    assert_ne!(&actual[..3], &[777, 778, 779]);
                    assert_eq!(&actual[3..], &[780, 781, 782]);
                }
                assert!(memory.execution_limitations.is_empty());
                assert!(!memory.system_bit(1067) && !memory.system_bit(1068));
            }
        }
    }

    #[test]
    fn configured_services_receive_parameters_and_retain_virtual_clock_status() {
        fn solar(request: &SolarRequest) -> Result<SolarTimes, String> {
            assert_eq!(request.longitude.to_bits(), 121.301f32.to_bits());
            assert_eq!(request.latitude.to_bits(), 24.993f32.to_bits());
            assert_eq!(request.timezone_hours, 8);
            assert_eq!(request.date, [18, 1, 1]);
            Ok(SolarTimes {
                sunrise: [6, 39, 47],
                sunset: [17, 16, 42],
            })
        }
        fn ntp(request: &NtpRequest) -> Result<NtpReply, String> {
            assert_eq!(request.server, [192, 0, 2, 1]);
            assert_eq!([request.timezone_hours, request.timezone_minutes], [5, 30]);
            Ok(match request.update_minutes {
                60 => NtpReply {
                    status: NtpStatus::InProgress,
                    unix_seconds: None,
                },
                61 => NtpReply {
                    status: NtpStatus::Successful,
                    unix_seconds: Some(123456),
                },
                _ => NtpReply {
                    status: NtpStatus::Failed,
                    unix_seconds: Some(999),
                },
            })
        }
        fn failure(_: &NtpRequest) -> Result<NtpReply, String> {
            Err("fixture transport failure".into())
        }
        let mut memory = Memory::default();
        write_double(&mut memory, 0, 121.301f32.to_bits());
        write_double(&mut memory, 2, 24.993f32.to_bits());
        for (index, value) in [(4, 8), (5, 0), (6, 18), (7, 1), (8, 1)] {
            memory.set_word(d(index), value);
        }
        memory.transform_context.solar_backend = Some(solar);
        assert!(
            action(15, 1)
                .validate_for_profile(&memory, NativeTransformProfile::CurrentEs2)
                .unwrap_err()
                .contains("callback")
        );
        action(15, 1).execute(&mut memory);
        assert_eq!(
            [
                word(&memory, 100),
                word(&memory, 101),
                word(&memory, 102),
                word(&memory, 103),
                word(&memory, 104),
                word(&memory, 105)
            ],
            [6, 39, 47, 17, 16, 42]
        );
        memory.set_word(d(0), 0xc000u16 as i16);
        memory.set_word(d(1), 0x0201);
        memory.set_word(d(2), 60);
        memory.set_word(d(3), 5);
        memory.set_word(d(4), 30);
        memory.transform_context.ntp_backend = Some(ntp);
        action(52, 1).execute(&mut memory);
        assert_eq!(word(&memory, 100), 1);
        assert_eq!(memory.transform_context.synchronized_unix_seconds, None);
        memory.set_word(d(2), 61);
        action(52, 1).execute(&mut memory);
        assert_eq!(word(&memory, 100), 0);
        assert_eq!(
            memory.transform_context.synchronized_unix_seconds,
            Some(123456)
        );
        memory.set_word(d(2), 62);
        action(52, 1).execute(&mut memory);
        assert_eq!(word(&memory, 100), 2);
        assert_eq!(
            memory.transform_context.synchronized_unix_seconds,
            Some(123456)
        );
        memory.transform_context.ntp_backend = Some(failure);
        action(52, 1).execute(&mut memory);
        assert_eq!(word(&memory, 100), 2);
        assert!(
            memory
                .execution_limitations
                .iter()
                .any(|message| message.contains("fixture transport failure"))
        );
        assert!(!memory.system_bit(1067));
    }
}
