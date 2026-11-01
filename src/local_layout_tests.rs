//! Local layout is metadata, not a packet of guessed native opcodes.
use super::*;
use crate::execution_plan::PlanOp;
use crate::plc::{Area, Device, compile_il};
fn empty_emitter(local_layout: bool) -> Emitter {
    Emitter {
        output: EncodedProgram {
            execution_plan: Default::default(),
            preflight: Default::default(),
            padded_error_skip: Default::default(),
            padded_short_circuit: Default::default(),
            action_pc: Vec::new(),
            words: Vec::new(),
            next_pc: Vec::new(),
        },
        unencoded_ranges: Default::default(),
        local_layout,
        transform_profile: crate::data_transform::NativeTransformProfile::InstalledLegacy,
        pending: Some(PlanOp::Action {
            network: 0,
            action: 0,
            ordinal: 0,
        }),
        continuations: Vec::new(),
    }
}
fn reset_program() -> Program {
    // Exercise the typed-action codec independently of the parser route.
    let mut p = compile_il("LD X0\nMOV K1 D10\nMOV K7 D200\nEND").unwrap();
    let action = crate::canopen::CanopenAction::reset_unencoded(
        crate::calendar_operand::CalendarValue::parse("K2", false).unwrap(),
        Device {
            area: Area::D,
            index: 100,
        },
        Device {
            area: Area::M,
            index: 0,
        },
        Device {
            area: Area::M,
            index: 1,
        },
    )
    .unwrap();
    p.networks[0].actions[0] = Action::Canopen(action);
    p
}
#[test]
fn rstd_encodes_nine_words_as_one_typed_action_and_preserves_later_headers() {
    let p = reset_program();
    let local = local_execution_layout(&p).unwrap();
    assert_eq!(local.action_pc, [1, 10]);
    assert_eq!(local.word_count, 16);
    assert!(local.unencoded_ranges.is_empty());
    let nodes: Vec<_> = local
        .execution_plan
        .nodes
        .iter()
        .filter(|n| n.pc >= 1 && n.pc < 10)
        .collect();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].fallthrough, 10);
    assert!(matches!(
        nodes[0].op,
        PlanOp::Action {
            network: 0,
            action: 0,
            ordinal: 0
        }
    ));
    let expected = [
        0xbe02, 0x0280, 0x0080, 0xc886, 0x0086, 0x0084, 0x0884, 0x0184, 0x0884,
    ];
    for (offset, word) in expected.into_iter().enumerate() {
        assert_eq!(local.known_word_at(1 + offset as u16), Some(word));
    }
    assert_eq!(local.known_word_at(0), Some(0x0024));
    assert_eq!(local.known_word_at(10), Some(0x2800));
    assert_eq!(local.known_word_at(15), Some(0x0f00));
    assert_eq!(local.known_word_at(16), None);
    assert!(encode(&p).is_err());
}
#[test]
fn local_end_keeps_one_real_node_after_genuine_rstd_interval() {
    let local = local_execution_layout(&reset_program()).unwrap();
    let end = local.execution_plan.nodes.last().unwrap();
    assert!(matches!(end.op, PlanOp::End));
    assert_eq!(end.pc, local.word_count - 1);
    assert_eq!(end.fallthrough, local.word_count);
    assert_eq!(
        local.execution_plan.by_pc.get(&end.pc),
        Some(&(local.execution_plan.nodes.len() - 1))
    );
    assert!(!local.execution_plan.by_pc.contains_key(&9));
}
#[test]
fn known_program_words_and_layout_remain_the_verified_native_fixture() {
    let p = compile_il("LD X0\nOUT Y0\nEND").unwrap();
    let native = encode(&p).unwrap();
    let local = local_execution_layout(&p).unwrap();
    assert_eq!(native.words, [0x0024, 0x00c5, 0x0f00]);
    assert_eq!(native.next_pc, [1, 2, 0]);
    assert!(local.unencoded_ranges.is_empty());
    assert_eq!(usize::from(local.word_count), native.words.len());
    assert_eq!(local.action_pc, native.action_pc);
    assert_eq!(local.execution_plan, native.execution_plan);
    for (pc, word) in native.words.iter().copied().enumerate() {
        assert_eq!(local.known_word_at(pc as u16), Some(word));
    }
}
#[test]
fn supported_control_and_pulse_layouts_are_byte_for_byte_unchanged() {
    for source in [
        "LD X0\nMPS\nOUT Y0\nMRD\nOUT Y1\nMPP\nOUT Y2\nEND",
        "LD X0\nMOVP K7 D100\nADD K1 D100 D101\nEND",
        "LD X0\nFOR K2\nLD X1\nOUT Y0\nNEXT\nEND",
    ] {
        let p = compile_il(source).unwrap();
        let native = encode(&p).unwrap();
        let local = local_execution_layout(&p).unwrap();
        assert!(local.unencoded_ranges.is_empty());
        assert_eq!(local.execution_plan, native.execution_plan);
        assert_eq!(local.action_pc, native.action_pc);
        assert_eq!(usize::from(local.word_count), native.words.len());
        for (pc, word) in native.words.iter().copied().enumerate() {
            assert_eq!(local.known_word_at(pc as u16), Some(word));
        }
    }
}
#[test]
fn zero_length_reservation_is_rejected_without_changing_the_plan() {
    let mut e = empty_emitter(true);
    assert!(e.reserve_local_instruction(0, "bad width").is_err());
    assert!(e.output.words.is_empty());
    assert!(e.output.execution_plan.nodes.is_empty());
    assert!(e.unencoded_ranges.is_empty());
    assert!(e.pending.is_some());
    assert!(e.instruction(&[]).is_err());
}
#[test]
fn overflow_reservation_is_rejected_before_allocating_a_plan_node() {
    let mut e = empty_emitter(true);
    assert!(e.reserve_local_instruction(u16::MAX, "oversize").is_err());
    assert!(e.output.words.is_empty());
    assert!(e.output.execution_plan.nodes.is_empty());
    assert!(e.unencoded_ranges.is_empty());
}
#[test]
fn exact_program_boundary_leaves_space_for_end_and_rejects_an_extra_word() {
    let mut e = empty_emitter(true);
    e.reserve_local_instruction((MAX_PROGRAM_WORDS - 1) as u16, "boundary")
        .unwrap();
    e.pending = Some(PlanOp::End);
    e.instruction(&[0x0f00]).unwrap();
    assert_eq!(e.output.words.len(), MAX_PROGRAM_WORDS);
    assert_eq!(e.output.execution_plan.nodes.len(), 2);
    assert!(e.instruction(&[0xfdf0]).is_err());
    assert_eq!(e.output.words.len(), MAX_PROGRAM_WORDS);
}
#[test]
fn native_emitter_never_accepts_an_unencoded_reservation() {
    let mut e = empty_emitter(false);
    assert!(
        e.reserve_local_instruction(9, "unknown native instruction")
            .is_err()
    );
    assert!(e.output.words.is_empty());
    assert!(e.unencoded_ranges.is_empty());
    assert!(e.output.execution_plan.nodes.is_empty());
}
#[test]
fn reserving_without_a_typed_action_cannot_invent_an_inert_fallback() {
    let mut e = empty_emitter(true);
    e.pending = None;
    assert!(e.reserve_local_instruction(9, "no action").is_err());
    e.pending = Some(PlanOp::End);
    assert!(e.reserve_local_instruction(9, "not an action").is_err());
    assert!(e.output.execution_plan.nodes.is_empty());
}
#[test]
fn existing_local_literals_hide_placeholder_words_including_pulse_form() {
    for source in [
        "LD X0\n$MOV \"AbC\" D0\nMOV K7 D100\nEND",
        "LD X0\n$MOVP \"AbC\" D0\nMOV K7 D100\nEND",
        "LD X0\nASC \"AbCdEfGh\" D0\nMOV K7 D100\nEND",
    ] {
        let p = compile_il(source).unwrap();
        let local = local_execution_layout(&p).unwrap();
        assert_eq!(local.unencoded_ranges.len(), 1);
        let (&start, range) = local.unencoded_ranges.first_key_value().unwrap();
        for pc in start..start + range.word_count {
            assert_eq!(local.known_word_at(pc), None);
        }
        let following = local.action_pc[1];
        assert_eq!(following, start + range.word_count);
        assert_eq!(local.known_word_at(following), Some(0x2800));
        assert!(encode(&p).is_err());
    }
}
