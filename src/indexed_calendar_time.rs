//! Gray-address TCMP/TZCP/TADD/TSUB bindings and exact per-visit outcomes.
//! Parent owns verified native tail profiles, gating and header-PC disposition.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::CalendarValue;
use crate::indexed_address::IndexedAddress;
use crate::indexed_comparison::ComparisonTarget;
use crate::indexed_multiply::{qualify_group, validate_model};
use crate::indexed_operand::{AddressedValue, has_modifier, has_modifiers, read_resolved_value};
use crate::plc::{Area, Device, Memory, Operand, comparison_destinations, word_range};
// Shared pure numerical seams; read, validation, flag/store order and fresh
// success classification live here. Parent owns the PC/tail disposition.
pub use crate::time_ops::CalendarTimeOutcome;
use crate::time_ops::{arithmetic_field, field_valid};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarTimeVisit {
    pub outcome: CalendarTimeOutcome,
    pub observed: BTreeSet<Device>,
}

/// Lexical endpoint bases may be rescued by signed modifiers, but EVERY
/// effective block must have three actual backed WORDs. No endpoint clipping.
#[derive(Debug, Clone, PartialEq)]
struct TripleBinding {
    base: Device,
    address: Option<IndexedAddress>,
}
impl TripleBinding {
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
        if !matches!(base.area, Area::T | Area::C | Area::D) {
            return Err("Saat bloğu üç WORD T/C/D adresi olmalı; K/H/Kn/E/F blok olamaz".into());
        }
        // Original C200+ is not a word descriptor, even with negative offset.
        word_range(base, if address.is_some() { 1 } else { 3 })?;
        Ok(Self { base, address })
    }
    fn modifier(&self) -> Option<Device> {
        self.address.map(|a| a.index.device())
    }
    fn resolve(&self, m: &Memory) -> Result<[Device; 3], String> {
        let base = match self.address {
            Some(a) => {
                let index = u16::try_from(a.base.index.wrapping_add(a.index.read(m) as u16) as i16)
                    .map_err(|_| "İndeksli saat bloğu negatif adrese taşıyor")?;
                Device { index, ..a.base }
            }
            None => self.base,
        };
        let ds = word_range(base, 3)?;
        validate_model(m, &ds.iter().copied().collect())?;
        Ok(ds.try_into().expect("three-word range"))
    }
    fn devices(&self, out: &mut BTreeSet<Device>) {
        if let Some(d) = self.modifier() {
            out.insert(d);
        } else {
            out.extend(word_range(self.base, 3).expect("validated direct triple"));
        }
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
impl std::fmt::Display for TripleBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.address {
            Some(a) => a.fmt(f),
            None => self.base.fmt(f),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Compare {
        setpoint: [AddressedValue; 3],
        source: TripleBinding,
        target: ComparisonTarget,
    },
    Zone {
        low: TripleBinding,
        high: TripleBinding,
        source: TripleBinding,
        target: ComparisonTarget,
    },
    Arithmetic {
        subtract: bool,
        left: TripleBinding,
        right: TripleBinding,
        target: TripleBinding,
    },
}
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedCalendarTimeAction {
    kind: Kind,
    pulse_label: bool,
}

fn resolved_scalar(binding: &AddressedValue, m: &Memory) -> Result<CalendarValue, String> {
    let value = binding.resolve(m, false)?;
    if let CalendarValue::Additional(AdditionalValue::Group { base, digits, .. }) = &value {
        qualify_group(
            binding.base().expect("scalar group"),
            *base,
            digits * 4,
            false,
        )?;
    }
    let mut ds = BTreeSet::new();
    value.devices(&mut ds);
    validate_model(m, &ds)?;
    Ok(value)
}

struct Context<'a, F> {
    m: &'a mut Memory,
    observed: &'a mut BTreeSet<Device>,
    fetch: F,
    captured: BTreeSet<Device>,
    written: BTreeSet<Device>,
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
    fn scalar(&mut self, binding: &AddressedValue) -> Result<i16, String> {
        self.capture(binding.modifier_devices().into_iter().collect())?;
        let value = resolved_scalar(binding, self.m)?;
        let mut ds = BTreeSet::new();
        value.devices(&mut ds);
        self.capture(ds)?;
        Ok(read_resolved_value(&value, self.m, self.observed)? as i16)
    }
    fn triple(&mut self, binding: &TripleBinding) -> Result<[Device; 3], String> {
        self.capture(binding.modifier().into_iter().collect())?;
        binding.resolve(self.m)
    }
    fn word(&mut self, device: Device) -> Result<i16, String> {
        self.capture(BTreeSet::from([device]))?;
        Ok(self.m.word(device))
    }
    fn put_word(&mut self, device: Device, value: i16) {
        self.m.set_word(device, value);
        // Even equal writes are owned by replay, never recaptured as old native.
        self.written.insert(device);
        self.observed.insert(device);
    }
    fn flag(&mut self, index: u16, value: bool) {
        let device = Memory::system_device(index);
        self.m.set_system(index, value);
        self.written.insert(device);
        self.observed.insert(device);
    }
    fn error(&mut self, offset: u16) -> Result<CalendarTimeOutcome, String> {
        let mut ds = BTreeSet::new();
        Memory::error_devices(&mut ds);
        self.capture(ds.clone())?;
        self.m.operation_error(0xe1a, offset);
        self.written.extend(ds);
        Ok(CalendarTimeOutcome::PlcError { offset })
    }
    /// Native validates hour/minute/second sequentially, then rereads all three
    /// to compute seconds. No public store occurs between the two read passes.
    fn time(&mut self, addresses: [Device; 3]) -> Result<Option<i32>, String> {
        for (offset, device) in addresses.into_iter().enumerate() {
            if !field_valid(self.word(device)?, offset as u16) {
                return Ok(None);
            }
        }
        let h = i32::from(self.word(addresses[0])?);
        let m = i32::from(self.word(addresses[1])?);
        let s = i32::from(self.word(addresses[2])?);
        Ok(Some(h * 3600 + m * 60 + s))
    }
    fn result(&mut self, binding: &ComparisonTarget, bits: [bool; 3]) -> Result<(), String> {
        self.capture(binding.modifier_devices().into_iter().collect())?;
        let base = binding.resolve(self.m)?;
        let devices = comparison_destinations(base)?;
        validate_model(self.m, &devices.iter().copied().collect())?;
        for (device, on) in devices.into_iter().zip(bits) {
            self.m.set_internal(device, on);
            self.written.insert(device);
            self.observed.insert(device);
        }
        Ok(())
    }
}

