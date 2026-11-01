//! End-to-end single-result arithmetic expectations independent of encoding.
use crate::plc::{Action, Device, Program, ScanStatus, Simulator, compile_il};

fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn enabled() -> Simulator {
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim
}
fn run(sim: &mut Simulator, p: &Program) {
    sim.scan(p, 0);
    assert_eq!(sim.last_scan, ScanStatus::Complete);
}

#[test]
fn eight_forms_keep_single_result_width_headers_and_quotient_sign() {
    for (name, wide, header, a, b, result) in [
        ("MUL16", false, 0xf400, 300, 300, 24464),
        ("MUL16P", false, 0xf410, 300, 300, 24464),
        ("MUL32", true, 0xf500, 100000, 100000, 1410065408),
        ("MUL32P", true, 0xf510, 100000, 100000, 1410065408),
        ("DIV16", false, 0xf600, -17, 3, -5),
        ("DIV16P", false, 0xf610, -17, 3, -5),
        ("DIV32", true, 0xf700, -700000001, 19, -36842105),
        ("DIV32P", true, 0xf710, -700000001, 19, -36842105),
    ] {
        let p = compile_il(&format!("LD X0\n{name} D10E1 K{b} D100F0\nEND")).unwrap();
        assert!(p.requires_vm());
        let e = crate::es2_encoder::encode(&p).unwrap();
        let pc = usize::from(e.action_pc[0]);
        assert_eq!(e.words[pc], header);
        assert_eq!(e.words.len() - pc - 1, if wide { 13 } else { 7 });
        let mut sim = enabled();
        sim.memory.set_word(d("D1182"), 1);
        sim.memory.set_word(d("D1029"), 2);
        if wide {
            sim.memory.set_double_word(d("D11"), a);
        } else {
            sim.memory.set_word(d("D11"), a as i16);
        }
        let guard = if wide { "D104" } else { "D103" };
        sim.memory.set_word(d(guard), 0x55aa);
        sim.memory.set_system(1020, true);
        sim.memory.set_system(1021, true);
        run(&mut sim, &p);
        let actual = if wide {
            sim.memory.double_word(d("D102"))
        } else {
            i32::from(sim.memory.word(d("D102")))
        };
        assert_eq!(actual, result, "{name}");
        assert_eq!(
            sim.memory.word(d(guard)),
            0x55aa,
            "{name} must not write a second result"
        );
        assert!(sim.memory.system_bit(1020) && sim.memory.system_bit(1021));
        assert_eq!(sim.memory.system_bit(1022), name.starts_with("MUL"));
        assert!(!p.devices.contains(&d("D100")));
        assert!(sim.observed_devices.contains(&d("D102")));
    }
}

#[test]
fn multiply_carry_gates_numeric_word_only_and_clears_before_a_fit_result() {
    for (name, a, b) in [("MUL16", 300, 300), ("MUL32", 100000, 100000)] {
        for target in ["D100F0", "K1M100F0"] {
            for error in [None, Some(1067), Some(1068)] {
                let p = compile_il(&format!("LD X0\n{name} D10E0 K{b} {target}\nEND")).unwrap();
                let mut sim = enabled();
                sim.memory.set_word(d("D1028"), 2);
                sim.memory.set_word(d("D1029"), 3);
                if name == "MUL32" {
                    sim.memory.set_double_word(d("D12"), a);
                } else {
                    sim.memory.set_word(d("D12"), a as i16);
                }
                if let Some(flag) = error {
                    sim.memory.set_system(flag, true);
                }
                sim.memory.set_system(1022, true);
                run(&mut sim, &p);
                assert_eq!(
                    sim.memory.system_bit(1022),
                    name == "MUL32" || target.starts_with('K') || error.is_none(),
                    "{name} {target} {error:?}"
                );
                // Tiny Kn target width does not itself cause overflow.
                if name == "MUL32" {
                    sim.memory.set_double_word(d("D12"), 1);
                } else {
                    sim.memory.set_word(d("D12"), 1);
                }
                run(&mut sim, &p);
                assert!(!sim.memory.system_bit(1022));
            }
        }
    }
}

#[test]
fn zero_divisor_skips_target_and_keeps_independent_sticky_error_channels() {
    for (name, offset) in [("DIV16", 5), ("DIV32", 9)] {
        let p = compile_il(&format!("LD X0\n{name} D10E0 D20E0 D100F0\nEND")).unwrap();
        let mut sim = enabled();
        sim.memory.set_word(d("D1028"), 1);
        sim.memory.set_word(d("D1029"), -30000);
        sim.memory.set_double_word(d("D11"), 17);
        sim.memory.set_system(1068, true);
        sim.memory.set_word(d("D1068"), 123);
        run(&mut sim, &p);
        assert!(sim.memory.execution_limitations.is_empty());
        assert_eq!(sim.memory.word(d("D1067")), 0xe19);
        assert_eq!(sim.memory.word(d("D1068")), 123);
        assert!(sim.observed_devices.contains(&d("D21")));
        assert!(
            !sim.observed_devices.contains(&d("D1029")),
            "unused destination index"
        );
        sim.memory.set_system(1068, false);
        run(&mut sim, &p);
        assert_eq!(
            sim.memory.word(d("D1068")),
            (p.native_action_pc[0] + offset) as i16
        );
    }
}

