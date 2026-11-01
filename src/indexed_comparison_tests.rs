//! Independent descriptor/role/value oracles for the 98 comparison spellings.
//! Full scan, pulse and native differential coverage is a separate suite.
use crate::indexed_comparison::{IndexedComparisonAction, IndexedPredicate};
use crate::indexed_operand::{AddressedValue, has_modifier};
use crate::indexed_scalar::IndexedScalarAction;
use crate::plc::{Action, Device, Memory};
use std::collections::BTreeSet;

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn args(raw: &[&str]) -> Vec<String> {
    raw.iter().map(|s| s.to_string()).collect()
}
fn action(name: &str, raw: &[&str]) -> IndexedComparisonAction {
    IndexedComparisonAction::parse(name, &args(raw)).unwrap()
}
fn predicate(name: &str, raw: &[&str]) -> IndexedPredicate {
    IndexedPredicate::parse(name, &args(raw)).unwrap()
}

#[test]
fn literal_headers_and_descriptors_cover_all_98_spellings() {
    let mut seen = BTreeSet::new();
    for (name, header, zone, wide) in [
        ("CMP", 0x2400, false, false),
        ("CMPP", 0x2410, false, false),
        ("DCMP", 0x2500, false, true),
        ("DCMPP", 0x2510, false, true),
        ("ZCP", 0x2600, true, false),
        ("ZCPP", 0x2610, true, false),
        ("DZCP", 0x2700, true, true),
        ("DZCPP", 0x2710, true, true),
    ] {
        let operands = if zone {
            args(&["D5E1", "T5F2", "K1M128E7", "Y0E1"])
        } else {
            args(&["D5E1", "T5F2", "Y0E1"])
        };
        let a = IndexedComparisonAction::parse(name, &operands).unwrap();
        let mut expected = vec![header];
        for pair in [[0x0ae6, 0x8086], [0x0af6, 0x4082]]
            .into_iter()
            .chain(zone.then_some([0x80e4, 0xe882]))
            .chain([[0x00e4, 0x2580]])
        {
            expected.extend(pair);
            if wide {
                expected.extend([0x0080, 0x0080]);
            }
        }
        assert_eq!(a.native_words().unwrap(), expected, "{name}");
        assert_eq!(a.label(), format!("{name} {}", operands.join(" ")));
        assert!(matches!(a.resolve(&Memory::default()).unwrap(), Action::Wide(_)) == wide);
        seen.insert(name.to_owned());
    }
    // Header matrices are literal native opcode families, not derived through
    // Relation::opcode_offset or the production encoder.
    for (prefix, signed, logic, difference) in [
        (
            "LD",
            [0xd001, 0xd201, 0xd401, 0xd801, 0xda01, 0xdc01],
            [0xbe01, 0xc001, 0xc201],
            [0x7802, 0x7002, 0x7402, 0x7a02, 0x7602, 0x7202],
        ),
        (
            "AND",
            [0xe001, 0xe201, 0xe401, 0xe801, 0xea01, 0xec01],
            [0xc401, 0xc601, 0xc801],
            [0x8402, 0x7c02, 0x8002, 0x8602, 0x8202, 0x7e02],
        ),
        (
            "OR",
            [0xf001, 0xf201, 0xf401, 0xf801, 0xfa01, 0xfc01],
            [0xca01, 0xcc01, 0xce01],
            [0x9002, 0x8802, 0x8c02, 0x9202, 0x8e02, 0x8a02],
        ),
    ] {
        let cases = ["=", ">", "<", "<>", "<=", ">="]
            .into_iter()
            .zip(signed)
            .map(|(s, h)| (s.to_owned(), h, false))
            .chain(
                ["&", "|", "^"]
                    .into_iter()
                    .zip(logic)
                    .map(|(s, h)| (s.to_owned(), h, false)),
            )
            .chain(
                ["=", ">", "<", "<>", "<=", ">="]
                    .into_iter()
                    .zip(difference)
                    .map(|(s, h)| (format!("Z{s}"), h, true)),
            );
        for (suffix, header, difference) in cases {
            for wide in [false, true] {
                let name = format!("{}{prefix}{suffix}", if wide { "D" } else { "" });
                let operands = if difference {
                    args(&["D5E1", "T5F2", "K1M128E7"])
                } else {
                    args(&["D5E1", "T5F2"])
                };
                assert!(IndexedPredicate::supports(&name));
                let p = IndexedPredicate::parse(&name, &operands).unwrap();
                let mut expected = vec![header + if wide { 0x100 } else { 0 }];
                for pair in [[0x0ae6, 0x8086], [0x0af6, 0x4082]]
                    .into_iter()
                    .chain(difference.then_some([0x80e4, 0xe882]))
                {
                    expected.extend(pair);
                    if wide {
                        expected.extend([0x0080, 0x0080]);
                    }
                }
                assert_eq!(p.native_words(prefix).unwrap(), expected, "{name}");
                assert_eq!(p.label(prefix), format!("{name} {}", operands.join(" ")));
                assert_eq!(p.short_circuits(), difference);
                assert_eq!(
                    p.needs_or_padding(prefix),
                    difference && prefix == "OR" && !wide
                );
                assert!(seen.insert(name));
            }
        }
    }
    assert_eq!(seen.len(), 98);
}

