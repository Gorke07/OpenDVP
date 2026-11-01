//! Independent DRAND tail tests drafted for crate-level test-module wiring.
//! Literal expectations come from the current dispatcher/control contract;
//! these local tests make no claim about native random-stream equality.
use crate::execution_plan::PlanOp;
use crate::interrupt::InterruptId;
use crate::plc::{Device, Program, ScanStatus, Simulator, compile_il};
use crate::program_control::{ControlSite, ControlState};
use crate::random_tail::{
    OperandTailControl as Tail, RandomTailProfile, decode_drand_tail, execute_control,
};

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}

fn instruction(target_first: u16, target_second: u16) -> [u16; 13] {
    [
        0x5501,
        0x0180,
        0x0080,
        0x0080,
        0x0080,
        0x0180,
        0x0080,
        0x0080,
        0x0080,
        target_first,
        target_second,
        0x0080,
        0x0080,
    ]
}

fn program(instruction: &str) -> Program {
    // The last indexed action also makes an ordinary DRAND a real VM program.
    let p = compile_il(&format!(
        "LD X0\n{instruction}\nMOV K7 D300\nLD X1\nSWAP D400E0\nEND"
    ))
    .unwrap();
    assert!(p.requires_vm());
    p
}

fn action_pc(p: &Program, label: &str) -> u16 {
    p.execution_plan
        .nodes
        .iter()
        .find_map(|n| match &n.op {
            PlanOp::Action {
                network, action, ..
            } if p.networks[*network].actions[*action].label() == label => Some(n.pc),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing action {label}"))
}

fn until(sim: &mut Simulator, p: &Program, pc: u16) {
    for _ in 0..500 {
        if sim.current_plan_pc() == Some(pc) {
            return;
        }
        assert!(sim.current_plan_pc().is_some(), "missed PC {pc}");
        sim.execute_plan_step(p).unwrap();
    }
    panic!("did not reach PC {pc}");
}

fn finish(sim: &mut Simulator, p: &Program) -> Vec<(u16, u16)> {
    let mut result = Vec::new();
    while sim.current_plan_pc().is_some() {
        let step = sim.execute_plan_step(p).unwrap();
        result.push((step.pc, step.next_pc));
        assert!(result.len() < 500);
    }
    assert_eq!(sim.last_scan, ScanStatus::Complete);
    assert!(sim.memory.execution_limitations.is_empty());
    result
}

fn current() -> Simulator {
    let mut sim = Simulator::default();
    sim.memory.random_tail_profile = RandomTailProfile::CurrentCommgr;
    sim
}

fn at_header(sim: &mut Simulator, p: &Program, header: u16) {
    sim.begin_plan_scan(p, 0, false).unwrap();
    until(sim, p, header);
}

#[test]
fn random_tail_decoder_requires_whole_original_instruction() {
    let original = instruction(0xfaf4, 0x0882);
    assert_eq!(
        decode_drand_tail(&original).unwrap(),
        [None, None, Some(Tail::Mps), None, None, None]
    );
    let mut pulse = original;
    pulse[0] = 0x5511;
    assert_eq!(decode_drand_tail(&pulse), decode_drand_tail(&original));
    assert!(decode_drand_tail(&original[..12]).is_err());
    let mut long = original.to_vec();
    long.push(0x0080);
    assert!(decode_drand_tail(&long).is_err());
    for header in [0x5401, 0x5411, 0x5521, 0x5500] {
        let mut bad = original;
        bad[0] = header;
        assert!(decode_drand_tail(&bad).is_err(), "{header:04x}");
    }
    for at in 7..13 {
        let mut bad = original;
        bad[at] = 0x0030;
        assert!(decode_drand_tail(&bad).is_err(), "tail WORD {at}");
    }
}

#[test]
fn random_tail_decoder_literal_controls_and_safe_payloads() {
    let controls = [
        Tail::Np,
        Tail::Pn,
        Tail::AndBlock,
        Tail::OrBlock,
        Tail::Mps,
        Tail::Mrd,
        Tail::Mpp,
        Tail::Invert,
    ];
    for (high, expected) in (0xf6u16..=0xfd).zip(controls) {
        for tag in [0xf4, 0xf6] {
            let words = instruction((high << 8) | tag, 0x0882);
            assert_eq!(decode_drand_tail(&words).unwrap()[2], Some(expected));
        }
        for tag in [0xe4, 0xe6] {
            let words = instruction((high << 8) | tag, 0x0882);
            assert_eq!(decode_drand_tail(&words).unwrap(), [None; 6]);
        }
    }
    for first in [0xf4f6, 0xf5f4, 0xfef6, 0xfff4, 0x0086, 0x0096] {
        assert_eq!(
            decode_drand_tail(&instruction(first, 0x0882)).unwrap(),
            [None; 6]
        );
    }
}

#[test]
fn random_tail_decoder_all_sixteen_modifier_banks_keep_original_payload() {
    for bank in 0u16..8 {
        // D125E/Fn: bank bits do not alter the FA payload in WORD one.
        let d_kind = ((bank & 1) << 15) | (0x86 + (bank >> 1));
        // K1M246E/Fn: bank lives in WORD two, payload F6 remains literal.
        let group_kind = (bank << 13) | 0x0882;
        for (tag, d_expected, group_expected) in
            [(0xe0, None, None), (0xf0, Some(Tail::Mps), Some(Tail::Np))]
        {
            assert_eq!(
                decode_drand_tail(&instruction(0xfa00 | tag | 6, d_kind)).unwrap(),
                [None, None, d_expected, None, None, None]
            );
            assert_eq!(
                decode_drand_tail(&instruction(0xf600 | tag | 4, group_kind)).unwrap(),
                [None, None, group_expected, None, None, None]
            );
        }
    }
}

#[test]
fn random_tail_kernel_changes_full_word_without_master_or_output_gating() {
    for (initial, op, expected) in [
        (0xa55a, Tail::AndBlock, 0x52ac),
        (0xa55a, Tail::OrBlock, 0x52ad),
        (0xa55a, Tail::Mps, 0x4ab4),
        (0xa55a, Tail::Mrd, 0xa55b),
        (0xa55a, Tail::Mpp, 0x52ad),
        (0xa55a, Tail::Invert, 0xa55b),
        (0xa55b, Tail::AndBlock, 0x52ad),
        (0xa55b, Tail::OrBlock, 0x52ad),
        (0xa55b, Tail::Mps, 0x4ab7),
        (0xa55b, Tail::Mrd, 0xa55b),
        (0xa55b, Tail::Mpp, 0x52ad),
        (0xa55b, Tail::Invert, 0xa55a),
        (0x8001, Tail::Mps, 0x0003),
        (0x8001, Tail::Mpp, 0x4000),
    ] {
        let mut control = ControlState::default();
        control.logic_word = initial;
        control.master_gate = 0;
        control.out_enabled = false;
        control.pulse_history.insert(99, true);
        execute_control(&mut control, op, 123);
        assert_eq!(control.logic_word, expected, "{initial:04x} {op:?}");
        assert_eq!(control.master_gate, 0);
        assert!(!control.out_enabled);
        assert_eq!(control.pulse_history.len(), 1);
        assert_eq!(control.pulse_history.get(&99), Some(&true));
    }
}

#[test]
fn random_tail_np_pn_use_actual_pc_history_and_preserve_upper_bits() {
    let mut c = ControlState::default();
    c.master_gate = 0;
    c.out_enabled = false;
    for (op, pc, input, output, stored) in [
        (Tail::Np, 10, 0xa55b, 0xa55b, true),
        (Tail::Np, 10, 0xa55b, 0xa55a, true),
        (Tail::Np, 11, 0xa55b, 0xa55b, true),
        (Tail::Np, 10, 0xa55a, 0xa55a, false),
        (Tail::Np, 10, 0xa55b, 0xa55b, true),
        (Tail::Pn, 20, 0xa55b, 0xa55a, true),
        (Tail::Pn, 20, 0xa55a, 0xa55b, false),
        (Tail::Pn, 20, 0xa55a, 0xa55a, false),
    ] {
        c.logic_word = input;
        execute_control(&mut c, op, pc);
        assert_eq!(c.logic_word, output, "{op:?} PC {pc}");
        assert_eq!(c.pulse_history.get(&pc), Some(&stored));
        assert_eq!(c.master_gate, 0);
        assert!(!c.out_enabled);
    }
    assert_eq!(c.pulse_history.len(), 3);
}

#[test]
fn random_tail_vm_header_flow_covers_seven_normal_and_pulse_rows() {
    for (name, visits) in [
        ("DRAND", vec![(false, 7), (true, 13)]),
        (
            "DRANDP",
            vec![(false, 7), (true, 13), (true, 13), (false, 13), (false, 7)],
        ),
    ] {
        let label = format!("{name} K1 K1 D20E0");
        let p = program(&label);
        let header = action_pc(&p, &label);
        let mut sim = current();
        for (power, advance) in visits {
            sim.memory.set_input(d("X0"), power);
            sim.begin_plan_scan(&p, 0, false).unwrap();
            let trace = finish(&mut sim, &p);
            assert!(
                trace.contains(&(header, header + advance)),
                "{name} {power}: {trace:?}"
            );
            let tail: Vec<_> = trace
                .iter()
                .copied()
                .filter(|(pc, _)| (header + 7..header + 13).contains(pc))
                .collect();
            let expected: Vec<_> = if advance == 7 {
                (header + 7..header + 13).map(|pc| (pc, pc + 1)).collect()
            } else {
                Vec::new()
            };
            assert_eq!(tail, expected);
        }
    }
    for name in ["RAND", "RANDP"] {
        let label = format!("{name} K1 K1 D20E0");
        let p = program(&label);
        let header = action_pc(&p, &label);
        let mut sim = current();
        for power in [false, true, true, false, false] {
            sim.memory.set_input(d("X0"), power);
            sim.begin_plan_scan(&p, 0, false).unwrap();
            assert!(finish(&mut sim, &p).contains(&(header, header + 7)));
        }
    }
}

#[test]
fn random_tail_vm_all_controls_are_owned_nodes_without_source_stack_balance() {
    for (base, op, expected) in [
        (246, Tail::Np, 0xa55b),
        (247, Tail::Pn, 0xa55a),
        (248, Tail::AndBlock, 0x52ad),
        (249, Tail::OrBlock, 0x52ad),
        (250, Tail::Mps, 0x4ab7),
        (251, Tail::Mrd, 0xa55b),
        (252, Tail::Mpp, 0x52ad),
        (253, Tail::Invert, 0xa55a),
    ] {
        // There is intentionally no textual MPS/MPP or LD block partner.
        let label = format!("DRAND K1 K1 K1M{base}F0");
        let p = program(&label);
        let h = action_pc(&p, &label);
        let owned: Vec<_> = p
            .execution_plan
            .nodes
            .iter()
            .filter(|n| matches!(n.op, PlanOp::OperandTailControl { .. }))
            .collect();
        assert_eq!(owned.len(), 1);
        assert_eq!(owned[0].pc, h + 9);
        assert_eq!(owned[0].op, PlanOp::OperandTailControl { header: h, op });
        let mut sim = current();
        sim.memory.set_word(d("D1029"), i16::MAX); // No target read on a false body.
        for n in 1020..=1022 {
            sim.memory.set_system(n, n != 1021);
        }
        at_header(&mut sim, &p, h);
        sim.control.logic_word = 0xa55b;
        sim.control.master_gate = 0;
        sim.control.out_enabled = false;
        let header_step = sim.execute_plan_step(&p).unwrap();
        assert_eq!((header_step.pc, header_step.next_pc), (h, h + 7));
        until(&mut sim, &p, h + 9);
        let tail_step = sim.execute_plan_step(&p).unwrap();
        assert_eq!((tail_step.pc, tail_step.next_pc), (h + 9, h + 10));
        assert_eq!(sim.control.logic_word, expected, "{op:?}");
        assert_eq!(sim.control.master_gate, 0);
        assert!(!sim.control.out_enabled);
        assert!(!sim.observed_devices.contains(&d("D1029")));
        assert_eq!(
            [1020, 1021, 1022].map(|n| sim.memory.system_bit(n)),
            [true, false, true]
        );
        finish(&mut sim, &p);
    }
}

#[test]
fn random_tail_documented_default_skips_disabled_tail_and_operand_observation() {
    let label = "DRAND K1 K1 K1M253F0";
    let p = program(label);
    let h = action_pc(&p, label);
    let mut sim = Simulator::default();
    assert_eq!(
        sim.memory.random_tail_profile,
        RandomTailProfile::Documented
    );
    sim.memory.set_word(d("D1029"), i16::MAX);
    sim.memory.set_internal(d("M253"), true);
    at_header(&mut sim, &p, h);
    sim.control.logic_word = 0xa55b;
    sim.control.master_gate = 0;
    let step = sim.execute_plan_step(&p).unwrap();
    assert_eq!((step.pc, step.next_pc), (h, h + 13));
    assert_eq!(sim.control.logic_word, 0xa55b);
    assert!(sim.control.pulse_history.is_empty());
    assert!(!sim.observed_devices.contains(&d("D1029")));
    assert!(sim.memory.bit(d("M253")));
    assert_eq!(sim.memory.random_state, None);
    finish(&mut sim, &p);
}

#[test]
fn random_tail_original_payload_not_effective_address_selects_hidden_control() {
    let label = "DRAND K1 K1 D125F7";
    let p = program(label);
    let h = action_pc(&p, label);
    for offset in [-5, 0, 20] {
        let mut sim = current();
        sim.memory.set_word(d("D1195"), offset);
        for n in [120, 125, 145] {
            sim.memory.set_word(d(&format!("D{n}")), 4321);
        }
        at_header(&mut sim, &p, h);
        sim.control.logic_word = 0xa55b;
        sim.control.master_gate = 0;
        sim.execute_plan_step(&p).unwrap();
        until(&mut sim, &p, h + 9);
        sim.execute_plan_step(&p).unwrap();
        assert_eq!(sim.control.logic_word, 0x4ab7, "offset {offset}");
        assert!(!sim.observed_devices.contains(&d("D1195")));
        for n in [120, 125, 145] {
            assert_eq!(sim.memory.word(d(&format!("D{n}"))), 4321);
        }
        finish(&mut sim, &p);
    }
}

#[test]
fn random_tail_history_survives_cj_and_active_skips_until_actual_false_visit() {
    let label = "DRAND K1 K1 K1M246F0";
    let p = compile_il(&format!(
        "LD X2\nCJ P0\nLD X0\n{label}\nP0\nLD X3\nOUT Y1\nEND"
    ))
    .unwrap();
    let h = action_pc(&p, label);
    let mut sim = current();
    at_header(&mut sim, &p, h);
    sim.control.logic_word = 0xa55b;
    sim.control.master_gate = 0;
    finish(&mut sim, &p);
    assert_eq!(sim.control.pulse_history.get(&(h + 9)), Some(&true));

    sim.memory.set_input(d("X2"), true);
    sim.begin_plan_scan(&p, 0, false).unwrap();
    assert!(!finish(&mut sim, &p).iter().any(|(pc, _)| *pc == h));
    assert_eq!(sim.control.pulse_history.get(&(h + 9)), Some(&true));

    sim.memory.set_input(d("X2"), false);
    sim.memory.set_input(d("X0"), true);
    sim.begin_plan_scan(&p, 0, false).unwrap();
    assert!(finish(&mut sim, &p).contains(&(h, h + 13)));
    assert_eq!(sim.control.pulse_history.get(&(h + 9)), Some(&true));

    sim.memory.set_input(d("X0"), false);
    sim.begin_plan_scan(&p, 0, false).unwrap();
    assert!(finish(&mut sim, &p).contains(&(h + 9, h + 10)));
    assert_eq!(sim.control.pulse_history.get(&(h + 9)), Some(&false));
}

#[test]
fn random_tail_history_scan_boundaries_retain_but_cold_restart_clears() {
    let mut c = ControlState::default();
    c.pulse_history.insert(81, true);
    c.begin_scan();
    assert_eq!(c.pulse_history.get(&81), Some(&true));
    c.begin_stepped_scan();
    c.finish_scan(ControlSite::new(100, 0, 1)).unwrap();
    assert_eq!(c.pulse_history.get(&81), Some(&true));
    c.reset_instruction_state();
    assert!(c.pulse_history.is_empty());

    let mut sim = current();
    sim.control.pulse_history.insert(81, true);
    sim.restart_preserving_file_store();
    assert_eq!(
        sim.memory.random_tail_profile,
        RandomTailProfile::CurrentCommgr
    );
    assert!(sim.control.pulse_history.is_empty());
}

#[test]
fn random_tail_iret_merges_actual_pc_history_and_restores_outer_logic() {
    let outer = "DRAND K1 K1 K1M246F0";
    let inner = "DRAND K1 K1 K1M247F0";
    let p = compile_il(&format!(
        "EI\nLD X0\n{outer}\nMOV K7 D300\nFEND\nI001\nLD X1\n{inner}\nIRET\nEND"
    ))
    .unwrap();
    let outer_pc = action_pc(&p, outer);
    let inner_pc = action_pc(&p, inner);
    let mut sim = current();
    at_header(&mut sim, &p, outer_pc);
    sim.control.logic_word = 0xa55b;
    sim.control.master_gate = 0;
    sim.control.pulse_history.insert(outer_pc + 9, true);
    sim.control.pulse_history.insert(inner_pc + 9, true);
    sim.memory
        .interrupts
        .request("I001".parse::<InterruptId>().unwrap(), 0)
        .unwrap();
    sim.execute_plan_step(&p).unwrap(); // Dispatch must occur before searching for ISR PC.
    until(&mut sim, &p, inner_pc);
    sim.control.logic_word = 2;
    sim.control.master_gate = 1;
    sim.execute_plan_step(&p).unwrap();
    until(&mut sim, &p, inner_pc + 9);
    sim.execute_plan_step(&p).unwrap();
    assert_eq!(sim.control.logic_word, 3); // PN: old TRUE, current FALSE.
    until(&mut sim, &p, outer_pc);
    assert_eq!(sim.control.logic_word, 0xa55b);
    assert_eq!(sim.control.master_gate, 0);
    assert_eq!(sim.control.pulse_history.get(&(outer_pc + 9)), Some(&true));
    assert_eq!(sim.control.pulse_history.get(&(inner_pc + 9)), Some(&false));
    finish(&mut sim, &p);
}

#[test]
fn random_tail_profile_is_frozen_for_cursor_and_legacy_allows_only_inert_tags() {
    let label = "DRAND K1 K1 D20E0";
    let p = program(label);
    let h = action_pc(&p, label);
    let mut sim = current();
    at_header(&mut sim, &p, h);
    sim.memory.random_tail_profile = RandomTailProfile::Documented;
    assert!(sim.execute_plan_step(&p).is_err());
    assert!(matches!(sim.last_scan, ScanStatus::Fault(_)));
    assert_eq!(sim.memory.double_word(d("D20")), 0);

    for target in ["D20E0", "D20F0", "D125F7", "K1M246F0"] {
        let p = program(&format!("DRAND K1 K1 {target}"));
        let mut sim = Simulator::default();
        sim.memory.random_tail_profile = RandomTailProfile::InstalledLegacyInertOnly;
        assert!(sim.begin_plan_scan(&p, 0, false).is_err(), "{target}");
    }
    let p = program("DRAND K1 K1 D20");
    let h = action_pc(&p, "DRAND K1 K1 D20");
    let mut sim = Simulator::default();
    sim.memory.random_tail_profile = RandomTailProfile::InstalledLegacyInertOnly;
    sim.begin_plan_scan(&p, 0, false).unwrap();
    assert!(finish(&mut sim, &p).contains(&(h, h + 7)));
}

#[test]
fn random_tail_active_semantic_error_skips_collision_and_target_index() {
    for bounds in ["K-1 K2", "K2 K1"] {
        let label = format!("DRAND {bounds} K1M253F0");
        let p = program(&label);
        let h = action_pc(&p, &label);
        let mut sim = current();
        sim.memory.set_input(d("X0"), true);
        sim.memory.set_word(d("D1029"), i16::MAX);
        sim.memory.set_internal(d("M253"), true);
        for n in 1020..=1022 {
            sim.memory.set_system(n, true);
        }
        at_header(&mut sim, &p, h);
        let before = sim.control.logic_word;
        let step = sim.execute_plan_step(&p).unwrap();
        assert_eq!((step.pc, step.next_pc), (h, h + 13));
        assert_eq!(sim.control.logic_word, before); // No hidden INV.
        assert_eq!(sim.memory.word(d("D1067")), 0x0e1a);
        assert_eq!(sim.memory.word(d("D1068")), (h + 5) as i16);
        assert!(!sim.observed_devices.contains(&d("D1029")));
        assert!(sim.memory.bit(d("M253")));
        assert_eq!(
            [1020, 1021, 1022].map(|n| sim.memory.system_bit(n)),
            [true; 3]
        );
        if bounds == "K-1 K2" {
            assert_eq!(sim.memory.random_state, None);
        } else {
            assert!(sim.memory.random_state.is_some());
        }
        assert!(
            !finish(&mut sim, &p)
                .iter()
                .any(|(pc, _)| (h + 7..h + 13).contains(pc))
        );
    }
}

#[test]
fn random_tail_layout_rejects_forged_bytes_owner_and_unowned_control() {
    let label = "DRAND K1 K1 K1M250F0";
    let p = program(label);
    let h = action_pc(&p, label);
    let encoded = crate::es2_encoder::encode(&p).unwrap();
    assert_eq!(
        &encoded.words[usize::from(h)..usize::from(h + 13)],
        &instruction(0xfaf4, 0x0882)
    );
    crate::random_tail::validate_layout(&p, &encoded).unwrap();
    let tail_index = encoded.execution_plan.by_pc[&(h + 9)];
    let mut changed_word = encoded.clone();
    changed_word.words[usize::from(h + 9)] = 0xfcf4;
    assert!(crate::random_tail::validate_layout(&p, &changed_word).is_err());
    let mut changed_owner = encoded.clone();
    changed_owner.execution_plan.nodes[tail_index].op = PlanOp::OperandTailControl {
        header: h + 1,
        op: Tail::Mps,
    };
    assert!(crate::random_tail::validate_layout(&p, &changed_owner).is_err());
    let mut changed_boundary = encoded.clone();
    changed_boundary.execution_plan.nodes[tail_index].fallthrough = h + 11;
    assert!(crate::random_tail::validate_layout(&p, &changed_boundary).is_err());
    let mut outside = encoded.clone();
    let end_index = outside.execution_plan.nodes.len() - 1;
    outside.execution_plan.nodes[end_index].op = PlanOp::OperandTailControl {
        header: h,
        op: Tail::Mps,
    };
    assert!(crate::random_tail::validate_layout(&p, &outside).is_err());
}
