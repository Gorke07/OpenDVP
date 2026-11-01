//! Address modifiers are signed 16-bit E/F values even when the addressed
//! operand is a double word. Device remains an ordinary concrete address.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::CalendarIndices;
use crate::plc::{Area, Device, Memory, Operand, word_range};
use crate::shift::bit_span;
use crate::wide::{WideOperand, double_devices};

/// Same bank/high_bank vocabulary as program_control::PointerIndex. The
/// direct E/F scalar value and its double-word pairing are separate concepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexWord {
    bank: u8,
    high_bank: bool,
}
impl IndexWord {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let text = raw.to_ascii_uppercase();
        if !text.starts_with(['E', 'F']) {
            return Err("Adres indeksi E0..E7/F0..F7 olmalı".into());
        }
        let bank = match text.as_bytes() {
            [_] => 0,
            [_, b'0'..=b'7'] => text.as_bytes()[1] - b'0',
            _ => return Err("Adres indeksi E0..E7/F0..F7 olmalı".into()),
        };
        Ok(Self {
            bank,
            high_bank: text.starts_with('F'),
        })
    }
    pub fn device(self) -> Device {
        CalendarIndices::alias_device(self.bank, self.high_bank).expect("validated index bank")
    }
    pub fn read(self, memory: &Memory) -> i16 {
        memory.word(self.device())
    }
}
impl std::fmt::Display for IndexWord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", if self.high_bank { 'F' } else { 'E' }, self.bank)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexedAddress {
    pub base: Device,
    pub index: IndexWord,
}

impl IndexedAddress {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let text = raw.trim().to_ascii_uppercase();
        let split = text
            .char_indices()
            .skip(1)
            .find(|(_, c)| matches!(c, 'E' | 'F'))
            .map(|(i, _)| i)
            .ok_or("Adresin E/F değiştiricisi eksik")?;
        let base: Device = text[..split].parse()?;
        let index = IndexWord::parse(&text[split..])?;
        Ok(Self { base, index })
    }
    /// Kn changes significance, never the index unit: a group offset is in
    /// single bits. Numeric offsets count logical registers, not bytes.
    pub fn resolve(
        self,
        memory: &Memory,
        wide: bool,
        digits: Option<u16>,
        write: bool,
    ) -> Result<AdditionalValue, String> {
        if self.base.area == Area::C && self.base.index >= 200 && !wide && digits.is_none() {
            return Err("16 bit indeksli operandın temel sayacı C0..C199 olmalı".into());
        }
        let effective = self.base.index.wrapping_add(self.index.read(memory) as u16) as i16;
        let index =
            u16::try_from(effective).map_err(|_| "İndeksli adres negatif aralığa taşıyor")?;
        let target = Device { index, ..self.base };
        // Validate the effective address; a valid base at a word/group endpoint
        // may become usable after a negative offset.
        target.to_string().parse::<Device>()?;
        if target.area == Area::D && target.index > memory.data_register_limit.unwrap_or(9999) {
            return Err("İndeksli D adresi seçilen model aralığını aşıyor".into());
        }
        if let Some(digits) = digits {
            if !(1..=if wide { 8 } else { 4 }).contains(&digits) {
                return Err("İndeksli Kn genişliği geçersiz".into());
            }
            // The installed generic group helper adds 0x200 when a low-bank
            // start resolves above raw 0x0D00. This established simulator
            // remap is not equivalent to canonical M1281..M1535 storage.
            if target.area == Area::M
                && self.base.index < 1536
                && (1281..=1535).contains(&target.index)
            {
                return Err("İndeksli M1281..M1535 başlangıcının native eşlemesi doğrulanmış kanonik adreslemeyle uyuşmuyor".into());
            }
            let span = bit_span(target, digits * 4, write)?;
            if target.area == Area::M {
                let limit = memory.relay_limit.unwrap_or(4095);
                if span.iter().any(|d| d.index > limit) {
                    return Err("İndeksli M grubu seçilen model aralığını aşıyor".into());
                }
                if (self.base.index < 1536) != (target.index < 1536)
                    || (target.index < 1536 && target.index + digits * 4 > 1536)
                {
                    return Err("İndeksli M grubunun fiziksel alan geçişi doğrulanmadı".into());
                }
            }
            Ok(AdditionalValue::Group {
                base: target,
                digits,
                wide,
            })
        } else {
            let span = if wide {
                double_devices(target)?
            } else {
                word_range(target, 1)?
            };
            if span
                .iter()
                .any(|d| d.area == Area::D && d.index > memory.data_register_limit.unwrap_or(9999))
            {
                return Err("İndeksli çift kelime seçilen model aralığını aşıyor".into());
            }
            Ok(if wide {
                AdditionalValue::Double(WideOperand::Register(target))
            } else {
                AdditionalValue::Word(Operand::Register(target))
            })
        }
    }
    /// Compiler 40920C uses different bank fields for D and T/C/Kn. These
    /// are original descriptors, never encoding of a resolved runtime address.
    pub fn native_words(self, wide: bool, digits: Option<u16>) -> Result<Vec<u16>, String> {
        self.base.to_string().parse::<Device>()?;
        if self.index.bank > 7 {
            return Err("Adres indeks bankı geçersiz".into());
        }
        let suffix = if self.index.high_bank { 0xf0 } else { 0xe0 };
        let (offset, kind, bank_bits) = if let Some(digits) = digits {
            if !(1..=if wide { 8 } else { 4 }).contains(&digits) {
                return Err("İndeksli Kn genişliği geçersiz".into());
            }
            let address = match self.base.area {
                Area::X => 0x400 + self.base.index,
                Area::Y => 0x500 + self.base.index,
                Area::M if self.base.index >= 1536 => 0x1000 + self.base.index - 1536,
                Area::M => 0x800 + self.base.index,
                Area::S => self.base.index,
                _ => return Err("İndeksli Kn alanı X/Y/M/S olmalı".into()),
            };
            (
                address,
                0x80 | (2 * digits),
                u16::from(self.index.bank) << 13,
            )
        } else {
            match self.base.area {
                Area::D => (
                    self.base.index * 2,
                    0x86 + u16::from(self.index.bank >> 1),
                    u16::from(self.index.bank & 1) << 15,
                ),
                Area::T => (self.base.index * 2, 0x82, u16::from(self.index.bank) << 13),
                Area::C if self.base.index < 200 => {
                    (self.base.index * 2, 0x84, u16::from(self.index.bank) << 13)
                }
                Area::C if wide => (
                    0x200 + (self.base.index - 200) * 4,
                    0x84,
                    u16::from(self.index.bank) << 13,
                ),
                _ => return Err("İndeksli sayısal operand alanı/genişliği geçersiz".into()),
            }
        };
        let tag = suffix | if digits.is_some() { 4 } else { 6 };
        let mut result = vec![
            ((offset & 0xff) << 8) | tag,
            (offset & 0xff00) | bank_bits | kind,
        ];
        if wide {
            result.extend([0x0080, 0x0080]);
        }
        Ok(result)
    }
}
impl std::fmt::Display for IndexedAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.base, self.index)
    }
}
