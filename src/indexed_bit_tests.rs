//! Literal compiler vectors and independent operand contracts. Root-owned VM
//! tests cover actual scan/output context and the native differential corpus.
use crate::indexed_bit::{IndexedBitAction, IndexedBitContact};
use crate::plc::{Action, Device, Memory};
use crate::system_relays::{ClockProfile, ClockState};
use std::collections::BTreeSet;

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn args(raw: &str) -> Vec<String> {
    vec![raw.into()]
}
fn contact(name: &str, raw: &str) -> IndexedBitContact {
    IndexedBitContact::parse(name, &args(raw)).unwrap()
}
fn action(name: &str, raw: &str) -> IndexedBitAction {
    IndexedBitAction::parse(name, &args(raw)).unwrap()
}

#[test]
fn all_nine_native_headers_keep_bare_default_width_one() {
    let mut inventory = BTreeSet::new();
    for (name, prefix, header) in [
        ("LD", "LD", 0x1202),
        ("LDI", "LD", 0x1302),
        ("AND", "AND", 0x1402),
        ("ANI", "AND", 0x1502),
        ("OR", "OR", 0x1602),
        ("ORI", "OR", 0x1702),
    ] {
        let p = contact(name, "X7E1");
        assert_eq!(
            p.native_words(prefix).unwrap(),
            vec![header, 0x07e4, 0x2480]
        );
        assert_eq!(p.label(prefix), format!("{name} X7E1"));
        assert!(inventory.insert(name));
    }
    for (name, header) in [("OUT", 0x1802), ("SET", 0x1902), ("RST", 0x1a02)] {
        let a = action(name, "M80F7");
        assert_eq!(a.native_words().unwrap(), vec![header, 0x50f4, 0xe880]);
        assert_eq!(a.label(), format!("{name} M80F7"));
        assert!(inventory.insert(name));
    }
    assert_eq!(inventory.len(), 9);
    // No implicit Kn1 width82, no eighth-bit width84 and no wide padding.
    for (raw, pair) in [
        ("Y200E2", [0x80e4, 0x4580]),
        ("S1023E7", [0xffe4, 0xe380]),
        ("M2048F7", [0x00f4, 0xf280]),
        ("M4095E7", [0xffe4, 0xf980]),
        ("M1280E0", [0x00e4, 0x0d80]),
    ] {
        assert_eq!(
            &action("OUT", raw).native_words().unwrap()[1..],
            &pair,
            "{raw}"
        );
    }
    assert!(contact("LD", "X0E0").native_words("ANDP").is_err());
}

#[test]
fn all_sixteen_alias_banks_are_live_signed_words() {
    for (suffix, alias, pair) in [
        ("E0", 1028, [0x08e4, 0x0480]),
        ("E1", 1182, [0x08e4, 0x2480]),
        ("E2", 1184, [0x08e4, 0x4480]),
        ("E3", 1186, [0x08e4, 0x6480]),
        ("E4", 1188, [0x08e4, 0x8480]),
        ("E5", 1190, [0x08e4, 0xa480]),
        ("E6", 1192, [0x08e4, 0xc480]),
        ("E7", 1194, [0x08e4, 0xe480]),
        ("F0", 1029, [0x08f4, 0x0480]),
        ("F1", 1183, [0x08f4, 0x2480]),
        ("F2", 1185, [0x08f4, 0x4480]),
        ("F3", 1187, [0x08f4, 0x6480]),
        ("F4", 1189, [0x08f4, 0x8480]),
        ("F5", 1191, [0x08f4, 0xa480]),
        ("F6", 1193, [0x08f4, 0xc480]),
        ("F7", 1195, [0x08f4, 0xe480]),
    ] {
        let mut m = Memory::default();
        let offset = d(&format!("D{alias}"));
        m.set_word(offset, -1);
        m.set_word(d(&format!("D{}", alias + 1)), 0x7fff);
        m.set_input(d("X7"), true);
        let p = contact("LD", &format!("X10{suffix}"));
        assert_eq!(p.modifier_devices(), vec![offset]);
        assert_eq!(p.resolve(&m).unwrap(), d("X7"));
        assert!(p.evaluate(&m).unwrap());
        assert_eq!(&p.native_words("LD").unwrap()[1..], &pair);
        m.set_word(offset, 1);
        assert_eq!(p.resolve(&m).unwrap(), d("X11"));
        assert!(!p.evaluate(&m).unwrap());
        let a = action("OUT", &format!("Y10{suffix}"));
        assert_eq!(a.resolve(&m).unwrap().destination(), d("Y11"));
    }
}

