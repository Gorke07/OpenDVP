//! Literal compiler descriptors and typed resolution tests. VM/native scan
//! behavior is independently covered by the root's separate integration suite.
use crate::indexed_address::{IndexWord, IndexedAddress};
use crate::indexed_scalar::IndexedScalarAction;
use crate::plc::{Action, Area, Device, Memory};
use std::collections::BTreeSet;

fn d(text: &str) -> Device {
    text.parse().unwrap()
}
fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}
fn action(name: &str, items: &[&str]) -> IndexedScalarAction {
    IndexedScalarAction::parse(name, &args(items)).unwrap()
}
fn source_value(a: Action, m: &Memory) -> i32 {
    match a {
        Action::Move(source, _) => source.read(m),
        Action::Wide(a) => a.sources[0].value().read(m),
        _ => panic!("expected move"),
    }
}

#[test]
fn literal_native_descriptors_preserve_d_bank_layout_and_counter_subtypes() {
    for (raw, wide, digits, expected) in [
        ("D5E1", false, None, vec![0x0ae6, 0x8086]),
        ("D5F7", false, None, vec![0x0af6, 0x8089]),
        ("D4095E0", false, None, vec![0xfee6, 0x1f86]),
        ("D4096E0", false, None, vec![0x00e6, 0x2086]),
        ("D9998E7", true, None, vec![0x1ce6, 0xce89, 0x0080, 0x0080]),
        ("T255F7", false, None, vec![0xfef6, 0xe182]),
        ("C199E1", false, None, vec![0x8ee6, 0x2184]),
        ("C200E1", true, None, vec![0x00e6, 0x2284, 0x0080, 0x0080]),
        ("C255F7", true, None, vec![0xdcf6, 0xe284, 0x0080, 0x0080]),
        ("M128E1", false, Some(1), vec![0x80e4, 0x2882]),
        ("X7F7", true, Some(8), vec![0x07f4, 0xe490, 0x0080, 0x0080]),
        ("Y200E2", false, Some(4), vec![0x80e4, 0x4588]),
        ("S128F3", false, Some(2), vec![0x80f4, 0x6084]),
    ] {
        assert_eq!(
            IndexedAddress::parse(raw)
                .unwrap()
                .native_words(wide, digits)
                .unwrap(),
            expected,
            "{raw}"
        );
    }
}

#[test]
fn all_forty_forms_have_correct_headers_labels_and_typed_actions() {
    let mut visited = BTreeSet::new();
    for (word, double, header, sources) in [
        ("MOV", "DMOV", 0x2800, 1),
        ("ADD", "DADD", 0x3800, 2),
        ("SUB", "DSUB", 0x3a00, 2),
        ("INC", "DINC", 0x4000, 0),
        ("DEC", "DDEC", 0x4200, 0),
        ("CML", "DCML", 0x2c00, 1),
        ("NEG", "DNEG", 0x4a00, 0),
        ("WAND", "DAND", 0x4400, 2),
        ("WOR", "DOR", 0x4600, 2),
        ("WXOR", "DXOR", 0x4800, 2),
    ] {
        for wide in [false, true] {
            for pulse in [false, true] {
                let name = format!(
                    "{}{}",
                    if wide { double } else { word },
                    if pulse { "P" } else { "" }
                );
                let mut operands = Vec::new();
                if sources >= 1 {
                    operands.push("D5E1".to_owned());
                }
                if sources >= 2 {
                    operands.push("T5F2".to_owned());
                }
                operands.push("K1M128E7".to_owned());
                assert!(IndexedScalarAction::supports(&name));
                assert!(IndexedScalarAction::has_modifiers(&operands));
                let a = IndexedScalarAction::parse(&name, &operands).unwrap();
                assert_eq!(a.label(), format!("{name} {}", operands.join(" ")));
                let mut expected =
                    vec![header + if wide { 0x100 } else { 0 } + if pulse { 0x10 } else { 0 }];
                for descriptor in [
                    Some([0x0ae6, 0x8086]).filter(|_| sources >= 1),
                    Some([0x0af6, 0x4082]).filter(|_| sources >= 2),
                    Some([0x80e4, 0xe882]),
                ]
                .into_iter()
                .flatten()
                {
                    expected.extend(descriptor);
                    if wide {
                        expected.extend([0x0080, 0x0080]);
                    }
                }
                assert_eq!(a.native_words().unwrap(), expected, "{name}");
                let concrete = a.resolve(&Memory::default()).unwrap();
                assert_eq!(matches!(concrete, Action::Wide(_)), wide);
                assert!(!matches!(concrete, Action::Pulse(_)));
                assert!(visited.insert(name));
            }
        }
    }
    assert_eq!(visited.len(), 40);
}

