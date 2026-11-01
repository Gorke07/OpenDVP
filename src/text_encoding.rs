//! Display/text encoding and byte checksums with instruction-specific roles.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::{CalendarDestination, CalendarValue};
use crate::plc::{Area, Device, Memory, Operand, word_range};
use std::collections::BTreeSet;

/// Exact current ES2/SS2 low-nibble segment lookup, shared by ordinary and
/// indexed bindings. Numeric writes replace the whole WORD, high byte zero.
pub(crate) fn segment_result(source: i16) -> i16 {
    const TABLE: [u16; 16] = [
        0x3f, 0x06, 0x5b, 0x4f, 0x66, 0x6d, 0x7d, 0x27, 0x7f, 0x6f, 0x77, 0x7c, 0x39, 0x5e, 0x79,
        0x71,
    ];
    TABLE[usize::from(source as u16 & 15)] as i16
}

#[derive(Debug, Clone, PartialEq)]
pub enum TextAction {
    Segment {
        source: CalendarValue,
        dest: CalendarDestination,
    },
    Literal {
        letters: [u8; 8],
        dest: Device,
    },
    Ascii {
        source: TextSource,
        dest: TextDestination,
        count: u16,
    },
    Hex {
        source: TextSource,
        dest: TextDestination,
        count: u16,
    },
    Check {
        source: TextSource,
        dest: TextDestination,
        count: Operand,
    },
    Lrc {
        source: Device,
        dest: Device,
        count: Operand,
    },
    Crc {
        source: Device,
        dest: Device,
        count: Operand,
    },
}

/// These text handlers advance packed operands by physical 16-bit slots,
/// independently of Kn. Kn limits the significant bits within each slot.
/// This is deliberately separate from the ordinary packed block stride.
#[derive(Debug, Clone, PartialEq)]
pub struct TextSource(AdditionalValue);

#[derive(Debug, Clone, PartialEq)]
pub struct TextDestination(AdditionalValue);

impl TextSource {
    fn parse(raw: &str, constants: bool) -> Result<Self, String> {
        let value = AdditionalValue::parse(raw, false, false)?;
        if !constants && value.base().is_none() {
            return Err("CCD kaynağı KnX/Y/M/S veya T/C/D olmalı".into());
        }
        Ok(Self(value))
    }
    pub fn value(&self) -> &AdditionalValue {
        &self.0
    }
    fn observe_words(&self, count: u16, m: &Memory, out: &mut BTreeSet<Device>) {
        observe_text(&self.0, count, false, m, out);
    }
}

impl TextDestination {
    fn parse(raw: &str, ccd: bool) -> Result<Self, String> {
        let value = AdditionalValue::parse(raw, false, true)?;
        if ccd && value.base().is_some_and(|d| d.area == Area::Y) {
            return Err("CCD hedefi KnM/S veya T/C/D olmalı".into());
        }
        Ok(Self(value))
    }
    pub fn value(&self) -> &AdditionalValue {
        &self.0
    }
    pub fn base(&self) -> Device {
        self.0.base().expect("validated text destination")
    }
    fn observe_words(&self, count: u16, m: &Memory, out: &mut BTreeSet<Device>) {
        observe_text(&self.0, count, true, m, out);
    }
}

