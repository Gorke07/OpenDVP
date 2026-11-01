//! Independent SQR expectations: integer results and instruction words below
//! are literal fixtures, independent of the implementation's root calculation.
use crate::indexed_root::{IndexedRootAction, PreparedRoot};
use crate::plc::{Device, Memory, Program, ScanStatus, Simulator, compile_il};
use std::collections::BTreeSet;

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}

fn parse(raw: &str) -> IndexedRootAction {
    let mut parts = raw.split_whitespace();
    let name = parts.next().unwrap();
    IndexedRootAction::parse(name, &parts.map(str::to_owned).collect::<Vec<_>>()).unwrap()
}

fn enabled() -> Simulator {
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim
}

fn run(sim: &mut Simulator, program: &Program) {
    sim.scan(program, 0);
    assert_eq!(sim.last_scan, ScanStatus::Complete);
}

fn flags(memory: &Memory) -> [bool; 3] {
    [1020, 1021, 1022].map(|n| memory.system_bit(n))
}

fn seed_flags(memory: &mut Memory, values: [bool; 3]) {
    for (n, value) in [1020, 1021, 1022].into_iter().zip(values) {
        memory.set_system(n, value);
    }
}

#[test]
fn four_forms_have_literal_descriptors_and_integer_results() {
    for (name, header, wide, input, expected, borrow) in [
        ("SQR", 0x7000, false, 144, 12, false),
        ("SQRP", 0x7010, false, 145, 12, true),
        ("DSQR", 0x7100, true, 2_147_395_600, 46_340, false),
        ("DSQRP", 0x7110, true, 2_147_483_647, 46_340, true),
    ] {
        let program = compile_il(&format!("LD X0\n{name} D10E1 D100F7\nEND")).unwrap();
        assert!(program.requires_vm());
        let encoded = crate::es2_encoder::encode(&program).unwrap();
        let pc = usize::from(encoded.action_pc[0]);
        let expected_words = if wide {
            vec![
                header, 0x14e6, 0x8086, 0x0080, 0x0080, 0xc8f6, 0x8089, 0x0080, 0x0080,
            ]
        } else {
            vec![header, 0x14e6, 0x8086, 0xc8f6, 0x8089]
        };
        assert_eq!(
            &encoded.words[pc..pc + expected_words.len()],
            expected_words,
            "{name}"
        );
        let mut sim = enabled();
        sim.memory.set_word(d("D1182"), 1);
        sim.memory.set_word(d("D1195"), 2);
        if wide {
            sim.memory.set_double_word(d("D11"), input);
        } else {
            sim.memory.set_word(d("D11"), input as i16);
        }
        sim.memory.set_word(d("D101"), 77);
        sim.memory.set_word(d("D104"), 88);
        seed_flags(&mut sim.memory, [true, true, true]);
        run(&mut sim, &program);
        assert_eq!(
            if wide {
                sim.memory.double_word(d("D102"))
            } else {
                i32::from(sim.memory.word(d("D102")))
            },
            expected
        );
        assert_eq!(flags(&sim.memory), [false, borrow, true]);
        assert_eq!(sim.memory.word(d("D101")), 77);
        assert_eq!(sim.memory.word(d("D104")), 88);
        assert!(!program.devices.contains(&d("D100")));
        assert!(sim.observed_devices.contains(&d("D102")));
    }
}

