//! Genuine compiled IL and event-scheduler integration. Native handlers are
//! stubs; every expectation here is the explicitly named local profile.
use crate::execution_plan::PlanOp;
use crate::plc::{Device, Program, ScanStatus, Simulator, compile_il};
fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn edge(s: &mut Simulator, t: u64, p: &str, on: bool) {
    s.memory.timing_io.enqueue_input_edge(t, d(p), on).unwrap();
}
fn pulse(s: &mut Simulator, t: u64) {
    edge(s, t, "X0", true);
    edge(s, t + 1, "X0", false);
}
fn scan(s: &mut Simulator, p: &Program, ms: u64) {
    s.scan(p, ms);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(
        s.memory.execution_limitations.is_empty(),
        "{:?}",
        s.memory.execution_limitations
    );
}
fn action_pc(p: &Program, label: &str) -> u16 {
    p.execution_plan
        .nodes
        .iter()
        .find_map(|n| match n.op {
            PlanOp::Action {
                network, action, ..
            } if p.networks[network].actions[action].label() == label => Some(n.pc),
            _ => None,
        })
        .unwrap()
}
fn until(s: &mut Simulator, p: &Program, pc: u16) {
    let mut n = 0;
    while s.current_plan_pc() != Some(pc) {
        s.execute_plan_step(p).unwrap();
        n += 1;
        assert!(n < 1000 && s.current_plan_pc().is_some());
    }
}
fn finish(s: &mut Simulator, p: &Program) {
    let mut n = 0;
    while s.current_plan_pc().is_some() {
        s.execute_plan_step(p).unwrap();
        n += 1;
        assert!(n < 1000);
    }
    assert_eq!(s.last_scan, ScanStatus::Complete);
}

