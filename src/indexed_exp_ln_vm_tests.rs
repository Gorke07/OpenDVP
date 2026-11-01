//! Literal binary32/domain expectations for indexed exponential/logarithm.
//! Current native exceptional responsiveness is a separate bounded corpus.
use crate::float_advanced::AdvancedAction;
use crate::float_math::FloatBinaryError;
use crate::indexed_exp_ln::{IndexedExpLnAction, PreparedExpLn};
use crate::plc::{Device, Memory, Program, ScanStatus, Simulator, compile_il};
use std::collections::BTreeSet;

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn parse(raw: &str) -> IndexedExpLnAction {
    let mut parts = raw.split_whitespace();
    IndexedExpLnAction::parse(
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
fn four_forms_encode_nine_words_and_execute_distinct_domains() {
    for (name, header, input, expected, zero) in [
        ("DEXP", 0x1901, 0, 0x3f80_0000, false),
        ("DEXPP", 0x1911, 0, 0x3f80_0000, false),
        ("DLN", 0x1b01, 0x3f80_0000, 0, true),
        ("DLNP", 0x1b11, 0x3f80_0000, 0, true),
    ] {
        let body = format!("{name} D10E1 D100F7");
        let action = parse(&body);
        let words = [
            header, 0x14e6, 0x8086, 0x0080, 0x0080, 0xc8f6, 0x8089, 0x0080, 0x0080,
        ];
        assert_eq!(action.native_words().unwrap(), words);
        assert_eq!(action.label(), body);
        let p = compile_il(&format!("LD X0\n{body}\nMOV K7 D300\nOUT Y0\nEND")).unwrap();
        assert!(p.requires_vm());
        let encoded = crate::es2_encoder::encode(&p).unwrap();
        assert_eq!(&encoded.words[1..10], &words);
        assert_eq!(encoded.action_pc[1], 10);
        let mut sim = enabled();
        sim.memory.set_word(d("D1182"), 1);
        sim.memory.set_word(d("D1195"), 2);
        put(&mut sim.memory, "D11", input);
        sim.memory.set_word(d("D101"), 77);
        sim.memory.set_word(d("D104"), 88);
        seed(&mut sim.memory);
        run(&mut sim, &p);
        assert_eq!(get(&sim.memory, "D102"), expected);
        assert_eq!(flags(&sim.memory), [zero, false, false]);
        assert_eq!(
            [sim.memory.word(d("D101")), sim.memory.word(d("D104"))],
            [77, 88]
        );
        assert_eq!(sim.memory.word(d("D300")), 7);
        assert!(sim.memory.bit(d("Y0")));
    }
}

#[test]
fn all_sixteen_banks_are_live_signed_words_for_both_roles() {
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
            ("DEXP", 0x3f80_0000, 0x402d_f854),
            ("DLN", 0x4000_0000, 0x3f31_7218),
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
        }
    }
}

#[test]
fn role_matrix_does_not_expand_other_advanced_functions() {
    for name in ["DEXP", "DEXPP", "DLN", "DLNP"] {
        for (source, target) in [
            ("K1", "D20E0"),
            ("H80000000", "D20F7"),
            ("D0E1", "D20"),
            ("D9999E0", "D9999F0"),
        ] {
            assert!(IndexedExpLnAction::parse(name, &[source.into(), target.into()]).is_ok());
        }
        for (source, target) in [
            ("F1", "D20E0"),
            ("E1", "D20E0"),
            ("T0E0", "D20"),
            ("C200F0", "D20"),
            ("K8M0E0", "D20"),
            ("D0E8", "D20"),
            ("K1E0", "D20"),
            ("D9999", "D20E0"),
            ("D0E0", "D9999"),
            ("D0E0", "K1"),
            ("D0E0", "E1"),
            ("D0E0", "T0"),
            ("D0E0", "K8M0"),
        ] {
            assert!(
                IndexedExpLnAction::parse(name, &[source.into(), target.into()]).is_err(),
                "{name} {source} {target}"
            );
        }
    }
    for name in [
        "EXP", "LN", "DEXPPP", "DLNPP", "DLOG", "DESQR", "DPOW", "DSIN",
    ] {
        assert!(!IndexedExpLnAction::supports(name));
    }
    assert!(!IndexedExpLnAction::has_modifiers(&[
        "HDEAD".into(),
        "D20".into()
    ]));
}

