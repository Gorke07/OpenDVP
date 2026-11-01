//! Independent fixed calendar/time contracts. Tail profiles/PC execution are
//! tested by the parent integration; these tests never run a native process.
use crate::indexed_address::IndexWord;
use crate::indexed_calendar_time::{CalendarTimeOutcome as Outcome, IndexedCalendarTimeAction};
use crate::plc::{Area, Device, Memory, ScanStatus, Simulator, compile_il};
use std::collections::BTreeSet;

fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn args(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}
fn a(s: &str) -> IndexedCalendarTimeAction {
    let v = args(s);
    IndexedCalendarTimeAction::parse(&v[0], &v[1..]).unwrap()
}
fn rejected(s: &str) -> bool {
    let v = args(s);
    IndexedCalendarTimeAction::parse(&v[0], &v[1..]).is_err()
}
fn visit(s: &str, m: &mut Memory) -> (Outcome, BTreeSet<Device>) {
    let mut seen = BTreeSet::new();
    let outcome = a(s).execute(m, true, &mut seen);
    (outcome, seen)
}
fn at(base: Device, offset: u16) -> Device {
    Device {
        index: base.index + offset,
        ..base
    }
}
fn put(m: &mut Memory, base: &str, values: [i16; 3]) {
    for (i, value) in values.into_iter().enumerate() {
        m.set_word(at(d(base), i as u16), value);
    }
}
fn get(m: &Memory, base: &str) -> [i16; 3] {
    [0, 1, 2].map(|i| m.word(at(d(base), i)))
}
fn flags(m: &Memory) -> [bool; 3] {
    [1020, 1021, 1022].map(|i| m.system_bit(i))
}
fn seed_flags(m: &mut Memory, values: [bool; 3]) {
    for (i, value) in values.into_iter().enumerate() {
        m.set_system(1020 + i as u16, value);
    }
}
fn bits(m: &mut Memory, base: &str, width: u16, value: u16) {
    for i in 0..width {
        m.set_snapshot_bit(at(d(base), i), i < 16 && value & (1u16 << i) != 0);
    }
}
fn read_bits(m: &Memory, base: &str, width: u16) -> u16 {
    (0..width).fold(0, |v, i| v | (u16::from(m.bit(at(d(base), i))) << i))
}
fn hydrate(dst: &mut Memory, src: &Memory, ds: &BTreeSet<Device>) {
    for &dev in ds {
        match dev.area {
            Area::X | Area::Y | Area::M | Area::S => dst.set_snapshot_bit(dev, src.bit(dev)),
            _ => dst.set_word(dev, src.word(dev)),
        }
    }
}
fn complete(sim: &mut Simulator, p: &crate::plc::Program) {
    sim.scan(p, 0);
    assert_eq!(sim.last_scan, ScanStatus::Complete);
    assert!(
        sim.memory.execution_limitations.is_empty(),
        "{:?}",
        sim.memory.execution_limitations
    );
}

#[test]
fn eight_exact_spellings_preserve_original_nominal_words_and_labels() {
    for (name, body, header, len) in [
        ("TCMP", "D0E0 D10F1 D20E7 D30F3 M80E2", 0x6001, 11),
        ("TZCP", "D0E0 D10F1 D20E7 M80E2", 0x6201, 9),
        ("TADD", "D0E0 D10F1 D20E7", 0x6401, 7),
        ("TSUB", "D0E0 D10F1 D20E7", 0x6601, 7),
    ] {
        for p in ["", "P"] {
            let source = format!("{name}{p} {body}");
            let action = a(&source);
            let w = action.native_words().unwrap();
            assert_eq!(w.len(), len);
            assert_eq!(usize::from(action.nominal_words()), len);
            assert_eq!(w[0], header | if p.is_empty() { 0 } else { 0x10 });
            assert_eq!(action.label(), source);
            assert_eq!(action.is_arithmetic(), matches!(name, "TADD" | "TSUB"));
        }
    }
    for invalid in ["DTCMP", "DTZCP", "DTADD", "DTSUB", "TCMPPP", "TZCPPP"] {
        assert!(!IndexedCalendarTimeAction::supports(invalid));
    }
}

