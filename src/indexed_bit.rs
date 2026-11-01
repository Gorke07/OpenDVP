//! The nine documented E/F-qualified basic bit instructions. Their handler
//! default is ONE bit; neither a Kn group nor an indexed edge instruction.
use crate::indexed_address::IndexedAddress;
use crate::indexed_operand::has_modifiers;
use crate::plc::{Action, Area, Device, Memory};
use crate::system_relays::ClockState;
use std::collections::BTreeSet;

fn parse_address(args: &[String], write: bool) -> Result<IndexedAddress, String> {
    if args.len() != 1 {
        return Err("İndeksli bit komutu bir operand gerektirir".into());
    }
    let address = IndexedAddress::parse(&args[0])?;
    if !matches!(address.base.area, Area::X | Area::Y | Area::M | Area::S)
        || (write && address.base.area == Area::X)
    {
        return Err(if write {
            "İndeksli bit hedefi Y/M/S olmalı"
        } else {
            "İndeksli kontak kaynağı X/Y/M/S olmalı"
        }
        .into());
    }
    Ok(address)
}

/// Only the effective bit is validated. A valid final X/Y/S/M bit must not be
/// rejected because a fictitious four-bit Kn span would extend past the area.
fn resolve_bit(address: IndexedAddress, memory: &Memory, write: bool) -> Result<Device, String> {
    let effective = address
        .base
        .index
        .wrapping_add(address.index.read(memory) as u16) as i16;
    let index =
        u16::try_from(effective).map_err(|_| "İndeksli bit adresi negatif aralığa taşıyor")?;
    let target = Device {
        index,
        ..address.base
    };
    if target.area == Area::M {
        if index > memory.relay_limit.unwrap_or(4095) {
            return Err("İndeksli M biti seçilen model aralığını aşıyor".into());
        }
        // The generic native packed helper remaps a low-bank start strictly
        // above raw0x0D00. M1280 is valid as a single bit; M1281..1535 is not
        // qualified as canonical storage even for this width-one descriptor.
        if address.base.index < 1536 && (1281..=1535).contains(&index) {
            return Err("İndeksli M1281..M1535 başlangıcının native eşlemesi kanonik adreslemeyle uyuşmuyor".into());
        }
        if (address.base.index < 1536) != (index < 1536) {
            return Err("İndeksli M bitinin fiziksel alan geçişi doğrulanmadı".into());
        }
    }
    target.to_string().parse::<Device>()?;
    if !target.bit() || (write && !target.writable_bit()) {
        return Err("İndeksli bit hedefi salt okunur veya geçersiz".into());
    }
    Ok(target)
}

fn descriptor(address: IndexedAddress) -> Result<Vec<u16>, String> {
    let mut words = address.native_words(false, Some(1))?;
    // Retain the proven generic address/bank encoding, replacing ONLY the
    // explicit Kn1 nibble width with the bare descriptor's handler default.
    words[1] &= !0x000f;
    Ok(words)
}

fn devices(address: IndexedAddress, out: &mut BTreeSet<Device>) {
    out.insert(address.index.device());
    Memory::error_devices(out);
}