#[test]
fn bare_result_descriptors_keep_zero_width_for_every_area_and_bank() {
    for (target, expected) in [
        ("Y0E1", [0x00e4, 0x2580]),
        ("M80F7", [0x50f4, 0xe880]),
        ("S128E0", [0x80e4, 0x0080]),
        ("M80", [0x5084, 0x0880]),
    ] {
        let a = action("CMP", &["D5E1", "K0", target]);
        assert_eq!(&a.native_words().unwrap()[5..], &expected);
        let a = action("DCMP", &["D5E1", "K0", target]);
        assert_eq!(
            &a.native_words().unwrap()[9..],
            &[expected[0], expected[1], 0x0080, 0x0080]
        );
    }
    let m = Memory::default();
    // Exactly three bits, not a fake K1 group: S1021..1023 is valid.
    let a = action("CMP", &["D0E0", "K0", "S1021E0"]);
    assert_eq!(a.resolve(&m).unwrap().destination(), d("S1021"));
    assert!(
        action("CMP", &["D0E0", "K0", "S1022E0"])
            .resolve(&m)
            .is_err()
    );
    for target in ["X0E0", "K1M0E0", "D0E0", "E0", "F0"] {
        assert!(
            IndexedComparisonAction::parse("CMP", &args(&["D0E0", "K0", target])).is_err(),
            "{target}"
        );
    }
}

#[test]
fn difference_s3_is_indexable_but_all_direct_index_registers_are_forbidden() {
    for name in ["LDZ=", "ANDZ<>", "ORZ<=", "DLDZ>", "DANDZ>=", "DORZ<"] {
        for role in 0..3 {
            for source in [
                "K1X0F7", "K1Y0E1", "K1M0F0", "K1S0E7", "T0E0", "C0F3", "D0E2",
            ] {
                let mut operands = args(&["K0", "K0", "K0"]);
                operands[role] = source.into();
                assert!(
                    IndexedPredicate::parse(name, &operands).is_ok(),
                    "{name} S{} {source}",
                    role + 1
                );
            }
            for source in ["E0", "F0", "E7", "F7", "K1E0", "H1E0", "X0E0"] {
                let mut operands = args(&["D0E0", "K0", "K0"]);
                operands[role] = source.into();
                // H1E0 is a valid hexadecimal literal, not a modifier.
                let expected = source == "H1E0";
                assert_eq!(
                    IndexedPredicate::parse(name, &operands).is_ok(),
                    expected,
                    "{name} S{} {source}",
                    role + 1
                );
            }
        }
    }
    for name in ["LD=", "AND<", "OR>=", "LD&", "AND|", "OR^"] {
        assert!(IndexedPredicate::parse(name, &args(&["D0E0", "F7"])).is_ok());
        assert!(IndexedPredicate::parse(&format!("D{name}"), &args(&["D0F7", "E7"])).is_ok());
        assert!(IndexedPredicate::parse(&format!("D{name}"), &args(&["D0F7", "F7"])).is_err());
    }
    for name in ["BLD", "DBLD", "BLDI", "BAND", "DBORI", "LD=P", "ANDZ=P"] {
        assert!(!IndexedPredicate::supports(name), "{name}");
    }
}

#[test]
fn signed_width_zero_extension_and_existing_difference_edges_are_preserved() {
    let mut m = Memory::default();
    m.set_word(d("D1028"), 1);
    for bit in 1..=32 {
        m.set_bit(d(&format!("M{bit}")), true);
    }
    for (name, left, right, expected) in [
        ("LD=", "K1M0E0", "K15", true),
        ("LD<", "K4M0E0", "K0", true),
        ("DLD=", "K4M0E0", "K65535", true),
        ("DLD<", "K8M0E0", "K0", true),
        ("LD&", "K1M0E0", "H10", false),
        ("DLD^", "K4M0E0", "K65535", false),
    ] {
        assert_eq!(
            predicate(name, &[left, right]).evaluate(&m).unwrap(),
            expected,
            "{name}"
        );
    }
    m.set_word(d("D1"), i16::MIN);
    m.set_word(d("D2"), 0);
    m.set_word(d("D3"), 0);
    assert!(
        predicate("LDZ=", &["D0E0", "D1E0", "D2E0"])
            .evaluate(&m)
            .unwrap()
    );
    m.set_word(d("D1"), 32767);
    m.set_word(d("D2"), -1);
    m.set_word(d("D3"), 0);
    // Native wrapping subtraction yields MIN, whose absolute helper yields0.
    assert!(
        predicate("LDZ=", &["D0E0", "D1E0", "D2E0"])
            .evaluate(&m)
            .unwrap()
    );
    m.set_double_word(d("D11"), i32::MIN);
    m.set_double_word(d("D21"), 0);
    m.set_double_word(d("D31"), 0);
    assert!(
        predicate("DLDZ=", &["D10E0", "D20E0", "D30E0"])
            .evaluate(&m)
            .unwrap()
    );
}

