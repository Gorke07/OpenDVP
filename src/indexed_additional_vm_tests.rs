//! Fixed oracles and ordered capture tests; parent owns Cargo/native admission.
use crate::additional_processing::{AdditionalAction, CompareTableProfile as Profile};
use crate::calendar_operand::CalendarIndices;
use crate::indexed_additional::{AdditionalOutcome as Outcome, IndexedAdditionalAction};
use crate::plc::{Area, Device, Memory, Program, ScanStatus, Simulator, compile_il};
use std::collections::BTreeSet;

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn args(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(str::to_owned).collect()
}
fn a(raw: &str) -> IndexedAdditionalAction {
    let words = args(raw);
    IndexedAdditionalAction::parse(&words[0], &words[1..]).unwrap()
}
fn run(raw: &str, m: &mut Memory) -> (Outcome, BTreeSet<Device>) {
    let mut seen = BTreeSet::new();
    let out = a(raw).execute(m, true, &mut seen);
    (out, seen)
}
fn put(m: &mut Memory, base: u16, values: &[i16]) {
    for (i, v) in values.iter().enumerate() {
        m.set_word(
            Device {
                area: Area::D,
                index: base + i as u16,
            },
            *v,
        );
    }
}
fn wide(m: &mut Memory, base: u16, values: &[i32]) {
    for (i, v) in values.iter().enumerate() {
        m.set_double_word(
            Device {
                area: Area::D,
                index: base + 2 * i as u16,
            },
            *v,
        );
    }
}
fn flags(m: &Memory) -> [bool; 3] {
    [1020, 1021, 1022].map(|i| m.system_bit(i))
}
fn seed_flags(m: &mut Memory) {
    m.set_system(1020, true);
    m.set_system(1021, false);
    m.set_system(1022, true);
}
fn complete(s: &mut Simulator, p: &Program) {
    s.scan(p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(
        s.memory.execution_limitations.is_empty(),
        "{:?}",
        s.memory.execution_limitations
    );
}
const FORMS: [&str; 14] = [
    "WSUM", "WSUMP", "DWSUM", "DWSUMP", "SCAL", "SCALP", "SCLP", "SCLPP", "DSCLP", "DSCLPP",
    "CMPT", "CMPTP", "DCMPT", "DCMPTP",
];

#[test]
fn all_fourteen_headers_and_original_operand_widths_are_literal() {
    for (name, header, len, body) in [
        ("WSUM", 0x8401, 7, "D5E1 D40F7 D20E0"),
        ("WSUMP", 0x8411, 7, "D5E1 D40F7 D20E0"),
        ("DWSUM", 0x8501, 13, "D5E1 D40F7 D20E0"),
        ("DWSUMP", 0x8511, 13, "D5E1 D40F7 D20E0"),
        ("SCAL", 0xb201, 9, "D5E1 D10F7 D15E0 D20F0"),
        ("SCALP", 0xb211, 9, "D5E1 D10F7 D15E0 D20F0"),
        ("SCLP", 0xb401, 7, "D5E1 D40F7 D20E0"),
        ("SCLPP", 0xb411, 7, "D5E1 D40F7 D20E0"),
        ("DSCLP", 0xb501, 13, "D5E1 D40F7 D20E0"),
        ("DSCLPP", 0xb511, 13, "D5E1 D40F7 D20E0"),
        ("CMPT", 0xb801, 9, "D5E1 D10F7 D40E0 D20F0"),
        ("CMPTP", 0xb811, 9, "D5E1 D10F7 D40E0 D20F0"),
        ("DCMPT", 0xb901, 17, "D5E1 D10F7 D40E0 D20F0"),
        ("DCMPTP", 0xb911, 17, "D5E1 D10F7 D40E0 D20F0"),
    ] {
        let op = a(&format!("{name} {body}"));
        let w = op.native_words().unwrap();
        assert_eq!(w[0], header);
        assert_eq!(&w[1..3], &[0x0ae6, 0x8086]);
        assert_eq!(w.len(), len);
        assert_eq!(usize::from(op.nominal_words()), len);
        assert_eq!(op.label(), format!("{name} {body}"));
        if name.starts_with('D') {
            assert_eq!(&w[3..5], &[0x0080, 0x0080]);
        }
    }
}

#[test]
fn role_matrix_rejects_white_suffixes_direct_indices_and_wrong_areas() {
    for raw in [
        "WSUM K1 K2 D20",
        "WSUM K1M0 K2 D20",
        "WSUM E1 K2 D20",
        "WSUM D0 T0 D20",
        "WSUM D0 F1 D20",
        "WSUM D0 K2E0 D20",
        "WSUM D0 K1 K1M0",
        "SCAL T0 K1 K2 D20",
        "SCAL D0 E1 K2 D20",
        "SCAL K1M0 K1 K2 D20",
        "SCAL K1 K1 K2 T0",
        "SCLP E1 D20 D30",
        "SCLP D0 K1M0 D30",
        "DSCLP D0 D20 F1",
        "CMPT K1 D20 K1 D30",
        "CMPT K1M0 D20 K1 D30",
        "CMPT C200 D20 K1 D30",
        "DCMPT C200E0 D20 K1 D30",
        "CMPT D0 D20 T0 D30",
        "DCMPT D0 D20 K1 K8X0",
        "CMPT D0 D20 K1 K5M128",
        "DCMPT D0 D20 K1 K9M128",
    ] {
        let words = args(raw);
        assert!(
            IndexedAdditionalAction::parse(&words[0], &words[1..]).is_err(),
            "{raw}"
        );
    }
    for raw in [
        "DWSUM C255E0 D9999F7 C255F0",
        "WSUM C199E0 K-1 C200F7",
        "DSCLP D9999E0 D9999F0 D9999E7",
        "DCMPT C199E0 T255F7 D9999E1 K8M128F0",
        "CMPT D0 D20 HFFFF K1Y200E0",
    ] {
        assert!(a(raw).native_words().is_ok(), "{raw}");
    }
    assert!(!IndexedAdditionalAction::supports("DSCAL"));
}

#[test]
fn every_bank_and_all_roles_use_signed_word_indices_even_for_double_forms() {
    for high in [false, true] {
        for bank in 0..8 {
            let suffix = format!("{}{bank}", if high { 'F' } else { 'E' });
            let delta = if bank % 2 == 0 { -1 } else { 1 };
            for name in FORMS {
                let mut m = Memory::default();
                m.set_word(CalendarIndices::alias_device(bank, high).unwrap(), delta);
                if high {
                    let x = CalendarIndices::alias_device(bank, true).unwrap();
                    m.set_word(
                        Device {
                            index: x.index + 1,
                            ..x
                        },
                        0x4000,
                    );
                }
                let idx = |base: i16| (base + delta) as u16;
                let is_wide = name.starts_with('D');
                let (body, expected) = if name.contains("WSUM") {
                    if is_wide {
                        wide(&mut m, idx(10), &[10, 20]);
                        wide(&mut m, idx(50), &[2]);
                    } else {
                        put(&mut m, idx(10), &[10, 20]);
                        put(&mut m, idx(50), &[2]);
                    }
                    (format!("D10{suffix} D50{suffix} D100{suffix}"), 30)
                } else if name.starts_with("SCAL") {
                    put(&mut m, idx(10), &[-3]);
                    put(&mut m, idx(20), &[500]);
                    put(&mut m, idx(30), &[7]);
                    (
                        format!("D10{suffix} D20{suffix} D30{suffix} D100{suffix}"),
                        6,
                    )
                } else if name.contains("SCLP") {
                    if is_wide {
                        wide(&mut m, idx(10), &[50]);
                        wide(&mut m, idx(20), &[100, 0, 1000, 0]);
                    } else {
                        put(&mut m, idx(10), &[50]);
                        put(&mut m, idx(20), &[100, 0, 1000, 0]);
                    }
                    (format!("D10{suffix} D20{suffix} D100{suffix}"), 500)
                } else {
                    if is_wide {
                        wide(&mut m, idx(10), &[-1, 2, 3]);
                        wide(&mut m, idx(20), &[0, 2, 1]);
                        wide(&mut m, idx(50), &[3]);
                    } else {
                        put(&mut m, idx(10), &[-1, 2, 3]);
                        put(&mut m, idx(20), &[0, 2, 1]);
                        put(&mut m, idx(50), &[3]);
                    }
                    (
                        format!("D10{suffix} D20{suffix} D50{suffix} D100{suffix}"),
                        2,
                    )
                };
                seed_flags(&mut m);
                assert_eq!(run(&format!("{name} {body}"), &mut m).0, Outcome::Complete);
                let target = Device {
                    area: Area::D,
                    index: idx(100),
                };
                assert_eq!(
                    if is_wide || name.contains("WSUM") {
                        m.double_word(target)
                    } else {
                        i32::from(m.word(target))
                    },
                    expected,
                    "{name}/{suffix}"
                );
                assert_eq!(flags(&m), [true, false, true]);
            }
        }
    }
}

#[test]
fn word_sum_sign_extension_count_clamp_and_wrapping_double_sum_are_literal() {
    for (n, expected, scratch) in [
        (-1, 32767, 11),
        (0, 32767, 11),
        (1, 32767, 11),
        (2, 32768, 12),
    ] {
        let mut m = Memory::default();
        put(&mut m, 10, &[32767, 1]);
        assert_eq!(
            run(&format!("WSUM D10E0 K{n} D20F0"), &mut m).0,
            Outcome::Complete
        );
        assert_eq!(m.double_word(d("D20")), expected);
        assert_eq!(m.native_numeric_scratch, Some(scratch));
    }
    let mut m = Memory::default();
    wide(&mut m, 10, &[i32::MAX, 1]);
    assert_eq!(run("DWSUM D10E0 K2 D20F0", &mut m).0, Outcome::Complete);
    assert_eq!(m.double_word(d("D20")), i32::MIN);
    assert_eq!(m.native_numeric_scratch, Some(14));
}

#[test]
fn sum_invalid_count_returns_zero_clamps_one_and_target_clears_transient_fault() {
    for (name, offset) in [("WSUM", 3), ("DWSUM", 5)] {
        let mut m = Memory::default();
        wide(&mut m, 10, &[123]);
        m.set_word(d("D1028"), -1);
        let (out, seen) = run(&format!("{name} D10E1 D0E0 D20F0"), &mut m);
        assert_eq!(out, Outcome::PlcError { offset });
        assert_eq!(m.double_word(d("D20")), 123);
        assert!(seen.contains(&d("D1029")) && seen.contains(&d("D10")));
        assert_eq!(m.word(d("D1068")), offset as i16);
        assert_eq!(
            m.native_numeric_scratch,
            Some(if name == "WSUM" { 11 } else { 12 })
        );
    }
}

#[test]
fn sum_initial_source_and_target_errors_have_distinct_observations_and_scratch() {
    let mut m = Memory::default();
    m.set_word(d("D1028"), -1);
    m.native_numeric_scratch = Some(777);
    let (out, seen) = run("WSUM D0E0 D40F0 D20E1", &mut m);
    assert_eq!(out, Outcome::PlcError { offset: 1 });
    assert_eq!(m.native_numeric_scratch, Some(-1));
    assert!(!seen.contains(&d("D1029")));
    assert!(!seen.contains(&d("D1182")));
    let mut m = Memory::default();
    m.set_word(d("D1029"), -1);
    put(&mut m, 10, &[333]);
    let (out, seen) = run("WSUM D10E0 K2 D0F0", &mut m);
    assert_eq!(out, Outcome::PlcError { offset: 5 });
    assert_eq!(m.native_numeric_scratch, Some(10));
    assert!(!seen.contains(&d("D10")));
    m.set_word(d("D1028"), 1);
    assert_eq!(
        run("WSUM C199E0 K1 D20", &mut m).0,
        Outcome::PlcError { offset: 1 }
    );
    assert_eq!(m.native_numeric_scratch, Some(200));
}

#[test]
fn sum_model_partial_lows_stop_without_extent_error_and_publish_next_cursor() {
    for last in [4999, 9999] {
        let mut m = Memory::default();
        m.data_register_limit = Some(last);
        m.set_word(
            Device {
                area: Area::D,
                index: last,
            },
            -1,
        );
        assert_eq!(
            run(&format!("DWSUM D{last}E0 K64 D20F0"), &mut m).0,
            Outcome::Complete
        );
        assert_eq!(m.double_word(d("D20")), 65535);
        assert_eq!(m.native_numeric_scratch, Some((last + 2) as i16));
        put(&mut m, 10, &[-1, -1]);
        assert_eq!(
            run(&format!("DWSUM D10E0 K1 D{last}F0"), &mut m).0,
            Outcome::Complete
        );
        assert_eq!(
            m.word(Device {
                area: Area::D,
                index: last
            }),
            -1
        );
        assert!(!m.system_bit(1067));
    }
    let mut m = Memory::default();
    m.set_word(d("T255"), -1);
    assert_eq!(run("DWSUM T255E0 K9 D20F0", &mut m).0, Outcome::Complete);
    assert_eq!(m.double_word(d("D20")), 65535);
    assert_eq!(m.native_numeric_scratch, Some(257));
}

#[test]
fn sum_counter_199_to_natural_200_and_result_layout_are_separate() {
    let mut m = Memory::default();
    m.set_word(d("C199"), -1);
    m.set_double_word(d("C200"), 2);
    assert_eq!(run("DWSUM C199E0 K2 D20F0", &mut m).0, Outcome::Complete);
    assert_eq!(m.double_word(d("D20")), 65537);
    assert_eq!(m.native_numeric_scratch, Some(201));
    wide(&mut m, 10, &[0x12345678]);
    assert_eq!(run("DWSUM D10E0 K1 C199F0", &mut m).0, Outcome::Complete);
    assert_eq!(m.word(d("C199")) as u16, 0x5678);
    assert_eq!(m.double_word(d("C200")), 2);
    assert_eq!(run("WSUM D10E0 K1 C200F0", &mut m).0, Outcome::Complete);
    assert_eq!(m.double_word(d("C200")), 0x5678);
}

#[test]
fn sum_cached_target_keeps_its_high_word_after_overwriting_own_modifier() {
    let mut m = Memory::default();
    put(&mut m, 10, &[32767, -32768]);
    m.set_word(d("D1184"), 777);
    assert_eq!(run("WSUM D10E3 K2 D1182E1", &mut m).0, Outcome::Complete);
    assert_eq!(m.word(d("D1182")), -1);
    assert_eq!(m.word(d("D1183")), -1);
    assert_eq!(m.word(d("D1184")), 777);
}

#[test]
fn scal_source_faults_continue_except_last_reader_fault_skips_target_modifier() {
    for (body, expected, offset, target_seen, scratch) in [
        ("SCAL D0E0 K500 K7 D20F0", 7, 1, true, 0),
        ("SCAL K3 D0E0 K7 D20F0", 7, 3, true, 3),
        ("SCAL K3 K500 D0E0 D20F0", 777, 5, false, 3),
    ] {
        let mut m = Memory::default();
        m.set_word(d("D1028"), -1);
        m.set_word(d("D20"), 777);
        let (out, seen) = run(body, &mut m);
        assert_eq!(out, Outcome::PlcError { offset });
        assert_eq!(m.word(d("D20")), expected);
        assert_eq!(seen.contains(&d("D1029")), target_seen);
        assert_eq!(m.native_numeric_scratch, Some(scratch));
    }
}

#[test]
fn scal_clamps_final_word_after_truncation_and_publishes_raw_first_source() {
    for (body, expected, raw) in [
        ("SCAL K-3 K500 K7 D20E0", 6, -3),
        ("SCAL K32767 K32767 K32767 D20E0", 32767, 32767),
        ("SCAL K-32768 K32767 K-32768 D20E0", -32768, -32768),
    ] {
        let mut m = Memory::default();
        seed_flags(&mut m);
        assert_eq!(run(body, &mut m).0, Outcome::Complete);
        assert_eq!(m.word(d("D20")), expected);
        assert_eq!(m.native_numeric_scratch, Some(raw));
        assert_eq!(flags(&m), [true, false, true]);
    }
}

#[test]
fn source_error_overlay_is_visible_to_later_scal_source_without_native_recapture() {
    let mut local = Memory::default();
    local.set_word(d("D1028"), -1);
    local.set_word(d("D1067"), 777);
    let mut fetched = BTreeSet::new();
    let visit = a("SCAL K1000 D0E0 D1067 D20F0").qualify_native_with(&local, |m, ds| {
        for v in ds {
            assert!(fetched.insert(*v));
            if *v == d("D1067") {
                m.set_word(*v, 777);
            }
        }
        Ok(())
    });
    assert_eq!(visit.outcome, Outcome::PlcError { offset: 3 });
    assert_eq!(local.word(d("D20")), 0);
    assert_eq!(local.word(d("D1067")), 777);
    let (out, _) = run("SCAL K1000 D0E0 D1067 D20F0", &mut local);
    assert_eq!(out, Outcome::PlcError { offset: 3 });
    assert_eq!(local.word(d("D20")), 0xe1a);
}

#[test]
fn sclp_zero_reversed_and_ordinary_spans_use_literal_integer_oracles() {
    for (table, source, expected) in [
        ([100, 0, 1000, 0], 50, 500),
        ([0, 100, 1000, 0], 50, 1000),
        ([0, 100, 1000, 0], -50, 0),
        ([2, 0, 3, 0], 1, 1),
    ] {
        let mut m = Memory::default();
        put(&mut m, 20, &table);
        assert_eq!(
            run(&format!("SCLP K{source} D20E0 D100F0"), &mut m).0,
            Outcome::Complete
        );
        assert_eq!(m.word(d("D100")), expected);
    }
    let mut m = Memory::default();
    put(&mut m, 20, &[1, 1, 100, 0]);
    m.set_word(d("D100"), 777);
    let (out, seen) = run("SCLP K1 D20E0 D100F0", &mut m);
    assert_eq!(out, Outcome::PlcError { offset: 3 });
    assert_eq!(m.word(d("D100")), 777);
    assert!(!seen.contains(&d("D1029")));
}

#[test]
fn source_fault_word_sclp_continues_but_double_returns_before_parameter_binding() {
    let mut m = Memory::default();
    m.set_word(d("D1028"), -1);
    put(&mut m, 20, &[100, 0, 1000, 10]);
    let (out, seen) = run("SCLP D0E0 D20E1 D100F0", &mut m);
    assert_eq!(out, Outcome::PlcError { offset: 1 });
    assert_eq!(m.word(d("D100")), 10);
    assert!(seen.contains(&d("D1182")) && seen.contains(&d("D1029")));
    let mut m = Memory::default();
    m.set_word(d("D1028"), -1);
    let (out, seen) = run("DSCLP D0E0 D20E1 D100F0", &mut m);
    assert_eq!(out, Outcome::PlcError { offset: 1 });
    assert!(!seen.contains(&d("D1182")));
    assert!(!seen.contains(&d("M1162")));
    assert!(!seen.contains(&d("D1029")));
}

#[test]
fn scale_parameter_tables_are_model_bounded_before_payload_and_target() {
    for (name, offset, size) in [("SCLP", 3, 4), ("DSCLP", 5, 8)] {
        for last in [4999, 9999] {
            let mut m = Memory::default();
            m.data_register_limit = Some(last);
            m.set_word(d("D1028"), 1);
            let start = last - size + 1;
            let (out, seen) = run(&format!("{name} K1 D{start}E0 D100F0"), &mut m);
            assert_eq!(out, Outcome::PlcError { offset });
            assert!(!seen.contains(&Device {
                area: Area::D,
                index: start + 1
            }));
            assert!(!seen.contains(&d("D1029")));
            assert!(!seen.contains(&d("M1162")));
        }
    }
}

#[test]
fn double_points_keeps_integer_truncation_and_float_fraction_store_boundaries() {
    let mut m = Memory::default();
    wide(&mut m, 20, &[2, 0, 3, 0]);
    assert_eq!(run("DSCLP K1 D20E0 D100F0", &mut m).0, Outcome::Complete);
    assert_eq!(m.double_word(d("D100")), 1);
    m.set_system(1162, true);
    wide(&mut m, 20, &[0x40000000, 0, 0x40400000, 0]);
    wide(&mut m, 10, &[0x3f800000]);
    assert_eq!(run("DSCLP D10E1 D20E0 D100F0", &mut m).0, Outcome::Complete);
    assert_eq!(m.double_word(d("D100")) as u32, 0x3fc00000);
    m.set_double_word(d("D20"), 0x7fc00000);
    let (out, seen) = run("DSCLP D10E1 D20E0 D100F0", &mut m);
    assert_eq!(out, Outcome::PlcError { offset: 5 });
    assert!(!seen.contains(&d("D1029")));
}

#[test]
fn point_table_aliases_are_fully_read_before_a_cached_two_word_result() {
    let mut m = Memory::default();
    wide(&mut m, 20, &[100, 0, 1000, 0]);
    m.set_word(d("D1184"), 777);
    assert_eq!(run("DSCLP K50 D20E0 D1182E1", &mut m).0, Outcome::Complete);
    assert_eq!(m.double_word(d("D1182")), 500);
    assert_eq!(m.word(d("D1184")), 777);
    let mut m = Memory::default();
    put(&mut m, 20, &[100, 0, 1000, 0]);
    assert_eq!(run("SCLP K50 D20E0 D21F0", &mut m).0, Outcome::Complete);
    assert_eq!(m.word(d("D21")), 500);
}

#[test]
fn comparison_relations_counts_and_ignored_upper_count_word_are_literal() {
    for name in ["CMPT", "DCMPT"] {
        for relation in 0..=5 {
            let mut m = Memory::default();
            if name == "CMPT" {
                put(&mut m, 10, &[-1, 2, 3]);
                put(&mut m, 20, &[0, 2, 1]);
            } else {
                wide(&mut m, 10, &[-1, 2, 3]);
                wide(&mut m, 20, &[0, 2, 1]);
            }
            let count = (relation << 8) | 3;
            let literal = if name == "DCMPT" {
                format!("H1234{count:04X}")
            } else {
                format!("H{count:04X}")
            };
            assert_eq!(
                run(&format!("{name} D10E0 D20F0 {literal} D100E1"), &mut m).0,
                Outcome::Complete
            );
            assert_eq!(m.word(d("D100")), [2, 1, 3, 4, 6, 2][relation as usize]);
            assert_eq!(
                m.native_numeric_scratch,
                Some(if name == "CMPT" { 13 } else { 16 })
            );
        }
    }
}

#[test]
fn comparison_unordered_and_mode_behavior_are_profile_specific() {
    for profile in [Profile::CurrentEs2, Profile::InstalledLegacy] {
        for relation in 0..=4 {
            let mut m = Memory::default();
            m.compare_table_profile = profile;
            m.set_system(1162, true);
            wide(&mut m, 10, &[0x7fc00000]);
            wide(&mut m, 20, &[0x3f800000]);
            let (out, seen) = run(
                &format!("DCMPT D10E0 D20F0 H{:04X} D100E1", (relation << 8) | 1),
                &mut m,
            );
            assert_eq!(out, Outcome::Complete);
            let expected = if profile == Profile::CurrentEs2 {
                [1, 1, 1, 0, 0]
            } else {
                [0, 0, 0, 1, 1]
            };
            assert_eq!(m.word(d("D100")), expected[relation]);
            assert_eq!(seen.contains(&d("M1162")), profile == Profile::CurrentEs2);
        }
    }
}

#[test]
fn comparison_left_cursor_advances_before_right_extent_failure() {
    for profile in [Profile::CurrentEs2, Profile::InstalledLegacy] {
        let mut m = Memory::default();
        m.compare_table_profile = profile;
        put(&mut m, 10, &[1, 2]);
        m.set_word(d("T255"), 1);
        m.set_word(d("D100"), 777);
        let (out, seen) = run("CMPT D10E0 T255F0 K2 D100E1", &mut m);
        assert_eq!(
            out,
            if profile == Profile::CurrentEs2 {
                Outcome::PlcError { offset: 5 }
            } else {
                Outcome::Complete
            }
        );
        assert_eq!(m.native_numeric_scratch, Some(12));
        assert!(seen.contains(&d("D11")));
        assert_eq!(m.word(d("D100")), 777);
        assert!(!seen.contains(&d("D1182")));
    }
}

#[test]
fn missing_double_high_has_exact_partial_left_cursor_in_each_profile() {
    for profile in [Profile::CurrentEs2, Profile::InstalledLegacy] {
        for last in [4999, 9999] {
            let mut m = Memory::default();
            m.compare_table_profile = profile;
            m.data_register_limit = Some(last);
            m.set_word(
                Device {
                    area: Area::D,
                    index: last,
                },
                -1,
            );
            wide(&mut m, 20, &[65535]);
            m.set_word(d("D100"), 777);
            let (out, seen) = run(&format!("DCMPT D{last}E0 D20F0 K1 D100E1"), &mut m);
            assert_eq!(
                out,
                if profile == Profile::CurrentEs2 {
                    Outcome::PlcError { offset: 9 }
                } else {
                    Outcome::Complete
                }
            );
            assert_eq!(
                m.native_numeric_scratch,
                Some((last + if profile == Profile::CurrentEs2 { 1 } else { 2 }) as i16)
            );
            assert_eq!(
                m.word(d("D100")),
                if profile == Profile::CurrentEs2 {
                    777
                } else {
                    1
                }
            );
            assert_eq!(
                seen.contains(&d("D20")),
                profile == Profile::InstalledLegacy
            );
        }
    }
}

#[test]
fn double_right_failure_keeps_completed_left_progress_and_counter199_is_unsigned() {
    let mut m = Memory::default();
    wide(&mut m, 10, &[1]);
    m.set_word(d("T255"), 1);
    let (out, seen) = run("DCMPT D10E0 T255F0 K1 D100E1", &mut m);
    assert_eq!(out, Outcome::PlcError { offset: 9 });
    assert_eq!(m.native_numeric_scratch, Some(12));
    assert!(seen.contains(&d("T255")));
    m.set_word(d("C199"), -1);
    m.set_double_word(d("C200"), 2);
    wide(&mut m, 20, &[65535, 2]);
    assert_eq!(
        run("DCMPT C199E0 D20F0 K2 D100E1", &mut m).0,
        Outcome::Complete
    );
    assert_eq!(m.word(d("D100")), 3);
    assert_eq!(m.native_numeric_scratch, Some(201));
}

#[test]
fn comparison_invalid_count_continues_only_on_legacy_profile() {
    for profile in [Profile::CurrentEs2, Profile::InstalledLegacy] {
        for name in ["CMPT", "DCMPT"] {
            let mut m = Memory::default();
            m.compare_table_profile = profile;
            wide(&mut m, 10, &[1]);
            wide(&mut m, 20, &[1]);
            m.set_word(d("D1028"), -1);
            m.set_word(d("D100"), 777);
            let (out, seen) = run(&format!("{name} D10E1 D20F1 D0E0 D100F0"), &mut m);
            assert_eq!(
                out,
                Outcome::PlcError {
                    offset: if name == "CMPT" { 5 } else { 9 }
                }
            );
            assert_eq!(
                m.word(d("D100")),
                if profile == Profile::CurrentEs2 {
                    777
                } else {
                    1
                }
            );
            assert_eq!(
                seen.contains(&d("D10")),
                profile == Profile::InstalledLegacy
            );
            assert_eq!(
                seen.contains(&d("D1029")),
                profile == Profile::InstalledLegacy
            );
        }
    }
}

#[test]
fn invalid_initial_comparison_sources_are_current_errors_but_legacy_native_limitations() {
    for bad_left in [true, false] {
        let body = if bad_left {
            "CMPT D0E0 D20E1 D40F1 D100F0"
        } else {
            "CMPT D10E1 D0E0 D40F1 D100F0"
        };
        let mut m = Memory::default();
        m.set_word(d("D1028"), -1);
        let (out, seen) = run(body, &mut m);
        assert_eq!(
            out,
            Outcome::PlcError {
                offset: if bad_left { 1 } else { 3 }
            }
        );
        assert!(!seen.contains(&d("D1183")));
        assert!(!seen.contains(&d("D1029")));
        m.compare_table_profile = Profile::InstalledLegacy;
        assert!(matches!(
            a(body).qualify_native(&m).outcome,
            Outcome::Limitation(_)
        ));
    }
}

#[test]
fn current_checked_kn_writer_supports_canonical_low_m_and_both_bank_crossings() {
    for (base, offset) in [("M1532", 0), ("M1536", -4)] {
        let mut m = Memory::default();
        put(&mut m, 10, &[1, 1, 1, 1]);
        put(&mut m, 20, &[1, 1, 1, 1]);
        m.set_word(d("D1028"), offset);
        assert_eq!(
            run(&format!("CMPT D10E1 D20F1 K4 K2{base}E0"), &mut m).0,
            Outcome::Complete
        );
        for i in 0..8 {
            assert_eq!(
                m.bit(Device {
                    area: Area::M,
                    index: 1532 + i
                }),
                i < 4
            );
        }
        m.compare_table_profile = Profile::InstalledLegacy;
        assert!(matches!(
            a(&format!("CMPT D10E1 D20F1 K4 K2{base}E0"))
                .qualify_native(&m)
                .outcome,
            Outcome::Limitation(_)
        ));
    }
}

#[test]
fn legacy_direct_and_indexed_generic_low_m_mapping_are_equally_unqualified() {
    for target in ["K1M1532", "K1M1532E0"] {
        let mut m = Memory::default();
        m.compare_table_profile = Profile::InstalledLegacy;
        assert!(matches!(
            a(&format!("CMPT D10E1 D20F1 K1 {target}"))
                .qualify_native(&m)
                .outcome,
            Outcome::Limitation(_)
        ));
    }
}

#[test]
fn checked_atomic_group_failure_differs_from_legacy_partial_store() {
    for profile in [Profile::CurrentEs2, Profile::InstalledLegacy] {
        let mut m = Memory::default();
        m.compare_table_profile = profile;
        m.set_word(d("D1028"), 4);
        put(&mut m, 10, &[1]);
        put(&mut m, 20, &[2]);
        for i in 1020..=1023 {
            m.set_internal(
                Device {
                    area: Area::S,
                    index: i,
                },
                true,
            );
        }
        let (out, _) = run("CMPT D10E1 D20F1 K1 K2S1016E0", &mut m);
        assert_eq!(out, Outcome::PlcError { offset: 7 });
        for i in 1020..=1023 {
            assert_eq!(
                m.bit(Device {
                    area: Area::S,
                    index: i
                }),
                profile == Profile::CurrentEs2
            );
        }
    }
}

#[test]
fn result_width_truncation_flags_and_neighbor_preservation_are_literal() {
    for (name, digits) in [("CMPT", 1), ("CMPT", 4), ("DCMPT", 1), ("DCMPT", 8)] {
        let mut m = Memory::default();
        seed_flags(&mut m);
        let wide_op = name.starts_with('D');
        if wide_op {
            wide(&mut m, 10, &[1, 2, 3]);
            wide(&mut m, 30, &[1, 2, 0]);
        } else {
            put(&mut m, 10, &[1, 2, 3]);
            put(&mut m, 30, &[1, 2, 0]);
        }
        m.set_internal(d("M127"), true);
        m.set_internal(
            Device {
                area: Area::M,
                index: 128 + digits * 4,
            },
            true,
        );
        assert_eq!(
            run(&format!("{name} D10E0 D30F0 K3 K{digits}M128E1"), &mut m).0,
            Outcome::Complete
        );
        for i in 0..digits * 4 {
            assert_eq!(
                m.bit(Device {
                    area: Area::M,
                    index: 128 + i
                }),
                i < 2
            );
        }
        assert!(m.bit(d("M127")));
        assert!(m.bit(Device {
            area: Area::M,
            index: 128 + digits * 4
        }));
        assert_eq!(flags(&m), [true, false, true]);
    }
}

#[test]
fn comparison_numeric_writer_caches_own_index_and_rejects_missing_high_atomically() {
    let mut m = Memory::default();
    wide(&mut m, 10, &[1, 2]);
    wide(&mut m, 20, &[1, 2]);
    m.set_word(d("D1184"), 777);
    assert_eq!(
        run("DCMPT D10E0 D20F0 K2 D1182E1", &mut m).0,
        Outcome::Complete
    );
    assert_eq!(m.double_word(d("D1182")), 3);
    assert_eq!(m.word(d("D1184")), 777);
    m.set_word(d("D1182"), 0);
    m.set_word(d("T255"), 777);
    assert_eq!(
        run("DCMPT D10E0 D20F0 K1 T255E1", &mut m).0,
        Outcome::PlcError { offset: 13 }
    );
    assert_eq!(m.word(d("T255")), 777);
}

#[test]
fn native_capture_follows_sum_binding_order_and_does_not_commit_reference_data() {
    let mut local = Memory::default();
    put(&mut local, 10, &[1, 2]);
    local.set_word(d("D40"), 1);
    local.native_numeric_scratch = Some(777);
    let mut trace = Vec::new();
    let visit = a("WSUM D10E0 D40F0 D100E1").qualify_native_with(&local, |m, ds| {
        trace.extend(ds.iter().copied());
        if ds.contains(&d("D40")) {
            m.set_word(d("D40"), 2);
        }
        Ok(())
    });
    assert_eq!(visit.outcome, Outcome::Complete);
    let at = |s| trace.iter().position(|v| *v == d(s)).unwrap();
    assert!(
        at("D1028") < at("D1029")
            && at("D1029") < at("D40")
            && at("D40") < at("D1182")
            && at("D1182") < at("D10")
    );
    assert!(trace.contains(&d("D11")));
    assert_eq!(local.word(d("D100")), 0);
    assert_eq!(local.word(d("D40")), 1);
    assert_eq!(local.native_numeric_scratch, Some(777));
}

#[test]
fn failed_capture_is_not_a_plc_error_and_power_off_reads_nothing() {
    let op = a("SCAL D10E0 K1000 K0 D100F0");
    let mut m = Memory::default();
    m.native_numeric_scratch = Some(123);
    let mut seen = BTreeSet::new();
    assert_eq!(op.execute(&mut m, false, &mut seen), Outcome::Nominal);
    assert!(seen.is_empty());
    assert_eq!(m.native_numeric_scratch, Some(123));
    let visit = op.qualify_native_with(&m, |_, ds| {
        if ds.contains(&d("D10")) {
            Err("input missing".into())
        } else {
            Ok(())
        }
    });
    assert_eq!(visit.outcome, Outcome::Limitation("input missing".into()));
    assert!(!visit.observed.contains(&d("D1029")));
    assert!(!m.system_bit(1067));
}

#[test]
fn monitors_require_actual_full_count_rows_and_partial_static_sum_is_not_widened() {
    let mut m = Memory::default();
    let op = a("DWSUM D4999E0 D40F0 D4999E1");
    assert_eq!(
        op.monitor_input_devices(&m).unwrap(),
        BTreeSet::from([d("D1028"), d("D1029"), d("D1182"), d("D40"), d("D41")])
    );
    let mut ds = BTreeSet::new();
    a("DWSUM D4999 K1 D4999").devices(&mut ds);
    assert!(ds.contains(&d("D4999")));
    assert!(!ds.contains(&d("D5000")));
    let table = a("DCMPT D10E0 D20F0 D40E1 D100E2");
    assert!(
        table
            .monitor_input_devices(&m)
            .unwrap()
            .contains(&d("M1162"))
    );
    m.compare_table_profile = Profile::InstalledLegacy;
    assert!(
        !table
            .monitor_input_devices(&m)
            .unwrap()
            .contains(&d("M1162"))
    );
}

#[test]
fn legacy_typed_adapter_and_public_ordinary_execution_share_new_scratch_contracts() {
    for (raw, expected, scratch) in [
        ("WSUM D10 K2 D100", 3, 12),
        ("SCAL K-3 K500 K7 D100", 6, -3),
        ("CMPT D10 D20 K2 D100", 1, 12),
    ] {
        let words = args(raw);
        let old = AdditionalAction::parse(&words[0], &words[1..]).unwrap();
        let wrap = IndexedAdditionalAction::from_legacy(&old).unwrap();
        let mut m = Memory::default();
        put(&mut m, 10, &[1, 2]);
        put(&mut m, 20, &[1, 3]);
        let mut copy = m.clone();
        old.execute(&mut m);
        assert_eq!(
            wrap.execute(&mut copy, true, &mut BTreeSet::new()),
            Outcome::Complete
        );
        assert_eq!(m.word(d("D100")), expected);
        assert_eq!(copy.word(d("D100")), expected);
        assert_eq!(m.native_numeric_scratch, Some(scratch));
        assert_eq!(copy.native_numeric_scratch, Some(scratch));
    }
}

#[test]
fn public_vm_all_fourteen_forms_keep_nominal_pc_and_outer_pulse_rearm() {
    for name in FORMS {
        let is_wide = name.starts_with('D');
        let pulse = matches!(
            name,
            "WSUMP" | "DWSUMP" | "SCALP" | "SCLPP" | "DSCLPP" | "CMPTP" | "DCMPTP"
        );
        let body = if name.contains("WSUM") {
            format!("{name} D10E0 K1 D100F0")
        } else if name.starts_with("SCAL") {
            format!("{name} D10E0 K1000 K0 D100F0")
        } else if name.contains("SCLP") {
            format!("{name} D10E0 D20F0 D100E1")
        } else {
            format!("{name} D10E0 D20F0 K1 D100E1")
        };
        let p = compile_il(&format!("LD X0\n{body}\nINC D300\nEND")).unwrap();
        assert!(p.requires_vm());
        let encoded = crate::es2_encoder::encode(&p).unwrap();
        assert_eq!(
            encoded.action_pc[1],
            encoded.action_pc[0] + a(&body).nominal_words()
        );
        let mut sim = Simulator::default();
        sim.memory.set_word(d("D100"), 777);
        if name.contains("SCLP") {
            if is_wide {
                wide(&mut sim.memory, 20, &[100, 0, 100, 0]);
            } else {
                put(&mut sim.memory, 20, &[100, 0, 100, 0]);
            }
        } else if name.contains("CMPT") {
            wide(&mut sim.memory, 20, &[1]);
        }
        for (on, input, logical, markers) in [
            (false, 1, 777, 0),
            (true, 1, 1, 1),
            (true, 2, if pulse { 1 } else { 2 }, 2),
            (false, 2, if pulse { 1 } else { 2 }, 2),
            (true, 2, 2, 3),
        ] {
            sim.memory.set_input(d("X0"), on);
            if is_wide {
                sim.memory.set_double_word(d("D10"), input);
            } else {
                sim.memory.set_word(d("D10"), input as i16);
            }
            complete(&mut sim, &p);
            let expected = if logical == 777 {
                777
            } else if name.contains("CMPT") {
                i16::from(logical == 1)
            } else {
                logical
            };
            assert_eq!(sim.memory.word(d("D100")), expected, "{name}");
            assert_eq!(sim.memory.word(d("D300")), markers);
        }
    }
}

#[test]
fn public_vm_first_error_pc_can_coexist_with_successful_result_and_successor() {
    for (body, offset, expected) in [
        ("WSUM D10E1 D0E0 D100F0", 3, 123),
        ("SCAL D0E0 K1000 K7 D100F0", 1, 7),
        ("SCLP D0E0 D20E1 D100F0", 1, 10),
        ("DCMPT D10E1 D20F1 D0E0 D100F0", 9, 777),
    ] {
        let p = compile_il(&format!("LD X0\n{body}\nMOV K1234 D300\nEND")).unwrap();
        let mut sim = Simulator::default();
        sim.memory.set_input(d("X0"), true);
        sim.memory.set_word(d("D1028"), -1);
        wide(&mut sim.memory, 10, &[123]);
        put(&mut sim.memory, 20, &[100, 0, 1000, 10]);
        sim.memory.set_word(d("D100"), 777);
        complete(&mut sim, &p);
        assert_eq!(sim.memory.word(d("D1068")), 1 + offset);
        assert_eq!(sim.memory.word(d("D100")), expected);
        assert_eq!(sim.memory.word(d("D300")), 1234);
    }
}

#[test]
fn public_vm_for_same_pc_pulse_and_skipped_cj_preserve_operand_visits() {
    let p=compile_il("LD X1\nCJ P0\nLD X0\nSCAL D0E7 K1 K0 D0F7\nP0\nLD X0\nFOR K3\nLD X0\nDWSUMP D10E0 K1 D100F0\nINC D300\nNEXT\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim.memory.set_input(d("X1"), true);
    sim.memory.set_word(d("D1194"), -1);
    sim.memory.set_word(d("D1195"), -1);
    wide(&mut sim.memory, 10, &[123]);
    complete(&mut sim, &p);
    assert_eq!(sim.memory.double_word(d("D100")), 123);
    assert_eq!(sim.memory.word(d("D300")), 3);
    assert!(!sim.observed_devices.contains(&d("D1194")));
    assert!(!sim.observed_devices.contains(&d("D1195")));
}

#[test]
fn public_vm_master_false_clears_pulse_history_and_output_context_is_not_a_gate() {
    let p = compile_il("LD X1\nMC N0\nLDI X7\nSCALP D10E0 K1000 K0 D100F0\nMCR N0\nEND").unwrap();
    let mut sim = Simulator::default();
    for (on, input, expected) in [
        (false, 1, 0),
        (true, 1, 1),
        (true, 2, 1),
        (false, 2, 1),
        (true, 2, 2),
    ] {
        sim.memory.set_input(d("X1"), on);
        sim.memory.set_word(d("D10"), input);
        complete(&mut sim, &p);
        assert_eq!(sim.memory.word(d("D100")), expected);
    }
    let p = compile_il("LD X0\nWSUM D10E0 K1 D100F0\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim.memory.set_word(d("D10"), 123);
    sim.begin_plan_scan(&p, 0, true).unwrap();
    sim.execute_plan_step(&p).unwrap();
    sim.control.out_enabled = false;
    sim.control.master_gate = 1;
    sim.execute_plan_step(&p).unwrap();
    assert_eq!(sim.current_plan_pc(), Some(8));
    assert_eq!(sim.memory.double_word(d("D100")), 123);
}