#[test]
fn actual_dcnt_equality_requests_counter_irq_before_following_source_action() {
    let p=compile_il("LD M1000\nDCNT C235 K10\nDHSCS K3 C235 I010\nEI\nLD M1000\nDELAY K10\nMOV K11 D30\nFEND\nI010\nLD M1000\nDMOV C235 D0\nMOV D30 D20\nINC D22\nIRET\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_word(d("D30"), 7);
    for t in [100, 200, 300] {
        pulse(&mut s, t);
    }
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.double_word(d("D0")), 3);
    assert_eq!(s.memory.word(d("D20")), 7);
    assert_eq!(s.memory.word(d("D30")), 11);
    assert_eq!(s.memory.word(d("D22")), 1);
    scan(&mut s, &p, 0);
    assert_eq!(
        s.memory.word(d("D22")),
        1,
        "held equality must not requeue I010"
    );
}
#[test]
fn set_reset_comparators_change_latched_state_during_instruction_delay() {
    let p=compile_il("LD M1000\nDCNT C235 K10\nDHSCS K1 C235 M0\nDHSCR K2 C235 M0\nDELAY K2\nLD M0\nOUT Y10\nLD M1000\nDELAY K2\nLD M0\nOUT Y11\nEND").unwrap();
    let mut s = Simulator::default();
    pulse(&mut s, 100);
    pulse(&mut s, 300);
    scan(&mut s, &p, 0);
    assert!(s.memory.bit(d("Y10")));
    assert!(!s.memory.bit(d("Y11")));
    assert!(!s.memory.bit(d("M0")));
}
#[test]
fn fast_y_is_physical_in_body_deferred_y_waits_and_later_normal_out_wins_at_end() {
    let p=compile_il("LD M1000\nDCNT C235 K10\nDHSCS K1 C235 Y0\nDHSCS K1 C235 Y4\nDELAY K10\nMOV K1 D30\nLD X20\nOUT Y0\nEND").unwrap();
    let mut s = Simulator::default();
    pulse(&mut s, 100);
    s.begin_plan_scan(&p, 0, false).unwrap();
    until(&mut s, &p, action_pc(&p, "MOV K1 D30"));
    assert!(s.memory.bit(d("Y0")));
    assert!(s.memory.physical_output(d("Y0")));
    assert!(s.memory.bit(d("Y4")));
    assert!(!s.memory.physical_output(d("Y4")));
    finish(&mut s, &p);
    assert!(!s.memory.bit(d("Y0")));
    assert!(!s.memory.physical_output(d("Y0")));
    assert!(s.memory.physical_output(d("Y4")));
}
#[test]
fn zone_one_hot_boundaries_are_observed_by_later_contacts_in_same_scan() {
    let p=compile_il("LD M1000\nDCNT C235 K10\nDHSZ K2 K4 C235 Y0\nDELAY K10\nLD Y0\nOUT M20\nLD Y1\nOUT M21\nLD Y2\nOUT M22\nEND").unwrap();
    let mut s = Simulator::default();
    for (scan_no, expected) in [0, 1, 1, 1, 2].into_iter().enumerate() {
        pulse(&mut s, scan_no as u64 * 1000 + 100);
        scan(&mut s, &p, 0);
        for n in 0..3 {
            assert_eq!(s.memory.bit(d(&format!("M{}", 20 + n))), n == expected);
        }
    }
}
#[test]
fn spd_deadline_is_processed_before_same_timestamp_physical_edge() {
    let p = compile_il("LD X20\nSPD X0 K2 D0\nLD M1000\nDELAY K20\nDMOV D0 D10\nDMOV D2 D12\nEND")
        .unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X20"), true);
    pulse(&mut s, 100);
    pulse(&mut s, 2000);
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.double_word(d("D10")), 1);
    assert_eq!(s.memory.double_word(d("D12")), 1);
    pulse(&mut s, 2500);
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.double_word(d("D10")), 2);
    assert_eq!(s.memory.double_word(d("D12")), 0);
}
#[test]
fn repeated_call_visits_preserve_one_spd_occurrence_window_and_capture_count() {
    let p=compile_il("LD M1000\nCALL P0\nLD M1000\nDELAY K10\nCALL P0\nLD M1000\nDELAY K10\nFEND\nP0\nLD X20\nSPD X0 K5 D0\nSRET\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X20"), true);
    pulse(&mut s, 100);
    pulse(&mut s, 1100);
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.high_speed.speed_count(), 1);
    assert_eq!(s.memory.double_word(d("D2")), 2);
    assert_eq!(s.memory.word(d("D4")), 3);
}
#[test]
fn spd_rejects_counter_owned_pin_in_actual_program_without_stopping_owner_count() {
    let p = compile_il("LD M1000\nDCNT C235 K10\nSPD X0 K2 D0\nDELAY K10\nEND").unwrap();
    let mut s = Simulator::default();
    pulse(&mut s, 100);
    s.scan(&p, 0);
    assert_eq!(s.memory.high_speed.speed_count(), 0);
    assert_eq!(s.memory.double_word(d("C235")), 1);
    assert!(
        s.memory
            .execution_limitations
            .iter()
            .any(|v| v.contains("SPD") && v.contains("kullanılıyor"))
    );
}
#[test]
fn registered_external_irq_owns_spd_pin_even_while_dispatch_is_disabled() {
    let p =
        compile_il("DI\nLD M1000\nSPD X0 K2 D0\nFEND\nI001\nLD M1000\nINC D20\nIRET\nEND").unwrap();
    let mut s = Simulator::default();
    s.scan(&p, 0);
    assert_eq!(s.memory.high_speed.speed_count(), 0);
    assert!(
        s.memory
            .execution_limitations
            .iter()
            .any(|v| v.contains("SPD"))
    );
    assert_eq!(s.memory.word(d("D20")), 0);
}
#[test]
fn dynamic_negative_fullwidth_threshold_is_read_at_visit_and_count_matches_signed32() {
    let p=compile_il("LD M1002\nDMOV K-70000 D0\nDMOV K-70001 C235\nLD M1000\nDCNT C235 K10\nDHSCS D0 C235 M0\nDELAY K10\nEND").unwrap();
    let mut s = Simulator::default();
    pulse(&mut s, 100);
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.double_word(d("C235")), -70000);
    assert!(s.memory.bit(d("M0")));
}
#[test]
fn comparison_counter_reset_is_real_and_does_not_disarm_the_count_backend() {
    let p = compile_il("LD M1000\nDCNT C235 K10\nDHSCR K2 C235 C235\nDELAY K10\nEND").unwrap();
    let mut s = Simulator::default();
    for t in [100, 200, 300, 400] {
        pulse(&mut s, t);
    }
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.double_word(d("C235")), 0);
    assert!(!s.memory.bit(d("C235")));
    assert!(s.memory.counters.active_counters().any(|v| v == 235));
}
#[test]
fn all_four_nominal_forms_have_explicit_native_guard_not_noop_parity() {
    for instruction in [
        "DHSCS K1 C235 M0",
        "DHSCR K1 C235 M0",
        "DHSZ K1 K2 C235 Y0",
        "SPD X0 K1 D0",
    ] {
        let p = compile_il(&format!("LD M1000\n{instruction}\nEND")).unwrap();
        let e = crate::es2_encoder::encode(&p).unwrap_err();
        assert!(e.contains("DHS") || e.contains("SPD"), "{e}");
    }
}
#[test]
fn shipped_example_runs_counter_comparison_zone_and_actual_irq_chain() {
    let p = compile_il(include_str!("../examples/high-speed-compare.il")).unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X20"), true);
    for t in [100, 200, 300] {
        pulse(&mut s, t);
    }
    scan(&mut s, &p, 0);
    assert!(s.memory.bit(d("M0")));
    assert!(s.memory.bit(d("Y1")));
    assert_eq!(s.memory.word(d("D20")), 1);
    assert_eq!(s.memory.double_word(d("D22")), 3);
}
