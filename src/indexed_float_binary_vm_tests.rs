//! Independent instruction words, binary32 bit patterns and error/flag
//! expectations for the eight indexed DE* arithmetic forms.
use crate::float_math::{FloatMathAction, MathOp};
use crate::float_ops::FloatOperand;
use crate::indexed_float_binary::{IndexedFloatBinaryAction, PreparedFloatBinary};
use crate::plc::{Device, Memory, Program, ScanStatus, Simulator, compile_il};
use std::collections::BTreeSet;

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn parse(raw: &str) -> IndexedFloatBinaryAction {
    let mut parts = raw.split_whitespace();
    IndexedFloatBinaryAction::parse(
        parts.next().unwrap(),
        &parts.map(str::to_owned).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn put(memory: &mut Memory, address: &str, bits: u32) {
    memory.set_double_word(d(address), bits as i32);
}
fn get(memory: &Memory, address: &str) -> u32 {
    memory.double_word(d(address)) as u32
}
fn flags(memory: &Memory) -> [bool; 3] {
    [1020, 1021, 1022].map(|n| memory.system_bit(n))
}
fn seed(memory: &mut Memory) {
    for (n, value) in [(1020, true), (1021, false), (1022, true)] {
        memory.set_system(n, value);
    }
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

#[test]
fn eight_forms_encode_literal_fields_and_write_one_binary32_result() {
    for (name, header, expected) in [
        ("DEADD", 0x1101, 0x4100_0000),
        ("DEADDP", 0x1111, 0x4100_0000),
        ("DESUB", 0x1301, 0x4080_0000),
        ("DESUBP", 0x1311, 0x4080_0000),
        ("DEMUL", 0x1501, 0x4140_0000),
        ("DEMULP", 0x1511, 0x4140_0000),
        ("DEDIV", 0x1701, 0x4040_0000),
        ("DEDIVP", 0x1711, 0x4040_0000),
    ] {
        let program = compile_il(&format!("LD X0\n{name} D10E1 D20F0 D100E7\nEND")).unwrap();
        assert!(program.requires_vm());
        let encoded = crate::es2_encoder::encode(&program).unwrap();
        let pc = usize::from(encoded.action_pc[0]);
        assert_eq!(
            &encoded.words[pc..pc + 13],
            &[
                header, 0x14e6, 0x8086, 0x0080, 0x0080, 0x28f6, 0x0086, 0x0080, 0x0080, 0xc8e6,
                0x8089, 0x0080, 0x0080,
            ],
            "{name}"
        );
        let mut sim = enabled();
        sim.memory.set_word(d("D1182"), 1);
        sim.memory.set_word(d("D1029"), 2);
        sim.memory.set_word(d("D1194"), 3);
        put(&mut sim.memory, "D11", 0x40c0_0000); // 6
        put(&mut sim.memory, "D22", 0x4000_0000); // 2
        sim.memory.set_word(d("D102"), 77);
        sim.memory.set_word(d("D105"), 88);
        seed(&mut sim.memory);
        run(&mut sim, &program);
        assert_eq!(get(&sim.memory, "D103"), expected);
        assert_eq!(flags(&sim.memory), [false; 3]);
        assert_eq!(sim.memory.word(d("D102")), 77);
        assert_eq!(sim.memory.word(d("D105")), 88);
        assert!(!program.devices.contains(&d("D100")));
        for actual in ["D11", "D12", "D22", "D23", "D103", "D104"] {
            assert!(sim.observed_devices.contains(&d(actual)), "{name} {actual}");
        }
    }
}

#[test]
fn every_modifier_bank_is_a_signed_word_for_each_operand_role() {
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
        let program = compile_il(&format!(
            "LD X0\nDEMUL D10{suffix} D20{suffix} D100{suffix}\nEND"
        ))
        .unwrap();
        let mut sim = enabled();
        let neighbor = Device {
            area: crate::plc::Area::D,
            index: d(alias).index + 1,
        };
        sim.memory.set_word(d(alias), -2);
        sim.memory.set_word(neighbor, i16::MIN);
        put(&mut sim.memory, "D8", 0x40c0_0000);
        put(&mut sim.memory, "D18", 0x4000_0000);
        run(&mut sim, &program);
        assert_eq!(get(&sim.memory, "D98"), 0x4140_0000, "{suffix}");
        assert!(sim.observed_devices.contains(&d(alias)));
        assert!(
            !sim.observed_devices.contains(&neighbor),
            "index is not a double word: {suffix}"
        );
    }
}

#[test]
fn documented_role_matrix_does_not_expand_other_float_or_numeric_roles() {
    for name in [
        "DEADD", "DEADDP", "DESUB", "DESUBP", "DEMUL", "DEMULP", "DEDIV", "DEDIVP",
    ] {
        for (left, right, target) in [("D0E0", "K1", "D20"), ("H80000000", "D0F7", "D20E1")] {
            assert!(
                IndexedFloatBinaryAction::parse(name, &[left.into(), right.into(), target.into()])
                    .is_ok()
            );
        }
        for (left, right, target) in [
            ("T0E0", "K1", "D20"),
            ("D0E0", "C200", "D20"),
            ("E0", "D0F0", "D20"),
            ("D0E0", "F1", "D20"),
            ("K4M0E0", "K1", "D20"),
            ("K1E0", "K1", "D20"),
            ("D0E8", "K1", "D20"),
            ("D0E0", "K1", "K1M0"),
            ("D0E0", "K1", "E1"),
            ("D0E0", "K1", "T0"),
            ("D0E0", "K1", "K1"),
            ("D9999", "K1", "D20E0"),
        ] {
            assert!(
                IndexedFloatBinaryAction::parse(name, &[left.into(), right.into(), target.into()])
                    .is_err(),
                "{name} {left} {right} {target}"
            );
        }
    }
    for name in ["DADDR", "DDIVR", "DRAD", "DEBCD", "DEADDPP", "EADD"] {
        assert!(!IndexedFloatBinaryAction::supports(name));
    }
}

#[test]
fn labels_and_integer_descriptors_preserve_source_provenance() {
    let action = parse("deaddp d0e1 hffffffff d20f7");
    assert_eq!(action.label(), "DEADDP D0E1 K-1 D20F7");
    assert_eq!(action.dest(), d("D20"));
    assert_eq!(action.modifier_devices(), vec![d("D1182"), d("D1195")]);
    assert_eq!(parse(&action.label()), action);
    assert_eq!(
        parse("DEADD K1 D0E0 D20F0").native_words().unwrap(),
        vec![
            0x1101, 0x0180, 0x0080, 0x0080, 0x0080, 0x00e6, 0x0086, 0x0080, 0x0080, 0x28f6, 0x0086,
            0x0080, 0x0080,
        ]
    );
    assert!(!IndexedFloatBinaryAction::has_modifiers(&[
        "HFE".into(),
        "K1".into(),
        "D20".into()
    ]));
    assert!(action.resolved_source(2, &Memory::default()).is_err());
}

#[test]
fn integers_convert_before_arithmetic_while_d_pairs_keep_raw_float_bits() {
    let program = compile_il(
        "LD X0\nDEADD K16777217 K1 D20E0\nDEADD H3F800000 K0 D30E0\nDEADD D0E0 K0 D40E0\nEND",
    )
    .unwrap();
    let mut sim = enabled();
    put(&mut sim.memory, "D0", 0x3f80_0000);
    sim.memory.set_system(1081, true);
    run(&mut sim, &program);
    assert_eq!(get(&sim.memory, "D20"), 0x4b80_0000); // 2^24, both conversions round first.
    assert_eq!(get(&sim.memory, "D30"), 0x4e7e_0000); // Integer 1065353216.
    assert_eq!(get(&sim.memory, "D40"), 0x3f80_0000); // Raw float 1.
    assert!(sim.memory.system_bit(1081));
}

#[test]
fn min32_integer_divisor_is_distinct_from_a_register_negative_zero() {
    for (left, expected, expected_flags) in [
        ("K1", 0xff80_0000, [false, true, false]),
        ("K-1", 0x7f80_0000, [false, false, true]),
        ("D0E0", 0xff80_0000, [false, true, false]),
    ] {
        let program = compile_il(&format!("LD X0\nDEDIV {left} H80000000 D20F0\nEND")).unwrap();
        let mut sim = enabled();
        put(&mut sim.memory, "D0", 0x3f80_0000);
        run(&mut sim, &program);
        assert_eq!(get(&sim.memory, "D20"), expected);
        assert_eq!(flags(&sim.memory), expected_flags);
        assert!(!sim.memory.system_bit(1067));
    }
    let program = compile_il("LD X0\nDEDIV K1 D0E0 D20F0\nEND").unwrap();
    let mut sim = enabled();
    put(&mut sim.memory, "D0", 0x8000_0000);
    put(&mut sim.memory, "D20", 0x1234_5678);
    seed(&mut sim.memory);
    run(&mut sim, &program);
    assert_eq!(get(&sim.memory, "D20"), 0x1234_5678);
    assert_eq!(sim.memory.word(d("D1067")), 0xe19);
    assert_eq!(flags(&sim.memory), [true, false, true]);
}

#[test]
fn division_error_precedence_holds_flags_and_skips_invalid_target_index() {
    for (left, right, code) in [
        (0x7fc0_1234, 0, 0xe19),
        (0x7fc0_1234, 0x8000_0000, 0xe1a),
        (0x7fc0_1234, 1, 0xe1a),
        (0x3f80_0000, 1, 0xe19),
        (0x3f80_0000, 0x8000_0001, 0xe19),
        (0x3f80_0000, 0x7f80_0001, 0xe1a),
    ] {
        let program = compile_il("LD X0\nDEDIV D0E0 D10F0 D100F7\nEND").unwrap();
        let mut sim = enabled();
        put(&mut sim.memory, "D0", left);
        put(&mut sim.memory, "D10", right);
        put(&mut sim.memory, "D100", 0x1234_5678);
        sim.memory.set_word(d("D1195"), -30000);
        sim.memory.set_system(1068, true);
        sim.memory.set_word(d("D1068"), 321);
        seed(&mut sim.memory);
        run(&mut sim, &program);
        assert_eq!(sim.memory.word(d("D1067")), code);
        assert_eq!(sim.memory.word(d("D1068")), 321);
        assert_eq!(flags(&sim.memory), [true, false, true]);
        assert_eq!(get(&sim.memory, "D100"), 0x1234_5678);
        assert!(!sim.observed_devices.contains(&d("D1195")));
        assert!(sim.memory.execution_limitations.is_empty());
        sim.memory.set_system(1068, false);
        run(&mut sim, &program);
        assert_eq!(
            sim.memory.word(d("D1068")),
            (program.native_action_pc[0] + 5) as i16
        );
    }
}

#[test]
fn pure_prepare_reports_semantic_error_without_writes_or_target_observation() {
    let action = parse("DEDIV D0E0 K0 D20F7");
    let mut memory = Memory::default();
    put(&mut memory, "D0", 0x7fc0_0001);
    memory.set_word(d("D1195"), -30000);
    seed(&mut memory);
    let mut observed = BTreeSet::new();
    assert_eq!(
        action.prepare(&memory, &mut observed).unwrap(),
        PreparedFloatBinary::SemanticError {
            code: 0xe19,
            offset: 5
        }
    );
    assert_eq!(flags(&memory), [true, false, true]);
    assert!(!memory.system_bit(1067));
    assert!(observed.contains(&d("D0")) && observed.contains(&d("D1")));
    assert!(!observed.contains(&d("D1195")));
    observed.clear();
    action.execute(&mut memory, false, &mut observed);
    assert!(observed.is_empty());
    assert_eq!(flags(&memory), [true, false, true]);
}

#[test]
fn successful_zero_subnormal_overflow_and_nan_results_use_shared_normalization() {
    for (op, left, right, expected, expected_flags) in [
        ("DEADD", 0x8000_0000, 0x8000_0000, 0, [true, false, false]),
        ("DESUB", 0x3f80_0000, 0x3f80_0000, 0, [true, false, false]),
        ("DEADD", 1, 1, 2, [false, false, false]),
        (
            "DEMUL",
            0x7f7f_ffff,
            0x4000_0000,
            0x7f80_0000,
            [false, false, true],
        ),
        (
            "DEMUL",
            0xff7f_ffff,
            0x4000_0000,
            0xff80_0000,
            [false, true, false],
        ),
        (
            "DEADD",
            0x7fc0_0001,
            0xffc0_0002,
            0xff80_0000,
            [false, true, false],
        ),
        (
            "DESUB",
            0xffc0_0001,
            0x7fc0_0001,
            0x7f80_0000,
            [false, false, true],
        ),
        (
            "DEMUL",
            0xff80_0002,
            0x7fc0_0001,
            0xff80_0000,
            [false, true, false],
        ),
        ("DEDIV", 0x3f80_0000, 0x7f80_0000, 0, [true, false, false]),
    ] {
        let mut memory = Memory::default();
        put(&mut memory, "D0", left);
        put(&mut memory, "D10", right);
        seed(&mut memory);
        parse(&format!("{op} D0E0 D10F0 D20E1")).execute(&mut memory, true, &mut BTreeSet::new());
        assert_eq!(get(&memory, "D20"), expected, "{op} {left:08x} {right:08x}");
        assert_eq!(flags(&memory), expected_flags);
        assert!(!memory.system_bit(1067));
    }
}

#[test]
fn pulse_gate_skips_disabled_and_held_bad_addresses_then_rearms() {
    for (name, first, second) in [
        ("DEADDP", 0x4100_0000, 0x4170_0000),
        ("DESUBP", 0x4080_0000, 0x4110_0000),
        ("DEMULP", 0x4140_0000, 0x4210_0000),
        ("DEDIVP", 0x4040_0000, 0x4080_0000),
    ] {
        let program = compile_il(&format!("LD X0\n{name} D0E0 D10F0 D20E1\nEND")).unwrap();
        let mut sim = Simulator::default();
        sim.memory.set_word(d("D1028"), -30000);
        seed(&mut sim.memory);
        run(&mut sim, &program);
        assert!(sim.memory.execution_limitations.is_empty());
        assert_eq!(flags(&sim.memory), [true, false, true]);
        assert!(!sim.observed_devices.contains(&d("D1028")));
        sim.memory.set_word(d("D1028"), 0);
        put(&mut sim.memory, "D0", 0x40c0_0000);
        put(&mut sim.memory, "D10", 0x4000_0000);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &program);
        assert_eq!(get(&sim.memory, "D20"), first);
        sim.memory.set_word(d("D1028"), -30000);
        seed(&mut sim.memory);
        sim.observed_devices.clear();
        run(&mut sim, &program);
        assert!(sim.memory.execution_limitations.is_empty());
        assert_eq!(flags(&sim.memory), [true, false, true]);
        assert!(!sim.observed_devices.contains(&d("D1028")));
        sim.memory.set_input(d("X0"), false);
        run(&mut sim, &program);
        sim.memory.set_word(d("D1028"), 0);
        put(&mut sim.memory, "D0", 0x4140_0000);
        put(&mut sim.memory, "D10", 0x4040_0000);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &program);
        assert_eq!(get(&sim.memory, "D20"), second);
    }
}

