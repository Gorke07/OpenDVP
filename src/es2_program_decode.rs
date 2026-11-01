//! Bounded, loss-detecting decoding of ES2/SS2 program words to IL.
//!
//! This is deliberately narrower than the encoder. Unknown instructions and
//! noncanonical encodings fail rather than becoming editable approximations.
//! Program comments, original variable names, and original source formatting
//! are not part of the simulator word image and cannot be reconstructed.

use crate::es2_encoder::{self, MAX_PROGRAM_WORDS};
use crate::plc::{self, Area, Device};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError {
    pub word_offset: usize,
    pub message: String,
    pub kind: DecodeErrorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeErrorKind {
    Invalid,
    Incomplete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrefixDecode {
    Complete {
        source: String,
        words_consumed: usize,
    },
    Incomplete {
        word_offset: usize,
    },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Program WORD {}: {}", self.word_offset, self.message)
    }
}
impl std::error::Error for DecodeError {}

fn err(at: usize, text: impl Into<String>) -> DecodeError {
    DecodeError {
        word_offset: at,
        message: text.into(),
        kind: DecodeErrorKind::Invalid,
    }
}

fn incomplete(at: usize) -> DecodeError {
    DecodeError {
        word_offset: at,
        message: "Eksik komut, operand veya END".into(),
        kind: DecodeErrorKind::Incomplete,
    }
}

fn word(words: &[u16], at: usize) -> Result<u16, DecodeError> {
    words.get(at).copied().ok_or_else(|| incomplete(at))
}

fn bit_address(address: u16, at: usize) -> Result<Device, DecodeError> {
    let (area, index) = match address {
        0x0000..=0x03ff => (Area::S, address),
        0x0400..=0x04ff => (Area::X, address - 0x0400),
        0x0500..=0x05ff => (Area::Y, address - 0x0500),
        0x0600..=0x06ff => (Area::T, address - 0x0600),
        0x0800..=0x0dff => (Area::M, address - 0x0800),
        0x0e00..=0x0eff => (Area::C, address - 0x0e00),
        0x1000..=0x19ff => (Area::M, address - 0x1000 + 1536),
        _ => {
            return Err(err(
                at,
                format!("Desteklenmeyen bit adresi 0x{address:04X}"),
            ));
        }
    };
    let d = Device { area, index };
    // Reuse project restrictions, including unsupported system relays.
    d.to_string().parse::<Device>().map_err(|e| err(at, e))
}

fn bit_pair(words: &[u16], at: usize) -> Result<Device, DecodeError> {
    let low = word(words, at)?;
    if low & 0xff != 0x84 {
        return Err(err(at, "Geçersiz bit operandı"));
    }
    let high = word(words, at + 1)?;
    if high & 0xff != 0x84 {
        return Err(err(at, "Geçersiz bit operandı"));
    }
    bit_address((low >> 8) | (high & 0xff00), at)
}

fn numeric_pair(words: &[u16], at: usize) -> Result<String, DecodeError> {
    let low = word(words, at)?;
    if !matches!(low & 0xff, 0x80 | 0x86) {
        return Err(err(at, "Desteklenmeyen sayısal operand etiketi"));
    }
    let high = word(words, at + 1)?;
    let value = (low >> 8) | (high & 0xff00);
    let tags = (low & 0xff, high & 0xff);
    if tags == (0x80, 0x80) {
        return Ok(format!("K{}", value as i16));
    }
    if value & 1 != 0 {
        return Err(err(at, "Hizalanmamış sayısal cihaz operandı"));
    }
    if tags == (0x86, 0x80) {
        let alias = value / 2 + 1000;
        for index in 0..=7 {
            for high in [false, true] {
                let d =
                    crate::calendar_operand::CalendarIndices::alias_device(index, high).unwrap();
                if alias == d.index {
                    return Ok(format!("{}{index}", if high { "F" } else { "E" }));
                }
            }
        }
        return Err(err(at, "Geçersiz E/F sayısal aliası"));
    }
    let device = match tags {
        (0x86, 0x86) => Device {
            area: Area::D,
            index: value / 2,
        },
        (0x86, 0x82) => Device {
            area: Area::T,
            index: value / 2,
        },
        (0x86, 0x84) => Device {
            area: Area::C,
            index: value / 2,
        },
        _ => {
            return Err(err(
                at,
                format!("Desteklenmeyen sayısal operand etiketi {tags:02X?}"),
            ));
        }
    };
    device
        .to_string()
        .parse::<Device>()
        .map_err(|e| err(at, e))?;
    if device.area == Area::C && device.index >= 200 {
        return Err(err(at, "32 bit sayaç bu WORD operandında desteklenmez"));
    }
    Ok(device.to_string())
}

/// Decode one verified program from the beginning of a streamed WORD buffer.
/// END is recognized only while reading an instruction, never in an operand.
/// An incomplete result requests more input; malformed/unknown instructions
/// fail closed. A complete result explicitly reports how many WORDs were used,
/// leaving any unread trailing buffer to the caller. Accepted source must
/// compile and re-encode identically to that exact prefix.
pub fn decode_prefix(words: &[u16]) -> Result<PrefixDecode, DecodeError> {
    match decode_inner(words) {
        Ok((source, words_consumed)) => Ok(PrefixDecode::Complete {
            source,
            words_consumed,
        }),
        Err(error)
            if error.kind == DecodeErrorKind::Incomplete && words.len() < MAX_PROGRAM_WORDS =>
        {
            Ok(PrefixDecode::Incomplete {
                word_offset: error.word_offset,
            })
        }
        Err(error) if error.kind == DecodeErrorKind::Incomplete => {
            Err(err(words.len(), "WORD sınırı içinde END bulunamadı"))
        }
        Err(error) => Err(error),
    }
}

/// Decode an entire image. Unlike `decode_prefix`, trailing WORDs are rejected.
pub fn decode(words: &[u16]) -> Result<String, DecodeError> {
    match decode_prefix(words)? {
        PrefixDecode::Complete {
            source,
            words_consumed,
        } if words_consumed == words.len() => Ok(source),
        PrefixDecode::Complete { words_consumed, .. } => Err(err(
            words_consumed,
            "END sonrasında WORD var; kaynak otomatik açılamaz",
        )),
        PrefixDecode::Incomplete { word_offset } => Err(incomplete(word_offset)),
    }
}

fn decode_inner(words: &[u16]) -> Result<(String, usize), DecodeError> {
    if words.len() > MAX_PROGRAM_WORDS {
        return Err(err(0, "Program desteklenen WORD sınırını aşıyor"));
    }
    let mut lines = Vec::new();
    let mut pc = 0;
    let mut ended = false;
    while pc < words.len() {
        let op = words[pc];
        if op == 0x0f00 {
            lines.push("END".to_owned());
            pc += 1;
            ended = true;
            break;
        }
        let compact = (op & 0x00f0, op & 0x000f);
        if matches!(compact.0, 0x20 | 0x30 | 0xc0 | 0xd0) {
            let d = bit_address(((op & 0x000f) << 8) | (op >> 8), pc)?;
            let mnemonic = match compact.0 {
                0x20 => "LD",
                0x30 => "LDI",
                0xc0 => "OUT",
                _ => "SET",
            };
            lines.push(format!("{mnemonic} {d}"));
            pc += 1;
            continue;
        }
        if op & 0x00ff == 0x01 {
            let high = (op >> 8) as u8;
            let (prefix, offset) = match high {
                0xd0..=0xdc => ("LD", high - 0xd0),
                0xe0..=0xec => ("AND", high - 0xe0),
                0xf0..=0xfc => ("OR", high - 0xf0),
                _ => ("", 0xff),
            };
            let relation = match offset {
                0 => Some("="),
                2 => Some(">"),
                4 => Some("<"),
                8 => Some("<>"),
                10 => Some("<="),
                12 => Some(">="),
                _ => None,
            };
            if let Some(relation) = relation {
                let first = numeric_pair(words, pc + 1)?;
                let second = numeric_pair(words, pc + 3)?;
                lines.push(format!("{prefix}{relation} {first} {second}"));
                pc += 5;
                continue;
            }
        }
        match op {
            0x0202 | 0x0302 | 0x0c02 | 0x0d02 => {
                let d = bit_pair(words, pc + 1)?;
                let mnemonic = match op {
                    0x0202 => "LD",
                    0x0302 => "LDI",
                    0x0c02 => "OUT",
                    _ => "SET",
                };
                lines.push(format!("{mnemonic} {d}"));
                pc += 3;
            }
            0xf8f0 | 0xf9f0 | 0xfaf0 | 0xfbf0 | 0xfcf0 | 0xfdf0 | 0xf6f0 | 0xf7f0 => {
                lines.push(
                    match op {
                        0xf8f0 => "ANB",
                        0xf9f0 => "ORB",
                        0xfaf0 => "MPS",
                        0xfbf0 => "MRD",
                        0xfcf0 => "MPP",
                        0xfdf0 => "INV",
                        0xf6f0 => "NP",
                        _ => "PN",
                    }
                    .into(),
                );
                pc += 1;
            }
            0x0700 => {
                let low = word(words, pc + 1)?;
                if low & 0xff == 0x86 {
                    let target = numeric_pair(words, pc + 1)?;
                    if !target.starts_with(['D', 'E', 'F']) {
                        return Err(err(pc + 1, "Geçersiz sayısal RST hedefi"));
                    }
                    lines.push(format!("RST {target}"));
                    pc += 3;
                    continue;
                }
                if !matches!(low & 0xff, 0x80 | 0x84) {
                    return Err(err(pc + 1, "Geçersiz sıfırlama operandı"));
                }
                let high = word(words, pc + 2)?;
                let device = match (low & 0xff, high) {
                    (0x80, 0x0082) => Device {
                        area: Area::T,
                        index: low >> 8,
                    },
                    (0x80, 0x0084) => Device {
                        area: Area::C,
                        index: low >> 8,
                    },
                    _ => bit_pair(words, pc + 1)?,
                };
                lines.push(format!("RST {device}"));
                pc += 3;
            }
            0x0300 | 0x0500 => {
                let target = word(words, pc + 1)?;
                if target & 0xff != if op == 0x0500 { 0x83 } else { 0x80 } {
                    return Err(err(pc + 1, "Geçersiz zamanlayıcı/sayaç adresi"));
                }
                let device = Device {
                    area: if op == 0x0300 { Area::T } else { Area::C },
                    index: target >> 8,
                };
                let preset = numeric_pair(words, pc + 2)?;
                lines.push(format!(
                    "{} {device} {preset}",
                    if op == 0x0300 { "TMR" } else { "CNT" }
                ));
                pc += 4;
            }
            0x2800 | 0x3800 | 0x3a00 => {
                let first = numeric_pair(words, pc + 1)?;
                let (mnemonic, args, len) = if op == 0x2800 {
                    (
                        "MOV",
                        format!("{first} {}", numeric_pair(words, pc + 3)?),
                        5,
                    )
                } else {
                    (
                        if op == 0x3800 { "ADD" } else { "SUB" },
                        format!(
                            "{first} {} {}",
                            numeric_pair(words, pc + 3)?,
                            numeric_pair(words, pc + 5)?
                        ),
                        7,
                    )
                };
                lines.push(format!("{mnemonic} {args}"));
                pc += len;
            }
            0x0701 | 0x0801 => {
                let d = bit_pair(words, pc + 1)?;
                lines.push(format!("{} {d}", if op == 0x0701 { "PLS" } else { "PLF" }));
                pc += 3;
            }
            _ => return Err(err(pc, format!("Desteklenmeyen komut WORD 0x{op:04X}"))),
        }
    }
    if !ended {
        return Err(incomplete(pc));
    }
    let source = lines.join("\n") + "\n";
    let program =
        plc::compile(&source).map_err(|e| err(0, format!("Çözülen program doğrulanamadı: {e}")))?;
    let reencoded = es2_encoder::encode(&program)
        .map_err(|e| err(0, format!("Çözülen program yeniden kodlanamadı: {e}")))?;
    let used = &words[..pc];
    if reencoded.words != used {
        let at = reencoded
            .words
            .iter()
            .zip(used)
            .position(|(a, b)| a != b)
            .unwrap_or(reencoded.words.len().min(used.len()));
        return Err(err(
            at,
            "Program kayıpsız yeniden kodlanamadı; güvenli açma reddedildi",
        ));
    }
    Ok((source, pc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(source: &str) {
        let original = es2_encoder::encode(&plc::compile(source).unwrap()).unwrap();
        let recovered = decode(&original.words).unwrap();
        let again = es2_encoder::encode(&plc::compile(&recovered).unwrap()).unwrap();
        assert_eq!(original.words, again.words, "{recovered}");
    }

    #[test]
    fn verified_demo_and_branching_round_trip() {
        round_trip(crate::project::DEMO);
        round_trip("LD X0\nOR Y0\nANI X1\nOUT Y0\nEND");
        round_trip("LD X0\nMPS\nOUT Y0\nMRD\nOUT Y1\nMPP\nOUT Y2\nEND");
    }

    #[test]
    fn basic_address_arithmetic_and_reset_round_trip() {
        round_trip(
            "LDI M2000\nOUT M2001\nLD X0\nRST M0\nRST T1\nRST C2\nMOV K-1 D0\nADD D0 K1 D1\nSUB T2 C3 D9999\nEND",
        );
        round_trip("LD X0\nTMR T5 K300\nCNT C2 K5\nEND");
        round_trip("LD X0\nRST D10\nRST E1\nRST F7\nCNT C112 D10\nCNT C199 K0\nEND");
        round_trip("LD= D0 K2\nAND< D1 K3\nOR>= D2 K9\nOUT Y0\nEND");
    }

    #[test]
    fn address_and_numeric_boundaries_preserve_word_images() {
        for address in [
            "S0", "S1023", "X0", "X377", "Y0", "Y377", "M0", "M999", "M1535", "M1536", "M4095",
            "T0", "T255", "C0", "C255",
        ] {
            round_trip(&format!("LD {address}\nOUT Y0\nEND"));
        }
        for value in [-32768, -1, 0, 1, 255, 256, 3840, 32767] {
            round_trip(&format!("LD X0\nMOV K{value} D0\nEND"));
        }
    }

    #[test]
    fn reject_truncation_unknown_trailing_or_noncanonical_words() {
        for words in [
            vec![],
            vec![0x0024],
            vec![0x0202, 0x0084],
            vec![0x1234, 0x0f00],
            vec![0x0f00, 0],
            vec![0x0202, 0x0084, 0x0484, 0x00c5, 0x0f00],
        ] {
            assert!(decode(&words).is_err(), "{words:04x?}");
        }
    }

    #[test]
    fn end_marker_inside_operand_is_not_treated_as_program_end() {
        // MOV requires four operand WORDs. 0x0f00 inside that operand is not
        // a valid encoded operand and must not terminate the scan early.
        let malformed = [0x0024, 0x2800, 0x0f00, 0x0080, 0x0086, 0x0086, 0x0f00];
        let error = decode(&malformed).unwrap_err();
        assert_eq!(error.word_offset, 2);
        assert!(error.message.contains("operand"));
        // The same numerical bits in a valid immediate are not END either.
        round_trip("LD X0\nMOV K3840 D0\nEND");
    }

    #[test]
    fn malformed_logic_stacks_and_noncanonical_aliases_are_rejected() {
        for image in [
            vec![0x0024, 0xf8f0, 0x00c5, 0x0f00], // ANB lacks a second block
            vec![0x0024, 0xfcf0, 0x00c5, 0x0f00], // MPP without MPS
            vec![0x0024, 0x0124, 0x00c5, 0x0f00], // unjoined LD blocks
            vec![0x0202, 0x0084, 0x0484, 0x00c5, 0x0f00], // long form for compact X0
        ] {
            assert!(decode(&image).is_err(), "{image:04x?}");
        }
    }

    #[test]
    fn streamed_program_reports_every_incomplete_boundary_then_exact_length() {
        let source = format!("LD X0\n{}END\n", "MOV K123 D0\n".repeat(30));
        let words = es2_encoder::encode(&plc::compile(&source).unwrap())
            .unwrap()
            .words;
        assert!(words.len() > 100);
        for boundary in 0..words.len() {
            assert!(
                matches!(
                    decode_prefix(&words[..boundary]),
                    Ok(PrefixDecode::Incomplete { .. })
                ),
                "boundary {boundary}"
            );
        }
        let mut buffered = words.clone();
        buffered.extend([0xabcd, 0x0f00, 0xffff]);
        let PrefixDecode::Complete {
            source,
            words_consumed,
        } = decode_prefix(&buffered).unwrap()
        else {
            panic!("full buffer incomplete")
        };
        assert_eq!(words_consumed, words.len());
        assert_eq!(
            es2_encoder::encode(&plc::compile(&source).unwrap())
                .unwrap()
                .words,
            words
        );
        assert!(decode(&buffered).is_err());
    }

    #[test]
    fn streamed_program_rejects_unknown_or_invalid_before_end() {
        assert!(matches!(
            decode_prefix(&[]),
            Ok(PrefixDecode::Incomplete { word_offset: 0 })
        ));
        assert!(matches!(
            decode_prefix(&[0x0024, 0x2800, 0x0180]),
            Ok(PrefixDecode::Incomplete { .. })
        ));
        assert!(decode_prefix(&[0x0024, 0x2800, 0x0f00]).is_err());
        assert!(decode_prefix(&[0x0024, 0xdead, 0x0f00]).is_err());
        assert!(decode_prefix(&[0x0024, 0xf8f0, 0x00c5, 0x0f00]).is_err());
        assert!(decode_prefix(&vec![0x0024; MAX_PROGRAM_WORDS]).is_err());
    }

    #[test]
    fn arbitrary_partial_images_fail_safely_without_panicking() {
        let mut random = 0x92d4_0471u32;
        for length in 0..32 {
            for _ in 0..80 {
                let mut image = Vec::with_capacity(length);
                for _ in 0..length {
                    random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    image.push((random >> 8) as u16);
                }
                let decoded = std::panic::catch_unwind(|| decode(&image));
                assert!(decoded.is_ok(), "decoder panicked on {image:04x?}");
                if let Ok(source) = decoded.unwrap() {
                    let encoded = es2_encoder::encode(&plc::compile(&source).unwrap()).unwrap();
                    assert_eq!(encoded.words, image);
                }
                let prefix = std::panic::catch_unwind(|| decode_prefix(&image));
                assert!(prefix.is_ok(), "stream decoder panicked on {image:04x?}");
                if let Ok(PrefixDecode::Complete {
                    source,
                    words_consumed,
                }) = prefix.unwrap()
                {
                    let encoded = es2_encoder::encode(&plc::compile(&source).unwrap()).unwrap();
                    assert_eq!(encoded.words, image[..words_consumed]);
                }
            }
        }
    }
}