#[test]
fn source_failure_precedes_divisor_zero_without_inventing_native_error_flags() {
    let p = compile_il("LD X0\nDIV32 D10E0 K0 D100F0\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_word(d("D1028"), -30000);
    run(&mut sim, &p);
    assert!(!sim.memory.execution_limitations.is_empty());
    assert!(!sim.memory.system_bit(1067));
}

#[test]
fn pulse_off_held_and_rearm_resolve_at_the_actual_visit_only() {
    for name in ["MUL16P", "MUL32P", "DIV16P", "DIV32P"] {
        let p = compile_il(&format!("LD X0\n{name} D10E0 K3 D100F0\nEND")).unwrap();
        let mut sim = Simulator::default();
        sim.memory.set_word(d("D1028"), -30000);
        run(&mut sim, &p);
        assert!(sim.memory.execution_limitations.is_empty());
        sim.memory.set_word(d("D1028"), 0);
        sim.memory.set_double_word(d("D10"), 12);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &p);
        let old = sim.memory.double_word(d("D100"));
        sim.memory.set_word(d("D1028"), -30000);
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
fn one_wide_result_retains_cursor_when_low_word_changes_its_index() {
    let p = compile_il("LD X0\nMUL32 H10002 H10003 D1027E0\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_word(d("D1028"), 1);
    sim.memory.set_word(d("D1030"), 0x5a5a);
    sim.memory.set_word(d("D1033"), 0x6b6b);
    run(&mut sim, &p);
    assert_eq!(sim.memory.word(d("D1028")), 6);
    assert_eq!(sim.memory.word(d("D1029")), 5);
    assert_eq!(sim.memory.word(d("D1030")), 0x5a5a);
    assert_eq!(sim.memory.word(d("D1033")), 0x6b6b);
}

#[test]
fn short_group_result_uses_one_chunk_and_overlapping_divisor_is_snapshotted() {
    let p = compile_il("LD X0\nDIV16 D10E0 D11E0 D11E0\nMUL16 K-17 K3 K1M100F0\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_word(d("D10"), -17);
    sim.memory.set_word(d("D11"), 3);
    sim.memory.set_word(d("D12"), 0x1234);
    sim.memory.set_word(d("D1029"), 3);
    sim.memory.set_internal(d("M107"), true);
    sim.memory.set_internal(d("M110"), true);
    run(&mut sim, &p);
    assert_eq!(sim.memory.word(d("D11")), -5);
    assert_eq!(sim.memory.word(d("D12")), 0x1234);
    assert_eq!(
        ["M103", "M104", "M105", "M106"].map(|s| sim.memory.bit(d(s))),
        [true, false, true, true]
    );
    assert!(sim.memory.bit(d("M107")) && sim.memory.bit(d("M110")));
    assert!(!sim.memory.bit(d("M108")) && !sim.memory.bit(d("M109")));
}

#[test]
fn single_result_model_ends_allow_more_targets_than_full_multiplication() {
    for (name, target, offset, valid) in [
        ("MUL16", "D4999E0", 0, true),
        ("MUL32", "D4999E0", 0, false),
        ("MUL32", "D4999E0", -1, true),
        ("MUL16", "T255E0", 0, true),
        ("MUL32", "T255E0", -1, true),
        ("MUL32", "C255E0", 0, true),
    ] {
        let p = compile_il(&format!("LD X0\n{name} K3 K7 {target}\nEND")).unwrap();
        let mut sim = enabled();
        sim.memory.data_register_limit = Some(4999);
        sim.memory.set_word(d("D1028"), offset);
        run(&mut sim, &p);
        assert_eq!(
            sim.memory.execution_limitations.is_empty(),
            valid,
            "{name} {target}"
        );
    }
}

#[test]
fn counter_c199_low_only_and_min16_quotient_do_not_touch_neighbors() {
    let p = compile_il("LD X0\nDIV32 K-17 K3 C200E0\nDIV16 H8000 K-1 D10F0\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_word(d("D1028"), -1);
    sim.memory.set_double_word(d("C200"), 0x12345678);
    sim.memory.set_word(d("D11"), 77);
    run(&mut sim, &p);
    assert_eq!(sim.memory.word(d("C199")), -5);
    assert_eq!(sim.memory.double_word(d("C200")), 0x12345678);
    assert_eq!(sim.memory.word(d("D10")), i16::MIN);
    assert_eq!(sim.memory.word(d("D11")), 77);
}

#[test]
fn loop_offsets_are_live_and_direct_e32_remains_canonical_locally() {
    let p =
        compile_il("LD X0\nFOR K3\nMUL16 D10E0 K2 D100E0\nINC E0\nNEXT\nMUL32 D10F0 K3 E1\nEND")
            .unwrap();
    let mut sim = enabled();
    for (s, v) in [("D10", 2), ("D11", 4), ("D12", 6)] {
        sim.memory.set_word(d(s), v);
    }
    sim.memory.set_double_word(d("D182"), 1234567);
    run(&mut sim, &p);
    assert_eq!(
        ["D100", "D101", "D102"].map(|s| sim.memory.word(d(s))),
        [4, 8, 12]
    );
    assert_eq!(sim.memory.double_word(d("D1182")), 786438);
    assert_eq!(sim.memory.double_word(d("D182")), 1234567);
    assert!(
        p.networks
            .iter()
            .flat_map(|n| &n.actions)
            .any(|a| matches!(a, Action::IndexedTruncated(_)))
    );
}