fn clock_bit(device: Device) -> bool {
    device.area == Area::M && (1011..=1014).contains(&device.index)
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedBitContact {
    address: IndexedAddress,
    inverted: bool,
}
impl IndexedBitContact {
    pub fn supports(name: &str) -> bool {
        matches!(
            name.to_ascii_uppercase().as_str(),
            "LD" | "LDI" | "AND" | "ANI" | "OR" | "ORI"
        )
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        has_modifiers(args)
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let upper = name.to_ascii_uppercase();
        if !Self::supports(&upper) {
            return Err("Bu kontak E/F indeksli temel bit kapsamında değil".into());
        }
        Ok(Self {
            address: parse_address(args, false)?,
            inverted: matches!(upper.as_str(), "LDI" | "ANI" | "ORI"),
        })
    }
    pub fn resolve(&self, memory: &Memory) -> Result<Device, String> {
        resolve_bit(self.address, memory, false)
    }
    /// A visual read never consumes edge history or inserts a limitation into
    /// Memory. Unseeded installed clock levels remain an explicit error.
    pub fn evaluate(&self, memory: &Memory) -> Result<bool, String> {
        let target = self.resolve(memory)?;
        if clock_bit(target) && memory.system_relays.clock_levels().is_none() {
            return Err(
                "İndeksli M1011..M1014 okuması için kurulu saat profilinin ilk seviyeleri gerekli"
                    .into(),
            );
        }
        Ok(memory.bit(target) ^ self.inverted)
    }
    /// Source VM calls this only at an actual visit. Basic AND/OR contacts
    /// still visit eagerly when the preceding accumulator decides the result.
    pub fn sample(&self, memory: &mut Memory) -> Result<bool, String> {
        let target = self.resolve(memory)?;
        if clock_bit(target) {
            ClockState::require_clock_observation(memory)?;
        }
        Ok(memory.bit(target) ^ self.inverted)
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        devices(self.address, out);
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        vec![self.address.index.device()]
    }
    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Ok(target) = self.resolve(memory) {
            out.insert(target);
        }
    }
    pub fn label(&self, prefix: &str) -> String {
        let name = match (prefix, self.inverted) {
            ("LD", true) => "LDI",
            ("AND", true) => "ANI",
            ("OR", true) => "ORI",
            _ => prefix,
        };
        format!("{name} {}", self.address)
    }
    pub fn native_words(&self, prefix: &str) -> Result<Vec<u16>, String> {
        let header = match (prefix, self.inverted) {
            ("LD", false) => 0x1202,
            ("LD", true) => 0x1302,
            ("AND", false) => 0x1402,
            ("AND", true) => 0x1502,
            ("OR", false) => 0x1602,
            ("OR", true) => 0x1702,
            _ => return Err("İndeksli kontak öneki LD/AND/OR olmalı".into()),
        };
        let mut words = vec![header];
        words.extend(descriptor(self.address)?);
        Ok(words)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BitActionKind {
    Out,
    Set,
    Reset,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedBitAction {
    op: BitActionKind,
    address: IndexedAddress,
}
impl IndexedBitAction {
    pub fn supports(name: &str) -> bool {
        matches!(name.to_ascii_uppercase().as_str(), "OUT" | "SET" | "RST")
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        has_modifiers(args)
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let op = match name.to_ascii_uppercase().as_str() {
            "OUT" => BitActionKind::Out,
            "SET" => BitActionKind::Set,
            "RST" => BitActionKind::Reset,
            _ => return Err("İndeksli bit çıkışı OUT/SET/RST olmalı".into()),
        };
        Ok(Self {
            op,
            address: parse_address(args, true)?,
        })
    }
    pub fn is_out(&self) -> bool {
        self.op == BitActionKind::Out
    }
    /// Called after the source VM's output-context gate. False OUT still
    /// clears its CURRENT selected bit; false SET/RST do not read the index.
    pub fn should_resolve(&self, power: bool) -> bool {
        self.is_out() || power
    }
    pub fn resolve(&self, memory: &Memory) -> Result<Action, String> {
        let target = resolve_bit(self.address, memory, true)?;
        Ok(match self.op {
            BitActionKind::Out => Action::Out(target),
            BitActionKind::Set => Action::Set(target),
            BitActionKind::Reset => Action::Reset(target),
        })
    }
    /// Static source label only. Execution and monitoring use resolve().
    pub fn dest(&self) -> Device {
        self.address.base
    }
    pub fn label(&self) -> String {
        format!(
            "{} {}",
            match self.op {
                BitActionKind::Out => "OUT",
                BitActionKind::Set => "SET",
                BitActionKind::Reset => "RST",
            },
            self.address
        )
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        devices(self.address, out);
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        vec![self.address.index.device()]
    }
    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        if let Ok(target) = resolve_bit(self.address, memory, true) {
            out.insert(target);
        }
    }
    pub fn display_value(&self, memory: &Memory) -> String {
        match resolve_bit(self.address, memory, true) {
            Ok(target) => u8::from(memory.bit(target)).to_string(),
            Err(error) => format!("İndeks: {error}"),
        }
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let mut words = vec![match self.op {
            BitActionKind::Out => 0x1802,
            BitActionKind::Set => 0x1902,
            BitActionKind::Reset => 0x1a02,
        }];
        words.extend(descriptor(self.address)?);
        Ok(words)
    }
}
