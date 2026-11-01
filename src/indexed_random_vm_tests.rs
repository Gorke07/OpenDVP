//! Independent numerical/address tests; DRAND tail nodes are
//! deliberately external. No native/local random stream equality is assumed.
use crate::float_math::FloatBinaryError;
use crate::indexed_random::{IndexedRandomAction, PreparedRandom};
use crate::plc::{Device, Memory, Program, ScanStatus, Simulator, compile_il};
use std::collections::BTreeSet;
fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn args(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(str::to_owned).collect()
}
fn action(raw: &str) -> IndexedRandomAction {
    let mut parts = raw.split_whitespace();
    IndexedRandomAction::parse(
        parts.next().unwrap(),
        &parts.map(str::to_owned).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn flags(m: &Memory) -> [bool; 3] {
    [1020, 1021, 1022].map(|n| m.system_bit(n))
}
fn seed_flags(m: &mut Memory) {
    for n in 1020..=1022 {
        m.set_system(n, true);
    }
}
fn group(m: &Memory, base: Device, width: u16) -> u32 {
    (0..width).fold(0, |v, n| {
        v | (u32::from(m.bit(Device {
            index: base.index + n,
            ..base
        })) << n)
    })
}
fn set_group(m: &mut Memory, base: Device, width: u16, value: u32) {
    for n in 0..width {
        let dev = Device {
            index: base.index + n,
            ..base
        };
        if base.area == crate::plc::Area::X {
            m.set_input(dev, value & (1 << n) != 0);
        } else {
            m.set_internal(dev, value & (1 << n) != 0);
        }
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
fn four_original_encodings_keep_actual_operand_tail_words_and_accept_f_targets() {
    for (name, header, wide) in [
        ("RAND", 0x5401, false),
        ("RANDP", 0x5411, false),
        ("DRAND", 0x5501, true),
        ("DRANDP", 0x5511, true),
    ] {
        let a = action(&format!("{name} D10E1 K7 D20F7"));
        let mut expected = vec![header, 0x14e6, 0x8086];
        if wide {
            expected.extend([0x0080, 0x0080]);
        }
        expected.extend([0x0780, 0x0080]);
        if wide {
            expected.extend([0x0080, 0x0080]);
        }
        expected.extend([0x28f6, 0x8089]);
        if wide {
            expected.extend([0x0080, 0x0080]);
        }
        assert_eq!(a.native_words().unwrap(), expected);
        assert_eq!(a.wide(), wide);
        assert_eq!(a.source_count(), 2);
        assert_eq!(a.label(), format!("{name} D10E1 K7 D20F7"));
    }
    for (target, word) in [
        ("D122F0", 0xf4f6),
        ("D123F0", 0xf6f6),
        ("D124F7", 0xf8f6),
        ("D125F0", 0xfaf6),
        ("D126F0", 0xfcf6),
        ("D127F0", 0xfef6),
        ("D125E7", 0xfae6),
        ("C255F7", 0xdcf6),
    ] {
        let a = action(&format!("DRAND K7 K7 {target}"));
        assert_eq!(a.native_words().unwrap().len(), 13);
        assert_eq!(a.native_words().unwrap()[9], word);
    }
    for (index, word) in [
        (246, 0xf6f4),
        (247, 0xf7f4),
        (248, 0xf8f4),
        (249, 0xf9f4),
        (250, 0xfaf4),
        (251, 0xfbf4),
        (252, 0xfcf4),
        (253, 0xfdf4),
    ] {
        assert_eq!(
            action(&format!("DRAND K7 K7 K1M{index}F0"))
                .native_words()
                .unwrap()[9],
            word
        );
    }
    // These are original emitted operands, not assertions that a false path
    // treats colliding control words as inert. Root owns their plan semantics.
}

#[test]
fn gray_roles_and_direct_wide_f_stay_instruction_specific() {
    for name in ["RAND", "RANDP", "DRAND", "DRANDP"] {
        for raw in [
            "K0 K1 D20E0",
            "H7 H7 K1Y20F7",
            "K1X0E0 D0F0 K2M80E1",
            "E1 F7 D20E0",
            "D0E0 D2F0 F1",
            "T255E0 C199F0 S80",
        ] {
            let result = IndexedRandomAction::parse(name, &args(raw));
            if raw.ends_with("S80") {
                assert!(result.is_err());
            } else {
                assert!(result.is_ok(), "{name} {raw}");
            }
        }
        for raw in [
            "K0 K1 K1X0E0",
            "K0 K1 M80E0",
            "K0 K1 X0",
            "K0 K1 K1",
            "K0 K1 K9M0E0",
            "D0E8 K1 D20",
            "E0E1 K1 D20",
            "K0 K1 F1E0",
        ] {
            assert!(IndexedRandomAction::parse(name, &args(raw)).is_err());
        }
        assert_eq!(
            IndexedRandomAction::parse(name, &args("C200E0 K7 D20F0")).is_ok(),
            name.starts_with('D')
        );
    }
    assert!(!IndexedRandomAction::supports("DRANDPP"));
    assert!(!IndexedRandomAction::supports("MVM"));
}

#[test]
fn every_index_bank_uses_signed_word_for_both_bounds_and_target() {
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
        for name in ["RAND", "DRAND"] {
            let mut m = Memory::default();
            m.set_word(d(alias), -2);
            let next = Device {
                index: d(alias).index + 1,
                ..d(alias)
            };
            m.set_word(next, i16::MIN);
            m.set_double_word(d("D8"), 7);
            m.set_double_word(d("D10"), 7);
            let a = action(&format!("{name} D10{suffix} D12{suffix} D20{suffix}"));
            let mut observed = BTreeSet::new();
            a.execute(&mut m, true, &mut observed);
            assert_eq!(m.double_word(d("D18")), 7);
            assert_eq!(m.random_state, Some(301));
            assert_eq!(a.modifier_devices(), vec![d(alias)]);
            assert!(!observed.contains(&next));
        }
    }
}

#[test]
fn negative_bounds_read_both_but_do_not_initialize_seed_or_touch_target_index() {
    for (name, offset) in [("RAND", 3), ("DRAND", 5)] {
        for (lower, upper) in [(-1, 7), (7, -1)] {
            for initial in [None, Some(0xffff)] {
                let mut m = Memory::default();
                m.random_state = initial;
                m.set_double_word(d("D0"), lower);
                m.set_double_word(d("D2"), upper);
                m.set_word(d("D1195"), -30000);
                m.set_word(d("D20"), 123);
                seed_flags(&mut m);
                let a = action(&format!("{name} D0E0 D2E1 D20F7"));
                let mut observed = BTreeSet::new();
                assert_eq!(
                    a.semantic_error(&m).unwrap(),
                    Some(FloatBinaryError {
                        code: 0xe1a,
                        offset
                    })
                );
                assert_eq!(
                    a.prepare(&m, &mut observed).unwrap(),
                    PreparedRandom::SemanticError {
                        code: 0xe1a,
                        offset
                    }
                );
                a.execute(&mut m, true, &mut observed);
                assert_eq!(m.random_state, initial);
                assert_eq!(m.word(d("D20")), 123);
                assert_eq!(
                    [m.word(d("D1067")), m.word(d("D1068"))],
                    [0xe1a, offset as i16]
                );
                assert_eq!(flags(&m), [true; 3]);
                assert!(observed.contains(&d("D1182")) && observed.contains(&d("D2")));
                assert!(!observed.contains(&d("D1195")));
                assert!(m.execution_limitations.is_empty());
            }
        }
    }
}

#[test]
fn reversed_nonnegative_bounds_update_state_before_error_without_target_access() {
    for (name, offset, initial, expected) in [
        ("RAND", 3, None, 301),
        ("DRAND", 5, None, 301),
        ("RAND", 3, Some(0x8000), 0),
        ("DRAND", 5, Some(0x8000), 0x8000),
        ("RAND", 3, Some(0xffff), 99),
        ("DRAND", 5, Some(0xffff), 99),
    ] {
        let mut m = Memory::default();
        m.random_state = initial;
        m.set_word(d("D1195"), -30000);
        seed_flags(&mut m);
        let a = action(&format!("{name} K10 K1 D20F7"));
        let mut observed = BTreeSet::new();
        a.prepare(&m, &mut observed).unwrap();
        assert_eq!(m.random_state, initial);
        assert!(!m.system_bit(1067));
        a.execute(&mut m, true, &mut observed);
        assert_eq!(m.random_state, Some(expected));
        assert_eq!(m.word(d("D1068")), offset);
        assert!(!observed.contains(&d("D1195")));
        assert_eq!(flags(&m), [true; 3]);
        assert!(m.execution_limitations.is_empty());
    }
}

#[test]
fn preflight_is_pure_but_invalid_target_execution_keeps_sampled_state() {
    for name in ["RAND", "DRAND"] {
        let a = action(&format!("{name} K7 K7 D20E0"));
        let mut m = Memory::default();
        let mut observed = BTreeSet::new();
        assert!(matches!(
            a.prepare(&m, &mut observed).unwrap(),
            PreparedRandom::Ready(_)
        ));
        assert_eq!(m.random_state, None);
        assert_eq!(m.word(d("D20")), 0);
        assert!(!m.system_bit(1067));
        m.set_word(d("D1028"), -30000);
        assert!(a.prepare(&m, &mut BTreeSet::new()).is_err());
        assert_eq!(m.random_state, None);
        seed_flags(&mut m);
        observed.clear();
        a.execute(&mut m, true, &mut observed);
        assert_eq!(m.random_state, Some(301));
        assert!(!m.execution_limitations.is_empty());
        assert!(!m.system_bit(1067));
        assert_eq!(flags(&m), [true; 3]);
        assert_eq!(observed, BTreeSet::from([d("D1028")]));
    }
}

#[test]
fn equal_bounds_write_exact_narrow_bits_with_neighbor_guards() {
    for (name, max_digits) in [("RAND", 4), ("DRAND", 8)] {
        for base in ["Y20", "M80", "S80"] {
            for digits in 1..=max_digits {
                let mut m = Memory::default();
                let original = d(base);
                let effective = Device {
                    index: original.index + 1,
                    ..original
                };
                m.set_word(d("D1028"), 1);
                m.set_internal(original, true);
                m.set_internal(
                    Device {
                        index: effective.index + 4 * digits,
                        ..effective
                    },
                    true,
                );
                let a = action(&format!("{name} K31 K31 K{digits}{base}E0"));
                a.execute(&mut m, true, &mut BTreeSet::new());
                assert_eq!(
                    group(&m, effective, 4 * digits),
                    if digits == 1 { 15 } else { 31 }
                );
                assert!(m.bit(original));
                assert!(m.bit(Device {
                    index: effective.index + 4 * digits,
                    ..effective
                }));
                assert_eq!(m.random_state, Some(301));
            }
        }
    }
}

#[test]
fn short_group_bounds_zero_extend_and_full_word_sign_is_preserved() {
    let mut m = Memory::default();
    set_group(&mut m, d("X0"), 4, 15);
    action("DRAND K1X0E0 K1X0E0 D20F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.double_word(d("D20")), 15);
    let mut m = Memory::default();
    set_group(&mut m, d("M80"), 16, 0xffff);
    action("RAND K4M80E0 K4M80E0 D20F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.random_state, None);
    assert_eq!(m.word(d("D1067")), 0xe1a);
    action("DRAND K4M80E0 K4M80E0 D20F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.double_word(d("D20")), 65535);
    assert_eq!(m.random_state, Some(301));
}

#[test]
fn direct_wide_f_all_banks_remain_sources_and_writable_canonical_pairs() {
    for (index, low, word0, word1) in [
        (0, 1029, 0x3a86, 0x0080),
        (1, 1183, 0x6e86, 0x0180),
        (2, 1185, 0x7286, 0x0180),
        (3, 1187, 0x7686, 0x0180),
        (4, 1189, 0x7a86, 0x0180),
        (5, 1191, 0x7e86, 0x0180),
        (6, 1193, 0x8286, 0x0180),
        (7, 1195, 0x8686, 0x0180),
    ] {
        let mut m = Memory::default();
        let alias = d(&format!("D{low}"));
        m.set_double_word(alias, 0x12345678);
        let a = action(&format!("DRAND F{index} F{index} D20E0"));
        assert_eq!(
            a.native_words().unwrap()[1..5],
            [word0, word1, 0x0080, 0x0080]
        );
        let mut observed = BTreeSet::new();
        a.execute(&mut m, true, &mut observed);
        assert_eq!(m.double_word(d("D20")), 0x12345678);
        assert!(
            observed.contains(&alias)
                && observed.contains(&Device {
                    index: low + 1,
                    ..alias
                })
        );
        m.set_double_word(d("D0"), 42);
        action(&format!("DRAND D0E0 K42 F{index}")).execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(m.double_word(alias), 42);
    }
}

#[test]
fn counter_natural_width_pair_endpoints_and_own_index_writes_are_cached() {
    let mut m = Memory::default();
    m.set_word(d("C199"), -1);
    m.set_double_word(d("C200"), 0x12345678);
    m.set_word(d("D1028"), -1);
    let mut observed = BTreeSet::new();
    action("DRAND C200E0 C200E0 D20F0").execute(&mut m, true, &mut observed);
    assert_eq!(m.double_word(d("D20")), 65535);
    assert!(!observed.contains(&d("C200")));
    m.set_word(d("D1028"), 1);
    action("DRAND C199E0 C199E0 C254E0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.double_word(d("C255")), 0x12345678);
    m.set_word(d("D1028"), 0);
    m.set_double_word(d("C200"), 0x12345678);
    action("DRAND K7 K7 C199E0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.word(d("C199")), 7);
    assert_eq!(m.double_word(d("C200")), 0x12345678);
    let mut m = Memory::default();
    m.set_word(d("D1028"), 1);
    m.set_word(d("D1030"), 88);
    action("DRAND K305419896 K305419896 D1027E0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.double_word(d("D1028")), 0x12345678);
    assert_eq!(m.word(d("D1030")), 88);
    let mut m = Memory::default();
    m.set_double_word(d("D0"), 7);
    action("DRAND D0E0 D0E0 D1F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.double_word(d("D1")), 7);
    assert_eq!(m.word(d("D0")), 7);
}

#[test]
fn pulse_off_held_and_rearm_gates_numerical_rng_and_operand_observation() {
    for name in ["RANDP", "DRANDP"] {
        let p = compile_il(&format!("LD X0\n{name} K7 K7 D20E0\nEND")).unwrap();
        let mut sim = Simulator::default();
        sim.memory.set_word(d("D1028"), -30000);
        run(&mut sim, &p);
        assert_eq!(sim.memory.random_state, None);
        assert!(!sim.observed_devices.contains(&d("D1028")));
        sim.memory.set_word(d("D1028"), 0);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &p);
        assert_eq!(sim.memory.random_state, Some(301));
        assert_eq!(sim.memory.word(d("D20")), 7);
        sim.memory.set_word(d("D1028"), -30000);
        sim.observed_devices.clear();
        run(&mut sim, &p);
        assert_eq!(sim.memory.random_state, Some(301));
        assert!(!sim.observed_devices.contains(&d("D1028")));
        sim.memory.set_input(d("X0"), false);
        run(&mut sim, &p);
        sim.memory.set_word(d("D1028"), 2);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &p);
        assert_eq!(sim.memory.random_state, Some(601));
        assert_eq!(sim.memory.word(d("D22")), 7);
        assert!(sim.memory.execution_limitations.is_empty());
    }
}

#[test]
fn local_seed_vectors_and_non_equal_results_respect_independent_stream_contract() {
    let mut m = Memory::default();
    let a = action("RAND K0 K10 D20E0");
    for (expected, state) in [(6, 301), (7, 601), (0, 901), (8, 1201)] {
        a.execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(m.word(d("D20")), expected);
        assert_eq!(m.random_state, Some(state));
    }
    for (name, upper) in [("RAND", 32767), ("DRAND", i32::MAX)] {
        for initial in [0, 1, 2, 3, 0x7fff, 0x8000, 0xffff] {
            let mut m = Memory::default();
            m.random_state = Some(initial);
            let a = action(&format!("{name} K0 K{upper} D20E0"));
            for _ in 0..32 {
                a.execute(&mut m, true, &mut BTreeSet::new());
                let value = if name == "RAND" {
                    i32::from(m.word(d("D20")))
                } else {
                    m.double_word(d("D20"))
                };
                assert!((0..=upper).contains(&value));
                assert!(!m.system_bit(1067));
            }
        }
    }
}

#[test]
fn source_order_model_qualification_and_monitoring_do_not_mutate_preflight_state() {
    let a = action("DRAND D0E0 D2E1 D20F7");
    let mut m = Memory::default();
    m.set_double_word(d("D0"), -1);
    m.set_word(d("D1182"), -30000);
    let mut observed = BTreeSet::new();
    a.execute(&mut m, true, &mut observed);
    assert!(observed.contains(&d("D1182")));
    assert!(!observed.contains(&d("D1195")));
    assert_eq!(m.random_state, None);
    assert!(!m.system_bit(1067));
    assert!(!m.execution_limitations.is_empty());
    for last in [4999, 9999] {
        let mut m = Memory::default();
        m.data_register_limit = Some(last);
        m.set_word(d("D1028"), -1);
        let a = action(&format!("DRAND D{last}E0 D{last}E0 D20F0"));
        assert!(a.prepare(&m, &mut BTreeSet::new()).is_ok());
        m.set_word(d("D1028"), 0);
        assert!(a.prepare(&m, &mut BTreeSet::new()).is_err());
        assert_eq!(m.random_state, None);
    }
    let mut m = Memory::default();
    m.set_word(d("D1028"), -30000);
    m.set_word(d("D1195"), 2);
    m.set_double_word(d("D22"), 99);
    let a = action("DRAND D0E0 K7 D20F7");
    let mut ui = BTreeSet::new();
    a.observed_devices(&m, &mut ui);
    assert!(ui.contains(&d("D22")) && ui.contains(&d("D23")));
    assert_eq!(a.display_value(&m), "99");
    for target in ["K1M1284E0", "K2M1532E0", "K2S1020E0", "K1Y377E0"] {
        assert!(
            action(&format!("DRAND K7 K7 {target}"))
                .prepare(&Memory::default(), &mut BTreeSet::new())
                .is_err()
        );
    }
}

#[test]
fn error_pc_sticky_recovery_flags_and_explicit_target_flag_alias_are_distinct() {
    for (name, offset) in [("RAND", 3), ("DRAND", 5)] {
        let p = compile_il(&format!(
            "LD X0\nMOV K7 D50\n{name} D0E0 K7 D20F0\nOUT Y0\nEND"
        ))
        .unwrap();
        let native = crate::es2_encoder::encode(&p).unwrap();
        let mut sim = enabled();
        sim.memory.set_double_word(d("D0"), -1);
        seed_flags(&mut sim.memory);
        run(&mut sim, &p);
        assert_eq!(
            sim.memory.word(d("D1068")) as u16,
            native.action_pc[1] + offset
        );
        assert_eq!(sim.memory.random_state, None);
        sim.memory.set_double_word(d("D0"), 7);
        run(&mut sim, &p);
        assert_eq!(sim.memory.word(d("D20")), 7);
        assert_eq!(sim.memory.random_state, Some(301));
        assert_eq!(sim.memory.word(d("D1067")), 0xe1a);
        assert_eq!(flags(&sim.memory), [true; 3]);
        assert!(sim.memory.bit(d("Y0")));
    }
    let mut m = Memory::default();
    seed_flags(&mut m);
    action("RAND K10 K10 K1M1019E0").execute(&mut m, true, &mut BTreeSet::new());
    assert!(!m.system_bit(1019));
    assert_eq!(flags(&m), [true, false, true]);
    assert_eq!(m.random_state, Some(301));
}
