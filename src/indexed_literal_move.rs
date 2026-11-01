//! PDF576 DMOVR/DMOVRP: finite F literal to an addressed KnY/M/S or T/C/D
//! target. This is raw32 copying, with no floating-point normalization.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::{CalendarDestination, CalendarValue};
use crate::float_ops::FloatOperand;
use crate::indexed_multiply::{qualify_group, validate_model};
use crate::indexed_operand::{AddressedValue, has_modifiers};
use crate::plc::{Area, Device, Memory};
use crate::wide::WideOperand;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedLiteralMove {
    target: CalendarDestination,
    bits: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedLiteralMoveAction {
    bits: u32,
    text: String,
    pulse_label: bool,
    target: AddressedValue,
}
impl IndexedLiteralMoveAction {
    pub fn supports(name: &str) -> bool {
        matches!(name.to_ascii_uppercase().as_str(), "DMOVR" | "DMOVRP")
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        has_modifiers(args)
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if !Self::supports(name) || args.len() != 2 {
            return Err("DMOVR bir sonlu F sabiti ve bir hedef gerektirir".into());
        }
        let FloatOperand::Literal { bits, text } = FloatOperand::parse(&args[0])? else {
            return Err("DMOVR kaynağı yalnız sonlu F sabiti olmalı".into());
        };
        let target = AddressedValue::parse(&args[1], true, true)?;
        let allowed = match &target {
            AddressedValue::Direct(CalendarValue::Additional(value)) => {
                value.base().is_some_and(|d| {
                    matches!(
                        d.area,
                        Area::Y | Area::M | Area::S | Area::T | Area::C | Area::D
                    )
                })
            }
            AddressedValue::Modified {
                address, digits, ..
            } => {
                if digits.is_some() {
                    matches!(address.base.area, Area::Y | Area::M | Area::S)
                } else {
                    matches!(address.base.area, Area::T | Area::C | Area::D)
                }
            }
            _ => false,
        };
        if !allowed {
            return Err("DMOVR hedefi KnY/M/S veya T/C/D olmalı; doğrudan E/F kullanılamaz".into());
        }
        Ok(Self {
            bits,
            text,
            pulse_label: name.eq_ignore_ascii_case("DMOVRP"),
            target,
        })
    }
    /// Resolve one complete writable target. A paired store retains this
    /// result even when its low word overwrites the modifier's own alias.
    pub fn resolve(&self, memory: &Memory) -> Result<CalendarDestination, String> {
        let value = self.target.resolve(memory, true)?;
        if let CalendarValue::Additional(AdditionalValue::Group { base, digits, .. }) = &value {
            qualify_group(self.dest(), *base, digits * 4, false)?;
        }
        let target = CalendarDestination::from_value(value)?;
        let mut devices = BTreeSet::new();
        target.devices(&mut devices);
        validate_model(memory, &devices)?;
        Ok(target)
    }
    pub fn prepare(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<PreparedLiteralMove, String> {
        observed.extend(self.modifier_devices());
        let target = self.resolve(memory)?;
        target.devices(observed);
        Ok(PreparedLiteralMove {
            target,
            bits: self.bits,
        })
    }
    /// The caller owns pulse qualification. OFF/held paths do not resolve or
    /// observe the destination. Target writes use ordinary control notifications.
    pub fn execute(&self, memory: &mut Memory, power: bool, observed: &mut BTreeSet<Device>) {
        if !power {
            return;
        }
        match self.prepare(memory, observed) {
            Ok(prepared) => prepared.target.write(memory, prepared.bits as i32),
            Err(reason) => {
                memory.execution_limitations.insert(reason);
            }
        }
    }
    pub fn dest(&self) -> Device {
        self.target.base().expect("validated DMOVR target")
    }
    pub fn label(&self) -> String {
        format!(
            "DMOVR{} {} {}",
            if self.pulse_label { "P" } else { "" },
            self.text,
            self.target
        )
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        self.target.devices(out);
        out.extend((1020..=1022).map(Memory::system_device));
        Memory::error_devices(out);
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        self.target.modifier_devices()
    }
    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Ok(target) = self.resolve(memory) {
            target.devices(out);
        }
    }
    pub fn display_value(&self, memory: &Memory) -> String {
        match self.resolve(memory) {
            Ok(target) => {
                let bits = target.read(memory) as u32;
                format!("H{bits:08X} / {}", f32::from_bits(bits))
            }
            Err(reason) => format!("İndeks: {reason}"),
        }
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let source = CalendarValue::Additional(AdditionalValue::Double(WideOperand::Constant(
            self.bits as i32,
        )));
        let mut words = vec![if self.pulse_label { 0xf110 } else { 0xf100 }];
        words.extend(crate::es2_encoder::calendar_operand(&source)?);
        words.extend(self.target.native_words()?);
        Ok(words)
    }
}
