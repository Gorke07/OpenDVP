//! Read-only access to arbitrary devices (bits and words) of a Delta PLC.
//!
//! Works over any [`PduLink`], so the same code reads the COMMGR simulator, a Modbus
//! TCP/RTU/ASCII link to a real controller, or a scripted test link. Only function
//! codes 01/02 (bits) and 03 (words) are used; nothing here writes to the PLC.
//!
//! Wire addresses follow the DVP map already used by the encoder and the simulator
//! bridge: X 0x0400, Y 0x0500, T 0x0600, M 0x0800 (M1536.. at 0xB000), C 0x0E00,
//! D 0x1000 (D4096.. at 0x9000). A 32-bit value is two consecutive device words,
//! low word first. Verified against the SS2 simulator; not yet against hardware.
use crate::es2_bridge::{bit_address, word_address};
use crate::plc::{Area, Device};
use crate::plc_download::PduLink;
use crate::st::DataType;
use std::collections::{BTreeMap, BTreeSet};

/// Most registers per FC03 request (the DVP limit).
const MAX_WORDS: u16 = 100;
/// Most bits per FC01/FC02 request; well inside every DVP frame limit.
const MAX_BITS: u16 = 256;
/// Unrequested addresses read to keep two nearby requests in one frame.
const WORD_GAP: u16 = 8;
const BIT_GAP: u16 = 16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i16),
    Dint(i32),
    Word(u16),
    Dword(u32),
    Real(f32),
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Bool(v) => write!(f, "{}", if *v { "TRUE" } else { "FALSE" }),
            Value::Int(v) => write!(f, "{v}"),
            Value::Dint(v) => write!(f, "{v}"),
            Value::Word(v) => write!(f, "16#{v:04X}"),
            Value::Dword(v) => write!(f, "16#{v:08X}"),
            Value::Real(v) => write!(f, "{v}"),
        }
    }
}

/// Wire addresses of the words that make up a value of `ty` starting at `device`.
fn word_addresses(device: Device, ty: DataType) -> Result<Vec<u16>, String> {
    // C200..C255 are 32-bit counters on DVP (ISPSoft's operand validator splits the counter
    // class at C200); their Modbus layout is not verified, so they are refused rather than
    // read as a 16-bit half.
    if device.area == Area::C && device.index >= 200 {
        return Err(format!(
            "{device}: C200 ve üstü 32-bit sayaçtır; Modbus adres düzeni henüz doğrulanmadı, okunamaz."
        ));
    }
    (0..ty.words() as u16)
        .map(|k| {
            let d = Device {
                area: device.area,
                index: device.index + k,
            };
            word_address(d).map_err(|e| format!("{d}: {e}"))
        })
        .collect()
}

fn exchange(
    link: &mut dyn PduLink,
    function: u8,
    start: u16,
    count: u16,
) -> Result<Vec<u8>, String> {
    let mut data = start.to_be_bytes().to_vec();
    data.extend(count.to_be_bytes());
    match link.transact(function, &data)? {
        Ok(reply) => Ok(reply),
        Err(code) => Err(format!(
            "Okuma {start:#06x}+{count}: PLC Modbus istisnası {code:#04x}"
        )),
    }
}

/// Merges sorted unique addresses into `(start, count)` runs.
fn runs(addresses: &BTreeSet<u16>, gap: u16, limit: u16) -> Vec<(u16, u16)> {
    let mut out: Vec<(u16, u16)> = Vec::new();
    for &a in addresses {
        match out.last_mut() {
            Some((start, count)) if a - *start < limit && a - (*start + *count - 1) <= gap + 1 => {
                *count = a - *start + 1;
            }
            _ => out.push((a, 1)),
        }
    }
    out
}

