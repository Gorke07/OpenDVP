//! Fixed literal oracles and ordered-input admission tests. No native run here.
use crate::calendar_operand::CalendarIndices;
use crate::indexed_checksum::{ChecksumOutcome as Outcome, IndexedChecksumAction};
use crate::plc::{Device, Memory, Operand, Program, ScanStatus, Simulator, compile_il};
use crate::text_encoding::TextAction;
use std::collections::BTreeSet;

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn args(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(str::to_owned).collect()
}
fn a(raw: &str) -> IndexedChecksumAction {
    let words = args(raw);
    IndexedChecksumAction::parse(&words[0], &words[1..]).unwrap()
}
fn visit(raw: &str, m: &mut Memory) -> (Outcome, BTreeSet<Device>) {
    let mut seen = BTreeSet::new();
    let outcome = a(raw).execute(m, true, &mut seen);
    assert!(
        m.execution_limitations.is_empty(),
        "{:?}",
        m.execution_limitations
    );
    (outcome, seen)
}
fn put(m: &mut Memory, first: u16, words: &[u16]) {
    for (offset, word) in words.iter().enumerate() {
        m.set_word(d(&format!("D{}", first + offset as u16)), *word as i16);
    }
}
fn sample(m: &mut Memory, first: u16, lrc: bool, low: bool) {
    let bytes: &[u8] = if lrc {
        b"010307080006"
    } else {
        &[1, 6, 7, 6, 0x17, 0x70]
    };
    if low {
        put(
            m,
            first,
            &bytes
                .iter()
                .map(|v| 0xab00 | u16::from(*v))
                .collect::<Vec<_>>(),
        );
    } else {
        put(
            m,
            first,
            &bytes
                .chunks(2)
                .map(|v| u16::from(v[0]) | (u16::from(v[1]) << 8))
                .collect::<Vec<_>>(),
        );
    }
}
fn flags(m: &Memory) -> [bool; 3] {
    [1020, 1021, 1022].map(|v| m.system_bit(v))
}
fn seed_flags(m: &mut Memory) {
    for (n, on) in [(1020, true), (1021, false), (1022, true)] {
        m.set_system(n, on);
    }
}
fn complete(sim: &mut Simulator, p: &Program) {
    sim.scan(p, 0);
    assert_eq!(sim.last_scan, ScanStatus::Complete);
    assert!(
        sim.memory.execution_limitations.is_empty(),
        "{:?}",
        sim.memory.execution_limitations
    );
}

#[test]
fn exact_four_headers_and_three_original_two_word_fields() {
    for (name, header) in [
        ("LRC", 0xd000),
        ("LRCP", 0xd010),
        ("CRC", 0xd200),
        ("CRCP", 0xd210),
    ] {
        let op = a(&format!("{name} D5E1 D10F7 D20E0"));
        assert_eq!(
            op.native_words().unwrap(),
            [header, 0x0ae6, 0x8086, 0x14f6, 0x8089, 0x28e6, 0x0086]
        );
        assert_eq!(op.nominal_words(), 7);
        assert_eq!(op.dest(), d("D20"));
        assert_eq!(op.label(), format!("{name} D5E1 D10F7 D20E0"));
    }
}

#[test]
fn roles_are_only_d_and_white_kh_count_without_extra_alias_families() {
    for name in ["LRC", "LRCP", "CRC", "CRCP"] {
        for count in ["K1", "K256", "H10", "D40", "D40E0", "D40F7"] {
            assert!(
                IndexedChecksumAction::parse(name, &args(&format!("D9999E0 {count} D9999F7")))
                    .is_ok()
            );
        }
        for bad in [
            "T0 K2 D20",
            "C0 K2 D20",
            "E1 K2 D20",
            "F1 K2 D20",
            "K1M0 K2 D20",
            "D10 T0 D20",
            "D10 E0 D20",
            "D10 F0 D20",
            "D10 K1M0 D20",
            "D10 K2E0 D20",
            "D10 K2 T0",
            "D10 K2 E0",
            "D10 K2 K1M0",
            "D10 K0 D20",
            "D10 K257 D20",
            "D10 K-1 D20",
        ] {
            assert!(
                IndexedChecksumAction::parse(name, &args(bad)).is_err(),
                "{name}/{bad}"
            );
        }
    }
    assert!(!IndexedChecksumAction::supports("DLRC"));
    assert!(!IndexedChecksumAction::supports("DCRC"));
}

