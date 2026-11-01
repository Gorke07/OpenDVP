//! Validated result layout for full MUL/DIV, distinct from truncated results.
//! Packed destinations store two declared-width chunks; word numeric targets
//! use the native combined 32-bit writer, including natural counter storage.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::{CalendarDestination, CalendarValue};
use crate::plc::{Area, Device, Memory, Operand};
use crate::wide::WideOperand;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
enum ResultStorage {
    WordNumeric(CalendarDestination),
    Split {
        primary: CalendarDestination,
        secondary: CalendarDestination,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct MultiplyDestination(ResultStorage);

impl MultiplyDestination {
    pub fn parse(raw: &str, wide: bool) -> Result<Self, String> {
        let text = raw.trim().to_ascii_uppercase();
        if text.starts_with('F') || (wide && text.starts_with('E')) {
            return Err(
                "MUL/DIV hedefinde F kullanılamaz; E yalnız 16 bit komutta kullanılabilir".into(),
            );
        }
        let value = CalendarDestination::parse(&text, wide || !text.starts_with('K'))?
            .value()
            .clone();
        Self::from_value(value, wide)
    }
    /// Build the complete result from a concrete typed primary. Instruction
    /// width controls Kn significance, while numeric word results still need
    /// 32-bit storage. No effective address is reparsed from source text.
    pub fn from_value(value: CalendarValue, wide: bool) -> Result<Self, String> {
        if let CalendarValue::Additional(AdditionalValue::Group { base, digits, .. }) = value {
            if !(1..=if wide { 8 } else { 4 }).contains(&digits) {
                return Err("MUL/DIV Kn sonuç genişliği komutla uyuşmuyor".into());
            }
            let group =
                |base| CalendarValue::Additional(AdditionalValue::Group { base, digits, wide });
            let primary = CalendarDestination::from_value(group(base))?;
            let next = Device {
                area: base.area,
                index: base
                    .index
                    .checked_add(digits * 4)
                    .ok_or("MUL/DIV sonuç aralığı taşıyor")?,
            };
            return Ok(Self(ResultStorage::Split {
                primary,
                secondary: CalendarDestination::from_value(group(next))?,
            }));
        }
        // Word MUL/DIV still store a combined double word: D/T pairs, E/F
        // aliases, one natural C200+ value, or the native C199 low-word slot.
        let value = match value {
            CalendarValue::Additional(AdditionalValue::Word(Operand::Register(base)))
            | CalendarValue::Additional(AdditionalValue::Double(WideOperand::Register(base))) => {
                CalendarValue::Additional(AdditionalValue::Double(WideOperand::Register(base)))
            }
            CalendarValue::Index {
                index,
                high_bank: false,
                ..
            } if !wide => CalendarValue::Index {
                index,
                wide: true,
                high_bank: false,
            },
            _ => {
                return Err(
                    "MUL/DIV sonuç hedefi KnY/M/S,T/C/D; E yalnız kelime komutunda olmalı".into(),
                );
            }
        };
        let primary = CalendarDestination::from_value(value)?;
        if !wide {
            return Ok(Self(ResultStorage::WordNumeric(primary)));
        }
        let base = primary.base();
        // The native four-halfword writer changes counter storage at C199.
        // C197 -> C199 (truncated), C198 -> C200, C199 -> C200, C200 -> C201.
        let stride = if base.area == Area::C && base.index >= 199 {
            1
        } else {
            2
        };
        let next = Device {
            area: base.area,
            index: base
                .index
                .checked_add(stride)
                .ok_or("MUL/DIV sonuç aralığı taşıyor")?,
        };
        Ok(Self(ResultStorage::Split {
            primary,
            secondary: CalendarDestination::from_value(CalendarValue::Additional(
                AdditionalValue::Double(WideOperand::Register(next)),
            ))?,
        }))
    }
    fn primary(&self) -> &CalendarDestination {
        match &self.0 {
            ResultStorage::WordNumeric(value) => value,
            ResultStorage::Split { primary, .. } => primary,
        }
    }
    pub fn value(&self) -> &CalendarValue {
        self.primary().value()
    }
    pub fn base(&self) -> Device {
        self.primary().base()
    }
    pub fn wide(&self) -> bool {
        match &self.0 {
            ResultStorage::WordNumeric(_) => false,
            ResultStorage::Split { primary, .. } => primary.wide(),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        self.primary().devices(out);
        if let ResultStorage::Split { secondary, .. } = &self.0 {
            secondary.devices(out);
        }
    }
    pub fn read_pair(&self, memory: &Memory) -> [i32; 2] {
        match &self.0 {
            ResultStorage::WordNumeric(value) => {
                let raw = value.read(memory);
                [i32::from(raw as i16), i32::from((raw >> 16) as i16)]
            }
            ResultStorage::Split { primary, secondary } => {
                [primary.read(memory), secondary.read(memory)]
            }
        }
    }
    pub fn write_pair(&self, memory: &mut Memory, values: [i32; 2]) {
        match &self.0 {
            ResultStorage::WordNumeric(value) => {
                let raw = u32::from(values[0] as u16) | (u32::from(values[1] as u16) << 16);
                value.write(memory, raw as i32);
            }
            ResultStorage::Split { primary, secondary } => {
                primary.write(memory, values[0]);
                secondary.write(memory, values[1]);
            }
        }
    }
}

impl std::fmt::Display for MultiplyDestination {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.primary().fmt(f)
    }
}