impl IndexedCalendarTimeAction {
    /// Typed adapter for manually constructed ordinary arithmetic actions.
    /// Public IL uses this wrapper directly; no runtime source parsing occurs.
    pub(crate) fn from_arithmetic(action: &crate::time_ops::TimeAction) -> Option<Self> {
        if let crate::time_ops::TimeAction::Arithmetic {
            subtract,
            left,
            right,
            dest,
        } = action
        {
            let binding = |base| TripleBinding {
                base,
                address: None,
            };
            Some(Self {
                kind: Kind::Arithmetic {
                    subtract: *subtract,
                    left: binding(*left),
                    right: binding(*right),
                    target: binding(*dest),
                },
                pulse_label: false,
            })
        } else {
            None
        }
    }
    fn spec(name: &str) -> Option<(&'static str, bool)> {
        Some(match name.to_ascii_uppercase().as_str() {
            "TCMP" => ("TCMP", false),
            "TCMPP" => ("TCMP", true),
            "TZCP" => ("TZCP", false),
            "TZCPP" => ("TZCP", true),
            "TADD" => ("TADD", false),
            "TADDP" => ("TADD", true),
            "TSUB" => ("TSUB", false),
            "TSUBP" => ("TSUB", true),
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
        let (stem, pulse_label) = Self::spec(name).ok_or("İndeksli saat komutu geçersiz")?;
        let expected = match stem {
            "TCMP" => 5,
            "TZCP" => 4,
            _ => 3,
        };
        if args.len() != expected {
            return Err(format!("{name}: {expected} operand gerekli"));
        }
        let kind = match stem {
            "TCMP" => Kind::Compare {
                setpoint: [
                    AddressedValue::parse(&args[0], false, false)?,
                    AddressedValue::parse(&args[1], false, false)?,
                    AddressedValue::parse(&args[2], false, false)?,
                ],
                source: TripleBinding::parse(&args[3])?,
                target: ComparisonTarget::parse(&args[4].to_ascii_uppercase())?,
            },
            "TZCP" => Kind::Zone {
                low: TripleBinding::parse(&args[0])?,
                high: TripleBinding::parse(&args[1])?,
                source: TripleBinding::parse(&args[2])?,
                target: ComparisonTarget::parse(&args[3].to_ascii_uppercase())?,
            },
            _ => Kind::Arithmetic {
                subtract: stem == "TSUB",
                left: TripleBinding::parse(&args[0])?,
                right: TripleBinding::parse(&args[1])?,
                target: TripleBinding::parse(&args[2])?,
            },
        };
        Ok(Self { kind, pulse_label })
    }
    pub fn is_arithmetic(&self) -> bool {
        matches!(self.kind, Kind::Arithmetic { .. })
    }
    pub fn nominal_words(&self) -> u16 {
        match self.kind {
            Kind::Compare { .. } => 11,
            Kind::Zone { .. } => 9,
            _ => 7,
        }
    }
    fn run<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>>(
        &self,
        c: &mut Context<'_, F>,
    ) -> Result<CalendarTimeOutcome, String> {
        match &self.kind {
            Kind::Compare {
                setpoint,
                source,
                target,
            } => {
                let mut values = [0i16; 3];
                for (i, binding) in setpoint.iter().enumerate() {
                    values[i] = c.scalar(binding)?;
                    if !field_valid(values[i], i as u16) {
                        return c.error(1 + i as u16 * 2);
                    }
                }
                let source = c.triple(source)?;
                let Some(current) = c.time(source)? else {
                    return c.error(7);
                };
                let set =
                    i32::from(values[0]) * 3600 + i32::from(values[1]) * 60 + i32::from(values[2]);
                c.result(target, [set > current, set == current, set < current])?;
                Ok(CalendarTimeOutcome::Nominal)
            }
            Kind::Zone {
                low,
                high,
                source,
                target,
            } => {
                let mut values = [0i32; 3];
                for (i, binding) in [low, high, source].into_iter().enumerate() {
                    let addresses = c.triple(binding)?;
                    let Some(value) = c.time(addresses)? else {
                        return c.error(1 + i as u16 * 2);
                    };
                    values[i] = value;
                }
                c.result(
                    target,
                    [
                        values[2] < values[0].min(values[1]),
                        values[2] >= values[0].min(values[1])
                            && values[2] <= values[0].max(values[1]),
                        values[2] > values[0].max(values[1]),
                    ],
                )?;
                Ok(CalendarTimeOutcome::Nominal)
            }
            Kind::Arithmetic {
                subtract,
                left,
                right,
                target,
            } => {
                // All three pointers are cached before any payload read. A
                // later store may overwrite a suffix alias without redirecting.
                let left = c.triple(left)?;
                let right = c.triple(right)?;
                let target = c.triple(target)?;
                let mut carry = 0i16;
                let mut output = [0i16; 3];
                for offset in [2usize, 1, 0] {
                    let a = c.word(left[offset])?;
                    let b = c.word(right[offset])?;
                    if !field_valid(a, offset as u16) || !field_valid(b, offset as u16) {
                        return c.error(3);
                    }
                    let (value, next) = arithmetic_field(a, b, carry, offset as u16, *subtract);
                    carry = next;
                    output[offset] = value;
                    // Final carry/borrow publication precedes the hour store;
                    // a later invalid field never publishes any final ALU flags.
                    if offset == 0 {
                        c.flag(if *subtract { 1021 } else { 1022 }, carry != 0);
                    }
                    c.put_word(target[offset], value);
                }
                c.flag(1020, output == [0; 3]);
                Ok(CalendarTimeOutcome::ArithmeticSuccess)
            }
        }
    }
    pub fn execute(
        &self,
        m: &mut Memory,
        power: bool,
        observed: &mut BTreeSet<Device>,
    ) -> CalendarTimeOutcome {
        if !power {
            return CalendarTimeOutcome::Nominal;
        }
        match self.run(&mut Context {
            m,
            observed,
            fetch: |_: &mut Memory, _: &BTreeSet<Device>| Ok(()),
            captured: BTreeSet::new(),
            written: BTreeSet::new(),
        }) {
            Ok(outcome) => outcome,
            Err(reason) => {
                m.execution_limitations.insert(reason.clone());
                CalendarTimeOutcome::Limitation(reason)
            }
        }
    }
    /// Active-path qualification only. Parent independently qualifies actual
    /// native L/master/P first and never uses this clone to mutate local flow.
    pub fn qualify_native_with<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>>(
        &self,
        m: &Memory,
        fetch: F,
    ) -> CalendarTimeVisit {
        let mut clone = m.clone();
        let mut observed = BTreeSet::new();
        let outcome = match self.run(&mut Context {
            m: &mut clone,
            observed: &mut observed,
            fetch,
            captured: BTreeSet::new(),
            written: BTreeSet::new(),
        }) {
            Ok(outcome) => outcome,
            Err(reason) => CalendarTimeOutcome::Limitation(reason),
        };
        CalendarTimeVisit { outcome, observed }
    }
    pub fn qualify_native(&self, m: &Memory) -> CalendarTimeVisit {
        self.qualify_native_with(m, |_, _| Ok(()))
    }
    fn triples(&self) -> Vec<&TripleBinding> {
        match &self.kind {
            Kind::Compare { source, .. } => vec![source],
            Kind::Zone {
                low, high, source, ..
            } => vec![low, high, source],
            Kind::Arithmetic {
                left,
                right,
                target,
                ..
            } => vec![left, right, target],
        }
    }
    fn target(&self) -> Option<&ComparisonTarget> {
        match &self.kind {
            Kind::Compare { target, .. } | Kind::Zone { target, .. } => Some(target),
            _ => None,
        }
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        let mut ds: BTreeSet<Device> = self
            .triples()
            .into_iter()
            .filter_map(TripleBinding::modifier)
            .collect();
        if let Kind::Compare { setpoint, .. } = &self.kind {
            for v in setpoint {
                ds.extend(v.modifier_devices());
            }
        }
        if let Some(target) = self.target() {
            ds.extend(target.modifier_devices());
        }
        ds.into_iter().collect()
    }
    pub fn control_devices(&self) -> Vec<Device> {
        vec![]
    }
    pub fn dest(&self) -> Device {
        match &self.kind {
            Kind::Compare { target, .. } | Kind::Zone { target, .. } => target.base(),
            Kind::Arithmetic { target, .. } => target.base,
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        for v in self.triples() {
            v.devices(out);
        }
        if let Kind::Compare { setpoint, .. } = &self.kind {
            for v in setpoint {
                v.devices(out);
            }
        }
        if let Some(target) = self.target() {
            target.devices(out);
        }
        if self.is_arithmetic() {
            out.extend((1020..=1022).map(Memory::system_device));
        }
        Memory::error_devices(out);
    }
    /// Fixed spans need only actual suffix rows for address-only display.
    /// This does not authorize eager runtime reads of later semantic roles.
    pub fn monitor_input_devices(&self, m: &Memory) -> Result<BTreeSet<Device>, String> {
        let ds = self.modifier_devices().into_iter().collect();
        validate_model(m, &ds)?;
        Ok(ds)
    }
    /// Potential current display candidates, not actual observed visits and
    /// not the native safety API. Runtime errors may skip most of these roles.
    pub fn capture_devices(&self, m: &Memory) -> Result<BTreeSet<Device>, String> {
        let mut ds = self.monitor_input_devices(m)?;
        for v in self.triples() {
            ds.extend(v.resolve(m)?);
        }
        if let Kind::Compare { setpoint, .. } = &self.kind {
            for v in setpoint {
                resolved_scalar(v, m)?.devices(&mut ds);
            }
        }
        if let Some(target) = self.target() {
            ds.extend(comparison_destinations(target.resolve(m)?)?);
        }
        validate_model(m, &ds)?;
        Ok(ds)
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Ok(ds) = self.capture_devices(m) {
            out.extend(ds);
        }
    }
    pub fn display_value(&self, m: &Memory) -> String {
        if let Some(target) = self.target() {
            return match target.resolve(m).and_then(comparison_destinations) {
                Ok(ds) => ds
                    .into_iter()
                    .map(|d| if m.bit(d) { '1' } else { '0' })
                    .collect(),
                Err(e) => format!("İndeks: {e}"),
            };
        }
        let Kind::Arithmetic { target, .. } = &self.kind else {
            unreachable!()
        };
        match target.resolve(m) {
            Ok(ds) => format!(
                "{:02}:{:02}:{:02}",
                m.word(ds[0]),
                m.word(ds[1]),
                m.word(ds[2])
            ),
            Err(e) => format!("İndeks: {e}"),
        }
    }
    pub fn label(&self) -> String {
        let p = if self.pulse_label { "P" } else { "" };
        match &self.kind {
            Kind::Compare {
                setpoint,
                source,
                target,
            } => format!(
                "TCMP{p} {} {} {} {source} {target}",
                setpoint[0], setpoint[1], setpoint[2]
            ),
            Kind::Zone {
                low,
                high,
                source,
                target,
            } => format!("TZCP{p} {low} {high} {source} {target}"),
            Kind::Arithmetic {
                subtract,
                left,
                right,
                target,
            } => format!(
                "{}{p} {left} {right} {target}",
                if *subtract { "TSUB" } else { "TADD" }
            ),
        }
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let header = match self.kind {
            Kind::Compare { .. } => 0x6001,
            Kind::Zone { .. } => 0x6201,
            Kind::Arithmetic {
                subtract: false, ..
            } => 0x6401,
            _ => 0x6601,
        };
        let mut words = vec![header | if self.pulse_label { 0x10 } else { 0 }];
        match &self.kind {
            Kind::Compare {
                setpoint,
                source,
                target,
            } => {
                for v in setpoint {
                    words.extend(v.native_words()?);
                }
                words.extend(source.native_words()?);
                words.extend(target.native_words(false)?);
            }
            Kind::Zone {
                low,
                high,
                source,
                target,
            } => {
                for v in [low, high, source] {
                    words.extend(v.native_words()?);
                }
                words.extend(target.native_words(false)?);
            }
            Kind::Arithmetic {
                left,
                right,
                target,
                ..
            } => {
                for v in [left, right, target] {
                    words.extend(v.native_words()?);
                }
            }
        }
        Ok(words)
    }
}