#[test]
fn all_sixteen_word_modifiers_resolve_all_three_roles_with_signed_offsets() {
    for high in [false, true] {
        for bank in 0..8 {
            for offset in [-1i16, 1] {
                let suffix = format!("{}{bank}", if high { "F" } else { "E" });
                let alias = CalendarIndices::alias_device(bank, high).unwrap();
                for name in ["LRC", "CRC"] {
                    let lrc = name == "LRC";
                    let mut m = Memory::default();
                    m.set_word(alias, offset);
                    // Poison adjacent index WORD: every suffix still reads only WORD.
                    m.set_word(
                        Device {
                            index: alias.index + 1,
                            ..alias
                        },
                        0x4321,
                    );
                    let source = (10i32 + i32::from(offset)) as u16;
                    let count = (40i32 + i32::from(offset)) as u16;
                    let target = (100i32 + i32::from(offset)) as u16;
                    sample(&mut m, source, lrc, false);
                    m.set_word(d(&format!("D{count}")), if lrc { 12 } else { 6 });
                    m.set_word(d(&format!("D{}", target + 1)), 0x4567);
                    let (out, seen) = visit(
                        &format!("{name} D10{suffix} D40{suffix} D100{suffix}"),
                        &mut m,
                    );
                    assert_eq!(out, Outcome::Complete);
                    assert_eq!(
                        m.word(d(&format!("D{target}"))) as u16,
                        if lrc { 0x4537 } else { 0xab66 }
                    );
                    assert_eq!(m.word(d(&format!("D{}", target + 1))), 0x4567);
                    assert!(seen.contains(&alias));
                }
            }
        }
    }
}

#[test]
fn manual_checksum_oracles_in_both_modes_replace_full_word_and_hold_alu() {
    for lrc in [true, false] {
        for low in [false, true] {
            let mut m = Memory::default();
            m.set_system(1161, low);
            seed_flags(&mut m);
            sample(&mut m, 10, lrc, low);
            put(&mut m, 99, &[0x1234, 0xffff, 0xffff, 0x5678]);
            let name = if lrc { "LRC" } else { "CRC" };
            let n = if lrc { 12 } else { 6 };
            let (out, seen) = visit(&format!("{name} D10E0 K{n} D100F0"), &mut m);
            assert_eq!(out, Outcome::Complete);
            let expected = match (lrc, low) {
                (true, false) => [0x4537, 0xffff],
                (true, true) => [0x45, 0x37],
                (false, false) => [0xab66, 0xffff],
                (false, true) => [0x66, 0xab],
            };
            assert_eq!(
                [m.word(d("D100")) as u16, m.word(d("D101")) as u16],
                expected
            );
            assert_eq!(m.word(d("D99")), 0x1234);
            assert_eq!(m.word(d("D102")), 0x5678);
            assert_eq!(flags(&m), [true, false, true]);
            assert!(seen.contains(&Memory::system_device(1161)));
        }
    }
}

#[test]
fn invalid_count_precedes_bad_source_mode_and_target_reads() {
    for n in [-1i16, 0, 257] {
        for name in ["LRC", "CRC"] {
            let mut m = Memory::default();
            m.set_word(d("D40"), n);
            m.set_word(d("D1182"), -1);
            m.set_word(d("D1183"), -1);
            m.native_numeric_scratch = Some(123);
            let (out, seen) = visit(&format!("{name} D0E1 D40E0 D0F1"), &mut m);
            assert_eq!(out, Outcome::PlcError { offset: 3 });
            assert_eq!(m.word(d("D1067")), 0xe1a);
            assert_eq!(m.word(d("D1068")), 3);
            assert!(!seen.contains(&d("D1182")) && !seen.contains(&d("D1183")));
            assert!(!seen.contains(&Memory::system_device(1161)));
            assert_eq!(m.native_numeric_scratch, Some(123));
            assert_eq!(
                a(&format!("{name} D0E1 D40E0 D0F1"))
                    .qualify_native(&m)
                    .outcome,
                Outcome::PlcError { offset: 3 }
            );
        }
    }
}

