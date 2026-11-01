use super::*;
fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
#[test]
fn call_returns_to_same_rung_and_restores_mps_word_before_fend() {
    let p =
        compile("LD X0\nMPS\nCALL P3\nMPP\nOUT Y0\nFEND\nP3\nLDI X1\nINC D0\nSRET\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim.scan(&p, 100);
    assert_eq!(sim.last_scan, ScanStatus::Complete);
    assert_eq!(sim.scans, 1);
    assert_eq!(sim.memory.word(d("D0")), 1);
    assert!(sim.memory.bit(d("Y0")));
    sim.memory.set_input(d("X0"), false);
    sim.scan(&p, 100);
    assert_eq!(sim.memory.word(d("D0")), 1);
    assert!(!sim.memory.bit(d("Y0")));
}
#[test]
fn each_loop_visit_samples_edges_but_retains_source_occurrence_identity() {
    let p = compile("FOR K3\nLD X0\nINCP D0\nLDP X0\nINC D1\nNEXT\nEND").unwrap();
    let mut sim = Simulator::default();
    for value in [true, true, false, true] {
        sim.memory.set_input(d("X0"), value);
        sim.scan(&p, 100);
        assert_eq!(sim.last_scan, ScanStatus::Complete);
    }
    assert_eq!(sim.memory.word(d("D0")), 2);
    assert_eq!(sim.memory.word(d("D1")), 2);
    assert_eq!(sim.scans, 4);
}
#[test]
fn for_captures_count_once_even_when_body_overwrites_count_register() {
    let p = compile("LD X1\nMOV K3 D0\nFOR D0\nLD X0\nINC D1\nMOV K1 D0\nNEXT\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim.memory.set_input(d("X1"), true);
    sim.scan(&p, 100);
    assert_eq!(sim.last_scan, ScanStatus::Complete);
    assert_eq!(sim.memory.word(d("D1")), 3);
    assert_eq!(sim.memory.word(d("D0")), 1);
}
#[test]
fn independent_budget_stops_backward_jump_without_claiming_completed_scan_or_plc_error() {
    let p = compile("P0\nLDI X0\nINC D0\nCJ P0\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.control.hard_step_limit = 20;
    sim.scan(&p, 100);
    assert!(matches!(sim.last_scan, ScanStatus::Fault(_)));
    assert_eq!(sim.scans, 0);
    assert_eq!(sim.elapsed_ms, 0);
    assert!(sim.memory.word(d("D0")) > 0);
    assert!(!sim.memory.system_bit(1067));
    assert_eq!(sim.control.steps, 20);
    assert!(!sim.memory.execution_limitations.is_empty());
}
#[test]
fn malformed_cfg_is_diagnosed_before_any_original_execution() {
    for source in [
        "P0\nP0\nEND",
        "LD X0\nCJ P3\nEND",
        "NEXT\nEND",
        "FOR K2\nLD X0\nINC D0\nEND",
        "LD X0\nMC N1\nLD X1\nOUT Y0\nMCR N1\nEND",
        "LD X0\nCJ P3\nFOR K2\nP3\nLD X1\nINC D0\nNEXT\nEND",
        "FOR K2\nRET\nNEXT\nEND",
        "IRET\nEND",
    ] {
        assert!(compile(source).is_err(), "{source}");
    }
}
