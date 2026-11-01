//! Independent raw-bit, storage-width and visit tests for indexed DMOVR/P.
use crate::float_ops::FloatAction;
use crate::indexed_literal_move::IndexedLiteralMoveAction;
use crate::plc::{Device, Memory, Program, ScanStatus, Simulator, compile_il};
use std::collections::BTreeSet;

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn args(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(str::to_owned).collect()
}
fn action(raw: &str) -> IndexedLiteralMoveAction {
    let mut parts = raw.split_whitespace();
    IndexedLiteralMoveAction::parse(
        parts.next().unwrap(),
        &parts.map(str::to_owned).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn flags(m: &Memory) -> [bool; 3] {
    [1020, 1021, 1022].map(|n| m.system_bit(n))
}
fn seed(m: &mut Memory) {
    for n in 1020..=1022 {
        m.set_system(n, true);
    }
}
fn packed(m: &Memory, base: Device, width: u16) -> u32 {
    (0..width).fold(0u32, |bits, n| {
        bits | ((u32::from(m.bit(Device {
            index: base.index + n,
            ..base
        }))) << n)
    })
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
fn both_headers_literal_bits_and_explicit_kn_width_match_fixed_nine_word_layout() {
    for (name, header) in [("DMOVR", 0xf100), ("DMOVRP", 0xf110)] {
        for (target, descriptor) in [
            ("D20F7", [0x28f6, 0x8089, 0x0080, 0x0080]),
            ("K3M80E1", [0x50e4, 0x2886, 0x0080, 0x0080]),
            ("C199E1", [0x8ee6, 0x2184, 0x0080, 0x0080]),
            ("C200F7", [0x00f6, 0xe284, 0x0080, 0x0080]),
        ] {
            let body = format!("{name} F1.2 {target}");
            let a = action(&body);
            let mut expected = vec![header, 0x9a80, 0x9980, 0x9980, 0x3f80];
            expected.extend(descriptor);
            assert_eq!(a.native_words().unwrap(), expected);
            assert_eq!(a.label(), body);
            let p = compile_il(&format!("LD X0\n{body}\nMOV K7 D300\nOUT Y0\nEND")).unwrap();
            assert!(p.requires_vm());
            let native = crate::es2_encoder::encode(&p).unwrap();
            assert_eq!(&native.words[1..10], expected);
            assert_eq!(native.action_pc[1], 10);
            let mut sim = enabled();
            seed(&mut sim.memory);
            run(&mut sim, &p);
            assert_eq!(flags(&sim.memory), [true; 3]);
            assert_eq!(sim.memory.word(d("D300")), 7);
            assert!(sim.memory.bit(d("Y0")));
        }
    }
    assert_eq!(
        action("DMOVR F-0.0 D20E0").native_words().unwrap(),
        [
            0xf100, 0x0080, 0x0080, 0x0080, 0x8080, 0x28e6, 0x0086, 0x0080, 0x0080
        ]
    );
}

#[test]
fn finite_literal_grammar_and_destination_matrix_are_scoped() {
    for name in ["DMOVR", "DMOVRP"] {
        for raw in [
            "F1.200E+0 D20E0",
            "F-0.0 K8Y20F7",
            "F2 K1M80E1",
            "F3 K3S80F0",
            "F4 T255E0",
            "F5 C255F7",
            "F6 D9999E0",
        ] {
            assert!(IndexedLiteralMoveAction::parse(name, &args(raw)).is_ok());
        }
        for raw in [
            "D0 D20E0",
            "K1 D20E0",
            "H3F800000 D20E0",
            "E1 D20E0",
            "FNaN D20E0",
            "FInf D20E0",
            "F1E100 D20E0",
            "F1 E0",
            "F1 F1",
            "F1 X0E0",
            "F1 M0E0",
            "F1 K1X0E0",
            "F1 K0M0E0",
            "F1 K9M0E0",
            "F1 D0E8",
            "F1 K1",
            "F1 D9999",
            "F1 T255",
        ] {
            assert!(
                IndexedLiteralMoveAction::parse(name, &args(raw)).is_err(),
                "{raw}"
            );
        }
    }
    assert!(!IndexedLiteralMoveAction::supports("MOVR"));
    assert!(!IndexedLiteralMoveAction::supports("DMOVRPP"));
    assert!(!IndexedLiteralMoveAction::has_modifiers(&args(
        "F1.2E+0 D20"
    )));
    let a = action("DMOVRP F-0.000E+0 D20F7");
    assert_eq!(a.label(), "DMOVRP F-0.000E+0 D20F7");
    assert_eq!(
        a.native_words().unwrap()[1..5],
        [0x0080, 0x0080, 0x0080, 0x8080]
    );
}

#[test]
fn all_sixteen_modifiers_are_signed_word_offsets_with_unchanged_literal() {
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
        let mut m = Memory::default();
        m.set_word(d(alias), -1);
        let next = Device {
            index: d(alias).index + 1,
            ..d(alias)
        };
        m.set_word(next, i16::MIN);
        m.set_word(d("D8"), 77);
        m.set_word(d("D11"), 88);
        seed(&mut m);
        let a = action(&format!("DMOVR F1.2 D10{suffix}"));
        let mut observed = BTreeSet::new();
        a.execute(&mut m, true, &mut observed);
        assert_eq!(m.double_word(d("D9")) as u32, 0x3f99_999a, "{suffix}");
        assert_eq!([m.word(d("D8")), m.word(d("D11"))], [77, 88]);
        assert_eq!(a.modifier_devices(), vec![d(alias)]);
        assert!(observed.contains(&d(alias)) && !observed.contains(&next));
        assert_eq!(flags(&m), [true; 3]);
    }
}

#[test]
fn every_kn_width_writes_exact_low_bits_with_bit_unit_offset_and_guards() {
    let literals = [
        0xau32, 0x9a, 0x99a, 0x999a, 0x9999a, 0x99999a, 0xf99999a, 0x3f99999a,
    ];
    for base in ["Y20", "M80", "S80"] {
        for (index, expected) in literals.into_iter().enumerate() {
            let digits = index as u16 + 1;
            let width = digits * 4;
            let mut m = Memory::default();
            m.set_word(d("D1028"), 1);
            let b = d(base);
            let effective = Device {
                index: b.index + 1,
                ..b
            };
            for n in 0..width + 2 {
                m.set_internal(
                    Device {
                        index: b.index + n,
                        ..b
                    },
                    true,
                );
            }
            let a = action(&format!("DMOVR F1.2 K{digits}{base}E0"));
            let mut observed = BTreeSet::new();
            a.execute(&mut m, true, &mut observed);
            assert_eq!(packed(&m, effective, width), expected, "K{digits}{base}");
            assert!(m.bit(b));
            assert!(m.bit(Device {
                index: effective.index + width,
                ..effective
            }));
            assert!(!observed.contains(&b));
            assert!(!observed.contains(&Device {
                index: effective.index + width,
                ..effective
            }));
        }
    }
}

#[test]
fn negative_zero_is_not_normalized_and_short_groups_drop_only_unstored_bits() {
    for digits in 1..=8 {
        let mut m = Memory::default();
        seed(&mut m);
        let width = digits * 4;
        let base = d("M80");
        for n in 0..width + 2 {
            m.set_internal(
                Device {
                    index: 79 + n,
                    ..base
                },
                true,
            );
        }
        action(&format!("DMOVR F-0.0 K{digits}M80E0")).execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(
            packed(&m, base, width),
            if digits == 8 { 0x80000000 } else { 0 }
        );
        assert!(m.bit(d("M79")));
        assert!(m.bit(Device {
            index: 80 + width,
            ..base
        }));
        assert_eq!(flags(&m), [true; 3]);
    }
    let mut m = Memory::default();
    let a = action("DMOVR F-0.0 D20E0");
    a.execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.double_word(d("D20")) as u32, 0x80000000);
    assert_eq!(a.display_value(&m), "H80000000 / -0");
}

#[test]
fn timer_pairs_and_c199_low_only_natural_counters_have_independent_guard_oracles() {
    let mut m = Memory::default();
    m.set_word(d("D1028"), -1);
    action("DMOVR F1.2 T255E0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(
        [m.word(d("T254")) as u16, m.word(d("T255")) as u16],
        [0x999a, 0x3f99]
    );
    for (base, offset, effective) in [
        ("C198", 0, 198),
        ("C199", 0, 199),
        ("C199", 1, 200),
        ("C200", -1, 199),
        ("C200", 0, 200),
        ("C254", 1, 255),
    ] {
        let mut m = Memory::default();
        m.set_word(d("D1028"), offset);
        m.set_double_word(d("C200"), 0x12345678);
        m.set_double_word(d("C201"), 0x23456789);
        let mut observed = BTreeSet::new();
        action(&format!("DMOVR F1.2 {base}E0")).execute(&mut m, true, &mut observed);
        match effective {
            198 => {
                assert_eq!(
                    [m.word(d("C198")) as u16, m.word(d("C199")) as u16],
                    [0x999a, 0x3f99]
                );
                assert_eq!(m.double_word(d("C200")), 0x12345678);
            }
            199 => {
                assert_eq!(m.word(d("C199")) as u16, 0x999a);
                assert_eq!(m.double_word(d("C200")), 0x12345678);
                assert!(!observed.contains(&d("C200")));
            }
            200 => {
                assert_eq!(m.double_word(d("C200")) as u32, 0x3f99999a);
                assert_eq!(m.double_word(d("C201")), 0x23456789);
                assert!(!observed.contains(&d("C201")));
            }
            255 => assert_eq!(m.double_word(d("C255")) as u32, 0x3f99999a),
            _ => unreachable!(),
        }
    }
}

#[test]
fn target_overwriting_its_own_modifier_uses_one_cached_pair() {
    for (suffix, alias) in [
        ("E0", "D1028"),
        ("F0", "D1029"),
        ("E1", "D1182"),
        ("F7", "D1195"),
    ] {
        let mut m = Memory::default();
        let target = d(alias);
        m.set_word(target, 1);
        let before = Device {
            index: target.index - 1,
            ..target
        };
        let after = Device {
            index: target.index + 2,
            ..target
        };
        m.set_word(before, 77);
        m.set_word(after, 88);
        let a = action(&format!("DMOVR F1.2 D{}{suffix}", target.index - 1));
        let mut observed = BTreeSet::new();
        a.execute(&mut m, true, &mut observed);
        assert_eq!(m.word(target) as u16, 0x999a);
        assert_eq!(
            m.word(Device {
                index: target.index + 1,
                ..target
            }) as u16,
            0x3f99
        );
        assert_eq!([m.word(before), m.word(after)], [77, 88]);
        assert!(m.execution_limitations.is_empty());
    }
}

#[test]
fn flags_are_held_except_when_target_bits_explicitly_overlap_them() {
    for mask in 0u8..8 {
        let mut m = Memory::default();
        for (i, n) in (1020..=1022).enumerate() {
            m.set_system(n, mask & (1 << i) != 0);
        }
        let expected = flags(&m);
        action("DMOVR F0 K8M80E0").execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(flags(&m), expected);
    }
    let mut m = Memory::default();
    seed(&mut m);
    action("DMOVR F1.2 K1M1019E0").execute(&mut m, true, &mut BTreeSet::new());
    assert!(!m.system_bit(1019));
    assert_eq!(flags(&m), [true, false, true]);
    assert!(!m.system_bit(1067));
}

#[test]
fn pulse_disabled_and_held_paths_do_not_resolve_retarget_or_observe() {
    let p = compile_il("LD X0\nDMOVRP F1.2 D20E0\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_word(d("D1028"), -30000);
    seed(&mut sim.memory);
    run(&mut sim, &p);
    assert!(sim.memory.execution_limitations.is_empty());
    assert!(!sim.observed_devices.contains(&d("D1028")));
    sim.memory.set_word(d("D1028"), 0);
    sim.memory.set_input(d("X0"), true);
    run(&mut sim, &p);
    assert_eq!(sim.memory.double_word(d("D20")) as u32, 0x3f99999a);
    sim.memory.set_word(d("D1028"), 2);
    sim.observed_devices.clear();
    run(&mut sim, &p);
    assert_eq!(sim.memory.double_word(d("D22")), 0);
    assert!(!sim.observed_devices.contains(&d("D1028")));
    sim.memory.set_word(d("D1028"), -30000);
    run(&mut sim, &p);
    assert!(sim.memory.execution_limitations.is_empty());
    sim.memory.set_input(d("X0"), false);
    run(&mut sim, &p);
    sim.memory.set_word(d("D1028"), 2);
    sim.memory.set_input(d("X0"), true);
    run(&mut sim, &p);
    assert_eq!(sim.memory.double_word(d("D22")) as u32, 0x3f99999a);
    assert_eq!(flags(&sim.memory), [true; 3]);
}

#[test]
fn normal_visits_same_scan_and_loop_changes_use_live_indices() {
    let p=compile_il("LD X0\nMOV K0 E0\nFOR K3\nDMOVR F1.2 D100E0\nADD E0 K2 E0\nNEXT\nMOV K1 E0\nDMOVR F-0.0 K8M80E0\nEND").unwrap();
    let mut sim = enabled();
    run(&mut sim, &p);
    for address in ["D100", "D102", "D104"] {
        assert_eq!(sim.memory.double_word(d(address)) as u32, 0x3f99999a);
    }
    assert_eq!(packed(&sim.memory, d("M81"), 32), 0x80000000);
    for address in ["D100", "D101", "D104", "D105", "M81", "M112"] {
        assert!(sim.observed_devices.contains(&d(address)));
    }
}

#[test]
fn full_effective_spans_are_model_checked_with_no_invented_plc_math_error() {
    for last in [4999, 9999] {
        let mut m = Memory::default();
        m.data_register_limit = Some(last);
        m.set_word(d("D1028"), -1);
        let a = action(&format!("DMOVR F1.2 D{last}E0"));
        a.execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(
            m.double_word(d(&format!("D{}", last - 1))) as u32,
            0x3f99999a
        );
        m.set_word(d("D1028"), 0);
        seed(&mut m);
        let mut observed = BTreeSet::new();
        a.execute(&mut m, true, &mut observed);
        assert!(!m.execution_limitations.is_empty());
        assert!(!m.system_bit(1067));
        assert_eq!(flags(&m), [true; 3]);
        assert_eq!(observed, BTreeSet::from([d("D1028")]));
    }
    let mut m = Memory::default();
    m.relay_limit = Some(511);
    assert!(action("DMOVR F1 K1M508E0").resolve(&m).is_ok());
    assert!(action("DMOVR F1 K1M509E0").resolve(&m).is_err());
    for target in [
        "K1M1284E0",
        "K2M1532E0",
        "K2S1020E0",
        "K1Y377E0",
        "T255E0",
        "C255E0",
    ] {
        let mut m = Memory::default();
        if target == "C255E0" {
            m.set_word(d("D1028"), 1);
        }
        assert!(
            action(&format!("DMOVR F1 {target}")).resolve(&m).is_err(),
            "{target}"
        );
    }
}

#[test]
fn preparation_is_pure_and_display_uses_actual_truncated_target_storage() {
    let a = action("DMOVR F1.2 K1M80E0");
    let mut m = Memory::default();
    let mut observed = BTreeSet::new();
    a.prepare(&m, &mut observed).unwrap();
    assert_eq!(packed(&m, d("M80"), 4), 0);
    assert!(observed.contains(&d("M83")));
    a.execute(&mut m, true, &mut observed);
    assert!(a.display_value(&m).starts_with("H0000000A / "));
    let a = action("DMOVR F1.2 C199E0");
    a.execute(&mut m, true, &mut observed);
    assert!(a.display_value(&m).starts_with("H0000999A / "));
    let a = action("DMOVR F1.2 D20E0");
    a.execute(&mut m, true, &mut observed);
    assert_eq!(a.display_value(&m), "H3F99999A / 1.2");
    m.set_word(d("D1028"), 2);
    m.set_double_word(d("D22"), 0x40000000);
    let mut ui = BTreeSet::new();
    a.observed_devices(&m, &mut ui);
    assert!(ui.contains(&d("D22")) && ui.contains(&d("D23")));
    assert_eq!(a.display_value(&m), "H40000000 / 2");
    m.set_word(d("D1028"), -30000);
    assert!(a.display_value(&m).starts_with("İndeks:"));
}

#[test]
fn ordinary_and_addressed_literals_have_identical_raw_results_and_no_hidden_scratch() {
    for (literal, expected) in [
        ("F1.2", 0x3f99999a),
        ("F-0.0", 0x80000000),
        ("F0", 0),
        ("F-2.5", 0xc0200000),
    ] {
        let mut ordinary = Memory::default();
        seed(&mut ordinary);
        ordinary.set_system(1018, true);
        ordinary.set_system(1081, true);
        ordinary.set_word(d("D5000"), 123);
        ordinary.set_word(d("D5001"), 456);
        let mut indexed = ordinary.clone();
        FloatAction::parse("DMOVR", &args(&format!("{literal} D20")))
            .unwrap()
            .execute(&mut ordinary);
        action(&format!("DMOVR {literal} D20E0")).execute(&mut indexed, true, &mut BTreeSet::new());
        for m in [&ordinary, &indexed] {
            assert_eq!(m.double_word(d("D20")) as u32, expected);
            assert_eq!(flags(m), [true; 3]);
            assert!(m.system_bit(1018) && m.system_bit(1081));
            assert_eq!([m.word(d("D5000")), m.word(d("D5001"))], [123, 456]);
            assert!(!m.system_bit(1067));
        }
    }
}