#[test]
fn all_sixteen_modifier_words_use_signed_word_offsets_at_both_widths() {
    for (suffix, alias) in [
        ("E0", "D1028"),
        ("F0", "D1029"),
        ("E1", "D1182"),
        ("F1", "D1183"),
        ("E2", "D1184"),
        ("F2", "D1185"),
        ("E3", "D1186"),
        ("F3", "D1187"),
        ("E4", "D1188"),
        ("F4", "D1189"),
        ("E5", "D1190"),
        ("F5", "D1191"),
        ("E6", "D1192"),
        ("F6", "D1193"),
        ("E7", "D1194"),
        ("F7", "D1195"),
    ] {
        for name in ["SQR", "DSQR"] {
            let program =
                compile_il(&format!("LD X0\n{name} D10{suffix} D100{suffix}\nEND")).unwrap();
            let mut sim = enabled();
            let neighbor = Device {
                area: crate::plc::Area::D,
                index: d(alias).index + 1,
            };
            sim.memory.set_word(d(alias), -2);
            sim.memory.set_word(neighbor, i16::MIN);
            sim.memory.set_double_word(d("D8"), 361);
            run(&mut sim, &program);
            assert_eq!(sim.memory.word(d("D98")), 19, "{name} {suffix}");
            assert!(sim.observed_devices.contains(&d(alias)));
            assert!(
                !sim.observed_devices.contains(&neighbor),
                "offset stays word-wide: {name} {suffix}"
            );
        }
    }
}

#[test]
fn role_matrix_stays_d_only_with_unmodified_integer_constants_as_sources() {
    for name in ["SQR", "SQRP", "DSQR", "DSQRP"] {
        for (source, target) in [("K144", "D20F7"), ("H90", "D20E0"), ("D0F0", "D20")] {
            assert!(IndexedRootAction::parse(name, &[source.into(), target.into()]).is_ok());
        }
        for (source, target) in [
            ("T0E0", "D20"),
            ("C0E0", "D20"),
            ("K1M0E0", "D20"),
            ("E0", "D20F0"),
            ("F7", "D20E0"),
            ("M0E0", "D20"),
            ("K4E0", "D20"),
            ("D0E8", "D20"),
            ("D0E0", "E1"),
            ("D0E0", "K4M0"),
            ("D0E0", "T20"),
            ("D0E0", "K1"),
        ] {
            assert!(
                IndexedRootAction::parse(name, &[source.into(), target.into()]).is_err(),
                "{name} {source} {target}"
            );
        }
    }
    for name in ["SQ", "DSQ", "SQRPP", "ROOT", "DROOT"] {
        assert!(!IndexedRootAction::supports(name));
    }
    assert_eq!(parse("dsqrp d0f7 d20e1").label(), "DSQRP D0F7 D20E1");
    assert_eq!(parse("SQR D0E0 D20F0").dest(), d("D20"));
    assert!(!IndexedRootAction::has_modifiers(&[
        "HFE".into(),
        "D20".into()
    ]));
}

#[test]
fn literal_sources_round_then_truncate_and_ignore_float_reverse_mode() {
    for (name, source, expected, expected_flags) in [
        ("SQR", "K0", 0, [true, false, true]),
        ("SQR", "K32767", 181, [false, true, true]),
        ("SQR", "H7A61", 177, [false, false, true]),
        ("DSQR", "K2147395600", 46_340, [false, false, true]),
        ("DSQR", "H7FFFFFFF", 46_340, [false, true, true]),
    ] {
        for reverse in [false, true] {
            let program = compile_il(&format!("LD X0\n{name} {source} D20F0\nEND")).unwrap();
            let mut sim = enabled();
            sim.memory.set_system(1081, reverse);
            seed_flags(&mut sim.memory, [false, true, true]);
            run(&mut sim, &program);
            assert_eq!(sim.memory.word(d("D20")), expected as i16);
            assert_eq!(flags(&sim.memory), expected_flags);
            assert_eq!(sim.memory.system_bit(1081), reverse);
        }
    }
}