#[test]
fn original_four_arithmetic_tail_words_remain_unmodified_and_result_uses_width_zero() {
    assert_eq!(
        a("TADD D0E0 D123F0 D125F7").native_words().unwrap(),
        [0x6401, 0x00e6, 0x0086, 0xf6f6, 0x0086, 0xfaf6, 0x8089]
    );
    assert_eq!(
        a("TSUBP D0E0 D124F0 D126F0").native_words().unwrap(),
        [0x6611, 0x00e6, 0x0086, 0xf8f6, 0x0086, 0xfcf6, 0x0086]
    );
    for source in ["TCMP K0 K0 K0 D0E0 M80F7", "TZCP D0E0 D10 D20 Y200E1"] {
        let w = a(source).native_words().unwrap();
        assert_eq!(w.last().unwrap() & 0xff, 0x80);
    }
}

#[test]
fn triples_scalar_white_roles_and_original_counter_bank_are_distinct() {
    for source in [
        "TCMP E0 F7 H3B T253E1 S128F1",
        "TCMP K4X0E0 C199F0 T255E1 C197 M80",
        "TZCP T255E0 C199F0 D9999E7 Y0F7",
        "TADD C199E0 T255F0 D9999E1",
    ] {
        assert!(!rejected(source), "{source}");
    }
    for source in [
        "TCMP X0 K0 K0 D0E0 M80",
        "TCMP K5M0E0 K0 K0 D0 M80",
        "TCMP K0E0 K0 K0 D0 M80",
        "TCMP K0 K0 K0 E0 M80",
        "TCMP K0 K0 K0 D0 X0E0",
        "TZCP K0 D0 D10 M80E0",
        "TZCP D0 K1M0 D10 M80E0",
        "TADD F0 D0E0 D10",
        "TSUB D0E0 D10 K1M80",
        "TADD C200E0 D0 D10",
        "TADD D0 C200F7 D10",
        "TADD D0 D10 C200E0",
    ] {
        assert!(rejected(source), "{source}");
    }
}

#[test]
fn all_sixteen_signed_word_modifiers_cover_all_fifteen_gray_roles() {
    for bank in 0..8 {
        for family in ['E', 'F'] {
            let modifier = format!("{family}{bank}");
            let alias = IndexWord::parse(&modifier).unwrap().device();
            let mut m = Memory::default();
            m.set_word(alias, -1);
            put(&mut m, "D0", [4, 5, 6]);
            put(&mut m, "D10", [1, 2, 3]);
            put(&mut m, "D20", [4, 5, 6]);
            m.set_word(d("D40"), 4);
            m.set_word(d("D50"), 5);
            m.set_word(d("D60"), 6);
            let body = format!(
                "TCMP D41{modifier} D51{modifier} D61{modifier} D1{modifier} M81{modifier}"
            );
            assert_eq!(visit(&body, &mut m).0, Outcome::Nominal);
            assert_eq!(read_bits(&m, "M80", 3), 2);
            let body = format!("TZCP D1{modifier} D11{modifier} D21{modifier} M85{modifier}");
            assert_eq!(visit(&body, &mut m).0, Outcome::Nominal);
            assert_eq!(read_bits(&m, "M84", 3), 2);
            for (name, expected) in [("TADD", [5, 7, 9]), ("TSUB", [3, 3, 3])] {
                let body = format!("{name} D1{modifier} D11{modifier} D101{modifier}");
                assert_eq!(visit(&body, &mut m).0, Outcome::ArithmeticSuccess);
                assert_eq!(get(&m, "D100"), expected);
            }
            assert_eq!(m.word(alias), -1);
        }
    }
}

