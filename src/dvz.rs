//! Delta EasyDownload/DVPtool `.dvz` package (Delta editor `PLC_CARD`).
//!
//! Layout recovered from DVPtool's writer (`routine`) and checked against a package the
//! tool produced from the Delta editor ES2 simulator; see
//! All integers are little-endian.
//!
//! | offset | content |
//! |---|---|
//! | 0x00 | u32 format value (0xD3 observed) |
//! | 0x09 | u16 DVPtool model index (ES2 = 8) |
//! | 0x44 | i32 sum of the program bytes as signed 8-bit values |
//! | 0x48 | 4 bytes, `b[i] = i - b[i]` encoding of a u32 (0xFFFFFFFF -> 01 02 03 04) |
//! | 0x68 | u32 program length in bytes |
//! | 0x100 | program words, big-endian, every byte XOR the low byte of the sum |
//!
//! After the program: five sections, each `u32 count` + data: D (u16), 16-bit C (u16),
//! 32-bit C (u32), T (u16), file registers (u16); then a u32 comment length (+ comment).
//! Password, PLC-ID and comment strings are not produced by [`write`].

const HEADER: usize = 0x100;
const FORMAT: u32 = 0xD3;
const MODEL_OFFSET: usize = 0x09;
const SUM_OFFSET: usize = 0x44;
const LENGTH_OFFSET: usize = 0x68;

/// DVPtool model indices (its `routine` switch), not the FC67 model codes.
pub const MODEL_INDICES: &[(&str, u16)] = &[
    ("ES2", 0x08),
    ("EX2", 0x09),
    ("SS2", 0x0F),
    ("SX2", 0x10),
    ("SA2", 0x11),
    ("MC", 0x12),
    ("SE", 0x13),
    ("EH", 0x14),
    ("EH2", 0x15),
    ("EH2-L", 0x18),
    ("EH3", 0x19),
    ("SV2", 0x1A),
    ("EH3-L", 0x1B),
    ("ES2-E", 0x1D),
    ("ES2-64K", 0x37),
];

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Package {
    pub model_index: u16,
    pub program: Vec<u16>,
    pub d: Vec<u16>,
    pub c16: Vec<u16>,
    pub c32: Vec<u32>,
    pub t: Vec<u16>,
    pub file_registers: Vec<u16>,
    pub comment: Vec<u8>,
}

impl Package {
    pub fn model_name(&self) -> Option<&'static str> {
        MODEL_INDICES
            .iter()
            .find(|(_, index)| *index == self.model_index)
            .map(|(name, _)| *name)
    }
}

fn program_sum(bytes: &[u8]) -> i32 {
    bytes.iter().map(|&b| i32::from(b as i8)).sum()
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}
impl Reader<'_> {
    fn take(&mut self, n: usize, what: &str) -> Result<&[u8], String> {
        let end = self
            .at
            .checked_add(n)
            .filter(|&end| end <= self.data.len())
            .ok_or_else(|| format!("DVZ dosyası {what} bölümünde kesik"))?;
        let slice = &self.data[self.at..end];
        self.at = end;
        Ok(slice)
    }
    fn u32(&mut self, what: &str) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4, what)?.try_into().unwrap()))
    }
    fn words(&mut self, what: &str) -> Result<Vec<u16>, String> {
        let count = self.u32(what)? as usize;
        let bytes = self.take(count.checked_mul(2).ok_or("DVZ sayacı taştı")?, what)?;
        Ok(bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|w| u16::from_le_bytes(*w))
            .collect())
    }
}

