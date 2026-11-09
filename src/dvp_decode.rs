//! Delta editor `.dvp` program image (the editor's *internal* instruction form) → IL.
//!
//! A `.dvp` file does not hold PLC memory words (see `es2_program_decode` for those): the editor
//! stores its own form and converts it when downloading. The differences that matter:
//!
//! * AND/ANI/OR/ORI are single words (`0x40`/`0x50`/`0x60`/`0x70` families) instead of LD + ANB/ORB.
//! * Block words end in `FF` (`F8FF` ANB … `FCFF` MPP, `FDFF` INV, `F6FF` NP, `F7FF` PN, `FFFF` NOP).
//! * Edge contacts are their own instructions: `0101` LDP, `0201` LDF, `0301` ANDP, `0401` ANDF,
//!   `0501` ORP, `0601` ORF (PLS `0701`, PLF `0801` as on the PLC), each followed by a bit operand.
//! * A bit operand is `[(addr & 0xFF) << 8 | 0x84, (addr & 0xFF00) | 0x80 + 2·n]` (n = Kn digits,
//!   0 for a plain bit) instead of the PLC form's `0x84`/`0x84` tags.
//! * TMR targets carry tag `0x81` (`T` index in the high byte), CNT `0x83`.
//! * RST of T/C uses the device's bit address with word tags: `[idx << 8 | 0x86, 0x0682 | hi]` for
//!   T, `[idx << 8 | 0x86, 0x0E84 | hi]` for C; D uses the ordinary register operand.
//! * API words (`(0x10 + 2·API) << 8`, +0x100 for D, low byte 0x10 for P) and their word operands
//!   (`K` 0x80/0x80, `H` 0x82/0x80, `D` ×2 0x86/0x86, `T` ×2 0x86/0x82, `C` ×2 0x86/0x84, C200+
//!   `0x200 + 4·(n-200)`, `KnX…` bit form, `P` 0x88/0x80, `N` 0x88/0x82) are the same as on the PLC.
//!
//! The opcode words, instruction lengths and operand tags were read from the editor's own
//! instruction table and operand encoder (statically, Delta editor 2.53); the area nibbles of
//! single-word bit instructions match the PLC form. Anything this module does not know fails with
//! the word offset and value; nothing is skipped or approximated. The result is checked with
//! [`crate::plc::compile`].

use crate::plc::{Area, Device};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError {
    pub word_offset: usize,
    pub word: Option<u16>,
    pub message: String,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.word {
            Some(w) => write!(
                f,
                "DVP program WORD {} (0x{w:04X}): {}",
                self.word_offset, self.message
            ),
            None => write!(f, "DVP program WORD {}: {}", self.word_offset, self.message),
        }
    }
}

impl std::error::Error for DecodeError {}

/// One decoded instruction: where it starts, how many words it uses, its family and IL text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    pub offset: usize,
    pub len: usize,
    /// Opcode family, e.g. `"AND"`, `"block"`, `"edge"`, `"API"`, `"compare"`.
    pub family: &'static str,
    pub text: String,
}

struct Reader<'a> {
    words: &'a [u16],
}

impl Reader<'_> {
    fn get(&self, at: usize) -> Result<u16, DecodeError> {
        self.words.get(at).copied().ok_or(DecodeError {
            word_offset: at,
            word: None,
            message: "Eksik operand veya END".into(),
        })
    }
    fn fail(&self, at: usize, message: impl Into<String>) -> DecodeError {
        DecodeError {
            word_offset: at,
            word: self.words.get(at).copied(),
            message: message.into(),
        }
    }
}

/// PLC-style bit address (`0x400` X, `0x500` Y, `0x600` T, `0x800..` M, `0xE00` C,
/// `0x1000..` M1536+, `0..0x3FF` S) → device.
fn bit_device(address: u16) -> Option<Device> {
    let (area, index) = match address {
        0x0000..=0x03ff => (Area::S, address),
        0x0400..=0x04ff => (Area::X, address - 0x0400),
        0x0500..=0x05ff => (Area::Y, address - 0x0500),
        0x0600..=0x06ff => (Area::T, address - 0x0600),
        0x0800..=0x0dff => (Area::M, address - 0x0800),
        0x0e00..=0x0eff => (Area::C, address - 0x0e00),
        0x1000..=0x19ff => (Area::M, address - 0x1000 + 1536),
        _ => return None,
    };
    Some(Device { area, index })
}