#[test]
fn manual_octal_address_examples_resolve_without_decimal_reinterpretation() {
    for (name, raw, alias, offset, expected, prefix, inverted) in [
        ("LD", "X5E2", "D1184", -2, "X3", "LD", false),
        ("LDI", "X7F5", "D1191", 3, "X12", "LD", true),
        ("AND", "X10E2", "D1184", 8, "X20", "AND", false),
        ("ANI", "X15F4", "D1189", -4, "X11", "AND", true),
        ("OR", "X0F1", "D1183", 5, "X5", "OR", false),
        ("ORI", "X7E6", "D1192", -3, "X4", "OR", true),
    ] {
        let mut m = Memory::default();
        m.set_word(d(alias), offset);
        m.set_input(d(expected), true);
        let p = contact(name, raw);
        assert_eq!(p.resolve(&m).unwrap(), d(expected));
        assert_eq!(p.evaluate(&m).unwrap(), !inverted);
        assert_eq!(p.label(prefix), format!("{name} {raw}"));
    }
    for (name, raw, alias, offset, expected) in [
        ("OUT", "Y10F0", "D1029", -3, "Y5"),
        ("SET", "Y15E5", "D1190", 3, "Y20"),
        ("RST", "Y5E0", "D1028", 0, "Y5"),
    ] {
        let mut m = Memory::default();
        m.set_word(d(alias), offset);
        assert_eq!(
            action(name, raw).resolve(&m).unwrap().destination(),
            d(expected)
        );
    }
}

#[test]
fn one_bit_endpoints_and_existing_native_mapping_limits_remain_distinct() {
    let mut m = Memory::default();
    for raw in ["X377E0", "Y377E0", "S1023E0", "M4095E0", "M1280E0"] {
        assert!(contact("LD", raw).resolve(&m).is_ok(), "{raw}");
    }
    for raw in ["Y377E0", "S1023E0", "M4095E0", "M1280E0"] {
        assert!(action("OUT", raw).resolve(&m).is_ok(), "{raw}");
    }
    m.set_word(d("D1028"), 1);
    for raw in [
        "X377E0", "Y377E0", "S1023E0", "M4095E0", "M1280E0", "M1535E0",
    ] {
        assert!(contact("LD", raw).resolve(&m).is_err(), "{raw}");
    }
    m.set_word(d("D1028"), -1);
    assert!(contact("LD", "X0E0").resolve(&m).is_err());
    assert!(contact("LD", "M1536E0").resolve(&m).is_err());
    m.set_word(d("D1028"), 0);
    for raw in ["M1284E0", "M1303E0", "M1535E0"] {
        assert!(contact("LD", raw).resolve(&m).is_err(), "{raw}");
    }
    m.relay_limit = Some(2047);
    assert!(action("OUT", "M2047E0").resolve(&m).is_ok());
    assert!(action("OUT", "M2048E0").resolve(&m).is_err());
    m.set_word(d("D1028"), i16::MIN);
    assert!(contact("LD", "S1023E0").resolve(&m).is_err());
    m.set_word(d("D1028"), i16::MAX);
    assert!(contact("LD", "S1023E0").resolve(&m).is_err());
}

#[test]
fn parser_rejects_other_roles_and_every_edge_or_pulse_spelling() {
    for raw in ["X0E0", "Y0F1", "M0E7", "S0F7"] {
        assert!(IndexedBitContact::parse("LD", &args(raw)).is_ok());
    }
    for raw in [
        "X0E0", "T0E0", "C0E0", "D0E0", "E0", "F0", "K1M0E0", "K1", "HFF", "Y0",
    ] {
        assert!(IndexedBitAction::parse("OUT", &args(raw)).is_err(), "{raw}");
    }
    for raw in [
        "T0E0", "C200F0", "D0E0", "E0", "F0", "K1X0E0", "K1", "X0", "X8E0", "X0E8", "X0F-1",
    ] {
        assert!(IndexedBitContact::parse("LD", &args(raw)).is_err(), "{raw}");
    }
    for name in [
        "LDP", "LDF", "ANDP", "ANDF", "ORP", "ORF", "PLS", "PLF", "OUTP", "SETP", "RSTP", "TMR",
        "CNT", "DCNT", "ATMR", "BLD", "NP", "PN",
    ] {
        assert!(!IndexedBitContact::supports(name), "{name}");
        assert!(!IndexedBitAction::supports(name), "{name}");
    }
    assert!(IndexedBitContact::parse("LD", &[]).is_err());
    assert!(IndexedBitAction::parse("SET", &["M0E0".into(), "M1E0".into()]).is_err());
    assert_eq!(contact("ldi", " x7f ").label("LD"), "LDI X7F0");
    assert_eq!(action("out", " m80e ").label(), "OUT M80E0");
}

