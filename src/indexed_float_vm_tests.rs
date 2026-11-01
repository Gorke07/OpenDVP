//! Independent values, flags, fields and actual access widths for FLT/INT.
use crate::indexed_float::{IndexedFloatAction, PreparedFloat};
use crate::plc::{Area, Device, Memory, Program, ScanStatus, Simulator, compile_il};
use std::collections::BTreeSet;

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn parse(text: &str) -> IndexedFloatAction {
    let parts = text
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    IndexedFloatAction::parse(&parts[0], &parts[1..]).unwrap()
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
    for (n, v) in (1020..=1022).zip(values) {
        memory.set_system(n, v);
    }
}

#[test]
fn eight_forms_have_literal_indexed_fields_and_independent_results() {
    for (name, wide, header, input, expected, wanted_flags) in [
        ("FLT", false, 0x7200, 10, 0x4120_0000, [true, false, true]),
        ("FLTP", false, 0x7210, 10, 0x4120_0000, [true, false, true]),
        (
            "DFLT",
            true,
            0x7300,
            100_000,
            0x47c3_5000,
            [true, false, true],
        ),
        (
            "DFLTP",
            true,
            0x7310,
            100_000,
            0x47c3_5000,
            [true, false, true],
        ),
        ("INT", false, 0x2201, 0x414c_0000, 12, [false, true, false]),
        ("INTP", false, 0x2211, 0x414c_0000, 12, [false, true, false]),
        (
            "DINT",
            true,
            0x2301,
            0x47c3_5000,
            100_000,
            [false, false, false],
        ),
        (
            "DINTP",
            true,
            0x2311,
            0x47c3_5000,
            100_000,
            [false, false, false],
        ),
    ] {
        let program = compile_il(&format!("LD X0\n{name} D10E1 D100F7\nEND")).unwrap();
        assert!(program.requires_vm());
        let encoded = crate::es2_encoder::encode(&program).unwrap();
        let pc = usize::from(encoded.action_pc[0]);
        let expected_fields = if wide {
            vec![
                header, 0x14e6, 0x8086, 0x0080, 0x0080, 0xc8f6, 0x8089, 0x0080, 0x0080,
            ]
        } else {
            vec![header, 0x14e6, 0x8086, 0xc8f6, 0x8089]
        };
        assert_eq!(
            &encoded.words[pc..pc + expected_fields.len()],
            expected_fields,
            "{name}"
        );
        let mut sim = enabled();
        sim.memory.set_word(d("D1182"), 1);
        sim.memory.set_word(d("D1195"), 2);
        sim.memory.set_double_word(d("D11"), input);
        sim.memory.set_word(d("D104"), 77);
        seed_flags(&mut sim.memory, [true, false, true]);
        run(&mut sim, &program);
        let result = if name.contains("FLT") || wide {
            sim.memory.double_word(d("D102"))
        } else {
            i32::from(sim.memory.word(d("D102")))
        };
        assert_eq!(result, expected, "{name}");
        assert_eq!(flags(&sim.memory), wanted_flags, "{name}");
        assert_eq!(sim.memory.word(d("D104")), 77);
    }
}

#[test]
fn documented_role_matrix_does_not_confuse_values_with_modifiers() {
    for text in [
        "FLT T0E0 D20",
        "DFLT C200E0 D20",
        "FLTP D0E0 T0",
        "INT E0 D20E1",
        "DINT F1 D20E1",
        "FLT K1 D20E0",
        "INT H41200000 D20E0",
        "INT K4M0E0 D20",
        "DINT D0E0 K8M0",
        "FLT D0E8 D20",
        "INT D0E0 C200",
        "INT D0E0 C200E1",
    ] {
        assert!(
            compile_il(&format!("LD X0\n{text}\nEND")).is_err(),
            "{text}"
        );
    }
    for name in ["FLTP", "DFLTP", "INTP", "DINTP"] {
        assert!(IndexedFloatAction::supports(name));
    }
    for name in ["FLTPP", "INTPP", "DEINT", "DMOVR", "DEBCD"] {
        assert!(!IndexedFloatAction::supports(name));
    }
    assert_eq!(parse("FLT D0E0 D20").mode_device(), Some(d("M1081")));
    assert_eq!(parse("INT D0E0 D20").mode_device(), None);
}