#[test]
fn signed_negative_source_errors_before_target_index_and_keeps_sticky_pc() {
    for name in ["SQR", "DSQR"] {
        let program = compile_il(&format!("LD X0\n{name} D10E0 D100F7\nEND")).unwrap();
        let mut sim = enabled();
        sim.memory.set_word(d("D1028"), 1);
        sim.memory.set_word(d("D1195"), -30000);
        sim.memory.set_double_word(d("D11"), -1);
        sim.memory.set_word(d("D100"), 77);
        sim.memory.set_system(1068, true);
        sim.memory.set_word(d("D1068"), 321);
        seed_flags(&mut sim.memory, [true, true, true]);
        run(&mut sim, &program);
        assert!(sim.memory.execution_limitations.is_empty());
        assert_eq!(sim.memory.word(d("D1067")), 0xe1b);
        assert_eq!(sim.memory.word(d("D1068")), 321);
        assert_eq!(sim.memory.word(d("D100")), 77);
        assert_eq!(flags(&sim.memory), [false, false, true]);
        assert!(!sim.observed_devices.contains(&d("D1195")));
        sim.memory.set_system(1068, false);
        run(&mut sim, &program);
        assert_eq!(
            sim.memory.word(d("D1068")),
            (program.native_action_pc[0] + 1) as i16
        );
        assert!(!sim.observed_devices.contains(&d("D1195")));
    }
}

#[test]
fn pure_prepare_and_inactive_visits_do_not_mutate_flags_or_read_target() {
    for instruction in ["SQR H8000 D20F7", "DSQR H80000000 D20F7"] {
        let action = parse(instruction);
        let mut memory = Memory::default();
        memory.set_word(d("D1195"), -30000);
        seed_flags(&mut memory, [true, true, true]);
        let mut observed = BTreeSet::new();
        assert_eq!(
            action.prepare(&memory, &mut observed).unwrap(),
            PreparedRoot::InvalidSource
        );
        assert!(observed.is_empty());
        assert_eq!(flags(&memory), [true, true, true]);
        assert!(!memory.system_bit(1067));
        action.execute(&mut memory, false, &mut observed);
        assert!(observed.is_empty());
        assert_eq!(flags(&memory), [true, true, true]);
    }
}

#[test]
fn pulse_disabled_and_held_visits_skip_invalid_addresses_and_rearm() {
    for name in ["SQRP", "DSQRP"] {
        let program = compile_il(&format!("LD X0\n{name} D10E0 D100F0\nEND")).unwrap();
        let mut sim = Simulator::default();
        sim.memory.set_word(d("D1028"), -30000);
        seed_flags(&mut sim.memory, [true, true, true]);
        run(&mut sim, &program);
        assert!(sim.memory.execution_limitations.is_empty());
        assert_eq!(flags(&sim.memory), [true, true, true]);
        assert!(!sim.observed_devices.contains(&d("D1028")));
        sim.memory.set_word(d("D1028"), 0);
        sim.memory.set_double_word(d("D10"), 144);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &program);
        assert_eq!(sim.memory.word(d("D100")), 12);
        sim.memory.set_word(d("D1028"), -30000);
        seed_flags(&mut sim.memory, [true, true, true]);
        sim.observed_devices.clear();
        run(&mut sim, &program);
        assert!(sim.memory.execution_limitations.is_empty());
        assert_eq!(flags(&sim.memory), [true, true, true]);
        assert!(!sim.observed_devices.contains(&d("D1028")));
        sim.memory.set_input(d("X0"), false);
        run(&mut sim, &program);
        sim.memory.set_word(d("D1028"), 1);
        sim.memory.set_double_word(d("D11"), 225);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &program);
        assert_eq!(sim.memory.word(d("D100")), 15);
    }
}

