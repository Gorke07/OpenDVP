//! End-to-end address-expression tests with literal independent memory results.
use crate::plc::{Action, Device, Program, ScanStatus, Simulator, compile_il};

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn scan(sim: &mut Simulator, program: &Program) {
    sim.scan(program, 0);
    assert_eq!(sim.last_scan, ScanStatus::Complete);
}
fn enabled() -> Simulator {
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim
}

#[test]
fn pdf_index_example_uses_current_offsets_and_retains_source_addresses() {
    let program = compile_il("LD X0\nMOV K8 E0\nMOV K14 F0\nMOV D5E0 D10F0\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_word(d("D13"), 12345);
    sim.memory.set_word(d("D5"), -5);
    sim.memory.set_word(d("D10"), -10);
    scan(&mut sim, &program);
    assert_eq!(sim.memory.word(d("D24")), 12345);
    assert_eq!(sim.memory.word(d("D5")), -5);
    assert_eq!(sim.memory.word(d("D10")), -10);
    let action = &program.networks[0].actions[2];
    assert_eq!(action.label(), "MOV D5E0 D10F0");
    let Action::IndexedScalar(indexed) = action else {
        panic!("typed modified action");
    };
    assert_eq!(indexed.display_value(&sim.memory), "12345");
    for address in ["D13", "D24", "D1028", "D1029"] {
        assert!(
            sim.observed_devices.contains(&d(address)) || program.devices.contains(&d(address)),
            "{address}"
        );
    }
}

#[test]
fn each_visit_resolves_offsets_written_by_earlier_actions_in_the_same_scan() {
    let program =
        compile_il("LD X0\nMOV K1 E1\nMOV D100E1 D200E1\nINC E1\nMOV D100E1 D200E1\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_word(d("D101"), 11);
    sim.memory.set_word(d("D102"), 22);
    scan(&mut sim, &program);
    assert_eq!(sim.memory.word(d("D201")), 11);
    assert_eq!(sim.memory.word(d("D202")), 22);
    assert_eq!(sim.memory.word(d("D1182")), 2);
}

#[test]
fn double_data_still_uses_a_single_signed_f_offset_word() {
    let program = compile_il("LD X0\nDMOV D100F0 D200E0\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_word(d("D1028"), 4);
    sim.memory.set_word(d("D1029"), 3);
    sim.memory.set_word(d("D1030"), 30000);
    sim.memory.set_double_word(d("D103"), 0x89ab_cdefu32 as i32);
    scan(&mut sim, &program);
    assert_eq!(sim.memory.double_word(d("D204")), 0x89ab_cdefu32 as i32);
    assert_eq!(sim.memory.word(d("D1030")), 30000);
}

#[test]
fn disabled_and_held_pulse_do_not_resolve_invalid_offsets_but_rearm_does() {
    let program = compile_il("LD X0\nMOVP D100E0 D200F0\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_word(d("D1028"), -30000);
    sim.memory.set_word(d("D1029"), -30000);
    scan(&mut sim, &program);
    assert!(sim.memory.execution_limitations.is_empty());
    sim.memory.set_word(d("D1028"), 1);
    sim.memory.set_word(d("D1029"), 2);
    sim.memory.set_word(d("D101"), 37);
    sim.memory.set_input(d("X0"), true);
    scan(&mut sim, &program);
    assert_eq!(sim.memory.word(d("D202")), 37);
    sim.memory.set_word(d("D1028"), -30000);
    sim.memory.set_word(d("D1029"), -30000);
    scan(&mut sim, &program);
    assert!(sim.memory.execution_limitations.is_empty());
    sim.memory.set_input(d("X0"), false);
    scan(&mut sim, &program);
    assert!(sim.memory.execution_limitations.is_empty());
    sim.memory.set_input(d("X0"), true);
    scan(&mut sim, &program);
    assert!(!sim.memory.execution_limitations.is_empty());
    assert!(!sim.memory.system_bit(1067));
    assert_eq!(sim.memory.word(d("D202")), 37);
}

#[test]
fn wide_output_resolves_once_even_when_low_word_overwrites_its_index() {
    let program = compile_il("LD X0\nDMOV H00070005 D1027E0\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_word(d("D1028"), 1);
    sim.memory.set_word(d("D1033"), 0x55aa);
    scan(&mut sim, &program);
    assert_eq!(sim.memory.word(d("D1028")), 5);
    assert_eq!(sim.memory.word(d("D1029")), 7);
    assert_eq!(sim.memory.word(d("D1033")), 0x55aa);
}

#[test]
fn packed_modifiers_count_bits_and_track_only_effective_width() {
    let program = compile_il("LD X0\nMOV H5 K1M100E1\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_word(d("D1182"), 3);
    sim.memory.set_internal(d("M102"), true);
    sim.memory.set_internal(d("M107"), true);
    scan(&mut sim, &program);
    assert_eq!(
        ["M103", "M104", "M105", "M106"].map(|s| sim.memory.bit(d(s))),
        [true, false, true, false]
    );
    assert!(sim.memory.bit(d("M102")) && sim.memory.bit(d("M107")));
    for address in ["M103", "M104", "M105", "M106"] {
        assert!(sim.observed_devices.contains(&d(address)));
    }
}

#[test]
fn loop_visits_reuse_action_identity_but_resolve_each_new_offset() {
    let program =
        compile_il("LD X0\nMOV K0 E1\nFOR K3\nMOV D100E1 D200E1\nINC E1\nNEXT\nEND").unwrap();
    let mut sim = enabled();
    for (address, value) in [("D100", 17), ("D101", 29), ("D102", 43)] {
        sim.memory.set_word(d(address), value);
    }
    scan(&mut sim, &program);
    assert_eq!(
        ["D200", "D201", "D202"].map(|s| sim.memory.word(d(s))),
        [17, 29, 43]
    );
}

#[test]
fn scalar_opt_in_does_not_widen_other_instruction_roles() {
    for body in [
        "BMOV D0 D10 D20E0",
        "XCH X0E0 D10",
        "MOV K1E0 D10",
        "MOV E0F0 D10",
        "PLS M0E0",
    ] {
        assert!(
            compile_il(&format!("LD X0\n{body}\nEND")).is_err(),
            "{body}"
        );
    }
}
