//! The n operand of indexed bit commands has its own compiler matrix.
//! In particular, a double-width F reads from the F word and its following D
//! word. This exception does not broaden CalendarValue or ABS/Gray operands.
use crate::calendar_operand::{CalendarIndices, CalendarValue};
use crate::plc::{Device, Memory};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
enum IndexStorage {
    Calendar(CalendarValue),
    WideF(u8),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BitIndexValue(IndexStorage);

impl BitIndexValue {
    pub fn parse(raw: &str, wide: bool) -> Result<Self, String> {
        let text = raw.trim().to_ascii_uppercase();
        if wide && text.starts_with('F') {
            // Reuse the established F0..F7 syntax and alias validation while
            // giving only this operand role its four-word native descriptor.
            let CalendarValue::Index {
                index,
                high_bank: true,
                ..
            } = CalendarValue::parse(&text, false)?
            else {
                unreachable!("F prefix validated as an index word");
            };
            return Ok(Self(IndexStorage::WideF(index)));
        }
        CalendarValue::parse(&text, wide).map(|value| Self(IndexStorage::Calendar(value)))
    }
    pub fn wide(&self) -> bool {
        match &self.0 {
            IndexStorage::Calendar(value) => value.wide(),
            IndexStorage::WideF(_) => true,
        }
    }
    pub fn constant(&self) -> Option<i32> {
        match &self.0 {
            IndexStorage::Calendar(value) => value.constant(),
            IndexStorage::WideF(_) => None,
        }
    }
    pub fn read(&self, memory: &Memory) -> i32 {
        match &self.0 {
            IndexStorage::Calendar(value) => value.read(memory),
            IndexStorage::WideF(index) => memory.double_word(
                CalendarIndices::alias_device(*index, true).expect("validated F index"),
            ),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        match &self.0 {
            IndexStorage::Calendar(value) => value.devices(out),
            IndexStorage::WideF(index) => {
                let low = CalendarIndices::alias_device(*index, true).expect("validated F index");
                out.insert(low);
                out.insert(Device {
                    index: low.index + 1,
                    ..low
                });
            }
        }
    }
    pub(crate) fn calendar_value(&self) -> Option<&CalendarValue> {
        match &self.0 {
            IndexStorage::Calendar(value) => Some(value),
            IndexStorage::WideF(_) => None,
        }
    }
    pub fn native_words(&self) -> Option<Vec<u16>> {
        match &self.0 {
            IndexStorage::Calendar(value) => value.native_words(),
            IndexStorage::WideF(index) => {
                let mut words = CalendarValue::Index {
                    index: *index,
                    wide: false,
                    high_bank: true,
                }
                .native_words()
                .expect("validated F descriptor");
                words.extend([0x0080, 0x0080]);
                Some(words)
            }
        }
    }
}

impl std::fmt::Display for BitIndexValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            IndexStorage::Calendar(value) => value.fmt(f),
            IndexStorage::WideF(index) => write!(f, "F{index}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bit_tools::BitAction;
    use crate::numeric_contacts::NumericPredicate;

    #[test]
    fn all_wide_f_indices_read_and_track_the_actual_adjacent_d_word() {
        // Literal aliases and emitter words are independent of the alias
        // helper used by the implementation. F0's high half is D1030.
        for (index, alias, low_word, high_word) in [
            (0, 1029, 0x3a86, 0x0080),
            (1, 1183, 0x6e86, 0x0180),
            (2, 1185, 0x7286, 0x0180),
            (3, 1187, 0x7686, 0x0180),
            (4, 1189, 0x7a86, 0x0180),
            (5, 1191, 0x7e86, 0x0180),
            (6, 1193, 0x8286, 0x0180),
            (7, 1195, 0x8686, 0x0180),
        ] {
            let low: Device = format!("D{alias}").parse().unwrap();
            let high: Device = format!("D{}", alias + 1).parse().unwrap();
            let mut memory = Memory::default();
            memory.set_word(low, 31);
            memory.set_word(high, 0x1234);
            let operand = BitIndexValue::parse(&format!("F{index}"), true).unwrap();
            assert!(operand.wide());
            assert_eq!(operand.constant(), None);
            assert_eq!(operand.to_string(), format!("F{index}"));
            assert_eq!(operand.read(&memory), 0x1234_001f);
            let mut devices = BTreeSet::new();
            operand.devices(&mut devices);
            assert_eq!(devices, [low, high].into_iter().collect());
            assert_eq!(
                operand.native_words(),
                Some(vec![low_word, high_word, 0x0080, 0x0080])
            );
            memory.set_word(high, -1);
            assert_eq!(operand.read(&memory), 0xffff_001fu32 as i32);
            assert_eq!(
                BitIndexValue::parse(&format!("F{index}"), false)
                    .unwrap()
                    .read(&memory),
                31
            );
        }
    }

    #[test]
    fn wide_f_exception_is_limited_to_the_index_operand() {
        for raw in ["F8", "F-1", "F01", "F1.0", "F1E0"] {
            assert!(BitIndexValue::parse(raw, true).is_err(), "{raw}");
        }
        assert_eq!(BitIndexValue::parse("f", true).unwrap().to_string(), "F0");
        assert!(CalendarValue::parse("F1", true).is_err());
        for (op, args) in [
            ("DABS", vec!["F1"]),
            ("DGRY", vec!["F1", "D0"]),
            ("DGBIN", vec!["D0", "F1"]),
            ("DBSET", vec!["F1", "K0"]),
        ] {
            assert!(
                BitAction::parse(op, &args.into_iter().map(String::from).collect::<Vec<_>>())
                    .is_err(),
                "{op}"
            );
        }
        assert!(NumericPredicate::parse("DBLD", &["F1".into(), "K0".into()]).is_err());
        let action = BitAction::parse("DBSET", &["D0".into(), "F1".into()]).unwrap();
        let contact = NumericPredicate::parse("DBLD", &["D0".into(), "F1".into()]).unwrap();
        let mut memory = Memory::default();
        memory.set_word("D1183".parse().unwrap(), 31);
        memory.set_word("D1184".parse().unwrap(), -1);
        action.execute(&mut memory, true);
        assert_eq!(memory.double_word("D0".parse().unwrap()), i32::MIN);
        assert!(contact.evaluate(&memory));
    }
}