impl std::fmt::Display for TextSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::fmt::Display for TextDestination {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

fn text_bit(base: Device, offset: u16, m: &Memory, write: bool) -> Option<Device> {
    let d = Device {
        index: base.index.checked_add(offset)?,
        ..base
    };
    (d.to_string().parse::<Device>().is_ok()
        && (d.area != Area::M || d.index <= m.relay_limit.unwrap_or(4095))
        && (!write || d.writable_bit()))
    .then_some(d)
}

fn text_word(value: &AdditionalValue, offset: u16, m: &Memory) -> u16 {
    match value {
        AdditionalValue::Word(source) => source_word(source, offset, m),
        AdditionalValue::Group { base, digits, .. } => (0..digits * 4).fold(0, |v, bit| {
            v | (u16::from(text_bit(*base, offset * 16 + bit, m, false).is_some_and(|d| m.bit(d)))
                << bit)
        }),
        _ => unreachable!("word-only text operand"),
    }
}

fn observe_text(
    value: &AdditionalValue,
    count: u16,
    writes: bool,
    m: &Memory,
    out: &mut BTreeSet<Device>,
) {
    match value {
        AdditionalValue::Word(Operand::Register(base)) => out.extend(
            (0..count).filter_map(|i| slot(*base, i, m.data_register_limit.unwrap_or(9999))),
        ),
        AdditionalValue::Group { base, digits, .. } => {
            let width = if writes { 16 } else { digits * 4 };
            out.extend((0..count).flat_map(|i| {
                (0..width).filter_map(move |bit| text_bit(*base, i * 16 + bit, m, writes))
            }));
        }
        _ => {}
    }
}

fn checked_dest(raw: &str, data_only: bool) -> Result<Device, String> {
    let d: Device = raw.parse()?;
    word_range(d, 1)?;
    if data_only && d.area != Area::D {
        return Err("Bu operand D yazmacı olmalı".into());
    }
    Ok(d)
}
fn count_operand(raw: &str, constant_only: bool) -> Result<Operand, String> {
    let n = Operand::parse(raw)?;
    match n {
        Operand::Constant(v) if !(1..=256).contains(&v) => {
            return Err("Uzunluk 1..256 aralığında olmalı".into());
        }
        Operand::Register(d) if constant_only || d.area != Area::D => {
            return Err(if constant_only {
                "Uzunluk K/H sabiti olmalı"
            } else {
                "Uzunluk K/H/D olmalı"
            }
            .into());
        }
        _ => {}
    }
    Ok(n)
}
fn slot(base: Device, offset: u16, data_limit: u16) -> Option<Device> {
    let d = Device {
        area: base.area,
        index: base.index.checked_add(offset)?,
    };
    (word_range(d, 1).is_ok() && (d.area != Area::D || d.index <= data_limit)).then_some(d)
}
fn source_word(source: &Operand, offset: u16, m: &Memory) -> u16 {
    match source {
        Operand::Constant(v) => *v as u16,
        Operand::Register(base) => slot(*base, offset, m.data_register_limit.unwrap_or(9999))
            .map(|d| m.word(d) as u16)
            .unwrap_or(0),
    }
}
fn write_word(m: &mut Memory, dest: Device, offset: u16, value: u16) {
    if let Some(d) = slot(dest, offset, m.data_register_limit.unwrap_or(9999)) {
        m.set_word(d, value as i16);
    }
}
/// Shared byte writer; indexed ASC supplies a previously sampled mode/base.
pub(crate) fn write_literal(m: &mut Memory, dest: Device, letters: &[u8; 8], low_only: bool) {
    write_bytes(m, dest, letters, low_only);
}
fn write_bytes(m: &mut Memory, dest: Device, values: &[u8], low_only: bool) {
    for (i, value) in values.iter().copied().enumerate() {
        let i = i as u16;
        if low_only {
            write_word(m, dest, i, u16::from(value));
        } else if let Some(d) = slot(dest, i / 2, m.data_register_limit.unwrap_or(9999)) {
            let old = m.word(d) as u16;
            m.set_word(
                d,
                if i.is_multiple_of(2) {
                    (old & 0xff00) | u16::from(value)
                } else {
                    (old & 0xff) | (u16::from(value) << 8)
                } as i16,
            );
        }
    }
}
pub(crate) fn nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}
pub(crate) fn hex_digit(n: u8) -> u8 {
    b"0123456789ABCDEF"[usize::from(n & 15)]
}

/// Shared CCD byte arithmetic; the ordered cursor engine owns reads and writes.
pub(crate) fn check_byte(sum: &mut u16, parity: &mut u16, byte: u8) {
    *sum = sum.wrapping_add(u16::from(byte));
    *parity ^= u16::from(byte);
}

