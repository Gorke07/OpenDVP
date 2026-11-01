//! Typed source execution sites emitted alongside the actual native WORD layout.
//! Runtime destinations come from independent local state, never observed PCs.
use crate::plc::Expr;
use crate::program_control::{
    ControlLinks, ControlOp, ControlProfile, ControlSite, ControlStatement,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicMode {
    Load,
    And,
    Or,
}
#[derive(Debug, Clone, PartialEq)]
pub enum PlanOp {
    Predicate {
        mode: LogicMode,
        expr: Expr,
    },
    Edge {
        rising: bool,
        id: usize,
    },
    Invert,
    AndBlock,
    OrBlock,
    /// A control decoded only from original compiler-owned operand WORDs.
    OperandTailControl {
        header: u16,
        op: crate::random_tail::OperandTailControl,
    },
    Capture(usize),
    Restore(bool),
    Action {
        network: usize,
        action: usize,
        ordinal: usize,
    },
    Control {
        op: ControlOp,
        line: usize,
    },
    End,
    /// Encoder-declared balanced padding, not an arbitrary decoded opcode.
    Inert,
}
#[derive(Debug, Clone, PartialEq)]
pub struct PlanNode {
    pub pc: u16,
    pub fallthrough: u16,
    pub op: PlanOp,
    /// Read-only Ladder projection at zero-code continuation sites, sampled
    /// before the following actual instruction changes the accumulator.
    pub continuations: Vec<usize>,
    /// Proven native predicate shortcut over encoder-owned padding. Chosen
    /// from the independent local accumulator, never from an observed PC.
    pub shortcut_pc: Option<u16>,
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExecutionPlan {
    pub operand_tail_owners: BTreeMap<u16, crate::random_tail::OperandTailOwner>,
    pub nodes: Vec<PlanNode>,
    pub by_pc: BTreeMap<u16, usize>,
    pub links: ControlLinks,
    pub has_control: bool,
}
impl ExecutionPlan {
    pub fn interrupt_region(&self, pc: u16) -> Option<u16> {
        self.links
            .interrupt_regions
            .iter()
            .find_map(|(&start, &end)| (start <= pc && pc <= end).then_some(start))
    }
    pub fn push(&mut self, pc: u16, fallthrough: u16, op: PlanOp) -> Result<(), String> {
        if self.by_pc.insert(pc, self.nodes.len()).is_some() {
            return Err("Yinelenen yürütme PC'si".into());
        }
        self.has_control |= matches!(op, PlanOp::Control { .. });
        self.nodes.push(PlanNode {
            pc,
            fallthrough,
            op,
            continuations: Vec::new(),
            shortcut_pc: None,
        });
        Ok(())
    }
    pub fn link(&mut self) -> Result<(), String> {
        let end = self.nodes.last().ok_or("Yürütme planı boş")?;
        if !matches!(end.op, PlanOp::End) {
            return Err("Yürütme planında son END eksik".into());
        }
        for node in &self.nodes {
            if let Some(target) = node.shortcut_pc
                && (!matches!(
                    node.op,
                    PlanOp::Predicate {
                        mode: LogicMode::Or,
                        ..
                    }
                ) || !self.by_pc.contains_key(&target))
            {
                return Err("Kontak kısa geçişi gerçek komut sınırına gitmiyor".into());
            }
        }
        let statements = self
            .nodes
            .iter()
            .filter_map(|node| {
                if let PlanOp::Control { op, line } = &node.op {
                    Some(ControlStatement {
                        op: op.clone(),
                        site: ControlSite::new(node.pc, node.fallthrough, *line),
                    })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        self.links = ControlLinks::link(&statements, end.pc, ControlProfile::Documented)?;
        for target in self.links.interrupts.values() {
            if !self.by_pc.contains_key(target) {
                return Err("Kesme hedefi gerçek komut sınırında değil".into());
            }
        }
        for target in self.links.labels.iter().flatten() {
            if !self.by_pc.contains_key(target) {
                return Err("Etiket hedefi gerçek komut sınırında değil".into());
            }
        }
        for pair in self.links.loops.values() {
            for target in [pair.for_pc, pair.body_pc, pair.next_pc, pair.after_pc] {
                if !self.by_pc.contains_key(&target) {
                    return Err("FOR/NEXT hedefi gerçek komut sınırında değil".into());
                }
            }
        }
        // A direct CJ may bypass an entire loop, but cannot enter an active
        // body's interior without ever visiting its corresponding FOR.
        for statement in &statements {
            if let ControlOp::Jump { pointer, .. } | ControlOp::Call { pointer, .. } = &statement.op
                && pointer.index.is_none()
                && let Some(target) = self.links.labels[usize::from(pointer.base)]
            {
                let source_region = self.interrupt_region(statement.site.pc);
                let target_region = self.interrupt_region(target);
                if matches!(statement.op, ControlOp::Call { .. }) {
                    if target_region.is_some() {
                        return Err(
                            "CALL kesme gövdesine giremez; olay yönlendiricisi gerekli".into()
                        );
                    }
                } else if source_region != target_region {
                    return Err("CJ kesme gövdesinin sınırını aşamaz; IRET kullanın".into());
                }
            }
            if let ControlOp::Jump { pointer, .. } = &statement.op
                && pointer.index.is_none()
                && let Some(target) = self.links.labels[usize::from(pointer.base)]
            {
                for pair in self
                    .links
                    .loops
                    .values()
                    .filter(|p| p.for_pc == statement.site.pc || p.for_pc < p.next_pc)
                {
                    let source_inside =
                        statement.site.pc >= pair.body_pc && statement.site.pc <= pair.next_pc;
                    let target_inside = target >= pair.body_pc && target <= pair.next_pc;
                    if target_inside && !source_inside {
                        return Err(format!(
                            "Satır {}: CJ, FOR çalıştırılmadan döngü gövdesine giremez",
                            statement.site.line
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}