pub fn read(data: &[u8]) -> Result<Package, String> {
    if data.len() < HEADER {
        return Err("DVZ başlığı eksik".into());
    }
    let u32_at = |o: usize| u32::from_le_bytes(data[o..o + 4].try_into().unwrap());
    if u32_at(0) != FORMAT {
        return Err(format!("Tanınmayan DVZ biçim değeri {:#x}", u32_at(0)));
    }
    let length = u32_at(LENGTH_OFFSET) as usize;
    if !length.is_multiple_of(2) {
        return Err("DVZ program uzunluğu tek sayı".into());
    }
    let expected_sum = u32_at(SUM_OFFSET) as i32;
    let key = expected_sum as u8;
    let mut reader = Reader { data, at: HEADER };
    let program_bytes: Vec<u8> = reader
        .take(length, "program")?
        .iter()
        .map(|b| b ^ key)
        .collect();
    if program_sum(&program_bytes) != expected_sum {
        return Err("DVZ program sağlama toplamı tutmuyor".into());
    }
    let program = program_bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|w| u16::from_be_bytes(*w))
        .collect();
    let d = reader.words("D")?;
    let c16 = reader.words("C (16 bit)")?;
    let c32_count = reader.u32("C (32 bit)")? as usize;
    let c32 = reader
        .take(
            c32_count.checked_mul(4).ok_or("DVZ sayacı taştı")?,
            "C (32 bit)",
        )?
        .as_chunks::<4>()
        .0
        .iter()
        .map(|w| u32::from_le_bytes(*w))
        .collect();
    let t = reader.words("T")?;
    let file_registers = reader.words("dosya register")?;
    let comment_len = reader.u32("yorum")? as usize;
    let comment = reader.take(comment_len, "yorum")?.to_vec();
    if reader.at != data.len() {
        return Err(format!(
            "DVZ dosyasında {} bayt çözülemeyen ek veri var",
            data.len() - reader.at
        ));
    }
    Ok(Package {
        model_index: u16::from_le_bytes([data[MODEL_OFFSET], data[MODEL_OFFSET + 1]]),
        program,
        d,
        c16,
        c32,
        t,
        file_registers,
        comment,
    })
}

/// Program-only package (empty register sections, no password, ID or comment), the same
/// shape DVPtool writes when register backup is not selected.
pub fn write(model_index: u16, program: &[u16]) -> Vec<u8> {
    let bytes: Vec<u8> = program.iter().flat_map(|w| w.to_be_bytes()).collect();
    let sum = program_sum(&bytes);
    let mut out = vec![0u8; HEADER];
    out[0..4].copy_from_slice(&FORMAT.to_le_bytes());
    // Constant fields observed in every DVPtool package; meaning not established.
    out[6] = 0x01;
    out[7] = 0x04;
    out[MODEL_OFFSET..MODEL_OFFSET + 2].copy_from_slice(&model_index.to_le_bytes());
    out[SUM_OFFSET..SUM_OFFSET + 4].copy_from_slice(&sum.to_le_bytes());
    // `b[i] = i - b[i]` encoding of 0xFFFFFFFF (no PLC ID).
    out[0x48..0x4C].copy_from_slice(&[1, 2, 3, 4]);
    out[0x4D] = 0x01;
    out[LENGTH_OFFSET..LENGTH_OFFSET + 4].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend(bytes.iter().map(|b| b ^ sum as u8));
    // D, C16, C32, T, file registers, comment: all empty.
    for _ in 0..6 {
        out.extend(0u32.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Header and program of the package DVPtool wrote from the ES2 simulator holding
    /// `LD X0 / OUT Y0 / END` (bk1.dvz), with its register sections emptied.
    fn dvptool_package() -> Vec<u8> {
        let mut data = vec![0u8; HEADER];
        for (offset, value) in [
            (0x00, 0xd3),
            (0x06, 0x01),
            (0x07, 0x04),
            (0x09, 0x08),
            (0x44, 0xf8),
            (0x45, 0xff),
            (0x46, 0xff),
            (0x47, 0xff),
            (0x48, 0x01),
            (0x49, 0x02),
            (0x4a, 0x03),
            (0x4b, 0x04),
            (0x4d, 0x01),
            (0x68, 0x06),
        ] {
            data[offset] = value;
        }
        data.extend([0xf8, 0xdc, 0xf8, 0x3d, 0xf7, 0xf8]);
        data.extend([0u8; 24]);
        data
    }

    #[test]
    fn reads_dvptool_package() {
        let package = read(&dvptool_package()).unwrap();
        assert_eq!(package.program, [0x0024, 0x00c5, 0x0f00]);
        assert_eq!(package.model_name(), Some("ES2"));
        assert!(package.d.is_empty() && package.comment.is_empty());
    }

    #[test]
    fn writer_reproduces_dvptool_bytes() {
        assert_eq!(write(8, &[0x0024, 0x00c5, 0x0f00]), dvptool_package());
    }

    #[test]
    fn rejects_corrupt_packages() {
        let mut bad_sum = dvptool_package();
        bad_sum[0x100] ^= 1;
        assert!(read(&bad_sum).unwrap_err().contains("sağlama"));
        let mut truncated = dvptool_package();
        truncated.truncate(truncated.len() - 1);
        assert!(read(&truncated).unwrap_err().contains("kesik"));
        let mut extra = dvptool_package();
        extra.push(0);
        assert!(read(&extra).unwrap_err().contains("ek veri"));
    }
}
