//! Floating DESQR independent IEEE/flag/operand expectations. Integer DSQR
//! and its arithmetic flags are a different instruction family.
use crate::float_advanced::AdvancedAction;
use crate::float_math::FloatBinaryError;
use crate::indexed_desqr::{IndexedDesqrAction, PreparedDesqr};
use crate::plc::{Device, Memory, Program, ScanStatus, Simulator, compile_il};
use std::collections::BTreeSet;
fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn parse(raw: &str) -> IndexedDesqrAction {
    let mut parts = raw.split_whitespace();
    IndexedDesqrAction::parse(
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
fn both_forms_encode_nine_words_and_write_one_float_pair() {
    for (name, header) in [("DESQR", 0x1f01), ("DESQRP", 0x1f11)] {
        let body = format!("{name} D10E1 D100F7");
        let action = parse(&body);
        let words = [
            header, 0x14e6, 0x8086, 0x0080, 0x0080, 0xc8f6, 0x8089, 0x0080, 0x0080,
        ];
        assert_eq!(action.native_words().unwrap(), words);
        assert_eq!(action.label(), body);
        assert_eq!(action.dest(), d("D100"));
        let p = compile_il(&format!("LD X0\n{body}\nMOV K7 D300\nOUT Y0\nEND")).unwrap();
        assert!(p.requires_vm());
        let encoded = crate::es2_encoder::encode(&p).unwrap();
        assert_eq!(&encoded.words[1..10], &words);
        assert_eq!(encoded.action_pc[1], 10);
        let mut sim = enabled();
        sim.memory.set_word(d("D1182"), 1);
        sim.memory.set_word(d("D1195"), 2);
        put(&mut sim.memory, "D11", 0x4080_0000);
        seed(&mut sim.memory);
        sim.memory.set_word(d("D101"), 77);
        sim.memory.set_word(d("D104"), 88);
        run(&mut sim, &p);
        assert_eq!(get(&sim.memory, "D102"), 0x4000_0000);
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
fn all_sixteen_index_aliases_are_signed_words_in_both_roles() {
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
        let action = parse(&format!("DESQR D10{suffix} D100{suffix}"));
        let mut m = Memory::default();
        m.set_word(d(alias), -2);
        let next = Device {
            index: d(alias).index + 1,
            ..d(alias)
        };
        m.set_word(next, i16::MIN);
        put(&mut m, "D8", 0x4080_0000);
        let mut observed = BTreeSet::new();
        action.execute(&mut m, true, &mut observed);
        assert_eq!(get(&m, "D98"), 0x4000_0000, "{suffix}");
        assert_eq!(action.modifier_devices(), vec![d(alias)]);
        assert!(observed.contains(&d(alias)) && !observed.contains(&next));
        assert!(m.execution_limitations.is_empty());
    }
}

#[test]
fn operand_matrix_keeps_integer_sqrt_and_other_float_roles_separate() {
    for name in ["DESQR", "DESQRP"] {
        for args in [
            ["K1234", "D20E0"],
            ["H80000000", "D20F7"],
            ["D0E1", "D20"],
            ["D9999E0", "D9999F0"],
        ] {
            assert!(IndexedDesqrAction::parse(name, &args.map(str::to_owned)).is_ok());
        }
        for (source, target) in [
            ("F4", "D20E0"),
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
            assert!(IndexedDesqrAction::parse(name, &[source.into(), target.into()]).is_err());
        }
    }
    for name in ["SQR", "SQRP", "DSQR", "DSQRP", "DESQRPP", "DEXP", "DLN"] {
        assert!(!IndexedDesqrAction::supports(name));
    }
    assert!(!IndexedDesqrAction::has_modifiers(&[
        "HFEED".into(),
        "D20".into()
    ]));
    assert_eq!(
        parse("DESQR K1234 D20F7").native_words().unwrap(),
        [
            0x1f01, 0xd280, 0x0480, 0x0080, 0x0080, 0x28f6, 0x8089, 0x0080, 0x0080
        ]
    );
}

#[test]
fn integer_sources_are_rounded_to_float_and_raw_negative_zero_remains_valid() {
    for (source, expected) in [
        ("K1", 0x3f80_0000),
        ("K4", 0x4000_0000),
        ("K1234", 0x420c_836b),
        ("K16777217", 0x4580_0000),
        ("K2147483647", 0x4735_04f3),
    ] {
        let mut m = Memory::default();
        parse(&format!("DESQR {source} D20E0")).execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(get(&m, "D20"), expected);
        assert_eq!(flags(&m), [false; 3]);
    }
    let mut m = Memory::default();
    seed(&mut m);
    parse("DESQR H80000000 D20E0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.word(d("D1067")), 0xe1b);
    assert_eq!(flags(&m), [true; 3]);
    let mut m = Memory::default();
    put(&mut m, "D0", 0x8000_0000);
    m.set_system(1021, true);
    parse("DESQR D0E0 D20F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(get(&m, "D20"), 0);
    assert_eq!(flags(&m), [true, true, false]);
}

#[test]
fn signed_zero_preserves_every_borrow_carry_combination_and_sets_only_zero() {
    for raw in [0, 0x8000_0000] {
        for mask in 0..8 {
            let mut ordinary = Memory::default();
            put(&mut ordinary, "D0", raw);
            put(&mut ordinary, "D20", 0x1234_5678);
            for (bit, flag) in [1020, 1021, 1022].into_iter().enumerate() {
                ordinary.set_system(flag, mask & (1 << bit) != 0);
            }
            let mut indexed = ordinary.clone();
            let action = parse("DESQR D0E0 D20F0");
            let before = flags(&indexed);
            assert!(action.prepare(&indexed, &mut BTreeSet::new()).is_ok());
            assert_eq!(flags(&indexed), before);
            action.execute(&mut indexed, true, &mut BTreeSet::new());
            AdvancedAction::parse("DESQR", &["D0".into(), "D20".into()])
                .unwrap()
                .execute(&mut ordinary);
            for m in [&ordinary, &indexed] {
                assert_eq!(get(m, "D20"), 0);
                assert_eq!(flags(m), [true, mask & 2 != 0, mask & 4 != 0]);
                assert!(!m.system_bit(1067));
            }
        }
    }
}

#[test]
fn positive_finite_and_infinite_results_use_full_normalization() {
    for (raw, expected, carry) in [
        (1, 0x1a35_04f3, false),
        (0x0080_0000, 0x2000_0000, false),
        (0x3f80_0000, 0x3f80_0000, false),
        (0x4000_0000, 0x3fb5_04f3, false),
        (0x7f7f_ffff, 0x5f7f_ffff, false),
        (0x7f80_0000, 0x7f80_0000, true),
    ] {
        let mut m = Memory::default();
        put(&mut m, "D0", raw);
        seed(&mut m);
        parse("DESQR D0E0 D20F0").execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(get(&m, "D20"), expected);
        assert_eq!(flags(&m), [false, false, carry]);
        assert!(!m.system_bit(1067));
    }
}

#[test]
fn negative_and_nan_sources_error_before_target_index_and_preserve_flags() {
    for raw in [
        0xbf80_0000,
        0xff80_0000,
        0x8000_0001,
        0x7fc0_0001,
        0xffc0_0001,
        0x7f80_0001,
    ] {
        let action = parse("DESQR D0E0 D20F7");
        let mut m = Memory::default();
        put(&mut m, "D0", raw);
        put(&mut m, "D20", 0x1234_5678);
        m.set_word(d("D1195"), -30000);
        seed(&mut m);
        assert_eq!(
            action.semantic_error(&m).unwrap(),
            Some(FloatBinaryError {
                code: 0xe1b,
                offset: 1
            })
        );
        let mut observed = BTreeSet::new();
        assert_eq!(
            action.prepare(&m, &mut observed).unwrap(),
            PreparedDesqr::SemanticError {
                code: 0xe1b,
                offset: 1
            }
        );
        assert!(!m.system_bit(1067));
        action.execute(&mut m, true, &mut observed);
        assert_eq!(flags(&m), [true; 3]);
        assert_eq!([m.word(d("D1067")), m.word(d("D1068"))], [0xe1b, 1]);
        assert_eq!(get(&m, "D20"), 0x1234_5678);
        assert!(!observed.contains(&d("D1195")) && !observed.contains(&d("D20")));
        assert!(m.execution_limitations.is_empty());
    }
}

#[test]
fn actual_error_pc_sticks_while_recovery_updates_results_and_flags() {
    let p = compile_il("LD X0\nMOV K7 D50\nDESQR D0E0 D20F0\nOUT Y0\nEND").unwrap();
    let encoded = crate::es2_encoder::encode(&p).unwrap();
    let mut sim = enabled();
    put(&mut sim.memory, "D0", 0xbf80_0000);
    seed(&mut sim.memory);
    run(&mut sim, &p);
    assert_eq!(sim.memory.word(d("D1067")), 0xe1b);
    assert_eq!(sim.memory.word(d("D1068")) as u16, encoded.action_pc[1] + 1);
    assert_eq!(flags(&sim.memory), [true; 3]);
    put(&mut sim.memory, "D0", 0);
    run(&mut sim, &p);
    assert_eq!(flags(&sim.memory), [true; 3]);
    put(&mut sim.memory, "D0", 0x4080_0000);
    run(&mut sim, &p);
    assert_eq!(get(&sim.memory, "D20"), 0x4000_0000);
    assert_eq!(flags(&sim.memory), [false; 3]);
    assert_eq!(sim.memory.word(d("D1067")), 0xe1b);
    assert!(sim.memory.bit(d("Y0")));
}

#[test]
fn aliases_use_one_snapshot_and_once_resolved_destination_pair() {
    let mut m = Memory::default();
    put(&mut m, "D0", 0x4000_0000);
    m.set_word(d("D3"), 77);
    parse("DESQR D0E0 D1F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(get(&m, "D1"), 0x3fb5_04f3);
    assert_eq!([m.word(d("D0")), m.word(d("D3"))], [0, 77]);
    let mut m = Memory::default();
    m.set_word(d("D1028"), 1);
    m.set_word(d("D1030"), 88);
    parse("DESQR K2 D1027E0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.word(d("D1028")) as u16, 0x04f3);
    assert_eq!(m.word(d("D1029")) as u16, 0x3fb5);
    assert_eq!(m.word(d("D1030")), 88);
    let mut m = Memory::default();
    m.set_word(d("D1028"), 1);
    parse("DESQR D1027E0 D20F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(get(&m, "D20"), 0x1a35_04f3);
}

#[test]
fn loop_visits_and_model_endpoints_resolve_live_indices() {
    let p = compile_il("LD X0\nMOV K0 E0\nFOR K3\nDESQR D10E0 D100E0\nADD E0 K2 E0\nNEXT\nEND")
        .unwrap();
    let mut sim = enabled();
    for (address, raw) in [("D10", 0), ("D12", 0x3f80_0000), ("D14", 0x4080_0000)] {
        put(&mut sim.memory, address, raw);
    }
    run(&mut sim, &p);
    assert_eq!(
        ["D100", "D102", "D104"].map(|a| get(&sim.memory, a)),
        [0, 0x3f80_0000, 0x4000_0000]
    );
    for last in [4999, 9999] {
        let action = parse(&format!("DESQR D{last}E0 D{last}F0"));
        let mut m = Memory::default();
        m.data_register_limit = Some(last);
        m.set_word(d("D1028"), -1);
        m.set_word(d("D1029"), -1);
        put(&mut m, &format!("D{}", last - 1), 0x4080_0000);
        action.execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(get(&m, &format!("D{}", last - 1)), 0x4000_0000);
        m.set_word(d("D1028"), 0);
        assert!(action.resolved_source(&m).is_err());
        m.set_word(d("D1028"), -1);
        m.set_word(d("D1029"), 0);
        assert!(action.prepare(&m, &mut BTreeSet::new()).is_err());
    }
    let mut m = Memory::default();
    m.data_register_limit = Some(4999);
    assert!(parse("DESQR D4999 D20E0").resolved_source(&m).is_err());
    assert!(
        parse("DESQR K1 D4999E0")
            .prepare(&m, &mut BTreeSet::new())
            .is_err()
    );
}

#[test]
fn off_and_held_pulse_visits_preserve_zero_flags_without_observing_invalid_indices() {
    let p = compile_il("LD X0\nDESQRP D0E0 D20F0\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_system(1021, true);
    sim.memory.set_system(1022, true);
    sim.memory.set_word(d("D1028"), -30000);
    run(&mut sim, &p);
    assert_eq!(flags(&sim.memory), [false, true, true]);
    assert!(sim.memory.execution_limitations.is_empty());
    sim.memory.set_word(d("D1028"), 0);
    sim.memory.set_input(d("X0"), true);
    run(&mut sim, &p);
    assert_eq!(flags(&sim.memory), [true; 3]);
    sim.memory.set_system(1020, false);
    sim.memory.set_word(d("D1028"), -30000);
    sim.observed_devices.clear();
    run(&mut sim, &p);
    assert_eq!(flags(&sim.memory), [false, true, true]);
    assert!(!sim.observed_devices.contains(&d("D1028")));
    assert!(sim.memory.execution_limitations.is_empty());
    sim.memory.set_input(d("X0"), false);
    run(&mut sim, &p);
    sim.memory.set_word(d("D1028"), 0);
    put(&mut sim.memory, "D0", 0x4080_0000);
    sim.memory.set_input(d("X0"), true);
    run(&mut sim, &p);
    assert_eq!(get(&sim.memory, "D20"), 0x4000_0000);
    assert_eq!(flags(&sim.memory), [false; 3]);
}

#[test]
fn structural_failure_order_and_held_monitoring_keep_zero_branch_flags() {
    let action = parse("DESQR D0E0 D100F0");
    let mut m = Memory::default();
    m.set_word(d("D1028"), 1);
    m.set_word(d("D1029"), 2);
    put(&mut m, "D1", 0x4080_0000);
    let mut observed = BTreeSet::new();
    action.execute(&mut m, true, &mut observed);
    for address in ["D1", "D2", "D102", "D103"] {
        assert!(observed.contains(&d(address)));
    }
    assert_eq!(action.display_value(&m), "2");
    put(&mut m, "D1", 0xbf80_0000);
    observed.clear();
    action.execute(&mut m, true, &mut observed);
    assert!(!observed.contains(&d("D1029")));
    let mut ui = BTreeSet::new();
    action.observed_devices(&m, &mut ui);
    assert!(ui.contains(&d("D102")));
    assert_eq!(action.display_value(&m), "2");
    m.set_word(d("D1028"), -30000);
    seed(&mut m);
    observed.clear();
    action.execute(&mut m, true, &mut observed);
    assert_eq!(flags(&m), [true; 3]);
    assert!(!observed.contains(&d("D1029")));
    m.set_word(d("D1028"), 1);
    m.set_word(d("D1029"), -30000);
    put(&mut m, "D1", 0);
    m.set_system(1020, false);
    action.execute(&mut m, true, &mut observed);
    assert_eq!(flags(&m), [true; 3]);
    assert!(action.display_value(&m).starts_with("İndeks:"));
    put(&mut m, "D1", 0x4080_0000);
    action.execute(&mut m, true, &mut observed);
    assert_eq!(flags(&m), [false; 3]);
}

#[test]
fn ordinary_and_indexed_positive_paths_ignore_unrelated_modes_and_scratch() {
    let mut ordinary = Memory::default();
    put(&mut ordinary, "D0", 0x4000_0000);
    seed(&mut ordinary);
    ordinary.set_system(1081, true);
    ordinary.set_system(1018, true);
    ordinary.set_word(d("D5000"), 123);
    ordinary.set_word(d("D5001"), 456);
    let mut indexed = ordinary.clone();
    AdvancedAction::parse("DESQR", &["D0".into(), "D20".into()])
        .unwrap()
        .execute(&mut ordinary);
    let mut observed = BTreeSet::new();
    parse("DESQR D0E0 D20F0").execute(&mut indexed, true, &mut observed);
    for m in [&ordinary, &indexed] {
        assert_eq!(get(m, "D20"), 0x3fb5_04f3);
        assert_eq!(flags(m), [false; 3]);
        assert!(m.system_bit(1081) && m.system_bit(1018));
        assert_eq!([m.word(d("D5000")), m.word(d("D5001"))], [123, 456]);
    }
    assert!(!observed.contains(&d("M1081")) && !observed.contains(&d("M1018")));
}

#[test]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn every_outcome_restores_the_callers_x87_control_word() {
    fn read() -> u16 {
        let mut word = 0;
        unsafe {
            std::arch::asm!("fnstcw word ptr [{p}]", p = in(reg) &mut word, options(nostack, preserves_flags));
        }
        word
    }
    fn write(word: u16) {
        unsafe {
            std::arch::asm!("fldcw word ptr [{p}]", p = in(reg) &word, options(nostack, preserves_flags));
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
    for raw in [0, 0x4000_0000, 0xbf80_0000, 0x7fc0_0001] {
        let mut m = Memory::default();
        put(&mut m, "D0", raw);
        let action = parse("DESQR D0E0 D20F0");
        let _ = action.semantic_error(&m).unwrap();
        assert_eq!(read(), caller);
        let _ = action.prepare(&m, &mut BTreeSet::new()).unwrap();
        assert_eq!(read(), caller);
        action.execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(read(), caller);
    }
}