#[test]
fn source_overlap_and_self_index_result_use_one_snapshot_and_fixed_pair() {
    for name in ["SQR", "DSQR"] {
        let program = compile_il(&format!("LD X0\n{name} D20E1 D20E1\nEND")).unwrap();
        let mut sim = enabled();
        sim.memory.set_word(d("D1182"), 1);
        sim.memory.set_double_word(d("D21"), 625);
        sim.memory.set_word(d("D23"), 77);
        run(&mut sim, &program);
        assert_eq!(sim.memory.double_word(d("D21")), 25);
        assert_eq!(sim.memory.word(d("D23")), 77);
    }
    let program = compile_il("LD X0\nDSQR D10F0 D1027E0\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_word(d("D1028"), 1);
    sim.memory.set_double_word(d("D10"), 144);
    sim.memory.set_word(d("D1030"), 88);
    run(&mut sim, &program);
    assert_eq!(sim.memory.word(d("D1028")), 12);
    assert_eq!(sim.memory.word(d("D1029")), 0);
    assert_eq!(sim.memory.word(d("D1030")), 88);
    assert!(sim.observed_devices.contains(&d("D1029")));
    assert!(!sim.observed_devices.contains(&d("D1039")));
}

#[test]
fn repeated_visits_resolve_current_indices_without_recompiling_actions() {
    let program = compile_il("LD X0\nFOR K3\nSQR D10E0 D100E0\nINC E0\nNEXT\nEND").unwrap();
    let mut sim = enabled();
    for (index, value) in [(10, 16), (11, 25), (12, 36)] {
        sim.memory.set_word(d(&format!("D{index}")), value);
    }
    run(&mut sim, &program);
    assert_eq!(
        [100, 101, 102].map(|n| sim.memory.word(d(&format!("D{n}")))),
        [4, 5, 6]
    );
    assert_eq!(sim.memory.word(d("D1028")), 3);
    assert!(sim.observed_devices.contains(&d("D12")) && sim.observed_devices.contains(&d("D102")));
}

#[test]
fn selected_model_endpoints_validate_entire_effective_word_or_pair() {
    for last in [4999, 9999] {
        let word = compile_il(&format!("LD X0\nSQR D{last}E0 D{last}E0\nEND")).unwrap();
        let mut sim = enabled();
        sim.memory.data_register_limit = Some(last);
        sim.memory.set_word(d(&format!("D{last}")), 324);
        run(&mut sim, &word);
        assert_eq!(sim.memory.word(d(&format!("D{last}"))), 18);
        assert!(sim.memory.execution_limitations.is_empty());

        let wide = compile_il(&format!("LD X0\nDSQR D{last}E0 D{last}F0\nEND")).unwrap();
        let mut sim = enabled();
        sim.memory.data_register_limit = Some(last);
        sim.memory.set_word(d("D1028"), -1);
        sim.memory.set_word(d("D1029"), -1);
        sim.memory
            .set_double_word(d(&format!("D{}", last - 1)), 1_000_000);
        run(&mut sim, &wide);
        assert_eq!(sim.memory.double_word(d(&format!("D{}", last - 1))), 1000);
        assert!(sim.memory.execution_limitations.is_empty());
        sim.memory.set_word(d("D1028"), 0);
        run(&mut sim, &wide);
        assert!(!sim.memory.execution_limitations.is_empty());
        assert!(!sim.memory.system_bit(1067));

        let mut memory = Memory::default();
        memory.data_register_limit = Some(last);
        memory.set_word(d("D1029"), 0);
        let target = parse(&format!("DSQR K144 D{last}F0"));
        assert!(target.prepare(&memory, &mut BTreeSet::new()).is_err());
    }
    let mut memory = Memory::default();
    memory.data_register_limit = Some(4999);
    assert!(parse("SQR D5000 D20E0").resolved_source(&memory).is_err());
    assert!(
        parse("SQR K144 D5000E0")
            .prepare(&memory, &mut BTreeSet::new())
            .is_err()
    );
}

#[test]
fn structural_limits_follow_flags_order_without_fabricated_plc_errors() {
    let action = parse("SQR D10E0 D20F0");
    let mut memory = Memory::default();
    memory.set_word(d("D1028"), -30000);
    seed_flags(&mut memory, [true, true, true]);
    let mut observed = BTreeSet::new();
    action.execute(&mut memory, true, &mut observed);
    assert_eq!(flags(&memory), [false, false, true]);
    assert!(!memory.execution_limitations.is_empty());
    assert!(!memory.system_bit(1067));
    assert!(!observed.contains(&d("D1029")));

    let mut memory = Memory::default();
    memory.set_word(d("D10"), 145);
    memory.set_word(d("D1029"), -30000);
    seed_flags(&mut memory, [true, false, true]);
    action.execute(&mut memory, true, &mut BTreeSet::new());
    assert_eq!(flags(&memory), [false, true, true]);
    assert!(!memory.execution_limitations.is_empty());
    assert!(!memory.system_bit(1067));
}

#[test]
fn actual_observations_and_held_display_are_independent_of_static_base_labels() {
    for (name, wide) in [("SQR", false), ("DSQR", true)] {
        let action = parse(&format!("{name} D10E0 D100F0"));
        let mut memory = Memory::default();
        memory.set_word(d("D1028"), 1);
        memory.set_word(d("D1029"), 2);
        memory.set_double_word(d("D11"), 144);
        let mut observed = BTreeSet::new();
        action.execute(&mut memory, true, &mut observed);
        assert!(observed.contains(&d("D11")) && observed.contains(&d("D102")));
        assert_eq!(observed.contains(&d("D12")), wide);
        assert_eq!(observed.contains(&d("D103")), wide);
        assert!(!observed.contains(&d("D10")) && !observed.contains(&d("D100")));
        assert_eq!(action.display_value(&memory), "12");
        memory.set_double_word(d("D11"), -1);
        observed.clear();
        assert_eq!(
            action.prepare(&memory, &mut observed).unwrap(),
            PreparedRoot::InvalidSource
        );
        assert!(!observed.contains(&d("D1029")) && !observed.contains(&d("D102")));
        let mut ui = BTreeSet::new();
        action.observed_devices(&memory, &mut ui);
        assert!(ui.contains(&d("D102")));
        assert_eq!(action.display_value(&memory), "12");
        memory.set_word(d("D1029"), -30000);
        assert!(action.display_value(&memory).starts_with("İndeks:"));
    }
}

#[test]
fn negative_signed_offsets_and_wrapped_invalid_addresses_have_distinct_results() {
    let action = parse("DSQR D9999E0 D100F0");
    let mut memory = Memory::default();
    memory.set_word(d("D1028"), -1);
    memory.set_double_word(d("D9998"), 81);
    action.execute(&mut memory, true, &mut BTreeSet::new());
    assert_eq!(memory.double_word(d("D100")), 9);
    memory.set_word(d("D1028"), i16::MAX);
    assert!(action.resolved_source(&memory).is_err());
    memory.set_word(d("D1028"), i16::MIN);
    assert!(action.resolved_source(&memory).is_err());
    assert!(!memory.system_bit(1067));
}

#[test]
fn ordinary_root_uses_the_same_flags_contract_without_changing_its_role_matrix() {
    for (name, source, result, expected_flags, error) in [
        ("SQR", "K0", 0, [true, false, true], false),
        ("SQR", "K145", 12, [false, true, true], false),
        ("DSQR", "K2147395600", 46_340, [false, false, true], false),
        ("SQR", "K-1", 77, [false, false, true], true),
        ("DSQR", "K-1", 77, [false, false, true], true),
    ] {
        let program = compile_il(&format!("LD X0\n{name} {source} D20\nEND")).unwrap();
        let mut sim = enabled();
        sim.memory.set_double_word(d("D20"), 77);
        seed_flags(&mut sim.memory, [true, true, true]);
        run(&mut sim, &program);
        assert_eq!(sim.memory.double_word(d("D20")), result);
        assert_eq!(flags(&sim.memory), expected_flags);
        assert_eq!(sim.memory.system_bit(1067), error);
        if error {
            assert_eq!(sim.memory.word(d("D1067")), 0xe1b);
        }
    }
}