/// Shared checksum arithmetic. Operand lifetimes, raw-word scratch publication,
/// errors and stores belong to the ordered caller rather than this accumulator.
pub(crate) enum ChecksumAccumulator {
    Lrc { sum: u8, high: Option<u8> },
    Crc(u16),
}
impl ChecksumAccumulator {
    pub(crate) fn lrc() -> Self {
        Self::Lrc { sum: 0, high: None }
    }
    pub(crate) fn crc() -> Self {
        Self::Crc(0xffff)
    }
    pub(crate) fn push_byte(&mut self, byte: u8) -> bool {
        match self {
            Self::Lrc { sum, high } => {
                let Some(value) = nibble(byte) else {
                    return false;
                };
                if let Some(first) = high.take() {
                    *sum = sum.wrapping_add((first << 4) | value);
                } else {
                    *high = Some(value);
                }
            }
            Self::Crc(crc) => {
                *crc ^= u16::from(byte);
                for _ in 0..8 {
                    *crc = if *crc & 1 != 0 {
                        (*crc >> 1) ^ 0xa001
                    } else {
                        *crc >> 1
                    };
                }
            }
        }
        true
    }
    pub(crate) fn push_packed_lrc(&mut self, word: u16) -> bool {
        let Self::Lrc { sum, high } = self else {
            return false;
        };
        // Native packed LRC validates the physical high byte first, although
        // the physical low byte supplies the decoded value's high nibble.
        let Some(low) = nibble((word >> 8) as u8) else {
            return false;
        };
        let Some(first) = nibble(word as u8) else {
            return false;
        };
        debug_assert!(high.is_none());
        *sum = sum.wrapping_add((first << 4) | low);
        true
    }
    pub(crate) fn result(&self, low_only: bool) -> [u16; 2] {
        match self {
            Self::Lrc { sum, high } => {
                debug_assert!(high.is_none());
                let check = sum.wrapping_neg();
                let first = u16::from(hex_digit(check >> 4));
                let second = u16::from(hex_digit(check));
                if low_only {
                    [first, second]
                } else {
                    [(first << 8) | second, 0]
                }
            }
            Self::Crc(crc) => {
                if low_only {
                    [u16::from(*crc as u8), *crc >> 8]
                } else {
                    [*crc, 0]
                }
            }
        }
    }
}