#[test]
fn tcmp_group_widths_areas_and_unaligned_offsets_zero_extend() {
    for area in ["X", "Y", "M", "S"] {
        for digits in 1..=4 {
            let mut m = Memory::default();
            m.set_word(d("D1028"), 1);
            // Every group supplies decimal 5; first/last guard must not count.
            bits(&mut m, &format!("{area}0"), digits * 4 + 2, 0);
            bits(&mut m, &format!("{area}1"), digits * 4, 5);
            m.set_bit(d(&format!("{area}0")), true);
            m.set_bit(at(d(&format!("{area}1")), digits * 4), true);
            put(&mut m, "D20", [5, 5, 5]);
            let v = format!("K{digits}{area}0E0");
            let body = format!("TCMP {v} {v} {v} D20 M80");
            assert_eq!(visit(&body, &mut m).0, Outcome::Nominal);
            assert_eq!(read_bits(&m, "M80", 3), 2);
        }
    }
}

#[test]
fn direct_white_index_values_are_words_and_not_implicit_pairs() {
    let mut m = Memory::default();
    m.set_word(d("D1028"), 7);
    m.set_word(d("D1029"), 59);
    m.set_word(d("D1195"), 58);
    m.set_word(d("D1194"), -12345);
    put(&mut m, "D20", [7, 59, 58]);
    assert_eq!(visit("TCMP E0 F0 F7 D20 M80E1", &mut m).0, Outcome::Nominal);
    assert_eq!(read_bits(&m, "M80", 3), 2);
}

#[test]
fn tcmp_three_results_keep_adjacent_guards_and_all_alu_flags() {
    for base in ["Y200", "M80", "S128"] {
        for (hour, expected) in [(11, 1), (12, 2), (13, 4)] {
            let mut m = Memory::default();
            seed_flags(&mut m, [true, true, true]);
            put(&mut m, "D20", [hour, 30, 45]);
            bits(&mut m, base, 4, 15);
            assert_eq!(
                visit(&format!("TCMP K12 K30 K45 D20E0 {base}F0"), &mut m).0,
                Outcome::Nominal
            );
            assert_eq!(read_bits(&m, base, 4), expected | 8);
            assert_eq!(flags(&m), [true, true, true]);
        }
    }
}

#[test]
fn tcmp_scalar_error_priority_skips_later_indices_and_target() {
    for (body, offset, absent) in [
        (
            "TCMP K24 D0E1 D10F1 D20E2 M80F2",
            1,
            vec!["D1182", "D1183", "D1184", "D1185"],
        ),
        (
            "TCMP K23 K60 D10F1 D20E2 M80F2",
            3,
            vec!["D1183", "D1184", "D1185"],
        ),
        ("TCMP K23 K59 HFFFF D20E2 M80F2", 5, vec!["D1184", "D1185"]),
    ] {
        let mut m = Memory::default();
        for i in 1182..=1185 {
            m.set_word(
                Device {
                    area: Area::D,
                    index: i,
                },
                -30000,
            );
        }
        bits(&mut m, "M80", 3, 5);
        seed_flags(&mut m, [true, false, true]);
        let (out, seen) = visit(body, &mut m);
        assert_eq!(out, Outcome::PlcError { offset });
        assert_eq!(m.word(d("D1067")), 0xe1a);
        assert_eq!(m.word(d("D1068")), offset as i16);
        for dev in absent {
            assert!(!seen.contains(&d(dev)), "{body}: {dev}");
        }
        assert_eq!(read_bits(&m, "M80", 3), 5);
        assert_eq!(flags(&m), [true, false, true]);
    }
}

#[test]
fn tcmp_block_first_invalid_field_stops_validation_and_target_resolution() {
    for offset in 0..3 {
        let mut m = Memory::default();
        put(&mut m, "D20", [12, 30, 40]);
        m.set_word(at(d("D20"), offset), if offset == 0 { 24 } else { 60 });
        m.set_word(d("D1182"), -30000);
        let (out, seen) = visit("TCMP K12 K30 K40 D20E0 M80E1", &mut m);
        assert_eq!(out, Outcome::PlcError { offset: 7 });
        assert!(!seen.contains(&d("D1182")));
        for later in offset + 1..3 {
            assert!(!seen.contains(&at(d("D20"), later)));
        }
        assert_eq!(m.word(d("D1068")), 7);
    }
}

