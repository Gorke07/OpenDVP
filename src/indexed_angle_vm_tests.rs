//! Independent descriptor and binary32 expectations for indexed DRAD/DDEG.
//! Native exceptional delivery/finite-overflow responsiveness is qualified
//! separately; these tests specify the local arithmetic and address contract.
use crate::float_math::{DecimalScratchProfile, FloatMathAction};
use crate::indexed_angle::IndexedAngleAction;
use crate::plc::{Device, Memory, Program, ScanStatus, Simulator, compile_il};
use std::collections::BTreeSet;

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn parse(raw: &str) -> IndexedAngleAction {
    let mut parts = raw.split_whitespace();
    IndexedAngleAction::parse(
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
fn flags(m: &Memory) -> [bool; 3] {
    [1020, 1021, 1022].map(|n| m.system_bit(n))
}
fn seed(m: &mut Memory) {
    for (n, v) in [(1020, true), (1021, true), (1022, false)] {
        m.set_system(n, v);
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
fn four_forms_have_literal_nine_word_layout_and_fixed_result_pairs() {
    for (name, header, input, expected) in [
        ("DRAD", 0xf900, 0x4334_0000, 0x4049_0fdb),
        ("DRADP", 0xf910, 0x4334_0000, 0x4049_0fdb),
        ("DDEG", 0xfb00, 0x4049_0fdb, 0x4334_0000),
        ("DDEGP", 0xfb10, 0x4049_0fdb, 0x4334_0000),
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
        assert!(program.requires_vm());
        let encoded = crate::es2_encoder::encode(&program).unwrap();
        assert_eq!(&encoded.words[1..10], &words);
        assert_eq!(encoded.action_pc[1], 10);
        let mut sim = enabled();
        sim.memory.set_word(d("D1182"), 1);
        sim.memory.set_word(d("D1195"), 2);
        put(&mut sim.memory, "D11", input);
        sim.memory.set_word(d("D101"), 77);
        sim.memory.set_word(d("D104"), 88);
        seed(&mut sim.memory);
        run(&mut sim, &program);
        assert_eq!(get(&sim.memory, "D102"), expected);
        assert_eq!(flags(&sim.memory), [false; 3]);
        assert_eq!(sim.memory.word(d("D101")), 77);
        assert_eq!(sim.memory.word(d("D104")), 88);
        assert_eq!(sim.memory.word(d("D300")), 7);
        assert!(sim.memory.bit(d("Y0")));
    }
}

#[test]
fn all_sixteen_index_aliases_are_signed_words_for_source_and_target() {
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
        let action = parse(&format!("DRAD D10{suffix} D100{suffix}"));
        let mut m = Memory::default();
        m.set_word(d(alias), -2);
        let next = Device {
            index: d(alias).index + 1,
            ..d(alias)
        };
        m.set_word(next, i16::MIN);
        put(&mut m, "D8", 0x4334_0000);
        let mut observed = BTreeSet::new();
        action.execute(&mut m, true, &mut observed);
        assert_eq!(get(&m, "D98"), 0x4049_0fdb, "{suffix}");
        assert_eq!(action.modifier_devices(), vec![d(alias)]);
        assert!(observed.contains(&d(alias)));
        assert!(
            !observed.contains(&next),
            "modifier is not a pair: {suffix}"
        );
        assert!(m.execution_limitations.is_empty());
    }
}

#[test]
fn role_matrix_is_only_integer_or_data_source_and_data_target() {
    for name in ["DRAD", "DRADP", "DDEG", "DDEGP"] {
        for (source, target) in [
            ("K180", "D0E0"),
            ("H80000000", "D0F7"),
            ("D0E1", "D20"),
            ("D0", "D20F1"),
        ] {
            assert!(IndexedAngleAction::parse(name, &[source.into(), target.into()]).is_ok());
        }
        for (source, target) in [
            ("F180", "D0E0"),
            ("E1", "D0E0"),
            ("F1", "D0E0"),
            ("T0E1", "D20"),
            ("C200F1", "D20"),
            ("K8M0E0", "D20"),
            ("D0E8", "D20"),
            ("K1E0", "D20"),
            ("D0E0", "E1"),
            ("D0E0", "T0"),
            ("D0E0", "C200F0"),
            ("D0E0", "K8M0"),
            ("D9999", "D20E0"),
            ("D0E0", "D9999"),
            ("D0E0", "K1"),
        ] {
            assert!(
                IndexedAngleAction::parse(name, &[source.into(), target.into()]).is_err(),
                "{name} {source} {target}"
            );
        }
        assert!(IndexedAngleAction::parse(name, &["D0E0".into()]).is_err());
    }
    for other in ["RAD", "DEG", "DADDR", "DEADD", "DEBCD", "DRADPP"] {
        assert!(!IndexedAngleAction::supports(other));
    }
    assert!(!IndexedAngleAction::has_modifiers(&[
        "HDEAD".into(),
        "D20".into()
    ]));
    assert!(IndexedAngleAction::has_modifiers(&[
        "K180".into(),
        "D20F7".into()
    ]));
}

#[test]
fn constants_are_converted_as_signed_integers_before_the_binary32_multiply() {
    for (literal, radians, degrees) in [
        ("K1", 0x3c8e_fa35, 0x4265_2ee1),
        ("K180", 0x4049_0fdb, 0x4621_24f6),
        ("K-180", 0xc049_0fdb, 0xc621_24f6),
        ("K16777217", 0x488e_fa35, 0x4e65_2ee1),
        ("H80000000", 0xcc0e_fa35, 0xd1e5_2ee1),
        ("K2147483647", 0x4c0e_fa35, 0x51e5_2ee1),
    ] {
        for (name, expected) in [("DRAD", radians), ("DDEG", degrees)] {
            let mut m = Memory::default();
            parse(&format!("{name} {literal} D20E0")).execute(&mut m, true, &mut BTreeSet::new());
            assert_eq!(get(&m, "D20"), expected, "{name} {literal}");
            assert_eq!(flags(&m), [false; 3]);
        }
    }
    let mut m = Memory::default();
    put(&mut m, "D0", 0x8000_0000); // Same raw bits as H80000000, but -0.
    parse("DRAD D0E0 D20F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(get(&m, "D20"), 0);
    assert_eq!(flags(&m), [true, false, false]);
    assert_eq!(
        parse("DRAD K180 D20E0").native_words().unwrap(),
        [
            0xf900, 0xb480, 0x0080, 0x0080, 0x0080, 0x28e6, 0x0086, 0x0080, 0x0080
        ]
    );
    assert_eq!(
        parse("DDEGP H80000000 D20F7").native_words().unwrap(),
        [
            0xfb10, 0x0080, 0x0080, 0x0080, 0x8080, 0x28f6, 0x8089, 0x0080, 0x0080
        ]
    );
}

#[test]
fn finite_ieee_vectors_cover_rounding_underflow_subnormal_and_large_input() {
    for (name, raw, expected, zero) in [
        ("DRAD", 0x4334_0000, 0x4049_0fdb, false),
        ("DDEG", 0x4049_0fdb, 0x4334_0000, false),
        ("DRAD", 0x4049_0fdb, 0x3d60_969d, false),
        ("DDEG", 0xbf80_0000, 0xc265_2ee1, false),
        ("DRAD", 1, 0, true),
        ("DRAD", 0x8000_0001, 0, true),
        ("DDEG", 1, 0x39, false),
        ("DRAD", 0x0080_0000, 0x0002_3be9, false),
        ("DDEG", 0x0080_0000, 0x0365_2ee1, false),
        ("DRAD", 0x7f7f_ffff, 0x7c8e_fa34, false),
    ] {
        let mut m = Memory::default();
        seed(&mut m);
        put(&mut m, "D0", raw);
        parse(&format!("{name} D0E0 D20F0")).execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(get(&m, "D20"), expected, "{name} {raw:08x}");
        assert_eq!(flags(&m), [zero, false, false]);
        assert!(!m.system_bit(1067));
    }
}

#[test]
fn local_nonfinite_normalization_is_not_the_r_input_nan_error_policy() {
    for name in ["DRAD", "DDEG"] {
        for (raw, expected, state) in [
            (0, 0, [true, false, false]),
            (0x8000_0000, 0, [true, false, false]),
            (0x7f80_0000, 0x7f80_0000, [false, false, true]),
            (0xff80_0000, 0xff80_0000, [false, true, false]),
            (0x7fc0_1234, 0x7f80_0000, [false, false, true]),
            (0xffc0_1234, 0xff80_0000, [false, true, false]),
            (0x7f80_0001, 0x7f80_0000, [false, false, true]),
        ] {
            let mut m = Memory::default();
            seed(&mut m);
            put(&mut m, "D0", raw);
            let action = parse(&format!("{name} D0E0 D20F7"));
            let mut observed = BTreeSet::new();
            assert!(action.prepare(&m, &mut observed).is_ok());
            assert_eq!(flags(&m), [true, true, false], "pure prepare");
            action.execute(&mut m, true, &mut observed);
            assert_eq!(get(&m, "D20"), expected);
            assert_eq!(flags(&m), state);
            assert!(observed.contains(&d("D1195")) && observed.contains(&d("D21")));
            assert!(!m.system_bit(1067));
        }
    }
}

#[test]
fn address_failures_remain_limitations_with_source_before_flag_before_target_order() {
    let action = parse("DRAD D0E0 D20F7");
    let mut m = Memory::default();
    seed(&mut m);
    m.set_word(d("D1028"), -30000);
    let mut observed = BTreeSet::new();
    action.execute(&mut m, true, &mut observed);
    assert_eq!(flags(&m), [true, true, false]);
    assert!(observed.contains(&d("D1028")) && !observed.contains(&d("D1195")));
    assert!(!m.system_bit(1067));
    assert!(!m.execution_limitations.is_empty());
    let mut m = Memory::default();
    seed(&mut m);
    m.set_word(d("D1195"), -30000);
    put(&mut m, "D0", 0x7fc0_0001);
    put(&mut m, "D20", 0x1234_5678);
    observed.clear();
    assert!(action.prepare(&m, &mut observed).is_err());
    assert_eq!(flags(&m), [true, true, false]);
    action.execute(&mut m, true, &mut observed);
    assert_eq!(flags(&m), [false, false, true]);
    assert_eq!(get(&m, "D20"), 0x1234_5678);
    assert!(observed.contains(&d("D1195")));
    assert!(!m.system_bit(1067));
}

#[test]
fn overlap_and_self_index_writes_use_one_source_snapshot_and_one_result_pair() {
    let mut m = Memory::default();
    put(&mut m, "D0", 0x4334_0000);
    m.set_word(d("D3"), 77);
    parse("DRAD D0E0 D1F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(get(&m, "D1"), 0x4049_0fdb);
    assert_eq!(m.word(d("D0")), 0);
    assert_eq!(m.word(d("D3")), 77);
    let mut m = Memory::default();
    m.set_word(d("D1028"), 1);
    m.set_word(d("D1030"), 88);
    parse("DRAD K180 D1027E0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.word(d("D1028")) as u16, 0x0fdb);
    assert_eq!(m.word(d("D1029")) as u16, 0x4049);
    assert_eq!(m.word(d("D1030")), 88);
    // The source's own modifier aliases its high half; it is still read once.
    let mut m = Memory::default();
    put(&mut m, "D1027", 0x0001_0000);
    parse("DDEG D1026E0 D1027F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(get(&m, "D1027"), 0x0039_4bb8);
}

#[test]
fn same_scan_and_loop_visits_reread_live_index_words() {
    let program =
        compile_il("LD X0\nMOV K0 E0\nFOR K3\nDRAD D10E0 D100E0\nADD E0 K2 E0\nNEXT\nEND").unwrap();
    let mut sim = enabled();
    for (address, bits) in [("D10", 0), ("D12", 0x4334_0000), ("D14", 0xc334_0000)] {
        put(&mut sim.memory, address, bits);
    }
    run(&mut sim, &program);
    assert_eq!(
        ["D100", "D102", "D104"].map(|address| get(&sim.memory, address)),
        [0, 0x4049_0fdb, 0xc049_0fdb]
    );
    assert_eq!(sim.memory.word(d("D1028")), 6);
}

#[test]
fn model_pair_endpoints_are_checked_after_signed_wrapped_index_resolution() {
    for last in [4999, 9999] {
        let action = parse(&format!("DRAD D{last}E0 D{last}F0"));
        let mut m = Memory::default();
        m.data_register_limit = Some(last);
        m.set_word(d("D1028"), -1);
        m.set_word(d("D1029"), -1);
        put(&mut m, &format!("D{}", last - 1), 0x4334_0000);
        action.execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(get(&m, &format!("D{}", last - 1)), 0x4049_0fdb);
        m.set_word(d("D1028"), 0);
        assert!(action.resolved_source(&m).is_err());
        m.set_word(d("D1028"), -1);
        m.set_word(d("D1029"), 0);
        assert!(action.prepare(&m, &mut BTreeSet::new()).is_err());
        m.set_word(d("D1028"), i16::MIN);
        assert!(action.resolved_source(&m).is_err());
    }
    let mut m = Memory::default();
    m.data_register_limit = Some(4999);
    assert!(parse("DRAD D4999 D20E0").resolved_source(&m).is_err());
    assert!(
        parse("DRAD K180 D4999E0")
            .prepare(&m, &mut BTreeSet::new())
            .is_err()
    );
    assert!(
        parse("DRAD D0E0 D4999")
            .prepare(&m, &mut BTreeSet::new())
            .is_err()
    );
}

#[test]
fn off_and_held_pulse_visits_do_not_touch_addresses_or_flags_then_rearm() {
    for (name, first, second, expected_first, expected_second) in [
        ("DRADP", 0x4334_0000, 0xc334_0000, 0x4049_0fdb, 0xc049_0fdb),
        ("DDEGP", 0x4049_0fdb, 0xc049_0fdb, 0x4334_0000, 0xc334_0000),
    ] {
        let program = compile_il(&format!("LD X0\n{name} D0E0 D20F0\nEND")).unwrap();
        let mut sim = Simulator::default();
        seed(&mut sim.memory);
        sim.memory.set_word(d("D1028"), -30000);
        run(&mut sim, &program);
        assert_eq!(flags(&sim.memory), [true, true, false]);
        assert!(sim.memory.execution_limitations.is_empty());
        assert!(!sim.observed_devices.contains(&d("D1028")));
        sim.memory.set_word(d("D1028"), 0);
        put(&mut sim.memory, "D0", first);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &program);
        assert_eq!(get(&sim.memory, "D20"), expected_first);
        sim.observed_devices.clear();
        seed(&mut sim.memory);
        sim.memory.set_word(d("D1028"), -30000);
        run(&mut sim, &program);
        assert_eq!(flags(&sim.memory), [true, true, false]);
        assert_eq!(get(&sim.memory, "D20"), expected_first);
        assert!(!sim.observed_devices.contains(&d("D1028")));
        assert!(sim.memory.execution_limitations.is_empty());
        sim.memory.set_input(d("X0"), false);
        run(&mut sim, &program);
        sim.memory.set_word(d("D1028"), 0);
        put(&mut sim.memory, "D0", second);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &program);
        assert_eq!(get(&sim.memory, "D20"), expected_second);
    }
    let mut m = Memory::default();
    seed(&mut m);
    m.set_word(d("D1028"), -30000);
    let mut observed = BTreeSet::new();
    parse("DDEG D0E0 D20F0").execute(&mut m, false, &mut observed);
    assert!(observed.is_empty() && m.execution_limitations.is_empty());
    assert_eq!(flags(&m), [true, true, false]);
}

#[test]
fn actual_observation_and_held_result_monitoring_use_resolved_pairs() {
    let action = parse("DDEG D0E0 D100F0");
    let mut m = Memory::default();
    m.set_word(d("D1028"), 1);
    m.set_word(d("D1029"), 2);
    put(&mut m, "D1", 0x4049_0fdb);
    let mut static_devices = BTreeSet::new();
    action.devices(&mut static_devices);
    assert!(static_devices.contains(&d("D1028")) && static_devices.contains(&d("D1029")));
    assert!(!static_devices.contains(&d("D0")) && !static_devices.contains(&d("D100")));
    let mut observed = BTreeSet::new();
    action.execute(&mut m, true, &mut observed);
    for address in ["D1", "D2", "D102", "D103"] {
        assert!(observed.contains(&d(address)));
    }
    assert_eq!(action.display_value(&m), "180");
    m.set_word(d("D1028"), -30000);
    observed.clear();
    action.execute(&mut m, true, &mut observed);
    assert!(!observed.contains(&d("D1029")) && !observed.contains(&d("D102")));
    let mut ui = BTreeSet::new();
    action.observed_devices(&m, &mut ui);
    assert!(ui.contains(&d("D102")) && ui.contains(&d("D103")));
    assert_eq!(action.display_value(&m), "180");
    m.set_word(d("D1029"), -30000);
    assert!(action.display_value(&m).starts_with("İndeks:"));
}

#[test]
fn ordinary_and_indexed_angle_paths_share_results_without_r_pc_skips() {
    for (name, raw, expected) in [
        ("DRAD", 0x4334_0000, 0x4049_0fdb),
        ("DDEG", 0x4049_0fdb, 0x4334_0000),
        ("DRAD", 0x8000_0000, 0),
        ("DDEG", 0x7fc0_0001, 0x7f80_0000),
    ] {
        let mut ordinary = Memory::default();
        seed(&mut ordinary);
        put(&mut ordinary, "D0", raw);
        let mut indexed = ordinary.clone();
        let action = FloatMathAction::parse(name, &["D0".into(), "D20".into()]).unwrap();
        assert!(!action.execute(&mut ordinary));
        parse(&format!("{name} D0E0 D20F0")).execute(&mut indexed, true, &mut BTreeSet::new());
        assert_eq!(get(&ordinary, "D20"), expected);
        assert_eq!(get(&indexed, "D20"), expected);
        assert_eq!(flags(&ordinary), flags(&indexed));
        assert!(!ordinary.system_bit(1067) && !indexed.system_bit(1067));
    }
}

#[test]
fn angle_execution_does_not_consult_m1081_or_publish_decimal_scratch() {
    for profile in [
        DecimalScratchProfile::CurrentPrivate,
        DecimalScratchProfile::LegacyVisible,
    ] {
        for mode in [false, true] {
            let mut m = Memory::default();
            m.decimal_scratch_profile = profile;
            m.set_system(1081, mode);
            m.set_word(d("D5000"), 321);
            m.set_word(d("D5001"), -123);
            m.set_system(1067, true);
            m.set_system(1068, true);
            m.set_word(d("D1067"), 0xe19);
            m.set_word(d("D1068"), 77);
            let mut observed = BTreeSet::new();
            parse("DRAD K180 D20E0").execute(&mut m, true, &mut observed);
            assert_eq!(get(&m, "D20"), 0x4049_0fdb);
            assert_eq!(m.system_bit(1081), mode);
            assert_eq!([m.word(d("D5000")), m.word(d("D5001"))], [321, -123]);
            assert_eq!([m.word(d("D1067")), m.word(d("D1068"))], [0xe19, 77]);
            assert!(!observed.contains(&d("M1081")) && !observed.contains(&d("D5000")));
        }
    }
}
