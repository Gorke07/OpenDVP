//! Independent operand-stage, descriptor and numerical expectations for
//! indexed DLOG/DPOW. Exceptional native responsiveness is qualified separately.
use crate::float_advanced::AdvancedAction;
use crate::float_math::FloatBinaryError;
use crate::indexed_log_pow::{IndexedLogPowAction, PreparedLogPow};
use crate::plc::{Device, Memory, Program, ScanStatus, Simulator, compile_il};
use std::collections::BTreeSet;
fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn parse(raw: &str) -> IndexedLogPowAction {
    let mut parts = raw.split_whitespace();
    IndexedLogPowAction::parse(
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
fn four_forms_encode_thirteen_words_in_base_right_result_order() {
    for (name, header, right, expected) in [
        ("DLOG", 0x1d01, 0x42fa_0000, 0x4040_0000),
        ("DLOGP", 0x1d11, 0x42fa_0000, 0x4040_0000),
        ("DPOW", 0x2101, 0x4040_0000, 0x42fa_0000),
        ("DPOWP", 0x2111, 0x4040_0000, 0x42fa_0000),
    ] {
        let body = format!("{name} D10E1 D20F0 D100E7");
        let action = parse(&body);
        let words = [
            header, 0x14e6, 0x8086, 0x0080, 0x0080, 0x28f6, 0x0086, 0x0080, 0x0080, 0xc8e6, 0x8089,
            0x0080, 0x0080,
        ];
        assert_eq!(action.native_words().unwrap(), words);
        assert_eq!(action.label(), body);
        assert_eq!(action.dest(), d("D100"));
        let p = compile_il(&format!("LD X0\n{body}\nMOV K7 D300\nOUT Y0\nEND")).unwrap();
        assert!(p.requires_vm());
        let encoded = crate::es2_encoder::encode(&p).unwrap();
        assert_eq!(&encoded.words[1..14], &words);
        assert_eq!(encoded.action_pc[1], 14);
        let mut sim = enabled();
        sim.memory.set_word(d("D1182"), 1);
        sim.memory.set_word(d("D1029"), 2);
        sim.memory.set_word(d("D1194"), 3);
        put(&mut sim.memory, "D11", 0x40a0_0000);
        put(&mut sim.memory, "D22", right);
        sim.memory.set_word(d("D102"), 77);
        sim.memory.set_word(d("D105"), 88);
        seed(&mut sim.memory);
        run(&mut sim, &p);
        assert_eq!(get(&sim.memory, "D103"), expected);
        assert_eq!(flags(&sim.memory), [false; 3]);
        assert_eq!(
            [sim.memory.word(d("D102")), sim.memory.word(d("D105"))],
            [77, 88]
        );
        assert_eq!(sim.memory.word(d("D300")), 7);
        assert!(sim.memory.bit(d("Y0")));
    }
}

#[test]
fn all_sixteen_index_aliases_resolve_each_of_three_roles_as_signed_words() {
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
        let action = parse(&format!("DPOW D10{suffix} D20{suffix} D100{suffix}"));
        let mut m = Memory::default();
        m.set_word(d(alias), -2);
        let next = Device {
            index: d(alias).index + 1,
            ..d(alias)
        };
        m.set_word(next, i16::MIN);
        put(&mut m, "D8", 0x4000_0000);
        put(&mut m, "D18", 0xc040_0000);
        let mut observed = BTreeSet::new();
        action.execute(&mut m, true, &mut observed);
        assert_eq!(get(&m, "D98"), 0x3e00_0000, "{suffix}");
        assert_eq!(action.modifier_devices(), vec![d(alias)]);
        assert!(observed.contains(&d(alias)) && !observed.contains(&next));
    }
}

#[test]
fn role_matrix_allows_integer_or_d_sources_and_only_d_targets() {
    for name in ["DLOG", "DLOGP", "DPOW", "DPOWP"] {
        for args in [
            ["K5", "H7D", "D20E0"],
            ["D0E0", "K-3", "D20"],
            ["K2", "D10F7", "D20E1"],
        ] {
            assert!(IndexedLogPowAction::parse(name, &args.map(str::to_owned)).is_ok());
        }
        for bad in ["F1", "E1", "T0E0", "C200F0", "K8M0E0", "D0E8", "D9999"] {
            assert!(
                IndexedLogPowAction::parse(name, &[bad.into(), "K3".into(), "D20E0".into()])
                    .is_err()
            );
            assert!(
                IndexedLogPowAction::parse(name, &["K5".into(), bad.into(), "D20E0".into()])
                    .is_err()
            );
            assert!(
                IndexedLogPowAction::parse(name, &["D0E0".into(), "K3".into(), bad.into()])
                    .is_err()
            );
        }
        assert!(
            IndexedLogPowAction::parse(name, &["D0E0".into(), "K3".into(), "K20".into()]).is_err()
        );
    }
    for name in [
        "LOG", "POW", "DLOGPP", "DPOWPP", "DEXP", "DLN", "DESQR", "DSIN",
    ] {
        assert!(!IndexedLogPowAction::supports(name));
    }
    assert_eq!(
        parse("DPOW H2 K-3 D20F7").native_words().unwrap(),
        [
            0x2101, 0x0280, 0x0080, 0x0080, 0x0080, 0xfd80, 0xff80, 0xff80, 0xff80, 0x28f6, 0x8089,
            0x0080, 0x0080
        ]
    );
}

#[test]
fn constants_round_to_binary32_and_keep_raw_pair_interpretation_distinct() {
    for (body, expected) in [
        ("DLOG K5 K125 D20E0", 0x4040_0000),
        ("DPOW K5 K3 D20E0", 0x42fa_0000),
        ("DLOG K2 K16777217 D20E0", 0x41c0_0000),
        ("DPOW K16777217 K1 D20E0", 0x4b80_0000),
        ("DPOW K2 H80000000 D20E0", 0),
    ] {
        let mut m = Memory::default();
        parse(body).execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(get(&m, "D20"), expected, "{body}");
    }
    let mut m = Memory::default();
    put(&mut m, "D0", 0x4000_0000);
    put(&mut m, "D10", 0x8000_0000);
    parse("DPOW D0E0 D10F0 D20E1").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(get(&m, "D20"), 0x3f80_0000);
    assert!(
        parse("DLOG K1 D10F0 D20E1")
            .semantic_error(&m)
            .unwrap()
            .is_some()
    );
}

#[test]
fn literal_kernel_vectors_preserve_integer_general_and_rounded_log_paths() {
    for (name, left, right, expected) in [
        // Separate rounded log10 values produce 0x40257006. A single
        // high-precision log ratio would incorrectly give 0x40257007.
        ("DLOG", 0x4000_0000, 0x40c0_0000, 0x4025_7006),
        ("DLOG", 0x3f00_0000, 0x4100_0000, 0xc040_0000),
        ("DLOG", 0x4040_0000, 0x40e0_0000, 0x3fe2_b81d),
        ("DLOG", 0x4120_0000, 0x3f80_0000, 0),
        ("DPOW", 0x4000_0000, 0xc040_0000, 0x3e00_0000),
        ("DPOW", 0x4110_0000, 0x3f00_0000, 0x4040_0000),
        ("DPOW", 0x4000_0000, 0x3f00_0000, 0x3fb5_04f3),
        ("DPOW", 0x3fa0_0000, 0x4060_0000, 0x400b_c116),
        ("DPOW", 0x3f80_0347, 0x4150_0000, 0x3f80_2aa2),
    ] {
        let mut m = Memory::default();
        put(&mut m, "D0", left);
        put(&mut m, "D10", right);
        seed(&mut m);
        parse(&format!("{name} D0E0 D10F0 D20E1")).execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(get(&m, "D20"), expected, "{name} {left:08x} {right:08x}");
        assert_eq!(flags(&m), [expected == 0, false, false]);
    }
}

#[test]
fn invalid_base_skips_both_right_and_target_modifiers() {
    for (name, left) in [
        ("DLOG", 0),
        ("DLOG", 0x8000_0000),
        ("DLOG", 0x3f80_0000),
        ("DLOG", 0x7fc0_0001),
        ("DPOW", 0),
        ("DPOW", 0xbf80_0000),
        ("DPOW", 0x7f80_0001),
    ] {
        let action = parse(&format!("{name} D0E0 D10F0 D20F7"));
        let mut m = Memory::default();
        put(&mut m, "D0", left);
        put(&mut m, "D20", 0x1234_5678);
        m.set_word(d("D1029"), -30000);
        m.set_word(d("D1195"), -30000);
        seed(&mut m);
        assert!(!action.needs_right_source(&m).unwrap());
        assert_eq!(
            action.semantic_error(&m).unwrap(),
            Some(FloatBinaryError {
                code: 0xe19,
                offset: 1
            })
        );
        let mut observed = BTreeSet::new();
        assert_eq!(
            action.prepare(&m, &mut observed).unwrap(),
            PreparedLogPow::SemanticError {
                code: 0xe19,
                offset: 1
            }
        );
        assert!(!m.system_bit(1067));
        action.execute(&mut m, true, &mut observed);
        assert_eq!(flags(&m), [true; 3]);
        assert_eq!([m.word(d("D1067")), m.word(d("D1068"))], [0xe19, 1]);
        assert_eq!(get(&m, "D20"), 0x1234_5678);
        assert!(!observed.contains(&d("D1029")) && !observed.contains(&d("D1195")));
        assert!(m.execution_limitations.is_empty());
    }
}

#[test]
fn log_argument_errors_visit_right_but_skip_the_target() {
    let action = parse("DLOG D0E0 D10F0 D20F7");
    for right in [0, 0x8000_0000, 0xbf80_0000, 0x7fc0_0001, 0xff80_0000] {
        let mut m = Memory::default();
        put(&mut m, "D0", 0x4000_0000);
        put(&mut m, "D10", right);
        put(&mut m, "D20", 0x1234_5678);
        m.set_word(d("D1195"), -30000);
        seed(&mut m);
        assert!(action.needs_right_source(&m).unwrap());
        assert_eq!(
            action.semantic_error(&m).unwrap(),
            Some(FloatBinaryError {
                code: 0xe19,
                offset: 5
            })
        );
        let mut observed = BTreeSet::new();
        action.execute(&mut m, true, &mut observed);
        assert_eq!([m.word(d("D1067")), m.word(d("D1068"))], [0xe19, 5]);
        assert_eq!(flags(&m), [true; 3]);
        assert!(observed.contains(&d("D1029")) && observed.contains(&d("D11")));
        assert!(!observed.contains(&d("D1195")));
        assert_eq!(get(&m, "D20"), 0x1234_5678);
    }
}

#[test]
fn pow_exponent_does_not_inherit_log_argument_validation() {
    for (left, right, expected, state) in [
        (0x4000_0000, 0, 0x3f80_0000, [false; 3]),
        (0x4000_0000, 0x8000_0000, 0x3f80_0000, [false; 3]),
        (0x4000_0000, 0xc040_0000, 0x3e00_0000, [false; 3]),
        (0x4000_0000, 0x7fc0_0001, 0x7f80_0000, [false, false, true]),
        (0x4000_0000, 0xff80_0000, 0x7f80_0000, [false, false, true]),
        (0x7f80_0000, 0x8000_0000, 0, [true, false, false]),
    ] {
        let action = parse("DPOW D0E0 D10F0 D20E1");
        let mut m = Memory::default();
        put(&mut m, "D0", left);
        put(&mut m, "D10", right);
        seed(&mut m);
        assert_eq!(action.semantic_error(&m).unwrap(), None);
        action.execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(get(&m, "D20"), expected);
        assert_eq!(flags(&m), state);
        assert!(!m.system_bit(1067));
    }
}

#[test]
fn actual_error_pc_and_sticky_state_do_not_suppress_successful_recovery() {
    for (left, right, offset) in [(0, 0x4000_0000, 1), (0x4000_0000, 0, 5)] {
        let p = compile_il("LD X0\nMOV K7 D50\nDLOG D0E0 D10F0 D20E1\nOUT Y0\nEND").unwrap();
        let encoded = crate::es2_encoder::encode(&p).unwrap();
        let mut sim = enabled();
        put(&mut sim.memory, "D0", left);
        put(&mut sim.memory, "D10", right);
        seed(&mut sim.memory);
        run(&mut sim, &p);
        assert_eq!(
            sim.memory.word(d("D1068")) as u16,
            encoded.action_pc[1] + offset
        );
        assert_eq!(flags(&sim.memory), [true; 3]);
        put(&mut sim.memory, "D0", 0x4000_0000);
        put(&mut sim.memory, "D10", 0x4100_0000);
        run(&mut sim, &p);
        assert_eq!(get(&sim.memory, "D20"), 0x4040_0000);
        assert_eq!(flags(&sim.memory), [false; 3]);
        assert_eq!(sim.memory.word(d("D1067")), 0xe19);
        assert!(sim.memory.bit(d("Y0")));
    }
}

#[test]
fn alias_inputs_and_self_indexed_outputs_use_snapshots_and_fixed_pairs() {
    let mut m = Memory::default();
    put(&mut m, "D0", 0x4000_0000);
    put(&mut m, "D2", 0x4040_0000);
    parse("DPOW D0E0 D2F0 D1E1").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(get(&m, "D1"), 0x4100_0000);
    assert_eq!(m.word(d("D0")), 0);
    assert_eq!(m.word(d("D3")) as u16, 0x4040);
    let mut m = Memory::default();
    m.set_word(d("D1028"), 1);
    m.set_word(d("D1030"), 77);
    parse("DPOW K2 K-3 D1027E0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.word(d("D1028")), 0);
    assert_eq!(m.word(d("D1029")) as u16, 0x3e00);
    assert_eq!(m.word(d("D1030")), 77);
}

#[test]
fn model_endpoints_and_same_scan_index_changes_are_live() {
    for last in [4999, 9999] {
        let action = parse(&format!("DPOW D{last}E0 D{last}F0 D{last}E1"));
        let mut m = Memory::default();
        m.data_register_limit = Some(last);
        for alias in ["D1028", "D1029", "D1182"] {
            m.set_word(d(alias), -1);
        }
        put(&mut m, &format!("D{}", last - 1), 0x4000_0000);
        action.execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(get(&m, &format!("D{}", last - 1)), 0x4080_0000);
        m.set_word(d("D1028"), 0);
        assert!(action.resolved_source(0, &m).is_err());
        m.set_word(d("D1028"), -1);
        m.set_word(d("D1029"), 0);
        assert!(action.resolved_source(1, &m).is_err());
        m.set_word(d("D1029"), -1);
        m.set_word(d("D1182"), 0);
        assert!(action.prepare(&m, &mut BTreeSet::new()).is_err());
    }
    let p = compile_il("LD X0\nMOV K0 E0\nFOR K3\nDPOW K2 D10E0 D100E0\nADD E0 K2 E0\nNEXT\nEND")
        .unwrap();
    let mut sim = enabled();
    for (address, raw) in [("D10", 0), ("D12", 0x3f80_0000), ("D14", 0x4000_0000)] {
        put(&mut sim.memory, address, raw);
    }
    run(&mut sim, &p);
    assert_eq!(
        ["D100", "D102", "D104"].map(|a| get(&sim.memory, a)),
        [0x3f80_0000, 0x4000_0000, 0x4080_0000]
    );
}

#[test]
fn off_and_held_pulse_gate_every_address_and_domain_stage() {
    for (name, good_right, expected) in [
        ("DLOGP", 0x4100_0000, 0x4040_0000),
        ("DPOWP", 0x4040_0000, 0x4100_0000),
    ] {
        let p = compile_il(&format!("LD X0\n{name} D0E0 D10F0 D20E1\nEND")).unwrap();
        let mut sim = Simulator::default();
        seed(&mut sim.memory);
        sim.memory.set_word(d("D1028"), -30000);
        run(&mut sim, &p);
        assert_eq!(flags(&sim.memory), [true; 3]);
        assert!(sim.memory.execution_limitations.is_empty());
        sim.memory.set_word(d("D1028"), 0);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &p);
        assert_eq!(sim.memory.word(d("D1067")), 0xe19);
        sim.memory.set_word(d("D1028"), -30000);
        sim.observed_devices.clear();
        run(&mut sim, &p);
        assert!(!sim.observed_devices.contains(&d("D1028")));
        assert!(sim.memory.execution_limitations.is_empty());
        sim.memory.set_input(d("X0"), false);
        run(&mut sim, &p);
        sim.memory.set_word(d("D1028"), 0);
        put(&mut sim.memory, "D0", 0x4000_0000);
        put(&mut sim.memory, "D10", good_right);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &p);
        assert_eq!(get(&sim.memory, "D20"), expected);
        assert_eq!(flags(&sim.memory), [false; 3]);
    }
}