#[test]
fn tzcp_reversed_bounds_are_symmetric_and_inclusive_not_midnight_intervals() {
    for (hour, expected) in [(5, 1), (6, 2), (12, 2), (18, 2), (19, 4)] {
        let mut m = Memory::default();
        put(&mut m, "D0", [18, 0, 0]);
        put(&mut m, "D10", [6, 0, 0]);
        put(&mut m, "D20", [hour, 0, 0]);
        bits(&mut m, "M80", 4, 15);
        seed_flags(&mut m, [true, false, true]);
        assert_eq!(
            visit("TZCP D0E0 D10F0 D20E1 M80F1", &mut m).0,
            Outcome::Nominal
        );
        assert_eq!(read_bits(&m, "M80", 4), expected | 8);
        assert_eq!(flags(&m), [true, false, true]);
    }
}

#[test]
fn tzcp_invalid_low_high_current_have_exact_priority_and_late_target() {
    for bad_role in 0..3usize {
        let mut m = Memory::default();
        for base in ["D0", "D10", "D20"] {
            put(&mut m, base, [12, 30, 40]);
        }
        m.set_word(d(["D0", "D10", "D20"][bad_role]), 24);
        for alias in ["D1182", "D1183", "D1184"].into_iter().skip(bad_role + 1) {
            m.set_word(d(alias), -30000);
        }
        let (out, seen) = visit("TZCP D0E0 D10E1 D20F1 M80E2", &mut m);
        assert_eq!(
            out,
            Outcome::PlcError {
                offset: 1 + bad_role as u16 * 2
            }
        );
        assert!(!seen.contains(&d("D1184")));
        for later in ["D10", "D20"].into_iter().skip(bad_role) {
            assert!(!seen.contains(&d(later)));
        }
    }
}

#[test]
fn arithmetic_wraps_day_with_literal_flags_and_fresh_success_despite_sticky_error() {
    for (name, l, r, expected, flag) in [
        (
            "TADD",
            [23, 59, 59],
            [0, 0, 1],
            [0, 0, 0],
            [true, true, true],
        ),
        (
            "TSUB",
            [0, 0, 0],
            [0, 0, 1],
            [23, 59, 59],
            [false, true, true],
        ),
    ] {
        let mut m = Memory::default();
        put(&mut m, "D0", l);
        put(&mut m, "D10", r);
        seed_flags(&mut m, [false, true, true]);
        m.operation_error(0xe19, 46);
        let saved = (m.word(d("D1067")), m.word(d("D1068")));
        assert_eq!(
            visit(&format!("{name} D0E0 D10F0 D20E1"), &mut m).0,
            Outcome::ArithmeticSuccess
        );
        assert_eq!(get(&m, "D20"), expected);
        assert_eq!(flags(&m), flag);
        assert_eq!((m.word(d("D1067")), m.word(d("D1068"))), saved);
    }
}

#[test]
fn tadd_progressive_overlapping_reads_are_not_a_triple_snapshot() {
    let mut m = Memory::default();
    put(&mut m, "D2", [1, 2, 3]);
    put(&mut m, "D10", [4, 5, 6]);
    m.set_word(d("D1"), 777);
    m.set_word(d("D0"), 888);
    m.set_word(d("D5"), 999);
    assert_eq!(
        visit("TADD D2E0 D10F0 D1E1", &mut m).0,
        Outcome::ArithmeticSuccess
    );
    assert_eq!(get(&m, "D1"), [18, 14, 9]);
    assert_eq!(m.word(d("D0")), 888);
    assert_eq!(m.word(d("D5")), 999);
}

#[test]
fn tsub_late_invalid_hour_keeps_seconds_and_minutes_but_all_final_flags_hold() {
    let mut m = Memory::default();
    put(&mut m, "D2", [10, 20, 30]);
    put(&mut m, "D10", [1, 2, 3]);
    m.set_word(d("D1"), 777);
    seed_flags(&mut m, [true, false, true]);
    let (out, seen) = visit("TSUB D2E0 D10F0 D1E1", &mut m);
    assert_eq!(out, Outcome::PlcError { offset: 3 });
    assert_eq!(get(&m, "D1"), [777, 25, 27]);
    assert_eq!(flags(&m), [true, false, true]);
    assert_eq!(m.word(d("D1067")), 0xe1a);
    assert_eq!(m.word(d("D1068")), 3);
    assert!(seen.contains(&d("D10")));
    assert!(!seen.contains(&d("D1")));
}

