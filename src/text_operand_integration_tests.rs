//! Independent literal oracles for the text operand matrices and native
//! cursor distinctions. Expected values are not computed by text helpers.
use crate::es2_encoder::{PreflightCheck, encode};
use crate::plc::{Area, Device, Memory, ScanStatus, Simulator, compile_il};
use crate::text_encoding::TextAction;
use std::collections::BTreeSet;

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn at(base: Device, offset: u16) -> Device {
    Device {
        index: base.index + offset,
        ..base
    }
}
fn put(m: &mut Memory, raw: &str, value: u16) {
    let base = d(raw);
    for bit in 0..16 {
        if base.area == Area::X {
            m.set_input(at(base, bit), value & (1 << bit) != 0);
        } else {
            m.set_internal(at(base, bit), value & (1 << bit) != 0);
        }
    }
}
fn get(m: &Memory, raw: &str) -> u16 {
    let base = d(raw);
    (0..16).fold(0, |value, bit| {
        value | (u16::from(m.bit(at(base, bit))) << bit)
    })
}
fn action(name: &str, args: &[&str]) -> TextAction {
    TextAction::parse(
        name,
        &args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn seed_flags(m: &mut Memory) {
    m.set_system(1020, true);
    m.set_system(1021, false);
    m.set_system(1022, true);
}
fn flags(m: &Memory) -> [bool; 3] {
    [1020, 1021, 1022].map(|n| m.system_bit(n))
}

#[test]
fn segment_groups_preserve_neighbors_and_all_index_aliases_are_real_words() {
    for source in ["X0", "Y0", "M0", "S0"] {
        for target in ["Y200", "M128", "S128"] {
            let mut m = Memory::default();
            put(&mut m, source, 0xfffa);
            put(&mut m, target, 0xa5a5);
            seed_flags(&mut m);
            action("SEGD", &[&format!("K1{source}"), &format!("K1{target}")]).execute(&mut m);
            assert_eq!(get(&m, target), 0xa5a7, "{source} -> {target}");
            assert_eq!(flags(&m), [true, false, true]);
        }
    }
    // Literal documented aliases, independent of CalendarIndices.
    for (index, low, high) in [
        (0, 1028, 1029),
        (1, 1182, 1183),
        (2, 1184, 1185),
        (3, 1186, 1187),
        (4, 1188, 1189),
        (5, 1190, 1191),
        (6, 1192, 1193),
        (7, 1194, 1195),
    ] {
        let mut m = Memory::default();
        m.set_word(d(&format!("D{low}")), 10);
        m.set_word(d(&format!("D{high}")), -1);
        action("SEGD", &[&format!("E{index}"), &format!("F{index}")]).execute(&mut m);
        assert_eq!(m.word(d(&format!("D{high}"))), 0x77);
        assert_eq!(m.word(d(&format!("D{low}"))), 10);
        m.set_word(d(&format!("D{high}")), 15);
        action("SEGD", &[&format!("F{index}"), &format!("E{index}")]).execute(&mut m);
        assert_eq!(m.word(d(&format!("D{low}"))), 0x71);
        action("SEGD", &[&format!("E{index}"), &format!("E{index}")]).execute(&mut m);
        assert_eq!(m.word(d(&format!("D{low}"))), 0x06);
    }
}

#[test]
fn all_eight_forms_obey_disabled_hold_and_pulse_rearm_with_packed_results() {
    for (op, count, first, second, out1, out2) in [
        ("SEGD", "", 2, 3, [0x5b, 0xffff], [0x4f, 0xffff]),
        (
            "ASCI",
            " K2",
            0x12,
            0x34,
            [0x3231, 0xffff],
            [0x3433, 0xffff],
        ),
        ("HEX", " K2", 0x3132, 0x3334, [0x21, 0xffff], [0x43, 0xffff]),
        ("CCD", " K2", 0x0201, 0x0402, [3, 3], [6, 6]),
    ] {
        for pulse in [false, true] {
            let line = format!("{op}{} K4M0 K4S128{count}", if pulse { "P" } else { "" });
            let program = compile_il(&format!("LD X200\n{line}\nEND")).unwrap();
            let mut s = Simulator::default();
            put(&mut s.memory, "M0", first);
            put(&mut s.memory, "S128", 0xffff);
            put(&mut s.memory, "S144", 0xffff);
            seed_flags(&mut s.memory);
            for (enabled, input, expected) in [
                (false, first, [0xffff, 0xffff]),
                (true, first, out1),
                (true, second, if pulse { out1 } else { out2 }),
                (false, second, if pulse { out1 } else { out2 }),
                (true, second, out2),
            ] {
                put(&mut s.memory, "M0", input);
                s.memory.set_input(d("X200"), enabled);
                s.scan(&program, 0);
                assert_eq!(s.last_scan, ScanStatus::Complete, "{line}");
                assert_eq!(
                    [get(&s.memory, "S128"), get(&s.memory, "S144")],
                    expected,
                    "{line}"
                );
                assert_eq!(flags(&s.memory), [true, false, true]);
            }
        }
    }
}

#[test]
fn ascii_short_sources_use_fixed_word_stride_and_zero_high_nibbles() {
    for low_only in [false, true] {
        let mut m = Memory::default();
        m.set_system(1161, low_only);
        put(&mut m, "M0", 0xabcd);
        put(&mut m, "M16", 0x1234);
        action("ASCI", &["K1M0", "D20", "K8"]).execute(&mut m);
        let expected: &[i16] = if low_only {
            &[0x30, 0x30, 0x30, 0x34, 0x30, 0x30, 0x30, 0x44]
        } else {
            &[0x3030, 0x3430, 0x3030, 0x4430]
        };
        for (i, value) in expected.iter().enumerate() {
            assert_eq!(m.word(at(d("D20"), i as u16)), *value);
        }
        assert_eq!(get(&m, "M0"), 0xabcd);
        assert_eq!(get(&m, "M16"), 0x1234);
    }
}

#[test]
fn ascii_group_writes_clear_used_bytes_but_odd_tail_only_clears_declared_high_bits() {
    for (digits, odd, even) in [
        (1, 0xff01, 0x0001),
        (2, 0xff41, 0x0041),
        (3, 0xf041, 0x0241),
        (4, 0x0041, 0x4241),
    ] {
        let mut m = Memory::default();
        put(&mut m, "M128", 0xffff);
        m.set_internal(d("M127"), true);
        m.set_internal(d("M160"), true);
        action("ASCI", &["HA", &format!("K{digits}M128"), "K1"]).execute(&mut m);
        assert_eq!(get(&m, "M128"), odd);
        put(&mut m, "M128", 0xffff);
        action("ASCI", &["HAB", &format!("K{digits}M128"), "K2"]).execute(&mut m);
        assert_eq!(get(&m, "M128"), even);
        assert!(m.bit(d("M127")) && m.bit(d("M160")));
    }
    let mut m = Memory::default();
    m.set_system(1161, true);
    put(&mut m, "S128", 0xffff);
    put(&mut m, "S144", 0xffff);
    action("ASCI", &["HAB", "K1S128", "K2"]).execute(&mut m);
    assert_eq!([get(&m, "S128"), get(&m, "S144")], [1, 2]);
}

#[test]
fn hex_group_input_ignores_bit_seven_and_short_low_byte_slots_still_stride_sixteen() {
    let mut m = Memory::default();
    put(&mut m, "M0", 0xc2c1);
    action("HEX", &["K4M0", "D20", "K2"]).execute(&mut m);
    assert_eq!(m.word(d("D20")), 0xab);
    assert!(!m.system_bit(1067));
    m.set_system(1161, true);
    put(&mut m, "M0", 0xffc1);
    put(&mut m, "M16", 0xffb2);
    action("HEX", &["K2M0", "K1S128", "K2"]).execute(&mut m);
    assert_eq!(get(&m, "S128"), 2);
    let mut numeric = Memory::default();
    numeric.set_word(d("D0"), 0xc2c1u16 as i16);
    numeric.set_word(d("D20"), 0x5678);
    action("HEX", &["D0", "D20", "K2"]).execute(&mut numeric);
    assert_eq!(numeric.word(d("D20")), 0x5678);
    assert_eq!(numeric.word(d("D1067")), 0xe1a);
    assert_eq!(numeric.word(d("D1068")), 7);
}

#[test]
fn hex_group_invalid_second_character_keeps_unwritten_nibbles_and_prior_output() {
    let mut m = Memory::default();
    m.set_word(d("D0"), 0x4741); // A, then invalid G.
    m.set_word(d("D1"), 0x3030);
    put(&mut m, "M128", 0x5678);
    action("HEX", &["D0", "K4M128", "K4"]).execute(&mut m);
    assert_eq!(get(&m, "M128"), 0xa678);
    assert_eq!(m.word(d("D1067")), 0xe1a);
    assert_eq!(m.word(d("D1068")), 7);
    let mut short = Memory::default();
    short.set_word(d("D0"), 0x3241); // A2, two output nibbles.
    put(&mut short, "M128", 0xffff);
    action("HEX", &["D0", "K1M128", "K2"]).execute(&mut short);
    assert_eq!(get(&short, "M128"), 2);
}

#[test]
fn checksum_groups_use_two_full_sixteen_bit_outputs_and_snapshot_overlapping_input() {
    for low_only in [false, true] {
        let mut m = Memory::default();
        m.set_system(1161, low_only);
        put(&mut m, "M0", 0xffff);
        put(&mut m, "M16", 0xffff);
        put(&mut m, "S128", 0xffff);
        put(&mut m, "S144", 0xffff);
        m.set_internal(d("S127"), true);
        m.set_internal(d("S160"), true);
        action(
            "CCD",
            &["K1M0", "K1S128", if low_only { "K2" } else { "K4" }],
        )
        .execute(&mut m);
        assert_eq!([get(&m, "S128"), get(&m, "S144")], [14, 0]);
        assert!(m.bit(d("S127")) && m.bit(d("S160")));
    }
    let mut m = Memory::default();
    put(&mut m, "M0", 0x0302);
    put(&mut m, "M16", 0x0705);
    action("CCD", &["K4M0", "K1M0", "K4"]).execute(&mut m);
    assert_eq!([get(&m, "M0"), get(&m, "M16")], [1, 3]);
}

#[test]
fn checksum_dynamic_count_and_group_endpoints_preserve_native_error_commit_order() {
    let mut m = Memory::default();
    put(&mut m, "S1008", 0x0201);
    m.set_word(d("D0"), 3);
    m.set_word(d("D20"), 77);
    m.set_word(d("D21"), 88);
    action("CCD", &["K4S1008", "D20", "D0"]).execute(&mut m);
    assert_eq!([m.word(d("D20")), m.word(d("D21"))], [77, 88]);
    assert_eq!([m.word(d("D1067")), m.word(d("D1068"))], [0xe1a, 5]);
    let mut target = Memory::default();
    target.set_word(d("D0"), 0x0201);
    put(&mut target, "S1008", 0xffff);
    action("CCD", &["D0", "K1S1008", "K2"]).execute(&mut target);
    assert_eq!(get(&target, "S1008"), 3);
    assert_eq!(
        [target.word(d("D1067")), target.word(d("D1068"))],
        [0xe1a, 3]
    );
    let mut invalid = Memory::default();
    invalid.set_word(d("D0"), 0);
    put(&mut invalid, "M128", 0xa5a5);
    action("CCD", &["K1S0", "K1M128", "D0"]).execute(&mut invalid);
    assert_eq!(get(&invalid, "M128"), 0xa5a5);
    assert_eq!(invalid.word(d("D1068")), 5);
}

#[test]
fn roles_labels_tracking_and_monitoring_keep_real_packed_operands() {
    for invalid in [
        "SEGD X0 D0",
        "SEGD K1 K1X0",
        "SEGD E8 D0",
        "ASCI E0 D0 K1",
        "ASCI D0 F0 K1",
        "ASCI D0 D20 D0",
        "ASCI D0 K1X0 K1",
        "HEX F0 D0 K1",
        "HEX D0 E0 K1",
        "HEX D0 D20 K257",
        "CCD K1 D0 K2",
        "CCD E0 D0 K2",
        "CCD D0 K1Y0 K2",
        "CCD D0 F0 K2",
        "CCD D0 D20 K1M0",
    ] {
        assert!(
            compile_il(&format!("LD X0\n{invalid}\nEND")).is_err(),
            "{invalid}"
        );
    }
    let a = action("ASCI", &["K1M0", "K1S128", "K5"]);
    assert_eq!(a.label(), "ASCI K1M0 K1S128 K5");
    let mut m = Memory::default();
    put(&mut m, "S128", 0xabcd);
    assert_eq!(a.display_value(&m), "13");
    let mut devices = BTreeSet::new();
    a.observed_devices(&m, &mut devices);
    for required in ["M0", "M3", "M16", "M19", "S128", "S159", "S167", "M1161"] {
        assert!(devices.contains(&d(required)), "{required}");
    }
    assert!(!devices.contains(&d("M4")));
    assert!(!devices.contains(&d("M20")));
    let index = action("SEGD", &["E7", "F0"]);
    devices.clear();
    index.devices(&mut devices);
    assert!(devices.contains(&d("D1194")) && devices.contains(&d("D1029")));
}

#[test]
fn unproven_ascii_hex_expanded_boundaries_hold_with_an_explicit_limitation() {
    for (name, args) in [("ASCI", ["D0", "K1S1016", "K2"])] {
        let mut m = Memory::default();
        m.set_word(d("D0"), 0x12);
        m.set_word(d("D20"), 0x5678);
        m.set_internal(d("S1016"), true);
        let a = action(name, &args);
        assert!(a.validate_execution(&m).is_err());
        a.execute(&mut m);
        assert_eq!(m.word(d("D20")), 0x5678);
        assert!(m.bit(d("S1016")));
        assert!(!m.execution_limitations.is_empty());
        assert!(!m.system_bit(1067));
    }
}

#[test]
fn checked_ascii_final_nibble_rollover_is_a_plc_error_before_output() {
    let mut m = Memory::default();
    m.set_word(d("D20"), 0x5678);
    let a = action("ASCI", &["K1S1020", "D20", "K2"]);
    assert!(a.validate_execution(&m).is_ok());
    assert!(!m.system_bit(1067));
    a.execute(&mut m);
    assert!(m.execution_limitations.is_empty());
    assert_eq!(m.word(d("D20")), 0x5678);
    assert_eq!(m.word(d("D1067")) as u16, 0xe1a);
    assert_eq!(m.word(d("D1068")), 7);
}

#[test]
fn all_eight_encoder_forms_use_literal_group_and_index_descriptors() {
    for (name, operands, words) in [
        (
            "SEGD",
            "E1 F7",
            vec![0xa200, 0x6c86, 0x0180, 0x8686, 0x0180],
        ),
        (
            "SEGD",
            "K1X7 K2Y200",
            vec![0xa200, 0x0784, 0x0482, 0x8084, 0x0584],
        ),
        (
            "ASCI",
            "K1M0 K3S128 K5",
            vec![0xb400, 0x0084, 0x0882, 0x8084, 0x0086, 0x0580, 0x0080],
        ),
        (
            "HEX",
            "K4Y0 K1M128 K2",
            vec![0xb600, 0x0084, 0x0588, 0x8084, 0x0882, 0x0280, 0x0080],
        ),
        (
            "CCD",
            "K2X0 K1S128 D20",
            vec![0xb800, 0x0084, 0x0484, 0x8084, 0x0082, 0x2886, 0x0086],
        ),
    ] {
        for pulse in [false, true] {
            let line = format!("{name}{} {operands}", if pulse { "P" } else { "" });
            let p = compile_il(&format!("LD X0\n{line}\nEND")).unwrap();
            let e = encode(&p).unwrap();
            let mut expected = words.clone();
            if pulse {
                expected[0] |= 0x10;
            }
            assert_eq!(&e.words[1..1 + expected.len()], expected, "{line}");
            assert_eq!(e.next_pc, [1, 1 + expected.len() as u16, 0], "{line}");
            if matches!(name, "ASCI" | "HEX" | "CCD") {
                assert_eq!(e.preflight.get(&1), Some(&PreflightCheck::IndexedText(0)));
            }
        }
    }
}
