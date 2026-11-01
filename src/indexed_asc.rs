//! DRAFT: documented local ASC destination modifiers. No computing native
//! implementation is claimed: installed ASC handlers are retained PC-only stubs.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::CalendarValue;
use crate::indexed_multiply::validate_model;
use crate::indexed_operand::{AddressedValue, has_modifier};
use crate::plc::{Area, Device, Memory, Operand, word_range};
use crate::text_encoding::TextAction;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedAsc {
    letters: [u8; 8],
    target: Device,
    low_only: bool,
}
impl PreparedAsc {
    /// The mode and effective base are fixed before the first write. Admission
    /// has checked every WORD; no partial endpoint or fabricated PLC error.
    pub fn write(self, memory: &mut Memory) {
        // Pending parent-owned seam: delegate to the existing write_bytes
        // implementation with an explicit, already sampled M1161 value.
        crate::text_encoding::write_literal(memory, self.target, &self.letters, self.low_only);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedAscAction {
    letters: [u8; 8],
    target: AddressedValue,
}
impl IndexedAscAction {
    pub fn supports(name: &str) -> bool {
        name.eq_ignore_ascii_case("ASC")
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        // ABCDEFGH contains E/F letters, not operand suffixes. Only the last
        // argument is an address, including the eight separate letter syntax.
        args.last().is_some_and(|raw| has_modifier(raw))
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if !Self::supports(name) {
            return Err("ASC has one documented form; ASCP/DASC are unsupported".into());
        }
        let (literal, raw_target) = match args.len() {
            2 => {
                let raw = &args[0];
                let literal = raw
                    .strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .or_else(|| raw.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
                    .unwrap_or(raw);
                (literal.to_owned(), &args[1])
            }
            9 if args[..8].iter().all(|s| s.len() == 1) => (args[..8].concat(), &args[8]),
            _ => return Err("ASC requires eight English letters and one T/C/D destination".into()),
        };
        if literal.len() != 8 || !literal.bytes().all(|c| c.is_ascii_alphabetic()) {
            return Err("ASC literal must contain exactly eight A..Z/a..z letters".into());
        }
        let target = AddressedValue::parse(raw_target, false, true)?;
        Self::check_target(&target)?;
        Ok(Self {
            letters: literal
                .as_bytes()
                .try_into()
                .expect("validated eight bytes"),
            target,
        })
    }
    fn check_target(target: &AddressedValue) -> Result<(), String> {
        let base = match target {
            AddressedValue::Direct(CalendarValue::Additional(AdditionalValue::Word(
                Operand::Register(base),
            ))) => *base,
            AddressedValue::Modified {
                address,
                digits: None,
                wide: false,
            } => address.base,
            _ => {
                return Err(
                    "ASC destination must be a WORD T/C/D address; no Kn/direct E/F".into(),
                );
            }
        };
        if !matches!(base.area, Area::T | Area::C | Area::D) {
            return Err("ASC destination must be T/C/D".into());
        }
        word_range(base, 1)?;
        Ok(())
    }
    /// No label formatting/reparsing: preserve the existing typed eight bytes.
    pub fn from_legacy(action: &TextAction) -> Result<Self, String> {
        let TextAction::Literal { letters, dest } = action else {
            return Err("ASC literal action required".into());
        };
        if !letters.iter().all(u8::is_ascii_alphabetic) {
            return Err("ASC literal must contain exactly eight English letters".into());
        }
        let target = AddressedValue::Direct(CalendarValue::Additional(AdditionalValue::Word(
            Operand::Register(*dest),
        )));
        Self::check_target(&target)?;
        Ok(Self {
            letters: *letters,
            target,
        })
    }
    pub fn nominal_words(&self) -> u16 {
        11
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        Err("ASC is local-only: installed native handlers only advance PC; no computing upload is qualified".into())
    }
    pub fn dest(&self) -> Device {
        self.target.base().expect("validated ASC target")
    }
    pub fn label(&self) -> String {
        format!(
            "ASC \"{}\" {}",
            String::from_utf8_lossy(&self.letters),
            self.target
        )
    }
    pub fn mode_device(&self) -> Device {
        Memory::system_device(1161)
    }
    pub fn control_devices(&self) -> Vec<Device> {
        vec![self.mode_device()]
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        self.target.modifier_devices()
    }
    pub fn target_modifier_devices(&self) -> Vec<Device> {
        self.modifier_devices()
    }
    fn resolved_span(&self, memory: &Memory, low_only: bool) -> Result<Vec<Device>, String> {
        let value = self.target.resolve(memory, true)?;
        let CalendarValue::Additional(AdditionalValue::Word(Operand::Register(base))) = value
        else {
            return Err("ASC destination lost its validated WORD address".into());
        };
        let span = word_range(base, if low_only { 8 } else { 4 })?;
        validate_model(memory, &span.iter().copied().collect())?;
        Ok(span)
    }
    pub fn prepare(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<PreparedAsc, String> {
        observed.insert(self.mode_device());
        // This is a declared local policy, not a native lifetime claim: sample
        // once, resolve once, require the entire mode-dependent destination.
        let low_only = memory.system_bit(1161);
        observed.extend(self.modifier_devices());
        let span = self.resolved_span(memory, low_only)?;
        observed.extend(span.iter().copied());
        Ok(PreparedAsc {
            letters: self.letters,
            target: span[0],
            low_only,
        })
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
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        self.target.devices(out);
        out.insert(self.mode_device());
        // Do not statically assume eight words: four may be the complete valid
        // span at the selected model endpoint when M1161 is OFF.
    }
    pub fn monitor_input_devices(&self, memory: &Memory) -> Result<BTreeSet<Device>, String> {
        let mut inputs: BTreeSet<_> = self.modifier_devices().into_iter().collect();
        inputs.insert(self.mode_device());
        validate_model(memory, &inputs)?;
        Ok(inputs)
    }
    pub fn capture_devices(&self, memory: &Memory) -> Result<BTreeSet<Device>, String> {
        let mut out = self.monitor_input_devices(memory)?;
        out.extend(self.resolved_span(memory, memory.system_bit(1161))?);
        Ok(out)
    }
    /// Potential monitor destinations, not an assertion of an actual visit.
    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Ok(devices) = self.capture_devices(memory) {
            out.extend(devices);
        }
    }
    pub fn display_value(&self, memory: &Memory) -> String {
        match self.resolved_span(memory, memory.system_bit(1161)) {
            Ok(span) => span
                .iter()
                .map(|d| format!("{:04X}", memory.word(*d) as u16))
                .collect::<Vec<_>>()
                .join(" "),
            Err(reason) => format!("ASC: {reason}"),
        }
    }
}
