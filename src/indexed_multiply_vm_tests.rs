//! Independent end-to-end expectations for full indexed result storage.
use crate::plc::{Action, Device, Program, ScanStatus, Simulator, compile_il};

fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn run(sim: &mut Simulator, program: &Program) {
    sim.scan(program, 0);
    assert_eq!(sim.last_scan, ScanStatus::Complete);
}
fn enabled() -> Simulator {
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim
}

#[test]
fn all_eight_forms_keep_original_lengths_labels_and_signed_results() {
    for (name, header, pair) in [
        ("MUL", 0x3c00, [-51, -1]),
        ("MULP", 0x3c10, [-51, -1]),
        ("DMUL", 0x3d00, [-51, -1]),
        ("DMULP", 0x3d10, [-51, -1]),
        ("DIV", 0x3e00, [-5, -2]),
        ("DIVP", 0x3e10, [-5, -2]),
        ("DDIV", 0x3f00, [-5, -2]),
        ("DDIVP", 0x3f10, [-5, -2]),
    ] {
        let p = compile_il(&format!("LD X0\n{name} D10E1 K3 D100F0\nEND")).unwrap();
        assert!(p.requires_vm());
        let e = crate::es2_encoder::encode(&p).unwrap();
        let pc = usize::from(e.action_pc[0]);
        let wide = name.starts_with('D') && name != "DIV" && name != "DIVP";
        assert_eq!(e.words[pc], header);
        assert_eq!(e.words.len() - pc - 1, if wide { 13 } else { 7 });
        assert_eq!(
            p.networks[0].actions[0].label(),
            format!("{name} D10E1 K3 D100F0")
        );
        let mut sim = enabled();
        sim.memory.set_word(d("D1182"), 2);
        sim.memory.set_word(d("D1029"), 3);
        if wide {
            sim.memory.set_double_word(d("D12"), -17);
        } else {
            sim.memory.set_word(d("D12"), -17);
        }
        sim.memory.set_system(1020, true);
        sim.memory.set_system(1022, true);
        run(&mut sim, &p);
        let actual = if wide {
            [
                sim.memory.double_word(d("D103")),
                sim.memory.double_word(d("D105")),
            ]
        } else {
            [
                i32::from(sim.memory.word(d("D103"))),
                i32::from(sim.memory.word(d("D104"))),
            ]
        };
        assert_eq!(actual, pair, "{name}");
        assert!(sim.memory.system_bit(1020) && sim.memory.system_bit(1022));
        assert!(sim.observed_devices.contains(&d("D103")));
        assert!(!p.devices.contains(&d("D100")));
    }
}

#[test]
fn zero_divisor_reads_sources_but_never_resolves_dynamic_destination() {
    for (name, wide, offset) in [("DIV", false, 5), ("DDIV", true, 9)] {
        let p = compile_il(&format!("LD X0\n{name} D10E0 D20E0 D100F0\nEND")).unwrap();
        let mut sim = enabled();
        sim.memory.set_word(d("D1028"), 1);
        sim.memory.set_word(d("D1029"), -30000);
        if wide {
            sim.memory.set_double_word(d("D11"), 17);
        } else {
            sim.memory.set_word(d("D11"), 17);
        }
        run(&mut sim, &p);
        assert!(sim.memory.execution_limitations.is_empty());
        assert_eq!(sim.memory.word(d("D1067")), 0xe19);
        assert_eq!(
            sim.memory.word(d("D1068")),
            (p.native_action_pc[0] + offset) as i16
        );
        assert!(sim.observed_devices.contains(&d("D11")));
        assert!(sim.observed_devices.contains(&d("D21")));
        assert!(!sim.observed_devices.contains(&d("D100")));
        // Independent sticky channels remain untouched on subsequent errors.
        sim.memory.set_word(d("D1067"), 0x777);
        sim.memory.set_word(d("D1068"), 123);
        run(&mut sim, &p);
        assert_eq!(sim.memory.word(d("D1067")), 0x777);
        assert_eq!(sim.memory.word(d("D1068")), 123);
    }
}

#[test]
fn source_failure_precedes_zero_divisor_without_fabricated_native_error() {
    let p = compile_il("LD X0\nDIV D10E0 K0 D100F0\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_word(d("D1028"), -30000);
    run(&mut sim, &p);
    assert!(!sim.memory.execution_limitations.is_empty());
    assert!(!sim.memory.system_bit(1067));
}

#[test]
fn disabled_and_held_pulse_skip_all_address_resolution() {
    for name in ["MULP", "DMULP", "DIVP", "DDIVP"] {
        let p = compile_il(&format!("LD X0\n{name} D10E0 K3 D100F0\nEND")).unwrap();
        let mut sim = Simulator::default();
        sim.memory.set_word(d("D1028"), -30000);
        sim.memory.set_word(d("D1029"), -30000);
        run(&mut sim, &p);
        assert!(sim.memory.execution_limitations.is_empty());
        sim.memory.set_word(d("D1028"), 0);
        sim.memory.set_word(d("D1029"), 0);
        sim.memory.set_double_word(d("D10"), 12);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &p);
        let old = sim.memory.double_word(d("D100"));
        sim.memory.set_word(d("D1028"), -30000);
        sim.memory.set_word(d("D1029"), -30000);
        run(&mut sim, &p);
        assert!(sim.memory.execution_limitations.is_empty());
        assert_eq!(sim.memory.double_word(d("D100")), old);
        sim.memory.set_input(d("X0"), false);
        run(&mut sim, &p);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &p);
        assert!(!sim.memory.execution_limitations.is_empty());
    }
}