#[test]
fn both_seconds_are_read_before_rejection_and_later_fields_are_not() {
    let mut m = Memory::default();
    put(&mut m, "D0", [1, 2, 60]);
    put(&mut m, "D10", [3, 4, 5]);
    put(&mut m, "D20", [7, 8, 9]);
    let (out, seen) = visit("TADD D0E0 D10F0 D20E1", &mut m);
    assert_eq!(out, Outcome::PlcError { offset: 3 });
    assert_eq!(get(&m, "D20"), [7, 8, 9]);
    for yes in ["D1028", "D1029", "D1182", "D2", "D12"] {
        assert!(seen.contains(&d(yes)));
    }
    for no in ["D0", "D1", "D10", "D11", "D20", "D21", "D22"] {
        assert!(!seen.contains(&d(no)));
    }
}

#[test]
fn minute_error_preserves_only_prior_seconds_store() {
    let mut m = Memory::default();
    put(&mut m, "D0", [1, 60, 59]);
    put(&mut m, "D10", [3, 4, 2]);
    put(&mut m, "D20", [7, 8, 9]);
    seed_flags(&mut m, [true, true, false]);
    let (out, seen) = visit("TADD D0E0 D10F0 D20E1", &mut m);
    assert_eq!(out, Outcome::PlcError { offset: 3 });
    assert_eq!(get(&m, "D20"), [7, 8, 1]);
    assert_eq!(flags(&m), [true, true, false]);
    assert!(!seen.contains(&d("D0")));
    assert!(!seen.contains(&d("D10")));
}

#[test]
fn arithmetic_resolves_all_three_modifiers_before_any_payload_even_on_field_error() {
    let mut m = Memory::default();
    put(&mut m, "D0", [1, 2, 60]);
    m.set_word(d("D1182"), -30000);
    let (out, seen) = visit("TADD D0E0 D10F0 D20E1", &mut m);
    assert!(matches!(out, Outcome::Limitation(_)));
    assert_eq!(seen, BTreeSet::from([d("D1028"), d("D1029"), d("D1182")]));
    assert!(!m.system_bit(1067));
    assert_eq!(get(&m, "D20"), [0, 0, 0]);
}

#[test]
fn cached_target_triple_survives_seconds_store_to_its_own_index() {
    let mut m = Memory::default();
    put(&mut m, "D0", [1, 2, 3]);
    put(&mut m, "D10", [4, 5, 6]);
    m.set_word(d("D1182"), 0);
    m.set_word(d("D1189"), 777);
    assert_eq!(
        visit("TADD D0E0 D10F0 D1180E1", &mut m).0,
        Outcome::ArithmeticSuccess
    );
    assert_eq!(get(&m, "D1180"), [5, 7, 9]);
    assert_eq!(m.word(d("D1189")), 777);
}

#[test]
fn cached_source_triple_survives_result_seconds_overwriting_its_modifier() {
    let mut m = Memory::default();
    put(&mut m, "D0", [1, 2, 3]);
    put(&mut m, "D10", [4, 5, 6]);
    m.set_word(d("D1182"), 0);
    put(&mut m, "D9", [18, 4, 5]);
    m.set_word(d("D12"), 6);
    // E1 becomes 9 at the first store; the left source must remain D0, not D9.
    assert_eq!(
        visit("TADD D0E1 D10F0 D1180E0", &mut m).0,
        Outcome::ArithmeticSuccess
    );
    assert_eq!(get(&m, "D1180"), [5, 7, 9]);
}

