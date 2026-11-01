//! Typed control flow and a program-independent, bounded control kernel.
//! It does not implement an interrupt scheduler, physical watchdog, or STL entry.
use crate::calendar_operand::{CalendarIndices, CalendarValue};
use crate::interrupt::InterruptId;
use crate::plc::{Area, Device, Memory};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PointerProfile {
    #[default]
    Direct,
    /// Compiler/native recovered extension; absent from the selected PDF table.
    InstalledIndexed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointerIndex {
    pub bank: u8,
    pub high_bank: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pointer {
    pub base: u8,
    pub index: Option<PointerIndex>,
}
impl Pointer {
    pub fn direct(base: u8) -> Self {
        Self { base, index: None }
    }
    pub fn parse(raw: &str) -> Result<Self, String> {
        Self::parse_with_profile(raw, PointerProfile::Direct)
    }
    pub fn parse_with_profile(raw: &str, profile: PointerProfile) -> Result<Self, String> {
        let text = raw.trim().to_ascii_uppercase();
        let tail = text.strip_prefix('P').ok_or("İşaretçi P0..P255 olmalı")?;
        let split = tail.find(['E', 'F']).unwrap_or(tail.len());
        let number = &tail[..split];
        if number.is_empty() || !number.bytes().all(|v| v.is_ascii_digit()) {
            return Err("İşaretçi P0..P255 olmalı".into());
        }
        let base = number
            .parse::<u8>()
            .map_err(|_| "İşaretçi P0..P255 olmalı")?;
        let index = if split < tail.len() {
            if profile != PointerProfile::InstalledIndexed {
                return Err(
                    "İndeksli P işaretçisi kurulu-runtime uzantı profili gerektirir".into(),
                );
            }
            let suffix = &tail[split..];
            let bank = if suffix.len() == 1 {
                0
            } else if suffix.len() == 2 && suffix.as_bytes()[1].is_ascii_digit() {
                suffix.as_bytes()[1] - b'0'
            } else {
                return Err("P indeksi E/F veya E0..E7/F0..F7 olmalı".into());
            };
            if bank > 7 {
                return Err("P indeks bankı 0..7 olmalı".into());
            }
            Some(PointerIndex {
                bank,
                high_bank: suffix.starts_with('F'),
            })
        } else {
            None
        };
        Ok(Self { base, index })
    }
    pub fn resolve(&self, m: &Memory) -> Option<u8> {
        let offset = match self.index {
            Some(i) => CalendarIndices::read_word(m, i.bank, i.high_bank).ok()?,
            None => 0,
        };
        let value = (u16::from(self.base).wrapping_add(offset as u16)) as i16;
        u8::try_from(value).ok()
    }
    pub fn native_words(&self) -> Result<[u16; 2], String> {
        let (kind, bank) = match self.index {
            None => (0x88, 0),
            Some(i) if i.bank <= 7 => (
                if i.high_bank { 0xf8 } else { 0xe8 },
                u16::from(i.bank) << 13,
            ),
            _ => return Err("P indeks bankı geçersiz".into()),
        };
        Ok([(u16::from(self.base) << 8) | kind, bank | 0x80])
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        if let Some(i) = self.index
            && let Ok(d) = CalendarIndices::alias_device(i.bank, i.high_bank)
        {
            out.insert(d);
        }
    }
}
impl fmt::Display for Pointer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "P{}", self.base)?;
        if let Some(i) = self.index {
            write!(f, "{}{}", if i.high_bank { 'F' } else { 'E' }, i.bank)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ControlOp {
    Noop,
    Step { step: Device },
    Jump { pointer: Pointer, pulse: bool },
    Call { pointer: Pointer, pulse: bool },
    SubReturn,
    MainEnd,
    For { count: CalendarValue },
    Next,
    MasterEnter { level: u8 },
    MasterExit { level: u8 },
    StepReturn,
    Label(u8),
    InterruptLabel(InterruptId),
    InterruptReturn,
    InterruptEnable,
    InterruptDisable,
    Watchdog { pulse: bool },
}
fn master_level(raw: &str) -> Result<u8, String> {
    let text = raw.to_ascii_uppercase();
    let n = text
        .strip_prefix('N')
        .and_then(|s| {
            if s.len() == 1 {
                s.parse::<u8>().ok()
            } else {
                None
            }
        })
        .filter(|n| *n < 8)
        .ok_or("Ana kontrol seviyesi N0..N7 olmalı")?;
    Ok(n)
}
impl ControlOp {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        Self::parse_with_profile(name, args, PointerProfile::Direct)
    }
    pub fn parse_with_profile(
        name: &str,
        args: &[String],
        profile: PointerProfile,
    ) -> Result<Self, String> {
        let name = name.to_ascii_uppercase();
        if name.starts_with('I') && name[1..].bytes().all(|b| b.is_ascii_digit()) && name.len() > 1
        {
            if !args.is_empty() {
                return Err("I etiketi operand almaz".into());
            }
            return Ok(Self::InterruptLabel(name.parse()?));
        }
        if name.starts_with('P') && name != "P" {
            if !args.is_empty() {
                return Err("P etiketi operand almaz".into());
            }
            return Ok(Self::Label(Pointer::parse(&name)?.base));
        }
        let arity = match name.as_str() {
            "CJ" | "CJP" | "CALL" | "CALLP" | "FOR" | "MC" | "MCR" | "P" | "STL" => 1,
            "SRET" | "FEND" | "NEXT" | "RET" | "NOP" | "IRET" | "EI" | "DI" | "WDT" | "WDTP" => 0,
            _ => return Err(format!("Desteklenmeyen program kontrolü: {name}")),
        };
        if args.len() != arity {
            return Err(format!("{name}: {arity} operand gerekli"));
        }
        Ok(match name.as_str() {
            "NOP" => Self::Noop,
            "IRET" => Self::InterruptReturn,
            "EI" => Self::InterruptEnable,
            "DI" => Self::InterruptDisable,
            "WDT" | "WDTP" => Self::Watchdog {
                pulse: name == "WDTP",
            },
            "STL" => {
                let step: Device = args[0].parse()?;
                if step.area != Area::S {
                    return Err("STL adımı S0..S1023 olmalı".into());
                }
                Self::Step { step }
            }
            "CJ" | "CJP" => Self::Jump {
                pointer: Pointer::parse_with_profile(&args[0], profile)?,
                pulse: name == "CJP",
            },
            "CALL" | "CALLP" => Self::Call {
                pointer: Pointer::parse_with_profile(&args[0], profile)?,
                pulse: name == "CALLP",
            },
            "SRET" => Self::SubReturn,
            "FEND" => Self::MainEnd,
            "NEXT" => Self::Next,
            "RET" => Self::StepReturn,
            "MC" => Self::MasterEnter {
                level: master_level(&args[0])?,
            },
            "MCR" => Self::MasterExit {
                level: master_level(&args[0])?,
            },
            "P" => Self::Label(Pointer::parse(&args[0])?.base),
            _ => Self::For {
                count: if args[0].starts_with(['T', 'C', 'D']) {
                    CalendarValue::Additional(crate::additional_processing::AdditionalValue::Word(
                        crate::plc::Operand::Register(args[0].parse()?),
                    ))
                } else {
                    CalendarValue::parse(&args[0], false)?
                },
            },
        })
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Noop => "NOP",
            Self::Step { .. } => "STL",
            Self::Jump { pulse: false, .. } => "CJ",
            Self::Jump { pulse: true, .. } => "CJP",
            Self::Call { pulse: false, .. } => "CALL",
            Self::Call { pulse: true, .. } => "CALLP",
            Self::SubReturn => "SRET",
            Self::MainEnd => "FEND",
            Self::For { .. } => "FOR",
            Self::Next => "NEXT",
            Self::MasterEnter { .. } => "MC",
            Self::MasterExit { .. } => "MCR",
            Self::StepReturn => "RET",
            Self::Label(_) => "P",
            Self::InterruptLabel(_) => "I",
            Self::InterruptReturn => "IRET",
            Self::InterruptEnable => "EI",
            Self::InterruptDisable => "DI",
            Self::Watchdog { pulse: false } => "WDT",
            Self::Watchdog { pulse: true } => "WDTP",
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::Step { step } => format!("STL {step}"),
            Self::Jump { pointer, .. } | Self::Call { pointer, .. } => {
                format!("{} {pointer}", self.name())
            }
            Self::For { count } => format!("FOR {count}"),
            Self::MasterEnter { level } | Self::MasterExit { level } => {
                format!("{} N{level}", self.name())
            }
            Self::Label(n) => format!("P{n}"),
            Self::InterruptLabel(id) => id.to_string(),
            _ => self.name().into(),
        }
    }
    pub fn header(&self) -> u16 {
        match self {
            Self::Noop => 0,
            Self::Step { step } => ((step.index & 255) << 8) | 0xa0 | (step.index >> 8),
            Self::Jump { pulse, .. } => {
                if *pulse {
                    0x1010
                } else {
                    0x1000
                }
            }
            Self::Call { pulse, .. } => {
                if *pulse {
                    0x1210
                } else {
                    0x1200
                }
            }
            Self::SubReturn => 0x1400,
            Self::MainEnd => 0x1c00,
            Self::For { .. } => 0x2000,
            Self::Next => 0x2200,
            Self::MasterEnter { .. } => 0x0a00,
            Self::MasterExit { .. } => 0x0b00,
            Self::StepReturn => 0x0e00,
            Self::Label(n) => (u16::from(*n) << 8) | 0xb0,
            // Genuine WPL compact label family, not an operand descriptor.
            Self::InterruptLabel(id) => (0xb800_u16 + id.number()).swap_bytes(),
            Self::InterruptReturn => 0x1600,
            Self::InterruptEnable => 0x1800,
            Self::InterruptDisable => 0x1a00,
            Self::Watchdog { pulse } => {
                if *pulse {
                    0x1e10
                } else {
                    0x1e00
                }
            }
        }
    }
    pub fn word_len(&self) -> u16 {
        match self {
            Self::Jump { .. }
            | Self::Call { .. }
            | Self::For { .. }
            | Self::MasterEnter { .. }
            | Self::MasterExit { .. } => 3,
            _ => 1,
        }
    }
    pub fn header_length(&self) -> (u16, u16) {
        (self.header(), self.word_len())
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        match self {
            Self::Step { step } => {
                out.insert(*step);
            }
            Self::Jump { pointer, .. } | Self::Call { pointer, .. } => pointer.devices(out),
            Self::For { count } => count.devices(out),
            Self::InterruptLabel(_)
            | Self::InterruptEnable
            | Self::InterruptDisable
            | Self::InterruptReturn => {
                for index in (1050..=1059).chain([1280, 1284, 1286]) {
                    out.insert(Memory::system_device(index));
                }
                if let Self::InterruptLabel(InterruptId::External { input, .. }) = self {
                    out.insert(Device {
                        area: Area::X,
                        index: u16::from(*input),
                    });
                }
            }
            Self::Watchdog { .. } => {
                out.insert(Memory::system_device(1008));
                for index in [1000, 1008] {
                    out.insert(Device {
                        area: Area::D,
                        index,
                    });
                }
            }
            _ => {}
        }
    }
    /// Numeric FOR operands should use the shared scalar encoder. Index and
    /// pointer descriptors are available directly. MC/MCR uses the genuine WPL
    /// mode1 N descriptor82008800, not arbitrary ignored-operand padding.
    pub fn native_word_operand(&self) -> Result<Option<[u16; 2]>, String> {
        match self {
            Self::Jump { pointer, .. } | Self::Call { pointer, .. } => {
                Ok(Some(pointer.native_words()?))
            }
            Self::For { count } => {
                if count.wide() {
                    return Err("FOR sayacı 16 bit olmalı".into());
                }
                Ok(count.native_words().map(|v| [v[0], v[1]]))
            }
            Self::MasterEnter { level } | Self::MasterExit { level } => {
                if *level > 7 {
                    return Err("Ana kontrol seviyesi N0..N7 olmalı".into());
                }
                Ok(Some([(u16::from(*level) << 8) | 0x88, 0x82]))
            }
            _ => Ok(None),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ControlProfile {
    /// Five FOR levels and guarded unmatched returns / scan boundaries; PDF
    /// master-state stack may hold32 entries across permitted CJ boundaries.
    #[default]
    Documented,
    /// Explicit old simulator quirks: master BYTE, seven safe FOR levels,
    /// empty SRET falls through, END/FEND drops call/loop state.
    InstalledLegacy,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlSite {
    pub pc: u16,
    pub fallthrough: u16,
    pub line: usize,
}
impl ControlSite {
    pub fn new(pc: u16, fallthrough: u16, line: usize) -> Self {
        Self {
            pc,
            fallthrough,
            line,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopLink {
    pub for_pc: u16,
    pub body_pc: u16,
    pub next_pc: u16,
    pub after_pc: u16,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ControlLinks {
    /// Actual post-label wordPC, not the PC of the label token.
    pub labels: [Option<u16>; 256],
    /// Same pair is indexed by BOTH its FOR and its NEXT header PC.
    pub loops: BTreeMap<u16, LoopLink>,
    pub interrupts: BTreeMap<InterruptId, u16>,
    /// Header through IRET inclusive; ordinary CALL subroutines are outside.
    pub interrupt_regions: BTreeMap<u16, u16>,
}
impl Default for ControlLinks {
    fn default() -> Self {
        Self {
            labels: [None; 256],
            loops: BTreeMap::new(),
            interrupts: BTreeMap::new(),
            interrupt_regions: BTreeMap::new(),
        }
    }
}
impl ControlLinks {
    pub fn define_label(&mut self, n: u8, target: u16) -> Result<(), String> {
        if self.labels[usize::from(n)].is_some() {
            return Err(format!("P{n} etiketi yineleniyor"));
        }
        self.labels[usize::from(n)] = Some(target);
        Ok(())
    }
    pub fn insert_loop(&mut self, pair: LoopLink) -> Result<(), String> {
        if pair.for_pc == pair.next_pc
            || self.loops.contains_key(&pair.for_pc)
            || self.loops.contains_key(&pair.next_pc)
        {
            return Err("FOR/NEXT eşleşmesi yineleniyor".into());
        }
        self.loops.insert(pair.for_pc, pair);
        self.loops.insert(pair.next_pc, pair);
        Ok(())
    }
    /// Control-only linking helper. `program_end` is the real final END PC,
    /// supplied by the root lowerer; non-control instruction PCs may lie in gaps.
    /// Jump/loop body ancestry and complete plan alignment remain root checks.
    pub fn link(
        statements: &[ControlStatement],
        program_end: u16,
        profile: ControlProfile,
    ) -> Result<Self, String> {
        let mut links = Self::default();
        let mut loop_stack: Vec<ControlSite> = Vec::new();
        let mut master_stack: Vec<u8> = Vec::new();
        let mut usage: BTreeMap<u8, (bool, bool)> = BTreeMap::new();
        let last_fend = statements
            .iter()
            .filter(|s| matches!(s.op, ControlOp::MainEnd))
            .map(|s| s.site.pc)
            .max();
        let max_loops = if profile == ControlProfile::Documented {
            5
        } else {
            7
        };
        let mut previous = None;
        let mut interrupt_start: Option<u16> = None;
        for statement in statements {
            let site = statement.site;
            let op = &statement.op;
            if site.pc >= program_end
                || site.fallthrough > program_end
                || site.fallthrough <= site.pc
            {
                return Err(format!(
                    "Satır {}: kontrol PC/uzunluk sınırı geçersiz",
                    site.line
                ));
            }
            if previous.is_some_and(|p| p >= site.pc) {
                return Err("Kontrol düğümleri benzersiz PC sırasıyla olmalı".into());
            }
            previous = Some(site.pc);
            match op {
                ControlOp::Noop
                | ControlOp::InterruptEnable
                | ControlOp::InterruptDisable
                | ControlOp::Watchdog { .. } => {}
                ControlOp::InterruptLabel(id) => {
                    if !last_fend.is_some_and(|pc| site.pc > pc) {
                        return Err(format!("{id} kesme gövdesi son FEND'den sonra olmalı"));
                    }
                    if interrupt_start.is_some()
                        || !loop_stack.is_empty()
                        || !master_stack.is_empty()
                    {
                        return Err("Yeni I etiketi öncesinde IRET/FOR/MC bölgesi kapanmalı".into());
                    }
                    if links.interrupts.insert(*id, site.fallthrough).is_some() {
                        return Err(format!("{id} kesme etiketi yineleniyor"));
                    }
                    interrupt_start = Some(site.pc);
                }
                ControlOp::InterruptReturn => {
                    if !loop_stack.is_empty() || !master_stack.is_empty() {
                        return Err("IRET öncesinde FOR/MC bölgesi kapanmalı".into());
                    }
                    let start = interrupt_start
                        .take()
                        .ok_or("I etiketi olmadan IRET kullanılamaz")?;
                    links.interrupt_regions.insert(start, site.pc);
                }
                ControlOp::Step { step } => {
                    if step.area != Area::S || step.index > 1023 {
                        return Err("STL adımı S0..1023 olmalı".into());
                    }
                    if !loop_stack.is_empty() {
                        return Err("C405: FOR içinde STL kullanılamaz".into());
                    }
                    if last_fend.is_some_and(|p| site.pc > p) {
                        return Err("C409: alt programda STL kullanılamaz".into());
                    }
                }
                ControlOp::Label(n) => links.define_label(*n, site.fallthrough)?,
                ControlOp::Jump { pointer, .. } | ControlOp::Call { pointer, .. } => {
                    if pointer.index.is_none() {
                        let entry = usage.entry(pointer.base).or_default();
                        if matches!(op, ControlOp::Call { .. }) {
                            entry.1 = true;
                        } else {
                            entry.0 = true;
                        }
                    }
                }
                ControlOp::For { .. } => {
                    if loop_stack.len() >= max_loops {
                        return Err(format!(
                            "Satır {}: FOR iç içe sınırı {max_loops}",
                            site.line
                        ));
                    }
                    loop_stack.push(site);
                }
                ControlOp::Next => {
                    let start = loop_stack
                        .pop()
                        .ok_or_else(|| format!("Satır {}: eşleşmeyen NEXT", site.line))?;
                    links.insert_loop(LoopLink {
                        for_pc: start.pc,
                        body_pc: start.fallthrough,
                        next_pc: site.pc,
                        after_pc: site.fallthrough,
                    })?;
                }
                ControlOp::MasterEnter { level } => {
                    if !loop_stack.is_empty() {
                        return Err("C405: FOR içinde MC/MCR kullanılamaz".into());
                    }
                    if last_fend.is_some_and(|p| site.pc > p) {
                        return Err("C40A: alt programda MC/MCR kullanılamaz".into());
                    }
                    if usize::from(*level) != master_stack.len() || *level > 7 {
                        return Err("C40B: MC seviyeleri N0'dan sırayla başlamalı".into());
                    }
                    master_stack.push(*level);
                }
                ControlOp::MasterExit { level } => {
                    if !loop_stack.is_empty() {
                        return Err("C405: FOR içinde MC/MCR kullanılamaz".into());
                    }
                    if master_stack.pop() != Some(*level) {
                        return Err("C40C: MC/MCR N seviyeleri eşleşmiyor".into());
                    }
                }
                ControlOp::MainEnd => {
                    if !loop_stack.is_empty() {
                        return Err("C405: FOR/NEXT arasında FEND var".into());
                    }
                    if !master_stack.is_empty() {
                        return Err("FEND öncesi kapatılmamış MC".into());
                    }
                }
                ControlOp::SubReturn => {
                    if interrupt_start.is_some() {
                        return Err("Kesme gövdesi SRET yerine IRET ile bitmeli".into());
                    }
                    if !loop_stack.is_empty() {
                        return Err("C405: FOR içinde SRET kullanılamaz".into());
                    }
                    if !last_fend.is_some_and(|p| site.pc > p) {
                        return Err("C40E: SRET son FEND'den sonra olmalı".into());
                    }
                }
                ControlOp::StepReturn => {
                    if !loop_stack.is_empty() {
                        return Err("C405: FOR içinde RET kullanılamaz".into());
                    }
                    if last_fend.is_some_and(|p| site.pc > p) {
                        return Err("C409: alt programda STL/RET kullanılamaz".into());
                    }
                }
            }
        }
        if !loop_stack.is_empty() {
            return Err("FOR için NEXT eksik".into());
        }
        if interrupt_start.is_some() {
            return Err("Kesme gövdesi için IRET eksik".into());
        }
        if !master_stack.is_empty() {
            return Err("MC için MCR eksik".into());
        }
        for (label, (jump, call)) in usage {
            if jump && call {
                return Err(format!("P{label} hem CJ hem CALL hedefi olamaz"));
            }
            let target = links.labels[usize::from(label)]
                .ok_or_else(|| format!("P{label} etiketi tanımlı değil"))?;
            if call && !last_fend.is_some_and(|p| target > p) {
                return Err(format!("P{label} CALL gövdesi son FEND'den sonra olmalı"));
            }
        }
        Ok(links)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct ControlStatement {
    pub site: ControlSite,
    pub op: ControlOp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlFaultKind {
    Budget,
    MissingLink,
    InvalidRegion,
    CallUnderflow,
    LoopUnderflow,
    LoopOverflow,
    MasterOverflow,
    MasterUnderflow,
    UnsupportedOperand,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlFault {
    pub pc: u16,
    pub line: usize,
    pub steps: u64,
    pub kind: ControlFaultKind,
    pub reason: String,
}
impl fmt::Display for ControlFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Satır {}, PC {}: {} ({} adım)",
            self.line, self.pc, self.reason, self.steps
        )
    }
}
impl std::error::Error for ControlFault {}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlTransition {
    pub next_pc: u16,
    pub scan_boundary: bool,
    pub step_return: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallFrame {
    pub return_pc: u16,
    pub logic_word: u16,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopFrame {
    pub link: LoopLink,
    pub count: u16,
    pub iteration: u16,
}

#[derive(Debug, Clone)]
pub struct ControlState {
    pub profile: ControlProfile,
    pub logic_word: u16,
    pub master_gate: u8,
    pub calls: Vec<CallFrame>,
    pub loops: Vec<LoopFrame>,
    pub steps: u64,
    /// Immutable during a run by the caller's policy. WDT must never reset it.
    pub hard_step_limit: u64,
    pub pulse_history: BTreeMap<u16, bool>,
    master_depth: u8,
    master_frames: Vec<u8>,
    pub step_context_active: bool,
    /// Native flag80 controls compact OUT within STL. Ordinary non-STL starts
    /// enabled; this is independent of the current logic WORD or first LD.
    pub out_enabled: bool,
}
impl Default for ControlState {
    fn default() -> Self {
        Self::new(ControlProfile::Documented, 100_000)
    }
}
impl ControlState {
    pub fn new(profile: ControlProfile, hard_step_limit: u64) -> Self {
        Self {
            profile,
            logic_word: 0,
            master_gate: 1,
            calls: Vec::new(),
            loops: Vec::new(),
            steps: 0,
            hard_step_limit,
            pulse_history: BTreeMap::new(),
            master_depth: 0,
            master_frames: Vec::new(),
            step_context_active: false,
            out_enabled: true,
        }
    }
    /// Normal M1170-OFF engine entry, not NEXT/CALL. The runner resets these
    /// three fields; MC/STL depth and pulse history survive. A cold restart
    /// must use reset_instruction_state, not repeatedly construct a new kernel.
    pub fn begin_scan(&mut self) {
        self.logic_word = 0;
        self.master_gate = 1;
        self.out_enabled = true;
        self.steps = 0;
    }
    /// M1170-ON instruction stepping bypasses normal runner entry resets.
    /// END has already cleared CALL/FOR; L/master/MC/STL/outflag are retained.
    pub fn begin_stepped_scan(&mut self) {
        self.steps = 0;
    }
    pub fn reset_instruction_state(&mut self) {
        let profile = self.profile;
        let limit = self.hard_step_limit;
        *self = Self::new(profile, limit);
    }
    pub fn effective_power(&self) -> bool {
        self.logic_word & 1 != 0 && self.master_gate & 1 != 0
    }
    pub fn call_depth(&self) -> usize {
        self.calls.len()
    }
    pub fn loop_depth(&self) -> usize {
        self.loops.len()
    }
    pub fn master_depth(&self) -> usize {
        if self.profile == ControlProfile::Documented {
            self.master_frames.len()
        } else {
            usize::from(self.master_depth)
        }
    }
    pub fn load(&mut self, value: bool) {
        self.logic_word = (self.logic_word << 1) | u16::from(value);
    }
    pub fn and(&mut self, value: bool) {
        if !value {
            self.logic_word &= !1;
        }
    }
    pub fn or(&mut self, value: bool) {
        if value {
            self.logic_word |= 1;
        }
    }
    pub fn invert(&mut self) {
        self.logic_word ^= 1;
    }
    pub fn and_block(&mut self) {
        let value = self.logic_word & 1 != 0 && self.logic_word & 2 != 0;
        self.logic_word = (self.logic_word >> 1) & !1 | u16::from(value);
    }
    pub fn or_block(&mut self) {
        let value = self.logic_word & 3 != 0;
        self.logic_word = (self.logic_word >> 1) & !1 | u16::from(value);
    }
    /// Actual F8/F9/FA word operations, NOT a separate saved-WORD stack.
    pub fn mps(&mut self) {
        self.logic_word = (self.logic_word << 1) | (self.logic_word & 1);
    }
    pub fn mrd(&mut self) {
        self.logic_word = (self.logic_word & !1) | ((self.logic_word >> 1) & 1);
    }
    pub fn mpp(&mut self) {
        self.logic_word >>= 1;
    }
    fn fault(
        &self,
        site: ControlSite,
        kind: ControlFaultKind,
        reason: impl Into<String>,
    ) -> ControlFault {
        ControlFault {
            pc: site.pc,
            line: site.line,
            steps: self.steps,
            kind,
            reason: reason.into(),
        }
    }
    /// Non-control plan nodes call this once. `transition` charges its own
    /// control node; callers must not charge that same visit a second time.
    pub fn charge_step(&mut self, site: ControlSite) -> Result<(), ControlFault> {
        if self.steps >= self.hard_step_limit {
            return Err(self.fault(
                site,
                ControlFaultKind::Budget,
                "Dinamik yürütme adım sınırı aşıldı; tarama tamamlanmadı",
            ));
        }
        self.steps += 1;
        Ok(())
    }
    pub(crate) fn qualify(&mut self, site: ControlSite, pulse: bool) -> bool {
        let power = self.effective_power();
        if !pulse {
            return power;
        }
        let old = self.pulse_history.insert(site.pc, power).unwrap_or(false);
        power && !old
    }
    pub fn transition(
        &mut self,
        op: &ControlOp,
        site: ControlSite,
        m: &Memory,
        links: &ControlLinks,
    ) -> Result<ControlTransition, ControlFault> {
        self.charge_step(site)?;
        let mut result = ControlTransition {
            next_pc: site.fallthrough,
            scan_boundary: false,
            step_return: false,
        };
        match op {
            ControlOp::Noop => {}
            ControlOp::InterruptLabel(_)
            | ControlOp::InterruptReturn
            | ControlOp::InterruptEnable
            | ControlOp::InterruptDisable
            | ControlOp::Watchdog { .. } => {
                return Err(self.fault(
                    site,
                    ControlFaultKind::InvalidRegion,
                    "Kesme/watchdog için VM zaman ve olay bağlamı gerekli",
                ));
            }
            ControlOp::Step { .. } => {
                return Err(self.fault(
                    site,
                    ControlFaultKind::InvalidRegion,
                    "STL için SFC yürütme bağlamı gerekli",
                ));
            }
            ControlOp::Label(_) => {}
            ControlOp::Jump { pointer, pulse } | ControlOp::Call { pointer, pulse } => {
                if !self.qualify(site, *pulse) {
                    return Ok(result);
                }
                let call = matches!(op, ControlOp::Call { .. });
                if call && self.calls.len() >= 5 {
                    return Ok(result);
                }
                let target = pointer
                    .resolve(m)
                    .and_then(|n| links.labels[usize::from(n)]);
                let Some(target) = target else {
                    if self.profile == ControlProfile::InstalledLegacy {
                        return Ok(result);
                    }
                    return Err(self.fault(
                        site,
                        ControlFaultKind::MissingLink,
                        "Etiket hedefi tanımlı değil veya indeks sınır dışında",
                    ));
                };
                if call {
                    self.calls.push(CallFrame {
                        return_pc: site.fallthrough,
                        logic_word: self.logic_word,
                    });
                    self.logic_word = 0;
                }
                result.next_pc = target;
            }
            ControlOp::SubReturn => {
                if let Some(frame) = self.calls.pop() {
                    self.logic_word = frame.logic_word;
                    result.next_pc = frame.return_pc;
                } else if self.profile != ControlProfile::InstalledLegacy {
                    return Err(self.fault(
                        site,
                        ControlFaultKind::CallUnderflow,
                        "Aktif CALL olmadan SRET",
                    ));
                }
            }
            ControlOp::MainEnd => return self.finish_scan(site),
            ControlOp::For { count } => {
                if count.wide() {
                    return Err(self.fault(
                        site,
                        ControlFaultKind::UnsupportedOperand,
                        "FOR sayacı 16 bit olmalı",
                    ));
                }
                let limit = if self.profile == ControlProfile::Documented {
                    5
                } else {
                    7
                };
                if self.loops.len() >= limit {
                    return Err(self.fault(site,ControlFaultKind::LoopOverflow,format!("Güvenli FOR iç içe sınırı {limit}; native sekizinci-slot hatası çalıştırılmaz")));
                }
                let link = *links
                    .loops
                    .get(&site.pc)
                    .filter(|l| l.for_pc == site.pc)
                    .ok_or_else(|| {
                        self.fault(
                            site,
                            ControlFaultKind::MissingLink,
                            "FOR/NEXT bağlantısı eksik",
                        )
                    })?;
                let mut count_devices = BTreeSet::new();
                count.devices(&mut count_devices);
                for d in count_devices {
                    if (d.area == Area::D && d.index > m.data_register_limit.unwrap_or(9999))
                        || (d.area == Area::M && d.index > m.relay_limit.unwrap_or(4095))
                    {
                        return Err(self.fault(
                            site,
                            ControlFaultKind::UnsupportedOperand,
                            "FOR sayacı model aralığı dışında",
                        ));
                    }
                }
                let captured = (count.read(m) as i16).max(1) as u16;
                self.loops.push(LoopFrame {
                    link,
                    count: captured,
                    iteration: 0,
                });
                result.next_pc = link.body_pc;
            }
            ControlOp::Next => {
                let Some(frame) = self.loops.last_mut() else {
                    return Err(self.fault(
                        site,
                        ControlFaultKind::LoopUnderflow,
                        "Aktif FOR olmadan NEXT; native PC takılma yolu engellendi",
                    ));
                };
                if frame.link.next_pc != site.pc {
                    return Err(self.fault(
                        site,
                        ControlFaultKind::InvalidRegion,
                        "NEXT aktif FOR çerçevesiyle eşleşmiyor",
                    ));
                }
                frame.iteration += 1;
                if frame.iteration < frame.count {
                    result.next_pc = frame.link.body_pc;
                } else {
                    result.next_pc = frame.link.after_pc;
                    self.loops.pop();
                }
            }
            ControlOp::MasterEnter { .. } => {
                let old = self.master_gate;
                if self.profile == ControlProfile::Documented {
                    if self.master_frames.len() >= 32 {
                        return Err(self.fault(
                            site,
                            ControlFaultKind::MasterOverflow,
                            "Ana kontrol durumu 32 öğe sınırında",
                        ));
                    }
                    self.master_frames.push(old);
                } else if self.master_depth < 8 {
                    self.master_depth += 1;
                }
                self.master_gate = (old << 1) | u8::from(self.logic_word & 1 != 0 && old & 1 != 0);
            }
            ControlOp::MasterExit { .. } => {
                if self.profile == ControlProfile::Documented {
                    // A permitted CJ outside->inside can reach an MCR without
                    // dynamically visiting its MC. Empty runtime pop holds;
                    // lexical N pairing is the linker's separate obligation.
                    if let Some(gate) = self.master_frames.pop() {
                        self.master_gate = gate;
                    }
                } else if self.master_depth > 0 {
                    self.master_depth -= 1;
                    self.master_gate = if self.master_depth == 0 {
                        1
                    } else {
                        self.master_gate >> 1
                    };
                }
            }
            ControlOp::StepReturn => {
                // 53AC99 is not proven to be an MPS depth. RET never shifts or
                // clears53A400, never pops CALL, and does not end this scan.
                // The installed RET leaves485.bit08 intact. This marker is
                // distinct from predecessor depth, which the SFC VM clears.
                self.out_enabled = true;
                self.master_gate |= 1;
                result.step_return = true;
            }
        }
        Ok(result)
    }
    /// Final END uses this same guarded boundary seam. It does not charge a
    /// second step when called from MainEnd; external END nodes charge first.
    pub fn finish_scan(&mut self, site: ControlSite) -> Result<ControlTransition, ControlFault> {
        if self.profile == ControlProfile::Documented
            && (!self.calls.is_empty() || !self.loops.is_empty())
        {
            return Err(self.fault(
                site,
                ControlFaultKind::InvalidRegion,
                "END/FEND aktif CALL veya FOR tamamlanmadan yürütüldü",
            ));
        }
        self.calls.clear();
        self.loops.clear();
        Ok(ControlTransition {
            next_pc: 0,
            scan_boundary: true,
            step_return: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn op(s: &str) -> ControlOp {
        let mut w = s.split_whitespace();
        ControlOp::parse(
            w.next().unwrap(),
            &w.map(str::to_string).collect::<Vec<_>>(),
        )
        .unwrap()
    }
    fn site(pc: u16, next: u16) -> ControlSite {
        ControlSite::new(pc, next, usize::from(pc) + 1)
    }
    #[test]
    fn syntax_and_word_layout_do_not_conflate_return_kinds_or_suffixes() {
        for (source, header, len) in [
            ("CJ P255", 0x1000, 3),
            ("CJP P0", 0x1010, 3),
            ("CALLP P7", 0x1210, 3),
            ("SRET", 0x1400, 1),
            ("IRET", 0x1600, 1),
            ("EI", 0x1800, 1),
            ("DI", 0x1a00, 1),
            ("WDT", 0x1e00, 1),
            ("WDTP", 0x1e10, 1),
            ("I610", 0x62ba, 1),
            ("FEND", 0x1c00, 1),
            ("FOR K-1", 0x2000, 3),
            ("NEXT", 0x2200, 1),
            ("MC N7", 0xa00, 3),
            ("MCR N0", 0xb00, 3),
            ("RET", 0xe00, 1),
            ("P23", 0x17b0, 1),
        ] {
            assert_eq!(op(source).header_length(), (header, len));
        }
        for name in ["IRETP", "EIP", "DIP", "DWDT", "FORP", "SRETP", "DCJ", "MCP"] {
            assert!(ControlOp::parse(name, &[]).is_err());
        }
        assert!(Pointer::parse("P3E1").is_err());
        assert!(Pointer::parse("P256").is_err());
        assert_eq!(Pointer::direct(255).native_words().unwrap(), [0xff88, 0x80]);
        for level in 0u16..8 {
            assert_eq!(
                op(&format!("MC N{level}")).native_word_operand().unwrap(),
                Some([(level << 8) | 0x88, 0x82])
            );
        }
        let p = Pointer::parse_with_profile("P3E1", PointerProfile::InstalledIndexed).unwrap();
        assert_eq!(p.native_words().unwrap(), [0x03e8, 0x2080]);
        let mut m = Memory::default();
        CalendarIndices::write_word(&mut m, 1, false, -3).unwrap();
        assert_eq!(p.resolve(&m), Some(0));
        CalendarIndices::write_word(&mut m, 1, false, -4).unwrap();
        assert_eq!(p.resolve(&m), None);
    }
    #[test]
    fn logic_word_stack_operations_preserve_upper_bits_and_call_restores_all() {
        let mut s = ControlState {
            logic_word: 0xa5a1,
            ..ControlState::default()
        };
        s.mps();
        assert_eq!(s.logic_word, 0x4b43);
        s.and(false);
        s.mrd();
        assert_eq!(s.logic_word, 0x4b43);
        s.mpp();
        assert_eq!(s.logic_word, 0x25a1);
        let mut links = ControlLinks::default();
        links.define_label(1, 30).unwrap();
        let m = Memory::default();
        let saved = s.logic_word;
        assert_eq!(
            s.transition(&op("CALL P1"), site(3, 6), &m, &links)
                .unwrap()
                .next_pc,
            30
        );
        assert_eq!(s.logic_word, 0);
        s.load(true);
        s.mps();
        assert_eq!(
            s.transition(&op("SRET"), site(40, 41), &m, &links)
                .unwrap()
                .next_pc,
            6
        );
        assert_eq!(s.logic_word, saved);
    }
    #[test]
    fn pulse_is_per_occurrence_per_visit_and_skipped_scan_does_not_rearm() {
        let mut s = ControlState::default();
        let mut links = ControlLinks::default();
        links.define_label(1, 50).unwrap();
        let m = Memory::default();
        let jump = op("CJP P1");
        s.logic_word = 1;
        assert_eq!(
            s.transition(&jump, site(4, 7), &m, &links).unwrap().next_pc,
            50
        );
        assert_eq!(
            s.transition(&jump, site(4, 7), &m, &links).unwrap().next_pc,
            7
        );
        s.begin_scan();
        s.logic_word = 1;
        assert_eq!(
            s.transition(&jump, site(4, 7), &m, &links).unwrap().next_pc,
            7
        );
        s.logic_word = 0;
        s.transition(&jump, site(4, 7), &m, &links).unwrap();
        s.logic_word = 1;
        assert_eq!(
            s.transition(&jump, site(4, 7), &m, &links).unwrap().next_pc,
            50
        );
        assert_eq!(
            s.transition(&jump, site(9, 12), &m, &links)
                .unwrap()
                .next_pc,
            50
        );
    }
    #[test]
    fn captured_loop_count_mutation_and_master_false_do_not_change_repetitions() {
        let mut s = ControlState::default();
        let mut links = ControlLinks::default();
        links
            .insert_loop(LoopLink {
                for_pc: 0,
                body_pc: 3,
                next_pc: 8,
                after_pc: 9,
            })
            .unwrap();
        let mut m = Memory::default();
        let d = "D0".parse().unwrap();
        m.set_word(d, 3);
        s.master_gate = 0;
        s.transition(&op("FOR D0"), site(0, 3), &m, &links).unwrap();
        m.set_word(d, 0);
        for expected in [3, 3, 9] {
            assert_eq!(
                s.transition(&op("NEXT"), site(8, 9), &m, &links)
                    .unwrap()
                    .next_pc,
                expected
            );
        }
        assert_eq!(s.loop_depth(), 0);
        s.transition(&op("FOR K-99"), site(0, 3), &m, &links)
            .unwrap();
        assert_eq!(
            s.transition(&op("NEXT"), site(8, 9), &m, &links)
                .unwrap()
                .next_pc,
            9
        );
    }
    #[test]
    fn call_depth_five_sixth_skip_and_nested_exact_returns() {
        let mut s = ControlState::default();
        let mut links = ControlLinks::default();
        links.define_label(1, 90).unwrap();
        let m = Memory::default();
        for pc in 0..5 {
            s.logic_word = 0xa101;
            assert_eq!(
                s.transition(&op("CALL P1"), site(pc * 3, pc * 3 + 3), &m, &links)
                    .unwrap()
                    .next_pc,
                90
            );
        }
        s.logic_word = 1;
        assert_eq!(
            s.transition(&op("CALL P1"), site(18, 21), &m, &links)
                .unwrap()
                .next_pc,
            21
        );
        assert_eq!(s.call_depth(), 5);
        for expected in [15, 12, 9, 6, 3] {
            assert_eq!(
                s.transition(&op("SRET"), site(95, 96), &m, &links)
                    .unwrap()
                    .next_pc,
                expected
            );
            assert_eq!(s.logic_word, 0xa101);
        }
        assert_eq!(
            s.transition(&op("SRET"), site(95, 96), &m, &links)
                .unwrap_err()
                .kind,
            ControlFaultKind::CallUnderflow
        );
    }
    #[test]
    fn master_profiles_are_explicit_and_ret_does_not_return_or_clear_logic() {
        let m = Memory::default();
        let links = ControlLinks::default();
        let mut s = ControlState {
            logic_word: 0x1235,
            ..ControlState::default()
        };
        s.transition(&op("MC N0"), site(0, 3), &m, &links).unwrap();
        s.logic_word = 0x1234;
        s.transition(&op("MC N1"), site(3, 6), &m, &links).unwrap();
        assert!(!s.effective_power());
        s.transition(&op("MCR N1"), site(6, 9), &m, &links).unwrap();
        assert_eq!(s.master_gate & 1, 1);
        s.calls.push(CallFrame {
            return_pc: 77,
            logic_word: 0x55,
        });
        s.step_context_active = true;
        let r = s.transition(&op("RET"), site(9, 10), &m, &links).unwrap();
        assert!(r.step_return);
        assert!(!r.scan_boundary);
        assert_eq!(s.logic_word, 0x1234);
        assert_eq!(s.call_depth(), 1);
        assert!(s.step_context_active);
        let mut legacy = ControlState::new(ControlProfile::InstalledLegacy, 100);
        legacy.logic_word = 1;
        for _ in 0..9 {
            legacy
                .transition(&op("MC N0"), site(0, 3), &m, &links)
                .unwrap();
        }
        assert_eq!(legacy.master_depth(), 8);
        assert_eq!(
            legacy
                .transition(&op("SRET"), site(5, 6), &m, &links)
                .unwrap()
                .next_pc,
            6
        );
    }
    #[test]
    fn linker_validates_symbols_regions_and_for_inside_mc_orientation() {
        let statements = [
            ControlStatement {
                site: site(0, 3),
                op: op("MC N0"),
            },
            ControlStatement {
                site: site(3, 6),
                op: op("FOR K2"),
            },
            ControlStatement {
                site: site(8, 9),
                op: op("NEXT"),
            },
            ControlStatement {
                site: site(9, 12),
                op: op("MCR N0"),
            },
            ControlStatement {
                site: site(12, 13),
                op: op("P1"),
            },
        ];
        let links = ControlLinks::link(&statements, 13, ControlProfile::Documented).unwrap();
        assert_eq!(links.labels[1], Some(13));
        assert_eq!(links.loops[&8].body_pc, 6);
        let invalid = [
            ControlStatement {
                site: site(0, 3),
                op: op("FOR K2"),
            },
            ControlStatement {
                site: site(3, 6),
                op: op("MC N0"),
            },
        ];
        assert!(
            ControlLinks::link(&invalid, 10, ControlProfile::Documented)
                .unwrap_err()
                .starts_with("C405")
        );
        let duplicate = [
            ControlStatement {
                site: site(0, 1),
                op: op("P1"),
            },
            ControlStatement {
                site: site(1, 2),
                op: op("P1"),
            },
        ];
        assert!(ControlLinks::link(&duplicate, 2, ControlProfile::Documented).is_err());
        let mixed = [
            ControlStatement {
                site: site(0, 3),
                op: op("CJ P1"),
            },
            ControlStatement {
                site: site(3, 6),
                op: op("CALL P1"),
            },
            ControlStatement {
                site: site(6, 7),
                op: op("P1"),
            },
        ];
        assert!(ControlLinks::link(&mixed, 7, ControlProfile::Documented).is_err());
    }
    #[test]
    fn budget_and_unmatched_next_fail_without_successful_scan_boundary() {
        let m = Memory::default();
        let links = ControlLinks::default();
        let mut s = ControlState::new(ControlProfile::Documented, 1);
        s.transition(&op("P1"), site(0, 1), &m, &links).unwrap();
        assert_eq!(
            s.transition(&op("P1"), site(0, 1), &m, &links)
                .unwrap_err()
                .kind,
            ControlFaultKind::Budget
        );
        let mut s = ControlState::default();
        assert_eq!(
            s.transition(&op("NEXT"), site(0, 1), &m, &links)
                .unwrap_err()
                .kind,
            ControlFaultKind::LoopUnderflow
        );
        s.calls.push(CallFrame {
            return_pc: 4,
            logic_word: 1,
        });
        assert!(s.finish_scan(site(5, 6)).is_err());
        assert_eq!(s.call_depth(), 1);
    }

    #[test]
    fn grouped_and_index_for_counts_are_captured_from_shared_plc_memory() {
        let mut m = Memory::default();
        m.set_internal("M1".parse().unwrap(), true);
        CalendarIndices::write_word(&mut m, 1, true, 3).unwrap();
        let mut links = ControlLinks::default();
        links
            .insert_loop(LoopLink {
                for_pc: 0,
                body_pc: 3,
                next_pc: 8,
                after_pc: 9,
            })
            .unwrap();
        let mut s = ControlState::default();
        s.transition(&op("FOR K1M0"), site(0, 3), &m, &links)
            .unwrap();
        assert_eq!(s.loops[0].count, 2);
        s.loops.clear();
        s.transition(&op("FOR F1"), site(0, 3), &m, &links).unwrap();
        CalendarIndices::write_word(&mut m, 1, true, 0).unwrap();
        assert_eq!(s.loops[0].count, 3);
        assert_eq!(
            op("FOR F1").native_word_operand().unwrap(),
            Some([0x6e86, 0x0180])
        );
    }

    #[test]
    fn stepped_and_normal_scan_entry_preserve_runtime_master_depth() {
        let m = Memory::default();
        let links = ControlLinks::default();
        let mut s = ControlState::new(ControlProfile::InstalledLegacy, 100);
        s.logic_word = 0xa501;
        s.transition(&op("MC N0"), site(0, 3), &m, &links).unwrap();
        s.logic_word = 0x8123;
        s.out_enabled = false;
        s.step_context_active = true;
        let gate = s.master_gate;
        s.finish_scan(site(10, 11)).unwrap();
        s.begin_stepped_scan();
        assert_eq!(
            (s.logic_word, s.master_gate, s.out_enabled, s.master_depth()),
            (0x8123, gate, false, 1)
        );
        assert!(s.step_context_active);
        assert_eq!(s.steps, 0);
        s.begin_scan();
        assert_eq!(
            (s.logic_word, s.master_gate, s.out_enabled, s.master_depth()),
            (0, 1, true, 1)
        );
        assert!(s.step_context_active);
        s.reset_instruction_state();
        assert_eq!(s.master_depth(), 0);
        assert!(!s.step_context_active);
        let mut s = ControlState {
            master_gate: 0,
            ..ControlState::default()
        };
        assert_eq!(
            s.transition(&op("MCR N0"), site(5, 8), &m, &links)
                .unwrap()
                .next_pc,
            8
        );
        assert_eq!(s.master_gate, 0);
    }
}
