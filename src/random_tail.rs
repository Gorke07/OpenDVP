//! Scoped DRAND operand-tail compatibility. Original bytes are retained; only
//! compiler-owned WORDs at header+7..+12 may become these typed instructions.
use crate::execution_plan::PlanOp;
use crate::plc::{Action, Program};
use crate::program_control::ControlState;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperandTailOwner {
    Drand,
    CalendarTime,
}

/// A paused owned native header is qualified against independent local
/// control state. It is observed for rejection, never applied to local flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RandomGateState {
    pub pc: u16,
    pub logic_word: u16,
    pub master_on: bool,
    pub pulse_latched: bool,
}
impl RandomGateState {
    pub(crate) fn body_active(self, pulse: bool) -> bool {
        self.logic_word & 1 != 0 && self.master_on && (!pulse || !self.pulse_latched)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RandomTailProfile {
    /// Standalone policy: a disabled numerical action has no operand effects.
    /// This is not a claim about physical PLC instruction stepping.
    #[default]
    Documented,
    /// Exact current COMMGR ES2/SS2 dispatcher, selected by executable digest.
    CurrentCommgr,
    /// Older verified WPL images: retain only established 0x8*/0x9* tails.
    InstalledLegacyInertOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperandTailControl {
    Np,
    Pn,
    AndBlock,
    OrBlock,
    Mps,
    Mrd,
    Mpp,
    Invert,
}

pub fn decode_drand_tail(words: &[u16]) -> Result<[Option<OperandTailControl>; 6], String> {
    if words.len() != 13 || !matches!(words[0], 0x5501 | 0x5511) {
        return Err("DRAND tail requires its original thirteen-WORD instruction".into());
    }
    let mut decoded = [None; 6];
    for (slot, word) in decoded.iter_mut().zip(&words[7..]) {
        *slot = match word & 0xf0 {
            0x80 | 0x90 | 0xe0 => None,
            0xf0 => match word >> 8 {
                0xf6 => Some(OperandTailControl::Np),
                0xf7 => Some(OperandTailControl::Pn),
                0xf8 => Some(OperandTailControl::AndBlock),
                0xf9 => Some(OperandTailControl::OrBlock),
                0xfa => Some(OperandTailControl::Mps),
                0xfb => Some(OperandTailControl::Mrd),
                0xfc => Some(OperandTailControl::Mpp),
                0xfd => Some(OperandTailControl::Invert),
                _ => None,
            },
            _ => return Err(format!("Unqualified DRAND operand-tail WORD H{word:04X}")),
        };
    }
    Ok(decoded)
}

/// These controls operate on the full logic WORD even with master/output off.
/// NP/PN use their actual instruction PC, independently of parser edge IDs.
pub fn execute_control(control: &mut ControlState, op: OperandTailControl, pc: u16) {
    match op {
        OperandTailControl::Np | OperandTailControl::Pn => {
            let now = control.logic_word & 1 != 0;
            let old = control.pulse_history.insert(pc, now).unwrap_or(false);
            let bit = if op == OperandTailControl::Np {
                now && !old
            } else {
                !now && old
            };
            control.logic_word = (control.logic_word & !1) | u16::from(bit);
        }
        OperandTailControl::AndBlock => control.and_block(),
        OperandTailControl::OrBlock => control.or_block(),
        OperandTailControl::Mps => control.mps(),
        OperandTailControl::Mrd => control.mrd(),
        OperandTailControl::Mpp => control.mpp(),
        OperandTailControl::Invert => control.invert(),
    }
}

pub(crate) fn is_drand(action: &Action) -> bool {
    match action {
        Action::Pulse(inner) => is_drand(inner),
        Action::Random(a) => a.wide(),
        Action::IndexedRandom(a) => a.wide(),
        _ => false,
    }
}

fn instruction_words(action: &Action) -> Result<Option<Vec<u16>>, String> {
    match action {
        Action::Pulse(inner) => {
            let mut words = instruction_words(inner)?;
            if let Some(words) = &mut words {
                words[0] |= 0x10;
            }
            Ok(words)
        }
        Action::Random(a) if a.wide() => a.native_words().map(Some),
        Action::IndexedRandom(a) if a.wide() => a.native_words().map(Some),
        _ => Ok(None),
    }
}

pub(crate) fn validate_program_profile(
    program: &Program,
    profile: RandomTailProfile,
) -> Result<(), String> {
    for action in program.networks.iter().flat_map(|n| &n.actions) {
        if let Some(words) = instruction_words(action)? {
            decode_drand_tail(&words)?;
            if profile == RandomTailProfile::InstalledLegacyInertOnly
                && words[7..]
                    .iter()
                    .any(|word| !matches!(word & 0xf0, 0x80 | 0x90))
            {
                return Err("DRAND indexed operand-tail dispatch is unverified for this older simulator image".into());
            }
        }
    }
    Ok(())
}

/// Verify ownership/layout only. Conditional native shifts are not a source
/// MPS stack and must not acquire invented static push/pop balance rules.
pub(crate) fn validate_layout(
    program: &Program,
    encoded: &crate::es2_encoder::EncodedProgram,
) -> Result<(), String> {
    let plan = &encoded.execution_plan;
    let mut owned_controls = BTreeSet::new();
    let mut owners = BTreeMap::new();
    for node in &plan.nodes {
        let PlanOp::Action {
            network, action, ..
        } = node.op
        else {
            continue;
        };
        let action = program
            .networks
            .get(network)
            .and_then(|n| n.actions.get(action))
            .ok_or("Random tail source action is missing")?;
        let (words, owner, tail_start, decoded) = if let Some(words) = instruction_words(action)? {
            let decoded = decode_drand_tail(&words)?.to_vec();
            (words, OperandTailOwner::Drand, 7, decoded)
        } else if let Some(words) = crate::calendar_tail::instruction_words(action)? {
            let decoded = crate::calendar_tail::decode_time_tail(&words)?.to_vec();
            (words, OperandTailOwner::CalendarTime, 3, decoded)
        } else {
            continue;
        };
        owners.insert(node.pc, owner);
        let start = usize::from(node.pc);
        if encoded.words.get(start..start + words.len()) != Some(words.as_slice())
            || node.fallthrough != node.pc + tail_start
            || !plan.by_pc.contains_key(&(node.pc + words.len() as u16))
        {
            return Err(
                "Original source bytes or header boundaries do not match their tail owner".into(),
            );
        }
        for (offset, op) in decoded.into_iter().enumerate() {
            let pc = node.pc + tail_start + offset as u16;
            let tail = plan
                .by_pc
                .get(&pc)
                .and_then(|i| plan.nodes.get(*i))
                .ok_or("DRAND operand-tail boundary is missing")?;
            if tail.fallthrough != pc + 1 {
                return Err("DRAND operand-tail must occupy one WORD".into());
            }
            let expected = match op {
                Some(op) => {
                    owned_controls.insert(pc);
                    PlanOp::OperandTailControl {
                        header: node.pc,
                        op,
                    }
                }
                None => PlanOp::Inert,
            };
            if tail.op != expected {
                return Err("DRAND typed tail disagrees with its original WORD".into());
            }
        }
    }
    if owners != plan.operand_tail_owners {
        return Err("Operand-tail owner metadata disagrees with original source actions".into());
    }
    if plan.nodes.iter().any(|n| {
        matches!(n.op, PlanOp::OperandTailControl { .. }) && !owned_controls.contains(&n.pc)
    }) {
        return Err("Operand-tail control has no verified source owner".into());
    }
    Ok(())
}