#[test]
fn lrc_odd_rejects_while_crc_odd_uses_real_available_bytes() {
    let mut m = Memory::default();
    put(&mut m, 10, &[0x0201, 0x0403]);
    let (out, seen) = visit("LRC D10E0 K3 D20F0", &mut m);
    assert_eq!(out, Outcome::PlcError { offset: 3 });
    assert!(!seen.contains(&d("D10")));
    let mut m = Memory::default();
    put(&mut m, 10, &[0x0201, 0x0403]);
    assert_eq!(visit("CRC D10E0 K1 D20F0", &mut m).0, Outcome::Complete);
    assert_eq!(m.word(d("D20")) as u16, 0x807e);
}

#[test]
fn resolver_failures_are_checked_before_payload_with_correct_stage_offsets() {
    for last in [4999u16, 9999] {
        for offset in [-1i16, 1] {
            for (body, error, seen_mode, seen_source) in [
                (
                    format!("CRC D10 D{}E1 D20F7", if offset < 0 { 0 } else { last }),
                    3,
                    false,
                    false,
                ),
                (
                    format!("CRC D{}E1 K2 D20F7", if offset < 0 { 0 } else { last }),
                    1,
                    false,
                    false,
                ),
                (
                    format!("CRC D10 K2 D{}E1", if offset < 0 { 0 } else { last }),
                    5,
                    true,
                    true,
                ),
            ] {
                let mut m = Memory::default();
                m.data_register_limit = Some(last);
                m.set_word(d("D1182"), offset);
                put(&mut m, 10, &[0x0201]);
                let (out, seen) = visit(&body, &mut m);
                assert_eq!(out, Outcome::PlcError { offset: error }, "{body}");
                assert_eq!(seen.contains(&Memory::system_device(1161)), seen_mode);
                assert_eq!(seen.contains(&d("D10")), seen_source);
                assert!(!seen.contains(&d("D1195")));
                assert_eq!(
                    a(&body).qualify_native(&m).outcome,
                    Outcome::PlcError { offset: error }
                );
            }
        }
    }
}

#[test]
fn source_extent_failure_reads_mode_but_neither_payload_nor_target() {
    for low in [false, true] {
        for name in ["LRC", "CRC"] {
            let mut m = Memory::default();
            m.data_register_limit = Some(4999);
            m.set_system(1161, low);
            m.set_word(d("D1195"), -1);
            m.native_numeric_scratch = Some(55);
            let (out, seen) = visit(&format!("{name} D4999E0 K4 D0F7"), &mut m);
            assert_eq!(out, Outcome::PlcError { offset: 1 });
            assert!(seen.contains(&Memory::system_device(1161)));
            assert!(!seen.contains(&d("D4999")) && !seen.contains(&d("D1195")));
            assert_eq!(m.native_numeric_scratch, Some(55));
        }
    }
}

#[test]
fn packed_lrc_bad_word_publishes_scratch_then_stops_before_later_payload() {
    for words in [[0x7a30, 0x3130, 0x3230], [0x3130, 0x30ff, 0x3230]] {
        let mut m = Memory::default();
        put(&mut m, 10, &words);
        m.set_word(d("D1195"), -1);
        m.native_numeric_scratch = Some(-111);
        seed_flags(&mut m);
        let (out, seen) = visit("LRC D10E0 K6 D0F7", &mut m);
        assert_eq!(out, Outcome::PlcError { offset: 1 });
        let bad = if words[0] == 0x7a30 { 0 } else { 1 };
        assert_eq!(m.native_numeric_scratch, Some(words[bad] as i16));
        assert!(seen.contains(&d(&format!("D{}", 10 + bad))));
        assert!(!seen.contains(&d(&format!("D{}", 11 + bad))));
        assert!(!seen.contains(&d("D1195")));
        assert_eq!(flags(&m), [true, false, true]);
    }
}

#[test]
fn low_byte_lrc_invalid_character_leaves_scratch_and_skips_next_character() {
    let mut m = Memory::default();
    m.set_system(1161, true);
    put(&mut m, 10, &[0xab30, 0xcd67, 0x0041, 0x0042]);
    m.native_numeric_scratch = Some(999);
    m.set_word(d("D1195"), -1);
    let (out, seen) = visit("LRC D10E0 K4 D0F7", &mut m);
    assert_eq!(out, Outcome::PlcError { offset: 1 });
    assert_eq!(m.native_numeric_scratch, Some(999));
    assert!(seen.contains(&d("D11")));
    assert!(!seen.contains(&d("D12")) && !seen.contains(&d("D1195")));
}

