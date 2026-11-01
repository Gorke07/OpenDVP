//! SFC source structure checks, independent of the execution-time step bitmap.
//! A definition and an AND-join reference are different uses of an STL token.
//! These are source diagnostics, never fabricated PLC operation-error flags.
use crate::execution_plan::{ExecutionPlan, PlanNode, PlanOp};
use crate::plc::{Action, Area, Device, Program};
use crate::program_control::ControlOp;
use std::collections::BTreeSet;

fn error(node: &PlanNode, message: &str) -> String {
    let line = match node.op {
        PlanOp::Control { line, .. } => line,
        _ => usize::from(node.pc) + 1,
    };
    format!("Satır {line}: {message}")
}
fn ist_count(action: &Action) -> usize {
    match action {
        Action::Sfc(_) => 1,
        Action::Pulse(inner) => ist_count(inner),
        _ => 0,
    }
}

/// Validate the already lowered/linked WORD plan without evaluating its paths.
/// CJ bypass is not implicit RET, and inactive branches are still structure.
pub fn validate(program: &Program, plan: &ExecutionPlan) -> Result<(), String> {
    let count: usize = program
        .networks
        .iter()
        .flat_map(|n| &n.actions)
        .map(ist_count)
        .sum();
    if count > 1 {
        return Err("IST programda yalnız bir kez kullanılabilir".into());
    }
    let last_fend = plan
        .nodes
        .iter()
        .filter_map(|node| {
            matches!(
                node.op,
                PlanOp::Control {
                    op: ControlOp::MainEnd,
                    ..
                }
            )
            .then_some(node.pc)
        })
        .max();
    let mut definitions = BTreeSet::<Device>::new();
    let mut region_start: Option<&PlanNode> = None;
    let mut loops = 0_usize;
    let mut index = 0;
    while index < plan.nodes.len() {
        let node = &plan.nodes[index];
        match &node.op {
            PlanOp::Control {
                op: ControlOp::Step { .. },
                ..
            } => {
                if loops != 0 {
                    return Err(error(node, "C405: FOR içinde STL kullanılamaz"));
                }
                if last_fend.is_some_and(|pc| node.pc > pc) {
                    return Err(error(node, "C409: alt programda STL kullanılamaz"));
                }
                let start = index;
                let mut group = BTreeSet::new();
                let mut previous_fallthrough = node.pc;
                while let Some(current) = plan.nodes.get(index) {
                    let PlanOp::Control {
                        op: ControlOp::Step { step },
                        ..
                    } = &current.op
                    else {
                        break;
                    };
                    if current.pc != previous_fallthrough {
                        break;
                    }
                    if step.area != Area::S || step.index > 1023 {
                        return Err(error(current, "STL adımı S0..S1023 olmalı"));
                    }
                    if !group.insert(*step) {
                        return Err(error(
                            current,
                            "Aynı STL adımı bir birleşme başlık grubunda yinelenemez",
                        ));
                    }
                    previous_fallthrough = current.fallthrough;
                    index += 1;
                }
                // Repeated headers after earlier output blocks identify the
                // documented consecutive-header AND join (PDF889). A lone
                // repeated header is a second ordinary definition, not a join.
                let references = group.iter().filter(|s| definitions.contains(*s)).count();
                if references != 0 && group.len() == 1 {
                    return Err(error(node, "STL adımı sıradan tanım olarak yineleniyor"));
                }
                if group.len() > 9 {
                    // C407's nine-consecutive declaration limit is distinct
                    // from the documented combined16-terminal convergence.
                    // Only already-defined join references qualify for that
                    // larger graph; do not flatten every graph into a9-step cap.
                    if references != group.len() || group.len() > 16 {
                        return Err(error(
                            node,
                            "C407: sıradan STL başlıkları en fazla 9 ardışık; belgeli birleşme en fazla 16 önceki adımı referanslar",
                        ));
                    }
                }
                definitions.extend(group);
                if region_start.is_none() {
                    region_start = Some(&plan.nodes[start]);
                }
                continue;
            }
            PlanOp::Control { op, .. } => match op {
                ControlOp::StepReturn => {
                    if loops != 0 {
                        return Err(error(node, "C405: FOR içinde RET kullanılamaz"));
                    }
                    if last_fend.is_some_and(|pc| node.pc > pc) {
                        return Err(error(node, "C409: alt programda RET kullanılamaz"));
                    }
                    // Standalone RET remains the established installed reset
                    // seam. It is not CALL-return and needs no invented STL.
                    region_start = None;
                }
                ControlOp::MasterEnter { .. }
                | ControlOp::MasterExit { .. }
                | ControlOp::Label(_)
                    if region_start.is_some() =>
                {
                    return Err(error(
                        node,
                        "C408: STL bölgesinde MC/MCR veya P etiketi kullanılamaz",
                    ));
                }
                ControlOp::MainEnd | ControlOp::SubReturn if region_start.is_some() => {
                    return Err(error(
                        node,
                        "STL bölgesi FEND/SRET öncesinde RET ile kapatılmalı",
                    ));
                }
                ControlOp::For { .. } => loops += 1,
                ControlOp::Next => {
                    loops = loops.saturating_sub(1);
                }
                _ => {}
            },
            PlanOp::End if region_start.is_some() => {
                return Err(error(
                    region_start.unwrap(),
                    "STL bölgesi için END öncesinde RET eksik",
                ));
            }
            _ => {}
        }
        index += 1;
    }
    if let Some(start) = region_start {
        return Err(error(start, "STL bölgesi için RET eksik"));
    }
    // Initial S0..9 is the documented sequence convention, not a raw address
    // restriction: explicitly initialized S20..1023 bodies are runnable.
    // Direct shared Y outputs across steps are expressly permitted. Timer
    // reuse needs graph adjacency classification; lexical neighbours alone do
    // not establish forbidden adjacency, so no guessed rejection occurs here.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plc::{Expr, Network};
    fn step(index: u16) -> ControlOp {
        ControlOp::Step {
            step: Device {
                area: Area::S,
                index,
            },
        }
    }
    fn fixture(ops: Vec<PlanOp>, actions: Vec<Action>) -> (Program, ExecutionPlan) {
        let mut plan = ExecutionPlan::default();
        for (n, op) in ops.into_iter().enumerate() {
            plan.push(n as u16, n as u16 + 1, op).unwrap();
        }
        let program = Program {
            source_is_st: false,
            execution_plan: ExecutionPlan::default(),
            native_program_words: 0,
            native_action_pc: Vec::new(),
            flow: Vec::new(),
            networks: vec![Network {
                line: 1,
                end_line: 1,
                comment: String::new(),
                condition: Expr::Contact(
                    Device {
                        area: Area::X,
                        index: 0,
                    },
                    false,
                ),
                actions,
            }],
            devices: BTreeSet::new(),
            instructions: 0,
        };
        (program, plan)
    }
    fn control(op: ControlOp) -> PlanOp {
        PlanOp::Control { op, line: 1 }
    }
    fn body() -> PlanOp {
        PlanOp::Inert
    }
    fn valid(ops: Vec<PlanOp>) -> Result<(), String> {
        let (program, plan) = fixture(ops, Vec::new());
        validate(&program, &plan)
    }
    #[test]
    fn independent_high_step_initialization_and_standalone_ret_are_valid() {
        for s in [0, 9, 20, 895, 896, 911, 912, 1023] {
            assert!(
                valid(vec![
                    control(step(s)),
                    body(),
                    control(ControlOp::StepReturn),
                    PlanOp::End
                ])
                .is_ok()
            );
        }
        assert!(valid(vec![control(ControlOp::StepReturn), PlanOp::End]).is_ok());
    }
    #[test]
    fn missing_ret_and_ordinary_duplicate_definitions_are_rejected() {
        assert!(valid(vec![control(step(0)), body(), PlanOp::End]).is_err());
        assert!(
            valid(vec![
                control(step(20)),
                body(),
                control(step(20)),
                body(),
                control(ControlOp::StepReturn),
                PlanOp::End
            ])
            .is_err()
        );
        assert!(
            valid(vec![
                control(step(20)),
                control(step(20)),
                control(ControlOp::StepReturn),
                PlanOp::End
            ])
            .is_err()
        );
    }
    #[test]
    fn genuine_consecutive_join_can_reference_previous_ordinary_definitions() {
        assert!(
            valid(vec![
                control(step(40)),
                body(),
                control(step(41)),
                body(),
                control(step(40)),
                control(step(41)),
                body(),
                control(step(80)),
                body(),
                control(ControlOp::StepReturn),
                PlanOp::End
            ])
            .is_ok()
        );
    }
    #[test]
    fn nine_contiguous_new_headers_are_safe_but_ten_is_c407() {
        for (n, expected) in [(8, true), (9, true), (10, false)] {
            let mut ops = (0..n).map(|n| control(step(n))).collect::<Vec<_>>();
            ops.extend([body(), control(ControlOp::StepReturn), PlanOp::End]);
            assert_eq!(valid(ops).is_ok(), expected);
        }
    }
    #[test]
    fn sixteen_terminal_graph_is_not_flattened_into_a_nine_header_cap() {
        let mut ops = Vec::new();
        for s in 40..56 {
            ops.extend([control(step(s)), body()]);
        }
        for s in 40..56 {
            ops.push(control(step(s)));
        }
        ops.extend([
            body(),
            control(step(80)),
            body(),
            control(ControlOp::StepReturn),
            PlanOp::End,
        ]);
        assert!(valid(ops).is_ok());
    }
    #[test]
    fn forbidden_region_tokens_and_for_or_subroutine_step_regions_are_rejected() {
        for op in [
            ControlOp::Label(10),
            ControlOp::MasterEnter { level: 0 },
            ControlOp::MasterExit { level: 0 },
        ] {
            assert!(
                valid(vec![
                    control(step(0)),
                    control(op),
                    control(ControlOp::StepReturn),
                    PlanOp::End
                ])
                .is_err()
            );
        }
        let count = crate::calendar_operand::CalendarValue::parse("K2", false).unwrap();
        assert!(
            valid(vec![
                control(ControlOp::For { count }),
                control(step(0)),
                control(ControlOp::StepReturn),
                control(ControlOp::Next),
                PlanOp::End
            ])
            .is_err()
        );
        assert!(
            valid(vec![
                control(ControlOp::MainEnd),
                control(ControlOp::Label(10)),
                control(step(0)),
                control(ControlOp::StepReturn),
                control(ControlOp::SubReturn),
                PlanOp::End
            ])
            .is_err()
        );
    }
    #[test]
    fn ist_occurrence_is_syntactic_even_when_conditions_would_be_false() {
        let ist = Action::Sfc(crate::sfc::SfcAction::Ist {
            source: Device {
                area: Area::X,
                index: 16,
            },
            first: Device {
                area: Area::S,
                index: 20,
            },
            last: Device {
                area: Area::S,
                index: 80,
            },
        });
        let (program, plan) = fixture(vec![PlanOp::End], vec![ist.clone(), ist]);
        assert!(validate(&program, &plan).is_err());
    }
    #[test]
    fn shared_direct_coils_are_not_rejected_as_duplicate_step_ownership() {
        let coil = Action::Out(Device {
            area: Area::Y,
            index: 0,
        });
        let (program, plan) = fixture(
            vec![
                control(step(20)),
                body(),
                control(step(40)),
                body(),
                control(ControlOp::StepReturn),
                PlanOp::End,
            ],
            vec![coil.clone(), coil],
        );
        assert!(validate(&program, &plan).is_ok());
    }
}
