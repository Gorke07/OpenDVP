//! API343, documented byte strings. The installed Delta editor catalog has no
//! genuine encoding for this newer instruction. Byte order, snapshot aliasing,
//! and preservation after the terminator are explicit local policies.
use crate::plc::{Area, Device, Memory};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringMoveSource {
    Literal(Vec<u8>),
    Register(Device),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringMoveAction {
    pub source: StringMoveSource,
    pub dest: Device,
}

/// Physical WORD cursor, including the two halves of natural C200..C255.
/// This is a local string-memory policy, not a recovered $MOV handler.
fn cell(base: Device, offset: usize, m: &Memory) -> Option<(Device, bool)> {
    let offset = u16::try_from(offset).ok()?;
    match base.area {
        Area::T | Area::D => {
            let index = base.index.checked_add(offset)?;
            let limit = if base.area == Area::T {
                255
            } else {
                m.data_register_limit.unwrap_or(9999)
            };
            (index <= limit).then_some((
                Device {
                    area: base.area,
                    index,
                },
                false,
            ))
        }
        Area::C => {
            let first = if base.index < 200 {
                base.index
            } else {
                200 + 2 * (base.index - 200)
            };
            let index = first.checked_add(offset)?;
            if index < 200 {
                Some((
                    Device {
                        area: Area::C,
                        index,
                    },
                    false,
                ))
            } else if index < 312 {
                Some((
                    Device {
                        area: Area::C,
                        index: 200 + (index - 200) / 2,
                    },
                    !(index - 200).is_multiple_of(2),
                ))
            } else {
                None
            }
        }
        _ => None,
    }
}
fn read_cell(m: &Memory, d: Device, high: bool) -> u16 {
    if d.area == Area::C && d.index >= 200 {
        ((m.double_word(d) as u32) >> if high { 16 } else { 0 }) as u16
    } else {
        m.word(d) as u16
    }
}
fn write_cell(m: &mut Memory, d: Device, high: bool, word: u16) {
    if d.area == Area::C && d.index >= 200 {
        let old = m.double_word(d) as u32;
        let value = if high {
            (old & 0xffff) | (u32::from(word) << 16)
        } else {
            (old & 0xffff0000) | u32::from(word)
        };
        m.set_double_word(d, value as i32);
    } else {
        m.set_word(d, word as i16);
    }
}
fn register(text: &str) -> Result<Device, String> {
    let d: Device = text.parse()?;
    if !matches!(d.area, Area::T | Area::C | Area::D) {
        return Err("$MOV yalnız T/C/D kelime cihazlarını kabul eder".into());
    }
    Ok(d)
}
impl StringMoveAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if name != "$MOV" || args.len() != 2 {
            return Err("$MOV: kaynak ve hedef gerekli; P dış pulse kapısında uygulanır".into());
        }
        let source = if args[0].starts_with('"') {
            let literal = args[0]
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .ok_or("$MOV metni çift tırnakla kapatılmalı")?;
            if !(1..=16).contains(&literal.len())
                || !literal
                    .bytes()
                    .all(|b| (32..=126).contains(&b) && b != b'"')
            {
                return Err("$MOV sabiti 1..16 yazdırılabilir ASCII karakter olmalı; kaçış dizileri tanımlı değil".into());
            }
            StringMoveSource::Literal(literal.as_bytes().to_vec())
        } else {
            StringMoveSource::Register(register(&args[0])?)
        };
        Ok(Self {
            source,
            dest: register(&args[1])?,
        })
    }
    pub fn name(&self) -> &'static str {
        "$MOV"
    }
    /// Local logical layout only: two address descriptors, or one/two packed
    /// literal WORDs through eight WORDs, plus destination and header.
    pub fn layout_words(&self) -> u16 {
        match &self.source {
            StringMoveSource::Register(_) => 5,
            StringMoveSource::Literal(v) => 4 + (v.len() as u16).div_ceil(2),
        }
    }
    pub fn dest(&self) -> Device {
        self.dest
    }
    pub fn label(&self) -> String {
        let source = match &self.source {
            StringMoveSource::Literal(v) => format!("\"{}\"", String::from_utf8_lossy(v)),
            StringMoveSource::Register(d) => d.to_string(),
        };
        format!("$MOV {source} {}", self.dest)
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        out.insert(self.dest);
        if let StringMoveSource::Register(d) = &self.source {
            out.insert(*d);
        }
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let StringMoveSource::Register(d) = self.source {
            for i in 0..128 {
                if let Some((d, _)) = cell(d, i, m) {
                    out.insert(d);
                }
            }
        }
        for i in 0..128 {
            if let Some((d, _)) = cell(self.dest, i, m) {
                out.insert(d);
            }
        }
        out.extend([
            Memory::system_device(1067),
            Memory::system_device(1068),
            Device {
                area: Area::D,
                index: 1067,
            },
            Device {
                area: Area::D,
                index: 1068,
            },
        ]);
    }
    fn snapshot(&self, m: &Memory) -> Option<Vec<u8>> {
        match &self.source {
            StringMoveSource::Literal(v) => {
                if v.is_empty() || v.len() > 16 || !v.iter().all(|b| (32..=126).contains(b)) {
                    return None;
                }
                let mut bytes = v.clone();
                bytes.push(0);
                Some(bytes)
            }
            StringMoveSource::Register(source) => {
                let mut bytes = Vec::new();
                for i in 0..256 {
                    let (d, high) = cell(*source, i / 2, m)?;
                    let byte = (read_cell(m, d, high) >> (8 * (i % 2))) as u8;
                    bytes.push(byte);
                    if byte == 0 {
                        return Some(bytes);
                    }
                }
                None
            }
        }
    }
    /// All validation precedes writes. Overlap uses the pre-instruction source;
    /// the byte following a low-byte NUL retains its original destination value.
    pub fn execute(&self, m: &mut Memory, power: bool) {
        if !power {
            return;
        }
        let Some(bytes) = self.snapshot(m) else {
            m.operation_error(0xe1a, 0);
            return;
        };
        let Some(last) = bytes.len().checked_sub(1) else {
            return;
        };
        if cell(self.dest, last / 2, m).is_none() {
            m.operation_error(0xe1a, 0);
            return;
        }
        for (i, byte) in bytes.into_iter().enumerate() {
            let (d, high) = cell(self.dest, i / 2, m).expect("destination checked");
            let old = read_cell(m, d, high);
            let word = if i.is_multiple_of(2) {
                (old & 0xff00) | u16::from(byte)
            } else {
                (old & 0xff) | (u16::from(byte) << 8)
            };
            write_cell(m, d, high, word);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn action(source: &str, dest: &str) -> StringMoveAction {
        StringMoveAction::parse("$MOV", &[source.into(), dest.into()]).unwrap()
    }
    fn d(i: u16) -> Device {
        Device {
            area: Area::D,
            index: i,
        }
    }
    #[test]
    fn literal_nul_preserves_unused_byte_and_false_holds() {
        let mut m = Memory::default();
        m.set_word(d(11), 0x7f7f);
        let a = action("\"Ab\"", "D10");
        a.execute(&mut m, false);
        assert_eq!(m.word(d(10)), 0);
        a.execute(&mut m, true);
        assert_eq!(m.word(d(10)), 0x6241);
        assert_eq!(m.word(d(11)), 0x7f00);
    }
    #[test]
    fn overlap_snapshots_and_empty_register_string_copies_nul() {
        let mut m = Memory::default();
        m.set_word(d(0), 0x4241);
        m.set_word(d(1), 0x0043);
        action("D0", "D1").execute(&mut m, true);
        assert_eq!(m.word(d(1)), 0x4241);
        assert_eq!(m.word(d(2)), 0x0043);
        m.set_word(d(3), 0x5500);
        m.set_word(d(4), 0x7777);
        action("D3", "D4").execute(&mut m, true);
        assert_eq!(m.word(d(4)), 0x7700);
    }
    #[test]
    fn last_allowed_nul_and_complete_destination_guard() {
        let mut m = Memory::default();
        for i in 0..128 {
            m.set_word(d(i), 0x4141);
        }
        m.set_word(d(127), 0x0041);
        action("D0", "D200").execute(&mut m, true);
        assert_eq!(m.word(d(327)), 0x0041);
        m.set_word(d(127), 0x4141);
        m.set_word(d(400), 123);
        action("D0", "D400").execute(&mut m, true);
        assert_eq!(m.word(d(400)), 123);
        assert!(m.system_bit(1067));
        let mut m = Memory::default();
        m.data_register_limit = Some(4999);
        m.set_word(d(4999), 123);
        action("\"AB\"", "D4999").execute(&mut m, true);
        assert_eq!(m.word(d(4999)), 123);
        assert_eq!(m.word(d(1067)), 0xe1a);
    }
    #[test]
    fn natural_counter_halves_and_parser_permissions() {
        let mut m = Memory::default();
        let c: Device = "C255".parse().unwrap();
        m.set_double_word(c, 0x00434241);
        action("C255", "D10").execute(&mut m, true);
        assert_eq!(m.word(d(10)), 0x4241);
        assert_eq!(m.word(d(11)), 0x0043);
        for args in [
            ["\"\"", "D0"],
            ["\"é\"", "D0"],
            ["K1", "D0"],
            ["D0", "Y0"],
            ["'abc'", "D0"],
        ] {
            assert!(StringMoveAction::parse("$MOV", &args.map(String::from)).is_err());
        }
    }
}