#[test]
fn successful_packed_lrc_publishes_last_word_and_other_modes_hold_scratch() {
    for lrc in [true, false] {
        for low in [false, true] {
            let mut m = Memory::default();
            m.set_system(1161, low);
            m.native_numeric_scratch = Some(-9);
            sample(&mut m, 10, lrc, low);
            assert_eq!(
                visit(
                    &format!(
                        "{} D10E0 K{} D100F0",
                        if lrc { "LRC" } else { "CRC" },
                        if lrc { 12 } else { 6 }
                    ),
                    &mut m
                )
                .0,
                Outcome::Complete
            );
            assert_eq!(
                m.native_numeric_scratch,
                if lrc && !low { Some(0x3630) } else { Some(-9) }
            );
        }
    }
}

#[test]
fn payload_is_complete_before_any_overlapping_result_store() {
    for lrc in [true, false] {
        for low in [false, true] {
            let mut m = Memory::default();
            m.set_system(1161, low);
            sample(&mut m, 10, lrc, low);
            let out = visit(
                &format!(
                    "{} D10E0 K{} D11F0",
                    if lrc { "LRC" } else { "CRC" },
                    if lrc { 12 } else { 6 }
                ),
                &mut m,
            )
            .0;
            assert_eq!(out, Outcome::Complete);
            assert_eq!(
                m.word(d("D11")) as u16,
                match (lrc, low) {
                    (true, false) => 0x4537,
                    (true, true) => 0x45,
                    (false, false) => 0xab66,
                    (false, true) => 0x66,
                }
            );
            if low {
                assert_eq!(m.word(d("D12")) as u16, if lrc { 0x37 } else { 0xab });
            }
        }
    }
}

#[test]
fn cached_count_can_alias_source_and_first_destination_word() {
    let mut m = Memory::default();
    put(&mut m, 40, &[2]);
    assert_eq!(visit("CRC D40E0 D40F0 D40E1", &mut m).0, Outcome::Complete);
    assert_eq!(m.word(d("D40")) as u16, 0xd000);
    let mut m = Memory::default();
    m.set_system(1161, true);
    sample(&mut m, 10, false, true);
    put(&mut m, 40, &[6, 0x7654]);
    assert_eq!(visit("CRC D10E0 D40F0 D40E1", &mut m).0, Outcome::Complete);
    assert_eq!([m.word(d("D40")), m.word(d("D41"))], [0x66, 0xab]);
}

#[test]
fn second_low_output_uses_cached_pointer_after_overwriting_own_index() {
    for lrc in [true, false] {
        for high in [false, true] {
            for bank in [0, 1, 7] {
                let alias = CalendarIndices::alias_device(bank, high).unwrap();
                let suffix = format!("{}{bank}", if high { "F" } else { "E" });
                let mut m = Memory::default();
                m.set_system(1161, true);
                sample(&mut m, 10, lrc, true);
                m.set_word(alias, 0);
                m.set_word(
                    Device {
                        index: alias.index + 1,
                        ..alias
                    },
                    0x5555,
                );
                let (out, seen) = visit(
                    &format!(
                        "{} D10 K{} {}{}",
                        if lrc { "LRC" } else { "CRC" },
                        if lrc { 12 } else { 6 },
                        alias,
                        suffix
                    ),
                    &mut m,
                );
                assert_eq!(out, Outcome::Complete);
                assert_eq!(m.word(alias), if lrc { 0x45 } else { 0x66 });
                assert_eq!(
                    m.word(Device {
                        index: alias.index + 1,
                        ..alias
                    }),
                    if lrc { 0x37 } else { 0xab }
                );
                assert!(!seen.contains(&Device {
                    index: alias.index + 0x67,
                    ..alias
                }));
            }
        }
    }
}

