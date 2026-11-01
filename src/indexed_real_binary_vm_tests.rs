//! Independent raw-bit, ordered-access and native-layout expectations for
//! indexed R arithmetic. Extreme native overflow responsiveness is separate.
use crate::float_math::{FloatBinaryError, FloatMathAction};
use crate::indexed_real_binary::{IndexedRealBinaryAction, PreparedRealBinary};
use crate::plc::{Device, Memory, Program, ScanStatus, Simulator, compile_il};
use std::collections::BTreeSet;

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn parse(raw: &str) -> IndexedRealBinaryAction {
    let mut parts = raw.split_whitespace();
    IndexedRealBinaryAction::parse(
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
    [1020, 1021, 1022].map(|flag| memory.system_bit(flag))
}
fn seed(memory: &mut Memory) {
    for (flag, value) in [(1020, true), (1021, false), (1022, true)] {
        memory.set_system(flag, value);
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
fn trace_scan(sim: &mut Simulator, program: &Program) -> Vec<(u16, u16)> {
    sim.begin_plan_scan(program, 0, false).unwrap();
    let mut trace = Vec::new();
    for _ in 0..128 {
        let step = sim.execute_plan_step(program).unwrap();
        trace.push((step.pc, step.next_pc));
        if step.scan_boundary {
            return trace;
        }
    }
    panic!("bounded test scan did not finish");
}

#[test]
fn eight_forms_encode_literal_fields_and_keep_ddivr_padding_outside_the_operand_words() {
    for (name, header, expected) in [
        ("DADDR", 0x7701, 0x4000_0000),
        ("DADDRP", 0x7711, 0x4000_0000),
        ("DSUBR", 0x7901, 0x3f80_0000),
        ("DSUBRP", 0x7911, 0x3f80_0000),
        ("DMULR", 0x7b01, 0x3f40_0000),
        ("DMULRP", 0x7b11, 0x3f40_0000),
        ("DDIVR", 0x7d01, 0x4040_0000),
        ("DDIVRP", 0x7d11, 0x4040_0000),
    ] {
        let body = format!("{name} F1.5 D10E1 D100F7");
        let action = parse(&body);
        let expected_words = [
            header, 0x0080, 0x0080, 0xc080, 0x3f80, 0x14e6, 0x8086, 0x0080, 0x0080, 0xc8f6, 0x8089,
            0x0080, 0x0080,
        ];
        assert_eq!(action.native_words().unwrap(), expected_words);
        let program = compile_il(&format!("LD X0\n{body}\nMOV K7 D300\nOUT Y0\nEND")).unwrap();
        assert!(program.requires_vm());
        let encoded = crate::es2_encoder::encode(&program).unwrap();
        assert_eq!(&encoded.words[1..14], &expected_words);
        assert_eq!(
            encoded.action_pc[1],
            if action.is_divide_real() { 18 } else { 14 }
        );
        if action.is_divide_real() {
            assert_eq!(&encoded.words[14..18], &[0xfdf0; 4]);
            assert_eq!(encoded.padded_error_skip.get(&1), Some(&18));
        } else {
            assert!(encoded.padded_error_skip.is_empty());
        }
        let mut sim = enabled();
        sim.memory.set_word(d("D1182"), 1);
        sim.memory.set_word(d("D1195"), 2);
        put(&mut sim.memory, "D11", 0x3f00_0000); // 0.5
        sim.memory.set_word(d("D101"), 77);
        sim.memory.set_word(d("D104"), 88);
        seed(&mut sim.memory);
        run(&mut sim, &program);
        assert_eq!(get(&sim.memory, "D102"), expected, "{name}");
        assert_eq!(flags(&sim.memory), [false; 3]);
        assert_eq!(
            (sim.memory.word(d("D101")), sim.memory.word(d("D104"))),
            (77, 88)
        );
        assert_eq!(sim.memory.word(d("D300")), 7);
        assert!(sim.memory.bit(d("Y0")));
    }
}

#[test]
fn all_sixteen_modifier_aliases_are_signed_words_in_all_three_roles() {
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
            "LD X0\nDMULR D10{suffix} D20{suffix} D100{suffix}\nEND"
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
        assert!(!sim.observed_devices.contains(&neighbor));
    }
}

#[test]
fn r_role_matrix_keeps_f_literals_distinct_from_f_modifiers_and_integer_constants() {
    for name in [
        "DADDR", "DADDRP", "DSUBR", "DSUBRP", "DMULR", "DMULRP", "DDIVR", "DDIVRP",
    ] {
        for values in [
            ["F7", "D0F7", "D20"],
            ["D0E0", "F-0.0", "D20F1"],
            ["F1.200E+0", "F2.200E+0", "D20E0"],
        ] {
            assert!(IndexedRealBinaryAction::parse(name, &values.map(str::to_owned)).is_ok());
        }
        for values in [
            ["K1", "D0E0", "D20"],
            ["H3F800000", "D0E0", "D20"],
            ["E1", "D0E0", "D20"],
            ["T0E0", "F1", "D20"],
            ["C200", "F1", "D20E0"],
            ["K4M0E0", "F1", "D20"],
            ["D0E8", "F1", "D20"],
            ["FNaN", "D0E0", "D20"],
            ["FInf", "D0E0", "D20"],
            ["F1", "D0E0", "F1"],
            ["F1", "D0E0", "E1"],
            ["F1", "D0E0", "K4M0"],
            ["F1", "D0E0", "T0"],
            ["F1", "D0E0", "D9999"],
        ] {
            assert!(
                IndexedRealBinaryAction::parse(name, &values.map(str::to_owned)).is_err(),
                "{name} {values:?}"
            );
        }
    }
    for name in ["DEADD", "DEMUL", "DRAD", "DEBCD", "ADDR", "DADDRPP"] {
        assert!(!IndexedRealBinaryAction::supports(name));
    }
    assert!(!IndexedRealBinaryAction::has_modifiers(&[
        "F1.200E+0".into(),
        "F-0".into(),
        "D20".into()
    ]));
}

#[test]
fn scientific_literals_and_negative_zero_roundtrip_with_literal_ieee_words() {
    let action = parse("daddrp f1.200e+0 f-0.0 d20f7");
    assert_eq!(action.label(), "DADDRP F1.200E+0 F-0.0 D20F7");
    assert_eq!(parse(&action.label()), action);
    assert_eq!(action.dest(), d("D20"));
    assert_eq!(action.source_modifier_devices(0), Vec::<Device>::new());
    assert_eq!(action.target_modifier_devices(), vec![d("D1195")]);
    assert_eq!(action.resolved_source(0, &Memory::default()).unwrap(), None);
    assert!(action.resolved_source(2, &Memory::default()).is_err());
    assert_eq!(
        parse("DMULR F-0.0 D0E0 D20F0").native_words().unwrap(),
        vec![
            0x7b01, 0x0080, 0x0080, 0x0080, 0x8080, 0x00e6, 0x0086, 0x0080, 0x0080, 0x28f6, 0x0086,
            0x0080, 0x0080,
        ]
    );
    let mut memory = Memory::default();
    put(&mut memory, "D0", 0x4000_0000);
    memory.set_system(1081, true);
    parse("DMULR F-0.0 D0E0 D20F0").execute(&mut memory, true, &mut BTreeSet::new());
    assert_eq!(get(&memory, "D20"), 0);
    assert_eq!(flags(&memory), [true, false, false]);
    assert!(memory.system_bit(1081));
}

#[test]
fn first_nan_stops_before_an_invalid_right_index_and_before_target() {
    for name in ["DADDR", "DSUBR", "DMULR"] {
        for nan in [0x7fc0_1234, 0xff80_0001] {
            let action = parse(&format!("{name} D0E0 D10F0 D100F7"));
            let mut memory = Memory::default();
            put(&mut memory, "D0", nan);
            memory.set_word(d("D1029"), -30000);
            memory.set_word(d("D1195"), -30000);
            seed(&mut memory);
            let mut observed = BTreeSet::new();
            assert!(!action.needs_right_source(&memory).unwrap());
            assert_eq!(
                action.semantic_error(&memory).unwrap(),
                Some(FloatBinaryError {
                    code: 0xe1a,
                    offset: 1
                })
            );
            assert_eq!(
                action.prepare(&memory, &mut observed).unwrap(),
                PreparedRealBinary::SemanticError {
                    code: 0xe1a,
                    offset: 1
                }
            );
            assert_eq!(flags(&memory), [true, false, true]);
            assert!(!memory.system_bit(1067));
            assert!(!observed.contains(&d("D1029")) && !observed.contains(&d("D1195")));
            assert!(!action.execute(&mut memory, true, &mut observed));
            assert_eq!(memory.word(d("D1067")), 0xe1a);
            assert_eq!(memory.word(d("D1068")), 1);
            assert_eq!(flags(&memory), [true, false, true]);
            assert!(memory.execution_limitations.is_empty());
        }
    }
}

#[test]
fn second_nan_records_offset_five_only_after_both_pairs_are_observed() {
    for name in ["DADDR", "DSUBR", "DMULR"] {
        let program = compile_il(&format!("LD X0\n{name} D0E0 D10F0 D100F7\nEND")).unwrap();
        let mut sim = enabled();
        put(&mut sim.memory, "D0", 0x3f80_0000);
        put(&mut sim.memory, "D10", 0x7fc0_0001);
        put(&mut sim.memory, "D100", 0x1234_5678);
        sim.memory.set_word(d("D1195"), -30000);
        seed(&mut sim.memory);
        run(&mut sim, &program);
        assert_eq!(sim.memory.word(d("D1067")), 0xe1a);
        assert_eq!(
            sim.memory.word(d("D1068")),
            (program.native_action_pc[0] + 5) as i16
        );
        assert_eq!(flags(&sim.memory), [true, false, true]);
        assert_eq!(get(&sim.memory, "D100"), 0x1234_5678);
        for actual in ["D0", "D1", "D10", "D11", "D1028", "D1029"] {
            assert!(sim.observed_devices.contains(&d(actual)));
        }
        assert!(!sim.observed_devices.contains(&d("D1195")));
    }
}

#[test]
fn ddivr_reads_both_sources_and_zero_or_subnormal_precedes_nan_at_offset_nine() {
    for (left, right, code, skip) in [
        (0x7fc0_1234, 0, 0xe19, false),
        (0x7fc0_1234, 0x8000_0000, 0xe19, false),
        (0x7fc0_1234, 1, 0xe19, false),
        (0x7fc0_1234, 0x8000_0001, 0xe19, false),
        (0x7fc0_1234, 0x3f80_0000, 0xe1a, true),
        (0x3f80_0000, 0x7fc0_1234, 0xe1a, true),
    ] {
        let action = parse("DDIVR D0E0 D10F0 D100F7");
        let mut memory = Memory::default();
        put(&mut memory, "D0", left);
        put(&mut memory, "D10", right);
        put(&mut memory, "D100", 0x1234_5678);
        memory.set_word(d("D1195"), -30000);
        seed(&mut memory);
        assert!(action.needs_right_source(&memory).unwrap());
        assert_eq!(
            action.semantic_error(&memory).unwrap(),
            Some(FloatBinaryError { code, offset: 9 })
        );
        let mut observed = BTreeSet::new();
        assert_eq!(action.execute(&mut memory, true, &mut observed), skip);
        assert_eq!(memory.word(d("D1067")), code);
        assert_eq!(memory.word(d("D1068")), 9);
        assert_eq!(flags(&memory), [true, false, true]);
        assert_eq!(get(&memory, "D100"), 0x1234_5678);
        assert!(observed.contains(&d("D1029")) && observed.contains(&d("D10")));
        assert!(!observed.contains(&d("D1195")));
        assert!(memory.execution_limitations.is_empty());
    }
}

#[test]
fn ddivr_vm_skip_uses_current_outcome_instead_of_sticky_error_registers() {
    let program = compile_il("LD X0\nDDIVR D0E0 D10F0 D20E1\nMOV K7 D30\nOUT Y0\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_system(1067, true);
    sim.memory.set_system(1068, true);
    sim.memory.set_word(d("D1067"), 0xe19);
    sim.memory.set_word(d("D1068"), 321);
    put(&mut sim.memory, "D0", 0x7fc0_0001);
    put(&mut sim.memory, "D10", 0x3f80_0000);
    let trace = trace_scan(&mut sim, &program);
    assert!(trace.contains(&(1, 18)));
    assert!(!trace.iter().any(|(pc, _)| (14..18).contains(pc)));
    assert_eq!(sim.memory.word(d("D1067")), 0xe19);
    assert_eq!(sim.memory.word(d("D1068")), 321);
    assert_eq!(sim.memory.word(d("D30")), 7);
    assert!(sim.memory.bit(d("Y0")));

    sim.memory.set_word(d("D1067"), 0xe1a);
    put(&mut sim.memory, "D0", 0x3f80_0000);
    put(&mut sim.memory, "D10", 0);
    let trace = trace_scan(&mut sim, &program);
    assert!(trace.contains(&(1, 14)));
    assert!(trace.contains(&(14, 15)) && trace.contains(&(17, 18)));
    assert_eq!(sim.memory.word(d("D1067")), 0xe1a);
    assert!(sim.memory.bit(d("Y0")));

    sim.memory.set_input(d("X0"), false);
    put(&mut sim.memory, "D0", 0x7fc0_0001);
    put(&mut sim.memory, "D10", 0x3f80_0000);
    let trace = trace_scan(&mut sim, &program);
    assert!(trace.contains(&(1, 14)));
    assert!(!sim.memory.bit(d("Y0")));
}

#[test]
fn ordinary_ddivr_in_a_vm_program_has_the_same_fresh_nan_skip() {
    // The ordinary network executor can ignore the new return value: its
    // action order is independent of the native padding words.
    let ordinary = compile_il("LD X0\nDDIVR D0 D10 D20\nMOV K77 D42\nOUT Y0\nEND").unwrap();
    assert!(!ordinary.requires_vm());
    let mut network = enabled();
    put(&mut network.memory, "D0", 0x7fc0_0001);
    put(&mut network.memory, "D10", 0x3f80_0000);
    run(&mut network, &ordinary);
    assert_eq!(network.memory.word(d("D1067")), 0xe1a);
    assert_eq!(network.memory.word(d("D42")), 77);
    assert!(network.memory.bit(d("Y0")));

    let program =
        compile_il("LD X0\nDDIVR D0 D10 D20\nMOV D40E0 D42\nDADDR F1 F2 D50E0\nOUT Y0\nEND")
            .unwrap();
    assert!(program.requires_vm());
    let mut sim = enabled();
    put(&mut sim.memory, "D0", 0x7fc0_0001);
    put(&mut sim.memory, "D10", 0x3f80_0000);
    sim.memory.set_word(d("D40"), 77);
    let trace = trace_scan(&mut sim, &program);
    assert!(trace.contains(&(1, 18)));
    assert_eq!(sim.memory.word(d("D42")), 77);
    assert!(sim.memory.bit(d("Y0")));
    put(&mut sim.memory, "D10", 0);
    let trace = trace_scan(&mut sim, &program);
    assert!(trace.contains(&(1, 14)));
    assert_eq!(
        sim.memory.word(d("D1067")),
        0xe1a,
        "old error is latched but current path is E19"
    );
}

#[test]
fn pulse_off_and_held_skip_all_reads_and_do_not_reuse_the_previous_nan_override() {
    for name in ["DADDRP", "DSUBRP", "DMULRP", "DDIVRP"] {
        let program = compile_il(&format!("LD X0\n{name} D0E0 D10F0 D20E1\nOUT Y0\nEND")).unwrap();
        let mut sim = Simulator::default();
        sim.memory.set_word(d("D1028"), -30000);
        seed(&mut sim.memory);
        run(&mut sim, &program);
        assert!(sim.memory.execution_limitations.is_empty());
        assert!(!sim.observed_devices.contains(&d("D1028")));
        sim.memory.set_word(d("D1028"), 0);
        put(&mut sim.memory, "D0", 0x7fc0_0001);
        put(&mut sim.memory, "D10", 0x3f80_0000);
        sim.memory.set_input(d("X0"), true);
        let trace = trace_scan(&mut sim, &program);
        assert!(trace.contains(&(1, if name == "DDIVRP" { 18 } else { 14 })));
        sim.memory.set_word(d("D1028"), -30000);
        sim.observed_devices.clear();
        seed(&mut sim.memory);
        let trace = trace_scan(&mut sim, &program);
        assert!(trace.contains(&(1, 14)));
        assert!(sim.memory.execution_limitations.is_empty());
        assert!(!sim.observed_devices.contains(&d("D1028")));
        assert_eq!(flags(&sim.memory), [true, false, true]);
        sim.memory.set_input(d("X0"), false);
        run(&mut sim, &program);
        sim.memory.set_word(d("D1028"), 0);
        put(&mut sim.memory, "D0", 0x40c0_0000);
        put(&mut sim.memory, "D10", 0x4000_0000);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &program);
        let expected = match name {
            "DADDRP" => 0x4100_0000,
            "DSUBRP" => 0x4080_0000,
            "DMULRP" => 0x4140_0000,
            _ => 0x4040_0000,
        };
        assert_eq!(get(&sim.memory, "D20"), expected);
    }
}

#[test]
fn success_normalization_handles_signed_zero_subnormal_and_infinite_inputs() {
    for (name, left, right, result, expected_flags) in [
        ("DADDR", 0x8000_0000, 0x8000_0000, 0, [true, false, false]),
        ("DSUBR", 0x3f80_0000, 0x3f80_0000, 0, [true, false, false]),
        ("DADDR", 1, 1, 2, [false, false, false]),
        (
            "DMULR",
            0x7f80_0000,
            0x4000_0000,
            0x7f80_0000,
            [false, false, true],
        ),
        (
            "DMULR",
            0xff80_0000,
            0x4000_0000,
            0xff80_0000,
            [false, true, false],
        ),
        ("DDIVR", 0x3f80_0000, 0x7f80_0000, 0, [true, false, false]),
    ] {
        let mut memory = Memory::default();
        put(&mut memory, "D0", left);
        put(&mut memory, "D10", right);
        seed(&mut memory);
        assert!(!parse(&format!("{name} D0E0 D10F0 D20E1")).execute(
            &mut memory,
            true,
            &mut BTreeSet::new()
        ));
        assert_eq!(get(&memory, "D20"), result);
        assert_eq!(flags(&memory), expected_flags);
        assert!(!memory.system_bit(1067));
    }
}

#[test]
fn source_overlap_and_self_index_result_keep_snapshots_and_one_fixed_pair() {
    let program = compile_il("LD X0\nDADDR D0E0 D2F0 D1E1\nEND").unwrap();
    let mut sim = enabled();
    put(&mut sim.memory, "D0", 0x3fc0_0000);
    put(&mut sim.memory, "D2", 0x4010_0000);
    run(&mut sim, &program);
    assert_eq!(get(&sim.memory, "D1"), 0x4070_0000);
    assert_eq!(sim.memory.word(d("D0")), 0);
    assert_eq!(sim.memory.word(d("D3")) as u16, 0x4010);
    let program = compile_il("LD X0\nDADDR F1.1 F0 D1027E0\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_word(d("D1028"), 1);
    sim.memory.set_word(d("D1030"), 77);
    run(&mut sim, &program);
    assert_eq!(get(&sim.memory, "D1028"), 0x3f8c_cccd);
    assert_eq!(sim.memory.word(d("D1030")), 77);
    assert!(sim.memory.execution_limitations.is_empty());
}

#[test]
fn repeated_visits_use_live_indices_and_selected_model_pair_bounds() {
    let program =
        compile_il("LD X0\nFOR K3\nDADDR D10E0 F1 D100E0\nADD E0 K2 E0\nNEXT\nEND").unwrap();
    let mut sim = enabled();
    for (address, bits) in [
        ("D10", 0x3f80_0000),
        ("D12", 0x4000_0000),
        ("D14", 0x4040_0000),
    ] {
        put(&mut sim.memory, address, bits);
    }
    run(&mut sim, &program);
    assert_eq!(
        ["D100", "D102", "D104"].map(|address| get(&sim.memory, address)),
        [0x4000_0000, 0x4040_0000, 0x4080_0000]
    );
    assert_eq!(sim.memory.word(d("D1028")), 6);
    for last in [4999, 9999] {
        let action = parse(&format!("DADDR D{last}E0 D{last}F0 D{last}E1"));
        let mut memory = Memory::default();
        memory.data_register_limit = Some(last);
        for alias in ["D1028", "D1029", "D1182"] {
            memory.set_word(d(alias), -1);
        }
        put(&mut memory, &format!("D{}", last - 1), 0x3f80_0000);
        action.execute(&mut memory, true, &mut BTreeSet::new());
        assert_eq!(get(&memory, &format!("D{}", last - 1)), 0x4000_0000);
        assert!(memory.execution_limitations.is_empty());
        memory.set_word(d("D1028"), 0);
        assert!(action.resolved_source(0, &memory).is_err());
        memory.set_word(d("D1028"), -1);
        memory.set_word(d("D1029"), 0);
        assert!(action.resolved_source(1, &memory).is_err());
        memory.set_word(d("D1029"), -1);
        memory.set_word(d("D1182"), 0);
        assert!(action.prepare(&memory, &mut BTreeSet::new()).is_err());
    }
    let mut memory = Memory::default();
    memory.data_register_limit = Some(4999);
    assert!(
        parse("DADDR D4999 F1 D20E0")
            .resolved_source(0, &memory)
            .is_err()
    );
    assert!(
        parse("DADDR F1 D4999 D20E0")
            .resolved_source(1, &memory)
            .is_err()
    );
    assert!(
        parse("DADDR F1 D0E0 D4999")
            .prepare(&memory, &mut BTreeSet::new())
            .is_err()
    );
}

#[test]
fn structural_limits_are_separate_from_nan_errors_and_success_flag_publication() {
    let mut memory = Memory::default();
    put(&mut memory, "D0", 0x7fc0_0001);
    memory.set_word(d("D1029"), -30000);
    seed(&mut memory);
    let action = parse("DDIVR D0E0 D10F0 D20E1");
    assert!(action.needs_right_source(&memory).unwrap());
    assert!(action.semantic_error(&memory).is_err());
    assert!(!action.execute(&mut memory, true, &mut BTreeSet::new()));
    assert!(!memory.execution_limitations.is_empty());
    assert!(!memory.system_bit(1067));
    assert_eq!(flags(&memory), [true, false, true]);
    let mut memory = Memory::default();
    memory.set_word(d("D1195"), -30000);
    seed(&mut memory);
    parse("DSUBR F1 F1 D20F7").execute(&mut memory, true, &mut BTreeSet::new());
    assert_eq!(flags(&memory), [true, false, false]);
    assert!(!memory.execution_limitations.is_empty());
    assert!(!memory.system_bit(1067));
}

#[test]
fn execution_observation_is_lazy_but_held_result_monitoring_is_independent() {
    let action = parse("DADDR D0E0 D10F0 D100E1");
    let mut memory = Memory::default();
    memory.set_word(d("D1028"), 1);
    memory.set_word(d("D1029"), 2);
    memory.set_word(d("D1182"), 3);
    put(&mut memory, "D1", 0x3fc0_0000);
    put(&mut memory, "D12", 0x4010_0000);
    let mut observed = BTreeSet::new();
    action.execute(&mut memory, true, &mut observed);
    for address in ["D1", "D2", "D12", "D13", "D103", "D104"] {
        assert!(observed.contains(&d(address)));
    }
    assert!(!observed.contains(&d("D0")) && !observed.contains(&d("D100")));
    assert_eq!(action.display_value(&memory), "3.75");
    put(&mut memory, "D1", 0x7fc0_0001);
    observed.clear();
    assert_eq!(
        action.prepare(&memory, &mut observed).unwrap(),
        PreparedRealBinary::SemanticError {
            code: 0xe1a,
            offset: 1
        }
    );
    assert!(!observed.contains(&d("D1029")) && !observed.contains(&d("D1182")));
    let mut ui = BTreeSet::new();
    action.observed_devices(&memory, &mut ui);
    assert!(ui.contains(&d("D12")) && ui.contains(&d("D103")));
    assert_eq!(action.display_value(&memory), "3.75");
    memory.set_word(d("D1182"), -30000);
    assert!(action.display_value(&memory).starts_with("İndeks:"));
}

#[test]
fn ordinary_and_indexed_r_math_share_fresh_skip_and_staged_error_semantics() {
    for (name, left, right, code, offset, skip) in [
        ("DADDR", 0x7fc0_0001, 0x3f80_0000, 0xe1a, 1, false),
        ("DMULR", 0x3f80_0000, 0x7fc0_0001, 0xe1a, 5, false),
        ("DDIVR", 0x7fc0_0001, 0x8000_0000, 0xe19, 9, false),
        ("DDIVR", 0x7fc0_0001, 0x3f80_0000, 0xe1a, 9, true),
    ] {
        let mut ordinary = Memory::default();
        put(&mut ordinary, "D0", left);
        put(&mut ordinary, "D10", right);
        put(&mut ordinary, "D20", 0x1234_5678);
        seed(&mut ordinary);
        let mut indexed = ordinary.clone();
        let action =
            FloatMathAction::parse(name, &["D0".into(), "D10".into(), "D20".into()]).unwrap();
        assert_eq!(action.execute(&mut ordinary), skip);
        assert_eq!(
            parse(&format!("{name} D0E0 D10F0 D20E1")).execute(
                &mut indexed,
                true,
                &mut BTreeSet::new()
            ),
            skip
        );
        for memory in [&ordinary, &indexed] {
            assert_eq!(memory.word(d("D1067")), code);
            assert_eq!(memory.word(d("D1068")), offset);
            assert_eq!(flags(memory), [true, false, true]);
            assert_eq!(get(memory, "D20"), 0x1234_5678);
        }
    }
}