#[test]
fn native_clone_uses_actual_source_fields_but_does_not_commit_results() {
    let mut local = Memory::default();
    put(&mut local, "D0", [1, 2, 3]);
    put(&mut local, "D10", [4, 5, 6]);
    let mut actual = local.clone();
    put(&mut actual, "D0", [24, 2, 3]);
    let action = a("TADD D0E0 D10F0 D20E1");
    let mut dry = local.clone();
    assert_eq!(
        action.execute(&mut dry, true, &mut BTreeSet::new()),
        Outcome::ArithmeticSuccess
    );
    let q = action.qualify_native_with(&local, |clone, ds| {
        hydrate(clone, &actual, ds);
        Ok(())
    });
    assert_eq!(q.outcome, Outcome::PlcError { offset: 3 });
    assert_eq!(get(&local, "D0"), [1, 2, 3]);
    assert_eq!(get(&local, "D20"), [0, 0, 0]);
    assert!(!local.system_bit(1067));
    assert!(q.observed.contains(&d("D21")));
    assert!(q.observed.contains(&d("D22")));
    assert!(!q.observed.contains(&d("D20")));
}

#[test]
fn native_clone_never_recaptures_earlier_payload_writes_over_live_aliases() {
    let mut actual = Memory::default();
    put(&mut actual, "D2", [10, 20, 30]);
    put(&mut actual, "D10", [1, 2, 3]);
    actual.set_word(d("D1"), 777);
    let local = Memory::default();
    let mut fetched = BTreeSet::<Device>::new();
    let q = a("TSUB D2E0 D10F0 D1E1").qualify_native_with(&local, |clone, ds| {
        fetched.extend(ds);
        hydrate(clone, &actual, ds);
        Ok(())
    });
    assert_eq!(q.outcome, Outcome::PlcError { offset: 3 });
    assert!(!fetched.contains(&d("D3")));
    assert!(!fetched.contains(&d("D2")));
    assert!(q.observed.contains(&d("D3")));
    assert!(q.observed.contains(&d("D2")));
    assert_eq!(get(&local, "D1"), [0, 0, 0]);
}

#[test]
fn native_clone_marks_same_value_stores_written_before_later_capture() {
    let mut actual = Memory::default();
    put(&mut actual, "D2", [1, 3, 3]);
    put(&mut actual, "D10", [4, 5, 0]);
    let mut fetched = BTreeSet::<Device>::new();
    let q = a("TADD D2E0 D10F0 D1E1").qualify_native_with(&actual, |clone, ds| {
        fetched.extend(ds);
        hydrate(clone, &actual, ds);
        Ok(())
    });
    assert_eq!(q.outcome, Outcome::ArithmeticSuccess);
    assert!(!fetched.contains(&d("D3")));
    assert!(!fetched.contains(&d("D2")));
    let mut independent = actual.clone();
    visit("TADD D2E0 D10F0 D1E1", &mut independent);
    assert_eq!(get(&independent, "D1"), [12, 8, 3]);
}

#[test]
fn capture_failure_has_fresh_limitation_not_plc_error_and_never_mutates_local() {
    let mut local = Memory::default();
    put(&mut local, "D0", [1, 2, 3]);
    put(&mut local, "D10", [4, 5, 6]);
    let q = a("TADD D0E0 D10F0 D20E1").qualify_native_with(&local, |_, ds| {
        if ds.contains(&d("D11")) {
            Err("read unavailable".into())
        } else {
            Ok(())
        }
    });
    assert_eq!(q.outcome, Outcome::Limitation("read unavailable".into()));
    assert!(q.observed.contains(&d("D22")));
    assert!(!q.observed.contains(&d("D21")));
    assert_eq!(get(&local, "D20"), [0, 0, 0]);
    assert!(!local.system_bit(1067));
    assert!(local.execution_limitations.is_empty());
}

#[test]
fn tcmp_clock_guard_occurs_before_later_role_and_seeded_values_remain_meaningful() {
    use crate::system_relays::{ClockProfile, ClockState};
    let action = a("TCMP K1M1011E0 K0 K0 D20F0 M80E1");
    let mut m = Memory::default();
    m.system_relays.set_profile(ClockProfile::InstalledFixed);
    let mut seen = BTreeSet::new();
    let out = action.execute(&mut m, true, &mut seen);
    assert!(matches!(out, Outcome::Limitation(_)));
    assert!(seen.contains(&d("M1011")));
    assert!(!seen.contains(&d("D1029")));
    assert!(!seen.contains(&d("D1182")));
    assert!(!m.system_bit(1067));
    for (profile, hour) in [
        (ClockProfile::Documented, 0),
        (ClockProfile::InstalledFixed, 5),
    ] {
        let mut m = Memory::default();
        m.system_relays.set_profile(profile);
        if profile == ClockProfile::InstalledFixed {
            ClockState::seed_installed_levels(&mut m, [true, false, true, false]).unwrap();
        }
        put(&mut m, "D20", [hour, 0, 0]);
        assert_eq!(
            action.execute(&mut m, true, &mut BTreeSet::new()),
            Outcome::Nominal
        );
        assert_eq!(read_bits(&m, "M80", 3), 2);
    }
}