#[test]
fn all_sixteen_modifier_aliases_are_signed_words_even_in_double_instructions() {
    for (bank, low, high) in [
        (0, 1028, 1029),
        (1, 1182, 1183),
        (2, 1184, 1185),
        (3, 1186, 1187),
        (4, 1188, 1189),
        (5, 1190, 1191),
        (6, 1192, 1193),
        (7, 1194, 1195),
    ] {
        for (letter, alias) in [('E', low), ('F', high)] {
            let mut m = Memory::default();
            let modifier = format!("{letter}{bank}");
            assert_eq!(
                IndexWord::parse(&modifier).unwrap().device(),
                d(&format!("D{alias}"))
            );
            m.set_word(d(&format!("D{alias}")), -2);
            // Adjacent high word must not participate in modifier arithmetic.
            m.set_word(d(&format!("D{}", alias + 1)), 0x7777);
            m.set_double_word(d("D3"), 0x1234_5678);
            let a = action("DMOV", &[&format!("D5{modifier}"), "D20"]);
            assert_eq!(source_value(a.resolve(&m).unwrap(), &m), 0x1234_5678);
            assert_eq!(a.modifier_devices(), vec![d(&format!("D{alias}"))]);
        }
    }
    assert_eq!(IndexWord::parse("F").unwrap().to_string(), "F0");
    assert_eq!(IndexedAddress::parse("X7F5").unwrap().to_string(), "X7F5");
}

#[test]
fn group_offsets_count_single_bits_and_keep_word_or_double_significance() {
    let mut m = Memory::default();
    m.set_word(d("D1183"), 1); // F1
    for bit in 8..24 {
        m.set_input(
            Device {
                area: Area::X,
                index: bit,
            },
            true,
        );
    }
    let word = action("MOV", &["K4X7F1", "D20"]);
    let wide = action("DMOV", &["K4X7F1", "D20"]);
    assert_eq!(source_value(word.resolve(&m).unwrap(), &m), -1);
    assert_eq!(source_value(wide.resolve(&m).unwrap(), &m), 65535);
    m.set_word(d("D1183"), -4);
    m.set_internal(d("M1"), true);
    m.set_internal(d("M2"), true);
    assert_eq!(
        source_value(action("MOV", &["K1M5F1", "D20"]).resolve(&m).unwrap(), &m),
        3
    );
}

#[test]
fn numeric_resolution_uses_effective_spans_and_natural_counter_boundaries() {
    let mut m = Memory::default();
    m.set_word(d("D1028"), 1);
    m.set_double_word(d("C200"), 0x1234_5678);
    assert_eq!(
        source_value(action("DMOV", &["C199E0", "D20"]).resolve(&m).unwrap(), &m),
        0x1234_5678
    );
    assert!(action("MOV", &["C199E0", "D20"]).resolve(&m).is_err());
    assert!(IndexedScalarAction::parse("MOV", &args(&["C200E0", "D20"])).is_err());
    m.set_word(d("D1028"), -1);
    m.set_word(d("C199"), -1);
    assert_eq!(
        source_value(action("DMOV", &["C200E0", "D20"]).resolve(&m).unwrap(), &m),
        65535
    );
    assert!(action("DMOV", &["T255E0", "D20"]).resolve(&m).is_ok());
    m.set_word(d("D1028"), 0);
    assert!(action("DMOV", &["T255E0", "D20"]).resolve(&m).is_err());
    m.data_register_limit = Some(4999);
    assert!(action("DMOV", &["D4999E0", "D20"]).resolve(&m).is_err());
    m.set_word(d("D1028"), -1);
    assert!(action("DMOV", &["D4999E0", "D20"]).resolve(&m).is_ok());
    m.set_word(d("D1028"), i16::MAX);
    assert!(action("MOV", &["D9999E0", "D20"]).resolve(&m).is_err());
    m.set_word(d("D1028"), i16::MIN);
    assert!(action("MOV", &["D0E0", "D20"]).resolve(&m).is_err());
}