/// Bit operand `[lo << 8 | 0x84, hi << 8 | 0x80 + 2n]` → `(device, n)`.
fn bit_operand(r: &Reader, at: usize) -> Result<(Device, u16), DecodeError> {
    let (w0, w1) = (r.get(at)?, r.get(at + 1)?);
    if w0 & 0xff != 0x84 || w1 & 0x80 == 0 || w1 & 0x61 != 0 {
        return Err(r.fail(at, "Bit operandı bekleniyordu"));
    }
    let digits = (w1 & 0x1f) >> 1;
    let address = (w1 & 0xff00) | (w0 >> 8);
    let device = bit_device(address)
        .ok_or_else(|| r.fail(at, format!("Bilinmeyen bit adresi 0x{address:04X}")))?;
    Ok((device, digits))
}

fn plain_bit(r: &Reader, at: usize) -> Result<Device, DecodeError> {
    match bit_operand(r, at)? {
        (device, 0) => Ok(device),
        _ => Err(r.fail(at, "Bu komut Kn grubu değil tek bit ister")),
    }
}

fn group_text(device: Device, digits: u16) -> String {
    if digits == 0 {
        device.to_string()
    } else {
        format!("K{digits}{device}")
    }
}

/// 16-bit word operand (two words).
fn word_operand(r: &Reader, at: usize) -> Result<String, DecodeError> {
    let (w0, w1) = (r.get(at)?, r.get(at + 1)?);
    let value = (w1 & 0xff00) | (w0 >> 8);
    let register = |area: Area, index: u16| Device { area, index }.to_string();
    Ok(match (w0 & 0xff, w1 & 0xff) {
        (0x80, 0x80) => format!("K{}", value as i16),
        (0x82, 0x80) => format!("H{value:X}"),
        (0x84, _) => {
            let (device, digits) = bit_operand(r, at)?;
            group_text(device, digits)
        }
        (0x86, 0x86) if value & 1 == 0 => register(Area::D, value / 2),
        (0x86, 0x82) if value & 1 == 0 => register(Area::T, value / 2),
        (0x86, 0x84) if value < 400 && value & 1 == 0 => register(Area::C, value / 2),
        (0x86, 0x84) if (0x200..0x200 + 56 * 4).contains(&value) && value & 3 == 0 => {
            register(Area::C, (value - 0x200) / 4 + 200)
        }
        (0x88, 0x80) => format!("P{value}"),
        (0x88, 0x82) => format!("N{value}"),
        tags => {
            return Err(r.fail(
                at,
                format!(
                    "Desteklenmeyen operand etiketi {:02X}/{:02X}",
                    tags.0, tags.1
                ),
            ));
        }
    })
}

/// 32-bit operand (four words): a word operand followed by `0x80`-tagged high bytes.
fn wide_operand(r: &Reader, at: usize) -> Result<String, DecodeError> {
    let (w0, w1, w2, w3) = (r.get(at)?, r.get(at + 1)?, r.get(at + 2)?, r.get(at + 3)?);
    if w2 & 0xff != 0x80 || w3 & 0xff != 0x80 {
        return Err(r.fail(at + 2, "32 bit operandın üst yarısı bekleniyordu"));
    }
    if (w0 & 0xff, w1 & 0xff) == (0x80, 0x80) || (w0 & 0xff, w1 & 0xff) == (0x82, 0x80) {
        let value = u32::from(w0 >> 8)
            | u32::from(w1 >> 8) << 8
            | u32::from(w2 >> 8) << 16
            | u32::from(w3 >> 8) << 24;
        return Ok(if w0 & 0xff == 0x80 {
            format!("K{}", value as i32)
        } else {
            format!("H{value:X}")
        });
    }
    if (w2, w3) != (0x0080, 0x0080) {
        return Err(r.fail(at + 2, "32 bit cihaz operandının üst yarısı sıfır olmalı"));
    }
    word_operand(r, at)
}

/// RST operand: bit devices in bit form, T/C by bit address with word tags, D as a register.
fn reset_operand(r: &Reader, at: usize) -> Result<String, DecodeError> {
    let (w0, w1) = (r.get(at)?, r.get(at + 1)?);
    let address = (w1 & 0xff00) | (w0 >> 8);
    match (w0 & 0xff, w1 & 0xff) {
        (0x84, 0x80) => Ok(plain_bit(r, at)?.to_string()),
        (0x86, 0x82) if (0x0600..=0x06ff).contains(&address) => Ok(Device {
            area: Area::T,
            index: address - 0x0600,
        }
        .to_string()),
        (0x86, 0x84) if (0x0e00..=0x0eff).contains(&address) => Ok(Device {
            area: Area::C,
            index: address - 0x0e00,
        }
        .to_string()),
        (0x86, 0x86) => word_operand(r, at),
        _ => Err(r.fail(at, "Desteklenmeyen RST operandı")),
    }
}