#[test]
fn off_visit_skips_clock_indices_fields_flags_and_scratch_for_all_eight_forms() {
    for (stem, body) in [
        ("TCMP", "K1M1011E0 K0 K0 D20F0 M80E1"),
        ("TZCP", "D0E0 D10F0 D20E1 M80F1"),
        ("TADD", "D0E0 D10F0 D20E1"),
        ("TSUB", "D0E0 D10F0 D20E1"),
    ] {
        for p in ["", "P"] {
            let mut m = Memory::default();
            seed_flags(&mut m, [true, true, true]);
            m.native_numeric_scratch = Some(321);
            for alias in ["D1028", "D1029", "D1182", "D1183"] {
                m.set_word(d(alias), -30000);
            }
            let mut seen = BTreeSet::new();
            assert_eq!(
                a(&format!("{stem}{p} {body}")).execute(&mut m, false, &mut seen),
                Outcome::Nominal
            );
            assert!(seen.is_empty());
            assert!(m.execution_limitations.is_empty());
            assert_eq!(flags(&m), [true, true, true]);
            assert_eq!(m.native_numeric_scratch, Some(321));
        }
    }
}

#[test]
fn complete_t_c_and_model_d_endpoints_allow_negative_lexical_rescue() {
    for (lexical, effective, last) in [
        ("T255", "T253", 9999),
        ("C199", "C197", 9999),
        ("D4999", "D4997", 4999),
        ("D9999", "D9997", 9999),
    ] {
        let mut m = Memory::default();
        m.data_register_limit = Some(last);
        m.set_word(d("D1028"), -2);
        m.set_word(d("D1029"), -2);
        m.set_word(d("D1182"), -2);
        put(&mut m, effective, [1, 2, 3]);
        let action = a(&format!("TADD {lexical}E0 {lexical}F0 {lexical}E1"));
        assert_eq!(
            action.execute(&mut m, true, &mut BTreeSet::new()),
            Outcome::ArithmeticSuccess
        );
        assert_eq!(get(&m, effective), [2, 4, 6]);
        assert_eq!(
            action.qualify_native(&m).outcome,
            Outcome::ArithmeticSuccess
        );
    }
}

#[test]
fn incomplete_effective_triples_fail_closed_without_zero_fill_or_plc_error() {
    for (source, last) in [
        ("T255E0", 9999),
        ("C199E0", 9999),
        ("D4999E0", 4999),
        ("D9999E0", 9999),
    ] {
        let mut m = Memory::default();
        m.data_register_limit = Some(last);
        put(&mut m, "D20", [7, 8, 9]);
        let action = a(&format!("TADD {source} D10F0 D20E1"));
        let mut seen = BTreeSet::new();
        assert!(matches!(
            action.execute(&mut m, true, &mut seen),
            Outcome::Limitation(_)
        ));
        assert_eq!(get(&m, "D20"), [7, 8, 9]);
        assert!(!m.system_bit(1067));
        assert!(matches!(
            action.qualify_native(&m).outcome,
            Outcome::Limitation(_)
        ));
    }
}

#[test]
fn static_direct_triple_inventory_contains_full_span_but_modified_bases_do_not() {
    let action = a("TADD D4998 D10E0 D20F0");
    let mut ds = BTreeSet::new();
    action.devices(&mut ds);
    for dev in ["D4998", "D4999", "D5000", "D1028", "D1029"] {
        assert!(ds.contains(&d(dev)));
    }
    for dev in ["D10", "D11", "D12", "D20", "D21", "D22"] {
        assert!(!ds.contains(&d(dev)));
    }
    let mut m = Memory::default();
    m.data_register_limit = Some(4999);
    assert!(matches!(
        action.qualify_native(&m).outcome,
        Outcome::Limitation(_)
    ));
}