#[test]
fn both_sources_snapshot_before_overlapping_destination_or_self_index_write() {
    let program = compile_il("LD X0\nDEADD D0E0 D2F0 D1E1\nEND").unwrap();
    let mut sim = enabled();
    put(&mut sim.memory, "D0", 0x3fc0_0000); // 1.5
    put(&mut sim.memory, "D2", 0x4010_0000); // 2.25
    run(&mut sim, &program);
    assert_eq!(get(&sim.memory, "D1"), 0x4070_0000); // 3.75
    assert_eq!(sim.memory.word(d("D0")), 0);
    assert_eq!(sim.memory.word(d("D3")) as u16, 0x4010);

    let program = compile_il("LD X0\nDEADD D0F1 K0 D1027E0\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_word(d("D1028"), 1);
    put(&mut sim.memory, "D0", 0x3f8c_cccd);
    sim.memory.set_word(d("D1030"), 77);
    run(&mut sim, &program);
    assert_eq!(sim.memory.word(d("D1028")) as u16, 0xcccd);
    assert_eq!(sim.memory.word(d("D1029")) as u16, 0x3f8c);
    assert_eq!(sim.memory.word(d("D1030")), 77);
    assert!(sim.memory.execution_limitations.is_empty());
}

#[test]
fn repeated_same_scan_visits_read_changed_indices_and_observe_actual_pairs() {
    let program =
        compile_il("LD X0\nFOR K3\nDEADD D10E0 K1 D100E0\nADD E0 K2 E0\nNEXT\nEND").unwrap();
    let mut sim = enabled();
    put(&mut sim.memory, "D10", 0x3f80_0000);
    put(&mut sim.memory, "D12", 0x4000_0000);
    put(&mut sim.memory, "D14", 0x4040_0000);
    run(&mut sim, &program);
    assert_eq!(
        ["D100", "D102", "D104"].map(|address| get(&sim.memory, address)),
        [0x4000_0000, 0x4040_0000, 0x4080_0000]
    );
    assert_eq!(sim.memory.word(d("D1028")), 6);
    assert!(sim.observed_devices.contains(&d("D15")) && sim.observed_devices.contains(&d("D105")));
}

#[test]
fn model_endpoints_validate_both_sources_and_the_entire_fixed_result_pair() {
    for last in [4999, 9999] {
        let program =
            compile_il(&format!("LD X0\nDEADD D{last}E0 D{last}F0 D{last}E1\nEND")).unwrap();
        let mut sim = enabled();
        sim.memory.data_register_limit = Some(last);
        for alias in ["D1028", "D1029", "D1182"] {
            sim.memory.set_word(d(alias), -1);
        }
        put(&mut sim.memory, &format!("D{}", last - 1), 0x3f80_0000);
        run(&mut sim, &program);
        assert_eq!(get(&sim.memory, &format!("D{}", last - 1)), 0x4000_0000);
        assert!(sim.memory.execution_limitations.is_empty());
        for (left, right, target) in [
            (format!("D{last}E2"), "K1".into(), "D20E0".into()),
            ("K1".into(), format!("D{last}E2"), "D20E0".into()),
            ("K1".into(), "K1".into(), format!("D{last}E2")),
        ] {
            let action = IndexedFloatBinaryAction::parse("DEADD", &[left, right, target]).unwrap();
            assert!(action.prepare(&sim.memory, &mut BTreeSet::new()).is_err());
            assert!(!sim.memory.system_bit(1067));
        }
    }
    let mut memory = Memory::default();
    memory.data_register_limit = Some(4999);
    assert!(
        parse("DEADD D4999 K1 D20E0")
            .resolved_source(0, &memory)
            .is_err()
    );
    assert!(
        parse("DEADD K1 D4999 D20E0")
            .resolved_source(1, &memory)
            .is_err()
    );
    assert!(
        parse("DEADD K1 D0E0 D4999")
            .prepare(&memory, &mut BTreeSet::new())
            .is_err()
    );
}

#[test]
fn source_limitations_hold_flags_but_success_publishes_flags_before_bad_target() {
    let mut memory = Memory::default();
    memory.set_word(d("D1028"), -30000);
    seed(&mut memory);
    let mut observed = BTreeSet::new();
    parse("DEDIV D0E0 D10F0 D20E1").execute(&mut memory, true, &mut observed);
    assert!(!memory.execution_limitations.is_empty());
    assert_eq!(flags(&memory), [true, false, true]);
    assert!(!memory.system_bit(1067));
    assert!(!observed.contains(&d("D1029")));

    let mut memory = Memory::default();
    put(&mut memory, "D0", 0x7fc0_0001);
    memory.set_word(d("D1029"), -30000);
    seed(&mut memory);
    parse("DEDIV D0E0 D10F0 D20E1").execute(&mut memory, true, &mut BTreeSet::new());
    assert!(!memory.execution_limitations.is_empty());
    assert_eq!(flags(&memory), [true, false, true]);
    assert!(
        !memory.system_bit(1067),
        "both source snapshots precede NaN validation"
    );

    let mut memory = Memory::default();
    memory.set_word(d("D1195"), -30000);
    seed(&mut memory);
    parse("DESUB K1 K1 D20F7").execute(&mut memory, true, &mut BTreeSet::new());
    assert_eq!(flags(&memory), [true, false, false]);
    assert!(!memory.execution_limitations.is_empty());
    assert!(!memory.system_bit(1067));
}

#[test]
fn failed_division_still_has_an_independently_displayable_held_result() {
    let action = parse("DEDIV D0E0 D10F0 D100E1");
    let mut memory = Memory::default();
    memory.set_word(d("D1028"), 1);
    memory.set_word(d("D1029"), 2);
    memory.set_word(d("D1182"), 3);
    put(&mut memory, "D1", 0x40c0_0000);
    put(&mut memory, "D12", 0x4000_0000);
    let mut observed = BTreeSet::new();
    action.execute(&mut memory, true, &mut observed);
    assert_eq!(action.display_value(&memory), "3");
    for actual in ["D1", "D2", "D12", "D13", "D103", "D104"] {
        assert!(observed.contains(&d(actual)));
    }
    assert!(!observed.contains(&d("D0")) && !observed.contains(&d("D100")));
    put(&mut memory, "D12", 0);
    observed.clear();
    assert_eq!(
        action.prepare(&memory, &mut observed).unwrap(),
        PreparedFloatBinary::SemanticError {
            code: 0xe19,
            offset: 5
        }
    );
    assert!(!observed.contains(&d("D1182")) && !observed.contains(&d("D103")));
    let mut ui = BTreeSet::new();
    action.observed_devices(&memory, &mut ui);
    assert!(ui.contains(&d("D103")) && ui.contains(&d("D104")));
    assert_eq!(action.display_value(&memory), "3");
    memory.set_word(d("D1182"), -30000);
    assert!(action.display_value(&memory).starts_with("İndeks:"));
}

#[test]
fn shared_seam_retains_manually_constructed_literal_and_ordinary_api_behavior() {
    let action = FloatMathAction {
        op: MathOp::Divide,
        source: FloatOperand::Integer(1),
        right: Some(FloatOperand::Literal {
            bits: 0x8000_0000,
            text: "F-0".into(),
        }),
        dest: d("D20"),
    };
    let mut memory = Memory::default();
    action.execute(&mut memory);
    assert_eq!(get(&memory, "D20"), 0xff80_0000);
    assert_eq!(flags(&memory), [false, true, false]);
    assert!(!memory.system_bit(1067));
    let ordinary =
        FloatMathAction::parse("DEDIV", &["K-1".into(), "H80000000".into(), "D20".into()]).unwrap();
    ordinary.execute(&mut memory);
    assert_eq!(get(&memory, "D20"), 0x7f80_0000);
    assert_eq!(flags(&memory), [false, false, true]);
    let ordinary =
        FloatMathAction::parse("DEADD", &["K16777217".into(), "K1".into(), "D20".into()]).unwrap();
    ordinary.execute(&mut memory);
    assert_eq!(get(&memory, "D20"), 0x4b80_0000);
    assert_eq!(flags(&memory), [false; 3]);
}
