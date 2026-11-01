use crate::plc::{Device, Memory, Simulator, compile_il};
use crate::word_counter::{WordCounterAction, WordCounterState};
fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn counter(s: &str) -> WordCounterAction {
    WordCounterAction::parse(&s.split_whitespace().map(str::to_owned).collect::<Vec<_>>()).unwrap()
}
#[test]
fn counter_compiler_literal_vectors_and_operand_roles() {
    for (source, words) in [
        ("CNT C199 K0", vec![0x0500, 0xc783, 0x0080, 0x0080]),
        ("CNT C112 D10", vec![0x0500, 0x7083, 0x1486, 0x0086]),
        ("CNT C0 H7FFF", vec![0x0500, 0x0083, 0xff80, 0x7f80]),
        ("RST D10", vec![0x0700, 0x1486, 0x0086]),
        ("RST E1", vec![0x0700, 0x6c86, 0x0180]),
        ("RST F1", vec![0x0700, 0x6e86, 0x0180]),
    ] {
        let p = compile_il(&format!("LD X0\n{source}\nEND")).unwrap();
        let e = crate::es2_encoder::encode(&p).unwrap();
        assert_eq!(&e.words[1..e.words.len() - 1], words, "{source}");
    }
    for source in [
        "CNT C200 K1",
        "CNT C0 K-1",
        "CNT C0 K32768",
        "CNT C0 E1",
        "CNT C0 H8000",
        "CNT C0 HFFFF",
        "CNT C0 H-1",
        "CNT C0 D0E1",
        "CNTP C0 K1",
        "RSTP D0",
        "RST D0E1",
        "RST K1M0",
        "CNT C0 K1\nCNT C0 K2",
    ] {
        assert!(
            compile_il(&format!("LD X0\n{source}\nEND")).is_err(),
            "{source}"
        );
    }
}
#[test]
fn counter_done_is_latched_across_changed_preset_and_count_writes() {
    let a = counter("C112 D10");
    let mut m = Memory::default();
    m.set_word(d("D10"), 1);
    a.execute(&mut m, true, 9);
    assert_eq!(m.word(d("C112")), 1);
    assert!(m.bit(d("C112")));
    m.set_word(d("D10"), 100);
    m.set_word(d("C112"), 20);
    a.execute(&mut m, false, 9);
    a.execute(&mut m, true, 9);
    assert_eq!(m.word(d("C112")), 20);
    assert!(m.bit(d("C112")));
}
#[test]
fn counter_wraps_then_compares_signed_and_zero_negative_clamp() {
    for preset in [0, -1, -32768] {
        let a = counter("C199 D10");
        let mut m = Memory::default();
        m.set_word(d("D10"), preset);
        a.execute(&mut m, true, 1);
        assert_eq!(m.word(d("C199")), preset);
        assert!(m.bit(d("C199")));
    }
    let a = counter("C199 K32767");
    let mut m = Memory::default();
    m.set_word(d("C199"), 32767);
    a.execute(&mut m, true, 1);
    assert_eq!(m.word(d("C199")), -32768);
    assert!(!m.bit(d("C199")));
}
#[test]
fn counter_history_is_pc_scoped_and_inhibition_is_cleared_by_false_rst() {
    let a = counter("C0 K5");
    let mut m = Memory::default();
    WordCounterState::visit_reset(&mut m, d("C0"), true);
    a.execute(&mut m, true, 10);
    WordCounterState::visit_reset(&mut m, d("C0"), false);
    a.execute(&mut m, true, 10);
    assert_eq!(m.word(d("C0")), 0); // inhibited visit still sampled the edge
    a.execute(&mut m, true, 20);
    assert_eq!(m.word(d("C0")), 1); // distinct instruction site
    a.execute(&mut m, false, 10);
    a.execute(&mut m, true, 10);
    assert_eq!(m.word(d("C0")), 2);
}
#[test]
fn numeric_reset_clears_only_the_target_word_and_obeys_power() {
    for (raw, target) in [("D10", "D10"), ("E1", "D1182"), ("F1", "D1183")] {
        let p = compile_il(&format!("LD X0\nRST {raw}\nEND")).unwrap();
        let mut s = Simulator::default();
        let target = d(target);
        let next = Device {
            index: target.index + 1,
            ..target
        };
        s.memory.set_word(target, 123);
        s.memory.set_word(next, 456);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(target), 123);
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(target), 0);
        assert_eq!(s.memory.word(next), 456);
    }
}
#[test]
fn counter_inhibit_survives_skipped_reset_across_scans() {
    let p = compile_il("LD X1\nCJ P0\nLD X2\nRST C0\nP0\nLD X0\nCNT C0 K5\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X2"), true);
    s.scan(&p, 100); // reset, CNT false
    s.memory.set_input(d("X1"), true); // skip RST, retain inhibition
    s.memory.set_input(d("X0"), true);
    s.scan(&p, 100);
    assert_eq!(s.memory.word(d("C0")), 0);
    s.memory.set_input(d("X1"), false);
    s.memory.set_input(d("X2"), false);
    s.memory.set_input(d("X0"), false);
    s.scan(&p, 100); // visited false RST releases inhibition
    s.memory.set_input(d("X0"), true);
    s.scan(&p, 100);
    assert_eq!(s.memory.word(d("C0")), 1);
}
#[test]
fn indexed_out_clears_current_target_only_and_set_false_does_not_resolve() {
    let p = compile_il("LD X0\nOUT Y0E1\nSET M0F1\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_word(d("D1182"), 1);
    s.memory.set_word(d("D1183"), 0);
    s.memory.set_input(d("X0"), true);
    s.scan(&p, 100);
    assert!(s.memory.bit(d("Y1")));
    s.memory.set_word(d("D1182"), 2);
    s.memory.set_word(d("D1183"), -1);
    s.memory.set_input(d("X0"), false);
    s.scan(&p, 100);
    assert!(s.memory.bit(d("Y1")));
    assert!(!s.memory.bit(d("Y2")));
    assert!(s.observed_devices.contains(&d("Y1")));
    assert!(s.observed_devices.contains(&d("Y2")));
    assert!(s.memory.execution_limitations.is_empty());
}
#[test]
fn indexed_contacts_are_eager_and_invalid_inv_never_turns_on_output() {
    for expr in ["LD X0\nAND X0E1", "LDI X0\nOR X0E1", "LD X0E1\nINV"] {
        let p = compile_il(&format!("{expr}\nOUT Y0\nEND")).unwrap();
        let mut s = Simulator::default();
        s.memory.set_word(d("D1182"), -1);
        s.scan(&p, 100);
        assert!(!s.memory.execution_limitations.is_empty(), "{expr}");
        assert!(!s.memory.bit(d("Y0")), "{expr}");
    }
}
#[test]
fn indexed_contact_np_history_belongs_to_occurrence_not_resolved_address() {
    let p = compile_il("LD X0E1\nNP\nOUT Y0\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X0"), true);
    s.memory.set_input(d("X1"), true);
    s.scan(&p, 100);
    assert!(s.memory.bit(d("Y0")));
    s.memory.set_word(d("D1182"), 1);
    s.scan(&p, 100);
    assert!(!s.memory.bit(d("Y0")));
}
#[test]
fn indirect_clock_guard_runs_at_actual_vm_read_and_numeric_reset_respects_step_context() {
    let p = compile_il("LDI M1008E1\nINV\nOUT Y0\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory
        .system_relays
        .set_profile(crate::system_relays::ClockProfile::InstalledFixed);
    s.memory.set_word(d("D1182"), 3);
    s.scan(&p, 100);
    assert!(!s.memory.execution_limitations.is_empty());
    assert!(!s.memory.bit(d("Y0")));
    let p = compile_il("STL S0\nLD X0\nRST D40\nRST E1\nRET\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X0"), true);
    s.memory.set_word(d("D40"), 123);
    s.memory.set_word(d("D1182"), 456);
    s.scan(&p, 100);
    assert!(s.memory.execution_limitations.is_empty());
    assert_eq!(s.memory.word(d("D40")), 123);
    assert_eq!(s.memory.word(d("D1182")), 456);
}
#[test]
fn indexed_writers_use_output_context_and_raw_s_bits_not_master_or_sfc_transitions() {
    for (op, before, expected) in [
        ("OUT", false, true),
        ("SET", false, true),
        ("RST", true, false),
    ] {
        let p = compile_il(&format!("LD X0\nMC N0\nLDI X7\n{op} M160E1\nMCR N0\nEND")).unwrap();
        let mut s = Simulator::default();
        s.memory.set_internal(d("M160"), before);
        s.scan(&p, 100);
        assert_eq!(
            s.memory.bit(d("M160")),
            expected,
            "{op} ignores master gate"
        );
        for inhibit in [false, true] {
            let p = compile_il(&format!("STL S0\nLDI X7\n{op} S160E1\nRET\nEND")).unwrap();
            let mut s = Simulator::default();
            s.memory.set_internal(d("S0"), true);
            s.memory.set_internal(d("M1040"), inhibit);
            s.memory.set_internal(d("S160"), before);
            s.scan(&p, 100); // previous-step snapshot takes effect after first END
            s.scan(&p, 100);
            assert!(s.memory.bit(d("S0")), "{op} must not clear predecessor");
            assert_eq!(s.memory.bit(d("S160")), expected, "{op} ignores M1040");
        }
    }
    let p = compile_il("STL S0\nLDI X7\nOUT M160E1\nRET\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_internal(d("M160"), true);
    s.scan(&p, 100);
    assert!(
        !s.memory.bit(d("M160")),
        "disabled output context clears indexed target"
    );
}