#[test]
fn both_model_last_words_and_negative_offset_rescue_are_mode_accurate() {
    for last in [4999u16, 9999] {
        let mut m = Memory::default();
        m.data_register_limit = Some(last);
        m.set_word(d("D1182"), -1);
        put(&mut m, last - 1, &[0x3130, 0x3330]);
        assert_eq!(
            visit(&format!("LRC D{last}E1 K4 D20"), &mut m).0,
            Outcome::Complete
        );
        assert_eq!(m.word(d("D20")) as u16, 0x4643); // 01+03 -> FC.
        put(&mut m, last, &[0x0201]);
        let (out, seen) = visit(&format!("CRC D{last}E0 K3 D30"), &mut m);
        assert_eq!(out, Outcome::Complete);
        assert_eq!(m.word(d("D30")) as u16, 0xe181);
        assert!(seen.contains(&d(&format!("D{last}"))));
        assert!(
            !seen
                .iter()
                .any(|v| v.area == crate::plc::Area::D && v.index > last)
        );
        assert_eq!(
            visit(&format!("CRC D{last}E0 K1 D31"), &mut m).0,
            Outcome::Complete
        );
        assert_eq!(m.word(d("D31")) as u16, 0x807e);
    }
}

#[test]
fn low_output_endpoint_preserves_first_store_before_error_plus5() {
    for last in [4999u16, 9999] {
        for lrc in [true, false] {
            let mut m = Memory::default();
            m.data_register_limit = Some(last);
            m.set_system(1161, true);
            sample(&mut m, 10, lrc, true);
            put(&mut m, last, &[0xffff]);
            seed_flags(&mut m);
            let op = format!(
                "{} D10E0 K{} D{last}F7",
                if lrc { "LRC" } else { "CRC" },
                if lrc { 12 } else { 6 }
            );
            let (out, seen) = visit(&op, &mut m);
            assert_eq!(out, Outcome::PlcError { offset: 5 });
            assert_eq!(
                m.word(d(&format!("D{last}"))),
                if lrc { 0x45 } else { 0x66 }
            );
            assert_eq!(m.word(d("D1068")), 5);
            assert_eq!(flags(&m), [true, false, true]);
            assert!(
                !seen
                    .iter()
                    .any(|v| v.area == crate::plc::Area::D && v.index > last)
            );
            assert_eq!(
                a(&op).qualify_native(&m).outcome,
                Outcome::PlcError { offset: 5 }
            );
        }
    }
}

#[test]
fn sticky_code_and_pc_are_independently_preserved_on_new_errors() {
    for (code, pc) in [(true, false), (false, true), (true, true)] {
        let mut m = Memory::default();
        m.set_system(1067, code);
        m.set_system(1068, pc);
        m.set_word(d("D1067"), 0x2222);
        m.set_word(d("D1068"), 123);
        seed_flags(&mut m);
        assert_eq!(
            visit("CRC D10E0 D40F0 D20E1", &mut m).0,
            Outcome::PlcError { offset: 3 }
        );
        assert_eq!(m.word(d("D1067")), if code { 0x2222 } else { 0xe1a });
        assert_eq!(m.word(d("D1068")), if pc { 123 } else { 3 });
        assert_eq!(flags(&m), [true, false, true]);
    }
}

#[test]
fn successful_checksum_holds_sticky_errors_and_alu_flags() {
    let mut m = Memory::default();
    put(&mut m, 10, &[0x0201]);
    seed_flags(&mut m);
    m.set_system(1067, true);
    m.set_system(1068, true);
    put(&mut m, 1067, &[0x2222, 123]);
    let (out, seen) = visit("CRC D10E0 K2 D20F0", &mut m);
    assert_eq!(out, Outcome::Complete);
    assert_eq!(flags(&m), [true, false, true]);
    assert_eq!([m.word(d("D1067")), m.word(d("D1068"))], [0x2222, 123]);
    assert!(!seen.contains(&Memory::system_device(1067)));
}

#[test]
fn off_skips_every_operand_and_keeps_legacy_scratch() {
    let mut m = Memory::default();
    m.set_word(d("D1028"), -1);
    m.set_word(d("D1182"), -1);
    m.set_word(d("D1195"), -1);
    m.native_numeric_scratch = Some(17);
    let mut seen = BTreeSet::new();
    assert_eq!(
        a("LRCP D0E1 D0E0 D0F7").execute(&mut m, false, &mut seen),
        Outcome::Nominal
    );
    assert!(seen.is_empty());
    assert_eq!(m.native_numeric_scratch, Some(17));
    assert!(!m.system_bit(1067));
}