#[test]
fn full_wide_result_keeps_one_destination_when_it_overwrites_its_own_index() {
    let p = compile_il("LD X0\nDMUL H10002 H10003 D1027E0\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_word(d("D1028"), 1);
    sim.memory.set_word(d("D1033"), 0x5a5a);
    run(&mut sim, &p);
    assert_eq!(
        ["D1028", "D1029", "D1030", "D1031"].map(|s| sim.memory.word(d(s))),
        [6, 5, 1, 0]
    );
    assert_eq!(sim.memory.word(d("D1033")), 0x5a5a);
    for s in ["D1028", "D1029", "D1030", "D1031"] {
        assert!(sim.observed_devices.contains(&d(s)));
    }
}

#[test]
fn both_sources_are_snapshotted_before_overlapping_result_writes() {
    let p = compile_il("LD X0\nDDIV D10E0 D12E0 D11E0\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_double_word(d("D10"), -100000);
    sim.memory.set_double_word(d("D12"), 7);
    run(&mut sim, &p);
    assert_eq!(sim.memory.double_word(d("D11")), -14285);
    assert_eq!(sim.memory.double_word(d("D13")), -5);
}

#[test]
fn narrow_bit_results_are_two_chunks_of_the_arithmetic_results() {
    for (name, expected) in [("MUL", 0xfd), ("DIV", 0xeb)] {
        let p = compile_il(&format!("LD X0\n{name} K-17 K3 K1M100E0\nEND")).unwrap();
        let mut sim = enabled();
        sim.memory.set_word(d("D1028"), 3);
        run(&mut sim, &p);
        let actual = (0..8).fold(0u16, |bits, i| {
            bits | (u16::from(sim.memory.bit(d(&format!("M{}", 103 + i)))) << i)
        });
        assert_eq!(actual, expected);
        assert!(sim.observed_devices.contains(&d("M110")));
    }
}

#[test]
fn dynamic_full_span_checks_allow_negative_offsets_to_rescue_end_addresses() {
    for (name, base, offset, valid) in [
        ("MUL", "D4999E0", 0, false),
        ("MUL", "D4999E0", -1, true),
        ("DMUL", "D4999E0", -2, false),
        ("DMUL", "D4999E0", -3, true),
    ] {
        let p = compile_il(&format!("LD X0\n{name} K3 K7 {base}\nEND")).unwrap();
        let mut sim = enabled();
        sim.memory.data_register_limit = Some(4999);
        sim.memory.set_word(d("D1028"), offset);
        run(&mut sim, &p);
        assert_eq!(
            sim.memory.execution_limitations.is_empty(),
            valid,
            "{name} {offset}"
        );
        if !valid {
            assert_eq!(sim.memory.word(d("D4997")), 0);
        }
    }
}

#[test]
fn natural_counter_result_crossing_uses_effective_address_storage() {
    for (base, offset, effective) in [("C199E0", 1, "C200"), ("C200E0", -1, "C199")] {
        let p = compile_il(&format!("LD X0\nMUL K30000 K3 {base}\nEND")).unwrap();
        let mut sim = enabled();
        sim.memory.set_word(d("D1028"), offset);
        sim.memory.set_double_word(d("C200"), 0x12345678);
        run(&mut sim, &p);
        assert_eq!(
            sim.memory.double_word(d(effective)),
            if effective == "C200" {
                90000
            } else {
                90000u32 as u16 as i32
            }
        );
        if effective == "C199" {
            assert_eq!(sim.memory.double_word(d("C200")), 0x12345678);
        }
    }
}

#[test]
fn loop_revisits_use_current_index_and_keep_typed_labels() {
    let p = compile_il("LD X0\nFOR K3\nMUL D10E0 K2 D100E0\nINC E0\nNEXT\nEND").unwrap();
    let mut sim = enabled();
    for (s, v) in [("D10", 2), ("D11", 4), ("D12", 6)] {
        sim.memory.set_word(d(s), v);
    }
    run(&mut sim, &p);
    assert_eq!(
        ["D100", "D101", "D102", "D103"].map(|s| sim.memory.word(d(s))),
        [4, 8, 12, 0]
    );
    assert!(
        p.networks
            .iter()
            .flat_map(|n| &n.actions)
            .any(|a| matches!(a, Action::IndexedMultiply(_)))
    );
}