/// `(API number, mnemonic, operand count)`; D forms use four words per operand.
const APIS: &[(u16, &str, usize)] = &[
    (0, "CJ", 1),
    (1, "CALL", 1),
    (2, "SRET", 0),
    (3, "IRET", 0),
    (4, "EI", 0),
    (5, "DI", 0),
    (6, "FEND", 0),
    (7, "WDT", 0),
    (8, "FOR", 1),
    (9, "NEXT", 0),
    (10, "CMP", 3),
    (11, "ZCP", 4),
    (12, "MOV", 2),
    (13, "SMOV", 5),
    (14, "CML", 2),
    (15, "BMOV", 3),
    (16, "FMOV", 3),
    (17, "XCH", 2),
    (18, "BCD", 2),
    (19, "BIN", 2),
    (20, "ADD", 3),
    (21, "SUB", 3),
    (22, "MUL", 3),
    (23, "DIV", 3),
    (24, "INC", 1),
    (25, "DEC", 1),
    (26, "WAND", 3),
    (27, "WOR", 3),
    (28, "WXOR", 3),
    (29, "NEG", 1),
    (30, "ROR", 2),
    (31, "ROL", 2),
    (32, "RCR", 2),
    (33, "RCL", 2),
    (34, "SFTR", 4),
    (35, "SFTL", 4),
    (36, "WSFR", 4),
    (37, "WSFL", 4),
    (38, "SFWR", 3),
    (39, "SFRD", 3),
    (40, "ZRST", 2),
    (41, "DECO", 3),
    (42, "ENCO", 3),
    (43, "SUM", 2),
    (44, "BON", 3),
    (45, "MEAN", 3),
    (46, "ANS", 3),
    (47, "ANR", 0),
    (48, "SQR", 2),
    (49, "FLT", 2),
];

fn bit_family(nibble: u16) -> Option<(&'static str, &'static str)> {
    Some(match nibble {
        0x2 => ("LD", "LD"),
        0x3 => ("LDI", "LDI"),
        0x4 => ("AND", "AND"),
        0x5 => ("ANI", "ANI"),
        0x6 => ("OR", "OR"),
        0x7 => ("ORI", "ORI"),
        0xa => ("STL", "STL"),
        0xc => ("OUT", "OUT"),
        0xd => ("SET", "SET"),
        _ => return None,
    })
}

/// Single-word area nibble → bit address base.
fn single_word_base(nibble: u16) -> Option<u16> {
    Some(match nibble {
        0x0..=0x6 => nibble << 8,
        0x8..=0xe => nibble << 8,
        _ => return None,
    })
}