fn read_word_run(link: &mut dyn PduLink, start: u16, count: u16) -> Result<Vec<u16>, String> {
    let reply = exchange(link, 0x03, start, count)?;
    let bytes = usize::from(count) * 2;
    if reply.len() != bytes + 1 || usize::from(reply[0]) != bytes {
        return Err("Kayıt okuma yanıtının bayt sayısı istekle eşleşmiyor.".into());
    }
    Ok(reply[1..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
        .collect())
}

fn read_bit_run(
    link: &mut dyn PduLink,
    function: u8,
    start: u16,
    count: u16,
) -> Result<Vec<bool>, String> {
    let reply = exchange(link, function, start, count)?;
    let bytes = usize::from(count).div_ceil(8);
    if reply.len() != bytes + 1 || usize::from(reply[0]) != bytes {
        return Err("Bit okuma yanıtının bayt sayısı istekle eşleşmiyor.".into());
    }
    Ok((0..usize::from(count))
        .map(|i| reply[1 + i / 8] & (1 << (i % 8)) != 0)
        .collect())
}

/// A merged read that the PLC rejects (a gap crossed a hole in the address map) is
/// retried address by address, so unrelated holes never hide a valid request.
fn fetch<T: Copy>(
    addresses: &BTreeSet<u16>,
    gap: u16,
    limit: u16,
    mut read: impl FnMut(u16, u16) -> Result<Vec<T>, String>,
) -> Result<BTreeMap<u16, T>, String> {
    let mut out = BTreeMap::new();
    for (start, count) in runs(addresses, gap, limit) {
        match read(start, count) {
            Ok(values) => out.extend((start..).zip(values)),
            Err(_) if count > 1 => {
                for a in (start..start + count).filter(|a| addresses.contains(a)) {
                    out.insert(a, read(a, 1)?[0]);
                }
            }
            Err(e) => return Err(e),
        }
    }
    Ok(out)
}

/// Reads one value for every `(device, type)` pair, in order. Bit devices (X, Y, M, S,
/// T, C contacts) take `Bool`; `D`, `T` and `C` registers take the word types.
pub fn read_values(
    link: &mut dyn PduLink,
    wanted: &[(Device, DataType)],
) -> Result<Vec<Value>, String> {
    let mut word_plan: Vec<Vec<u16>> = Vec::new();
    let mut bit_plan: Vec<(u8, u16)> = Vec::new();
    let (mut word_set, mut x_set, mut other_bits) =
        (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    for &(device, ty) in wanted {
        if ty == DataType::Bool {
            if device.area == Area::D {
                return Err(format!("{device}: D kaydı BOOL olarak okunamaz."));
            }
            let address = bit_address(device).map_err(|e| format!("{device}: {e}"))?;
            let function = if device.area == Area::X { 2 } else { 1 };
            bit_plan.push((function, address));
            if function == 2 {
                &mut x_set
            } else {
                &mut other_bits
            }
            .insert(address);
        } else {
            if device.area != Area::D && device.area != Area::T && device.area != Area::C {
                return Err(format!(
                    "{device}: {ty} yalnız D, T veya C kaydında okunur."
                ));
            }
            let addresses = word_addresses(device, ty)?;
            word_set.extend(addresses.iter().copied());
            word_plan.push(addresses);
        }
    }
    let words = fetch(&word_set, WORD_GAP, MAX_WORDS, |s, c| {
        read_word_run(link, s, c)
    })?;
    let x_bits = fetch(&x_set, BIT_GAP, MAX_BITS, |s, c| {
        read_bit_run(link, 2, s, c)
    })?;
    let other = fetch(&other_bits, BIT_GAP, MAX_BITS, |s, c| {
        read_bit_run(link, 1, s, c)
    })?;
    let (mut next_word, mut next_bit) = (0, 0);
    let mut out = Vec::with_capacity(wanted.len());
    for &(_, ty) in wanted {
        if ty == DataType::Bool {
            let (function, address) = bit_plan[next_bit];
            next_bit += 1;
            let map = if function == 2 { &x_bits } else { &other };
            out.push(Value::Bool(map[&address]));
            continue;
        }
        let raw: Vec<u16> = word_plan[next_word].iter().map(|a| words[a]).collect();
        next_word += 1;
        out.push(decode(ty, &raw));
    }
    Ok(out)
}

/// Decodes the device words of a non-`BOOL` value (low word first).
pub fn decode(ty: DataType, raw: &[u16]) -> Value {
    let double = || u32::from(raw[0]) | u32::from(raw[1]) << 16;
    match ty {
        DataType::Int => Value::Int(raw[0] as i16),
        DataType::Word => Value::Word(raw[0]),
        DataType::Dint => Value::Dint(double() as i32),
        DataType::Dword => Value::Dword(double()),
        DataType::Real => Value::Real(f32::from_bits(double())),
        DataType::Bool => Value::Bool(raw[0] != 0),
    }
}

/// The same values from the local simulator's memory (no I/O, never fails).
pub fn read_memory(memory: &crate::plc::Memory, wanted: &[(Device, DataType)]) -> Vec<Value> {
    wanted
        .iter()
        .map(|&(device, ty)| {
            if ty == DataType::Bool {
                return Value::Bool(memory.bit(device));
            }
            let raw: Vec<u16> = (0..ty.words() as u16)
                .map(|k| {
                    memory.word(Device {
                        area: device.area,
                        index: device.index.saturating_add(k),
                    }) as u16
                })
                .collect();
            decode(ty, &raw)
        })
        .collect()
}

/// One row of a live-value list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Watch {
    /// `name`, or `name[i]` for an array element.
    pub label: String,
    pub device: Device,
    pub ty: DataType,
    /// One-based ST source line of the declaration.
    pub line: usize,
}

/// Array elements listed per array; longer arrays show their first elements only.
pub const MAX_ARRAY_ROWS: i32 = 16;

/// The live-value rows of an ST program: every symbol, arrays expanded element by
/// element. Hidden helper instances of inlined functions (`__fn_...`) are skipped.
pub fn watches(symbols: &[crate::st::StSymbol]) -> Vec<Watch> {
    let mut out = Vec::new();
    for s in symbols {
        if s.name.starts_with("__") || s.name.contains(".__") {
            continue;
        }
        let Ok(base) = s.address.parse::<Device>() else {
            continue;
        };
        let step = s.data_type.words() as i32;
        match s.array {
            None => out.push(Watch {
                label: s.name.clone(),
                device: base,
                ty: s.data_type,
                line: s.line,
            }),
            Some((lo, hi)) => {
                for i in lo..=hi.min(lo + MAX_ARRAY_ROWS - 1) {
                    let Some(index) =
                        u16::try_from(i64::from(base.index) + i64::from((i - lo) * step)).ok()
                    else {
                        break;
                    };
                    out.push(Watch {
                        label: format!("{}[{i}]", s.name),
                        device: Device {
                            area: base.area,
                            index,
                        },
                        ty: s.data_type,
                        line: s.line,
                    });
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serves FC03/FC01/FC02 from a fixed memory image and records every request.
    struct Image {
        words: BTreeMap<u16, u16>,
        bits: BTreeMap<u16, bool>,
        log: Vec<(u8, u16, u16)>,
        holes: BTreeSet<u16>,
    }
    impl PduLink for Image {
        fn transact(&mut self, f: u8, d: &[u8]) -> Result<Result<Vec<u8>, u8>, String> {
            let start = u16::from_be_bytes([d[0], d[1]]);
            let count = u16::from_be_bytes([d[2], d[3]]);
            self.log.push((f, start, count));
            if (start..start + count).any(|a| self.holes.contains(&a)) {
                return Ok(Err(2));
            }
            Ok(Ok(if f == 3 {
                let mut r = vec![(count * 2) as u8];
                for a in start..start + count {
                    r.extend(self.words.get(&a).copied().unwrap_or(0).to_be_bytes());
                }
                r
            } else {
                let mut r = vec![0u8; usize::from(count).div_ceil(8) + 1];
                r[0] = r.len() as u8 - 1;
                for i in 0..count {
                    if self.bits.get(&(start + i)).copied().unwrap_or(false) {
                        r[1 + usize::from(i / 8)] |= 1 << (i % 8);
                    }
                }
                r
            }))
        }
    }
    fn image() -> Image {
        Image {
            words: BTreeMap::new(),
            bits: BTreeMap::new(),
            log: Vec::new(),
            holes: BTreeSet::new(),
        }
    }
    fn dev(s: &str) -> Device {
        s.parse().unwrap()
    }

    #[test]
    fn typed_values_decode_low_word_first_and_batch_nearby_registers() {
        let mut link = image();
        link.words.insert(0x1000, 0xFFFE); // D0 = -2
        link.words.insert(0x1002, 0x4240); // D2/D3 = 1_000_000
        link.words.insert(0x1003, 0x000F);
        link.words.insert(0x1005, 0x0000); // D5/D6 = 1.5f32 = 0x3FC00000
        link.words.insert(0x1006, 0x3FC0);
        link.words.insert(0x1007, 0xBEEF);
        let wanted = [
            (dev("D0"), DataType::Int),
            (dev("D2"), DataType::Dint),
            (dev("D5"), DataType::Real),
            (dev("D7"), DataType::Word),
        ];
        let v = read_values(&mut link, &wanted).unwrap();
        assert_eq!(
            v,
            [
                Value::Int(-2),
                Value::Dint(1_000_000),
                Value::Real(1.5),
                Value::Word(0xBEEF)
            ]
        );
        assert_eq!(link.log, [(3, 0x1000, 8)], "one frame for D0..D7");
    }

    #[test]
    fn bit_areas_use_the_documented_function_codes_and_addresses() {
        let mut link = image();
        link.bits.insert(0x0400, true); // X0
        link.bits.insert(0x0507, true); // Y7
        link.bits.insert(0x0800 + 10, true); // M10
        link.bits.insert(0xB000, true); // M1536
        let wanted: Vec<_> = ["X0", "X1", "Y7", "M10", "M11", "M1536"]
            .iter()
            .map(|s| (dev(s), DataType::Bool))
            .collect();
        let v = read_values(&mut link, &wanted).unwrap();
        let on: Vec<_> = v.iter().map(|v| *v == Value::Bool(true)).collect();
        assert_eq!(on, [true, false, true, true, false, true]);
        assert!(link.log.contains(&(2, 0x0400, 2)), "{:?}", link.log);
        assert!(link.log.iter().all(|(f, ..)| matches!(f, 1 | 2)));
    }

    #[test]
    fn d4095_and_d4096_are_not_contiguous_on_the_wire() {
        let mut link = image();
        link.words.insert(0x1FFF, 0x0001);
        link.words.insert(0x9000, 0x0002);
        let v = read_values(&mut link, &[(dev("D4095"), DataType::Dint)]).unwrap();
        assert_eq!(v, [Value::Dint(0x0002_0001)]);
        assert_eq!(link.log, [(3, 0x1FFF, 1), (3, 0x9000, 1)]);
    }

    #[test]
    fn a_rejected_merged_read_falls_back_to_exact_addresses() {
        let mut link = image();
        link.words.insert(0x1000, 7);
        link.words.insert(0x1004, 9);
        link.holes.insert(0x1002);
        let wanted = [(dev("D0"), DataType::Int), (dev("D4"), DataType::Int)];
        let v = read_values(&mut link, &wanted).unwrap();
        assert_eq!(v, [Value::Int(7), Value::Int(9)]);
        assert_eq!(link.log.len(), 3);
    }

    #[test]
    fn invalid_requests_are_rejected_before_any_traffic() {
        let mut link = image();
        for wanted in [
            (dev("D0"), DataType::Bool),
            (dev("M0"), DataType::Int),
            (dev("C232"), DataType::Int),
            (dev("C200"), DataType::Int),
            (dev("D9999"), DataType::Dint),
        ] {
            assert!(read_values(&mut link, &[wanted]).is_err(), "{wanted:?}");
        }
        assert!(link.log.is_empty());
    }

    #[test]
    fn st_symbols_become_watches_and_read_from_simulator_memory() {
        let src = "PROGRAM P
VAR
 run AT Y0 : BOOL;
 n AT D20 : INT := -3;
 big : DINT := 70000;
 r : REAL := 2.5;
 a : ARRAY[1..3] OF INT := [4, 5, 6];
END_VAR
run := TRUE;
END_PROGRAM";
        let lowered = crate::st::lower(src).unwrap();
        let w = watches(&lowered.symbols);
        let labels: Vec<_> = w.iter().map(|w| w.label.as_str()).collect();
        assert_eq!(labels, ["RUN", "N", "BIG", "R", "A[1]", "A[2]", "A[3]"]);
        assert_eq!(w[1].line, 4);
        let program = crate::plc::compile_st(src).unwrap();
        let mut sim = crate::plc::Simulator::default();
        sim.scan(&program, 100);
        let wanted: Vec<_> = w.iter().map(|w| (w.device, w.ty)).collect();
        let v = read_memory(&sim.memory, &wanted);
        assert_eq!(
            v,
            [
                Value::Bool(true),
                Value::Int(-3),
                Value::Dint(70000),
                Value::Real(2.5),
                Value::Int(4),
                Value::Int(5),
                Value::Int(6)
            ]
        );
    }

    #[test]
    fn hidden_function_instances_are_not_listed() {
        let src = "FUNCTION F : INT
VAR_INPUT a : INT; END_VAR
F := a;
END_FUNCTION
PROGRAM P
VAR x : INT; END_VAR
x := F(1);
END_PROGRAM";
        let w = watches(&crate::st::lower(src).unwrap().symbols);
        assert_eq!(
            w.iter().map(|w| w.label.as_str()).collect::<Vec<_>>(),
            ["X"]
        );
    }

    #[test]
    fn a_real_exception_is_reported() {
        let mut link = image();
        link.holes.insert(0x1000);
        let e = read_values(&mut link, &[(dev("D0"), DataType::Int)]).unwrap_err();
        assert!(e.contains("0x02"), "{e}");
    }
}
