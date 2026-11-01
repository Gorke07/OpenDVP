//! Cached address modifiers for the four three-WORD RTC instructions.
//! Environmental clock service and native gate/drift admission belong to the
//! caller. Native snapshots hydrate a disposable safety clone, never local RTC.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::CalendarValue;
use crate::indexed_address::IndexedAddress;
use crate::indexed_operand::{has_modifier, has_modifiers};
use crate::plc::{Area, Device, Memory, Operand, word_range};
pub use crate::rtc_clock::RtcOutcome;
use crate::rtc_clock::{
    copy_live_word, finish_twr, normalize_twr_public, refresh_public, word_limit,
};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtcVisit {
    pub outcome: RtcOutcome,
    pub observed: BTreeSet<Device>,
}

fn data(index: u16) -> Device {
    Device {
        area: Area::D,
        index,
    }
}
fn public_words() -> BTreeSet<Device> {
    (1313..=1319).map(data).collect()
}

/// Original C descriptors are WORD-only. The special indexed resolver's
/// effective C200..255 is nevertheless distinct from an invalid resolver: the
/// handler subsequently stops TRD or rejects TWR's seven-WORD extent.
#[derive(Debug, Clone, PartialEq)]
struct RtcBlockAddress {
    base: Device,
    address: Option<IndexedAddress>,
}
impl RtcBlockAddress {
    fn parse(raw: &str) -> Result<Self, String> {
        let text = raw.trim().to_ascii_uppercase();
        let address = if has_modifier(&text) {
            Some(IndexedAddress::parse(&text)?)
        } else {
            None
        };
        let base = match address {
            Some(a) => a.base,
            None => text.parse()?,
        };
        word_range(base, 1)?;
        Ok(Self { base, address })
    }
    fn modifier(&self) -> Option<Device> {
        self.address.map(|a| a.index.device())
    }
    fn resolve(&self, m: &Memory) -> Result<Device, String> {
        let index = match self.address {
            Some(a) => u16::try_from(a.base.index.wrapping_add(a.index.read(m) as u16) as i16)
                .map_err(|_| "RTC değiştiricisi negatif etkin adres üretiyor")?,
            None => self.base.index,
        };
        let maximum = match self.base.area {
            Area::T | Area::C => 255,
            Area::D => m.data_register_limit.unwrap_or(9999),
            _ => return Err("RTC özel çözümleyicisi T/C/D adresi gerektirir".into()),
        };
        if index > maximum {
            return Err("RTC etkin temel adresi özel çözümleyici alanını aşıyor".into());
        }
        Ok(Device { index, ..self.base })
    }
    fn devices(&self, out: &mut BTreeSet<Device>) {
        // Base-only static inventory is intentional: direct model endpoints
        // have defined TRD partial or TWR extent-error behavior at execution.
        out.insert(self.modifier().unwrap_or(self.base));
    }
    fn native_words(&self) -> Result<Vec<u16>, String> {
        match self.address {
            Some(a) => a.native_words(false, None),
            None => crate::es2_encoder::calendar_operand(&CalendarValue::Additional(
                AdditionalValue::Word(Operand::Register(self.base)),
            )),
        }
    }
}
impl std::fmt::Display for RtcBlockAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.address {
            Some(a) => a.fmt(f),
            None => self.base.fmt(f),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedRtcAction {
    read: bool,
    pulse_label: bool,
    block: RtcBlockAddress,
}

struct Context<'a, F> {
    m: &'a mut Memory,
    observed: &'a mut BTreeSet<Device>,
    fetch: F,
    captured: BTreeSet<Device>,
    written: BTreeSet<Device>,
    native: bool,
    decoder_diagnostic: bool,
}
impl<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>> Context<'_, F> {
    fn capture(&mut self, devices: BTreeSet<Device>) -> Result<(), String> {
        let missing = devices
            .difference(&self.captured)
            .copied()
            .filter(|d| !self.written.contains(d))
            .collect();
        (self.fetch)(self.m, &missing)?;
        self.captured.extend(&devices);
        self.observed.extend(devices);
        Ok(())
    }
    fn owns(&mut self, devices: BTreeSet<Device>) {
        self.written.extend(&devices);
        self.observed.extend(devices);
    }
    fn flag_input(&mut self, index: u16) -> Result<(), String> {
        self.capture(BTreeSet::from([Memory::system_device(index)]))
    }
    fn error_inputs(&mut self) -> Result<BTreeSet<Device>, String> {
        let mut ds = BTreeSet::new();
        Memory::error_devices(&mut ds);
        self.capture(ds.clone())?;
        Ok(ds)
    }
    fn error(&mut self, offset: u16) -> Result<RtcOutcome, String> {
        let ds = self.error_inputs()?;
        self.m.operation_error(0xe1a, offset);
        self.owns(ds);
        Ok(RtcOutcome::PlcError { offset })
    }
    fn refresh(&mut self) -> Result<(), String> {
        // Preserve the ordinary explicit-seed failure before reading any public
        // control or modifier. No native private shadow is imported here.
        if self.m.rtc_clock.shadow().is_none() {
            refresh_public(self.m)?;
        }
        self.flag_input(1016)?;
        refresh_public(self.m)?;
        self.owns(public_words());
        Ok(())
    }
    fn resolve(&mut self, binding: &RtcBlockAddress) -> Result<Result<Device, String>, String> {
        self.capture(binding.modifier().into_iter().collect())?;
        Ok(binding.resolve(self.m))
    }
    fn copy(&mut self, source: Device, dest: Device) -> Result<(), String> {
        self.capture(BTreeSet::from([source]))?;
        copy_live_word(self.m, source, dest);
        self.owns(BTreeSet::from([dest]));
        Ok(())
    }
    fn finish_write(&mut self) -> Result<RtcOutcome, String> {
        // Every public row was already written by the forward source copy.
        // The shared kernel clamps ALL fields; it does not stop at first error.
        let valid = normalize_twr_public(self.m);
        self.owns(public_words());
        let error_rows = if valid {
            self.flag_input(1016)?;
            BTreeSet::new()
        } else {
            self.error_inputs()?
        };
        let outcome = finish_twr(self.m, valid);
        self.owns(error_rows);
        self.owns(BTreeSet::from([Memory::system_device(1082)]));
        Ok(outcome)
    }
}

