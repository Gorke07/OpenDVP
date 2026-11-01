//! Opt-in numeric bindings for instructions whose PDF operand cells allow
//! E/F address modifiers. This does not broaden CalendarValue or Device parsing.
use crate::calendar_operand::{CalendarDestination, CalendarValue};
use crate::indexed_address::IndexedAddress;
use crate::plc::{Area, Device, Memory};
use std::collections::BTreeSet;

pub fn has_modifiers(args: &[String]) -> bool {
    args.iter().any(|s| has_modifier(s))
}

/// Read an already resolved scalar at an actual instruction visit. Dynamic
/// Kn spans are absent from the program's static clock-input inventory. Keep
/// address resolution pure so native preflight can capture its storage first.
pub(crate) fn read_resolved_value(
    value: &CalendarValue,
    memory: &Memory,
    observed: &mut BTreeSet<Device>,
) -> Result<i32, String> {
    let mut devices = BTreeSet::new();
    value.devices(&mut devices);
    observed.extend(devices.iter().copied());
    if devices
        .iter()
        .any(|d| d.area == Area::M && (1011..=1014).contains(&d.index))
        && memory.system_relays.clock_levels().is_none()
    {
        return Err(
            "İndeksli M1011..M1014 okuması için kurulu saat profilinin ilk seviyeleri gerekli"
                .into(),
        );
    }
    Ok(value.read(memory))
}

/// Preserve the original signed scalar width after a checked source snapshot.
pub(crate) fn frozen_value(value: i32, wide: bool) -> CalendarValue {
    use crate::additional_processing::AdditionalValue;
    CalendarValue::Additional(if wide {
        AdditionalValue::Double(crate::wide::WideOperand::Constant(value))
    } else {
        AdditionalValue::Word(crate::plc::Operand::Constant(value as i16))
    })
}

#[derive(Debug, Clone, PartialEq)]
pub enum AddressedValue {
    Direct(CalendarValue),
    Modified {
        address: IndexedAddress,
        digits: Option<u16>,
        wide: bool,
    },
}
pub fn has_modifier(raw: &str) -> bool {
    let upper = raw.trim().to_ascii_uppercase();
    // H literals can contain E/F digits; direct E/F are values, not suffixes.
    if upper.starts_with(['H', 'E', 'F']) {
        return false;
    }
    upper
        .char_indices()
        .skip(1)
        .any(|(_, c)| matches!(c, 'E' | 'F'))
}
impl AddressedValue {
    pub fn parse(raw: &str, wide: bool, write: bool) -> Result<Self, String> {
        if !has_modifier(raw) {
            return Ok(Self::Direct(if write {
                CalendarDestination::parse(raw, wide)?.value().clone()
            } else {
                CalendarValue::parse(raw, wide)?
            }));
        }
        let upper = raw.trim().to_ascii_uppercase();
        let (digits, address) = if let Some(rest) = upper.strip_prefix('K') {
            let split = rest
                .find(['X', 'Y', 'M', 'S'])
                .ok_or("Sabit veya Kn genişliği E/F ile değiştirilemez")?;
            let digits = rest[..split]
                .parse::<u16>()
                .map_err(|_| "İndeksli Kn genişliği geçersiz")?;
            if !(1..=if wide { 8 } else { 4 }).contains(&digits) {
                return Err("İndeksli Kn kelimede K1..K4, çift kelimede K1..K8 olmalı".into());
            }
            (Some(digits), IndexedAddress::parse(&rest[split..])?)
        } else {
            (None, IndexedAddress::parse(&upper)?)
        };
        if digits.is_some() {
            if !matches!(address.base.area, Area::X | Area::Y | Area::M | Area::S)
                || (write && address.base.area == Area::X)
            {
                return Err("İndeksli Kn alanı bu operand rolünde kullanılamaz".into());
            }
        } else if !matches!(address.base.area, Area::T | Area::C | Area::D) {
            return Err("Sayısal adres T/C/D veya Kn grubu olmalı".into());
        }
        if !wide && address.base.area == Area::C && address.base.index >= 200 {
            return Err("16 bit indeksli operandın temel sayacı C0..C199 olmalı".into());
        }
        Ok(Self::Modified {
            address,
            digits,
            wide,
        })
    }
    pub fn resolve(&self, memory: &Memory, write: bool) -> Result<CalendarValue, String> {
        match self {
            Self::Direct(value) => Ok(value.clone()),
            Self::Modified {
                address,
                digits,
                wide,
            } => Ok(CalendarValue::Additional(
                address.resolve(memory, *wide, *digits, write)?,
            )),
        }
    }
    pub fn base(&self) -> Option<Device> {
        match self {
            Self::Direct(value) => value.base(),
            Self::Modified { address, .. } => Some(address.base),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        match self {
            Self::Direct(value) => value.devices(out),
            // Effective devices are dynamic; the base is a label, not a read.
            Self::Modified { address, .. } => {
                out.insert(address.index.device());
            }
        }
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        match self {
            Self::Modified { address, .. } => vec![address.index.device()],
            Self::Direct(_) => Vec::new(),
        }
    }
    pub fn observed_devices(&self, memory: &Memory, write: bool, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Ok(value) = self.resolve(memory, write) {
            value.devices(out);
        }
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        match self {
            Self::Direct(value) => crate::es2_encoder::calendar_operand(value),
            Self::Modified {
                address,
                digits,
                wide,
            } => address.native_words(*wide, *digits),
        }
    }
}
impl std::fmt::Display for AddressedValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Direct(value) => value.fmt(f),
            Self::Modified {
                address, digits, ..
            } => {
                if let Some(digits) = digits {
                    write!(f, "K{digits}")?;
                }
                address.fmt(f)
            }
        }
    }
}
