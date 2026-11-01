//! Independent PDF684..697 acceptance witnesses, using literal counts/times.
//! Explicit virtual profiles are not installed-native or physical PLC parity.
use crate::execution_plan::PlanOp;
use crate::plc::{Device, Program, ScanStatus, Simulator, compile_il};
fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn sim() -> Simulator {
    let mut s = Simulator::default();
    s.memory.set_input(d("X20"), true);
    s
}
fn scan(s: &mut Simulator, p: &Program) {
    s.scan(p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(
        s.memory.execution_limitations.is_empty(),
        "{:?}",
        s.memory.execution_limitations
    );
}
fn advance(s: &mut Simulator, us: u64) {
    crate::timing_io::advance_by(&mut s.memory, us);
}
fn pv(s: &Simulator) -> i32 {
    s.memory.pulse_output.axis_position(0, &s.memory)
}
fn pc(p: &Program, name: &str) -> u16 {
    p.execution_plan
        .nodes
        .iter()
        .find_map(|n| match n.op {
            PlanOp::Action {
                network, action, ..
            } if p.networks[network].actions[action]
                .label()
                .starts_with(name) =>
            {
                Some(n.pc)
            }
            _ => None,
        })
        .unwrap()
}
fn table(s: &mut Simulator) {
    // Two pulses at1000Hz, then two at2000Hz: exact total3ms.
    for (addr, val) in [(0, 1000), (2, 2), (4, 2000), (6, 2)] {
        s.memory.set_double_word(d(&format!("D{addr}")), val);
    }
    s.memory.set_word(d("D100"), 2);
}
fn variable(s: &mut Simulator, hz: i32, count: i32, gap: i16, ms: i16) {
    s.memory.set_double_word(d("D0"), hz);
    s.memory.set_double_word(d("D10"), count);
    s.memory.set_word(d("D20"), gap);
    s.memory.set_word(d("D21"), ms);
}
#[test]
fn genuine_three_headers_layout_and_native_guard_are_separate() {
    for (line, head, len) in [
        ("DPTPO D0 D100 Y0", 0xa501, 13),
        ("DCLLM X4 K10 K1000 Y0", 0xa901, 17),
        ("DVSPO D0 D10 D20 Y0", 0xab01, 17),
    ] {
        let p = compile_il(&format!("LD X20\n{line}\nMOV K7 D200\nEND")).unwrap();
        let e = crate::es2_encoder::local_execution_layout(&p).unwrap();
        assert_eq!(e.known_word_at(e.action_pc[0]), Some(head));
        assert_eq!(e.action_pc[1] - e.action_pc[0], len);
        assert!(crate::es2_encoder::encode(&p).is_err());
    }
}
#[test]
fn undocumented_pulse_and_narrow_spellings_are_rejected() {
    for line in [
        "PTPO D0 D100 Y0",
        "DPTPOP D0 D100 Y0",
        "CLLM X4 K10 K1000 Y0",
        "DCLLMP X4 K10 K1000 Y0",
        "VSPO D0 D10 D20 Y0",
        "DVSPOP D0 D10 D20 Y0",
    ] {
        assert!(
            compile_il(&format!("LD X20\n{line}\nEND")).is_err(),
            "{line}"
        );
    }
}
#[test]
fn table_exact_segment_deadlines_cross_without_an_extra_scan() {
    let p = compile_il("LD X20\nDPTPO D0 D100 Y0\nEND").unwrap();
    let mut s = sim();
    table(&mut s);
    scan(&mut s, &p);
    advance(&mut s, 1999);
    assert_eq!(pv(&s), 1);
    advance(&mut s, 1);
    assert_eq!(pv(&s), 2);
    advance(&mut s, 499);
    assert_eq!(pv(&s), 2);
    advance(&mut s, 1);
    assert_eq!(pv(&s), 3);
    advance(&mut s, 500);
    assert_eq!(pv(&s), 4);
    advance(&mut s, 10000);
    assert_eq!(pv(&s), 4);
}
#[test]
fn table_segment_monitor_is_written_at_instruction_visit_only() {
    let p = compile_il("LD X20\nDPTPO D0 D100 Y0\nEND").unwrap();
    let mut s = sim();
    table(&mut s);
    scan(&mut s, &p);
    assert_eq!(s.memory.word(d("D101")), 1);
    advance(&mut s, 2000);
    assert_eq!(s.memory.word(d("D101")), 1);
    scan(&mut s, &p);
    assert_eq!(s.memory.word(d("D101")), 2);
}
#[test]
fn table_latches_parameters_until_false_rearm() {
    let p = compile_il("LD X20\nDPTPO D0 D100 Y0\nEND").unwrap();
    let mut s = sim();
    table(&mut s);
    scan(&mut s, &p);
    s.memory.set_double_word(d("D0"), 10000);
    s.memory.set_double_word(d("D2"), 100);
    scan(&mut s, &p);
    advance(&mut s, 3000);
    assert_eq!(pv(&s), 4);
}
#[test]
fn table_false_stops_immediately() {
    let p = compile_il("LD X20\nDPTPO D0 D100 Y0\nEND").unwrap();
    let mut s = sim();
    table(&mut s);
    scan(&mut s, &p);
    advance(&mut s, 1000);
    s.memory.set_input(d("X20"), false);
    scan(&mut s, &p);
    advance(&mut s, 10000);
    assert_eq!(pv(&s), 1);
    assert!(!s.memory.physical_output(d("Y0")));
}
#[test]
fn table_has_no_direction_pin_and_does_not_own_adjacent_y1() {
    let p = compile_il("LD X20\nDPTPO D0 D100 Y0\nOUT Y1\nEND").unwrap();
    let mut s = sim();
    table(&mut s);
    scan(&mut s, &p);
    assert!(s.memory.bit(d("Y1")));
    assert!(s.memory.physical_output(d("Y1")));
    advance(&mut s, 3000);
    assert!(s.memory.bit(d("Y1")));
}
#[test]
fn repeated_call_for_visits_share_one_table_owner() {
    let p =
        compile_il("LD X20\nFOR K3\nCALL P0\nNEXT\nFEND\nP0\nLD X20\nDPTPO D0 D100 Y0\nSRET\nEND")
            .unwrap();
    let mut s = sim();
    table(&mut s);
    scan(&mut s, &p);
    assert_eq!(s.memory.pulse_output.axis_owner(0), Some(pc(&p, "DPTPO")));
    advance(&mut s, 3000);
    assert_eq!(pv(&s), 4);
}
#[test]
fn mc_false_visits_stop_table() {
    let p = compile_il("LD X21\nMC N0\nLD X20\nDPTPO D0 D100 Y0\nMCR N0\nEND").unwrap();
    let mut s = sim();
    table(&mut s);
    s.memory.set_input(d("X21"), true);
    scan(&mut s, &p);
    advance(&mut s, 1000);
    s.memory.set_input(d("X21"), false);
    scan(&mut s, &p);
    advance(&mut s, 10000);
    assert_eq!(pv(&s), 1);
}
#[test]
fn variable_startup_pdf_1000_hz_first_step_is_immediate() {
    let p = compile_il("LD X20\nDVSPO D0 D10 D20 Y0\nEND").unwrap();
    let mut s = sim();
    variable(&mut s, 6000, 0, 1000, 10);
    scan(&mut s, &p);
    assert_eq!(
        s.memory
            .pulse_output
            .axis_snapshot(pc(&p, "DVSPO"))
            .unwrap()
            .frequency_millihz,
        1_000_000
    );
    advance(&mut s, 10000);
    assert_eq!(pv(&s), 10);
    assert_eq!(
        s.memory
            .pulse_output
            .axis_snapshot(pc(&p, "DVSPO"))
            .unwrap()
            .frequency_millihz,
        2_000_000
    );
}
#[test]
fn variable_target_is_latched_while_frequency_is_live() {
    let p = compile_il("LD X20\nDVSPO D0 D10 D20 Y0\nEND").unwrap();
    let mut s = sim();
    variable(&mut s, 1000, 10, 1000, 1);
    scan(&mut s, &p);
    s.memory.set_double_word(d("D10"), 1000);
    scan(&mut s, &p);
    advance(&mut s, 100000);
    assert_eq!(pv(&s), 10);
}
#[test]
fn variable_gap_changes_alone_do_not_revision_current_ramp() {
    let p = compile_il("LD X20\nDVSPO D0 D10 D20 Y0\nEND").unwrap();
    let mut s = sim();
    variable(&mut s, 6000, 0, 1000, 10);
    scan(&mut s, &p);
    s.memory.set_word(d("D20"), 6000);
    s.memory.set_word(d("D21"), 1);
    scan(&mut s, &p);
    advance(&mut s, 10000);
    assert_eq!(pv(&s), 10);
    assert_eq!(
        s.memory
            .pulse_output
            .axis_snapshot(pc(&p, "DVSPO"))
            .unwrap()
            .frequency_millihz,
        2_000_000
    );
}
#[test]
fn physical_inhibit_blocks_all_extended_output_edges() {
    for line in [
        "DPTPO D0 D100 Y0",
        "DVSPO D200 K0 D20 Y0",
        "DCLLM X4 K100 K1000 Y0",
    ] {
        let p = compile_il(&format!("LD X20\n{line}\nSET M1034\nEND")).unwrap();
        let mut s = sim();
        table(&mut s);
        s.memory.set_double_word(d("D200"), 1000);
        s.memory.set_word(d("D20"), 1000);
        s.memory.set_word(d("D21"), 1);
        scan(&mut s, &p);
        advance(&mut s, 10000);
        assert!(!s.memory.physical_output(d("Y0")));
        assert!(!s.memory.physical_output(d("Y1")));
    }
}
#[test]
fn abort_disarms_all_three_extended_engines() {
    for line in [
        "DPTPO D0 D100 Y0",
        "DVSPO D200 K0 D20 Y0",
        "DCLLM X4 K100 K1000 Y0",
    ] {
        let p = compile_il(&format!(
            "LD X20\n{line}\nDELAY K1000\nDELAY K1000\nDELAY K1\nEND"
        ))
        .unwrap();
        let mut s = sim();
        table(&mut s);
        s.memory.set_double_word(d("D200"), 1000);
        s.memory.set_word(d("D20"), 1000);
        s.memory.set_word(d("D21"), 1);
        s.scan(&p, 0);
        assert_ne!(s.last_scan, ScanStatus::Complete);
        let stopped = pv(&s);
        advance(&mut s, 1000000);
        assert_eq!(pv(&s), stopped);
        assert!(s.memory.pulse_output.axis_owner(0).is_none());
    }
}

#[test]
fn table_and_variable_commands_cannot_steal_the_same_y0() {
    let p = compile_il("LD X20\nDPTPO D0 D100 Y0\nDVSPO D200 K0 D20 Y0\nEND").unwrap();
    let mut s = sim();
    table(&mut s);
    s.memory.set_double_word(d("D200"), 1000);
    s.memory.set_word(d("D20"), 1000);
    s.memory.set_word(d("D21"), 1);
    s.scan(&p, 0);
    assert_eq!(s.memory.pulse_output.axis_owner(0), Some(pc(&p, "DPTPO")));
    assert!(!s.memory.execution_limitations.is_empty());
    advance(&mut s, 3000);
    assert_eq!(pv(&s), 4);
}
#[test]
fn closed_loop_commanded_position_is_not_external_encoder_feedback() {
    let p =
        compile_il("EI\nLD X20\nDCLLM X4 K2 K1000 Y0\nFEND\nI401\nLD M1000\nINC D300\nIRET\nEND")
            .unwrap();
    let mut s = sim();
    scan(&mut s, &p);
    advance(&mut s, 1000000);
    assert!(
        pv(&s) > 2,
        "output must continue at idle after planned pulse count"
    );
    assert!(!s.memory.system_bit(1029));
    assert_eq!(s.memory.word(d("D300")), 0);
    assert!(s.memory.pulse_output.axis_owner(0).is_some());
}
#[test]
fn closed_loop_x4_edge_stops_only_when_its_enabled_irq_is_dispatched() {
    let p =
        compile_il("EI\nLD X20\nDCLLM X4 K2 K1000 Y0\nFEND\nI401\nLD M1000\nINC D300\nIRET\nEND")
            .unwrap();
    let mut s = sim();
    scan(&mut s, &p);
    s.memory.set_system(1054, true);
    s.memory
        .timing_io
        .enqueue_input_edge(100000, d("X4"), true)
        .unwrap();
    advance(&mut s, 100000);
    scan(&mut s, &p);
    assert_eq!(s.memory.word(d("D300")), 0);
    assert!(!s.memory.system_bit(1029));
    s.memory.set_system(1054, false);
    scan(&mut s, &p);
    assert_eq!(s.memory.word(d("D300")), 1);
    assert!(s.memory.system_bit(1029));
    let stopped = pv(&s);
    advance(&mut s, 100000);
    assert_eq!(pv(&s), stopped);
}
#[test]
fn closed_loop_di_keeps_pending_feedback_without_premature_completion() {
    let disabled =
        compile_il("DI\nLD X20\nDCLLM X4 K2 K1000 Y0\nFEND\nI401\nLD M1000\nINC D300\nIRET\nEND")
            .unwrap();
    let mut s = sim();
    scan(&mut s, &disabled);
    s.memory
        .timing_io
        .enqueue_input_edge(100000, d("X4"), true)
        .unwrap();
    advance(&mut s, 100000);
    scan(&mut s, &disabled);
    assert!(!s.memory.system_bit(1029));
    assert_eq!(s.memory.word(d("D300")), 0);
    assert!(s.memory.pulse_output.axis_owner(0).is_some());
}
#[test]
fn dicf_requires_matching_frequency_source_and_output_for_variable_axis() {
    let p = compile_il("LD X20\nDVSPO D0 K0 D20 Y0\nLD X21\nDICF D2 D20 Y0\nEND").unwrap();
    let mut s = sim();
    variable(&mut s, 1000, 0, 1000, 10);
    s.memory.set_double_word(d("D2"), 6000);
    s.memory.set_input(d("X21"), true);
    s.scan(&p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(
        s.memory
            .execution_limitations
            .iter()
            .any(|message| message.contains("DICF"))
    );
    assert_eq!(s.memory.pulse_output.axis_owner(0), Some(pc(&p, "DVSPO")));
    advance(&mut s, 10000);
    assert_eq!(pv(&s), 10);
    assert_eq!(
        s.memory
            .pulse_output
            .axis_snapshot(pc(&p, "DVSPO"))
            .unwrap()
            .frequency_millihz,
        1_000_000
    );
}

#[test]
fn ordinary_variable_edit_waits_one_gap_before_first_live_step() {
    let p = compile_il("LD X20\nDVSPO D0 K0 D20 Y0\nEND").unwrap();
    let mut s = sim();
    variable(&mut s, 1000, 0, 1000, 10);
    scan(&mut s, &p);
    advance(&mut s, 10000);
    assert_eq!(pv(&s), 10);
    s.memory.set_double_word(d("D0"), 6000);
    scan(&mut s, &p);
    assert_eq!(
        s.memory
            .pulse_output
            .axis_snapshot(pc(&p, "DVSPO"))
            .unwrap()
            .frequency_millihz,
        1_000_000
    );
    advance(&mut s, 10000);
    assert_eq!(pv(&s), 20);
    assert_eq!(
        s.memory
            .pulse_output
            .axis_snapshot(pc(&p, "DVSPO"))
            .unwrap()
            .frequency_millihz,
        2_000_000
    );
}
#[test]
fn dicf_in_a_real_irq_dispatch_applies_first_step_at_that_visit() {
    let p=compile_il("EI\nLD X21\nCJ P0\nLD X20\nDVSPO D0 K0 D20 Y0\nP0\nFEND\nI001\nLD M1000\nDMOV K6000 D0\nDICF D0 D20 Y0\nIRET\nEND").unwrap();
    let mut s = sim();
    variable(&mut s, 1000, 0, 1000, 10);
    scan(&mut s, &p);
    advance(&mut s, 10000);
    assert_eq!(pv(&s), 10);
    s.memory.set_input(d("X21"), true);
    s.memory
        .timing_io
        .enqueue_input_edge(10000, d("X0"), true)
        .unwrap();
    advance(&mut s, 0);
    scan(&mut s, &p);
    assert_eq!(
        s.memory
            .pulse_output
            .axis_snapshot(pc(&p, "DVSPO"))
            .unwrap()
            .frequency_millihz,
        2_000_000
    );
    advance(&mut s, 10000);
    assert_eq!(pv(&s), 30);
}

#[test]
fn table_fortieth_segment_uses_full_four_word_stride_and_exact_final_count() {
    let p = compile_il("LD X20\nDPTPO D0 D300 Y0\nEND").unwrap();
    let mut s = sim();
    for segment in 0..40 {
        s.memory
            .set_double_word(d(&format!("D{}", segment * 4)), 1000);
        s.memory
            .set_double_word(d(&format!("D{}", segment * 4 + 2)), 1);
    }
    s.memory.set_word(d("D300"), 40);
    scan(&mut s, &p);
    advance(&mut s, 39000);
    assert_eq!(pv(&s), 39);
    assert_eq!(s.memory.word(d("D301")), 1);
    scan(&mut s, &p);
    assert_eq!(s.memory.word(d("D301")), 40);
    advance(&mut s, 1000);
    assert_eq!(pv(&s), 40);
    advance(&mut s, 10000);
    assert_eq!(pv(&s), 40);
}