#[test]
fn natural_counter_resolution_and_word_modifier_width_remain_distinct() {
    let mut m = Memory::default();
    m.set_word(d("D1183"), 1); // F1 is one signed word even in DLD.
    m.set_word(d("D1184"), 0x7fff);
    m.set_double_word(d("C200"), 0x1234_5678);
    assert!(
        predicate("DLD=", &["C199F1", "H12345678"])
            .evaluate(&m)
            .unwrap()
    );
    m.set_word(d("D1183"), -1);
    m.set_word(d("C199"), -1);
    assert!(
        predicate("DLD=", &["C200F1", "K65535"])
            .evaluate(&m)
            .unwrap()
    );
    assert!(IndexedPredicate::parse("LD=", &args(&["C200F1", "K0"])).is_err());
    assert!(IndexedPredicate::parse("LD=", &args(&["C199F1", "K0"])).is_ok());
    m.set_word(d("D1183"), 1);
    assert!(predicate("LD=", &["C199F1", "K0"]).evaluate(&m).is_err());
}

#[test]
fn observed_spans_track_effective_sources_and_exact_three_results() {
    let mut m = Memory::default();
    m.set_word(d("D1028"), 2);
    m.set_word(d("D1183"), 1);
    let a = action("DCMP", &["D5E0", "K1M32F1", "M80F1"]);
    assert_eq!(a.modifier_devices(), vec![d("D1028"), d("D1183")]);
    let mut observed = BTreeSet::new();
    a.observed_devices(&m, &mut observed);
    for device in ["D7", "D8", "M33", "M34", "M35", "M36", "M81", "M82", "M83"] {
        assert!(observed.contains(&d(device)), "{device}");
    }
    for device in ["D5", "D6", "M32", "M37", "M80", "M84"] {
        assert!(!observed.contains(&d(device)), "{device}");
    }
    m.set_bit(d("M81"), true);
    m.set_bit(d("M83"), true);
    assert_eq!(a.display_value(&m), "5");
    m.set_word(d("D1183"), 2);
    assert_eq!(a.resolve(&m).unwrap().destination(), d("M82"));
    assert_eq!(a.display_value(&m), "2");
    let p = predicate("LDZ=", &["D5E0", "K1M32F1", "D20E0"]);
    observed.clear();
    p.observed_devices(&m, &mut observed);
    assert!(observed.contains(&d("D22")));
    assert!(!observed.contains(&d("D20")));
}

#[test]
fn effective_endpoint_model_and_native_mapping_limits_are_explicit() {
    let mut m = Memory::default();
    for (target, offset) in [
        ("M100E0", 1181),
        ("M1532E0", 0),
        ("M998E0", 0),
        ("M4094E0", 0),
    ] {
        m.set_word(d("D1028"), offset);
        assert!(
            action("CMP", &["K0", "K0", target]).resolve(&m).is_err(),
            "{target}"
        );
    }
    m.set_word(d("D1028"), 0);
    assert!(action("CMP", &["D0E0", "K0", "M900E0"]).resolve(&m).is_ok());
    // M1280 has no native start remap, but the complete result still includes
    // M1281/1282, which are not admitted special-relay roles in this model.
    assert!(action("CMP", &["K0", "K0", "M1280E0"]).resolve(&m).is_err());
    m.set_word(d("D1028"), 1);
    assert!(
        action("CMP", &["D0F0", "K0", "M1535E0"])
            .resolve(&m)
            .is_err()
    );
    m.set_word(d("D1028"), -1);
    assert!(action("CMP", &["D0F0", "K0", "S0E0"]).resolve(&m).is_err());
    m.relay_limit = Some(82);
    m.set_word(d("D1028"), 1);
    assert!(action("CMP", &["D0F0", "K0", "M80E0"]).resolve(&m).is_err());
    m.data_register_limit = Some(4999);
    m.set_word(d("D1028"), 0);
    assert!(predicate("DLD=", &["D4999E0", "K0"]).evaluate(&m).is_err());
    assert!(predicate("LD=", &["D4999E0", "K0"]).evaluate(&m).is_ok());
}

#[test]
fn shared_addressed_values_do_not_broaden_other_roles_or_change_scalar_encoding() {
    for literal in ["HFEED", "HEF", "E0", "F7"] {
        assert!(!has_modifier(literal));
    }
    for modified in ["D5E0", "K1M128F7"] {
        assert!(has_modifier(modified));
    }
    for invalid in ["K4E0", "K5M0E0", "X0E0", "E0F0", "D0E8"] {
        assert!(
            AddressedValue::parse(invalid, false, false).is_err(),
            "{invalid}"
        );
    }
    assert!(AddressedValue::parse("K1X0E0", false, true).is_err());
    let scalar = IndexedScalarAction::parse("ADDP", &args(&["D5E1", "T5F2", "K1M128E7"])).unwrap();
    assert_eq!(
        scalar.native_words().unwrap(),
        vec![0x3810, 0x0ae6, 0x8086, 0x0af6, 0x4082, 0x80e4, 0xe882]
    );
    assert_eq!(scalar.label(), "ADDP D5E1 T5F2 K1M128E7");
}