/// Decode the program words (END included) into an instruction listing.
pub fn decode_listing(words: &[u16]) -> Result<Vec<Instruction>, DecodeError> {
    let r = Reader { words };
    let mut out = Vec::new();
    let mut pc = 0;
    loop {
        let op = r.get(pc)?;
        let low = op & 0xff;
        let high = op >> 8;
        let push = |out: &mut Vec<Instruction>, len: usize, family: &'static str, text: String| {
            out.push(Instruction {
                offset: pc,
                len,
                family,
                text,
            });
            len
        };
        let len = match low {
            // Single-word bit instructions and P labels: (index << 8) | family << 4 | area.
            0x20..=0x7f | 0xa0..=0xaf | 0xc0..=0xdf => {
                let (family, mnemonic) = bit_family(low >> 4).expect("matched family");
                let base = single_word_base(low & 0x0f)
                    .ok_or_else(|| r.fail(pc, "Bilinmeyen alan kodu"))?;
                let device = bit_device(base | high).expect("single-word address");
                if mnemonic == "STL" && device.area != Area::S {
                    return Err(r.fail(pc, "STL yalnız S ile kullanılır"));
                }
                push(&mut out, 1, family, format!("{mnemonic} {device}"))
            }
            0xb0..=0xb7 => push(
                &mut out,
                1,
                "label",
                format!("P{}", ((low - 0xb0) << 8) | high),
            ),
            0xff => {
                let mnemonic = match high {
                    0xf8 => "ANB",
                    0xf9 => "ORB",
                    0xfa => "MPS",
                    0xfb => "MRD",
                    0xfc => "MPP",
                    0xfd => "INV",
                    0xf6 => "NP",
                    0xf7 => "PN",
                    0xff => "NOP",
                    _ => return Err(r.fail(pc, "Bilinmeyen blok komutu")),
                };
                push(&mut out, 1, "block", mnemonic.into())
            }
            0x01 => match high {
                0x01..=0x08 => {
                    let mnemonic = ["LDP", "LDF", "ANDP", "ANDF", "ORP", "ORF", "PLS", "PLF"]
                        [usize::from(high - 1)];
                    let device = plain_bit(&r, pc + 1)?;
                    push(&mut out, 3, "edge", format!("{mnemonic} {device}"))
                }
                0xd0..=0xfd => {
                    let prefix = match high & 0xf0 {
                        0xd0 => "LD",
                        0xe0 => "AND",
                        _ => "OR",
                    };
                    let relation = match high & 0x0e {
                        0x0 => "=",
                        0x2 => ">",
                        0x4 => "<",
                        0x8 => "<>",
                        0xa => "<=",
                        0xc => ">=",
                        _ => return Err(r.fail(pc, "Bilinmeyen karşılaştırma")),
                    };
                    let wide = high & 1 == 1;
                    let (a, b, len) = if wide {
                        (wide_operand(&r, pc + 1)?, wide_operand(&r, pc + 5)?, 9)
                    } else {
                        (word_operand(&r, pc + 1)?, word_operand(&r, pc + 3)?, 5)
                    };
                    let d = if wide { "D" } else { "" };
                    push(
                        &mut out,
                        len,
                        "compare",
                        format!("{d}{prefix}{relation} {a} {b}"),
                    )
                }
                _ => return Err(r.fail(pc, "Desteklenmeyen komut")),
            },
            // Long bit forms (M1536 and up).
            0x02 => {
                let mnemonic = match high {
                    0x02 => "LD",
                    0x03 => "LDI",
                    0x04 => "AND",
                    0x05 => "ANI",
                    0x06 => "OR",
                    0x07 => "ORI",
                    0x0c => "OUT",
                    0x0d => "SET",
                    _ => return Err(r.fail(pc, "Desteklenmeyen komut (indeksli/uzun biçim)")),
                };
                let device = plain_bit(&r, pc + 1)?;
                push(&mut out, 3, "long-bit", format!("{mnemonic} {device}"))
            }
            0x00 | 0x10 => match (high, low) {
                (0x0f, 0x00) => {
                    push(&mut out, 1, "END", "END".into());
                    return Ok(out);
                }
                (0x0e, 0x00) => push(&mut out, 1, "RET", "RET".into()),
                (0x0a | 0x0b, 0x00) => {
                    let n = word_operand(&r, pc + 1)?;
                    if !n.starts_with('N') {
                        return Err(r.fail(pc + 1, "MC/MCR N operandı bekleniyordu"));
                    }
                    let mnemonic = if high == 0x0a { "MC" } else { "MCR" };
                    push(&mut out, 3, "MC", format!("{mnemonic} {n}"))
                }
                (0x07, 0x00) => {
                    let target = reset_operand(&r, pc + 1)?;
                    push(&mut out, 3, "RST", format!("RST {target}"))
                }
                (0x03 | 0x05 | 0x06, 0x00) => {
                    let target = r.get(pc + 1)?;
                    let (area, tag, mnemonic) = match high {
                        0x03 => (Area::T, 0x81, "TMR"),
                        0x05 => (Area::C, 0x83, "CNT"),
                        _ => (Area::C, 0x83, "DCNT"),
                    };
                    if target & 0xff != tag {
                        return Err(r.fail(pc + 1, "Zamanlayıcı/sayaç numarası bekleniyordu"));
                    }
                    let device = Device {
                        area,
                        index: target >> 8,
                    };
                    let (preset, len) = if high == 0x06 {
                        (wide_operand(&r, pc + 2)?, 6)
                    } else {
                        (word_operand(&r, pc + 2)?, 4)
                    };
                    push(
                        &mut out,
                        len,
                        mnemonic,
                        format!("{mnemonic} {device} {preset}"),
                    )
                }
                (0x10..=0xff, _) => {
                    let api = (high - 0x10) / 2;
                    let wide = high & 1 == 1;
                    let &(_, name, count) = APIS
                        .iter()
                        .find(|(n, _, _)| *n == api)
                        .ok_or_else(|| r.fail(pc, format!("Desteklenmeyen API {api}")))?;
                    let mut text = match (wide, name) {
                        (true, "WAND" | "WOR" | "WXOR") => format!("D{}", &name[1..]),
                        (true, _) => format!("D{name}"),
                        (false, _) => name.to_owned(),
                    };
                    if low == 0x10 {
                        text.push('P');
                    }
                    let step = if wide { 4 } else { 2 };
                    for i in 0..count {
                        let at = pc + 1 + i * step;
                        let operand = if wide {
                            wide_operand(&r, at)?
                        } else {
                            word_operand(&r, at)?
                        };
                        text.push(' ');
                        text.push_str(&operand);
                    }
                    push(&mut out, 1 + count * step, "API", text)
                }
                _ => return Err(r.fail(pc, "Desteklenmeyen komut")),
            },
            _ => return Err(r.fail(pc, "Desteklenmeyen komut")),
        };
        pc += len;
    }
}

