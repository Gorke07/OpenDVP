//! Calendar scalar operands, including the documented E/F index-register
//! values. Direct E/F sources differ from address modifiers such as D5E0.
//! Canonical storage is their documented special-D alias, never a shadow array.
use crate::additional_processing::AdditionalValue;
use crate::plc::{Area, Device, Memory, Operand};
use crate::wide::WideOperand;
use std::collections::BTreeSet;

/// Typed access to existing aliased D memory. No extra Memory field is needed:
/// normal MOV/DMOV to these D words and direct E/F reads must see one value.
#[derive(Debug, Clone, Copy, Default)]
pub struct CalendarIndices;

impl CalendarIndices {
    pub fn alias_device(index: u8, high_bank: bool) -> Result<Device, String> {
        if index > 7 {
            return Err("İndeks register numarası 0..7 olmalı".into());
        }
        let low = if index == 0 {
            1028
        } else {
            1180 + 2 * u16::from(index)
        };
        Ok(Device {
            area: Area::D,
            index: low + u16::from(high_bank),
        })
    }
    pub fn read_word(m: &Memory, index: u8, high_bank: bool) -> Result<i16, String> {
        Ok(m.word(Self::alias_device(index, high_bank)?))
    }
    pub fn write_word(
        m: &mut Memory,
        index: u8,
        high_bank: bool,
        value: i16,
    ) -> Result<(), String> {
        m.set_word(Self::alias_device(index, high_bank)?, value);
        Ok(())
    }
    pub fn read_double(m: &Memory, index: u8) -> Result<i32, String> {
        Ok(m.double_word(Self::alias_device(index, false)?))
    }
    pub fn write_double(m: &mut Memory, index: u8, value: i32) -> Result<(), String> {
        m.set_double_word(Self::alias_device(index, false)?, value);
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CalendarValue {
    Additional(AdditionalValue),
    Index {
        index: u8,
        wide: bool,
        high_bank: bool,
    },
}

/// A validated writable scalar. Its base device is derived from the operand;
/// it is not a second stored address that could disagree with a packed group.
#[derive(Debug, Clone, PartialEq)]
pub struct CalendarDestination(CalendarValue);

impl CalendarDestination {
    pub(crate) fn from_value(value: CalendarValue) -> Result<Self, String> {
        match &value {
            CalendarValue::Additional(AdditionalValue::Word(Operand::Register(d))) => {
                crate::plc::word_range(*d, 1)?;
            }
            CalendarValue::Additional(AdditionalValue::Double(WideOperand::Register(d))) => {
                crate::wide::double_devices(*d)?;
            }
            CalendarValue::Additional(AdditionalValue::Group { base, digits, wide }) => {
                if !(1..=if *wide { 8 } else { 4 }).contains(digits) {
                    return Err("Kn hedef genişliği geçersiz".into());
                }
                crate::shift::bit_span(*base, digits * 4, true)?;
            }
            CalendarValue::Index {
                index,
                wide,
                high_bank,
            } => {
                CalendarIndices::alias_device(*index, *high_bank)?;
                if *wide && *high_bank {
                    return Err("32 bit indeks hedefi E0..E7 olmalı".into());
                }
            }
            _ => return Err("Hedef yazılabilir bir sayısal operand olmalı".into()),
        }
        Ok(Self(value))
    }
    pub fn parse(raw: &str, wide: bool) -> Result<Self, String> {
        let text = raw.trim().to_ascii_uppercase();
        let value = if text.starts_with(['E', 'F']) {
            CalendarValue::parse(&text, wide)?
        } else {
            CalendarValue::Additional(AdditionalValue::parse(&text, wide, true)?)
        };
        Ok(Self(value))
    }
    pub fn value(&self) -> &CalendarValue {
        &self.0
    }
    pub fn base(&self) -> Device {
        self.0.base().expect("validated writable scalar")
    }
    pub fn wide(&self) -> bool {
        self.0.wide()
    }
    pub fn read(&self, m: &Memory) -> i32 {
        self.0.read(m)
    }
    /// XCH's M1303 resolver uses an index different from the E/F storage
    /// reader. Preserve that verified distinction without changing aliases.
    pub fn same_exchange_address(&self, other: &Self) -> bool {
        let key = |value: &Self| {
            let mut base = value.base();
            if matches!(value.value(), CalendarValue::Index { .. }) {
                base.index -= 1000;
            }
            base
        };
        key(self) == key(other)
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        self.0.devices(out);
    }
    pub fn write(&self, m: &mut Memory, value: i32) {
        match &self.0 {
            CalendarValue::Additional(target) => target.write(m, value),
            CalendarValue::Index {
                index,
                wide: true,
                high_bank: false,
            } => {
                CalendarIndices::write_double(m, *index, value).expect("validated E pair");
            }
            CalendarValue::Index {
                index, high_bank, ..
            } => {
                CalendarIndices::write_word(m, *index, *high_bank, value as i16)
                    .expect("validated E/F word");
            }
        }
    }
}

impl std::fmt::Display for CalendarDestination {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl CalendarValue {
    pub fn parse(raw: &str, wide: bool) -> Result<Self, String> {
        let text = raw.trim().to_ascii_uppercase();
        if text.starts_with(['E', 'F']) {
            let high_bank = text.starts_with('F');
            if wide && high_bank {
                return Err(
                    "32 bit indeks kaynağı E0..E7 olmalı; F yalnız 16 bit kullanılabilir".into(),
                );
            }
            let suffix = &text[1..];
            let index = if suffix.is_empty() {
                0
            } else {
                if suffix.len() != 1 || !suffix.as_bytes()[0].is_ascii_digit() {
                    return Err(
                        "İndeks kaynağı E0..E7 veya F0..F7 olmalı; bu sütun float sabiti değildir"
                            .into(),
                    );
                }
                suffix.as_bytes()[0] - b'0'
            };
            CalendarIndices::alias_device(index, high_bank)?;
            return Ok(Self::Index {
                index,
                wide,
                high_bank,
            });
        }
        Ok(Self::Additional(AdditionalValue::parse(
            &text, wide, false,
        )?))
    }
    pub fn wide(&self) -> bool {
        match self {
            Self::Additional(value) => value.wide(),
            Self::Index { wide, .. } => *wide,
        }
    }
    pub fn base(&self) -> Option<Device> {
        match self {
            Self::Additional(value) => value.base(),
            Self::Index {
                index, high_bank, ..
            } => CalendarIndices::alias_device(*index, *high_bank).ok(),
        }
    }
    pub fn constant(&self) -> Option<i32> {
        match self {
            Self::Additional(AdditionalValue::Word(Operand::Constant(value))) => {
                Some(i32::from(*value))
            }
            Self::Additional(AdditionalValue::Double(WideOperand::Constant(value))) => Some(*value),
            _ => None,
        }
    }
    pub fn read(&self, m: &Memory) -> i32 {
        match self {
            Self::Additional(value) => value.read(m),
            Self::Index {
                index,
                wide: true,
                high_bank: false,
            } => CalendarIndices::read_double(m, *index).expect("validated E pair"),
            Self::Index {
                index, high_bank, ..
            } => i32::from(
                CalendarIndices::read_word(m, *index, *high_bank).expect("validated E/F word"),
            ),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        match self {
            Self::Additional(value) => value.devices(out),
            Self::Index {
                index,
                wide,
                high_bank,
            } => {
                out.insert(
                    CalendarIndices::alias_device(*index, *high_bank).expect("validated index"),
                );
                if *wide {
                    out.insert(
                        CalendarIndices::alias_device(*index, true).expect("validated E high word"),
                    );
                }
            }
        }
    }
    /// Exact logical native words recovered from installed compiler408A00/
    ///408A6C and the word/wide emitter paths. Additional variants use the
    /// existing numeric/group encoder; this method deliberately returns None.
    pub fn native_words(&self) -> Option<Vec<u16>> {
        let Self::Index {
            index,
            wide,
            high_bank,
        } = self
        else {
            return None;
        };
        if *index > 7 || (*wide && *high_bank) {
            return None;
        }
        let offset = (if *index == 0 {
            0x0038
        } else {
            0x0168 + 4 * u16::from(*index)
        }) + if *high_bank { 2 } else { 0 };
        let mut words = vec![((offset & 0xff) << 8) | 0x86, (offset & 0xff00) | 0x80];
        if *wide {
            words.extend([0x0080, 0x0080]);
        }
        Some(words)
    }
}

impl std::fmt::Display for CalendarValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Additional(value) => write!(f, "{value}"),
            Self::Index {
                index, high_bank, ..
            } => write!(f, "{}{index}", if *high_bank { 'F' } else { 'E' }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn index_aliases_stay_coherent_with_general_d_writes_and_pair_high_word() {
        let mut m = Memory::default();
        for index in 0..8 {
            CalendarIndices::write_word(&mut m, index, false, 0x5678).unwrap();
            CalendarIndices::write_word(&mut m, index, true, 0x1234).unwrap();
            let value = CalendarValue::parse(&format!("E{index}"), true).unwrap();
            assert_eq!(value.read(&m), 0x12345678);
            assert_eq!(
                CalendarValue::parse(&format!("F{index}"), false)
                    .unwrap()
                    .read(&m),
                0x1234
            );
            let low = CalendarIndices::alias_device(index, false).unwrap();
            assert_eq!(
                low.index,
                if index == 0 {
                    1028
                } else {
                    1180 + 2 * u16::from(index)
                }
            );
            m.set_word(low, -2);
            assert_eq!(
                CalendarValue::parse(&format!("E{index}"), false)
                    .unwrap()
                    .read(&m),
                -2
            );
            CalendarIndices::write_double(&mut m, index, -1234567).unwrap();
            assert_eq!(value.read(&m), -1234567);
        }
    }
    #[test]
    fn compiler_fixture_descriptors_cover_all_e_f_banks_and_wide_pair_width() {
        let e = [
            0x3886, 0x6c86, 0x7086, 0x7486, 0x7886, 0x7c86, 0x8086, 0x8486,
        ];
        let f = [
            0x3a86, 0x6e86, 0x7286, 0x7686, 0x7a86, 0x7e86, 0x8286, 0x8686,
        ];
        for index in 0..8 {
            let high = if index == 0 { 0x0080 } else { 0x0180 };
            assert_eq!(
                CalendarValue::parse(&format!("E{index}"), false)
                    .unwrap()
                    .native_words(),
                Some(vec![e[index], high])
            );
            assert_eq!(
                CalendarValue::parse(&format!("F{index}"), false)
                    .unwrap()
                    .native_words(),
                Some(vec![f[index], high])
            );
            assert_eq!(
                CalendarValue::parse(&format!("E{index}"), true)
                    .unwrap()
                    .native_words(),
                Some(vec![e[index], high, 0x0080, 0x0080])
            );
        }
    }
    #[test]
    fn scalar_matrix_is_distinct_from_float_literals_and_wide_f_is_rejected() {
        for raw in ["F0", "F7", "F"] {
            assert!(CalendarValue::parse(raw, true).is_err());
        }
        for raw in ["E8", "F8", "E-1", "E1.0", "F1.25", "E00"] {
            assert!(CalendarValue::parse(raw, false).is_err());
        }
        assert_eq!(CalendarValue::parse("E", true).unwrap().to_string(), "E0");
        assert_eq!(CalendarValue::parse("f", false).unwrap().to_string(), "F0");
        assert!(CalendarIndices::alias_device(8, false).is_err());
        assert_eq!(
            CalendarValue::parse("K123", false).unwrap().native_words(),
            None
        );
        assert_eq!(
            CalendarValue::parse("K123", false).unwrap().constant(),
            Some(123)
        );
        let mut observed = BTreeSet::new();
        CalendarValue::parse("E7", true)
            .unwrap()
            .devices(&mut observed);
        assert_eq!(
            observed.into_iter().map(|d| d.index).collect::<Vec<_>>(),
            [1194, 1195]
        );
    }
}