#[test]
fn parser_rejects_non_grey_roles_bad_shapes_and_direct_double_f_without_global_widening() {
    for (name, operands) in [
        ("MOV", vec!["K2E0", "D20"]),
        ("MOV", vec!["E0F0", "D20"]),
        ("MOV", vec!["D0E8", "D20"]),
        ("MOV", vec!["D0F00", "D20"]),
        ("MOV", vec!["X0E0", "D20"]),
        ("MOV", vec!["D0E0", "K1X0E0"]),
        ("MOV", vec!["K0M0E0", "D20"]),
        ("MOV", vec!["K5M0E0", "D20"]),
        ("DMOV", vec!["K9M0E0", "D20"]),
        ("DMOV", vec!["F0", "D20E0"]),
        ("DMOV", vec!["D0F0", "F0"]),
        ("BMOV", vec!["D0E0", "D20", "K2"]),
        ("DWAND", vec!["D0E0", "D1", "D20"]),
    ] {
        assert!(
            IndexedScalarAction::parse(name, &args(&operands)).is_err(),
            "{name} {operands:?}"
        );
    }
    assert!(!IndexedScalarAction::has_modifiers(&args(&[
        "HFE", "E0", "F7", "K-2"
    ])));
    assert!(!IndexedScalarAction::supports("BMOV"));
    assert!(!IndexedScalarAction::supports("XCH"));
    assert!(!IndexedScalarAction::supports("LD"));
    assert_eq!(action("MOV", &["D5E", "D20F"]).label(), "MOV D5E0 D20F0");
}

#[test]
fn dynamic_observation_and_monitoring_use_effective_devices_not_literal_base() {
    let a = action("MOV", &["D5E1", "K1M128F7"]);
    let mut m = Memory::default();
    m.set_word(d("D1182"), 8);
    m.set_word(d("D1195"), 4);
    m.set_internal(d("M132"), true);
    m.set_internal(d("M134"), true);
    assert_eq!(a.dest(), d("M128"));
    assert_eq!(a.display_value(&m), "5");
    let mut observed = BTreeSet::new();
    a.observed_devices(&m, &mut observed);
    for required in ["D1182", "D1195", "D13", "M132", "M133", "M134", "M135"] {
        assert!(observed.contains(&d(required)), "{required}");
    }
    assert!(!observed.contains(&d("D5")));
    assert!(!observed.contains(&d("M128")));
    m.set_word(d("D1182"), -5);
    m.set_word(d("D1195"), 8);
    observed.clear();
    a.observed_devices(&m, &mut observed);
    assert!(observed.contains(&d("D0")) && observed.contains(&d("M136")));
    assert!(!observed.contains(&d("D13")) && !observed.contains(&d("M132")));
}

#[test]
fn unresolved_ranges_remain_explicit_limitations_not_silent_aliases_or_plc_errors() {
    let mut m = Memory::default();
    m.set_word(d("D1028"), 1);
    assert!(action("MOV", &["K1M1535E0", "D20"]).resolve(&m).is_err());
    m.set_word(d("D1028"), 0);
    assert!(action("MOV", &["K2M1532E0", "D20"]).resolve(&m).is_err());
    assert!(action("MOV", &["K1M1532E0", "D20"]).resolve(&m).is_err());
    m.set_word(d("D1028"), 2);
    assert!(action("MOV", &["K1", "K1M998E0"]).resolve(&m).is_err());
    m.relay_limit = Some(255);
    assert!(action("MOV", &["K1M252E0", "D20"]).resolve(&m).is_err());
    assert!(!m.system_bit(1067));
    assert!(!m.system_bit(1068));
}
