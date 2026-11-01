//! PRUN's octal/decimal grouped-bit conversion, in native forward visit order.
use crate::additional_processing::AdditionalValue;
use crate::plc::{Area, Device, Memory};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub struct ParallelRunAction {
    pub source: AdditionalValue,
    pub destination: AdditionalValue,
    pub wide: bool,
}

impl ParallelRunAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let wide = match name {
            "PRUN" => false,
            "DPRUN" => true,
            _ => return Err("PRUN veya DPRUN bekleniyor".into()),
        };
        if args.len() != 2 {
            return Err("PRUN kaynak ve hedef bit grubu gerektirir".into());
        }
        let action = Self {
            source: AdditionalValue::parse(&args[0], wide, false)?,
            destination: AdditionalValue::parse(&args[1], wide, true)?,
            wide,
        };
        action.groups()?;
        Ok(action)
    }
    pub fn name(&self) -> &'static str {
        if self.wide { "DPRUN" } else { "PRUN" }
    }
    pub fn header(&self) -> u16 {
        if self.wide { 0xb300 } else { 0xb200 }
    }
    pub fn words(&self) -> u16 {
        if self.wide { 9 } else { 5 }
    }
    pub fn wide(&self) -> bool {
        self.wide
    }
    pub fn operands(&self) -> Vec<AdditionalValue> {
        vec![self.source.clone(), self.destination.clone()]
    }
    pub fn dest(&self) -> Device {
        self.destination.base().expect("PRUN hedef bit grubu")
    }
    pub fn label(&self) -> String {
        format!("{} {} {}", self.name(), self.source, self.destination)
    }
    pub fn display_value(&self, m: &Memory) -> String {
        format!("{}", self.destination.read(m) as u32)
    }
    fn groups(&self) -> Result<(Device, u16, Device, u16), String> {
        let group = |v: &AdditionalValue| match v {
            AdditionalValue::Group { base, digits, wide }
                if *wide == self.wide && (1..=if self.wide { 8 } else { 4 }).contains(digits) =>
            {
                Ok((*base, digits * 4))
            }
            _ => Err("PRUN yalnız uygun genişlikte Kn bit grupları kabul eder".to_string()),
        };
        let (s, sn) = group(&self.source)?;
        let (d, dn) = group(&self.destination)?;
        if !matches!(s.area, Area::X | Area::M) || !matches!(d.area, Area::Y | Area::M) {
            return Err("PRUN kaynağı KnX/KnM, hedefi KnY/KnM olmalı".into());
        }
        Ok((s, sn, d, dn))
    }
    fn aligned(d: Device) -> bool {
        d.index
            .is_multiple_of(if d.area == Area::M { 10 } else { 8 })
    }
    fn slots(&self) -> Result<(Vec<Device>, Vec<Device>), String> {
        let (s, sn, d, dn) = self.groups()?;
        let decimal_source = s.area == Area::M;
        let device = |base: Device, i: u16, gap: bool| -> Result<Device, String> {
            let offset = i + if gap { 2 * (i / 8) } else { 0 };
            Ok(Device {
                area: base.area,
                index: base
                    .index
                    .checked_add(offset)
                    .ok_or("PRUN genişletilmiş bit aralığı taşıyor")?,
            })
        };
        let source = (0..sn)
            .map(|i| device(s, i, decimal_source))
            .collect::<Result<_, _>>()?;
        let destination = (0..sn.min(dn))
            .map(|i| device(d, i, !decimal_source))
            .collect::<Result<_, _>>()?;
        Ok((source, destination))
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        Memory::error_devices(out);
        if let Ok((source, destination)) = self.slots() {
            // Include intervening untouched bits so a monitor can verify gap retention.
            for slots in [source, destination] {
                if let (Some(first), Some(last)) = (slots.first(), slots.last()) {
                    out.extend((first.index..=last.index).map(|index| Device {
                        area: first.area,
                        index,
                    }));
                }
            }
        }
    }
    pub fn observed_devices(&self, _m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
    }
    pub fn validate_native_span(&self, m: &Memory) -> Result<(), String> {
        let (source, destination) = self.slots()?;
        for (slots, write) in [(&source, false), (&destination, true)] {
            for d in slots {
                if (d.area == Area::M && d.index > m.relay_limit.unwrap_or(4095))
                    || d.to_string().parse::<Device>().is_err()
                    || (write && !d.writable_bit())
                {
                    return Err(format!(
                        "PRUN {d} genişletilmiş bit aralığının model dışı fiziksel eşlemesi doğrulanmadı"
                    ));
                }
            }
        }
        Ok(())
    }
    pub fn execute(&self, m: &mut Memory, power: bool, pc: u16) {
        if !power {
            return;
        }
        let Ok((source, _, destination, _)) = self.groups() else {
            m.execution_limitations
                .insert("PRUN bit grubu tanımı geçersiz".into());
            return;
        };
        let offset = if !Self::aligned(source) {
            Some(1)
        } else if !Self::aligned(destination) {
            Some(if self.wide { 5 } else { 3 })
        } else {
            None
        };
        if let Some(offset) = offset {
            m.operation_error(
                0xe1a,
                pc.wrapping_sub(m.native_instruction_pc())
                    .wrapping_add(offset),
            );
            return;
        }
        if let Err(reason) = self.validate_native_span(m) {
            m.execution_limitations.insert(reason);
            return;
        }
        let (source, destination) = self.slots().expect("validated PRUN groups");
        for (i, source) in source.into_iter().enumerate() {
            let value = m.bit(source);
            if let Some(dest) = destination.get(i) {
                m.set_internal(*dest, value);
            }
        }
    }
}