#[test]
fn actual_native_capture_is_ordered_and_does_not_feed_independent_local_state() {
    let op = a("CRC D10E1 D40E0 D20F7");
    let mut local = Memory::default();
    put(&mut local, 40, &[0]);
    put(&mut local, 20, &[777]);
    local.native_numeric_scratch = Some(33);
    let mut requests = Vec::new();
    let visit = op.qualify_native_with(&local, |clone, ds| {
        for dev in ds {
            requests.push(*dev);
            match dev.to_string().as_str() {
                "D1028" => clone.set_word(*dev, 1),
                "D41" => clone.set_word(*dev, 2),
                "D1182" => clone.set_word(*dev, 1),
                "M1161" => clone.set_system(1161, false),
                "D11" => clone.set_word(*dev, 0x0201),
                "D1195" => clone.set_word(*dev, 2),
                other => panic!("unexpected capture {other}"),
            }
        }
        Ok(())
    });
    assert_eq!(visit.outcome, Outcome::Complete);
    assert_eq!(
        requests,
        [
            d("D1028"),
            d("D41"),
            d("D1182"),
            Memory::system_device(1161),
            d("D11"),
            d("D1195")
        ]
    );
    assert!(visit.observed.contains(&d("D22")));
    assert!(!visit.observed.contains(&d("D20")));
    assert_eq!(local.word(d("D20")), 777);
    assert_eq!(local.word(d("D40")), 0);
    assert_eq!(local.stored_word(d("D11")), None);
    assert_eq!(local.native_numeric_scratch, Some(33));
}

#[test]
fn native_invalid_count_shortcuts_use_actual_not_stale_local_count() {
    let mut local = Memory::default();
    put(&mut local, 40, &[2]);
    put(&mut local, 10, &[0x0201]);
    let mut requested = BTreeSet::new();
    let visit = a("CRC D0E1 D40E0 D0F7").qualify_native_with(&local, |clone, ds| {
        requested.extend(ds.iter().copied());
        for dev in ds {
            assert!(![d("D1182"), d("D1195"), Memory::system_device(1161)].contains(dev));
            if *dev == d("D40") {
                clone.set_word(*dev, 0);
            }
        }
        Ok(())
    });
    assert_eq!(visit.outcome, Outcome::PlcError { offset: 3 });
    assert_eq!(local.word(d("D40")), 2);
    assert!(requested.contains(&d("D40")));
    assert!(!local.system_bit(1067));
}

#[test]
fn native_bad_ascii_stops_capture_and_does_not_publish_scratch_to_local() {
    let mut local = Memory::default();
    put(&mut local, 10, &[0x3130, 0x3230]);
    local.native_numeric_scratch = Some(77);
    let visit = a("LRC D10E0 K4 D0F7").qualify_native_with(&local, |clone, ds| {
        for dev in ds {
            assert_ne!(*dev, d("D11"));
            assert_ne!(*dev, d("D1195"));
            if *dev == d("D10") {
                clone.set_word(*dev, 0x7a30);
            }
        }
        Ok(())
    });
    assert_eq!(visit.outcome, Outcome::PlcError { offset: 1 });
    assert_eq!(local.native_numeric_scratch, Some(77));
    assert!(!local.system_bit(1067));
}

#[test]
fn failed_capture_is_limitation_without_invented_local_plc_fault() {
    let m = Memory::default();
    let mut requests = 0;
    let visit = a("CRC D10E1 D40E0 D20F7").qualify_native_with(&m, |_, _| {
        requests += 1;
        Err("input unavailable".into())
    });
    assert_eq!(requests, 1);
    assert_eq!(
        visit.outcome,
        Outcome::Limitation("input unavailable".into())
    );
    assert!(!m.system_bit(1067));
    assert!(m.execution_limitations.is_empty());
}

#[test]
fn alias_capture_is_deduplicated_and_never_reads_old_result_storage() {
    let mut m = Memory::default();
    put(&mut m, 10, &[0x0201]);
    put(&mut m, 40, &[2]);
    let mut requests = Vec::new();
    let visit = a("CRC D10E1 D40E1 D20E1").qualify_native_with(&m, |_, ds| {
        requests.extend(ds.iter().copied());
        Ok(())
    });
    assert_eq!(visit.outcome, Outcome::Complete);
    assert_eq!(requests.iter().filter(|v| **v == d("D1182")).count(), 1);
    assert!(!requests.contains(&d("D20")));
}

