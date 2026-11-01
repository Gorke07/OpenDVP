//! Compiled IL/scan integration for the explicitly local DCNT backend.
use crate::counter::CounterModel;
use crate::plc::{self, Action, Device, Simulator};
fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn compile(raw: &str) -> plc::Program {
    plc::compile_il(raw).unwrap()
}
fn input(s: &mut Simulator, p: &str, on: bool) {
    s.memory.set_input(d(p), on);
}
fn edge(s: &mut Simulator, at: u64, p: &str, on: bool) {
    s.memory.timing_io.enqueue_input_edge(at, d(p), on).unwrap();
}
fn pulse(s: &mut Simulator, at: u64, p: &str) {
    edge(s, at, p, true);
    edge(s, at + 1, p, false);
}
fn clean(s: &Simulator) {
    assert!(
        s.memory.execution_limitations.is_empty(),
        "{:?}",
        s.memory.execution_limitations
    );
}

#[test]
fn dcnt_is_typed_six_word_genuine_encoding_with_physical_classes_still_guarded() {
    let p = compile("LD X0\nDCNT C200 K70000\nEND");
    let Action::DoubleCounter(a) = &p.networks[0].actions[0] else {
        panic!("typed DCNT missing")
    };
    assert_eq!(a.header(), 0x0600);
    assert_eq!(p.native_program_words, 8);
    assert_eq!(
        crate::es2_encoder::encode(&p).unwrap().words,
        [
            0x0024, 0x0600, 0xc883, 0x7080, 0x1180, 0x0180, 0x0080, 0x0f00
        ]
    );
    let physical = compile("LD X0\nDCNT C235 K70000\nEND");
    assert!(
        crate::es2_encoder::encode(&physical)
            .unwrap_err()
            .contains("C235..C254")
    );
    let mut m = plc::Memory::default();
    m.set_double_word(d("C200"), -100_001);
    assert_eq!(a.display_value(&m), "-100001");
}
#[test]
fn compiled_general_counter_continues_past_preset_and_rearms_only_on_false() {
    let p = compile("LD X0\nDCNT C200 K2\nLD M1000\nDMOV C200 D20\nLD C200\nOUT Y0\nEND");
    let mut s = Simulator::default();
    input(&mut s, "X0", true);
    s.scan(&p, 1);
    s.scan(&p, 1);
    assert_eq!(s.memory.double_word(d("D20")), 1);
    assert!(!s.memory.bit(d("Y0")));
    for expected in [2, 3] {
        input(&mut s, "X0", false);
        s.scan(&p, 1);
        input(&mut s, "X0", true);
        s.scan(&p, 1);
        assert_eq!(s.memory.double_word(d("D20")), expected);
        assert!(s.memory.bit(d("Y0")));
    }
    clean(&s);
}
#[test]
fn compiled_dynamic_dword_preset_preserves_more_than_sixteen_bits() {
    let p = compile(
        "LD M1002\nDMOV K70000 D0\nDMOV K69999 C200\nLD X0\nDCNT C200 D0\nLD M1000\nDMOV C200 D20\nLD C200\nOUT Y0\nEND",
    );
    let mut s = Simulator::default();
    input(&mut s, "X0", true);
    s.scan(&p, 1);
    assert_eq!(s.memory.double_word(d("D20")), 70000);
    assert!(s.memory.bit(d("Y0")));
    s.memory.set_double_word(d("D0"), 70002);
    input(&mut s, "X0", false);
    s.scan(&p, 1);
    input(&mut s, "X0", true);
    s.scan(&p, 1);
    assert_eq!(s.memory.double_word(d("C200")), 70001);
    assert!(!s.memory.bit(d("Y0")));
    clean(&s);
}
#[test]
fn compiled_negative_direction_wrap_is_signed32_not_saturation() {
    let p = compile(
        "LD M1002\nDMOV K-2147483648 C200\nLD M1000\nSET M1200\nLD X0\nDCNT C200 K0\nLD M1000\nDMOV C200 D20\nEND",
    );
    let mut s = Simulator::default();
    input(&mut s, "X0", true);
    s.scan(&p, 1);
    assert_eq!(s.memory.double_word(d("D20")), i32::MAX);
    assert!(s.memory.bit(d("C200")));
    clean(&s);
}
#[test]
fn compiled_rst_same_scan_inhibits_new_edge_and_preserves_held_edge_next_scan() {
    let p = compile("LD X1\nRST C200\nLD X0\nDCNT C200 K-5\nEND");
    let mut s = Simulator::default();
    input(&mut s, "X0", true);
    input(&mut s, "X1", true);
    s.scan(&p, 1);
    assert_eq!(s.memory.double_word(d("C200")), 0);
    assert!(!s.memory.bit(d("C200")));
    input(&mut s, "X1", false);
    s.scan(&p, 1);
    assert_eq!(s.memory.double_word(d("C200")), 0);
    assert!(!s.memory.bit(d("C200")));
    input(&mut s, "X0", false);
    s.scan(&p, 1);
    input(&mut s, "X0", true);
    s.scan(&p, 1);
    assert_eq!(s.memory.double_word(d("C200")), 1);
    assert!(s.memory.bit(d("C200")));
    clean(&s);
}
#[test]
fn queued_edge_during_delay_updates_counter_before_later_action_but_not_input_image() {
    let p = compile(
        "LD X20\nDCNT C235 K1\nLD M1000\nDELAY K10\nDMOV C235 D0\nLD X0\nOUT Y0\nLD C235\nOUT Y1\nEND",
    );
    let mut s = Simulator::default();
    input(&mut s, "X20", true);
    edge(&mut s, 250, "X0", true);
    s.scan(&p, 0);
    assert_eq!(s.memory.double_word(d("D0")), 1);
    assert!(s.memory.bit(d("Y1")));
    assert!(!s.memory.bit(d("Y0")));
    assert!(s.memory.timing_io.physical_inputs[&d("X0")]);
    s.scan(&p, 0);
    assert!(s.memory.bit(d("Y0")));
    assert_eq!(s.memory.double_word(d("C235")), 1);
    clean(&s);
}
#[test]
fn disabled_highspeed_counter_ignores_queued_pulses_then_resumes_without_enable_count() {
    let p = compile("LD X20\nDCNT C235 K1\nEND");
    let mut s = Simulator::default();
    input(&mut s, "X20", true);
    pulse(&mut s, 250, "X0");
    s.scan(&p, 1);
    assert_eq!(s.memory.double_word(d("C235")), 1);
    input(&mut s, "X20", false);
    pulse(&mut s, 1250, "X0");
    s.scan(&p, 1);
    assert_eq!(s.memory.double_word(d("C235")), 1);
    input(&mut s, "X20", true);
    s.scan(&p, 0);
    assert_eq!(s.memory.double_word(d("C235")), 1);
    pulse(&mut s, 2250, "X0");
    s.scan(&p, 1);
    assert_eq!(s.memory.double_word(d("C235")), 2);
    clean(&s);
}
#[test]
fn same_compiled_c232_has_explicit_es2_quadrature_and_ss2_general_semantics() {
    let p = compile("LD X20\nDCNT C232 K1\nEND");
    let mut es = Simulator::default();
    let mut ss = Simulator::default();
    ss.memory.counters.model = CounterModel::Ss2;
    input(&mut es, "X20", true);
    input(&mut ss, "X20", true);
    es.scan(&p, 0);
    ss.scan(&p, 0);
    assert_eq!(es.memory.double_word(d("C232")), 0);
    assert_eq!(ss.memory.double_word(d("C232")), 1);
    edge(&mut es, 250, "X0", true);
    es.scan(&p, 1);
    ss.scan(&p, 1);
    assert_eq!(es.memory.double_word(d("C232")), 1);
    assert_eq!(ss.memory.double_word(d("C232")), 1);
    clean(&es);
    clean(&ss);
}
#[test]
fn queued_hardware_direction_input_drives_signed_value_and_monitor() {
    let p = compile("LD X20\nDCNT C245 K1\nEND");
    let mut s = Simulator::default();
    input(&mut s, "X20", true);
    edge(&mut s, 100, "X1", true);
    pulse(&mut s, 250, "X0");
    s.scan(&p, 1);
    assert_eq!(s.memory.double_word(d("C245")), -1);
    assert!(s.memory.system_bit(1245));
    clean(&s);
}
#[test]
fn queued_reset_with_compiled_irq_ownership_captures_before_reset_and_isr() {
    let p = compile(
        "LD X20\nDCNT C248 K1\nEI\nLD M1000\nDELAY K10\nFEND\nI401\nLD M1000\nDMOV D1240 D20\nIRET\nEND",
    );
    let mut s = Simulator::default();
    input(&mut s, "X20", true);
    pulse(&mut s, 100, "X0");
    pulse(&mut s, 200, "X0");
    edge(&mut s, 300, "X4", true);
    s.scan(&p, 0);
    assert_eq!(s.memory.double_word(d("D1240")), 2);
    assert_eq!(s.memory.double_word(d("D20")), 2);
    assert_eq!(s.memory.double_word(d("C248")), 0);
    assert!(!s.memory.bit(d("C248")));
    clean(&s);
}
#[test]
fn compiled_csfo_cannot_steal_an_enabled_counter_input() {
    let p = compile("LD X20\nDCNT C235 K1\nCSFO X0 D100 D110\nEND");
    let mut s = Simulator::default();
    input(&mut s, "X20", true);
    s.memory.set_word(d("D100"), 2);
    s.memory.set_word(d("D110"), 10000);
    s.scan(&p, 1);
    assert!(
        s.memory
            .execution_limitations
            .iter()
            .any(|x| x.contains("CSFO") && x.contains("kanal"))
    );
    assert!(s.memory.counters.active_counters().any(|x| x == 235));
}
#[test]
fn example_general_and_highspeed_counters_are_independent_fullwidth_readbacks() {
    let p = compile(include_str!("../examples/dcnt.il"));
    let mut s = Simulator::default();
    input(&mut s, "X20", true);
    input(&mut s, "X23", true);
    pulse(&mut s, 100, "X0");
    s.scan(&p, 1);
    s.scan(&p, 0);
    assert_eq!(s.memory.double_word(d("D0")), 1);
    assert_eq!(s.memory.double_word(d("D2")), 1);
    input(&mut s, "X22", true);
    pulse(&mut s, 1100, "X0");
    s.scan(&p, 1);
    s.scan(&p, 0);
    assert_eq!(s.memory.double_word(d("D0")), 0);
    assert_eq!(s.memory.double_word(d("D2")), 1);
    clean(&s);
}
