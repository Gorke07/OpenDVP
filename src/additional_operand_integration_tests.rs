//! Values and operand boundaries independent of the native corpus generator.
use crate::additional_processing::AdditionalAction;
use crate::plc::{Area, Device, Memory, ScanStatus, Simulator, compile_il};
use crate::random_ops::RandomAction;
use std::collections::BTreeSet;
fn d(name: &str) -> Device {
    name.parse().unwrap()
}
fn a(name: &str, args: &[&str]) -> AdditionalAction {
    AdditionalAction::parse(
        name,
        &args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn r(name: &str, args: &[&str]) -> RandomAction {
    RandomAction::parse(
        name,
        &args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn group(m: &Memory, start: u16, width: u16) -> u32 {
    (0..width).fold(0, |v, i| {
        v | (u32::from(m.bit(Device {
            area: Area::M,
            index: start + i,
        })) << i)
    })
}

#[test]
fn swap_indices_preserve_adjacent_words_and_double_swaps_bytes_within_each_word() {
    for (index, first) in [1028, 1182, 1184, 1186, 1188, 1190, 1192, 1194]
        .into_iter()
        .enumerate()
    {
        for high in [false, true] {
            let name = format!("{}{index}", if high { "F" } else { "E" });
            let alias = d(&format!("D{}", first + u16::from(high)));
            let mut m = Memory::default();
            m.set_double_word(alias, 0x34cd_12ab);
            let op = a("SWAP", &[&name]);
            op.execute(&mut m);
            assert_eq!(m.double_word(alias), 0x34cd_ab12);
            assert_eq!(op.display_value(&m), (-21742i16).to_string());
            op.execute(&mut m);
            assert_eq!(m.double_word(alias), 0x34cd_12ab);
        }
        let mut m = Memory::default();
        let alias = d(&format!("D{first}"));
        m.set_double_word(alias, 0x12ab_34cd);
        let op = a("DSWAP", &[&format!("E{index}")]);
        op.execute(&mut m);
        assert_eq!(m.double_word(alias), 0xab12_cd34u32 as i32);
        assert_eq!(op.display_value(&m), (0xab12_cd34u32 as i32).to_string());
    }
}

#[test]
fn mask_move_wide_f_reads_and_writes_actual_adjacent_d_words() {
    for (index, first) in [1029, 1183, 1185, 1187, 1189, 1191, 1193, 1195]
        .into_iter()
        .enumerate()
    {
        let f = format!("F{index}");
        let alias = d(&format!("D{first}"));
        let before = d(&format!("D{}", first - 1));
        let after = d(&format!("D{}", first + 2));
        let mut m = Memory::default();
        m.set_double_word(d("D0"), 0xaa55_cc33u32 as i32);
        m.set_double_word(alias, 0x1234_5678);
        m.set_word(before, 111);
        m.set_word(after, 222);
        let op = a("DMVM", &["D0", "HFF0000FF", &f]);
        op.execute(&mut m);
        assert_eq!(m.double_word(alias), 0xaa34_5633u32 as i32);
        assert_eq!(m.word(before), 111);
        assert_eq!(m.word(after), 222);
        assert_eq!(op.display_value(&m), (0xaa34_5633u32 as i32).to_string());
        let mut devices = BTreeSet::new();
        op.devices(&mut devices);
        assert!(devices.contains(&alias));
        assert!(devices.contains(&d(&format!("D{}", first + 1))));
        m.set_double_word(alias, 0xff00_00ffu32 as i32);
        m.set_double_word(d("D10"), 0x1234_5678);
        a("DMVM", &["D0", &f, "D10"]).execute(&mut m);
        assert_eq!(m.double_word(d("D10")), 0xaa34_5633u32 as i32);
    }
}

#[test]
fn mask_move_snapshots_overlapping_f_source_and_e_destination() {
    let mut m = Memory::default();
    m.set_word(d("D1182"), 0xaa);
    m.set_double_word(d("D1183"), 0x1234_5678);
    a("DMVM", &["F1", "H00FF00FF", "E1"]).execute(&mut m);
    assert_eq!(m.double_word(d("D1182")), 0x5634_0078);
    assert_eq!(m.word(d("D1184")), 0x1234);
}

#[test]
fn index_mask_into_short_group_truncates_only_the_target_and_monitor_reads_group() {
    let mut m = Memory::default();
    for bit in 127..=132 {
        m.set_internal(
            Device {
                area: Area::M,
                index: bit,
            },
            true,
        );
    }
    m.set_word(d("D1182"), 0x1235);
    m.set_word(d("D1183"), -1);
    let op = a("MVM", &["E1", "F1", "K1M128"]);
    op.execute(&mut m);
    assert_eq!(group(&m, 128, 4), 5);
    assert!(m.bit(d("M127")) && m.bit(d("M132")));
    assert_eq!(op.display_value(&m), "5");
    let swap = a("SWAP", &["K1M128"]);
    swap.execute(&mut m);
    assert_eq!(group(&m, 128, 4), 0);
    assert_eq!(swap.display_value(&m), "0");
}

#[test]
fn random_equal_index_bounds_snapshot_then_write_word_or_pair() {
    for wide in [false, true] {
        for (index, first) in [1029, 1183, 1185, 1187, 1189, 1191, 1193, 1195]
            .into_iter()
            .enumerate()
        {
            let f = format!("F{index}");
            let alias = d(&format!("D{first}"));
            let mut m = Memory::default();
            let value = if wide { 70000 } else { 1234 };
            if wide {
                m.set_double_word(alias, value);
            } else {
                m.set_double_word(alias, 0x1234_04d2);
            }
            let op = r(if wide { "DRAND" } else { "RAND" }, &[&f, &f, &f]);
            op.execute(&mut m);
            assert_eq!(m.word(alias), value as i16);
            assert_eq!(
                m.word(d(&format!("D{}", first + 1))),
                if wide { 1 } else { 0x1234 }
            );
            assert_eq!(m.random_state, Some(301));
            assert_eq!(op.display_value(&m), value.to_string());
        }
    }
}

#[test]
fn random_negative_wide_f_bound_does_not_advance_state_or_write_target() {
    for args in [["F1", "E3", "F7"], ["E3", "F1", "F7"]] {
        let mut m = Memory::default();
        m.set_double_word(d("D1183"), i32::MIN);
        m.set_double_word(d("D1186"), 123);
        m.set_double_word(d("D1195"), 0x1234_5678);
        m.random_state = Some(0xffff);
        r("DRAND", &args).execute(&mut m);
        assert_eq!(m.random_state, Some(0xffff));
        assert_eq!(m.double_word(d("D1195")), 0x1234_5678);
        assert_eq!(m.word(d("D1067")), 0xe1a);
        assert_eq!(m.word(d("D1068")), 5);
    }
    let mut m = Memory::default();
    m.random_state = Some(0xffff);
    m.set_double_word(d("D1183"), 10);
    m.set_double_word(d("D1186"), 1);
    m.set_double_word(d("D1195"), 99);
    r("DRAND", &["F1", "E3", "F7"]).execute(&mut m);
    assert_eq!(
        m.random_state,
        Some(99),
        "reversed nonnegative bounds advance first"
    );
    assert_eq!(m.double_word(d("D1195")), 99);
}

#[test]
fn indexed_pulse_actions_rearm_without_clobbering_unpowered_results() {
    for (body, wide, first) in [
        ("SWAPP E1", false, 1182),
        ("DSWAPP E1", true, 1182),
        ("MVMP E2 HFFFF F1", false, 1183),
        ("DMVMP F2 HFFFFFFFF F1", true, 1183),
        ("RANDP E2 E2 F1", false, 1183),
        ("DRANDP F2 F2 F1", true, 1183),
    ] {
        let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X200"), true);
        s.memory.set_double_word(d("D1182"), 0x1234_5678);
        s.memory.set_double_word(d("D1184"), 7);
        s.memory.set_double_word(d("D1185"), 9);
        s.scan(&p, 0);
        assert_eq!(s.last_scan, ScanStatus::Complete);
        let read = |m: &Memory| {
            if wide {
                m.double_word(d(&format!("D{first}")))
            } else {
                i32::from(m.word(d(&format!("D{first}"))))
            }
        };
        let old = read(&s.memory);
        s.scan(&p, 0);
        assert_eq!(read(&s.memory), old, "{body}");
        s.memory.set_input(d("X200"), false);
        s.scan(&p, 0);
        assert_eq!(read(&s.memory), old);
        s.memory.set_word(d("D1184"), 3);
        s.memory.set_word(d("D1185"), 4);
        s.memory.set_word(d("D1186"), 0);
        s.memory.set_input(d("X200"), true);
        s.scan(&p, 0);
        let expected = if body.starts_with("SWAP") {
            0x5678
        } else if body.starts_with("DSWAP") {
            0x1234_5678
        } else if wide {
            4
        } else {
            3
        };
        assert_eq!(read(&s.memory), expected, "{body}");
    }
}

#[test]
fn family_operand_exceptions_do_not_expand_unrelated_matrices() {
    for body in [
        "DSWAP F1",
        "MVM K1 K1 D0",
        "DMVM H1 H1 F1",
        "MVM E1 K1 K1X0",
        "MMOV E1 D0",
        "MMOV K1 E1",
        "WSUM E1 K1 D0",
        "WSUM D0 E1 D10",
        "WSUM D0 K1 E1",
        "SCAL E1 K1 K1 D0",
        "SCLP E1 D0 D10",
        "DSCLP D0 E1 D10",
        "CMPT E1 D0 K1 D20",
        "DCMPT D0 D10 K1 E1",
        "DSUM F1 E1",
        "DMEAN K1M0 F1 K1",
        "RAND D0 D1 K1X0",
    ] {
        assert!(
            compile_il(&format!("LD X0\n{body}\nEND")).is_err(),
            "{body}"
        );
    }
}

#[test]
fn current_es2_cmpt_float_relations_cover_unordered_zero_infinity_and_subnormals() {
    let left: [u32; 11] = [
        0xbf800000, 0xbf800000, 0x80000000, 0, 0x7f800000, 0xff800000, 0x7fc00001, 0x7fc00001,
        0x7f800001, 1, 0x80000001,
    ];
    let right: [u32; 11] = [
        0xc0000000, 0x3f800000, 0, 0x80000000, 0x7f800000, 0x7f800000, 0, 0x7fc00001, 0x3f800000,
        0, 0,
    ];
    for (mode, expected) in [
        (0, 0x1dc),
        (1, 0x5e2),
        (2, 0x5fe),
        (3, 0x201),
        (4, 0x21d),
        (5, 0x1dc),
        (255, 0x1dc),
    ] {
        let mut m = Memory::default();
        m.set_system(1162, true);
        for (start, values) in [(0, left), (40, right)] {
            for (i, word) in values.into_iter().enumerate() {
                m.set_double_word(d(&format!("D{}", start + i * 2)), word as i32);
            }
        }
        m.set_double_word(d("D100"), -1);
        a(
            "DCMPT",
            &[
                "D0",
                "D40",
                &format!("H{:08X}", 0x12340000u32 | (mode << 8) | 11),
                "D100",
            ],
        )
        .execute(&mut m);
        assert_eq!(m.double_word(d("D100")), expected, "relation {mode}");
        assert!(!m.system_bit(1067));
    }
    let p = compile_il("LD X0\nDCMPT D0 D40 K1 D100\nEND").unwrap();
    assert!(
        p.devices.contains(&d("M1162")),
        "implicit mode read must be compared"
    );
}

#[test]
fn current_cmpt_partial_reads_hold_target_and_report_exact_operand_offset() {
    for (op, wide, offset) in [("CMPT", false, 5), ("DCMPT", true, 9)] {
        for source in ["D9999", "T255"] {
            let mut m = Memory::default();
            m.set_word(d(source), -1);
            m.set_double_word(d("D0"), 65535);
            m.set_double_word(d("D100"), 0x12345678);
            a(op, &[source, "D0", if wide { "K1" } else { "K2" }, "D100"]).execute(&mut m);
            assert_eq!(m.double_word(d("D100")), 0x12345678);
            assert_eq!(m.word(d("D1067")), 0xe1a);
            assert_eq!(m.word(d("D1068")), offset);
        }
    }
    let mut m = Memory::default();
    m.set_word(d("C199"), -1);
    m.set_double_word(d("D0"), 65535);
    a("DCMPT", &["C199", "D0", "K1", "D100"]).execute(&mut m);
    assert_eq!(
        m.double_word(d("D100")),
        1,
        "C199 intentionally has no high half"
    );
    assert!(!m.system_bit(1067));
}

#[test]
fn legacy_cmpt_keeps_integer_comparison_and_silent_partial_hold() {
    use crate::additional_processing::CompareTableProfile;
    for (profile, expected) in [
        (CompareTableProfile::CurrentEs2, [0, 3]),
        (CompareTableProfile::InstalledLegacy, [1, 6]),
    ] {
        let mut m = Memory::default();
        m.compare_table_profile = profile;
        m.set_system(1162, true);
        for (address, word) in [
            ("D0", 0xbf800000u32),
            ("D2", 0x40000000),
            ("D4", 0),
            ("D40", 0xc0000000),
            ("D42", 0x3f800000),
            ("D44", 0x80000000),
        ] {
            m.set_double_word(d(address), word as i32);
        }
        for (relation, want) in [(1, expected[0]), (3, expected[1])] {
            a(
                "DCMPT",
                &[
                    "D0",
                    "D40",
                    &format!("H{:04X}", (relation << 8) | 3),
                    "D100",
                ],
            )
            .execute(&mut m);
            assert_eq!(m.double_word(d("D100")), want);
        }
    }
    for (op, count, initial) in [
        ("CMPT", "K2", 1234),
        ("DCMPT", "K2", 1234),
        ("DCMPT", "K1", 65535),
    ] {
        let mut m = Memory::default();
        m.compare_table_profile = CompareTableProfile::InstalledLegacy;
        m.set_word(d("T255"), -1);
        m.set_double_word(d("D0"), 65535);
        m.set_double_word(d("D100"), initial);
        a(op, &["T255", "D0", count, "D100"]).execute(&mut m);
        assert_eq!(
            m.double_word(d("D100")),
            if count == "K1" { 1 } else { initial }
        );
        assert!(!m.system_bit(1067));
    }
}
