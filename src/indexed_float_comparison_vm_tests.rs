//! Literal encodings and independent action/contact contracts for22 forms.
use crate::float_math::FloatBinaryError;
use crate::float_ops::{FloatAction, FloatPredicate};
use crate::indexed_float_comparison::{
    IndexedFloatComparisonAction, IndexedFloatPredicate, PreparedFloatComparison,
};
use crate::plc::{Device, Expr, Memory, Program, ScanStatus, Simulator, compile_il};
use std::collections::BTreeSet;

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn args(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(str::to_owned).collect()
}
fn action(raw: &str) -> IndexedFloatComparisonAction {
    let mut parts = raw.split_whitespace();
    IndexedFloatComparisonAction::parse(
        parts.next().unwrap(),
        &parts.map(str::to_owned).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn pred(raw: &str) -> IndexedFloatPredicate {
    let mut parts = raw.split_whitespace();
    IndexedFloatPredicate::parse(
        parts.next().unwrap(),
        &parts.map(str::to_owned).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn put(m: &mut Memory, address: &str, raw: u32) {
    m.set_double_word(d(address), raw as i32);
}
fn flags(m: &Memory) -> [bool; 3] {
    [1020, 1021, 1022].map(|n| m.system_bit(n))
}
fn seed(m: &mut Memory) {
    for n in 1020..=1022 {
        m.set_system(n, true);
    }
}
fn result(m: &Memory, base: &str) -> [bool; 3] {
    let base = d(base);
    [0, 1, 2].map(|n| {
        m.bit(Device {
            index: base.index + n,
            ..base
        })
    })
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

#[test]
fn four_action_headers_preserve_default_three_bit_target_and_original_descriptors() {
    for (name, header, zone) in [
        ("DECMP", 0xed00, false),
        ("DECMPP", 0xed10, false),
        ("DEZCP", 0xef00, true),
        ("DEZCPP", 0xef10, true),
    ] {
        let body = if zone {
            format!("{name} D10E1 K2 D14F0 M80F7")
        } else {
            format!("{name} D10E1 K2 M80F7")
        };
        let a = action(&body);
        let mut expected = vec![
            header, 0x14e6, 0x8086, 0x0080, 0x0080, 0x0280, 0x0080, 0x0080, 0x0080,
        ];
        if zone {
            expected.extend([0x1cf6, 0x0086, 0x0080, 0x0080]);
        }
        expected.extend([0x50f4, 0xe880, 0x0080, 0x0080]);
        assert_eq!(a.native_words().unwrap(), expected);
        assert_eq!(a.label(), body);
        let p = compile_il(&format!("LD X0\n{body}\nMOV K7 D300\nOUT Y0\nEND")).unwrap();
        assert!(p.requires_vm());
        let native = crate::es2_encoder::encode(&p).unwrap();
        assert_eq!(&native.words[1..1 + expected.len()], expected);
        assert_eq!(native.action_pc[1] as usize, 1 + expected.len());
        let mut sim = enabled();
        put(&mut sim.memory, "D10", 0x3f80_0000);
        put(&mut sim.memory, "D14", 0x3fc0_0000);
        seed(&mut sim.memory);
        run(&mut sim, &p);
        assert_eq!(
            result(&sim.memory, "M80"),
            if zone {
                [false, true, false]
            } else {
                [false, false, true]
            }
        );
        assert_eq!(flags(&sim.memory), [true; 3]);
        assert_eq!(sim.memory.word(d("D300")), 7);
        assert!(sim.memory.bit(d("Y0")));
    }
}

#[test]
fn all_eighteen_contact_headers_are_nine_words_without_padding() {
    for (prefix, base) in [("LD", 0x47), ("AND", 0x53), ("OR", 0x5f)] {
        for (relation, offset, expected) in [
            ("=", 0, true),
            (">", 2, false),
            ("<", 4, false),
            ("<>", 6, false),
            ("<=", 8, true),
            (">=", 10, true),
        ] {
            let name = format!("F{prefix}{relation}");
            let p = pred(&format!("{name} D10E1 F-0.0"));
            let literal = [
                ((base + offset) << 8) | 2,
                0x14e6,
                0x8086,
                0x0080,
                0x0080,
                0x0080,
                0x0080,
                0x0080,
                0x8080,
            ];
            assert_eq!(p.native_words(prefix).unwrap(), literal);
            assert_eq!(p.label(prefix), format!("{name} D10E1 F-0.0"));
            assert!(p.short_circuits());
            let mut m = Memory::default();
            put(&mut m, "D10", 0);
            seed(&mut m);
            assert_eq!(p.evaluate(&m).unwrap(), expected);
            assert_eq!(flags(&m), [true; 3]);
            assert!(!m.system_bit(1067));
            let source = match prefix {
                "LD" => format!("{name} D10E1 F-0.0\nOUT Y0\nEND"),
                "AND" => format!("LD X0\n{name} D10E1 F-0.0\nOUT Y0\nEND"),
                _ => format!("LD X1\n{name} D10E1 F-0.0\nOUT Y0\nEND"),
            };
            let program = compile_il(&source).unwrap();
            let native = crate::es2_encoder::encode(&program).unwrap();
            let start = usize::from(prefix != "LD");
            assert_eq!(&native.words[start..start + 9], literal);
            let mut sim = enabled();
            run(&mut sim, &program);
            assert_eq!(sim.memory.bit(d("Y0")), expected);
        }
    }
}

#[test]
fn every_index_bank_remains_a_signed_word_for_sources_and_bit_result() {
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
        m.set_word(d(alias), -2);
        let upper = Device {
            index: d(alias).index + 1,
            ..d(alias)
        };
        m.set_word(upper, i16::MIN);
        put(&mut m, "D8", 0x3f80_0000);
        let mut observed = BTreeSet::new();
        let a = action(&format!("DECMP D10{suffix} K0 M80{suffix}"));
        a.execute(&mut m, true, &mut observed);
        assert_eq!(result(&m, "M78"), [true, false, false]);
        assert_eq!(a.modifier_devices(), vec![d(alias)]);
        assert!(observed.contains(&d(alias)) && !observed.contains(&upper));
        let p = pred(&format!("FLD> D10{suffix} F0"));
        observed.clear();
        assert!(p.visit(&m, &mut observed).unwrap());
        assert!(observed.contains(&d("D8")) && observed.contains(&d("D9")));
        assert!(!observed.contains(&upper));
    }
}

#[test]
fn source_matrices_distinguish_integer_actions_from_raw_real_contacts() {
    for name in ["DECMP", "DECMPP"] {
        for raw in ["K1 HFFFFFFFF M80E0", "D0E0 K0 Y0F7", "D9999E1 D2 S10"] {
            assert!(IndexedFloatComparisonAction::parse(name, &args(raw)).is_ok());
        }
        for raw in [
            "F1 K0 M80E0",
            "E1 K0 M80E0",
            "T0E0 K0 M80",
            "C200F0 K0 M80",
            "K8M0E0 K0 M80",
            "D0E0 K0 X0",
            "D0E0 K0 K1M0",
            "D0E8 K0 M80",
            "D0E0 K0 D20",
            "D9999 K0 M80E0",
        ] {
            assert!(
                IndexedFloatComparisonAction::parse(name, &args(raw)).is_err(),
                "{raw}"
            );
        }
    }
    for prefix in ["FLD", "FAND", "FOR"] {
        for relation in ["=", ">", "<", "<>", "<=", ">="] {
            let name = format!("{prefix}{relation}");
            for raw in [
                "F1.200E+0 D0E0",
                "T255E0 C200F7",
                "C255E1 D9999F0",
                "F-0.0 C199E0",
            ] {
                assert!(IndexedFloatPredicate::parse(&name, &args(raw)).is_ok());
            }
            for raw in [
                "K0 D0E0",
                "H3F800000 D0E0",
                "E1 D0E0",
                "K8M0E0 F0",
                "X0E0 F0",
                "D0E8 F0",
                "T255 F0",
                "D9999 F0",
                "FNaN D0E0",
                "FInf D0E0",
            ] {
                assert!(
                    IndexedFloatPredicate::parse(&name, &args(raw)).is_err(),
                    "{raw}"
                );
            }
        }
    }
    assert_eq!(
        pred("FLD= F1.200E+0 D0E0").label("LD"),
        "FLD= F1.200E+0 D0E0"
    );
    assert!(!IndexedFloatPredicate::supports("FLD=P"));
    assert!(!IndexedFloatComparisonAction::supports("DECMPPP"));
}

#[test]
fn compare_integer_rounding_is_numeric_and_raw_pairs_keep_ieee_bits() {
    for (source, raw, expected) in [
        ("K16777217", 0x4b80_0000, [false, true, false]),
        ("H80000000", 0xcf00_0000, [false, true, false]),
        ("H1", 1, [true, false, false]),
        ("K0", 0x8000_0000, [false, true, false]),
        ("K1", 0x7f80_0000, [false, false, true]),
        ("K-1", 0xff80_0000, [true, false, false]),
    ] {
        let mut m = Memory::default();
        put(&mut m, "D0", raw);
        action(&format!("DECMP {source} D0E0 M80F0")).execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(result(&m, "M80"), expected, "{source} {raw:08x}");
    }
    let mut m = Memory::default();
    put(&mut m, "D0", 1);
    assert!(pred("FLD> D0E0 F0").evaluate(&m).unwrap());
    assert!(!pred("FLD= D0E0 F1").evaluate(&m).unwrap());
}

#[test]
fn decmp_reads_both_before_nan_error_and_never_resolves_error_target() {
    for (left, right) in [(0x7fc0_0001, 0x3f80_0000), (0x3f80_0000, 0xff80_0001)] {
        let a = action("DECMP D0E0 D10E1 M80F7");
        let mut m = Memory::default();
        put(&mut m, "D0", left);
        put(&mut m, "D10", right);
        m.set_word(d("D1195"), -30000);
        seed(&mut m);
        for bit in ["M80", "M81", "M82"] {
            m.set_internal(d(bit), true);
        }
        let mut observed = BTreeSet::new();
        assert_eq!(
            a.semantic_error(&m).unwrap(),
            Some(FloatBinaryError {
                code: 0xe1a,
                offset: 5
            })
        );
        assert_eq!(
            a.prepare(&m, &mut observed).unwrap(),
            PreparedFloatComparison::SemanticError {
                code: 0xe1a,
                offset: 5
            }
        );
        assert!(!m.system_bit(1067));
        a.execute(&mut m, true, &mut observed);
        assert_eq!([m.word(d("D1067")), m.word(d("D1068"))], [0xe1a, 5]);
        assert_eq!(result(&m, "M80"), [true; 3]);
        assert_eq!(flags(&m), [true; 3]);
        for bit in ["D1028", "D1182", "D0", "D1", "D10", "D11"] {
            assert!(observed.contains(&d(bit)));
        }
        assert!(!observed.contains(&d("D1195")));
        assert!(m.execution_limitations.is_empty());
    }
    let a = action("DECMP D0E0 D10E1 M80F7");
    let mut m = Memory::default();
    put(&mut m, "D0", 0x7fc0_0001);
    m.set_word(d("D1182"), -30000);
    let mut observed = BTreeSet::new();
    a.execute(&mut m, true, &mut observed);
    assert!(!m.system_bit(1067));
    assert!(!m.execution_limitations.is_empty());
    assert!(observed.contains(&d("D1182")) && !observed.contains(&d("D1195")));
}

#[test]
fn zone_reads_all_three_and_retains_inclusive_reversed_and_unordered_contracts() {
    for (lo, hi, value, expected) in [
        (1.0, 3.0, 0.0, [true, false, false]),
        (1.0, 3.0, 1.0, [false, true, false]),
        (1.0, 3.0, 3.0, [false, true, false]),
        (1.0, 3.0, 4.0, [false, false, true]),
        (3.0, 1.0, 2.0, [true, false, false]),
        (3.0, 1.0, 3.0, [false, true, false]),
        (3.0, 1.0, 4.0, [false, false, true]),
    ] {
        let mut m = Memory::default();
        for (address, n) in [("D0", lo), ("D2", hi), ("D4", value)] {
            put(&mut m, address, f32::to_bits(n));
        }
        seed(&mut m);
        action("DEZCP D0E0 D2F0 D4E1 M80F7").execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(result(&m, "M80"), expected);
        assert_eq!(flags(&m), [true; 3]);
    }
    for nan_index in 0..3 {
        let mut m = Memory::default();
        for (index, address) in ["D0", "D2", "D4"].into_iter().enumerate() {
            put(
                &mut m,
                address,
                if index == nan_index {
                    0x7fc0_0001
                } else {
                    0x3f80_0000
                },
            );
        }
        let a = action("DEZCP D0E0 D2F0 D4E1 M80F7");
        let mut observed = BTreeSet::new();
        assert_eq!(a.semantic_error(&m).unwrap(), None);
        a.execute(&mut m, true, &mut observed);
        assert_eq!(result(&m, "M80"), [true, false, false]);
        assert!(!m.system_bit(1067));
        for address in ["D0", "D1", "D2", "D3", "D4", "D5", "D1195"] {
            assert!(observed.contains(&d(address)));
        }
    }
}

#[test]
fn result_writes_only_three_bits_and_flag_aliases_are_real_storage() {
    for base in ["Y20", "M80", "S80"] {
        let mut m = Memory::default();
        let b = d(base);
        for offset in 0..10 {
            m.set_internal(
                Device {
                    index: b.index - 1 + offset,
                    ..b
                },
                true,
            );
        }
        seed(&mut m);
        action(&format!("DECMP K1 K2 {base}E0")).execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(result(&m, base), [false, false, true]);
        for offset in [0, 4, 5, 6, 7, 8, 9] {
            assert!(m.bit(Device {
                index: b.index - 1 + offset,
                ..b
            }));
        }
        assert_eq!(flags(&m), [true; 3]);
    }
    let mut m = Memory::default();
    seed(&mut m);
    action("DECMP K2 K1 M1020E0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(flags(&m), [true, false, false]);
}

#[test]
fn contacts_preserve_native_unordered_mapping_without_errors_or_alu_writes() {
    for prefix in ["FLD", "FAND", "FOR"] {
        for (relation, expected) in [
            ("=", true),
            (">", false),
            ("<", true),
            ("<>", false),
            ("<=", true),
            (">=", false),
        ] {
            for (left, right) in [(0x7fc0_0001, 0x3f80_0000), (0x3f80_0000, 0xff80_0001)] {
                let mut m = Memory::default();
                put(&mut m, "D0", left);
                put(&mut m, "D2", right);
                seed(&mut m);
                let mut observed = BTreeSet::new();
                assert_eq!(
                    pred(&format!("{prefix}{relation} D0E0 D2F0"))
                        .visit(&m, &mut observed)
                        .unwrap(),
                    expected
                );
                assert_eq!(flags(&m), [true; 3]);
                assert!(!m.system_bit(1067));
                assert!(observed.contains(&d("D0")) && observed.contains(&d("D3")));
            }
        }
    }
}

#[test]
fn contacts_follow_timer_pairs_and_natural_counter_reclassification() {
    let mut m = Memory::default();
    put(&mut m, "T254", 0x3f80_0000);
    put(&mut m, "C200", 0x4000_0000);
    put(&mut m, "C255", 0xbf80_0000);
    m.set_word(d("C199"), -1);
    m.set_word(d("D1028"), -1);
    assert!(pred("FLD= T255E0 F1").evaluate(&m).unwrap());
    assert!(pred("FLD< C200E0 F1").evaluate(&m).unwrap()); // C199 zero-high subnormal, not C200 high half.
    let mut observed = BTreeSet::new();
    pred("FLD< C200E0 F1").visit(&m, &mut observed).unwrap();
    assert!(observed.contains(&d("C199")) && !observed.contains(&d("C200")));
    m.set_word(d("D1028"), 1);
    assert!(pred("FLD= C199E0 F2").evaluate(&m).unwrap());
    assert!(pred("FLD= C254E0 F-1").evaluate(&m).unwrap());
    assert_eq!(
        pred("FLD= C199E1 F2").native_words("LD").unwrap()[1..5],
        [0x8ee6, 0x2184, 0x0080, 0x0080]
    );
    m.set_word(d("D1028"), 0);
    assert!(pred("FLD= T255E0 F1").evaluate(&m).is_err());
    m.set_word(d("D1028"), 1);
    assert!(pred("FLD= C255E0 F1").evaluate(&m).is_err());
}

#[test]
fn accumulator_gates_contacts_before_address_reads_and_inv_does_not_hide_faults() {
    for (source, expected) in [
        ("LD X0\nFAND= D0E0 F0\nOUT Y0\nEND", false),
        ("LDI X0\nFOR= D0E0 F0\nOUT Y0\nEND", true),
    ] {
        let p = compile_il(source).unwrap();
        let mut sim = Simulator::default();
        sim.memory.set_word(d("D1028"), -30000);
        run(&mut sim, &p);
        assert_eq!(sim.memory.bit(d("Y0")), expected);
        assert!(!sim.observed_devices.contains(&d("D1028")));
        assert!(sim.memory.execution_limitations.is_empty());
    }
    let p = compile_il("FLD= D0E0 F0\nINV\nOUT Y0\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_word(d("D1028"), -30000);
    let condition = Expr::Invert(Box::new(Expr::IndexedFloatComparison(pred("FLD= D0E0 F0"))));
    assert!(!condition.evaluate(&sim.memory));
    sim.scan(&p, 0);
    assert_ne!(sim.last_scan, ScanStatus::Complete);
    assert!(!sim.memory.bit(d("Y0")));
    assert!(!sim.memory.system_bit(1067));
}

#[test]
fn action_pulse_off_and_held_paths_do_not_read_modifiers_or_change_results() {
    for (name, zone) in [("DECMPP", false), ("DEZCPP", true)] {
        let body = if zone {
            format!("{name} K0 K2 D0E0 M80F0")
        } else {
            format!("{name} D0E0 K0 M80F0")
        };
        let p = compile_il(&format!("LD X0\n{body}\nEND")).unwrap();
        let mut sim = Simulator::default();
        sim.memory.set_word(d("D1028"), -30000);
        seed(&mut sim.memory);
        run(&mut sim, &p);
        assert!(sim.memory.execution_limitations.is_empty());
        assert_eq!(flags(&sim.memory), [true; 3]);
        sim.memory.set_word(d("D1028"), 0);
        put(&mut sim.memory, "D0", 0x3f80_0000);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &p);
        let held = result(&sim.memory, "M80");
        sim.memory.set_word(d("D1028"), -30000);
        sim.observed_devices.clear();
        run(&mut sim, &p);
        assert_eq!(result(&sim.memory, "M80"), held);
        assert!(!sim.observed_devices.contains(&d("D1028")));
        sim.memory.set_input(d("X0"), false);
        run(&mut sim, &p);
        sim.memory.set_word(d("D1028"), 0);
        put(&mut sim.memory, "D0", 0xbf80_0000);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &p);
        assert_eq!(
            result(&sim.memory, "M80"),
            if zone {
                [true, false, false]
            } else {
                [false, false, true]
            }
        );
    }
}

#[test]
fn same_scan_changes_retarget_reads_and_outputs_without_stale_observation() {
    let p=compile_il("LD X0\nMOV K0 E0\nDECMP D0E0 K0 M80E0\nMOV K2 E0\nDECMP D0E0 K0 M80E0\nFLD= D0E0 F-1\nOUT Y0\nEND").unwrap();
    let mut sim = enabled();
    put(&mut sim.memory, "D0", 0x3f80_0000);
    put(&mut sim.memory, "D2", 0xbf80_0000);
    run(&mut sim, &p);
    assert!(sim.memory.bit(d("M80")));
    assert_eq!(result(&sim.memory, "M82"), [false, false, true]);
    assert!(sim.memory.bit(d("Y0")));
    for address in ["D0", "D1", "D2", "D3", "M80", "M84"] {
        assert!(sim.observed_devices.contains(&d(address)));
    }
}

#[test]
fn model_endpoints_and_result_span_limits_are_explicit() {
    for last in [4999, 9999] {
        let mut m = Memory::default();
        m.data_register_limit = Some(last);
        m.set_word(d("D1028"), -1);
        let a = action(&format!("DECMP D{last}E0 K0 M80F0"));
        assert!(a.prepare(&m, &mut BTreeSet::new()).is_ok());
        assert!(pred(&format!("FLD= D{last}E0 F0")).evaluate(&m).unwrap());
        m.set_word(d("D1028"), 0);
        assert!(a.resolved_source(0, &m).is_err());
    }
    let mut m = Memory::default();
    m.relay_limit = Some(100);
    assert!(
        action("DECMP D0E0 K0 M98F0")
            .prepare(&m, &mut BTreeSet::new())
            .is_ok()
    );
    assert!(
        action("DECMP D0E0 K0 M99F0")
            .prepare(&m, &mut BTreeSet::new())
            .is_err()
    );
    for target in ["M1284E0", "M1532E0", "S1022E0", "Y377E0"] {
        assert!(
            action(&format!("DECMP D0E0 K0 {target}"))
                .prepare(&Memory::default(), &mut BTreeSet::new())
                .is_err()
        );
    }
}

#[test]
fn active_contact_reads_right_even_when_left_is_unordered_and_monitoring_is_independent() {
    let p = pred("FAND= D0E0 D10F7");
    let mut m = Memory::default();
    put(&mut m, "D0", 0x7fc0_0001);
    m.set_word(d("D1195"), -30000);
    let mut observed = BTreeSet::new();
    assert!(p.visit(&m, &mut observed).is_err());
    assert!(observed.contains(&d("D0")) && observed.contains(&d("D1195")));
    assert!(!m.system_bit(1067));
    let a = action("DECMP D0E0 K0 M80F7");
    m.set_word(d("D1195"), 2);
    m.set_internal(d("M82"), true);
    observed.clear();
    a.execute(&mut m, true, &mut observed);
    assert!(!observed.contains(&d("D1195")) && !observed.contains(&d("M82")));
    let mut display = BTreeSet::new();
    a.observed_devices(&m, &mut display);
    assert!(display.contains(&d("M82")) && display.contains(&d("M84")));
    assert_eq!(a.display_value(&m), "1 / 0 / 0");
}

#[test]
fn error_pc_is_header_plus_five_and_success_retains_sticky_error() {
    let p = compile_il("LD X0\nMOV K7 D50\nDECMP D0E0 K0 M80F0\nOUT Y0\nEND").unwrap();
    let native = crate::es2_encoder::encode(&p).unwrap();
    let mut sim = enabled();
    put(&mut sim.memory, "D0", 0x7fc0_0001);
    seed(&mut sim.memory);
    run(&mut sim, &p);
    assert_eq!(sim.memory.word(d("D1067")), 0xe1a);
    assert_eq!(sim.memory.word(d("D1068")) as u16, native.action_pc[1] + 5);
    put(&mut sim.memory, "D0", 0);
    run(&mut sim, &p);
    assert_eq!(result(&sim.memory, "M80"), [false, true, false]);
    assert_eq!(sim.memory.word(d("D1067")), 0xe1a);
    assert_eq!(flags(&sim.memory), [true; 3]);
}

#[test]
fn ordinary_and_indexed_compare_seams_agree_without_mode_or_scratch_changes() {
    for (name, raw, expected) in [
        ("DECMP", 0x3f80_0000, [true, false, false]),
        ("DEZCP", 0x3f80_0000, [false, true, false]),
        ("DEZCP", 0x7fc0_0001, [true, false, false]),
    ] {
        let mut ordinary = Memory::default();
        put(&mut ordinary, "D0", raw);
        seed(&mut ordinary);
        ordinary.set_system(1018, true);
        ordinary.set_system(1081, true);
        ordinary.set_word(d("D5000"), 123);
        ordinary.set_word(d("D5001"), 456);
        let mut indexed = ordinary.clone();
        let direct = if name == "DECMP" {
            "D0 K0 M80"
        } else {
            "K0 K2 D0 M80"
        };
        FloatAction::parse(name, &args(direct))
            .unwrap()
            .execute(&mut ordinary);
        let addressed = if name == "DECMP" {
            "D0E0 K0 M80F0"
        } else {
            "K0 K2 D0E0 M80F0"
        };
        action(&format!("{name} {addressed}")).execute(&mut indexed, true, &mut BTreeSet::new());
        for m in [&ordinary, &indexed] {
            assert_eq!(result(m, "M80"), expected);
            assert_eq!(flags(m), [true; 3]);
            assert!(m.system_bit(1018) && m.system_bit(1081));
            assert_eq!([m.word(d("D5000")), m.word(d("D5001"))], [123, 456]);
        }
    }
    let mut m = Memory::default();
    put(&mut m, "D0", 0x7fc0_0001);
    for relation in ["=", ">", "<", "<>", "<=", ">="] {
        let ordinary = FloatPredicate::parse(&format!("FLD{relation}"), &args("D0 F0")).unwrap();
        assert_eq!(
            ordinary.evaluate(&m),
            pred(&format!("FLD{relation} D0E0 F0"))
                .evaluate(&m)
                .unwrap()
        );
    }
}
