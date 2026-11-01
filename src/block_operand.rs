//! Contiguous block elements. Kn elements advance by their declared bit width;
//! numeric elements advance by one word or the native double-counter slot.
use crate::additional_processing::AdditionalValue;
use crate::plc::{Area, Device, Memory, Operand, word_range};
use crate::shift::bit_span;
use crate::wide::{WideOperand, double_devices};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub struct BlockSource(AdditionalValue);

#[derive(Debug, Clone, PartialEq)]
pub struct BlockDestination(AdditionalValue);

fn shifted(value: &AdditionalValue, offset: u16) -> Result<AdditionalValue, String> {
    let base = value.base().ok_or("Blok operandı adres olmalı")?;
    let stride = match value {
        AdditionalValue::Group { digits, .. } => digits * 4,
        AdditionalValue::Double(_) if !(base.area == Area::C && base.index >= 200) => 2,
        _ => 1,
    };
    let index = base
        .index
        .checked_add(offset.checked_mul(stride).ok_or("Blok aralığı taşıyor")?)
        .ok_or("Blok aralığı taşıyor")?;
    let next = Device {
        area: base.area,
        index,
    };
    Ok(match value {
        AdditionalValue::Group { digits, wide, .. } => AdditionalValue::Group {
            base: next,
            digits: *digits,
            wide: *wide,
        },
        AdditionalValue::Double(_) => {
            if base.area == Area::C && base.index < 200 && next.index >= 200 {
                return Err("32 bit blok C199/C200 alan sınırını aşamaz".into());
            }
            AdditionalValue::Double(WideOperand::Register(next))
        }
        AdditionalValue::Word(_) => AdditionalValue::Word(Operand::Register(next)),
    })
}

fn at(value: &AdditionalValue, offset: u16, write: bool) -> Result<AdditionalValue, String> {
    let next = shifted(value, offset)?;
    match &next {
        AdditionalValue::Group { base, digits, .. } => {
            bit_span(*base, digits * 4, write)?;
        }
        AdditionalValue::Word(Operand::Register(base)) => {
            word_range(*base, 1)?;
        }
        AdditionalValue::Double(WideOperand::Register(base)) => {
            double_devices(*base)?;
        }
        _ => unreachable!("validated block address"),
    }
    Ok(next)
}

fn devices(value: &AdditionalValue, count: u16, write: bool) -> Result<Vec<Device>, String> {
    let mut result = BTreeSet::new();
    for offset in 0..count {
        at(value, offset, write)?.devices(&mut result);
    }
    Ok(result.into_iter().collect())
}

fn kind(value: &AdditionalValue) -> Option<u8> {
    match value {
        AdditionalValue::Group { .. } => None,
        _ => Some(match value.base().expect("validated block address").area {
            Area::T => 0x82,
            Area::C => 0x84,
            Area::D => 0x86,
            _ => unreachable!("validated numeric block address"),
        }),
    }
}

impl BlockSource {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let value = AdditionalValue::parse(raw, false, false)?;
        if value.base().is_none() {
            return Err("BMOV kaynağı KnX/Y/M/S veya T/C/D olmalı".into());
        }
        Ok(Self(value))
    }
    pub fn value(&self) -> &AdditionalValue {
        &self.0
    }
    pub fn base(&self) -> Device {
        self.0.base().expect("validated block source")
    }
    pub fn element(&self, offset: u16) -> Result<AdditionalValue, String> {
        at(&self.0, offset, false)
    }
    pub fn devices(&self, count: u16) -> Result<Vec<Device>, String> {
        devices(&self.0, count, false)
    }
    pub fn numeric_kind(&self) -> Option<u8> {
        kind(&self.0)
    }
    pub fn read_bmov_element(
        &self,
        m: &Memory,
        offset: u16,
        more: bool,
        observed: &mut BTreeSet<Device>,
    ) -> Result<i32, String> {
        let element = self.element(offset)?;
        element.devices(observed);
        // Native BMOV validates the next bit immediately after increment,
        // except for the very last bit of the final requested element.
        if more && let AdditionalValue::Group { base, digits, .. } = &element {
            bit_span(
                Device {
                    area: base.area,
                    index: base.index + digits * 4,
                },
                1,
                false,
            )?;
        }
        Ok(element.read(m))
    }
    pub fn validate_pair(&self, dest: &BlockDestination) -> Result<(), String> {
        if let (AdditionalValue::Group { digits: a, .. }, AdditionalValue::Group { digits: b, .. }) =
            (&self.0, &dest.0)
            && a != b
        {
            return Err("BMOV kaynak ve hedef Kn genişlikleri aynı olmalı".into());
        }
        Ok(())
    }
}
impl BlockDestination {
    pub fn parse(raw: &str, wide: bool) -> Result<Self, String> {
        Ok(Self(AdditionalValue::parse(raw, wide, true)?))
    }
    pub fn value(&self) -> &AdditionalValue {
        &self.0
    }
    pub fn base(&self) -> Device {
        self.0.base().expect("validated block destination")
    }
    pub fn element(&self, offset: u16) -> Result<AdditionalValue, String> {
        at(&self.0, offset, true)
    }
    pub fn devices(&self, count: u16) -> Result<Vec<Device>, String> {
        devices(&self.0, count, true)
    }
    pub fn numeric_kind(&self) -> Option<u8> {
        kind(&self.0)
    }
    pub fn word_fill_kind(&self) -> u8 {
        match &self.0 {
            AdditionalValue::Group { digits, .. } => (0x80 | (2 * digits) as u8) & 0x86,
            _ => self.numeric_kind().expect("numeric destination"),
        }
    }
    pub fn read(&self, memory: &Memory) -> i32 {
        self.0.read(memory)
    }
    pub fn write(&self, memory: &mut Memory, value: i32) {
        self.0.write(memory, value);
    }
    pub fn write_bmov_element(
        &self,
        m: &mut Memory,
        offset: u16,
        more: bool,
        value: i32,
        observed: &mut BTreeSet<Device>,
    ) -> Result<(), String> {
        if let AdditionalValue::Group { base, digits, .. } = shifted(&self.0, offset)? {
            for n in 0..digits * 4 {
                let bit = Device {
                    area: base.area,
                    index: base.index + n,
                };
                bit_span(bit, 1, true)?;
                observed.insert(bit);
                m.set_internal(bit, value as u32 & (1 << n) != 0);
                if n + 1 != digits * 4 || more {
                    bit_span(
                        Device {
                            area: bit.area,
                            index: bit.index + 1,
                        },
                        1,
                        true,
                    )?;
                }
            }
        } else {
            let element = self.element(offset)?;
            element.devices(observed);
            element.write(m, value);
        }
        Ok(())
    }
}
impl std::fmt::Display for BlockSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::fmt::Display for BlockDestination {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