fn is_output(i: &Instruction) -> bool {
    matches!(
        i.family,
        "OUT" | "SET" | "RST" | "TMR" | "CNT" | "DCNT" | "API"
    ) || (i.family == "long-bit" && (i.text.starts_with("OUT ") || i.text.starts_with("SET ")))
        || (i.family == "edge" && (i.text.starts_with("PLS ") || i.text.starts_with("PLF ")))
}

fn is_series_contact(i: &Instruction) -> bool {
    matches!(i.family, "AND" | "ANI")
        || (i.family == "long-bit" && (i.text.starts_with("AND ") || i.text.starts_with("ANI ")))
        || (i.family == "edge" && i.text.starts_with("AND"))
        || (i.family == "compare" && (i.text.starts_with("AND") || i.text.starts_with("DAND")))
}

fn is_load(i: &Instruction) -> bool {
    matches!(i.family, "LD" | "LDI")
        || (matches!(i.family, "long-bit" | "edge") && i.text.starts_with("LD"))
        || (i.family == "compare" && (i.text.starts_with("LD") || i.text.starts_with("DLD")))
}

fn is_block_merge(i: &Instruction) -> bool {
    i.family == "block" && matches!(i.text.as_str(), "ANB" | "ORB")
}

/// Does the `LD` at `n` (right after an output) open a block that a later ANB/ORB merges with
/// the result from before the output? Counts open blocks from `n` on: reaching zero means an
/// ANB/ORB consumed the older result; an output while a block is still open means `n` began a
/// new rung.
fn load_continues_rung(listing: &[Instruction], n: usize) -> bool {
    let mut depth = 0i32;
    for i in &listing[n..] {
        if is_load(i) {
            depth += 1;
        } else if is_block_merge(i) {
            depth -= 1;
            if depth == 0 {
                return true;
            }
        } else if is_output(i) || matches!(i.family, "RET" | "END" | "label") || i.text.starts_with("STL ") {
            return false;
        }
    }
    false
}

/// The editor lets a rung continue after an output: in series (`LD a, OUT x, AND b, OUT y`) or
/// through a block merged with the earlier result (`LD a, OUT x, LD b, OR c, ANB, OUT y`). The
/// outputs leave the logic result unchanged, as the ladder's branch point does. OpenDVP IL asks
/// for that branch to be explicit, so the output run is wrapped in `MPS` … `MPP`
/// (`LD a, MPS, OUT x, MPP, AND b, OUT y`), which keeps the same result.
fn il_lines(listing: &[Instruction]) -> Vec<(usize, &str)> {
    let mut lines: Vec<(usize, &str)> = Vec::with_capacity(listing.len());
    let mut run_start: Option<usize> = None;
    for (n, i) in listing.iter().enumerate() {
        if is_output(i) {
            if run_start.is_none() {
                run_start = Some(lines.len());
            }
        } else if n > 0
            && is_output(&listing[n - 1])
            && (is_series_contact(i) || is_load(i) && load_continues_rung(listing, n))
        {
            let start = run_start.expect("an output run is open");
            lines.insert(start, (lines[start].0, "MPS"));
            lines.push((i.offset, "MPP"));
            run_start = None;
        } else {
            run_start = None;
        }
        lines.push((i.offset, &i.text));
    }
    lines
}

/// Compile the whole decoded IL once, so a decoded program is known to load, encode and
/// simulate as one piece. A compile error is reported at the word offset of its line.
fn check_compiles(lines: &[(usize, &str)]) -> Result<(), DecodeError> {
    let mut source = String::new();
    for (_, text) in lines {
        source.push_str(text);
        source.push('\n');
    }
    crate::plc::compile(&source).map(|_| ()).map_err(|e| DecodeError {
        word_offset: lines
            .get(e.line.saturating_sub(1))
            .map_or(0, |l| l.0),
        word: None,
        message: format!("Çözülen program derlenemedi: {}", e.message),
    })
}

