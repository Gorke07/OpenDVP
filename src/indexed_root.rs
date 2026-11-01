//! The four SQR/DSQR modifier forms. PDF260 allows K/H/D source and D result;
//! only the D roles are grey. Index values remain signed WORDs at both widths.
use crate::calendar_operand::CalendarValue;
use crate::data_processing::RootResult;
use crate::indexed_multiply::validate_model;
use crate::indexed_operand::{AddressedValue, has_modifier};
use crate::plc::{Area, Device, Memory};
use std::collections::BTreeSet;

fn binding(raw: &str, wide: bool, write: bool) -> Result<AddressedValue, String> {
    let value = AddressedValue::parse(raw, wide, write)?;
    let allowed = match &value {
        AddressedValue::Direct(CalendarValue::Additional(value)) => {
            (!write && value.base().is_none()) || value.base().is_some_and(|d| d.area == Area::D)
        }
        AddressedValue::Modified {
            address,
            digits: None,
            ..
        } => address.base.area == Area::D,
        _ => false,
    };
    if !allowed {
        return Err(
            "SQR kaynağı K/H/D, hedefi D olmalı; yalnız D adresine E/F son eki eklenebilir".into(),
        );
    }
    Ok(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreparedRootResult {
    target: Device,
    result: RootResult,
}

impl PreparedRootResult {
    fn write(self, memory: &mut Memory) {
        self.result.write_value(memory, self.target);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreparedRoot {
    Ready(PreparedRootResult),
    InvalidSource,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedRootAction {
    wide: bool,
    pulse_label: bool,
    source: AddressedValue,
    target: AddressedValue,
}

impl IndexedRootAction {
    pub fn supports(name: &str) -> bool {
        matches!(
            name.to_ascii_uppercase().as_str(),
            "SQR" | "SQRP" | "DSQR" | "DSQRP"
        )
    }

    pub fn has_modifiers(args: &[String]) -> bool {
        args.iter().any(|raw| has_modifier(raw))
    }

    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if !Self::supports(name) || args.len() != 2 {
            return Err("İndeksli SQR bir kaynak ve bir hedef gerektirir".into());
        }
        let name = name.to_ascii_uppercase();
        let wide = name.starts_with('D');
        Ok(Self {
            wide,
            pulse_label: name.ends_with('P'),
            source: binding(&args[0], wide, false)?,
            target: binding(&args[1], wide, true)?,
        })
    }

    pub fn source(&self) -> &AddressedValue {
        &self.source
    }

    pub fn resolved_source(&self, memory: &Memory) -> Result<CalendarValue, String> {
        let source = self.source.resolve(memory, false)?;
        let mut devices = BTreeSet::new();
        source.devices(&mut devices);
        validate_model(memory, &devices)?;
        Ok(source)
    }

    fn resolved_target(&self, memory: &Memory) -> Result<CalendarValue, String> {
        let target = self.target.resolve(memory, true)?;
        let mut devices = BTreeSet::new();
        target.devices(&mut devices);
        validate_model(memory, &devices)?;
        Ok(target)
    }

    fn read_result(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<Option<RootResult>, String> {
        observed.extend(self.source.modifier_devices());
        let source = self.resolved_source(memory)?;
        source.devices(observed);
        Ok(RootResult::convert(source.read(memory), self.wide))
    }

    /// Pure preflight: an ordinary signed-negative source is not an address
    /// failure and does not read/qualify/observe the destination index.
    pub fn prepare(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<PreparedRoot, String> {
        let Some(result) = self.read_result(memory, observed)? else {
            return Ok(PreparedRoot::InvalidSource);
        };
        observed.extend(self.target.modifier_devices());
        let target = self.resolved_target(memory)?;
        target.devices(observed);
        Ok(PreparedRoot::Ready(PreparedRootResult {
            target: target.base().expect("D result"),
            result,
        }))
    }

    /// Pulse qualification is owned by the caller. An inactive visit cannot
    /// clear flags or access any source/index, even if its address is invalid.
    pub fn execute(&self, memory: &mut Memory, power: bool, observed: &mut BTreeSet<Device>) {
        if !power {
            return;
        }
        RootResult::begin(memory);
        let result = match self.read_result(memory, observed) {
            Ok(Some(result)) => result,
            Ok(None) => {
                memory.operation_error(0xe1b, 1);
                return;
            }
            Err(reason) => {
                memory.execution_limitations.insert(reason);
                return;
            }
        };
        result.publish_flags(memory);
        observed.extend(self.target.modifier_devices());
        match self.resolved_target(memory) {
            Ok(target) => {
                target.devices(observed);
                PreparedRootResult {
                    target: target.base().expect("D result"),
                    result,
                }
                .write(memory);
            }
            Err(reason) => {
                memory.execution_limitations.insert(reason);
            }
        }
    }

    pub fn dest(&self) -> Device {
        self.target.base().expect("validated D result")
    }

    pub fn label(&self) -> String {
        format!(
            "{}SQR{} {} {}",
            if self.wide { "D" } else { "" },
            if self.pulse_label { "P" } else { "" },
            self.source,
            self.target
        )
    }

    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        self.source.devices(out);
        self.target.devices(out);
        out.extend((1020..=1022).map(Memory::system_device));
        Memory::error_devices(out);
    }

    pub fn modifier_devices(&self) -> Vec<Device> {
        self.source
            .modifier_devices()
            .into_iter()
            .chain(self.target.modifier_devices())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// Monitoring can display a held result after a negative-source failure;
    /// execution footprints come only from prepare/execute's actual accesses.
    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Ok(source) = self.resolved_source(memory) {
            source.devices(out);
        }
        if let Ok(target) = self.resolved_target(memory) {
            target.devices(out);
        }
    }

    pub fn display_value(&self, memory: &Memory) -> String {
        match self.resolved_target(memory) {
            Ok(target) => target.read(memory).to_string(),
            Err(reason) => format!("İndeks: {reason}"),
        }
    }

    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let mut header = if self.wide { 0x7100 } else { 0x7000 };
        if self.pulse_label {
            header |= 0x10;
        }
        let mut words = vec![header];
        words.extend(self.source.native_words()?);
        words.extend(self.target.native_words()?);
        Ok(words)
    }
}