#[test]
fn false_output_resolution_policy_differs_from_latched_set_and_reset() {
    let mut m = Memory::default();
    m.set_word(d("D1028"), -1);
    for name in ["OUT", "SET", "RST"] {
        let a = action(name, "Y0E0");
        assert_eq!(a.is_out(), name == "OUT");
        assert_eq!(a.should_resolve(false), name == "OUT");
        assert!(a.should_resolve(true));
        // Bad effective target is an error only if the caller must resolve.
        assert!(a.resolve(&m).is_err());
    }
    m.set_word(d("D1028"), 1);
    assert!(
        matches!(action("OUT","Y0E0").resolve(&m).unwrap(),Action::Out(target) if target==d("Y1"))
    );
    assert!(
        matches!(action("SET","Y0E0").resolve(&m).unwrap(),Action::Set(target) if target==d("Y1"))
    );
    assert!(
        matches!(action("RST","Y0E0").resolve(&m).unwrap(),Action::Reset(target) if target==d("Y1"))
    );
    m.set_word(d("D1028"), 1000);
    assert!(contact("LD", "M0E0").resolve(&m).is_ok());
    assert!(action("OUT", "M0E0").resolve(&m).is_err());
    m.set_word(d("D1028"), 1004);
    assert!(contact("LD", "M0E0").resolve(&m).is_err());
}

#[test]
fn monitoring_tracks_only_actual_bits_and_retains_prior_selected_targets() {
    let mut m = Memory::default();
    m.set_word(d("D1183"), 2);
    let p = contact("LD", "X10F1");
    let a = action("OUT", "M80F1");
    let mut static_devices = BTreeSet::new();
    p.devices(&mut static_devices);
    a.devices(&mut static_devices);
    let mut expected = BTreeSet::from([d("D1183")]);
    Memory::error_devices(&mut expected);
    assert_eq!(static_devices, expected);
    let mut observed = BTreeSet::new();
    p.observed_devices(&m, &mut observed);
    a.observed_devices(&m, &mut observed);
    assert!(observed.contains(&d("X12")));
    assert!(observed.contains(&d("M82")));
    for raw in ["X10", "X11", "X13", "M80", "M81", "M83"] {
        assert!(!observed.contains(&d(raw)), "{raw}");
    }
    m.set_bit(d("M82"), true);
    assert_eq!(a.display_value(&m), "1");
    m.set_word(d("D1183"), 3);
    a.observed_devices(&m, &mut observed);
    assert!(observed.contains(&d("M82")));
    assert!(observed.contains(&d("M83")));
    assert_eq!(a.dest(), d("M80"));
    assert_eq!(a.display_value(&m), "0");
    m.set_word(d("D1183"), -100);
    assert!(a.display_value(&m).starts_with("İndeks:"));
}

#[test]
fn unseeded_indirect_clocks_remain_fallible_and_visual_reads_are_pure() {
    let mut m = Memory::default();
    m.system_relays.set_profile(ClockProfile::InstalledFixed);
    let p = contact("LDI", "M1008E0");
    m.set_word(d("D1028"), 3);
    assert_eq!(p.resolve(&m).unwrap(), d("M1011"));
    assert!(p.evaluate(&m).is_err());
    assert!(m.execution_limitations.is_empty());
    assert!(p.sample(&mut m).is_err());
    assert_eq!(m.execution_limitations.len(), 1);
    ClockState::seed_installed_levels(&mut m, [true, false, true, false]).unwrap();
    assert!(m.execution_limitations.is_empty());
    assert!(!p.evaluate(&m).unwrap());
    assert!(!p.sample(&mut m).unwrap());
    m.set_word(d("D1028"), 4);
    assert_eq!(p.resolve(&m).unwrap(), d("M1012"));
    assert!(p.evaluate(&m).unwrap());
    assert!(p.sample(&mut m).unwrap());
    assert!(m.execution_limitations.is_empty());
    m.system_relays.restart_installed_run();
    m.set_word(d("D1028"), 0);
    assert_eq!(p.resolve(&m).unwrap(), d("M1008"));
    assert!(p.sample(&mut m).is_ok());
    assert!(m.execution_limitations.is_empty());
}
