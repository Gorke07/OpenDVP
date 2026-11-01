//! A typed modifier wrapper for the documented single-result scalar subset.
//! Resolve only at an enabled visit, then reuse the ordinary arithmetic action.
//! Multi-write/block/stateful instructions require their own resolution order.
use crate::calendar_operand::{CalendarDestination, CalendarValue};
use crate::indexed_operand::{AddressedValue, has_modifier};
use crate::plc::{Action, Device, Memory, WordLogic, WordUnary};
use crate::wide::{WideAction, WideDestination, WideOp, WideSource};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScalarOp {
    Move,
    Add,
    Sub,
    Inc,
    Dec,
    Complement,
    Neg,
    And,
    Or,
    Xor,
}
impl ScalarOp {
    fn from_name(name: &str) -> Option<(Self, bool)> {
        Some(match name {
            "MOV" => (Self::Move, false),
            "DMOV" => (Self::Move, true),
            "ADD" => (Self::Add, false),
            "DADD" => (Self::Add, true),
            "SUB" => (Self::Sub, false),
            "DSUB" => (Self::Sub, true),
            "INC" => (Self::Inc, false),
            "DINC" => (Self::Inc, true),
            "DEC" => (Self::Dec, false),
            "DDEC" => (Self::Dec, true),
            "CML" => (Self::Complement, false),
            "DCML" => (Self::Complement, true),
            "NEG" => (Self::Neg, false),
            "DNEG" => (Self::Neg, true),
            "WAND" => (Self::And, false),
            "DAND" => (Self::And, true),
            "WOR" => (Self::Or, false),
            "DOR" => (Self::Or, true),
            "WXOR" => (Self::Xor, false),
            "DXOR" => (Self::Xor, true),
            _ => return None,
        })
    }
    fn wide_op(self) -> WideOp {
        match self {
            Self::Move => WideOp::Move,
            Self::Add => WideOp::Add,
            Self::Sub => WideOp::Sub,
            Self::Inc => WideOp::Inc,
            Self::Dec => WideOp::Dec,
            Self::Complement => WideOp::Complement,
            Self::Neg => WideOp::Neg,
            Self::And => WideOp::And,
            Self::Or => WideOp::Or,
            Self::Xor => WideOp::Xor,
        }
    }
    fn source_count(self) -> usize {
        match self {
            Self::Inc | Self::Dec | Self::Neg => 0,
            Self::Move | Self::Complement => 1,
            _ => 2,
        }
    }
    fn name(self, wide: bool) -> &'static str {
        if wide {
            self.wide_op().name()
        } else {
            match self {
                Self::Move => "MOV",
                Self::Add => "ADD",
                Self::Sub => "SUB",
                Self::Inc => "INC",
                Self::Dec => "DEC",
                Self::Complement => "CML",
                Self::Neg => "NEG",
                Self::And => "WAND",
                Self::Or => "WOR",
                Self::Xor => "WXOR",
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedScalarAction {
    op: ScalarOp,
    wide: bool,
    pulse_label: bool,
    sources: Vec<AddressedValue>,
    dest: AddressedValue,
}
impl IndexedScalarAction {
    pub fn supports(name: &str) -> bool {
        let upper = name.to_ascii_uppercase();
        ScalarOp::from_name(upper.strip_suffix('P').unwrap_or(&upper)).is_some()
    }
    pub fn has_modifiers(args: &[String]) -> bool {
        args.iter().any(|s| has_modifier(s))
    }
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let upper = name.to_ascii_uppercase();
        let pulse_label = upper.ends_with('P');
        let (op, wide) = ScalarOp::from_name(upper.strip_suffix('P').unwrap_or(&upper))
            .ok_or("Bu komut indeksli skaler kapsamına dahil değil")?;
        let count = op.source_count();
        if args.len() != count + 1 {
            return Err(format!("{name}: {} operand gerekli", count + 1));
        }
        let sources = args[..count]
            .iter()
            .map(|s| AddressedValue::parse(s, wide, false))
            .collect::<Result<Vec<_>, _>>()?;
        let dest = AddressedValue::parse(&args[count], wide, true)?;
        Ok(Self {
            op,
            wide,
            pulse_label,
            sources,
            dest,
        })
    }
    /// Returns the ordinary non-pulse action. The source VM owns edge/disabled
    /// qualification and must not resolve a skipped instruction.
    pub fn resolve(&self, memory: &Memory) -> Result<Action, String> {
        let sources = self
            .sources
            .iter()
            .map(|s| s.resolve(memory, false))
            .collect::<Result<Vec<_>, _>>()?;
        let dest = CalendarDestination::from_value(self.dest.resolve(memory, true)?)?;
        self.action_from_sources(sources, dest)
    }
    /// Actual source reads are ordered and fallible; native address-only
    /// preflight continues to use resolve() without consuming clock inputs.
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
        observed.extend(self.dest.modifier_devices());
        let dest = CalendarDestination::from_value(self.dest.resolve(memory, true)?)?;
        dest.devices(observed);
        self.action_from_sources(sources, dest)
    }
    fn action_from_sources(
        &self,
        sources: Vec<CalendarValue>,
        dest: CalendarDestination,
    ) -> Result<Action, String> {
        if self.wide {
            return Ok(Action::Wide(WideAction {
                op: self.op.wide_op(),
                sources: sources.into_iter().map(WideSource::Read).collect(),
                dest: WideDestination::Numeric(dest),
                count: 0,
            }));
        }
        let mut sources = sources.into_iter();
        Ok(match self.op {
            ScalarOp::Move => Action::Move(sources.next().unwrap(), dest),
            ScalarOp::Complement => Action::Complement(sources.next().unwrap(), dest),
            ScalarOp::Add => Action::Add(sources.next().unwrap(), sources.next().unwrap(), dest),
            ScalarOp::Sub => Action::Sub(sources.next().unwrap(), sources.next().unwrap(), dest),
            ScalarOp::Inc => Action::Unary(WordUnary::Inc, dest),
            ScalarOp::Dec => Action::Unary(WordUnary::Dec, dest),
            ScalarOp::Neg => Action::Unary(WordUnary::Neg, dest),
            op => Action::Logic(
                match op {
                    ScalarOp::And => WordLogic::And,
                    ScalarOp::Or => WordLogic::Or,
                    ScalarOp::Xor => WordLogic::Xor,
                    _ => unreachable!(),
                },
                sources.next().unwrap(),
                sources.next().unwrap(),
                dest,
            ),
        })
    }
    /// Literal base for source layout only. Execution/monitoring must use
    /// resolve() and never interpret this static label as the effective target.
    pub fn dest(&self) -> Device {
        self.dest.base().expect("validated writable scalar")
    }
    pub fn label(&self) -> String {
        let mut parts = vec![format!(
            "{}{}",
            self.op.name(self.wide),
            if self.pulse_label { "P" } else { "" }
        )];
        parts.extend(self.sources.iter().map(ToString::to_string));
        parts.push(self.dest.to_string());
        parts.join(" ")
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        for source in &self.sources {
            source.devices(out);
        }
        self.dest.devices(out);
        out.extend((1020..=1022).map(Memory::system_device));
        Memory::error_devices(out);
    }
    pub fn modifier_devices(&self) -> Vec<Device> {
        self.sources
            .iter()
            .chain(std::iter::once(&self.dest))
            .filter_map(|v| match v {
                AddressedValue::Modified { address, .. } => Some(address.index.device()),
                _ => None,
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    pub fn observed_devices(&self, memory: &Memory, out: &mut BTreeSet<Device>) {
        self.devices(out);
        for (binding, write) in self
            .sources
            .iter()
            .map(|v| (v, false))
            .chain(std::iter::once((&self.dest, true)))
        {
            if let Ok(value) = binding.resolve(memory, write) {
                value.devices(out);
            }
        }
    }
    pub fn display_value(&self, memory: &Memory) -> String {
        match self
            .dest
            .resolve(memory, true)
            .and_then(CalendarDestination::from_value)
        {
            Ok(dest) => dest.read(memory).to_string(),
            Err(error) => format!("İndeks: {error}"),
        }
    }
    pub fn native_words(&self) -> Result<Vec<u16>, String> {
        let mut header = self.op.wide_op().header();
        if !self.wide {
            header -= 0x100;
        }
        if self.pulse_label {
            header |= 0x10;
        }
        let mut words = vec![header];
        for operand in self.sources.iter().chain(std::iter::once(&self.dest)) {
            words.extend(operand.native_words()?);
        }
        Ok(words)
    }
}
