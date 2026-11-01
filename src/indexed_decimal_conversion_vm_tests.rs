//! Independent pair/IEEE, error-order and profile expectations for DEBCD/DEBIN.
use crate::float_math::{DecimalScratchProfile, FloatBinaryError, FloatMathAction};
use crate::indexed_decimal_conversion::{
    IndexedDecimalConversionAction, PreparedDecimalConversion,
};
use crate::plc::{Device, Memory, Program, ScanStatus, Simulator, compile_il};
use std::collections::BTreeSet;

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn parse(raw: &str) -> IndexedDecimalConversionAction {
    let mut parts = raw.split_whitespace();
    IndexedDecimalConversionAction::parse(
        parts.next().unwrap(),
        &parts.map(str::to_owned).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn put(m: &mut Memory, address: &str, bits: u32) {
    m.set_double_word(d(address), bits as i32);
}
fn get(m: &Memory, address: &str) -> u32 {
    m.double_word(d(address)) as u32
}
fn pair(m: &mut Memory, address: &str, coefficient: i16, exponent: i16) {
    put(
        m,
        address,
        u32::from(coefficient as u16) | (u32::from(exponent as u16) << 16),
    );
}
fn read_pair(m: &Memory, address: &str) -> (i16, i16) {
    let bits = get(m, address);
    (bits as i16, (bits >> 16) as i16)
}
fn flags(m: &Memory) -> [bool; 3] {
    [1020, 1021, 1022].map(|n| m.system_bit(n))
}
fn seed(m: &mut Memory) {
    for n in 1020..=1022 {
        m.set_system(n, true);
    }
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
fn four_forms_encode_nine_words_and_preserve_the_two_different_result_formats() {
    for (name, header, raw, expected) in [
        ("DEBCD", 0xfd00, 0x4048_f5c3, 0xfffd_0c44),
        ("DEBCDP", 0xfd10, 0x4048_f5c3, 0xfffd_0c44),
        ("DEBIN", 0xff00, 0xfffe_013a, 0x4048_f5c3),
        ("DEBINP", 0xff10, 0xfffe_013a, 0x4048_f5c3),
    ] {
        let body = format!("{name} D10E1 D100F7");
        let action = parse(&body);
        let words = [
            header, 0x14e6, 0x8086, 0x0080, 0x0080, 0xc8f6, 0x8089, 0x0080, 0x0080,
        ];
        assert_eq!(action.native_words().unwrap(), words);
        assert_eq!(action.label(), body);
        assert_eq!(action.dest(), d("D100"));
        let program = compile_il(&format!("LD X0\n{body}\nMOV K7 D300\nOUT Y0\nEND")).unwrap();
        let encoded = crate::es2_encoder::encode(&program).unwrap();
        assert!(program.requires_vm());
        assert_eq!(&encoded.words[1..10], &words);
        assert_eq!(encoded.action_pc[1], 10);
        let mut sim = enabled();
        sim.memory.set_word(d("D1182"), 1);
        sim.memory.set_word(d("D1195"), 2);
        sim.memory.set_word(d("D101"), 77);
        sim.memory.set_word(d("D104"), 88);
        put(&mut sim.memory, "D11", raw);
        seed(&mut sim.memory);
        run(&mut sim, &program);
        assert_eq!(get(&sim.memory, "D102"), expected);
        assert_eq!(flags(&sim.memory), [false; 3]);
        assert_eq!(
            [sim.memory.word(d("D101")), sim.memory.word(d("D104"))],
            [77, 88]
        );
        assert_eq!(sim.memory.word(d("D300")), 7);
        assert!(sim.memory.bit(d("Y0")));
    }
}

#[test]
fn all_sixteen_signed_word_index_banks_apply_to_both_conversion_roles() {
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
        for (name, raw, expected) in [
            ("DEBCD", 0xc1b8_0000, 0xfffe_f704),
            ("DEBIN", 0xfffe_f704, 0xc1b8_0000),
        ] {
            let action = parse(&format!("{name} D10{suffix} D100{suffix}"));
            let mut m = Memory::default();
            m.set_word(d(alias), -2);
            let next = Device {
                index: d(alias).index + 1,
                ..d(alias)
            };
            m.set_word(next, i16::MIN);
            put(&mut m, "D8", raw);
            let mut observed = BTreeSet::new();
            action.execute(&mut m, true, &mut observed);
            assert_eq!(get(&m, "D98"), expected, "{name} {suffix}");
            assert_eq!(action.modifier_devices(), vec![d(alias)]);
            assert!(observed.contains(&d(alias)) && !observed.contains(&next));
            assert!(m.execution_limitations.is_empty());
        }
    }
}

#[test]
fn matrix_accepts_only_direct_or_modified_d_pairs() {
    for name in ["DEBCD", "DEBCDP", "DEBIN", "DEBINP"] {
        for (source, target) in [("D0E0", "D20"), ("D0", "D20F7"), ("D9999E1", "D9999F1")] {
            assert!(
                IndexedDecimalConversionAction::parse(name, &[source.into(), target.into()])
                    .is_ok()
            );
        }
        for bad in [
            "K314", "HFFFF", "F3.14", "E1", "F1", "T0", "C200", "K8M0", "D9999", "D0E8",
        ] {
            assert!(
                IndexedDecimalConversionAction::parse(name, &[bad.into(), "D20E0".into()]).is_err(),
                "{name} source {bad}"
            );
            assert!(
                IndexedDecimalConversionAction::parse(name, &["D0E0".into(), bad.into()]).is_err(),
                "{name} target {bad}"
            );
        }
        assert!(IndexedDecimalConversionAction::parse(name, &["D0E0".into()]).is_err());
    }
    for name in ["BCD", "DBCD", "DEBCDPP", "DEBINPP", "DRAD", "DEADD"] {
        assert!(!IndexedDecimalConversionAction::supports(name));
    }
}

#[test]
fn debin_manual_example_and_valid_boundaries_have_literal_ieee_results() {
    for (coefficient, exponent, expected) in [
        (314, -2, 0x4048_f5c3),
        (-314, -2, 0xc048_f5c3),
        (3141, -4, 0x3ea0_d1b7),
        (1176, -41, 0x0080_0e18),
        (-1176, -41, 0x8080_0e18),
        (3402, 35, 0x7f7f_f023),
        (-3402, 35, 0xff7f_f023),
        (9999, 0, 0x461c_3c00),
        (1, -40, 0x0001_16c2),
        (0, -40, 0),
        (0, 35, 0),
    ] {
        let mut m = Memory::default();
        seed(&mut m);
        pair(&mut m, "D0", coefficient, exponent);
        let action = parse("DEBIN D0E0 D20F0");
        assert_eq!(action.semantic_error(&m).unwrap(), None);
        action.execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(get(&m, "D20"), expected, "{coefficient}e{exponent}");
        assert_eq!(flags(&m), [coefficient == 0, false, false]);
        assert!(!m.system_bit(1067));
    }
}

#[test]
fn debin_invalid_ranges_precede_zero_and_never_visit_a_bad_target_index() {
    let action = parse("DEBIN D0E0 D20F7");
    for (coefficient, exponent) in [
        (0, -41),
        (1175, -41),
        (-1175, -41),
        (3403, 35),
        (-3403, 35),
        (10000, 0),
        (-10000, 0),
        (1, -42),
        (1, 36),
        (0, 36),
        (i16::MIN, 0),
    ] {
        let mut m = Memory::default();
        seed(&mut m);
        pair(&mut m, "D0", coefficient, exponent);
        m.set_word(d("D1195"), -30000);
        put(&mut m, "D20", 0x1234_5678);
        assert_eq!(
            action.semantic_error(&m).unwrap(),
            Some(FloatBinaryError {
                code: 0xe1a,
                offset: 1
            })
        );
        let mut observed = BTreeSet::new();
        assert_eq!(
            action.prepare(&m, &mut observed).unwrap(),
            PreparedDecimalConversion::InvalidSource
        );
        assert_eq!(flags(&m), [true; 3]);
        assert!(!observed.contains(&d("D1195")));
        action.execute(&mut m, true, &mut observed);
        assert_eq!(flags(&m), [false; 3]);
        assert_eq!([m.word(d("D1067")), m.word(d("D1068"))], [0xe1a, 1]);
        assert_eq!(get(&m, "D20"), 0x1234_5678);
        assert!(m.execution_limitations.is_empty());
        assert!(!observed.contains(&d("D1195")) && !observed.contains(&d("D20")));
    }
}

#[test]
fn invalid_conversion_pc_uses_its_actual_native_header_and_holds_sticky_errors() {
    for name in ["DEBIN", "DEBINP"] {
        let p = compile_il(&format!(
            "LD X0\nMOV K7 D50\n{name} D0E0 D20F7\nOUT Y0\nEND"
        ))
        .unwrap();
        let encoded = crate::es2_encoder::encode(&p).unwrap();
        let mut sim = enabled();
        pair(&mut sim.memory, "D0", 0, -41);
        sim.memory.set_word(d("D1195"), -30000);
        seed(&mut sim.memory);
        run(&mut sim, &p);
        assert_eq!(sim.memory.word(d("D1068")) as u16, encoded.action_pc[1] + 1);
        assert_eq!(sim.memory.word(d("D1067")), 0xe1a);
        assert_eq!(flags(&sim.memory), [false; 3]);
        assert!(sim.memory.bit(d("Y0")));
        let mut m = Memory::default();
        pair(&mut m, "D0", 0, -41);
        seed(&mut m);
        m.set_system(1067, true);
        m.set_system(1068, true);
        m.set_word(d("D1067"), 0xe19);
        m.set_word(d("D1068"), 321);
        parse("DEBIN D0E0 D20F0").execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!([m.word(d("D1067")), m.word(d("D1068"))], [0xe19, 321]);
        assert_eq!(flags(&m), [false; 3]);
    }
}

#[test]
fn debcd_profiles_preserve_preclamp_diagnostics_and_zero_unordered_bypass() {
    for (raw, output, state, diagnostics) in [
        (0x4048_f5c3, (3140, -3), [false; 3], Some((-3, 3140))),
        (0xc1b8_0000, (-2300, -2), [false; 3], Some((-2, -2300))),
        (
            0x0080_0000,
            (1176, -41),
            [false, true, false],
            Some((-41, 1175)),
        ),
        (1, (1176, -41), [false, true, false], Some((-48, 1401))),
        (0x7f7f_ffff, (3402, 35), [false; 3], Some((35, 3402))),
        (0x7f80_0000, (0, -4), [false; 3], Some((-4, 0))),
        (0, (0, 0), [true, false, false], None),
        (0x8000_0000, (0, 0), [true, false, false], None),
        (0x7fc0_1234, (0, 0), [true, false, false], None),
        (0xff80_0001, (0, 0), [true, false, false], None),
    ] {
        for profile in [
            DecimalScratchProfile::CurrentPrivate,
            DecimalScratchProfile::LegacyVisible,
        ] {
            let mut m = Memory::default();
            m.decimal_scratch_profile = profile;
            m.set_word(d("D5000"), 123);
            m.set_word(d("D5001"), 456);
            put(&mut m, "D0", raw);
            seed(&mut m);
            let mut observed = BTreeSet::new();
            parse("DEBCD D0E0 D20F0").execute(&mut m, true, &mut observed);
            assert_eq!(read_pair(&m, "D20"), output);
            assert_eq!(flags(&m), state);
            let visible = profile == DecimalScratchProfile::LegacyVisible && diagnostics.is_some();
            let expected = if visible {
                diagnostics.unwrap()
            } else {
                (123, 456)
            };
            assert_eq!(read_pair(&m, "D5000"), expected);
            assert_eq!(observed.contains(&d("D5000")), visible);
            assert_eq!(observed.contains(&d("D5001")), visible);
            assert!(!m.system_bit(1067));
        }
    }
}

#[test]
fn final_result_writes_override_legacy_diagnostics_only_at_the_explicit_pair() {
    for (profile, target, expected) in [
        (
            DecimalScratchProfile::CurrentPrivate,
            4999,
            [-2300, -2, 456, 789],
        ),
        (
            DecimalScratchProfile::CurrentPrivate,
            5000,
            [111, -2300, -2, 789],
        ),
        (
            DecimalScratchProfile::CurrentPrivate,
            5001,
            [111, 123, -2300, -2],
        ),
        (
            DecimalScratchProfile::LegacyVisible,
            4999,
            [-2300, -2, -2300, 789],
        ),
        (
            DecimalScratchProfile::LegacyVisible,
            5000,
            [111, -2300, -2, 789],
        ),
        (
            DecimalScratchProfile::LegacyVisible,
            5001,
            [111, -2, -2300, -2],
        ),
    ] {
        let mut m = Memory::default();
        m.decimal_scratch_profile = profile;
        for (address, value) in [
            ("D4999", 111),
            ("D5000", 123),
            ("D5001", 456),
            ("D5002", 789),
        ] {
            m.set_word(d(address), value);
        }
        put(&mut m, "D0", 0xc1b8_0000);
        parse(&format!("DEBCD D0E0 D{target}F0")).execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(
            ["D4999", "D5000", "D5001", "D5002"].map(|address| m.word(d(address))),
            expected
        );
    }
}

#[test]
fn source_snapshot_survives_legacy_scratch_alias_and_shifted_destination_overlap() {
    for profile in [
        DecimalScratchProfile::CurrentPrivate,
        DecimalScratchProfile::LegacyVisible,
    ] {
        for (source, target) in [(4999, 20), (5000, 20), (5001, 20), (5000, 5001)] {
            let mut m = Memory::default();
            m.decimal_scratch_profile = profile;
            put(&mut m, &format!("D{source}"), 0xc1b8_0000);
            parse(&format!("DEBCD D{source}E0 D{target}F0")).execute(
                &mut m,
                true,
                &mut BTreeSet::new(),
            );
            assert_eq!(read_pair(&m, &format!("D{target}")), (-2300, -2));
        }
    }
    let mut m = Memory::default();
    pair(&mut m, "D0", 314, -2);
    m.set_word(d("D3"), 77);
    parse("DEBIN D0E0 D1F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(get(&m, "D1"), 0x4048_f5c3);
    assert_eq!([m.word(d("D0")), m.word(d("D3"))], [314, 77]);
}

#[test]
fn self_index_results_cannot_redirect_the_high_half_after_the_low_write() {
    for (name, raw, low, high) in [
        ("DEBCD", 0x4048_f5c3, 3140u16, 0xfffd),
        ("DEBIN", 0xfffe_013a, 0xf5c3, 0x4048),
    ] {
        let mut m = Memory::default();
        put(&mut m, "D0", raw);
        m.set_word(d("D1028"), 1);
        m.set_word(d("D1030"), 77);
        parse(&format!("{name} D0F0 D1027E0")).execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(m.word(d("D1028")) as u16, low);
        assert_eq!(m.word(d("D1029")) as u16, high);
        assert_eq!(m.word(d("D1030")), 77);
    }
}

#[test]
fn effective_pair_bounds_and_hidden_ss2_diagnostics_are_separate() {
    for last in [4999, 9999] {
        let action = parse(&format!("DEBIN D{last}E0 D{last}F0"));
        let mut m = Memory::default();
        m.data_register_limit = Some(last);
        m.set_word(d("D1028"), -1);
        m.set_word(d("D1029"), -1);
        pair(&mut m, &format!("D{}", last - 1), 314, -2);
        action.execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(get(&m, &format!("D{}", last - 1)), 0x4048_f5c3);
        m.set_word(d("D1028"), 0);
        assert!(action.resolved_source(&m).is_err());
        m.set_word(d("D1028"), -1);
        m.set_word(d("D1029"), 0);
        pair(&mut m, &format!("D{}", last - 1), 314, -2);
        assert!(action.prepare(&m, &mut BTreeSet::new()).is_err());
    }
    let mut m = Memory::default();
    m.data_register_limit = Some(4999);
    m.decimal_scratch_profile = DecimalScratchProfile::LegacyVisible;
    put(&mut m, "D0", 0x4048_f5c3);
    let action = parse("DEBCD D0E0 D20F0");
    let mut observed = BTreeSet::new();
    action.execute(&mut m, true, &mut observed);
    assert_eq!(read_pair(&m, "D5000"), (-3, 3140));
    assert!(!observed.contains(&d("D5000")));
    action.observed_devices(&m, &mut observed);
    assert!(!observed.contains(&d("D5000")));
    assert!(parse("DEBCD D4999 D20E0").resolved_source(&m).is_err());
    assert!(
        parse("DEBCD D0E0 D4999")
            .prepare(&m, &mut BTreeSet::new())
            .is_err()
    );
}

#[test]
fn off_and_held_pulse_skip_flag_clear_reads_diagnostics_and_errors() {
    for (name, first, second, expected) in [
        ("DEBCDP", 0x4048_f5c3, 0xc1b8_0000, 0xfffe_f704),
        ("DEBINP", 0xfffe_013a, 0xfffe_fec6, 0xc048_f5c3),
    ] {
        let p = compile_il(&format!("LD X0\n{name} D0E0 D20F0\nEND")).unwrap();
        let mut sim = Simulator::default();
        seed(&mut sim.memory);
        sim.memory.set_word(d("D1028"), -30000);
        run(&mut sim, &p);
        assert_eq!(flags(&sim.memory), [true; 3]);
        assert!(sim.memory.execution_limitations.is_empty());
        assert!(!sim.observed_devices.contains(&d("D1028")));
        sim.memory.set_word(d("D1028"), 0);
        put(&mut sim.memory, "D0", first);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &p);
        let held = get(&sim.memory, "D20");
        seed(&mut sim.memory);
        sim.observed_devices.clear();
        sim.memory.set_word(d("D1028"), -30000);
        run(&mut sim, &p);
        assert_eq!(flags(&sim.memory), [true; 3]);
        assert_eq!(get(&sim.memory, "D20"), held);
        assert!(sim.memory.execution_limitations.is_empty());
        assert!(!sim.observed_devices.contains(&d("D1028")));
        sim.memory.set_input(d("X0"), false);
        run(&mut sim, &p);
        sim.memory.set_word(d("D1028"), 0);
        put(&mut sim.memory, "D0", second);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &p);
        assert_eq!(get(&sim.memory, "D20"), expected);
    }
}

#[test]
fn loop_visits_reread_offsets_and_conversion_sources_in_same_scan() {
    let p = compile_il("LD X0\nMOV K0 E0\nFOR K3\nDEBIN D10E0 D100E0\nADD E0 K2 E0\nNEXT\nEND")
        .unwrap();
    let mut sim = enabled();
    for (address, c, e) in [("D10", 0, 0), ("D12", 314, -2), ("D14", -314, -2)] {
        pair(&mut sim.memory, address, c, e);
    }
    run(&mut sim, &p);
    assert_eq!(
        ["D100", "D102", "D104"].map(|address| get(&sim.memory, address)),
        [0, 0x4048_f5c3, 0xc048_f5c3]
    );
    assert_eq!(sim.memory.word(d("D1028")), 6);
}

#[test]
fn runtime_observation_is_staged_while_monitoring_keeps_held_result_visible() {
    let action = parse("DEBIN D0E0 D100F0");
    let mut m = Memory::default();
    m.set_word(d("D1028"), 1);
    m.set_word(d("D1029"), 2);
    pair(&mut m, "D1", 314, -2);
    let mut devices = BTreeSet::new();
    action.devices(&mut devices);
    assert!(!devices.contains(&d("D0")) && !devices.contains(&d("D100")));
    assert!(!devices.contains(&d("D5000")));
    let mut observed = BTreeSet::new();
    action.execute(&mut m, true, &mut observed);
    for address in ["D1", "D2", "D102", "D103"] {
        assert!(observed.contains(&d(address)));
    }
    assert_eq!(action.display_value(&m), "3.14");
    pair(&mut m, "D1", 0, -41);
    observed.clear();
    action.execute(&mut m, true, &mut observed);
    assert!(!observed.contains(&d("D1029")) && !observed.contains(&d("D102")));
    let mut ui = BTreeSet::new();
    action.observed_devices(&m, &mut ui);
    assert!(ui.contains(&d("D102")) && ui.contains(&d("D103")));
    assert_eq!(action.display_value(&m), "3.14");
    pair(&mut m, "D102", 3140, -3);
    assert_eq!(parse("DEBCD D0E0 D100F0").display_value(&m), "3140 × 10^-3");
    m.set_word(d("D1029"), -30000);
    assert!(action.display_value(&m).starts_with("İndeks:"));
}

#[test]
fn structural_source_failure_clears_flags_but_target_failure_retains_prior_conversion_effects() {
    let mut m = Memory::default();
    seed(&mut m);
    m.set_word(d("D1028"), -30000);
    let mut observed = BTreeSet::new();
    parse("DEBCD D0E0 D20F7").execute(&mut m, true, &mut observed);
    assert_eq!(flags(&m), [false; 3]);
    assert!(!m.system_bit(1067));
    assert!(!observed.contains(&d("D1195")));
    let mut m = Memory::default();
    seed(&mut m);
    m.decimal_scratch_profile = DecimalScratchProfile::LegacyVisible;
    put(&mut m, "D0", 1);
    put(&mut m, "D20", 0x1234_5678);
    m.set_word(d("D1195"), -30000);
    let action = parse("DEBCD D0E0 D20F7");
    observed.clear();
    assert!(action.prepare(&m, &mut observed).is_err());
    assert_eq!(flags(&m), [true; 3]);
    assert_eq!(read_pair(&m, "D5000"), (0, 0));
    action.execute(&mut m, true, &mut observed);
    assert_eq!(flags(&m), [false, true, false]);
    assert_eq!(read_pair(&m, "D5000"), (-48, 1401));
    assert_eq!(get(&m, "D20"), 0x1234_5678);
    assert!(!m.system_bit(1067));
    assert!(!m.execution_limitations.is_empty());
}

#[test]
fn ordinary_and_indexed_conversion_share_math_profiles_and_ignore_angle_mode() {
    for (name, raw, expected) in [
        ("DEBCD", 0xc1b8_0000, 0xfffe_f704),
        ("DEBIN", 0xfffe_013a, 0x4048_f5c3),
    ] {
        for profile in [
            DecimalScratchProfile::CurrentPrivate,
            DecimalScratchProfile::LegacyVisible,
        ] {
            let mut ordinary = Memory::default();
            ordinary.decimal_scratch_profile = profile;
            ordinary.set_system(1081, true);
            put(&mut ordinary, "D0", raw);
            seed(&mut ordinary);
            let mut indexed = ordinary.clone();
            let action = FloatMathAction::parse(name, &["D0".into(), "D20".into()]).unwrap();
            assert!(!action.execute(&mut ordinary));
            let mut observed = BTreeSet::new();
            parse(&format!("{name} D0E0 D20F0")).execute(&mut indexed, true, &mut observed);
            assert_eq!(get(&ordinary, "D20"), expected);
            assert_eq!(get(&indexed, "D20"), expected);
            assert_eq!(flags(&ordinary), flags(&indexed));
            assert_eq!(read_pair(&ordinary, "D5000"), read_pair(&indexed, "D5000"));
            assert!(ordinary.system_bit(1081) && indexed.system_bit(1081));
            assert!(!observed.contains(&d("M1081")));
        }
    }
}