#[test]
fn static_inventory_and_monitor_require_real_dynamic_count_and_mode_rows() {
    let op = a("CRC D10E1 D40F1 D4999E7");
    let mut m = Memory::default();
    m.data_register_limit = Some(4999);
    m.set_word(d("D1182"), 1);
    m.set_word(d("D1183"), 2);
    m.set_word(d("D1194"), -1);
    m.set_word(d("D42"), 3);
    let mut static_ds = BTreeSet::new();
    op.devices(&mut static_ds);
    for raw in ["D10", "D40", "D4999"] {
        assert!(!static_ds.contains(&d(raw)));
    }
    let inputs = op.monitor_input_devices(&m).unwrap();
    assert_eq!(
        inputs,
        BTreeSet::from([
            d("D1182"),
            d("D1183"),
            d("D1194"),
            d("D42"),
            Memory::system_device(1161)
        ])
    );
    let packed = op.capture_devices(&m).unwrap();
    assert!(
        packed.contains(&d("D11")) && packed.contains(&d("D12")) && packed.contains(&d("D4998"))
    );
    assert!(!packed.contains(&d("D13")) && !packed.contains(&d("D4999")));
    m.set_system(1161, true);
    let low = op.capture_devices(&m).unwrap();
    assert!(low.contains(&d("D13")) && low.contains(&d("D4999")));
    m.set_word(d("D42"), 0);
    let invalid = op.capture_devices(&m).unwrap();
    assert!(!invalid.contains(&d("D11")) && !invalid.contains(&d("D4998")));
}

#[test]
fn typed_legacy_adapter_preserves_manual_and_parser_constructed_forms() {
    for lrc in [true, false] {
        for low in [false, true] {
            let n = if lrc { 12 } else { 6 };
            let name = if lrc { "LRC" } else { "CRC" };
            let old = TextAction::parse(name, &args(&format!("D10 K{n} D30"))).unwrap();
            let wrapper = IndexedChecksumAction::from_legacy(&old).unwrap();
            let mut m = Memory::default();
            m.set_system(1161, low);
            sample(&mut m, 10, lrc, low);
            m.native_numeric_scratch = Some(111);
            let mut newer = m.clone();
            old.execute(&mut m);
            let mut seen = BTreeSet::new();
            assert_eq!(
                wrapper.execute(&mut newer, true, &mut seen),
                Outcome::Complete
            );
            assert_eq!(m.word(d("D30")), newer.word(d("D30")));
            assert_eq!(m.word(d("D31")), newer.word(d("D31")));
            assert_eq!(m.native_numeric_scratch, newer.native_numeric_scratch);
        }
    }
    let old = TextAction::Crc {
        source: d("D10"),
        count: Operand::Constant(0),
        dest: d("D20"),
    };
    let op = IndexedChecksumAction::from_legacy(&old).unwrap();
    let mut m = Memory::default();
    let mut seen = BTreeSet::new();
    assert_eq!(
        op.execute(&mut m, true, &mut seen),
        Outcome::PlcError { offset: 3 }
    );
}

#[test]
fn public_vm_all_four_forms_keep_seven_words_and_outer_pulse_rearm() {
    for name in ["LRC", "LRCP", "CRC", "CRCP"] {
        let lrc = name.starts_with("LRC");
        let n = if lrc { 2 } else { 1 };
        let p = compile_il(&format!(
            "LD X0\n{name} D10E0 K{n} D20F0\nINC D300\nOUT Y0\nEND"
        ))
        .unwrap();
        assert!(p.requires_vm());
        let encoded = crate::es2_encoder::encode(&p).unwrap();
        assert_eq!(encoded.action_pc[1], encoded.action_pc[0] + 7);
        let mut sim = Simulator::default();
        sim.memory.set_word(d("D20"), 1234);
        for (on, input, lrc_normal, lrc_pulse, crc_normal, crc_pulse, markers) in [
            (false, 0, 1234, 1234, 1234, 1234, 0),
            (true, 1, 0x4646, 0x4646, 0x807e, 0x807e, 1),
            (true, 2, 0x4645, 0x4646, 0x813e, 0x807e, 2),
            (false, 2, 0x4645, 0x4646, 0x813e, 0x807e, 2),
            (true, 2, 0x4645, 0x4645, 0x813e, 0x813e, 3),
        ] {
            sim.memory.set_input(d("X0"), on);
            sim.memory.set_word(
                d("D10"),
                if lrc {
                    ((0x30 + input) << 8) | 0x30
                } else {
                    input
                },
            );
            complete(&mut sim, &p);
            let expected = match (lrc, name.ends_with('P')) {
                (true, false) => lrc_normal,
                (true, true) => lrc_pulse,
                (false, false) => crc_normal,
                (false, true) => crc_pulse,
            };
            assert_eq!(sim.memory.word(d("D20")) as u16, expected as u16, "{name}");
            assert_eq!(sim.memory.word(d("D300")), markers);
            assert_eq!(sim.memory.bit(d("Y0")), on);
        }
    }
}