#[test]
fn observation_is_staged_but_monitoring_resolves_held_right_and_result() {
    let action = parse("DLOG D0E0 D10F0 D100E1");
    let mut m = Memory::default();
    m.set_word(d("D1028"), 1);
    m.set_word(d("D1029"), 2);
    m.set_word(d("D1182"), 3);
    put(&mut m, "D1", 0x4000_0000);
    put(&mut m, "D12", 0x4100_0000);
    let mut observed = BTreeSet::new();
    action.execute(&mut m, true, &mut observed);
    for address in ["D1", "D2", "D12", "D13", "D103", "D104"] {
        assert!(observed.contains(&d(address)));
    }
    assert_eq!(action.display_value(&m), "3");
    put(&mut m, "D1", 0);
    observed.clear();
    action.execute(&mut m, true, &mut observed);
    assert!(!observed.contains(&d("D1029")) && !observed.contains(&d("D1182")));
    let mut ui = BTreeSet::new();
    action.observed_devices(&m, &mut ui);
    assert!(ui.contains(&d("D12")) && ui.contains(&d("D103")));
    assert_eq!(action.display_value(&m), "3");
    put(&mut m, "D1", 0x4000_0000);
    m.set_word(d("D1029"), -30000);
    seed(&mut m);
    observed.clear();
    action.execute(&mut m, true, &mut observed);
    assert!(!m.execution_limitations.is_empty());
    assert_eq!(flags(&m), [true; 3]);
    assert!(!observed.contains(&d("D1182")));
    m.set_word(d("D1029"), 2);
    put(&mut m, "D12", 0x3f80_0000);
    m.set_word(d("D1182"), -30000);
    action.execute(&mut m, true, &mut observed);
    assert_eq!(flags(&m), [true, false, false]);
    assert!(action.display_value(&m).starts_with("İndeks:"));
}

