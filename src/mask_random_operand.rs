//! Writable operands for MVM/DMVM and RAND/DRAND only. Their matrices and
//! compiler/native paths permit a double-width F, unlike CalendarDestination.
use crate::calendar_operand::{CalendarDestination, CalendarIndices, CalendarValue};
use crate::plc::{Device, Memory};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
enum Storage {
    Calendar(CalendarDestination),
    WideF(u8),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MaskRandomDestination(Storage);

impl MaskRandomDestination {
    pub fn parse(raw: &str, wide: bool) -> Result<Self, String> {
        let text = raw.trim().to_ascii_uppercase();
        if wide && text.starts_with('F') {
            let CalendarValue::Index {
                index,
                high_bank: true,
                ..
            } = CalendarValue::parse(&text, false)?
            else {
                unreachable!("validated F index");
            };
            return Ok(Self(Storage::WideF(index)));
        }
        CalendarDestination::parse(&text, wide).map(|value| Self(Storage::Calendar(value)))
    }
    pub fn base(&self) -> Device {
        match &self.0 {
            Storage::Calendar(value) => value.base(),
            Storage::WideF(index) => {
                CalendarIndices::alias_device(*index, true).expect("validated F index")
            }
        }
    }
    pub fn wide(&self) -> bool {
        match &self.0 {
            Storage::Calendar(value) => value.wide(),
            Storage::WideF(_) => true,
        }
    }
    pub fn read(&self, memory: &Memory) -> i32 {
        match &self.0 {
            Storage::Calendar(value) => value.read(memory),
            Storage::WideF(_) => memory.double_word(self.base()),
        }
    }
    pub fn write(&self, memory: &mut Memory, value: i32) {
        match &self.0 {
            Storage::Calendar(target) => target.write(memory, value),
            Storage::WideF(_) => memory.set_double_word(self.base(), value),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        match &self.0 {
            Storage::Calendar(value) => value.devices(out),
            Storage::WideF(_) => {
                let low = self.base();
                out.insert(low);
                out.insert(Device {
                    index: low.index + 1,
                    ..low
                });
            }
        }
    }
    pub(crate) fn calendar_destination(&self) -> Option<&CalendarDestination> {
        match &self.0 {
            Storage::Calendar(value) => Some(value),
            Storage::WideF(_) => None,
        }
    }
    pub fn native_words(&self) -> Option<Vec<u16>> {
        let Storage::WideF(index) = &self.0 else {
            return None;
        };
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

impl std::fmt::Display for MaskRandomDestination {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Storage::Calendar(value) => value.fmt(f),
            Storage::WideF(index) => write!(f, "F{index}"),
        }
    }
}
