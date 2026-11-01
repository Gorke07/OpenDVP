use super::*;
fn d(s: &str) -> Device {
    s.parse().unwrap()
}

#[test]
fn atmr_gates_actual_following_actions_and_loop_does_not_multiply_time() {
    let p = compile("FOR K3\nLD X0\nATMR T200 K15\nINC D0\nOUT Y0\nNEXT\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X0"), true);
    s.scan(&p, 100);
    assert_eq!(s.memory.word(d("T200")), 10);
    assert_eq!(s.memory.word(d("D0")), 0);
    assert!(!s.memory.bit(d("Y0")));
    s.scan(&p, 100);
    assert_eq!(s.memory.word(d("T200")), 20);
    assert_eq!(s.memory.word(d("D0")), 0);
    s.scan(&p, 100);
    assert_eq!(s.memory.word(d("T200")), 15);
    assert_eq!(s.memory.word(d("D0")), 3);
    assert!(s.memory.bit(d("Y0")));
    assert_eq!(s.scans, 3);
}

#[test]
fn ordinary_skipped_timer_holds_but_subroutine_timer_keeps_its_enable() {
    let p = compile("LD X1\nCJ P0\nLD X0\nTMR T0 K4\nTMR T184 K4\nP0\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X0"), true);
    s.scan(&p, 100);
    s.memory.set_input(d("X1"), true);
    for _ in 0..3 {
        s.scan(&p, 100);
    }
    assert_eq!(s.memory.word(d("T0")), 1);
    assert!(!s.memory.bit(d("T0")));
    assert_eq!(s.memory.word(d("T184")), 4);
    assert!(s.memory.bit(d("T184")));
}

#[test]
fn accumulative_false_retains_and_reset_clears_timer_before_next_sweep() {
    let p = compile("LD X0\nTMR T240 K20\nLD X1\nRST T240\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X0"), true);
    s.scan(&p, 100);
    assert_eq!(s.memory.word(d("T240")), 10);
    s.memory.set_input(d("X0"), false);
    s.scan(&p, 100);
    assert_eq!(s.memory.word(d("T240")), 10);
    s.memory.set_input(d("X1"), true);
    s.scan(&p, 100);
    assert_eq!(s.memory.word(d("T240")), 0);
    assert!(!s.memory.bit(d("T240")));
}

#[test]
fn special_timer_parser_tracks_implicit_spans_and_rejects_invented_forms() {
    let p = compile(include_str!("../examples/special-timers.il")).unwrap();
    for name in [
        "D0", "D10", "D11", "M20", "M21", "M22", "M23", "T7", "T200", "T240",
    ] {
        assert!(p.devices.contains(&d(name)), "{name}");
    }
    for op in [
        "TTMRP D0 K1",
        "DTTMR D0 K1",
        "STMR T184 K3 M0",
        "STMR T0 K0 M0",
        "TTMR D9999 K0",
        "ATMR T0 K1 Y0",
    ] {
        assert!(compile(&format!("LD X0\n{op}\nEND")).is_err(), "{op}");
    }
}

#[test]
fn timer_reset_inhibits_later_visit_and_false_releases_preserved_enable() {
    let p = compile("LD X1\nRST T184\nLD X0\nTMR T184 K3\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X0"), true);
    s.memory.set_input(d("X1"), true);
    s.scan(&p, 100);
    assert_eq!(s.memory.word(d("T184")), 0);
    assert!(s.memory.special_timers.is_enabled(d("T184")));
    s.memory.set_input(d("X1"), false);
    s.scan(&p, 100);
    assert_eq!(s.memory.word(d("T184")), 1);
}

#[test]
fn counter_false_preserves_contact_written_by_another_instruction() {
    let p = compile("LD X1\nCNT C0 K1\nDIV16 K11 K1 C0\nLD X0\nSTMR T0 K1 M1535\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X1"), true);
    s.scan(&p, 100);
    s.memory.set_input(d("X1"), false);
    s.memory.set_input(d("X0"), true);
    s.scan(&p, 100);
    assert!(!s.memory.bit(d("C0")));
    s.scan(&p, 100);
    assert!(!s.memory.bit(d("C0")));
    assert_eq!(s.memory.word(d("C0")), 11);
}
