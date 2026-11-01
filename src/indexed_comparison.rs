//! Typed E/F-addressed comparisons. Resolve only when the instruction actually
//! reads its operands; absolute ANDZ/ORZ have distinct short-circuit semantics.
use crate::calendar_operand::CalendarValue;
use crate::indexed_address::IndexedAddress;
use crate::indexed_operand::{AddressedValue, has_modifier, has_modifiers};
use crate::numeric_contacts::{ContactKind, NumericPredicate, parse_spec};
use crate::plc::{
    Action, Area, Device, Expr, Memory, Relation, WordLogic, comparison_destinations,
};
use crate::wide::{WideAction, WideDestination, WideOp, WideSource};
use std::collections::BTreeSet;

/// A result descriptor has the handler's default width of three bits. It is
/// not a Kn scalar, whose smallest declared width would incorrectly be four.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ComparisonTarget {
    Direct(Device),
    Modified(IndexedAddress),
}
impl ComparisonTarget {
    pub(crate) fn parse(raw: &str) -> Result<Self, String> {
        if has_modifier(raw) {
            let address = IndexedAddress::parse(raw)?;
            if !matches!(address.base.area, Area::Y | Area::M | Area::S) {
                return Err("Karşılaştırma hedefi Y/M/S veya indeksli Y/M/S olmalı".into());
            }
            Ok(Self::Modified(address))
        } else {
            let base = raw.parse()?;
            comparison_destinations(base)?;
            Ok(Self::Direct(base))
        }
    }
    pub(crate) fn base(&self) -> Device {
        match self {
            Self::Direct(base) => *base,
            Self::Modified(address) => address.base,
        }
    }
    pub(crate) fn resolve(&self, memory: &Memory) -> Result<Device, String> {
        let target = match self {
            Self::Direct(base) => *base,
            Self::Modified(address) => {
                let effective = address
                    .base
                    .index
                    .wrapping_add(address.index.read(memory) as u16)
                    as i16;
                let index = u16::try_from(effective)
                    .map_err(|_| "İndeksli sonuç adresi negatif aralığa taşıyor")?;
                let target = Device {
                    index,
                    ..address.base
                };
                // Same generic packed writer as Kn; retain its established
                // low-bank remap and physical-bank qualification boundaries.
                if target.area == Area::M {
                    if address.base.index < 1536 && (1281..=1535).contains(&index) {
                        return Err("İndeksli M1281..M1535 sonuç başlangıcının native eşlemesi kanonik adreslemeyle uyuşmuyor".into());
                    }
                    if (address.base.index < 1536) != (index < 1536)
                        || (index < 1536 && index + 3 > 1536)
                    {
                        return Err(
                            "İndeksli M sonuçlarının fiziksel alan geçişi doğrulanmadı".into()
                        );
                    }
                }
                target
            }
        };
        let span = comparison_destinations(target)?;
        if span
            .iter()
            .any(|d| d.area == Area::M && d.index > memory.relay_limit.unwrap_or(4095))
        {
            return Err("Karşılaştırma sonuçları seçilen model aralığını aşıyor".into());
        }
        Ok(target)
    }
    pub(crate) fn devices(&self, out: &mut BTreeSet<Device>) {
        match self {
            Self::Direct(base) => {
                out.extend(comparison_destinations(*base).expect("validated result span"));
            }
            Self::Modified(address) => {
                out.insert(address.index.device());
            }
        }
    }
    pub(crate) fn modifier_devices(&self) -> Vec<Device> {
        match self {
            Self::Direct(_) => Vec::new(),
            Self::Modified(address) => vec![address.index.device()],
        }
    }
    pub(crate) fn native_words(&self, wide: bool) -> Result<Vec<u16>, String> {
        let mut words = match self {
            Self::Modified(address) => {
                let mut words = address.native_words(false, Some(1))?;
                // Compiler bare bit descriptors retain width zero; the CMP/
                // ZCP handler supplies three. Preserve bank and area fields.
                words[1] &= !0x000f;
                words
            }
            Self::Direct(base) => {
                let offset = match base.area {
                    Area::Y => 0x500 + base.index,
                    Area::M if base.index >= 1536 => 0x1000 + base.index - 1536,
                    Area::M => 0x800 + base.index,
                    Area::S => base.index,
                    _ => return Err("Karşılaştırma hedef alanı geçersiz".into()),
                };
                vec![((offset & 0xff) << 8) | 0x84, (offset & 0xff00) | 0x80]
            }
        };
        if wide {
            words.extend([0x0080, 0x0080]);
        }
        Ok(words)
    }
}
impl std::fmt::Display for ComparisonTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Direct(base) => base.fmt(f),
            Self::Modified(address) => address.fmt(f),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedComparisonAction {
    zone: bool,
    wide: bool,
    pulse_label: bool,
    sources: Vec<AddressedValue>,
    target: ComparisonTarget,
}
impl IndexedComparisonAction {
    fn spec(name: &str) -> Option<(bool, bool, bool)> {
        Some(match name.to_ascii_uppercase().as_str() {
            "CMP" => (false, false, false),
            "CMPP" => (false, false, true),
            "DCMP" => (false, true, false),
            "DCMPP" => (false, true, true),
            "ZCP" => (true, false, false),
            "ZCPP" => (true, false, true),
            "DZCP" => (true, true, false),
            "DZCPP" => (true, true, true),
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
        let (zone, wide, pulse_label) =
            Self::spec(name).ok_or("İndeksli karşılaştırma komutu geçersiz")?;
        let count = if zone { 3 } else { 2 };
        if args.len() != count + 1 {
            return Err(format!("{name}: {} operand gerekli", count + 1));
        }
        Ok(Self {
            zone,
            wide,
            pulse_label,
            sources: args[..count]
                .iter()
                .map(|s| AddressedValue::parse(s, wide, false))
                .collect::<Result<_, _>>()?,
            target: ComparisonTarget::parse(&args[count])?,
        })
    }
    /// Reuse the ordinary snapshot-before-result-write algorithm. Result bits
    /// cannot change the D words that store address modifiers.
    pub fn resolve(&self, memory: &Memory) -> Result<Action, String> {
        let sources = self
            .sources
            .iter()
            .map(|s| s.resolve(memory, false))
            .collect::<Result<Vec<_>, _>>()?;
        let dest = self.target.resolve(memory)?;
        self.action_from_sources(sources, dest)
    }
    /// Snapshot sources only on a qualified numerical visit. Keep resolve()
    /// address-only for the native bridge's independent storage capture.
    pub fn prepare(
        &self,
        memory: &Memory,
        observed: &mut BTreeSet<Device>,
    ) -> Result<Action, String> {
        let mut sources = Vec::with_capacity(self.sources.len());
        for binding in &self.sources {
            observed.extend(binding.modifier_devices());
            let source = binding.resolve(memory, false)?;
            let value = crate::indexed_operand::read_resolved_value(&source, memory, observed)?;
            sources.push(crate::indexed_operand::frozen_value(value, self.wide));
        }
        observed.extend(self.target.modifier_devices());
        let dest = self.target.resolve(memory)?;
        observed.extend(comparison_destinations(dest)?);
        self.action_from_sources(sources, dest)
    }
    fn action_from_sources(
        &self,
        sources: Vec<CalendarValue>,
        dest: Device,
    ) -> Result<Action, String> {
        if self.wide {
            return Ok(Action::Wide(WideAction {
                op: if self.zone {
                    WideOp::Zone
                } else {
                    WideOp::Compare
                },
                sources: sources.into_iter().map(WideSource::Read).collect(),
                dest: WideDestination::Register(dest),
                count: 0,
            }));
        }
        let mut sources = sources.into_iter();
        let a = sources.next().expect("validated source count");
        let b = sources.next().expect("validated source count");
        Ok(if self.zone {
            Action::ZoneCompare(a, b, sources.next().expect("validated source count"), dest)
        } else {
            Action::Compare(a, b, dest)
        })
    }
    pub fn dest(&self) -> Device {
        self.target.base()
    }
    pub fn label(&self) -> String {
        let mut parts = vec![format!(
            "{}{}{}",
            if self.wide { "D" } else { "" },
            if self.zone { "ZCP" } else { "CMP" },
            if self.pulse_label { "P" } else { "" }
        )];
        parts.extend(self.sources.iter().map(ToString::to_string));
        parts.push(self.target.to_string());
        parts.join(" ")
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        for source in &self.sources {
            source.devices(out);
        }
        self.target.devices(out);
        Memory::error_devices(out);
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        self.sources
            .iter()
            .flat_map(AddressedValue::modifier_devices)
            .chain(self.target.modifier_devices())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        for source in &self.sources {
            source.observed_devices(memory, false, out);
        }
        if let Ok(target) = self.target.resolve(memory) {
            out.extend(comparison_destinations(target).expect("resolved three-bit span"));
        }
    }
    pub fn display_value(&self, memory: &Memory) -> String {
        match self.target.resolve(memory) {
            Ok(target) => comparison_destinations(target)
                .expect("resolved span")
                .iter()
                .enumerate()
                .fold(0u8, |value, (bit, device)| {
                    value | (u8::from(memory.bit(*device)) << bit)
                })
                .to_string(),
            Err(error) => format!("İndeks: {error}"),
        }
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let header = (if self.zone { 0x2600 } else { 0x2400 })
            + if self.wide { 0x100 } else { 0 }
            + if self.pulse_label { 0x10 } else { 0 };
        let mut words = vec![header];
        for source in &self.sources {
            words.extend(source.native_words()?);
        }
        words.extend(self.target.native_words(self.wide)?);
        Ok(words)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum PredicateKind {
    Signed(Relation),
    Logic(WordLogic),
    Difference(Relation),
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedPredicate {
    kind: PredicateKind,
    wide: bool,
    sources: Vec<AddressedValue>,
}
impl IndexedPredicate {
    fn spec(name: &str) -> Option<(PredicateKind, bool, usize)> {
        let upper = name.to_ascii_uppercase();
        if let Some(spec) = parse_spec(&upper) {
            return match spec.kind {
                ContactKind::Logic(op) => Some((PredicateKind::Logic(op), spec.wide, 2)),
                ContactKind::Difference(op) => Some((PredicateKind::Difference(op), spec.wide, 3)),
                ContactKind::Indexed(_) => None,
            };
        }
        let (wide, plain) = upper
            .strip_prefix('D')
            .map_or((false, upper.as_str()), |s| (true, s));
        for prefix in ["LD", "AND", "OR"] {
            if let Some(suffix) = plain.strip_prefix(prefix) {
                return Some((PredicateKind::Signed(Relation::parse(suffix)?), wide, 2));
            }
        }
        None
    }
    pub fn supports(name: &str) -> bool {
        Self::spec(name).is_some()
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        has_modifiers(args)
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let (kind, wide, count) =
            Self::spec(name).ok_or("İndeksli karşılaştırma kontağı geçersiz")?;
        if args.len() != count {
            return Err(format!("{name}: {count} operand gerekli"));
        }
        let sources = args
            .iter()
            .map(|raw| {
                let value = AddressedValue::parse(raw, wide, false)?;
                // PDF 741/743/745: every S1/S2/S3 Kn/T/C/D cell is grey,
                // but all direct E/F cells are blank. Keep this role restriction.
                if matches!(kind, PredicateKind::Difference(_))
                    && matches!(value, AddressedValue::Direct(CalendarValue::Index { .. }))
                {
                    return Err("Fark kontağı doğrudan E/F kaynağı kabul etmez".into());
                }
                Ok(value)
            })
            .collect::<Result<_, String>>()?;
        Ok(Self {
            kind,
            wide,
            sources,
        })
    }
    pub fn resolve(&self, memory: &Memory) -> Result<Expr, String> {
        let sources = self
            .sources
            .iter()
            .map(|s| s.resolve(memory, false))
            .collect::<Result<Vec<_>, _>>()?;
        self.expression_from_sources(sources)
    }
    fn expression_from_sources(&self, sources: Vec<CalendarValue>) -> Result<Expr, String> {
        let mut sources = sources.into_iter();
        Ok(match &self.kind {
            PredicateKind::Signed(op) => {
                let a = sources.next().expect("validated source count");
                let b = sources.next().expect("validated source count");
                if self.wide {
                    Expr::CompareWide(op.clone(), a, b)
                } else {
                    Expr::Compare(op.clone(), a, b)
                }
            }
            PredicateKind::Logic(op) => {
                let pair = [
                    sources.next().expect("validated source count"),
                    sources.next().expect("validated source count"),
                ];
                Expr::Numeric(if self.wide {
                    NumericPredicate::LogicWide(op.clone(), pair)
                } else {
                    NumericPredicate::Logic(op.clone(), pair)
                })
            }
            PredicateKind::Difference(op) => {
                let mut additional = sources.map(|source| match source {
                    CalendarValue::Additional(value) => Ok(value),
                    CalendarValue::Index { .. } => {
                        Err("Fark kontağında doğrudan E/F kullanılamaz".to_string())
                    }
                });
                let triple = [
                    additional.next().expect("validated source count")?,
                    additional.next().expect("validated source count")?,
                    additional.next().expect("validated source count")?,
                ];
                Expr::Numeric(if self.wide {
                    NumericPredicate::DifferenceWide(op.clone(), triple)
                } else {
                    NumericPredicate::Difference(op.clone(), triple)
                })
            }
        })
    }
    pub fn evaluate(&self, memory: &Memory) -> Result<bool, String> {
        self.visit(memory, &mut BTreeSet::new())
    }
    /// Caller retains the existing eager signed/logic and gated difference
    /// contracts. A failed read must precede inversion or boolean fallback.
    pub fn visit(&self, memory: &Memory, observed: &mut BTreeSet<Device>) -> Result<bool, String> {
        let mut sources = Vec::with_capacity(self.sources.len());
        for binding in &self.sources {
            observed.extend(binding.modifier_devices());
            let source = binding.resolve(memory, false)?;
            let value = crate::indexed_operand::read_resolved_value(&source, memory, observed)?;
            sources.push(crate::indexed_operand::frozen_value(value, self.wide));
        }
        Ok(self.expression_from_sources(sources)?.evaluate(memory))
    }
    pub fn short_circuits(&self) -> bool {
        matches!(self.kind, PredicateKind::Difference(_))
    }
    pub fn needs_or_padding(&self, prefix: &str) -> bool {
        prefix == "OR" && !self.wide && self.short_circuits()
    }
    pub fn label(&self, prefix: &str) -> String {
        let suffix = match &self.kind {
            PredicateKind::Signed(op) => op.symbol().to_string(),
            PredicateKind::Logic(op) => match op {
                WordLogic::And => "&",
                WordLogic::Or => "|",
                WordLogic::Xor => "^",
            }
            .into(),
            PredicateKind::Difference(op) => format!("Z{}", op.symbol()),
        };
        let mut parts = vec![format!(
            "{}{prefix}{suffix}",
            if self.wide { "D" } else { "" }
        )];
        parts.extend(self.sources.iter().map(ToString::to_string));
        parts.join(" ")
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        for source in &self.sources {
            source.devices(out);
        }
        Memory::error_devices(out);
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        self.sources
            .iter()
            .flat_map(AddressedValue::modifier_devices)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        for source in &self.sources {
            source.observed_devices(memory, false, out);
        }
    }
    /// Source opcode only. Encoder owns the six balanced INV instructions and
    /// shortcut-PC metadata for the installed word ORZ skip behavior.
    pub fn native_words(&self, prefix: &str) -> Result<Vec<u16>, String> {
        let prefix_id = match prefix {
            "LD" => 0,
            "AND" => 1,
            "OR" => 2,
            _ => return Err("Kontak öneki LD/AND/OR olmalı".into()),
        };
        let wide = u16::from(self.wide);
        let header = match &self.kind {
            PredicateKind::Signed(op) => {
                (([0xd0, 0xe0, 0xf0][prefix_id] + op.opcode_offset() + wide) << 8) | 1
            }
            PredicateKind::Logic(op) => {
                let offset = match op {
                    WordLogic::And => 0,
                    WordLogic::Or => 2,
                    WordLogic::Xor => 4,
                };
                (([0xbe, 0xc4, 0xca][prefix_id] + offset + wide) << 8) | 1
            }
            PredicateKind::Difference(op) => {
                let offset = match op {
                    Relation::Gt => 0,
                    Relation::Ge => 2,
                    Relation::Lt => 4,
                    Relation::Le => 6,
                    Relation::Eq => 8,
                    Relation::Ne => 10,
                };
                (([0x70, 0x7c, 0x88][prefix_id] + offset + wide) << 8) | 2
            }
        };
        let mut words = vec![header];
        for source in &self.sources {
            words.extend(source.native_words()?);
        }
        Ok(words)
    }
}
