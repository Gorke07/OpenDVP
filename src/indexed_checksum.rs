//! Ordered D-address bindings for LRC/LRCP and CRC/CRCP.
//! All paths have seven original WORDs; outer VM owns power and pulse history.
use crate::additional_processing::AdditionalValue;
use crate::calendar_operand::CalendarValue;
use crate::indexed_address::IndexedAddress;
use crate::indexed_multiply::validate_model;
use crate::indexed_operand::{has_modifier, has_modifiers, read_resolved_value};
use crate::plc::{Area, Device, Memory, Operand, word_range};
use crate::text_encoding::{ChecksumAccumulator, TextAction};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChecksumOutcome {
    Nominal,
    Complete,
    PlcError { offset: u16 },
    Limitation(String),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecksumVisit {
    pub outcome: ChecksumOutcome,
    pub observed: BTreeSet<Device>,
}

fn data(index: u16) -> Device {
    Device {
        area: Area::D,
        index,
    }
}
fn scalar(device: Device) -> CalendarValue {
    CalendarValue::Additional(AdditionalValue::Word(Operand::Register(device)))
}

/// One lexical D base, optionally modified. Endpoints are intentionally valid
/// bases: mode/count-dependent source errors and partial output happen later.
#[derive(Debug, Clone, PartialEq)]
struct DataAddress {
    base: Device,
    address: Option<IndexedAddress>,
}
impl DataAddress {
    fn direct(base: Device) -> Result<Self, String> {
        word_range(base, 1)?;
        if base.area != Area::D {
            return Err("LRC/CRC adresi D yazmacı olmalı".into());
        }
        Ok(Self {
            base,
            address: None,
        })
    }
    fn parse(raw: &str) -> Result<Self, String> {
        let text = raw.trim().to_ascii_uppercase();
        if has_modifier(&text) {
            let address = IndexedAddress::parse(&text)?;
            let mut value = Self::direct(address.base)?;
            value.address = Some(address);
            Ok(value)
        } else {
            Self::direct(text.parse()?)
        }
    }
    fn modifier(&self) -> Option<Device> {
        self.address.map(|a| a.index.device())
    }
    fn resolve(&self, m: &Memory) -> Result<Device, String> {
        let index = match self.address {
            Some(a) => u16::try_from(a.base.index.wrapping_add(a.index.read(m) as u16) as i16)
                .map_err(|_| "LRC/CRC etkin D adresi negatif")?,
            None => self.base.index,
        };
        if index > m.data_register_limit.unwrap_or(9999) {
            return Err("LRC/CRC etkin D adresi model sınırını aşıyor".into());
        }
        Ok(data(index))
    }
    fn devices(&self, out: &mut BTreeSet<Device>) {
        out.insert(self.modifier().unwrap_or(self.base));
    }
    fn native_words(&self) -> Result<Vec<u16>, String> {
        match self.address {
            Some(a) => a.native_words(false, None),
            None => crate::es2_encoder::calendar_operand(&scalar(self.base)),
        }
    }
}
impl std::fmt::Display for DataAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.address {
            Some(a) => a.fmt(f),
            None => self.base.fmt(f),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Count {
    Constant(i16),
    Data(DataAddress),
}
impl Count {
    fn parse(raw: &str) -> Result<Self, String> {
        let text = raw.trim().to_ascii_uppercase();
        if text.starts_with('D') {
            return Ok(Self::Data(DataAddress::parse(&text)?));
        }
        if !text.starts_with(['K', 'H']) || has_modifier(&text) {
            return Err("LRC/CRC uzunluğu K/H sabiti veya D adresi olmalı".into());
        }
        let Operand::Constant(n) = Operand::parse(&text)? else {
            return Err("LRC/CRC uzunluğu K/H sabiti olmalı".into());
        };
        if !(1..=256).contains(&n) {
            return Err("LRC/CRC sabit uzunluğu 1..256 olmalı".into());
        }
        Ok(Self::Constant(n))
    }
    fn from_operand(value: &Operand) -> Result<Self, String> {
        match value {
            // Typed legacy values retain execution-time domain checking.
            Operand::Constant(n) => Ok(Self::Constant(*n)),
            Operand::Register(d) => Ok(Self::Data(DataAddress::direct(*d)?)),
        }
    }
    fn modifier(&self) -> Option<Device> {
        match self {
            Self::Data(a) => a.modifier(),
            Self::Constant(_) => None,
        }
    }
    fn devices(&self, out: &mut BTreeSet<Device>) {
        if let Self::Data(a) = self {
            a.devices(out);
        }
    }
    fn native_words(&self) -> Result<Vec<u16>, String> {
        match self {
            Self::Data(a) => a.native_words(),
            Self::Constant(n) => crate::es2_encoder::calendar_operand(&CalendarValue::Additional(
                AdditionalValue::Word(Operand::Constant(*n)),
            )),
        }
    }
}
impl std::fmt::Display for Count {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Constant(n) => write!(f, "K{n}"),
            Self::Data(a) => a.fmt(f),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedChecksumAction {
    lrc: bool,
    pulse_label: bool,
    source: DataAddress,
    count: Count,
    target: DataAddress,
}

struct Context<'a, F> {
    m: &'a mut Memory,
    observed: &'a mut BTreeSet<Device>,
    fetch: F,
    captured: BTreeSet<Device>,
    written: BTreeSet<Device>,
}
impl<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>> Context<'_, F> {
    fn capture(&mut self, ds: BTreeSet<Device>) -> Result<(), String> {
        let missing: BTreeSet<_> = ds
            .difference(&self.captured)
            .copied()
            .filter(|d| !self.written.contains(d))
            .collect();
        if !missing.is_empty() {
            (self.fetch)(self.m, &missing)?;
        }
        self.captured.extend(&ds);
        self.observed.extend(ds);
        Ok(())
    }
    fn address(&mut self, binding: &DataAddress) -> Result<Option<Device>, String> {
        self.capture(binding.modifier().into_iter().collect())?;
        match binding.resolve(self.m) {
            Ok(d) => Ok(Some(d)),
            // Closed current-image guards check signed canonical D bounds
            // before any payload access or store; report the caller's stage.
            Err(_) => Ok(None),
        }
    }
    fn word(&mut self, device: Device) -> Result<i16, String> {
        self.capture(BTreeSet::from([device]))?;
        Ok(read_resolved_value(&scalar(device), self.m, self.observed)? as i16)
    }
    fn mode(&mut self) -> Result<bool, String> {
        self.capture(BTreeSet::from([Memory::system_device(1161)]))?;
        Ok(self.m.system_bit(1161))
    }
    fn put(&mut self, device: Device, value: u16) {
        self.m.set_word(device, value as i16);
        self.written.insert(device);
        self.observed.insert(device);
    }
    fn error(&mut self, offset: u16) -> Result<ChecksumOutcome, String> {
        let mut ds = BTreeSet::new();
        Memory::error_devices(&mut ds);
        self.capture(ds.clone())?;
        self.m.operation_error(0xe1a, offset);
        self.written.extend(ds);
        Ok(ChecksumOutcome::PlcError { offset })
    }
}

impl IndexedChecksumAction {
    fn spec(name: &str) -> Option<(bool, bool)> {
        match name.to_ascii_uppercase().as_str() {
            "LRC" => Some((true, false)),
            "LRCP" => Some((true, true)),
            "CRC" => Some((false, false)),
            "CRCP" => Some((false, true)),
            _ => None,
        }
    }
    pub fn supports(name: &str) -> bool {
        Self::spec(name).is_some()
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        has_modifiers(args)
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let (lrc, pulse_label) = Self::spec(name).ok_or("LRC/LRCP/CRC/CRCP gerekli")?;
        if args.len() != 3 {
            return Err(format!("{name}: kaynak, uzunluk ve hedef gerekli"));
        }
        Ok(Self {
            lrc,
            pulse_label,
            source: DataAddress::parse(&args[0])?,
            count: Count::parse(&args[1])?,
            target: DataAddress::parse(&args[2])?,
        })
    }
    /// Ordinary constructors use identical ordered behavior and shared math;
    /// this typed adapter never serializes or reparses source text.
    pub fn from_legacy(action: &TextAction) -> Result<Self, String> {
        let (lrc, source, count, target) = match action {
            TextAction::Lrc {
                source,
                count,
                dest,
            } => (true, source, count, dest),
            TextAction::Crc {
                source,
                count,
                dest,
            } => (false, source, count, dest),
            _ => return Err("LRC/CRC metin eylemi gerekli".into()),
        };
        Ok(Self {
            lrc,
            pulse_label: false,
            source: DataAddress::direct(*source)?,
            count: Count::from_operand(count)?,
            target: DataAddress::direct(*target)?,
        })
    }
    fn valid_count(&self, n: i16) -> bool {
        (1..=256).contains(&n) && (!self.lrc || n % 2 == 0)
    }
    /// floor(n/2), not ceil, is the proven packed CRC precheck. The odd final
    /// byte beyond the model edge is omitted by the bounded loop, never padded.
    fn source_count(&self, base: Device, n: u16, low_only: bool, m: &Memory) -> Option<u16> {
        let available = m
            .data_register_limit
            .unwrap_or(9999)
            .checked_sub(base.index)?
            + 1;
        if (if low_only { n } else { n / 2 }) > available {
            return None;
        }
        Some(if low_only || self.lrc {
            n
        } else {
            n.min(available.saturating_mul(2))
        })
    }
    fn run<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>>(
        &self,
        c: &mut Context<'_, F>,
    ) -> Result<ChecksumOutcome, String> {
        let n = match &self.count {
            Count::Constant(n) => *n,
            Count::Data(a) => {
                let Some(d) = c.address(a)? else {
                    return c.error(3);
                };
                c.word(d)?
            }
        };
        if !self.valid_count(n) {
            return c.error(3);
        }
        let Some(source) = c.address(&self.source)? else {
            return c.error(1);
        };
        // Native resamples M1161 in the loop/output. No legal D-only operation
        // changes it; first-sample equivalence excludes asynchronous host writes.
        let low_only = c.mode()?;
        let Some(read_count) = self.source_count(source, n as u16, low_only, c.m) else {
            return c.error(1);
        };
        let mut checksum = if self.lrc {
            ChecksumAccumulator::lrc()
        } else {
            ChecksumAccumulator::crc()
        };
        if self.lrc && !low_only {
            for offset in 0..read_count / 2 {
                let raw = c.word(data(source.index + offset))? as u16;
                // This private shared WORD affects SORT. Publish even if this
                // pair is invalid; count/extent/low-byte/CRC paths do not write it.
                c.m.native_numeric_scratch = Some(raw as i16);
                if !checksum.push_packed_lrc(raw) {
                    return c.error(1);
                }
            }
        } else {
            let mut cached = 0u16;
            for offset in 0..read_count {
                if low_only || offset % 2 == 0 {
                    cached = c.word(data(
                        source.index + if low_only { offset } else { offset / 2 },
                    ))? as u16;
                }
                let byte = (cached >> if !low_only && offset % 2 == 1 { 8 } else { 0 }) as u8;
                if !checksum.push_byte(byte) {
                    return c.error(1);
                }
            }
        }
        let result = checksum.result(low_only);
        let Some(target) = c.address(&self.target)? else {
            return c.error(5);
        };
        c.put(target, result[0]);
        if low_only {
            if target.index == c.m.data_register_limit.unwrap_or(9999) {
                return c.error(5);
            }
            // The first result can overwrite its own E/F index. Native retains
            // the first effective pointer for this second store.
            c.put(data(target.index + 1), result[1]);
        }
        Ok(ChecksumOutcome::Complete)
    }
    pub fn execute(
        &self,
        m: &mut Memory,
        power: bool,
        observed: &mut BTreeSet<Device>,
    ) -> ChecksumOutcome {
        if !power {
            return ChecksumOutcome::Nominal;
        }
        let outcome = self.run(&mut Context {
            m,
            observed,
            fetch: |_: &mut Memory, _: &BTreeSet<Device>| Ok(()),
            captured: BTreeSet::new(),
            written: BTreeSet::new(),
        });
        match outcome {
            Ok(outcome) => outcome,
            Err(reason) => {
                m.execution_limitations.insert(reason.clone());
                ChecksumOutcome::Limitation(reason)
            }
        }
    }
    /// Disposable actual-input replay, always in count/source/payload/target
    /// order. Outputs and scratch are never copied back to independent local
    /// execution. Parent owns gate/P admission and actual instruction stepping.
    pub fn qualify_native_with<F: FnMut(&mut Memory, &BTreeSet<Device>) -> Result<(), String>>(
        &self,
        m: &Memory,
        fetch: F,
    ) -> ChecksumVisit {
        let mut clone = m.clone();
        let mut observed = BTreeSet::new();
        let outcome = self
            .run(&mut Context {
                m: &mut clone,
                observed: &mut observed,
                fetch,
                captured: BTreeSet::new(),
                written: BTreeSet::new(),
            })
            .unwrap_or_else(ChecksumOutcome::Limitation);
        ChecksumVisit { outcome, observed }
    }
    pub fn qualify_native(&self, m: &Memory) -> ChecksumVisit {
        self.qualify_native_with(m, |_, _| Ok(()))
    }
    pub fn nominal_words(&self) -> u16 {
        7
    }
    pub fn dest(&self) -> Device {
        self.target.base
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        [
            self.count.modifier(),
            self.source.modifier(),
            self.target.modifier(),
        ]
        .into_iter()
        .flatten()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
    }
    pub fn control_devices(&self) -> Vec<Device> {
        vec![Memory::system_device(1161)]
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        self.source.devices(out);
        self.count.devices(out);
        self.target.devices(out);
        out.extend(self.control_devices());
        out.extend((1020..=1022).map(Memory::system_device));
        Memory::error_devices(out);
        // Only initial bases are unconditional. Dynamic mode/count may reject
        // before payload or allow one safe output plus a defined second fault.
    }
    /// UI first hydrates modifier rows. It must then require these actual count
    /// and mode rows before calling capture_devices; absent values are not zero.
    pub fn monitor_input_devices(&self, m: &Memory) -> Result<BTreeSet<Device>, String> {
        let mut ds: BTreeSet<_> = self.modifier_devices().into_iter().collect();
        ds.extend(self.control_devices());
        if let Count::Data(a) = &self.count {
            ds.insert(a.resolve(m)?);
        }
        validate_model(m, &ds)?;
        Ok(ds)
    }
    /// Bounded potential monitor candidates, deliberately separate from actual
    /// ordered observations. No checksum calculation or private scratch write.
    pub fn capture_devices(&self, m: &Memory) -> Result<BTreeSet<Device>, String> {
        let mut ds = self.monitor_input_devices(m)?;
        let n = match &self.count {
            Count::Constant(n) => *n,
            Count::Data(a) => m.word(a.resolve(m)?),
        };
        if !self.valid_count(n) {
            return Ok(ds);
        }
        let source = self.source.resolve(m)?;
        ds.insert(source);
        let low_only = m.system_bit(1161);
        let Some(read_count) = self.source_count(source, n as u16, low_only, m) else {
            return Ok(ds);
        };
        let words = if low_only {
            read_count
        } else {
            read_count.div_ceil(2)
        };
        ds.extend((0..words).map(|i| data(source.index + i)));
        let target = self.target.resolve(m)?;
        ds.insert(target);
        if low_only && target.index < m.data_register_limit.unwrap_or(9999) {
            ds.insert(data(target.index + 1));
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
        match self.target.resolve(m) {
            Ok(d) => format!("H{:04X}", m.word(d) as u16),
            Err(reason) => format!("İndeks: {reason}"),
        }
    }
    pub fn label(&self) -> String {
        format!(
            "{}{} {} {} {}",
            if self.lrc { "LRC" } else { "CRC" },
            if self.pulse_label { "P" } else { "" },
            self.source,
            self.count,
            self.target
        )
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let mut words = vec![
            (if self.lrc { 0xd000 } else { 0xd200 }) | if self.pulse_label { 0x10 } else { 0 },
        ];
        words.extend(self.source.native_words()?);
        words.extend(self.count.native_words()?);
        words.extend(self.target.native_words()?);
        Ok(words)
    }
}