impl TextAction {
    /// Read-only ordered qualification. Defined PLC-error paths remain valid;
    /// only genuinely unqualified cursor/model paths return a limitation.
    pub fn validate_execution(&self, m: &Memory) -> Result<(), String> {
        if matches!(
            self,
            Self::Ascii { .. } | Self::Hex { .. } | Self::Check { .. }
        ) {
            let action = crate::indexed_text::IndexedTextAction::from_legacy(self)?;
            if let crate::indexed_text::TextOutcome::Limitation(reason) =
                action.qualify_native(m).outcome
            {
                return Err(reason);
            }
        }
        Ok(())
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if name == "ASC" {
            let (literal, target) = match args.len() {
                2 => (
                    args[0].trim_matches('"').trim_matches('\'').to_string(),
                    &args[1],
                ),
                9 if args[..8].iter().all(|s| s.len() == 1) => (args[..8].concat(), &args[8]),
                _ => return Err("ASC: sekiz harf ve hedef gerekli".into()),
            };
            if literal.len() != 8 || !literal.bytes().all(|c| c.is_ascii_alphabetic()) {
                return Err("ASC: A..Z/a..z aralığında sekiz harf gerekli".into());
            }
            return Ok(Self::Literal {
                letters: literal.as_bytes().try_into().unwrap(),
                dest: checked_dest(target, false)?,
            });
        }
        let expected = if name == "SEGD" { 2 } else { 3 };
        if args.len() != expected {
            return Err(format!("{name}: {expected} operand gerekli"));
        }
        Ok(match name {
            "SEGD" => Self::Segment {
                source: CalendarValue::parse(&args[0], false)?,
                dest: CalendarDestination::parse(&args[1], false)?,
            },
            "ASCI" | "ASCII" | "HEX" => {
                let source = TextSource::parse(&args[0], true)?;
                let dest = TextDestination::parse(&args[1], false)?;
                let Operand::Constant(n) = count_operand(&args[2], true)? else {
                    unreachable!()
                };
                if name == "HEX" {
                    Self::Hex {
                        source,
                        dest,
                        count: n as u16,
                    }
                } else {
                    Self::Ascii {
                        source,
                        dest,
                        count: n as u16,
                    }
                }
            }
            "CCD" => Self::Check {
                source: TextSource::parse(&args[0], false)?,
                dest: TextDestination::parse(&args[1], true)?,
                count: count_operand(&args[2], false)?,
            },
            "LRC" | "CRC" => {
                let source = checked_dest(&args[0], true)?;
                let count = count_operand(&args[1], false)?;
                let dest = checked_dest(&args[2], true)?;
                if name == "LRC" {
                    Self::Lrc {
                        source,
                        dest,
                        count,
                    }
                } else {
                    Self::Crc {
                        source,
                        dest,
                        count,
                    }
                }
            }
            _ => return Err("Bilinmeyen metin/sağlama komutu".into()),
        })
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Segment { .. } => "SEGD",
            Self::Literal { .. } => "ASC",
            Self::Ascii { .. } => "ASCI",
            Self::Hex { .. } => "HEX",
            Self::Check { .. } => "CCD",
            Self::Lrc { .. } => "LRC",
            Self::Crc { .. } => "CRC",
        }
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::Segment { dest, .. } => dest.base(),
            Self::Ascii { dest, .. } | Self::Hex { dest, .. } | Self::Check { dest, .. } => {
                dest.base()
            }
            Self::Literal { dest, .. } | Self::Lrc { dest, .. } | Self::Crc { dest, .. } => *dest,
        }
    }
    pub fn label(&self) -> String {
        let n = self.name();
        match self {
            Self::Segment { source, dest } => format!("{n} {source} {dest}"),
            Self::Literal { letters, dest } => {
                format!("{n} \"{}\" {dest}", String::from_utf8_lossy(letters))
            }
            Self::Ascii {
                source,
                dest,
                count,
            }
            | Self::Hex {
                source,
                dest,
                count,
            } => format!("{n} {source} {dest} K{count}"),
            Self::Check {
                source,
                dest,
                count,
            } => format!("{n} {source} {dest} {count}"),
            Self::Lrc {
                source,
                dest,
                count,
            }
            | Self::Crc {
                source,
                dest,
                count,
            } => format!("{n} {source} {count} {dest}"),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        out.insert(self.dest());
        out.insert(Memory::system_device(1161));
        out.extend((1020..=1022).map(Memory::system_device));
        Memory::error_devices(out);
        match self {
            Self::Segment { source, dest } => {
                source.devices(out);
                dest.devices(out);
            }
            Self::Ascii { source, dest, .. } | Self::Hex { source, dest, .. } => {
                source.value().devices(out);
                dest.value().devices(out);
            }
            Self::Check {
                source,
                dest,
                count,
            } => {
                source.value().devices(out);
                dest.value().devices(out);
                if let Operand::Register(d) = count {
                    out.insert(*d);
                }
            }
            Self::Lrc { source, count, .. } | Self::Crc { source, count, .. } => {
                out.insert(*source);
                if let Operand::Register(d) = count {
                    out.insert(*d);
                }
            }
            Self::Literal { .. } => {}
        }
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        let low_only = m.system_bit(1161);
        let byte_words = |count: u16| if low_only { count } else { count.div_ceil(2) };
        let runtime_count = |count: &Operand| {
            u16::try_from(count.read(m))
                .ok()
                .filter(|n| (1..=256).contains(n))
                .unwrap_or(0)
        };
        match self {
            Self::Segment { .. } => return,
            Self::Ascii {
                source,
                dest,
                count,
            } => {
                source.observe_words(count.div_ceil(4), m, out);
                dest.observe_words(byte_words(*count), m, out);
                return;
            }
            Self::Hex {
                source,
                dest,
                count,
            } => {
                source.observe_words(byte_words(*count), m, out);
                dest.observe_words(count.div_ceil(4), m, out);
                return;
            }
            Self::Check {
                source,
                dest,
                count,
            } => {
                source.observe_words(byte_words(runtime_count(count)), m, out);
                dest.observe_words(2, m, out);
                return;
            }
            _ => {}
        }
        let mut add_span = |base: Device, count: u16| {
            out.extend(
                (0..count).filter_map(|i| slot(base, i, m.data_register_limit.unwrap_or(9999))),
            );
        };
        let (source, reads, writes) = match self {
            Self::Literal { .. } => (None, 0, byte_words(8)),
            Self::Lrc { source, count, .. } | Self::Crc { source, count, .. } => (
                Some(Operand::Register(*source)),
                byte_words(runtime_count(count)),
                byte_words(2),
            ),
            _ => unreachable!("typed text spans handled above"),
        };
        add_span(self.dest(), writes);
        if let Some(Operand::Register(d)) = source {
            add_span(d, reads);
        }
    }
    pub fn execute(&self, m: &mut Memory) {
        if matches!(
            self,
            Self::Ascii { .. } | Self::Hex { .. } | Self::Check { .. }
        ) {
            match crate::indexed_text::IndexedTextAction::from_legacy(self) {
                Ok(action) => {
                    action.execute(m, true, &mut BTreeSet::new());
                }
                Err(reason) => {
                    m.execution_limitations.insert(reason);
                }
            }
            return;
        }
        if matches!(self, Self::Lrc { .. } | Self::Crc { .. }) {
            match crate::indexed_checksum::IndexedChecksumAction::from_legacy(self) {
                Ok(action) => {
                    action.execute(m, true, &mut BTreeSet::new());
                }
                Err(reason) => {
                    m.execution_limitations.insert(reason);
                }
            }
            return;
        }
        if let Err(reason) = self.validate_execution(m) {
            m.execution_limitations.insert(reason);
            return;
        }
        let low_only = m.system_bit(1161);
        match self {
            Self::Segment { source, dest } => {
                let value = segment_result(source.read(m) as i16);
                if dest.base().area != Area::D
                    || dest.base().index <= m.data_register_limit.unwrap_or(9999)
                {
                    dest.write(m, i32::from(value));
                }
            }
            Self::Literal { letters, dest } => write_literal(m, *dest, letters, low_only),
            Self::Ascii { .. } | Self::Hex { .. } | Self::Check { .. } => {
                unreachable!("ordered text handled above")
            }
            Self::Lrc { .. } | Self::Crc { .. } => unreachable!("ordered checksum handled above"),
        }
    }
    pub fn display_value(&self, m: &Memory) -> String {
        match self {
            Self::Segment { dest, .. } => dest.read(m).to_string(),
            Self::Ascii { dest, .. } | Self::Hex { dest, .. } | Self::Check { dest, .. } => {
                (text_word(dest.value(), 0, m) as i16).to_string()
            }
            _ => m.word(self.dest()).to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Layout assertions retain their byte-view helper; production checksums
    // now consume words in order without allocating an eager source snapshot.
    fn bytes(source: &Operand, count: u16, low_only: bool, m: &Memory) -> Vec<u8> {
        (0..count)
            .map(|i| {
                let word = source_word(source, if low_only { i } else { i / 2 }, m);
                (word >> if !low_only && i % 2 == 1 { 8 } else { 0 }) as u8
            })
            .collect()
    }
    fn d(raw: &str) -> Device {
        raw.parse().unwrap()
    }
    fn action(name: &str, args: &[&str]) -> TextAction {
        TextAction::parse(
            name,
            &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        )
        .unwrap()
    }
    #[test]
    fn manual_ascii_hex_examples_and_odd_packed_tail() {
        for mode in [false, true] {
            let mut m = Memory::default();
            m.set_system(1161, mode);
            m.set_word(d("D10"), 0x0123);
            m.set_word(d("D11"), 0x4567);
            m.set_word(d("D30"), 0x5a5a);
            action("ASCI", &["D10", "D20", "K6"]).execute(&mut m);
            assert_eq!(bytes(&Operand::Register(d("D20")), 6, mode, &m), b"670123");
            action("HEX", &["D20", "D40", "K6"]).execute(&mut m);
            assert_eq!(m.word(d("D40")) as u16, 0x0123);
            assert_eq!(m.word(d("D41")) as u16, 0x0067);
            action("ASCII", &["D10", "D30", "K1"]).execute(&mut m);
            assert_eq!(m.word(d("D30")) as u16, 0x0033);
        }
    }
    #[test]
    fn literal_keeps_case_and_uses_mode_layout() {
        for mode in [false, true] {
            let mut m = Memory::default();
            m.set_system(1161, mode);
            action("ASC", &["AbCdEfGh", "D10"]).execute(&mut m);
            assert_eq!(
                bytes(&Operand::Register(d("D10")), 8, mode, &m),
                b"AbCdEfGh"
            );
            if mode {
                assert_eq!(m.word(d("D17")), i16::from(b'h'));
            } else {
                assert_eq!(m.word(d("D13")) as u16, 0x6847);
            }
            action("ASC", &["A", "b", "C", "d", "E", "f", "G", "h", "D20"]).execute(&mut m);
            assert_eq!(
                bytes(&Operand::Register(d("D20")), 8, mode, &m),
                b"AbCdEfGh"
            );
        }
    }
    #[test]
    fn checksum_manual_examples_use_bytes_not_words_or_ascii_sum() {
        for mode in [false, true] {
            let mut m = Memory::default();
            m.set_system(1161, mode);
            write_bytes(&mut m, d("D10"), &[100, 111, 120, 202, 123, 211], mode);
            action("CCD", &["D10", "D100", "K6"]).execute(&mut m);
            assert_eq!(m.word(d("D100")), 867);
            assert_eq!(m.word(d("D101")), 0x11);
            write_bytes(&mut m, d("D20"), b"010307080006", mode);
            action("LRC", &["D20", "K12", "D110"]).execute(&mut m);
            assert_eq!(
                bytes(&Operand::Register(d("D110")), 2, mode, &m),
                if mode { b"E7" } else { b"7E" }
            );
            write_bytes(&mut m, d("D40"), &[1, 6, 7, 6, 0x17, 0x70], mode);
            action("CRC", &["D40", "K6", "D120"]).execute(&mut m);
            assert_eq!(
                bytes(&Operand::Register(d("D120")), 2, mode, &m),
                [0x66, 0xab]
            );
        }
    }
    #[test]
    fn transformations_observe_forward_reads_and_reverse_writes() {
        let mut m = Memory::default();
        m.set_word(d("D0"), 0x0123);
        m.set_word(d("D1"), 0x4567);
        action("ASCI", &["D0", "D1", "K8"]).execute(&mut m);
        assert_eq!(
            bytes(&Operand::Register(d("D1")), 8, false, &m),
            b"35670123"
        );
        action("HEX", &["D1", "D10", "K8"]).execute(&mut m);
        assert_eq!(m.word(d("D10")) as u16, 0x0123);
        assert_eq!(m.word(d("D11")) as u16, 0x3567);
        for i in 0..4 {
            m.set_word(d(&format!("D{i}")), 0x4443);
        }
        action("HEX", &["D0", "D0", "K8"]).execute(&mut m);
        assert_eq!(m.word(d("D1")) as u16, 0xcdc0);
        assert_eq!(m.word(d("D1067")), 0xe1a);
    }
    #[test]
    fn model_boundary_errors_preserve_source_failure_and_partial_results() {
        for limit in [4999, 9999] {
            let mut m = Memory::default();
            m.data_register_limit = Some(limit);
            let last = d(&format!("D{limit}"));
            m.set_word(last, 0x3030);
            m.set_word(d("D20"), 1234);
            action("CCD", &[&format!("D{limit}"), "D20", "K4"]).execute(&mut m);
            assert_eq!(m.word(d("D20")), 1234);
            assert_eq!(m.word(d("D1068")), 5);
            m.set_system(1067, false);
            m.set_system(1068, false);
            action("CRC", &[&format!("D{limit}"), "K4", "D20"]).execute(&mut m);
            assert_eq!(m.word(d("D20")), 1234);
            assert_eq!(m.word(d("D1068")), 1);
            m.set_system(1067, false);
            m.set_system(1068, false);
            action("CRC", &[&format!("D{limit}"), "K3", "D20"]).execute(&mut m);
            assert_eq!(m.word(d("D20")) as u16, 0xa415);
            assert!(!m.system_bit(1067));
            m.set_word(d("D0"), 0x3030);
            action("CCD", &["D0", &format!("D{limit}"), "K2"]).execute(&mut m);
            assert_eq!(m.word(last), 96);
            assert_eq!(m.word(d("D1068")), 3);
            m.set_system(1067, false);
            m.set_system(1068, false);
            m.set_system(1161, true);
            m.set_word(d("D0"), i16::from(b'0'));
            m.set_word(d("D1"), i16::from(b'0'));
            action("LRC", &["D0", "K2", &format!("D{limit}")]).execute(&mut m);
            assert_eq!(m.word(last), i16::from(b'0'));
            assert_eq!(m.word(d("D1068")), 5);
        }
    }
    #[test]
    fn operand_matrices_and_dynamic_count_are_bounded() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        for (name, raw) in [
            ("ASC", vec!["Abc12345", "D0"]),
            ("ASC", vec!["Abcdefg", "D0"]),
            ("ASC", vec!["Abcdefgh", "Y0"]),
            ("ASCI", vec!["D0", "D10", "D20"]),
            ("HEX", vec!["D0", "D10", "K257"]),
            ("CCD", vec!["K1", "D10", "K4"]),
            ("CCD", vec!["D0", "D10", "T0"]),
            ("LRC", vec!["T0", "K4", "D10"]),
            ("CRC", vec!["D0", "K4", "C0"]),
        ] {
            assert!(
                TextAction::parse(name, &args(&raw)).is_err(),
                "{name} {raw:?}"
            );
        }
        let mut m = Memory::default();
        m.set_word(d("D20"), 0);
        m.set_word(d("D10"), 1234);
        action("CCD", &["D0", "D10", "D20"]).execute(&mut m);
        assert_eq!(m.word(d("D10")), 1234);
        assert!(m.system_bit(1067));
        assert_eq!(m.word(d("D1067")), 0xe1a);
    }
}