#[test]
fn public_vm_error_pc_uses_original_header_and_successor_marker_executes() {
    for (body, offset) in [
        ("CRC D10E1 D40F1 D20E7", 3),
        ("LRC D10E1 K2 D20F7", 1),
        ("CRC D10E1 K2 D0F7", 5),
    ] {
        let p = compile_il(&format!("LD X0\n{body}\nINC D300\nEND")).unwrap();
        let encoded = crate::es2_encoder::encode(&p).unwrap();
        assert_eq!(encoded.action_pc[0], 1);
        let mut sim = Simulator::default();
        sim.memory.set_input(d("X0"), true);
        sim.memory.set_word(d("D1195"), -1);
        complete(&mut sim, &p);
        assert_eq!(sim.memory.word(d("D1068")), 1 + offset);
        assert_eq!(sim.memory.word(d("D300")), 1);
    }
}

#[test]
fn public_vm_same_scan_modifiers_and_mode_changes_are_live() {
    let p = compile_il(
        "LD X0\nMOV K1 E1\nCRC D10E1 K2 D20E1\nSET M1161\nMOV K2 E1\nCRC D10E1 K2 D20E1\nEND",
    )
    .unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    put(&mut sim.memory, 11, &[0x0201, 1, 2]);
    complete(&mut sim, &p);
    assert_eq!(sim.memory.word(d("D21")) as u16, 0xe181);
    assert_eq!(
        [sim.memory.word(d("D22")), sim.memory.word(d("D23"))],
        [0x81, 0xe1]
    );
}

#[test]
fn public_vm_cj_and_master_false_skip_all_checksum_operands() {
    let p=compile_il("LD X1\nCJ P0\nLD X0\nCRC D0E1 D0E0 D0F7\nP0\nLD X7\nMC N0\nLDI X7\nLRC D0E1 D0E0 D0F7\nMCR N0\nLDI X7\nMOV K1234 D300\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim.memory.set_input(d("X1"), true);
    for raw in ["D1028", "D1182", "D1195"] {
        sim.memory.set_word(d(raw), -1);
    }
    complete(&mut sim, &p);
    assert_eq!(sim.memory.word(d("D300")), 1234);
    assert!(!sim.memory.system_bit(1067));
    for raw in ["D1028", "D1182", "D1195"] {
        assert!(!sim.observed_devices.contains(&d(raw)));
    }
}

#[test]
fn isolated_vm_gate_ignores_output_context_but_obeys_master() {
    for name in ["CRC", "CRCP"] {
        for (master, output, active) in [(1, false, true), (0, true, false)] {
            let p = compile_il(&format!("LD X0\n{name} D10E0 K2 D20F0\nEND")).unwrap();
            let mut sim = Simulator::default();
            sim.memory.set_input(d("X0"), true);
            put(&mut sim.memory, 10, &[0x0201]);
            put(&mut sim.memory, 20, &[1234]);
            sim.begin_plan_scan(&p, 0, true).unwrap();
            sim.execute_plan_step(&p).unwrap();
            assert_eq!(sim.current_plan_pc(), Some(1));
            sim.control.master_gate = master;
            sim.control.out_enabled = output;
            sim.execute_plan_step(&p).unwrap();
            assert_eq!(sim.current_plan_pc(), Some(8));
            assert_eq!(
                sim.memory.word(d("D20")) as u16,
                if active { 0xe181 } else { 1234 }
            );
            assert_eq!(sim.observed_devices.contains(&d("D10")), active);
        }
    }
}
