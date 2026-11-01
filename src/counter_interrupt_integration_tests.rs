//! Compiled interrupt/reset coupling; explicit local capture policy, not
//! installed simulator/hardware interrupt timing parity.
use crate::plc::{Device, ScanStatus, Simulator, compile_il};
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
fn run(s: &mut Simulator, source: &str) {
    let p = compile_il(source).unwrap();
    s.memory.set_input(d("X20"), true);
    s.scan(&p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(
        s.memory.execution_limitations.is_empty(),
        "{:?}",
        s.memory.execution_limitations
    );
}

#[test]
fn registered_reset_irq_captures_current_value_before_reset_and_isr() {
    let mut s = Simulator::default();
    pulse(&mut s, 100);
    pulse(&mut s, 200);
    edge(&mut s, 300, "X1", true);
    run(
        &mut s,
        "LD X20\nDCNT C243 K1\nEI\nLD M1000\nDELAY K10\nFEND\nI101\nLD M1000\nDMOV D1240 D20\nDMOV C243 D22\nIRET\nEND",
    );
    assert_eq!(s.memory.double_word(d("D20")), 2);
    assert_eq!(s.memory.double_word(d("D22")), 0);
    assert!(!s.memory.bit(d("C243")));
}
#[test]
fn di_delays_isr_but_preserves_physical_reset_capture() {
    let mut s = Simulator::default();
    pulse(&mut s, 100);
    pulse(&mut s, 200);
    edge(&mut s, 300, "X1", true);
    run(
        &mut s,
        "DI\nLD X20\nDCNT C243 K1\nLD M1000\nDELAY K10\nDMOV D1240 D30\nEI\nFEND\nI101\nLD M1000\nDMOV D1240 D20\nDMOV C243 D22\nIRET\nEND",
    );
    assert_eq!(s.memory.double_word(d("D30")), 2);
    assert_eq!(s.memory.double_word(d("D20")), 2);
    assert_eq!(s.memory.double_word(d("D22")), 0);
}
#[test]
fn rising_and_falling_handlers_share_reset_capture_without_losing_an_edge() {
    let mut s = Simulator::default();
    pulse(&mut s, 100);
    edge(&mut s, 250, "X1", true);
    pulse(&mut s, 400);
    edge(&mut s, 550, "X1", false);
    run(
        &mut s,
        "LD X20\nDCNT C243 K1\nEI\nLD M1000\nDELAY K3\nDELAY K3\nFEND\nI101\nLD M1000\nDMOV D1240 D20\nIRET\nI100\nLD M1000\nDMOV D1240 D22\nIRET\nEND",
    );
    assert_eq!(s.memory.double_word(d("D20")), 1);
    assert_eq!(s.memory.double_word(d("D22")), 1);
    assert_eq!(s.memory.double_word(d("C243")), 0);
}
#[test]
fn ei_latched_x4_reversal_changes_reset_edge_and_preserves_capture() {
    let mut s = Simulator::default();
    edge(&mut s, 100, "X4", true);
    pulse(&mut s, 200);
    pulse(&mut s, 300);
    edge(&mut s, 500, "X4", false);
    run(
        &mut s,
        "LD M1000\nSET M1284\nLD X20\nDCNT C248 K1\nEI\nLD M1000\nDELAY K10\nFEND\nI401\nLD M1000\nDMOV D1240 D20\nDMOV C248 D22\nIRET\nEND",
    );
    assert_eq!(s.memory.double_word(d("D20")), 2);
    assert_eq!(s.memory.double_word(d("D22")), 0);
}
#[test]
fn external_mask_delays_dispatch_but_not_registered_reset_capture() {
    let mut s = Simulator::default();
    pulse(&mut s, 100);
    pulse(&mut s, 200);
    edge(&mut s, 300, "X1", true);
    run(
        &mut s,
        "LD M1000\nSET M1051\nLD X20\nDCNT C243 K1\nEI\nLD M1000\nDELAY K10\nDMOV D1240 D30\nRST M1051\nFEND\nI101\nLD M1000\nDMOV D1240 D20\nIRET\nEND",
    );
    assert_eq!(s.memory.double_word(d("D30")), 2);
    assert_eq!(s.memory.double_word(d("D20")), 2);
    assert_eq!(s.memory.double_word(d("C243")), 0);
}

#[test]
fn invalid_watchdog_limit_stops_previous_armed_counter_without_fake_plc_alarm() {
    for limit in [0, -1] {
        let mut s = Simulator::default();
        let p = compile_il("LD X20\nDCNT C235 K5\nEND").unwrap();
        s.memory.set_input(d("X20"), true);
        s.scan(&p, 0);
        assert_eq!(s.last_scan, ScanStatus::Complete);
        s.memory.set_word(d("D1000"), limit);
        s.scan(&p, 0);
        assert!(matches!(s.last_scan, ScanStatus::Fault(_)));
        s.memory.set_input(d("X0"), true);
        assert_eq!(s.memory.double_word(d("C235")), 0);
        assert!(!s.memory.system_bit(1008));
        assert!(s.memory.watchdog.fault.is_none());
    }
}