#[test]
fn integer_conversion_and_raw_binary32_sources_are_distinct() {
    for (name, source, expected) in [
        ("DEXP", "K0", 0x3f80_0000),
        ("DEXP", "K1", 0x402d_f854),
        ("DEXP", "K-1", 0x3ebc_5ab2),
        ("DEXP", "H80000000", 0),
        ("DLN", "K1", 0),
        ("DLN", "K2", 0x3f31_7218),
        ("DLN", "K16777217", 0x4185_1592),
    ] {
        let mut m = Memory::default();
        parse(&format!("{name} {source} D20E0")).execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(get(&m, "D20"), expected, "{name} {source}");
    }
    let mut m = Memory::default();
    put(&mut m, "D0", 0x8000_0000);
    parse("DEXP D0E0 D20F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(get(&m, "D20"), 0x3f80_0000);
    put(&mut m, "D0", 1);
    parse("DEXP D0E0 D20F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(get(&m, "D20"), 0x3f80_0000);
    assert_eq!(
        parse("DEXP K-1 D20F7").native_words().unwrap(),
        [
            0x1901, 0xff80, 0xff80, 0xff80, 0xff80, 0x28f6, 0x8089, 0x0080, 0x0080
        ]
    );
}

#[test]
fn independent_finite_kernel_vectors_cover_rounding_and_subnormals() {
    for (name, raw, expected) in [
        ("DEXP", 0xbf80_0000, 0x3ebc_5ab2),
        ("DEXP", 0x4000_0000, 0x40ec_7326),
        ("DEXP", 0xc2c8_0000, 0x0000_001b),
        ("DEXP", 0x42b0_0000, 0x7ef8_82b7),
        ("DLN", 0x3f00_0000, 0xbf31_7218),
        ("DLN", 0x4120_0000, 0x4013_5d8e),
        ("DLN", 0x0080_0000, 0xc2ae_ac50),
        ("DLN", 1, 0xc2ce_8ed0),
    ] {
        let mut m = Memory::default();
        put(&mut m, "D0", raw);
        seed(&mut m);
        parse(&format!("{name} D0E0 D20F0")).execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(get(&m, "D20"), expected, "{name} {raw:08x}");
        assert_eq!(flags(&m), [false; 3]);
        assert!(!m.system_bit(1067));
    }
}

#[test]
fn domain_errors_hold_flags_and_skip_invalid_target_modifier() {
    for (name, raw, code) in [
        ("DEXP", 0x7fc0_0001, 0xe1a),
        ("DEXP", 0xff80_0001, 0xe1a),
        ("DLN", 0, 0xe19),
        ("DLN", 0x8000_0000, 0xe19),
        ("DLN", 0xbf80_0000, 0xe19),
        ("DLN", 0x7fc0_0001, 0xe19),
        ("DLN", 0x7f80_0001, 0xe19),
        ("DLN", 0xff80_0000, 0xe19),
    ] {
        let action = parse(&format!("{name} D0E0 D20F7"));
        let mut m = Memory::default();
        seed(&mut m);
        put(&mut m, "D0", raw);
        put(&mut m, "D20", 0x1234_5678);
        m.set_word(d("D1195"), -30000);
        assert_eq!(
            action.semantic_error(&m).unwrap(),
            Some(FloatBinaryError { code, offset: 1 })
        );
        let mut observed = BTreeSet::new();
        assert_eq!(
            action.prepare(&m, &mut observed).unwrap(),
            PreparedExpLn::SemanticError { code, offset: 1 }
        );
        assert_eq!(flags(&m), [true; 3]);
        assert!(!m.system_bit(1067));
        action.execute(&mut m, true, &mut observed);
        assert_eq!([m.word(d("D1067")), m.word(d("D1068"))], [code, 1]);
        assert_eq!(flags(&m), [true; 3]);
        assert_eq!(get(&m, "D20"), 0x1234_5678);
        assert!(!observed.contains(&d("D1195")) && !observed.contains(&d("D20")));
        assert!(m.execution_limitations.is_empty());
    }
}

#[test]
fn error_pc_is_header_plus_one_and_sticky_errors_do_not_block_recovery() {
    for (name, raw, code, good, expected) in [
        ("DEXP", 0x7fc0_0001, 0xe1a, 0, 0x3f80_0000),
        ("DLN", 0, 0xe19, 0x3f80_0000, 0),
    ] {
        let p = compile_il(&format!(
            "LD X0\nMOV K7 D50\n{name} D0E0 D20F0\nOUT Y0\nEND"
        ))
        .unwrap();
        let encoded = crate::es2_encoder::encode(&p).unwrap();
        let mut sim = enabled();
        put(&mut sim.memory, "D0", raw);
        seed(&mut sim.memory);
        run(&mut sim, &p);
        assert_eq!(sim.memory.word(d("D1067")), code);
        assert_eq!(sim.memory.word(d("D1068")) as u16, encoded.action_pc[1] + 1);
        assert_eq!(flags(&sim.memory), [true; 3]);
        put(&mut sim.memory, "D0", good);
        run(&mut sim, &p);
        assert_eq!(get(&sim.memory, "D20"), expected);
        assert_eq!(sim.memory.word(d("D1067")), code);
        assert_eq!(flags(&sim.memory), [expected == 0, false, false]);
        assert!(sim.memory.bit(d("Y0")));
    }
}

#[test]
fn source_and_target_aliases_keep_snapshot_and_cached_pair() {
    let mut m = Memory::default();
    put(&mut m, "D0", 0x3f80_0000);
    m.set_word(d("D3"), 77);
    parse("DEXP D0E0 D1F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(get(&m, "D1"), 0x402d_f854);
    assert_eq!([m.word(d("D0")), m.word(d("D3"))], [0, 77]);
    let mut m = Memory::default();
    m.set_word(d("D1028"), 1);
    m.set_word(d("D1030"), 88);
    parse("DEXP K1 D1027E0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.word(d("D1028")) as u16, 0xf854);
    assert_eq!(m.word(d("D1029")) as u16, 0x402d);
    assert_eq!(m.word(d("D1030")), 88);
}

#[test]
fn same_scan_loop_reads_current_index_and_current_source_each_visit() {
    let p =
        compile_il("LD X0\nMOV K0 E0\nFOR K3\nDEXP D10E0 D100E0\nADD E0 K2 E0\nNEXT\nEND").unwrap();
    let mut sim = enabled();
    for (address, bits) in [("D10", 0), ("D12", 0x3f80_0000), ("D14", 0xbf80_0000)] {
        put(&mut sim.memory, address, bits);
    }
    run(&mut sim, &p);
    assert_eq!(
        ["D100", "D102", "D104"].map(|address| get(&sim.memory, address)),
        [0x3f80_0000, 0x402d_f854, 0x3ebc_5ab2]
    );
    assert_eq!(sim.memory.word(d("D1028")), 6);
}

#[test]
fn effective_model_pairs_are_checked_after_signed_index_addition() {
    for last in [4999, 9999] {
        let action = parse(&format!("DEXP D{last}E0 D{last}F0"));
        let mut m = Memory::default();
        m.data_register_limit = Some(last);
        m.set_word(d("D1028"), -1);
        m.set_word(d("D1029"), -1);
        action.execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(get(&m, &format!("D{}", last - 1)), 0x3f80_0000);
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
    assert!(parse("DLN D4999 D20E0").resolved_source(&m).is_err());
    assert!(
        parse("DEXP K0 D4999E0")
            .prepare(&m, &mut BTreeSet::new())
            .is_err()
    );
}

#[test]
fn off_held_pulse_and_rearm_apply_before_domain_or_address_checks() {
    for (name, bad, good, expected) in [
        ("DEXPP", 0x7fc0_0001, 0, 0x3f80_0000),
        ("DLNP", 0, 0x3f80_0000, 0),
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
        put(&mut sim.memory, "D0", bad);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &p);
        assert_eq!(flags(&sim.memory), [true; 3]);
        sim.memory.set_word(d("D1028"), -30000);
        sim.observed_devices.clear();
        run(&mut sim, &p);
        assert!(sim.memory.execution_limitations.is_empty());
        assert!(!sim.observed_devices.contains(&d("D1028")));
        sim.memory.set_input(d("X0"), false);
        run(&mut sim, &p);
        sim.memory.set_word(d("D1028"), 0);
        put(&mut sim.memory, "D0", good);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &p);
        assert_eq!(get(&sim.memory, "D20"), expected);
        assert_eq!(flags(&sim.memory), [expected == 0, false, false]);
    }
}

#[test]
fn structural_failure_flags_and_held_result_monitoring_remain_distinct() {
    let action = parse("DLN D0E0 D100F0");
    let mut m = Memory::default();
    m.set_word(d("D1028"), 1);
    m.set_word(d("D1029"), 2);
    put(&mut m, "D1", 0x3f80_0000);
    let mut observed = BTreeSet::new();
    action.execute(&mut m, true, &mut observed);
    for address in ["D1", "D2", "D102", "D103"] {
        assert!(observed.contains(&d(address)));
    }
    assert_eq!(action.display_value(&m), "0");
    seed(&mut m);
    m.set_word(d("D1028"), -30000);
    observed.clear();
    action.execute(&mut m, true, &mut observed);
    assert_eq!(flags(&m), [true; 3]);
    assert!(!observed.contains(&d("D1029")));
    assert!(!m.system_bit(1067));
    let mut ui = BTreeSet::new();
    action.observed_devices(&m, &mut ui);
    assert!(ui.contains(&d("D102")));
    assert_eq!(action.display_value(&m), "0");
    m.set_word(d("D1028"), 1);
    m.set_word(d("D1029"), -30000);
    observed.clear();
    assert!(action.prepare(&m, &mut observed).is_err());
    assert_eq!(flags(&m), [true; 3]);
    action.execute(&mut m, true, &mut observed);
    assert_eq!(flags(&m), [true, false, false]);
    assert!(action.display_value(&m).starts_with("İndeks:"));
}

#[test]
fn ordinary_and_indexed_share_kernels_without_mode_or_public_scratch_changes() {
    for (name, input, expected) in [
        ("DEXP", 0x3f80_0000, 0x402d_f854),
        ("DLN", 0x4000_0000, 0x3f31_7218),
    ] {
        let mut ordinary = Memory::default();
        seed(&mut ordinary);
        put(&mut ordinary, "D0", input);
        ordinary.set_system(1081, true);
        ordinary.set_system(1018, true);
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
            assert_eq!(flags(m), [false; 3]);
            assert!(m.system_bit(1081) && m.system_bit(1018));
            assert_eq!([m.word(d("D5000")), m.word(d("D5001"))], [123, 456]);
        }
        assert!(
            !observed.contains(&d("M1081"))
                && !observed.contains(&d("M1018"))
                && !observed.contains(&d("D5000"))
        );
    }
}

#[test]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn indexed_kernel_and_error_paths_restore_the_callers_x87_control_word() {
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
    let mut m = Memory::default();
    put(&mut m, "D0", 0x3f80_0000);
    let success = parse("DEXP D0E0 D20F0");
    assert_eq!(success.semantic_error(&m).unwrap(), None);
    assert_eq!(read(), caller);
    assert!(success.prepare(&m, &mut BTreeSet::new()).is_ok());
    assert_eq!(read(), caller);
    success.execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(read(), caller);
    assert_eq!(get(&m, "D20"), 0x402d_f854);
    put(&mut m, "D0", 0xbf80_0000);
    let failure = parse("DLN D0E0 D20F0");
    assert_eq!(
        failure.semantic_error(&m).unwrap(),
        Some(FloatBinaryError {
            code: 0xe19,
            offset: 1
        })
    );
    assert_eq!(read(), caller);
    failure.execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(read(), caller);
    assert_eq!(m.word(d("D1067")), 0xe19);
}