#[test]
fn same_scan_mode_changes_select_forward_and_reverse_widths_at_each_visit() {
    let program = compile_il("LD X0\nRST M1081\nFLT D10E0 D20F0\nSET M1081\nFLT D20F0 D30E0\nRST M1081\nDFLT D40E0 D50F0\nSET M1081\nDFLT D50F0 D60E0\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_word(d("D1028"), 1);
    sim.memory.set_word(d("D1029"), 2);
    sim.memory.set_word(d("D11"), -23);
    sim.memory.set_word(d("D12"), 0x7fff); // Forward FLT must not read this high half.
    sim.memory.set_double_word(d("D41"), 100_000);
    sim.memory.set_word(d("D32"), 77); // Reverse FLT writes only one word.
    run(&mut sim, &program);
    assert_eq!(sim.memory.double_word(d("D22")) as u32, 0xc1b8_0000);
    assert_eq!(sim.memory.word(d("D31")), -23);
    assert_eq!(sim.memory.word(d("D32")), 77);
    assert_eq!(sim.memory.double_word(d("D52")) as u32, 0x47c3_5000);
    assert_eq!(sim.memory.double_word(d("D61")), 100_000);
}

#[test]
fn every_suffix_bank_is_a_signed_word_for_float_and_integer_data() {
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
        for (name, source, result) in [("DFLT", 10, 0x4120_0000), ("DINT", 0x4120_0000, 10)] {
            let program =
                compile_il(&format!("LD X0\n{name} D10{suffix} D100{suffix}\nEND")).unwrap();
            let mut sim = enabled();
            sim.memory.set_word(d(alias), -2);
            let high = Device {
                area: Area::D,
                index: d(alias).index + 1,
            };
            sim.memory.set_word(high, i16::MIN);
            sim.memory.set_double_word(d("D8"), source);
            run(&mut sim, &program);
            assert_eq!(sim.memory.double_word(d("D98")), result, "{name} {suffix}");
            assert!(sim.observed_devices.contains(&d(alias)));
            assert!(!sim.observed_devices.contains(&high));
        }
    }
}

#[test]
fn pulse_disabled_and_held_visits_do_not_sample_invalid_addresses_or_clear_flags() {
    for name in ["FLTP", "DFLTP", "INTP", "DINTP"] {
        let program = compile_il(&format!("LD X0\n{name} D10E0 D100F0\nEND")).unwrap();
        let mut sim = Simulator::default();
        sim.memory.set_word(d("D1028"), -30000);
        seed_flags(&mut sim.memory, [true, true, true]);
        run(&mut sim, &program);
        assert!(sim.memory.execution_limitations.is_empty());
        assert_eq!(flags(&sim.memory), [true, true, true]);
        sim.memory.set_word(d("D1028"), 0);
        sim.memory.set_double_word(
            d("D10"),
            if name.contains("FLT") {
                10
            } else {
                0x4120_0000
            },
        );
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &program);
        let old = sim.memory.double_word(d("D100"));
        sim.memory.set_word(d("D1028"), -30000);
        seed_flags(&mut sim.memory, [true, true, true]);
        run(&mut sim, &program);
        assert!(sim.memory.execution_limitations.is_empty());
        assert_eq!(sim.memory.double_word(d("D100")), old);
        assert_eq!(flags(&sim.memory), [true, true, true]);
        sim.memory.set_input(d("X0"), false);
        run(&mut sim, &program);
        sim.memory.set_word(d("D1028"), 1);
        sim.memory.set_double_word(
            d("D11"),
            if name.contains("FLT") {
                23
            } else {
                0x41b8_0000
            },
        );
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &program);
        assert_eq!(
            sim.memory.double_word(d("D100")),
            if name.contains("FLT") {
                0x41b8_0000
            } else {
                23
            }
        );
    }
}

#[test]
fn exceptional_sources_clear_flags_and_report_e1a_before_bad_target_index() {
    for name in ["FLT", "DFLT", "INT", "DINT"] {
        let samples: &[u32] = if name.contains("FLT") {
            &[0x7f80_0000, 0xff80_0000, 0x7fc0_1234, 0x7f80_0001]
        } else {
            &[0x7fc0_1234, 0x7f80_0001]
        };
        for bits in samples {
            let program = compile_il(&format!("LD X0\n{name} D10E0 D100F7\nEND")).unwrap();
            let mut sim = enabled();
            sim.memory.set_system(1081, true);
            sim.memory.set_word(d("D1028"), 1);
            sim.memory.set_word(d("D1195"), -30000);
            sim.memory.set_double_word(d("D11"), *bits as i32);
            sim.memory.set_word(d("D100"), 77);
            seed_flags(&mut sim.memory, [true, true, true]);
            run(&mut sim, &program);
            assert!(
                sim.memory.execution_limitations.is_empty(),
                "{name} {bits:x}"
            );
            assert_eq!(sim.memory.word(d("D1067")), 0xe1a);
            assert_eq!(
                sim.memory.word(d("D1068")),
                (program.native_action_pc[0] + 1) as i16
            );
            assert_eq!(flags(&sim.memory), [false, false, false]);
            assert_eq!(sim.memory.word(d("D100")), 77);
            assert!(!sim.observed_devices.contains(&d("D1195")));
        }
    }
}