#[test]
fn monitor_candidates_need_only_actual_modifier_rows_and_are_not_runtime_visits() {
    let action = a("TCMP D0E0 D10F0 D20E1 D30F1 M80E2");
    let mut m = Memory::default();
    assert_eq!(
        action.monitor_input_devices(&m).unwrap(),
        BTreeSet::from([d("D1028"), d("D1029"), d("D1182"), d("D1183"), d("D1184")])
    );
    assert!(action.control_devices().is_empty());
    m.set_word(d("D0"), 24);
    let potential = action.capture_devices(&m).unwrap();
    assert!(potential.contains(&d("D32")));
    assert!(potential.contains(&d("M82")));
    assert!(!potential.contains(&d("M83")));
    let (out, actual) = visit("TCMP D0E0 D10F0 D20E1 D30F1 M80E2", &mut m);
    assert_eq!(out, Outcome::PlcError { offset: 1 });
    assert!(!actual.contains(&d("D10")));
    assert!(!actual.contains(&d("M80")));
    bits(&mut m, "M80", 3, 2);
    assert_eq!(action.display_value(&m), "010");
    put(&mut m, "D20", [1, 2, 3]);
    assert_eq!(a("TADD D0E0 D10 D20F0").display_value(&m), "01:02:03");
}

#[test]
fn every_success_and_error_path_holds_legacy_numeric_scratch() {
    for body in [
        "TCMP K0 K0 K0 D0E0 M80",
        "TCMP K24 K0 K0 D0E0 M80",
        "TZCP D0E0 D10 D20 M80",
        "TADD D0E0 D10 D20",
        "TSUB D0E0 D10 D20",
    ] {
        let mut m = Memory::default();
        m.native_numeric_scratch = Some(-1234);
        visit(body, &mut m);
        assert_eq!(m.native_numeric_scratch, Some(-1234));
    }
}

#[test]
fn public_tcmpp_held_and_rearmed_visits_keep_original_pulse_lifecycle() {
    let p = compile_il("LD X0\nTCMPP K1 K0 K0 D20E0 M80F0\nLDI X7\nMOV K1234 D60\nEND").unwrap();
    let mut sim = Simulator::default();
    put(&mut sim.memory, "D20", [1, 0, 0]);
    complete(&mut sim, &p);
    assert_eq!(read_bits(&sim.memory, "M80", 3), 0);
    sim.memory.set_input(d("X0"), true);
    complete(&mut sim, &p);
    assert_eq!(read_bits(&sim.memory, "M80", 3), 2);
    put(&mut sim.memory, "D20", [2, 0, 0]);
    complete(&mut sim, &p);
    assert_eq!(read_bits(&sim.memory, "M80", 3), 2);
    sim.memory.set_input(d("X0"), false);
    complete(&mut sim, &p);
    sim.memory.set_input(d("X0"), true);
    complete(&mut sim, &p);
    assert_eq!(read_bits(&sim.memory, "M80", 3), 4);
    assert_eq!(sim.memory.word(d("D60")), 1234);
}

#[test]
fn public_comparison_cj_and_master_false_do_not_evaluate_clock_or_invalid_indices() {
    let p=compile_il("LD X1\nCJ P0\nLD X0\nTCMP K1M1011E0 K0 K0 D20F0 M80E1\nP0\nLD X7\nMC N0\nLDI X7\nTZCP D0E0 D10F0 D20E1 M84F1\nMCR N0\nLDI X7\nMOV K1234 D60\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X1"), true);
    sim.memory.set_input(d("X0"), true);
    for alias in ["D1028", "D1029", "D1182", "D1183"] {
        sim.memory.set_word(d(alias), -30000);
    }
    complete(&mut sim, &p);
    assert_eq!(sim.memory.word(d("D60")), 1234);
    assert_eq!(read_bits(&sim.memory, "M80", 7), 0);
}