/// Decode a `.dvp` program (END included, nothing after it) to IL and check that it compiles.
pub fn decode(words: &[u16]) -> Result<String, DecodeError> {
    let listing = decode_listing(words)?;
    let used: usize = listing.iter().map(|i| i.len).sum();
    if used != words.len() {
        return Err(DecodeError {
            word_offset: used,
            word: words.get(used).copied(),
            message: "END sonrasında WORD var".into(),
        });
    }
    let lines = il_lines(&listing);
    check_compiles(&lines)?;
    Ok(lines
        .iter()
        .map(|(_, t)| {
            format!("{t}\n")
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn il(words: &[u16]) -> String {
        decode(words).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn single_word_bit_families_and_areas() {
        let source = il(&[
            0xea2b, // LD M1002
            0x0074, // ORI X0
            0x0154, // ANI X1
            0x0548, // AND M5
            0x1160, // OR S17
            0x0336, // LDI T3
            0xf8ff, // ANB
            0x0749, // AND M263
            0x22c8, // OUT M34
            0x12c5, // OUT Y22 (raw index 18)
            0x58da, // SET M600
            0x0f00,
        ]);
        assert_eq!(
            source,
            "LD M1002\nORI X0\nANI X1\nAND M5\nOR S17\nLDI T3\nANB\nAND M263\nOUT M34\nOUT Y22\nSET M600\nEND\n"
        );
    }

    #[test]
    fn block_words_and_stack() {
        assert_eq!(
            il(&[
                0x0024, 0x0124, 0xf9ff, 0x0224, 0x0344, 0xf8ff, 0xfaff, 0x05c5, 0xfbff, 0x06c5,
                0xfcff, 0xfdff, 0x07c5, 0x0f00
            ]),
            "LD X0\nLD X1\nORB\nLD X2\nAND X3\nANB\nMPS\nOUT Y5\nMRD\nOUT Y6\nMPP\nINV\nOUT Y7\nEND\n"
        );
        assert_eq!(
            il(&[
                0x0024, 0xf6ff, 0x00c5, 0x0124, 0xf7ff, 0x01c5, 0xffff, 0x0f00
            ]),
            "LD X0\nNP\nOUT Y0\nLD X1\nPN\nOUT Y1\nNOP\nEND\n"
        );
    }

    #[test]
    fn edge_contacts_and_pulses() {
        assert_eq!(
            il(&[
                0x0101, 0x0184, 0x0480, // LDP X1
                0x0601, 0x0584, 0x0880, // ORF M5
                0x0301, 0x0b84, 0x0680, // ANDP T11
                0x0401, 0x0284, 0x0480, // ANDF X2
                0x0501, 0x0084, 0x0080, // ORP S0
                0x00c5, // OUT Y0
                0x0201, 0x0184, 0x0480, // LDF X1
                0x0701, 0x0284, 0x0880, // PLS M2
                0x0801, 0x0384, 0x0580, // PLF Y3
                0x0f00
            ]),
            "LDP X1\nORF M5\nANDP T11\nANDF X2\nORP S0\nOUT Y0\nLDF X1\nPLS M2\nPLF Y3\nEND\n"
        );
    }

    #[test]
    fn step_ladder_reset_timer_counter() {
        assert_eq!(
            il(&[
                0xea2b, // LD M1002
                0x6000, 0x0084, 0x0080, 0x7f84, 0x0080, // ZRST S0 S127
                0x6000, 0x0084, 0x0580, 0x1c84, 0x0580, // ZRST Y0 Y34
                0x0700, 0x0086, 0x0e84, // RST C0
                0x0700, 0x0586, 0x0682, // RST T5
                0x0700, 0x0484, 0x0880, // RST M4
                0x0700, 0x1486, 0x0086, // RST D10
                0x00d0, // SET S0
                0x00a0, // STL S0
                0x0024, // LD X0
                0x0300, 0x0c81, 0x0a80, 0x0080, // TMR T12 K10
                0x0300, 0x0581, 0xe886, 0x0386, // TMR T5 D500
                0x0500, 0x0083, 0x0280, 0x0080, // CNT C0 K2
                0x0c26, // LD T12
                0x14d0, // SET S20
                0x14a0, // STL S20
                0x0124, 0x00c0, // LD X1, OUT S0
                0x0e00, // RET
                0x0f00
            ]),
            "LD M1002\nZRST S0 S127\nZRST Y0 Y34\nRST C0\nRST T5\nRST M4\nRST D10\nSET S0\nSTL S0\nLD X0\nTMR T12 K10\nTMR T5 D500\nCNT C0 K2\nLD T12\nSET S20\nSTL S20\nLD X1\nOUT S0\nRET\nEND\n"
        );
    }

    #[test]
    fn api_and_comparison_operands() {
        assert_eq!(
            il(&[
                0xd001, 0xe886, 0x0386, 0x0080, 0x0080, // LD= D500 K0
                0xf001, 0xea86, 0x0386, 0x0180, 0x0080, // OR= D501 K1
                0xea01, 0x0a86, 0x0082, 0x3280, 0x0080, // AND<= T5 K50
                0x3a00, 0x2486, 0x0486, 0x0a86, 0x0082, 0x9286, 0x0186, // SUB D530 T5 D201
                0x4010, 0x2686, 0x0486, // INCP D531
                0x2800, 0xff80, 0xff80, 0x0886, 0x0086, // MOV K-1 D4
                0x2800, 0x3482, 0x1280, 0x0a86, 0x0086, // MOV H1234 D5
                0x2800, 0x2084, 0x0888, 0x0c86, 0x0086, // MOV K4M32 D6
                0x2900, 0x2180, 0x4380, 0x6580, 0x8780, 0x0086, 0x0284, 0x0080,
                0x0080, // DMOV
                0xd901, 0x1486, 0x0086, 0x0080, 0x0080, 0x6480, 0x0080, 0x0080,
                0x0080, // DLD<>
                0x00c5, 0x0f00
            ]),
            "LD= D500 K0\nOR= D501 K1\nAND<= T5 K50\nSUB D530 T5 D201\nINCP D531\nMOV K-1 D4\nMOV H1234 D5\nMOV K4M32 D6\nDMOV K-2023406815 C200\nDLD<> D10 K100\nOUT Y0\nEND\n"
        );
    }

    #[test]
    fn long_bit_forms_and_labels() {
        assert_eq!(
            il(&[
                0x0202, 0x0084, 0x1080, // LD M1536
                0x0402, 0x0184, 0x1080, // AND M1537
                0x0c02, 0xff84, 0x1980, // OUT M4095
                0x0024, 0x1000, 0x0088, 0x0080, // LD X0, CJ P0
                0x0124, 0x0a00, 0x0088, 0x0082, 0x0024, 0x01c5, 0x0b00, 0x0088,
                0x0082, // MC N0 .. MCR N0
                0x00b0, // P0
                0x0f00
            ]),
            "LD M1536\nAND M1537\nOUT M4095\nLD X0\nCJ P0\nLD X1\nMC N0\nLD X0\nOUT Y1\nMCR N0\nP0\nEND\n"
        );
    }

    #[test]
    fn series_contact_after_an_output_becomes_an_explicit_branch() {
        assert_eq!(
            il(&[
                0x0528, // LD M5
                0x04d8, // SET M4
                0x24c8, // OUT M36
                0x2458, // ANI M36
                0x22c8, // OUT M34
                0x0301, 0x1484, 0x0680, // ANDP T20
                0x14d0, // SET S20
                0x0f00
            ]),
            "LD M5\nMPS\nSET M4\nOUT M36\nMPP\nANI M36\nMPS\nOUT M34\nMPP\nANDP T20\nSET S20\nEND\n"
        );
    }

    #[test]
    fn unknown_or_malformed_words_fail_with_offset_and_value() {
        for (words, at, word) in [
            (vec![0x0024, 0xf8f0, 0x00c5, 0x0f00], 1, Some(0xf8f0)), // PLC-form ANB
            (vec![0x0024, 0x0027, 0x0f00], 1, Some(0x0027)),         // area nibble 7
            (vec![0x0024, 0x1209, 0x0f00], 1, Some(0x1209)),
            (
                vec![0x0024, 0x0700, 0x0184, 0x0584, 0x0f00],
                2,
                Some(0x0184),
            ), // RST K2Y1
            (
                vec![0x0024, 0x0300, 0x0c80, 0x0a80, 0x0080, 0x0f00],
                2,
                Some(0x0c80),
            ), // PLC TMR
            (vec![0x0024, 0xd001, 0xe886], 3, None), // truncated
            (vec![0x0024, 0x00c5], 2, None),         // no END
            (vec![0x0024, 0x0aa4, 0x0f00], 1, Some(0x0aa4)), // STL X12
            (vec![0x0024, 0x9000, 0x0f00], 1, Some(0x9000)), // API 64: not in the table
        ] {
            let error = decode(&words).unwrap_err();
            assert_eq!(
                (error.word_offset, error.word),
                (at, word),
                "{words:04x?}: {error}"
            );
            assert!(error.to_string().contains(&format!("WORD {at}")));
        }
        // Trailing words after END are rejected too.
        let error = decode(&[0x0f00, 0x0024]).unwrap_err();
        assert_eq!(error.word_offset, 1);
    }

    /// Internal form of a program, written by hand from the rules in the module docs, decodes
    /// back to the same IL; the IL then compiles and the PLC encoder accepts it.
    #[test]
    fn hand_encoded_program_round_trips_through_the_compiler() {
        let source = "LD X0\nOR M0\nANI X1\nOUT M0\nLD M0\nTMR T0 K50\nLDP X2\nORF X3\nCNT C1 K3\nLD T0\nAND C1\nOUT Y0\nLD X4\nRST C1\nRST T0\nEND\n";
        let words = [
            0x0024, 0x0068, 0x0154, 0x00c8, 0x0028, 0x0300, 0x0081, 0x3280, 0x0080, 0x0101, 0x0284,
            0x0480, 0x0601, 0x0384, 0x0480, 0x0500, 0x0183, 0x0380, 0x0080, 0x0026, 0x014e, 0x00c5,
            0x0424, 0x0700, 0x0186, 0x0e84, 0x0700, 0x0086, 0x0682, 0x0f00,
        ];
        assert_eq!(il(&words), source);
        let program = crate::plc::compile(source).unwrap();
        crate::es2_encoder::encode(&program).unwrap();
    }

    /// `LD a, OUT x, TMR …, LD b, OR c, ANB, OUT y`: the block after the outputs is merged with
    /// the result from before them, so the outputs are wrapped in MPS … MPP.
    #[test]
    fn block_after_an_output_continues_the_rung() {
        let source = il(&[
            0x0024, // LD X0
            0x01c8, // OUT M1
            0x0124, // LD X1
            0x0264, // OR X2
            0xf8ff, // ANB
            0x02c8, // OUT M2
            0x0324, // LD X3 (a new rung: its block is never merged below its level)
            0x0424, // LD X4
            0xf8ff, // ANB
            0x03c8, // OUT M3
            0x0f00, // END
        ]);
        assert_eq!(
            source,
            "LD X0\nMPS\nOUT M1\nMPP\nLD X1\nOR X2\nANB\nOUT M2\nLD X3\nLD X4\nANB\nOUT M3\nEND\n"
        );
    }

    /// A program far beyond the old 512-instruction cap decodes and compiles as one piece,
    /// including a block (`LD … ANB`) right after many single-output rungs.
    #[test]
    fn long_program_compiles_as_one_piece() {
        let mut words = Vec::new();
        for i in 0..900u16 {
            words.extend([0x0024, ((i % 256) << 8) | 0xc8]); // LD X0, OUT M(i mod 256)
        }
        // LD X0, AND X3, LD X1, OR X2, ANB, OUT Y1, END
        words.extend([0x0024, 0x0344, 0x0124, 0x0264, 0xf8ff, 0x01c5, 0x0f00]);
        let source = il(&words);
        assert_eq!(source.lines().count(), 1807);
        assert!(source.ends_with("LD X0\nAND X3\nLD X1\nOR X2\nANB\nOUT Y1\nEND\n"));
        crate::es2_encoder::encode(&crate::plc::compile(&source).unwrap()).unwrap();
    }

    /// Decode a real `.dvp` given by `OPENDVP_DVP_INPUT`; write IL, comments and an opcode
    /// histogram into `OPENDVP_DVP_OUTPUT_DIR`. Private projects stay outside the repository.
    #[test]
    #[ignore]
    fn decode_external_file() {
        let (Ok(input), Ok(dir)) = (
            std::env::var("OPENDVP_DVP_INPUT"),
            std::env::var("OPENDVP_DVP_OUTPUT_DIR"),
        ) else {
            return;
        };
        let dir = std::path::Path::new(&dir);
        let file = crate::dvp_file::read(&std::fs::read(&input).unwrap()).unwrap();
        let listing = decode_listing(&file.program).unwrap_or_else(|e| panic!("{e}"));
        let mut histogram = std::collections::BTreeMap::<String, (usize, usize)>::new();
        for i in &listing {
            let mnemonic = i.text.split(' ').next().unwrap();
            let key = format!("{:<9} {mnemonic}", i.family);
            let e = histogram.entry(key).or_default();
            e.0 += 1;
            e.1 += i.len;
        }
        let mut text = format!(
            "{} words, {} instructions\nfamily    mnemonic  count  words\n",
            file.program.len(),
            listing.len()
        );
        for (k, (n, w)) in &histogram {
            text.push_str(&format!("{k:<20} {n:>5} {w:>6}\n"));
        }
        std::fs::write(dir.join("opcode-histogram.txt"), text).unwrap();
        let listing: String = listing
            .iter()
            .map(|i| format!("{}\t{}\n", i.offset, i.text))
            .collect();
        std::fs::write(dir.join("listing.txt"), listing).unwrap();
        let source = decode(&file.program).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(dir.join("decoded.il"), &source).unwrap();
        let comments: String = file
            .device_comments
            .iter()
            .map(|(d, c)| format!("{d}\t{c}\n"))
            .collect();
        std::fs::write(dir.join("comments.tsv"), comments).unwrap();
        let project = crate::project::Project::load(std::path::Path::new(&input)).unwrap();
        println!(
            "project: {} comments of {}, {} source bytes",
            project.comments.len(),
            file.device_comments.len(),
            project.source.len()
        );
    }
}