#[test]
fn integer_infinities_saturate_and_exact_two_to_31_retains_native_roundtrip_flags() {
    for (name, bits, expected, wanted) in [
        ("INT", 0x7f80_0000u32, 32767, [false, false, true]),
        ("INT", 0xff80_0000, -32768, [false, false, true]),
        ("DINT", 0x7f80_0000, i32::MAX, [false, false, true]),
        ("DINT", 0xff80_0000, i32::MIN, [false, false, true]),
        ("DINT", 0x4f00_0000, i32::MIN, [false, true, false]),
        ("DINT", 0xcf00_0000, i32::MIN, [false, false, false]),
        ("DINT", 0x4eff_ffff, 2_147_483_520, [false, false, false]),
    ] {
        let program = compile_il(&format!("LD X0\n{name} D0E0 D20F0\nEND")).unwrap();
        let mut sim = enabled();
        sim.memory.set_double_word(d("D0"), bits as i32);
        run(&mut sim, &program);
        assert_eq!(
            if name == "INT" {
                i32::from(sim.memory.word(d("D20")))
            } else {
                sim.memory.double_word(d("D20"))
            },
            expected
        );
        assert_eq!(flags(&sim.memory), wanted, "{name} {bits:x}");
        assert!(!sim.memory.system_bit(1067));
    }
}

#[test]
fn zero_subnormal_and_fraction_vectors_do_not_use_an_approximate_conversion() {
    for (bits, expected, wanted) in [
        (0x0000_0000u32, 0, [true, false, false]),
        (0x8000_0000, 0, [true, false, false]),
        (0x0000_0001, 0, [true, false, false]),
        (0x8000_0001, 0, [true, false, false]),
        (0x3f40_0000, 0, [true, false, false]),
        (0xbf40_0000, 0, [true, false, false]),
        (0x414c_0000, 12, [false, true, false]),
        (0xc14c_0000, -12, [false, true, false]),
    ] {
        for name in ["FLT", "DFLT", "INT", "DINT"] {
            let program = compile_il(&format!("LD X0\n{name} D0E0 D20F0\nEND")).unwrap();
            let mut sim = enabled();
            sim.memory.set_system(1081, true);
            sim.memory.set_double_word(d("D0"), bits as i32);
            run(&mut sim, &program);
            assert_eq!(
                if name.starts_with('D') {
                    sim.memory.double_word(d("D20"))
                } else {
                    i32::from(sim.memory.word(d("D20")))
                },
                expected
            );
            assert_eq!(flags(&sim.memory), wanted, "{name} {bits:x}");
        }
    }
}

#[test]
fn forward_conversion_only_ors_zero_and_uses_binary32_rounding() {
    for (value, expected) in [
        (0, 0x0000_0000u32),
        (16_777_217, 0x4b80_0000),
        (16_777_219, 0x4b80_0002),
        (-23, 0xc1b8_0000),
    ] {
        let program = compile_il("LD X0\nDFLT D0E0 D20F0\nEND").unwrap();
        let mut sim = enabled();
        sim.memory.set_double_word(d("D0"), value);
        seed_flags(&mut sim.memory, [false, true, true]);
        run(&mut sim, &program);
        assert_eq!(sim.memory.double_word(d("D20")) as u32, expected);
        assert_eq!(flags(&sim.memory), [value == 0, true, true]);
    }
}