#[test]
fn ordinary_and_indexed_share_kernels_and_leave_unrelated_modes_and_scratch_alone() {
    for (name, right, expected) in [
        ("DLOG", 0x4100_0000, 0x4040_0000),
        ("DPOW", 0xc040_0000, 0x3e00_0000),
    ] {
        let mut ordinary = Memory::default();
        put(&mut ordinary, "D0", 0x4000_0000);
        put(&mut ordinary, "D10", right);
        ordinary.set_system(1081, true);
        ordinary.set_system(1018, true);
        ordinary.set_word(d("D5000"), 123);
        ordinary.set_word(d("D5001"), 456);
        let mut indexed = ordinary.clone();
        AdvancedAction::parse(name, &["D0".into(), "D10".into(), "D20".into()])
            .unwrap()
            .execute(&mut ordinary);
        let mut observed = BTreeSet::new();
        parse(&format!("{name} D0E0 D10F0 D20E1")).execute(&mut indexed, true, &mut observed);
        for m in [&ordinary, &indexed] {
            assert_eq!(get(m, "D20"), expected);
            assert_eq!(flags(m), [false; 3]);
            assert!(m.system_bit(1081) && m.system_bit(1018));
            assert_eq!([m.word(d("D5000")), m.word(d("D5001"))], [123, 456]);
        }
        assert!(!observed.contains(&d("M1081")) && !observed.contains(&d("M1018")));
    }
}

#[test]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn staged_validation_and_calculation_restore_the_callers_x87_control_word() {
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
    let action = parse("DLOG D0E0 D10F0 D20E1");
    let mut m = Memory::default();
    put(&mut m, "D0", 0x4000_0000);
    put(&mut m, "D10", 0x4100_0000);
    assert!(action.needs_right_source(&m).unwrap());
    assert_eq!(read(), caller);
    assert_eq!(action.semantic_error(&m).unwrap(), None);
    assert_eq!(read(), caller);
    action.execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(read(), caller);
    assert_eq!(get(&m, "D20"), 0x4040_0000);
    for (left, right) in [(0, 0x4100_0000), (0x4000_0000, 0)] {
        put(&mut m, "D0", left);
        put(&mut m, "D10", right);
        assert!(action.semantic_error(&m).unwrap().is_some());
        assert_eq!(read(), caller);
        action.execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(read(), caller);
    }
}
