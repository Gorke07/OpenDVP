//! SEGD/SEGDP gray-address modifiers. Ordinary Kn and direct E/F scalar roles
//! already exist in TextAction; this wrapper adds only address suffix bindings.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::{CalendarDestination, CalendarValue};
use crate::indexed_multiply::{qualify_group, validate_model};
use crate::indexed_operand::{AddressedValue, has_modifiers, read_resolved_value};
use crate::plc::{Device, Memory};
use crate::text_encoding::{TextAction, segment_result};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedSegment {
    target: CalendarDestination,
    value: i16,
}
impl PreparedSegment {
    /// One terminal generic WORD/group write. Both the source snapshot and
    /// complete destination address are fixed before this store begins.
    pub fn write(self, memory: &mut Memory) {
        self.target.write(memory, i32::from(self.value));
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedSegmentAction {
    source: AddressedValue,
    target: AddressedValue,
    pulse_label: bool,
}
impl IndexedSegmentAction {
    pub fn supports(name: &str) -> bool {
        matches!(name.to_ascii_uppercase().as_str(), "SEGD" | "SEGDP")
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        has_modifiers(args)
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if !Self::supports(name) {
            return Err("İndeksli SEGD/SEGDP komutu gerekli".into());
        }
        if args.len() != 2 {
            return Err(format!("{name}: kaynak ve hedef gerekli"));
        }
        Ok(Self {
            source: AddressedValue::parse(&args[0], false, false)?,
            target: AddressedValue::parse(&args[1], false, true)?,
            pulse_label: name.eq_ignore_ascii_case("SEGDP"),
        })
    }
    /// Typed legacy adapter; no formatted-source reparsing or TextAction shape
    /// change. The outer Action::Pulse remains the owner of pulse history.
    pub fn from_legacy(action: &TextAction) -> Result<Self, String> {
        let TextAction::Segment { source, dest } = action else {
            return Err("SEGD metin eylemi gerekli".into());
        };
        if source.wide() || dest.wide() {
            return Err("SEGD kaynak ve hedefi WORD genişliğinde olmalı".into());
        }
        Ok(Self {
            source: AddressedValue::Direct(source.clone()),
            target: AddressedValue::Direct(dest.value().clone()),
            pulse_label: false,
        })
    }
    pub fn source_count(&self) -> usize {
        1
    }
    pub fn source_modifier_devices(&self, index: usize) -> Vec<Device> {
        if index == 0 {
            self.source.modifier_devices()
        } else {
            vec![]
        }
    }
    pub fn target_modifier_devices(&self) -> Vec<Device> {
        self.target.modifier_devices()
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        let mut ds = self.source.modifier_devices();
        for d in self.target.modifier_devices() {
            if !ds.contains(&d) {
                ds.push(d);
            }
        }
        ds
    }
    pub fn control_devices(&self) -> Vec<Device> {
        vec![]
    }
    fn qualify(
        binding: &AddressedValue,
        value: &CalendarValue,
        memory: &Memory,
    ) -> Result<(), String> {
        if let CalendarValue::Additional(AdditionalValue::Group { base, digits, .. }) = value {
            // Generic WORD helpers use the strict >1280 remap boundary, not
            // the >=1280 special block/MUL resolver. Preserve existing bounds.
            qualify_group(
                binding.base().expect("group base"),
                *base,
                digits * 4,
                false,
            )?;
        }
        let mut ds = BTreeSet::new();
        value.devices(&mut ds);
        validate_model(memory, &ds)
    }
    pub fn resolved_source(&self, index: usize, memory: &Memory) -> Result<CalendarValue, String> {
        if index != 0 {
            return Err("SEGD yalnız bir kaynak okur".into());
        }
        let value = self.source.resolve(memory, false)?;
        Self::qualify(&self.source, &value, memory)?;
        Ok(value)
    }
    fn resolved_target(&self, memory: &Memory) -> Result<CalendarDestination, String> {
        let value = self.target.resolve(memory, true)?;
        Self::qualify(&self.target, &value, memory)?;
        CalendarDestination::from_value(value)
    }
    pub fn qualify_target(&self, memory: &Memory) -> Result<(), String> {
        self.resolved_target(memory).map(|_| ())
    }
    pub fn prepare(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<PreparedSegment, String> {
        observed.extend(self.source_modifier_devices(0));
        let source = self.resolved_source(0, memory)?;
        // Reuse the already-closed shared scalar-clock contract. This is not a
        // new SEGD-only clock policy and does not change ordinary TextAction.
        let snapshot = read_resolved_value(&source, memory, observed)? as i16;
        let value = segment_result(snapshot);
        observed.extend(self.target_modifier_devices());
        let target = self.resolved_target(memory)?;
        target.devices(observed);
        Ok(PreparedSegment { target, value })
    }
    pub fn execute(&self, memory: &mut Memory, power: bool, observed: &mut BTreeSet<Device>) {
        if !power {
            return;
        }
        match self.prepare(memory, observed) {
            Ok(prepared) => prepared.write(memory),
            Err(reason) => {
                memory.execution_limitations.insert(reason);
            }
        }
    }
    /// Read-only staged safety qualification. No native result is committed to
    /// local memory, and invalid source never requests the target modifier.
    /// There is no intermediate PLC store: the writer is terminal, so a written
    /// overlay is unnecessary here (unlike RTC/FIFO progressive operations).
    pub fn qualify_native_with<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>>(
        &self,
        memory: &Memory,
        mut fetch: F,
    ) -> Result<BTreeSet<Device>, String> {
        fn capture<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>>(
            clone: &mut Memory,
            ds: BTreeSet<Device>,
            captured: &mut BTreeSet<Device>,
            observed: &mut BTreeSet<Device>,
            fetch: &mut F,
        ) -> Result<(), String> {
            let missing = ds.difference(captured).copied().collect();
            fetch(clone, &missing)?;
            captured.extend(&ds);
            observed.extend(ds);
            Ok(())
        }
        let mut clone = memory.clone();
        let mut captured = BTreeSet::new();
        let mut observed = BTreeSet::new();
        capture(
            &mut clone,
            self.source_modifier_devices(0).into_iter().collect(),
            &mut captured,
            &mut observed,
            &mut fetch,
        )?;
        let source = self.resolved_source(0, &clone)?;
        let mut sources = BTreeSet::new();
        source.devices(&mut sources);
        capture(
            &mut clone,
            sources,
            &mut captured,
            &mut observed,
            &mut fetch,
        )?;
        // Actual source is consumed before touching destination metadata. The
        // lookup result cannot affect target address/extent and is not returned.
        let _ = read_resolved_value(&source, &clone, &mut observed)?;
        capture(
            &mut clone,
            self.target_modifier_devices().into_iter().collect(),
            &mut captured,
            &mut observed,
            &mut fetch,
        )?;
        self.resolved_target(&clone)?.devices(&mut observed);
        Ok(observed)
    }
    pub fn qualify_native(&self, memory: &Memory) -> Result<BTreeSet<Device>, String> {
        self.qualify_native_with(memory, |_, _| Ok(()))
    }
    pub fn nominal_words(&self) -> u16 {
        5
    }
    pub fn dest(&self) -> Device {
        self.target.base().expect("validated SEGD destination")
    }
    pub fn label(&self) -> String {
        format!(
            "SEGD{} {} {}",
            if self.pulse_label { "P" } else { "" },
            self.source,
            self.target
        )
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let mut words = vec![if self.pulse_label { 0xa210 } else { 0xa200 }];
        words.extend(self.source.native_words()?);
        words.extend(self.target.native_words()?);
        Ok(words)
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        self.source.devices(out);
        self.target.devices(out);
        out.extend((1020..=1022).map(Memory::system_device));
        Memory::error_devices(out);
    }
    pub fn monitor_input_devices(&self, memory: &Memory) -> Result<BTreeSet<Device>, String> {
        let ds = self.modifier_devices().into_iter().collect();
        validate_model(memory, &ds)?;
        Ok(ds)
    }
    /// Potential fixed-span monitor expansion; not actual visited reads.
    pub fn capture_devices(&self, memory: &Memory) -> Result<BTreeSet<Device>, String> {
        let mut ds = self.monitor_input_devices(memory)?;
        self.resolved_source(0, memory)?.devices(&mut ds);
        self.resolved_target(memory)?.devices(&mut ds);
        Ok(ds)
    }
    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Ok(ds) = self.capture_devices(memory) {
            out.extend(ds);
        }
    }
    pub fn display_value(&self, memory: &Memory) -> String {
        match self.resolved_target(memory) {
            Ok(target) => target.read(memory).to_string(),
            Err(reason) => format!("İndeks: {reason}"),
        }
    }
}