#[test]
fn source_overlap_and_result_index_alias_keep_snapshot_and_fixed_pair() {
    let same = compile_il("LD X0\nFLT D10E0 D10E0\nSET M1081\nDFLT D10E0 D10E0\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_word(d("D1028"), 1);
    sim.memory.set_word(d("D11"), 23);
    run(&mut sim, &same);
    assert_eq!(sim.memory.double_word(d("D11")), 23);

    let alias = compile_il("LD X0\nDINT D20E1 D1027F0\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_word(d("D1029"), 1);
    sim.memory.set_double_word(d("D20"), 0x4780_0100); // 65538.0 -> low2/high1.
    sim.memory.set_word(d("D1030"), 77);
    sim.memory.set_word(d("D1031"), 88);
    run(&mut sim, &alias);
    assert_eq!(sim.memory.word(d("D1028")), 2);
    assert_eq!(sim.memory.word(d("D1029")), 1);
    assert_eq!(sim.memory.word(d("D1030")), 77);
    assert_eq!(sim.memory.word(d("D1031")), 88);

    let partial = compile_il("LD X0\nINT D20E1 D21E1\nEND").unwrap();
    sim.memory.set_double_word(d("D20"), 0x414c_0000);
    run(&mut sim, &partial);
    assert_eq!(sim.memory.word(d("D21")), 12);
}

#[test]
fn int_word_source_natural_counter_uses_two_descriptor_words_but_reads_full_value() {
    let action = parse("INT C200E1 D20F0");
    assert_eq!(
        action.native_words().unwrap(),
        [0x2201, 0x00e6, 0x2284, 0x28f6, 0x0086]
    );
    for source in ["C200", "C200E1", "C199E1"] {
        let program = compile_il(&format!("LD X0\nINT {source} D20F0\nEND")).unwrap();
        let mut sim = enabled();
        if source == "C199E1" {
            sim.memory.set_word(d("D1182"), 1);
        }
        sim.memory.set_double_word(d("C200"), 0x414c_0000);
        sim.memory.set_double_word(d("C201"), 0x7fc0_0000);
        run(&mut sim, &program);
        assert_eq!(sim.memory.word(d("D20")), 12);
        assert!(sim.observed_devices.contains(&d("C200")));
        assert!(!sim.observed_devices.contains(&d("C201")));
    }
    let program = compile_il("LD X0\nDINT C200E0 C255F0\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_word(d("D1028"), -1);
    sim.memory.set_word(d("C199"), -1); // Zero-high binary32 subnormal, not signed32 -1.
    sim.memory.set_double_word(d("C200"), 0x7fc0_0000);
    run(&mut sim, &program);
    assert_eq!(sim.memory.double_word(d("C255")), 0);
    assert_eq!(flags(&sim.memory), [true, false, false]);
    assert_eq!(sim.memory.double_word(d("C200")), 0x7fc0_0000);
}

#[test]
fn plain_and_modified_flt_endpoints_validate_actual_mode_width_for_both_models() {
    for last in [4999, 9999] {
        for suffix in ["", "E0"] {
            let forward = compile_il(&format!("LD X0\nFLT D{last}{suffix} D20\nEND")).unwrap();
            if last == 4999 {
                assert!(!forward.devices.contains(&d("D5000")));
            }
            let mut sim = enabled();
            sim.memory.data_register_limit = Some(last);
            sim.memory.set_word(d(&format!("D{last}")), 10);
            run(&mut sim, &forward);
            assert!(sim.memory.execution_limitations.is_empty());
            assert_eq!(sim.memory.double_word(d("D20")), 0x4120_0000);
            sim.memory.set_system(1081, true);
            run(&mut sim, &forward);
            assert!(!sim.memory.execution_limitations.is_empty());
            assert!(!sim.memory.system_bit(1067));

            let reverse = compile_il(&format!("LD X0\nFLT D20 D{last}{suffix}\nEND")).unwrap();
            if last == 4999 {
                assert!(!reverse.devices.contains(&d("D5000")));
            }
            let mut sim = enabled();
            sim.memory.data_register_limit = Some(last);
            sim.memory.set_system(1081, true);
            sim.memory.set_double_word(d("D20"), 0x414c_0000);
            run(&mut sim, &reverse);
            assert!(sim.memory.execution_limitations.is_empty());
            assert_eq!(sim.memory.word(d(&format!("D{last}"))), 12);
            sim.memory.set_system(1081, false);
            run(&mut sim, &reverse);
            assert!(!sim.memory.execution_limitations.is_empty());
        }
        let rescue = compile_il(&format!("LD X0\nDFLT D{last}E0 D{last}E0\nEND")).unwrap();
        let mut sim = enabled();
        sim.memory.data_register_limit = Some(last);
        sim.memory.set_word(d("D1028"), -1);
        sim.memory.set_double_word(d(&format!("D{}", last - 1)), 10);
        run(&mut sim, &rescue);
        assert!(sim.memory.execution_limitations.is_empty());
        assert_eq!(
            sim.memory.double_word(d(&format!("D{}", last - 1))),
            0x4120_0000
        );
    }
}

#[test]
fn structural_limits_follow_clear_and_success_flag_order_without_inventing_plc_error() {
    let mut memory = Memory::default();
    memory.set_word(d("D1028"), -30000);
    seed_flags(&mut memory, [true, true, true]);
    parse("INT D0E0 D20F0").execute(&mut memory, true, &mut BTreeSet::new());
    assert_eq!(flags(&memory), [false, false, false]);
    assert!(!memory.execution_limitations.is_empty());
    assert!(!memory.system_bit(1067));

    let mut memory = Memory::default();
    memory.set_word(d("D1029"), -30000);
    memory.set_double_word(d("D0"), 0x414c_0000);
    seed_flags(&mut memory, [true, true, true]);
    parse("INT D0E0 D20F0").execute(&mut memory, true, &mut BTreeSet::new());
    assert_eq!(flags(&memory), [false, true, false]);
    assert!(!memory.execution_limitations.is_empty());
    assert!(!memory.system_bit(1067));
}

#[test]
fn runtime_observation_and_display_follow_mode_not_static_possible_pairs() {
    let action = parse("FLT D10E0 D100F0");
    let mut memory = Memory::default();
    memory.set_word(d("D1028"), 1);
    memory.set_word(d("D1029"), 2);
    memory.set_word(d("D11"), 10);
    let mut observed = BTreeSet::new();
    action.execute(&mut memory, true, &mut observed);
    assert!(observed.contains(&d("D11")) && !observed.contains(&d("D12")));
    assert!(observed.contains(&d("D102")) && observed.contains(&d("D103")));
    assert_eq!(action.display_value(&memory), "10");
    memory.set_system(1081, true);
    memory.set_double_word(d("D11"), 0x414c_0000);
    observed.clear();
    action.execute(&mut memory, true, &mut observed);
    assert!(observed.contains(&d("D11")) && observed.contains(&d("D12")));
    assert!(observed.contains(&d("D102")) && !observed.contains(&d("D103")));
    assert_eq!(action.display_value(&memory), "12");

    memory.set_double_word(d("D11"), 0x7fc0_0000);
    let mut executed = BTreeSet::new();
    assert_eq!(
        action.prepare(&memory, &mut executed).unwrap(),
        PreparedFloat::InvalidSource
    );
    assert!(!executed.contains(&d("D1029")));
    let mut ui = BTreeSet::new();
    action.observed_devices(&memory, &mut ui);
    assert!(
        ui.contains(&d("D102")),
        "held result is independently displayable"
    );
}

#[test]
fn pure_prepare_preserves_mode_flags_and_source_values_even_on_invalid_input() {
    let action = parse("FLTP D0E0 D20F7");
    let mut memory = Memory::default();
    memory.set_system(1081, true);
    memory.set_double_word(d("D0"), 0x7f80_0000);
    memory.set_word(d("D1195"), -30000);
    seed_flags(&mut memory, [true, true, true]);
    let mut observed = BTreeSet::new();
    assert_eq!(
        action.prepare(&memory, &mut observed).unwrap(),
        PreparedFloat::InvalidSource
    );
    assert_eq!(flags(&memory), [true, true, true]);
    assert_eq!(memory.double_word(d("D0")), 0x7f80_0000);
    assert!(memory.system_bit(1081));
    assert!(!memory.system_bit(1067));
    assert!(!observed.contains(&d("D1195")));
    action.execute(&mut memory, false, &mut observed);
    assert_eq!(flags(&memory), [true, true, true]);
}

#[test]
fn source_fault_and_sticky_exception_channels_keep_separate_contracts() {
    let program = compile_il("LD X0\nDINT D0E0 D20F0\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_double_word(d("D0"), 0x7fc0_0000);
    sim.memory.set_system(1068, true);
    sim.memory.set_word(d("D1068"), 321);
    run(&mut sim, &program);
    assert_eq!(sim.memory.word(d("D1067")), 0xe1a);
    assert_eq!(sim.memory.word(d("D1068")), 321);
    sim.memory.set_system(1068, false);
    run(&mut sim, &program);
    assert_eq!(
        sim.memory.word(d("D1068")),
        (program.native_action_pc[0] + 1) as i16
    );
}
