//! Independent descriptor, mode/domain and IEEE expectations for all twelve
//! indexed trig forms. RTL responsiveness is separately native-qualified.
use crate::float_advanced::AdvancedAction;
use crate::float_math::FloatBinaryError;
use crate::indexed_trig::{IndexedTrigAction, PreparedTrig};
use crate::plc::{Device, Memory, Program, ScanStatus, Simulator, compile_il};
use std::collections::BTreeSet;
fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn parse(raw: &str) -> IndexedTrigAction {
    let mut parts = raw.split_whitespace();
    IndexedTrigAction::parse(
        parts.next().unwrap(),
        &parts.map(str::to_owned).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn put(m: &mut Memory, address: &str, raw: u32) {
    m.set_double_word(d(address), raw as i32);
}
fn get(m: &Memory, address: &str) -> u32 {
    m.double_word(d(address)) as u32
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
fn all_twelve_forms_have_literal_nine_word_layout_and_mode_inventory() {
    for (name, header, expected, forward) in [
        ("DSIN", 0x2501, 0, true),
        ("DSINP", 0x2511, 0, true),
        ("DCOS", 0x2701, 0x3f80_0000, true),
        ("DCOSP", 0x2711, 0x3f80_0000, true),
        ("DTAN", 0x2901, 0, true),
        ("DTANP", 0x2911, 0, true),
        ("DASIN", 0x2b01, 0, false),
        ("DASINP", 0x2b11, 0, false),
        ("DACOS", 0x2d01, 0x3fc9_0fdb, false),
        ("DACOSP", 0x2d11, 0x3fc9_0fdb, false),
        ("DATAN", 0x2f01, 0, false),
        ("DATANP", 0x2f11, 0, false),
    ] {
        let body = format!("{name} D10E1 D100F7");
        let action = parse(&body);
        let words = [
            header, 0x14e6, 0x8086, 0x0080, 0x0080, 0xc8f6, 0x8089, 0x0080, 0x0080,
        ];
        assert_eq!(action.native_words().unwrap(), words);
        assert_eq!(action.label(), body);
        assert_eq!(action.mode_device(), forward.then(|| d("M1018")));
        let p = compile_il(&format!("LD X0\n{body}\nMOV K7 D300\nOUT Y0\nEND")).unwrap();
        assert!(p.requires_vm());
        let encoded = crate::es2_encoder::encode(&p).unwrap();
        assert_eq!(&encoded.words[1..10], &words);
        assert_eq!(encoded.action_pc[1], 10);
        let mut sim = enabled();
        sim.memory.set_word(d("D1182"), 1);
        sim.memory.set_word(d("D1195"), 2);
        sim.memory.set_word(d("D101"), 77);
        sim.memory.set_word(d("D104"), 88);
        seed(&mut sim.memory);
        run(&mut sim, &p);
        assert_eq!(get(&sim.memory, "D102"), expected);
        assert_eq!(flags(&sim.memory), [expected == 0, false, false]);
        assert_eq!(
            [sim.memory.word(d("D101")), sim.memory.word(d("D104"))],
            [77, 88]
        );
        assert_eq!(sim.observed_devices.contains(&d("M1018")), forward);
        assert_eq!(sim.memory.word(d("D300")), 7);
        assert!(sim.memory.bit(d("Y0")));
    }
}

#[test]
fn every_index_bank_is_a_signed_word_for_all_six_families() {
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
        for (name, expected) in [
            ("DSIN", 0x3ef5_7744),
            ("DCOS", 0x3f60_a940),
            ("DTAN", 0x3f0b_da7b),
            ("DASIN", 0x3f06_0a92),
            ("DACOS", 0x3f86_0a92),
            ("DATAN", 0x3eed_6338),
        ] {
            let action = parse(&format!("{name} D10{suffix} D100{suffix}"));
            let mut m = Memory::default();
            m.set_word(d(alias), -2);
            let next = Device {
                index: d(alias).index + 1,
                ..d(alias)
            };
            m.set_word(next, i16::MIN);
            put(&mut m, "D8", 0x3f00_0000);
            let mut observed = BTreeSet::new();
            action.execute(&mut m, true, &mut observed);
            assert_eq!(get(&m, "D98"), expected, "{name} {suffix}");
            assert_eq!(action.modifier_devices(), vec![d(alias)]);
            assert!(observed.contains(&d(alias)) && !observed.contains(&next));
        }
    }
}

#[test]
fn matrices_allow_only_integer_or_d_sources_and_d_targets() {
    for name in [
        "DSIN", "DSINP", "DCOS", "DCOSP", "DTAN", "DTANP", "DASIN", "DASINP", "DACOS", "DACOSP",
        "DATAN", "DATANP",
    ] {
        for args in [
            ["K1", "D20E0"],
            ["H80000000", "D20F7"],
            ["D0E1", "D20"],
            ["D9999E0", "D9999F0"],
        ] {
            assert!(IndexedTrigAction::parse(name, &args.map(str::to_owned)).is_ok());
        }
        for (source, target) in [
            ("F1", "D20E0"),
            ("E1", "D20E0"),
            ("T0E0", "D20"),
            ("C200F0", "D20"),
            ("K8M0E0", "D20"),
            ("D0E8", "D20"),
            ("D9999", "D20E0"),
            ("D0E0", "D9999"),
            ("D0E0", "K1"),
            ("D0E0", "E1"),
            ("D0E0", "T0"),
            ("D0E0", "K8M0"),
        ] {
            assert!(IndexedTrigAction::parse(name, &[source.into(), target.into()]).is_err());
        }
    }
    for name in [
        "SIN", "COS", "TAN", "ASIN", "DSINPP", "DEXP", "DESQR", "DPOW",
    ] {
        assert!(!IndexedTrigAction::supports(name));
    }
    assert_eq!(
        parse("DSIN K30 D20F7").native_words().unwrap(),
        [
            0x2501, 0x1e80, 0x0080, 0x0080, 0x0080, 0x28f6, 0x8089, 0x0080, 0x0080
        ]
    );
}

#[test]
fn degree_mode_uses_two_rounding_boundaries_and_does_not_clamp_angles() {
    for (name, degrees, expected) in [
        ("DSIN", 30, 0x3f00_0000),
        ("DCOS", 30, 0x3f5d_b3d7),
        ("DTAN", 30, 0x3f13_cd3a),
        ("DSIN", 45, 0x3f35_04f3),
        ("DTAN", 45, 0x3f80_0000),
        ("DSIN", 90, 0x3f80_0000),
        ("DCOS", 90, 0xb33b_bd2e),
        ("DSIN", 180, 0xb3bb_bd2e),
        ("DCOS", 180, 0xbf80_0000),
        ("DSIN", 270, 0xbf80_0000),
        ("DSIN", -30, 0xbf00_0000),
        ("DSIN", 360, 0x343b_bd2e),
        ("DSIN", 5, 0x3db2_7eb6),
    ] {
        let mut m = Memory::default();
        m.set_system(1018, true);
        seed(&mut m);
        parse(&format!("{name} K{degrees} D20E0")).execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(get(&m, "D20"), expected, "{name} {degrees}");
        assert_eq!(flags(&m), [false; 3]);
        assert!(!m.system_bit(1067));
    }
    // Single-factor DRAD-style conversion for five degrees would produce
    // sin=0x3db27eb5; the retained two-rounding contract gives ...b6.
}

#[test]
fn radians_and_integer_conversion_are_not_raw_descriptor_bits() {
    for (name, expected) in [
        ("DSIN", 0x3f57_6aa4),
        ("DCOS", 0x3f0a_5140),
        ("DTAN", 0x3fc7_5923),
    ] {
        let mut m = Memory::default();
        parse(&format!("{name} K1 D20E0")).execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(get(&m, "D20"), expected);
    }
    let mut m = Memory::default();
    put(&mut m, "D0", 1);
    parse("DSIN D0E0 D20F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(get(&m, "D20"), 1);
    parse("DATAN K16777217 D20E0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(get(&m, "D20"), 0x3fc9_0fda);
    parse("DASIN H80000000 D20E0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.word(d("D1067")), 0xe19);
}

#[test]
fn inverse_endpoints_are_radians_and_ignore_degree_mode() {
    for (name, raw, expected) in [
        ("DASIN", 0xbf80_0000, 0xbfc9_0fdb),
        ("DASIN", 0, 0),
        ("DASIN", 0x3f00_0000, 0x3f06_0a92),
        ("DASIN", 0x3f80_0000, 0x3fc9_0fdb),
        ("DACOS", 0xbf80_0000, 0x4049_0fdb),
        ("DACOS", 0, 0x3fc9_0fdb),
        ("DACOS", 0x3f00_0000, 0x3f86_0a92),
        ("DACOS", 0x3f80_0000, 0),
        ("DATAN", 0xbf80_0000, 0xbf49_0fdb),
        ("DATAN", 0x3f80_0000, 0x3f49_0fdb),
        ("DATAN", 0x7f80_0000, 0x3fc9_0fdb),
        ("DATAN", 0xff80_0000, 0xbfc9_0fdb),
    ] {
        for mode in [false, true] {
            let mut m = Memory::default();
            m.set_system(1018, mode);
            put(&mut m, "D0", raw);
            seed(&mut m);
            let mut observed = BTreeSet::new();
            parse(&format!("{name} D0E0 D20F0")).execute(&mut m, true, &mut observed);
            assert_eq!(get(&m, "D20"), expected, "{name} {raw:08x}");
            assert_eq!(flags(&m), [expected == 0, false, false]);
            assert!(!observed.contains(&d("M1018")));
        }
    }
}

#[test]
fn inverse_nan_priority_and_range_errors_skip_target_index_and_hold_flags() {
    for name in ["DASIN", "DACOS", "DATAN"] {
        for raw in [0x7fc0_0001, 0xffc0_0001, 0x7f80_0001] {
            let action = parse(&format!("{name} D0E0 D20F7"));
            let mut m = Memory::default();
            put(&mut m, "D0", raw);
            put(&mut m, "D20", 0x1234_5678);
            m.set_word(d("D1195"), -30000);
            seed(&mut m);
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
                PreparedTrig::SemanticError {
                    code: 0xe1a,
                    offset: 1
                }
            );
            action.execute(&mut m, true, &mut observed);
            assert_eq!(flags(&m), [true; 3]);
            assert_eq!([m.word(d("D1067")), m.word(d("D1068"))], [0xe1a, 1]);
            assert_eq!(get(&m, "D20"), 0x1234_5678);
            assert!(!observed.contains(&d("D1195")) && !observed.contains(&d("M1018")));
            assert!(m.execution_limitations.is_empty());
        }
    }
    for name in ["DASIN", "DACOS"] {
        for raw in [0x3f80_0001, 0xbf80_0001, 0x7f80_0000, 0xff80_0000] {
            let mut m = Memory::default();
            put(&mut m, "D0", raw);
            seed(&mut m);
            parse(&format!("{name} D0E0 D20F0")).execute(&mut m, true, &mut BTreeSet::new());
            assert_eq!(m.word(d("D1067")), 0xe19);
            assert_eq!(flags(&m), [true; 3]);
        }
    }
}

#[test]
fn forward_exceptional_cutoff_is_normalized_output_without_plc_domain_error() {
    for name in ["DSIN", "DCOS", "DTAN"] {
        for raw in [
            0x5a00_0000,
            0xda00_0000,
            0x7f80_0000,
            0xff80_0000,
            0x7fc0_0001,
            0xff80_0001,
        ] {
            let action = parse(&format!("{name} D0E0 D20F0"));
            let mut m = Memory::default();
            put(&mut m, "D0", raw);
            seed(&mut m);
            assert_eq!(action.semantic_error(&m).unwrap(), None);
            action.execute(&mut m, true, &mut BTreeSet::new());
            assert_eq!(get(&m, "D20"), 0x7f80_0000);
            assert_eq!(flags(&m), [false, false, true]);
            assert!(!m.system_bit(1067));
        }
    }
}

#[test]
fn signed_zero_uses_full_normalization_instead_of_desqr_flag_hold() {
    for (name, expected) in [
        ("DSIN", 0),
        ("DCOS", 0x3f80_0000),
        ("DTAN", 0),
        ("DASIN", 0),
        ("DACOS", 0x3fc9_0fdb),
        ("DATAN", 0),
    ] {
        for raw in [0, 0x8000_0000] {
            let mut m = Memory::default();
            put(&mut m, "D0", raw);
            seed(&mut m);
            parse(&format!("{name} D0E0 D20F0")).execute(&mut m, true, &mut BTreeSet::new());
            assert_eq!(get(&m, "D20"), expected);
            assert_eq!(flags(&m), [expected == 0, false, false]);
        }
    }
}

#[test]
fn mode_changes_in_same_scan_are_live_but_inverse_never_uses_them() {
    let p = compile_il(
        "LD X0\nRST M1018\nDSIN K1 D20E0\nSET M1018\nDSIN K30 D22E0\nDASIN K1 D24E0\nEND",
    )
    .unwrap();
    let mut sim = enabled();
    run(&mut sim, &p);
    assert_eq!(
        ["D20", "D22", "D24"].map(|a| get(&sim.memory, a)),
        [0x3f57_6aa4, 0x3f00_0000, 0x3fc9_0fdb]
    );
}

#[test]
fn pulse_off_held_mode_changes_and_rearm_are_gated_before_reads() {
    for name in ["DSINP", "DCOSP", "DTANP", "DASINP", "DACOSP", "DATANP"] {
        let p = compile_il(&format!("LD X0\n{name} D0E0 D20F0\nEND")).unwrap();
        let mut sim = Simulator::default();
        seed(&mut sim.memory);
        sim.memory.set_word(d("D1028"), -30000);
        run(&mut sim, &p);
        assert_eq!(flags(&sim.memory), [true; 3]);
        assert!(sim.memory.execution_limitations.is_empty());
        assert!(!sim.observed_devices.contains(&d("M1018")));
        sim.memory.set_word(d("D1028"), 0);
        put(&mut sim.memory, "D0", 0x3f00_0000);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &p);
        let held = get(&sim.memory, "D20");
        sim.memory.set_system(1018, true);
        sim.memory.set_word(d("D1028"), -30000);
        sim.observed_devices.clear();
        seed(&mut sim.memory);
        run(&mut sim, &p);
        assert_eq!(get(&sim.memory, "D20"), held);
        assert_eq!(flags(&sim.memory), [true; 3]);
        assert!(
            !sim.observed_devices.contains(&d("D1028"))
                && !sim.observed_devices.contains(&d("M1018"))
        );
        assert!(sim.memory.execution_limitations.is_empty());
        sim.memory.set_input(d("X0"), false);
        run(&mut sim, &p);
        sim.memory.set_word(d("D1028"), 0);
        put(&mut sim.memory, "D0", 0);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &p);
        let expected = match name {
            "DCOSP" => 0x3f80_0000,
            "DACOSP" => 0x3fc9_0fdb,
            _ => 0,
        };
        assert_eq!(get(&sim.memory, "D20"), expected);
        assert_eq!(flags(&sim.memory), [expected == 0, false, false]);
    }
}

#[test]
fn error_pc_sticks_and_later_success_recovers_result_flags() {
    let p = compile_il("LD X0\nMOV K7 D50\nDACOS D0E0 D20F0\nOUT Y0\nEND").unwrap();
    let encoded = crate::es2_encoder::encode(&p).unwrap();
    let mut sim = enabled();
    put(&mut sim.memory, "D0", 0x4000_0000);
    seed(&mut sim.memory);
    run(&mut sim, &p);
    assert_eq!(sim.memory.word(d("D1067")), 0xe19);
    assert_eq!(sim.memory.word(d("D1068")) as u16, encoded.action_pc[1] + 1);
    assert_eq!(flags(&sim.memory), [true; 3]);
    put(&mut sim.memory, "D0", 0x3f80_0000);
    run(&mut sim, &p);
    assert_eq!(get(&sim.memory, "D20"), 0);
    assert_eq!(flags(&sim.memory), [true, false, false]);
    assert_eq!(sim.memory.word(d("D1067")), 0xe19);
    assert!(sim.memory.bit(d("Y0")));
}

#[test]
fn overlap_and_self_indexed_writes_use_one_snapshot_and_fixed_pair() {
    let mut m = Memory::default();
    put(&mut m, "D0", 0x3f00_0000);
    m.set_word(d("D3"), 77);
    parse("DASIN D0E0 D1F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(get(&m, "D1"), 0x3f06_0a92);
    assert_eq!([m.word(d("D0")), m.word(d("D3"))], [0, 77]);
    let mut m = Memory::default();
    m.set_word(d("D1028"), 1);
    m.set_word(d("D1030"), 88);
    parse("DASIN K1 D1027E0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.word(d("D1028")) as u16, 0x0fdb);
    assert_eq!(m.word(d("D1029")) as u16, 0x3fc9);
    assert_eq!(m.word(d("D1030")), 88);
}

#[test]
fn model_bounds_loop_indices_and_actual_observation_follow_resolved_pairs() {
    for last in [4999, 9999] {
        let action = parse(&format!("DSIN D{last}E0 D{last}F0"));
        let mut m = Memory::default();
        m.data_register_limit = Some(last);
        m.set_word(d("D1028"), -1);
        m.set_word(d("D1029"), -1);
        action.execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(get(&m, &format!("D{}", last - 1)), 0);
        m.set_word(d("D1028"), 0);
        assert!(action.resolved_source(&m).is_err());
        m.set_word(d("D1028"), -1);
        m.set_word(d("D1029"), 0);
        assert!(action.prepare(&m, &mut BTreeSet::new()).is_err());
    }
    let p = compile_il("LD X0\nMOV K0 E0\nFOR K3\nDATAN D10E0 D100E0\nADD E0 K2 E0\nNEXT\nEND")
        .unwrap();
    let mut sim = enabled();
    for (address, raw) in [("D10", 0), ("D12", 0x3f80_0000), ("D14", 0xbf80_0000)] {
        put(&mut sim.memory, address, raw);
    }
    run(&mut sim, &p);
    assert_eq!(
        ["D100", "D102", "D104"].map(|a| get(&sim.memory, a)),
        [0, 0x3f49_0fdb, 0xbf49_0fdb]
    );
    for a in ["D10", "D11", "D12", "D13", "D14", "D15", "D104", "D105"] {
        assert!(sim.observed_devices.contains(&d(a)));
    }
}

#[test]
fn structural_failures_and_monitoring_do_not_invent_domain_errors_or_mode_reads() {
    let action = parse("DSIN D0E0 D100F0");
    let mut m = Memory::default();
    m.set_word(d("D1028"), -30000);
    seed(&mut m);
    let mut observed = BTreeSet::new();
    action.execute(&mut m, true, &mut observed);
    assert_eq!(flags(&m), [true; 3]);
    assert!(!observed.contains(&d("M1018")) && !observed.contains(&d("D1029")));
    assert!(!m.system_bit(1067));
    m.set_word(d("D1028"), 0);
    m.set_word(d("D1029"), -30000);
    action.execute(&mut m, true, &mut observed);
    assert_eq!(flags(&m), [true, false, false]);
    assert!(observed.contains(&d("M1018")));
    assert!(action.display_value(&m).starts_with("İndeks:"));
    let inverse = parse("DASIN D0E0 D100F0");
    m.set_word(d("D1029"), 2);
    put(&mut m, "D102", 0x3f80_0000);
    put(&mut m, "D0", 0x4000_0000);
    observed.clear();
    inverse.execute(&mut m, true, &mut observed);
    assert!(!observed.contains(&d("D1029")));
    let mut ui = BTreeSet::new();
    inverse.observed_devices(&m, &mut ui);
    assert!(ui.contains(&d("D102")) && ui.contains(&d("D103")));
    assert!(!ui.contains(&d("M1018")));
    assert_eq!(inverse.display_value(&m), "1");
}

#[test]
fn ordinary_and_indexed_share_results_without_m1081_or_public_scratch_effects() {
    for (name, raw, mode, expected) in [
        ("DSIN", 0x41f0_0000, true, 0x3f00_0000),
        ("DCOS", 0, false, 0x3f80_0000),
        ("DTAN", 0x3f00_0000, false, 0x3f0b_da7b),
        ("DASIN", 0x3f00_0000, true, 0x3f06_0a92),
        ("DACOS", 0x3f00_0000, true, 0x3f86_0a92),
        ("DATAN", 0x3f00_0000, true, 0x3eed_6338),
    ] {
        let mut ordinary = Memory::default();
        put(&mut ordinary, "D0", raw);
        ordinary.set_system(1018, mode);
        ordinary.set_system(1081, true);
        ordinary.set_word(d("D5000"), 123);
        ordinary.set_word(d("D5001"), 456);
        let mut indexed = ordinary.clone();
        AdvancedAction::parse(name, &["D0".into(), "D20".into()])
            .unwrap()
            .execute(&mut ordinary);
        let mut observed = BTreeSet::new();
        parse(&format!("{name} D0E0 D20F0")).execute(&mut indexed, true, &mut observed);
        for m in [&ordinary, &indexed] {
            assert_eq!(get(m, "D20"), expected);
            assert_eq!(m.system_bit(1018), mode);
            assert!(m.system_bit(1081));
            assert_eq!([m.word(d("D5000")), m.word(d("D5001"))], [123, 456]);
        }
        assert!(!observed.contains(&d("M1081")));
    }
}

#[test]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn mode_conversion_inverse_kernel_and_error_restore_caller_control_word() {
    fn read() -> u16 {
        let mut word = 0;
        unsafe {
            std::arch::asm!("fnstcw word ptr [{p}]",p=in(reg)&mut word,options(nostack,preserves_flags));
        }
        word
    }
    fn write(word: u16) {
        unsafe {
            std::arch::asm!("fldcw word ptr [{p}]",p=in(reg)&word,options(nostack,preserves_flags));
        }
    }
    struct Restore(u16);
    impl Drop for Restore {
        fn drop(&mut self) {
            write(self.0);
        }
    }
    let _restore = Restore(read());
    let caller = (read() & !0x0f00) | 0x0600;
    write(caller);
    for (name, raw, mode) in [
        ("DSIN", 0x41f0_0000, true),
        ("DTAN", 0x3f00_0000, false),
        ("DASIN", 0x3f00_0000, true),
        ("DACOS", 0x4000_0000, false),
        ("DATAN", 0x7fc0_0001, false),
    ] {
        let mut m = Memory::default();
        put(&mut m, "D0", raw);
        m.set_system(1018, mode);
        let action = parse(&format!("{name} D0E0 D20F0"));
        let _ = action.semantic_error(&m).unwrap();
        assert_eq!(read(), caller);
        let _ = action.prepare(&m, &mut BTreeSet::new()).unwrap();
        assert_eq!(read(), caller);
        action.execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(read(), caller);
    }
}