impl IndexedRtcAction {
    fn spec(name: &str) -> Option<(bool, bool)> {
        Some(match name.to_ascii_uppercase().as_str() {
            "TRD" => (true, false),
            "TRDP" => (true, true),
            "TWR" => (false, false),
            "TWRP" => (false, true),
            _ => return None,
        })
    }
    pub fn supports(name: &str) -> bool {
        Self::spec(name).is_some()
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        has_modifiers(args)
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let (read, pulse_label) = Self::spec(name).ok_or("İndeksli RTC komutu geçersiz")?;
        if args.len() != 1 {
            return Err(format!("{name}: bir T/C/D operandı gerekli"));
        }
        Ok(Self {
            read,
            pulse_label,
            block: RtcBlockAddress::parse(&args[0])?,
        })
    }
    /// Typed adapter for an existing manually constructed ordinary action.
    /// Pulse execution remains the enclosing instruction's responsibility.
    pub fn from_legacy(action: &crate::rtc_clock::RtcAction) -> Self {
        let (read, base) = match action {
            crate::rtc_clock::RtcAction::Read { dest } => (true, *dest),
            crate::rtc_clock::RtcAction::Write { source } => (false, *source),
        };
        Self {
            read,
            pulse_label: false,
            block: RtcBlockAddress {
                base,
                address: None,
            },
        }
    }
    pub fn is_read(&self) -> bool {
        self.read
    }
    pub fn is_write(&self) -> bool {
        !self.read
    }
    pub fn nominal_words(&self) -> u16 {
        3
    }
    pub fn dest(&self) -> Device {
        self.block.base
    }
    pub fn name(&self) -> &'static str {
        if self.read { "TRD" } else { "TWR" }
    }
    pub fn label(&self) -> String {
        format!(
            "{}{} {}",
            self.name(),
            if self.pulse_label { "P" } else { "" },
            self.block
        )
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let mut result = vec![
            (if self.read { 0x6a01 } else { 0x6c01 }) | if self.pulse_label { 0x10 } else { 0 },
        ];
        result.extend(self.block.native_words()?);
        Ok(result)
    }
    fn run<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>>(
        &self,
        c: &mut Context<'_, F>,
    ) -> Result<RtcOutcome, String> {
        if self.read {
            c.refresh()?;
        }
        let base = match c.resolve(&self.block)? {
            Ok(base) => base,
            Err(reason) => {
                // The local fixed descriptor contract is E1A+1. Native invalid
                // decode qualification is deliberately a separate diagnostic.
                if c.native && !c.decoder_diagnostic {
                    return Err(format!(
                        "RTC geçersiz çözümleyici yolu native ana koşuya alınmadı: {reason}"
                    ));
                }
                return c.error(1);
            }
        };
        let last = word_limit(base, c.m);
        if self.read {
            for n in 0..7u16 {
                let Some(index) = base.index.checked_add(n).filter(|i| *i <= last) else {
                    return Ok(RtcOutcome::PartialRead { written: n as u8 });
                };
                c.copy(data(1319 - n), Device { index, ..base })?;
            }
            Ok(RtcOutcome::Complete)
        } else {
            if base.index.checked_add(6).is_none_or(|end| end > last) {
                return c.error(3);
            }
            for n in 0..7u16 {
                c.copy(
                    Device {
                        index: base.index + n,
                        ..base
                    },
                    data(1319 - n),
                )?;
            }
            c.finish_write()
        }
    }
    pub fn execute(
        &self,
        m: &mut Memory,
        power: bool,
        observed: &mut BTreeSet<Device>,
    ) -> RtcOutcome {
        if !power {
            return RtcOutcome::Nominal;
        }
        match self.run(&mut Context {
            m,
            observed,
            fetch: |_: &mut Memory, _: &BTreeSet<Device>| Ok(()),
            captured: BTreeSet::new(),
            written: BTreeSet::new(),
            native: false,
            decoder_diagnostic: false,
        }) {
            Ok(outcome) => outcome,
            Err(reason) => {
                m.execution_limitations.insert(reason.clone());
                RtcOutcome::Limitation(reason)
            }
        }
    }
    /// Disposable, active-path safety replay. Parent owns full L/master/P and
    /// independent environmental drift checks; no native result seeds local RTC.
    pub fn qualify_native_with<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>>(
        &self,
        m: &Memory,
        fetch: F,
    ) -> RtcVisit {
        self.qualify_native_mode(m, fetch, false)
    }
    /// Separate safety replay for an explicitly labeled decoder-status
    /// diagnostic. Caller must independently qualify the unchanged image,
    /// gate, next PC, internal status persistence and environmental window.
    pub fn qualify_native_decoder_with<
        F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>,
    >(
        &self,
        m: &Memory,
        fetch: F,
    ) -> RtcVisit {
        self.qualify_native_mode(m, fetch, true)
    }
    fn qualify_native_mode<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>>(
        &self,
        m: &Memory,
        fetch: F,
        decoder_diagnostic: bool,
    ) -> RtcVisit {
        let mut clone = m.clone();
        let mut observed = BTreeSet::new();
        let outcome = match self.run(&mut Context {
            m: &mut clone,
            observed: &mut observed,
            fetch,
            captured: BTreeSet::new(),
            written: BTreeSet::new(),
            native: true,
            decoder_diagnostic,
        }) {
            Ok(RtcOutcome::PartialRead { written }) if !decoder_diagnostic => {
                RtcOutcome::Limitation(format!(
                    "RTC TRD {written}/7 kısmi yazımın iç çözümleyici durumu native ana koşuda doğrulanmadı"
                ))
            }
            Ok(outcome) => outcome,
            Err(reason) => RtcOutcome::Limitation(reason),
        };
        RtcVisit { outcome, observed }
    }
    pub fn qualify_native(&self, m: &Memory) -> RtcVisit {
        self.qualify_native_with(m, |_, _| Ok(()))
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        self.block.modifier().into_iter().collect()
    }
    /// M1017 is an external engine-service input, not an instruction-body read.
    pub fn control_devices(&self) -> Vec<Device> {
        vec![Memory::system_device(1016)]
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        self.block.devices(out);
        out.extend(public_words());
        out.extend([1016, 1017, 1082].map(Memory::system_device));
        Memory::error_devices(out);
    }
    pub fn monitor_input_devices(&self, _m: &Memory) -> Result<BTreeSet<Device>, String> {
        Ok(self.modifier_devices().into_iter().collect())
    }
    /// Potential address display only. Incomplete TWR reads no payload, whereas
    /// TRD has a clipped output. Runtime observations come from execute/qualify.
    pub fn capture_devices(&self, m: &Memory) -> Result<BTreeSet<Device>, String> {
        let mut ds = self.monitor_input_devices(m)?;
        let base = self.block.resolve(m)?;
        let last = word_limit(base, m);
        if !self.read && base.index.checked_add(6).is_none_or(|end| end > last) {
            return Ok(ds);
        }
        for n in 0..7 {
            if let Some(index) = base.index.checked_add(n).filter(|i| *i <= last) {
                ds.insert(Device { index, ..base });
            }
        }
        Ok(ds)
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Ok(ds) = self.capture_devices(m) {
            out.extend(ds);
        }
    }
    pub fn display_value(&self, m: &Memory) -> String {
        match self.block.resolve(m) {
            Ok(base) => {
                let last = word_limit(base, m);
                let fields: Vec<_> = (0..7)
                    .map(|n| {
                        base.index
                            .checked_add(n)
                            .filter(|i| *i <= last)
                            .map(|index| m.word(Device { index, ..base }).to_string())
                            .unwrap_or_else(|| "—".into())
                    })
                    .collect();
                fields.join(" / ")
            }
            Err(reason) => format!("İndeks: {reason}"),
        }
    }
}
