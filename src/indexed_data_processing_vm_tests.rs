//! Independent fixed-value contracts; parent owns compilation/native runs.
use crate::indexed_data_processing::IndexedDataProcessingAction;
use crate::plc::{Area, Device, Memory, ScanStatus, Simulator, compile_il};
use std::collections::BTreeSet;

fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn args(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}
fn a(s: &str) -> IndexedDataProcessingAction {
    let v = args(s);
    IndexedDataProcessingAction::parse(&v[0], &v[1..]).unwrap()
}
fn run(s: &str, m: &mut Memory) -> BTreeSet<Device> {
    let mut seen = BTreeSet::new();
    a(s).execute(m, true, &mut seen);
    assert!(
        m.execution_limitations.is_empty(),
        "{s}: {:?}",
        m.execution_limitations
    );
    seen
}
fn bits(m: &mut Memory, base: Device, n: u16, value: u32) {
    for i in 0..n {
        m.set_bit(
            Device {
                index: base.index + i,
                ..base
            },
            value & (1u32 << i) != 0,
        );
    }
}
fn packed(m: &Memory, base: Device, n: u16) -> u32 {
    (0..n).fold(0, |v, i| {
        v | (u32::from(m.bit(Device {
            index: base.index + i,
            ..base
        })) << i)
    })
}
fn seed_flags(m: &mut Memory) {
    for i in 1020..=1022 {
        m.set_system(i, true);
    }
}
fn flags(m: &Memory) -> [bool; 3] {
    [1020, 1021, 1022].map(|i| m.system_bit(i))
}
fn hydrate(dst: &mut Memory, src: &Memory, ds: &BTreeSet<Device>) {
    for &d in ds {
        match d.area {
            Area::X | Area::Y | Area::M | Area::S => dst.set_bit(d, src.bit(d)),
            Area::C if d.index >= 200 => dst.set_double_word_raw(d, src.double_word(d)),
            _ => dst.set_word(d, src.word(d)),
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
fn all_sixteen_headers_lengths_and_original_bare_bit_descriptors() {
    for (name, operands, header, len) in [
        ("DECO", "X0E1 Y200F2 K3", 0x6200, 7),
        ("ENCO", "M0E0 D10F0 K3", 0x6400, 7),
        ("SUM", "K1M128E1 D20F2", 0x6600, 5),
        ("DSUM", "K8M128E1 D20F2", 0x6700, 9),
        ("BON", "K1M128E1 M80F7 D10E0", 0x6800, 7),
        ("DBON", "K8M128E1 M80F7 F7", 0x6900, 13),
        ("MEAN", "K1M128E1 D20F2 D10E0", 0x6a00, 7),
        ("DMEAN", "C199E1 D20F2 F7", 0x6b00, 13),
    ] {
        for p in ["", "P"] {
            let body = format!("{name}{p} {operands}");
            let action = a(&body);
            let words = action.native_words().unwrap();
            assert_eq!(words[0], header | if p.is_empty() { 0 } else { 0x10 });
            assert_eq!(words.len(), len);
            assert_eq!(action.label(), body);
        }
    }
    assert_eq!(
        a("DECO X0E1 Y200F2 K3").native_words().unwrap(),
        [0x6200, 0x00e4, 0x2480, 0x80f4, 0x4580, 0x0380, 0x0080]
    );
    assert_eq!(
        a("ENCO M0E0 D10F0 K3").native_words().unwrap(),
        [0x6400, 0x00e4, 0x0880, 0x14f6, 0x0086, 0x0380, 0x0080]
    );
    assert_eq!(
        a("BON K1M128E1 M80F7 D10E0").native_words().unwrap(),
        [0x6800, 0x80e4, 0x2882, 0x50f4, 0xe880, 0x14e6, 0x0086]
    );
    // An unchanged raw operand inside the indexed wrapper still uses the
    // compiler's bare80 form, not an accidental eight-bit width84.
    assert_eq!(
        a("DECO M0 D10E0 K2").native_words().unwrap()[1..3],
        [0x0084, 0x0880]
    );
    assert_eq!(
        a("DBON K1 M80E0 F7").native_words().unwrap()[5..9],
        [0x50e4, 0x0880, 0x0080, 0x0080]
    );
}

#[test]
fn pdf_role_matrix_keeps_kn_raw_constants_and_wide_f_separate() {
    for body in [
        "DECO K1M0E0 D20 K2",
        "DECO D0E0 K1M80 K2",
        "DECO D0E0 X0 K2",
        "DECO D0E0 D20 K5",
        "DECO D0E0 M80 K0",
        "DECO D0E0 M80 K9",
        "DECO D0E0 M80 D30",
        "ENCO K1 D20E0 K1",
        "ENCO K1M0E0 D20 K1",
        "ENCO M0E0 M80 K2",
        "ENCO M0E0 K1M80 K2",
        "ENCO D0E0 D20 K5",
        "SUM D0E0 K1M80",
        "DSUM F0 D20E0",
        "DSUM D0E0 F0",
        "BON D0E0 K1M80 K1",
        "BON D0E0 X0 K1",
        "BON D0E0 M80 K1M0",
        "DBON F0 M80E0 K1",
        "BON D0E0 M80 K16",
        "DBON D0E0 M80 K32",
        "MEAN K1 D20E0 K2",
        "MEAN E0 D20F0 K2",
        "DMEAN F0 D20E0 K2",
        "MEAN M0E0 D20 K2",
        "DMEAN D0E0 F0 K2",
        "MEAN D0E0 K1X0 K2",
        "MEAN D0E0 D20 K0",
        "MEAN D0E0 D20 K65",
        "MEAN C200E0 D20 K1",
    ] {
        let v = args(body);
        assert!(
            IndexedDataProcessingAction::parse(&v[0], &v[1..]).is_err(),
            "{body}"
        );
    }
    for body in [
        "DBON D0E0 M80 F0",
        "DMEAN D0E0 D20 F7",
        "SUM F0 D20E0",
        "MEAN K1X0E0 F7 K1",
        "DMEAN T255E0 E1 K1",
    ] {
        a(body);
    }
    for name in ["SQR", "DSQR", "DDECO", "DENCO", "SUMPP"] {
        assert!(!IndexedDataProcessingAction::supports(name));
    }
}

#[test]
fn all_sixteen_signed_word_modifier_banks_work_for_source_target_and_count() {
    use crate::calendar_operand::CalendarIndices;
    for high in [false, true] {
        for bank in 0..8 {
            let name = format!("{}{}", if high { 'F' } else { 'E' }, bank);
            let offset = CalendarIndices::alias_device(bank, high).unwrap();
            let mut m = Memory::default();
            m.set_word(offset, -2);
            m.set_word(d("D8"), 15);
            run(&format!("SUM D10{name} D22{name}"), &mut m);
            assert_eq!(m.word(d("D20")), 4);
            m.set_double_word(d("D8"), -1);
            run(&format!("DSUM D10{name} D22{name}"), &mut m);
            assert_eq!(m.double_word(d("D20")), 32);
            m.set_word(d("D28"), 3);
            m.set_word(d("D0"), 8);
            run(&format!("BON D0 M80{name} D30{name}"), &mut m);
            assert!(m.bit(d("M78")));
        }
    }
}

#[test]
fn deco_snapshots_raw_source_before_overlapping_ascending_span_and_holds_flags() {
    let mut m = Memory::default();
    seed_flags(&mut m);
    bits(&mut m, d("M128"), 3, 5);
    m.set_internal(d("M129"), false);
    m.set_internal(d("M138"), true);
    run("DECO M128E0 M130F0 K3", &mut m);
    assert_eq!(packed(&m, d("M130"), 8), 0x20);
    assert!(m.bit(d("M128")));
    assert!(!m.bit(d("M129")));
    assert!(m.bit(d("M138")));
    assert_eq!(flags(&m), [true; 3]);
}

#[test]
fn deco_n8_reads_exactly_eight_inputs_and_writes_256_bits_with_neighbor_guards() {
    let mut m = Memory::default();
    bits(&mut m, d("X0"), 8, 255);
    m.set_internal(d("S127"), true);
    m.set_internal(d("S384"), true);
    let seen = run("DECO X0E0 S128F0 K8", &mut m);
    assert!(m.bit(d("S383")));
    assert!((128..383).all(|i| !m.bit(Device {
        area: Area::S,
        index: i
    })));
    assert!(m.bit(d("S127")) && m.bit(d("S384")));
    assert!(seen.contains(&d("X7")));
    assert!(!seen.contains(&d("X10")));
}

#[test]
fn deco_numeric_word_truncation_and_direct_ef_aliases_preserve_neighbors() {
    let mut m = Memory::default();
    m.set_word(d("D0"), -1);
    m.set_word(d("D1183"), 777);
    run("DECO D0E0 E1 K4", &mut m);
    assert_eq!(m.word(d("D1182")), i16::MIN);
    assert_eq!(m.word(d("D1183")), 777);
    run("DECO D0E0 F7 K1", &mut m);
    assert_eq!(m.word(d("D1195")), 2);
}

#[test]
fn enco_reads_every_raw_bit_ascending_and_selects_highest() {
    let mut m = Memory::default();
    bits(&mut m, d("M128"), 8, 0b1010_0001);
    let action = a("ENCO M128E0 D20F0 K3");
    let mut order = Vec::new();
    action
        .qualify_native_with(&m, |_, ds| {
            order.extend(
                ds.iter()
                    .copied()
                    .filter(|d| d.area == Area::M && (128..136).contains(&d.index)),
            );
            Ok(())
        })
        .unwrap();
    assert_eq!(
        order,
        (128..136)
            .map(|index| Device {
                area: Area::M,
                index
            })
            .collect::<Vec<_>>()
    );
    let seen = run("ENCO M128E0 D20F0 K3", &mut m);
    assert_eq!(m.word(d("D20")), 7);
    assert!(seen.contains(&d("M128")) && seen.contains(&d("M135")));
}

#[test]
fn enco_empty_source_errors_before_even_invalid_target_modifier() {
    let mut m = Memory::default();
    seed_flags(&mut m);
    m.set_word(d("D1182"), -1);
    let seen = run("ENCO M128E0 D0E1 K3", &mut m);
    assert!(m.system_bit(1067));
    assert_eq!(m.word(d("D1067")), 0xe1a);
    assert_eq!(m.word(d("D1068")), 3);
    assert!(!seen.contains(&d("D1182")));
    assert_eq!(flags(&m), [true; 3]);
    let mut fetched = BTreeSet::<Device>::new();
    a("ENCO M128E0 D0E1 K3")
        .qualify_native_with(&m, |_, ds| {
            fetched.extend(ds);
            Ok(())
        })
        .unwrap();
    assert!(!fetched.contains(&d("D1182")));
}

#[test]
fn enco_does_not_shortcut_unknown_clock_even_if_last_bit_is_on() {
    use crate::system_relays::ClockProfile;
    let mut m = Memory::default();
    m.system_relays.set_profile(ClockProfile::InstalledFixed);
    m.set_bit(d("M1014"), true);
    m.set_word(d("D20"), 99);
    let mut seen = BTreeSet::new();
    a("ENCO M1011E0 D20F0 K2").execute(&mut m, true, &mut seen);
    assert!(!m.execution_limitations.is_empty());
    assert!(!m.system_bit(1067));
    assert_eq!(m.word(d("D20")), 99);
    assert!(seen.contains(&d("M1011")));
    assert!(!seen.contains(&d("D1029")));
}

#[test]
fn sum_counts_declared_source_width_and_zero_flag_is_sticky_or_only() {
    for (body, width, raw, expected) in [
        ("SUM K1M128E0 D20", 4, 15, 4),
        ("SUM K4M128E0 D20", 16, 65535, 16),
        ("DSUM K4M128E0 D20", 16, 65535, 16),
        ("DSUM K8M128E0 D20", 32, u32::MAX, 32),
    ] {
        let mut m = Memory::default();
        seed_flags(&mut m);
        bits(&mut m, d("M128"), width, raw);
        run(body, &mut m);
        assert_eq!(m.word(d("D20")), expected);
        assert_eq!(flags(&m), [true; 3]);
    }
    let mut m = Memory::default();
    run("SUM K0 D20E0", &mut m);
    assert_eq!(flags(&m), [true, false, false]);
    m.set_word(d("D0"), 7);
    run("SUM D0E0 D20", &mut m);
    assert_eq!(m.word(d("D20")), 3);
    assert!(m.system_bit(1020));
}

#[test]
fn sum_zero_is_published_before_late_target_qualification() {
    let mut m = Memory::default();
    m.set_word(d("D1182"), -1);
    let mut seen = BTreeSet::new();
    a("SUM K0 D0E1").execute(&mut m, true, &mut seen);
    assert!(m.system_bit(1020));
    assert!(!m.execution_limitations.is_empty());
    assert!(seen.contains(&d("D1182")));
    assert!(!m.system_bit(1067));
    let mut original = Memory::default();
    original.set_word(d("D1182"), -1);
    assert!(a("SUM K0 D0E1").qualify_native(&original).is_err());
    assert!(!original.system_bit(1020));
}

#[test]
fn bon_full_width_count_is_not_masked_and_error_preserves_target() {
    for (body, n, offset) in [
        ("BON HFFFF M80E1 D30E0", -1, 5),
        ("BON HFFFF M80E1 D30E0", 16, 5),
        ("DBON HFFFFFFFF M80E1 D30E0", 32, 9),
        ("DBON HFFFFFFFF M80E1 D30E0", 65536, 9),
    ] {
        let mut m = Memory::default();
        seed_flags(&mut m);
        m.set_double_word(d("D30"), n);
        m.set_word(d("D1182"), -200);
        m.set_internal(d("M80"), true);
        let seen = run(body, &mut m);
        assert!(m.bit(d("M80")));
        assert_eq!(m.word(d("D1067")), 0xe1a);
        assert_eq!(m.word(d("D1068")), offset);
        assert!(!seen.contains(&d("D1182")));
        assert_eq!(flags(&m), [true; 3]);
    }
}

#[test]
fn bon_direct_f_wide_count_reads_canonical_pair_including_f7_highword() {
    for (name, low, high) in [
        ("F0", "D1029", "D1030"),
        ("F1", "D1183", "D1184"),
        ("F7", "D1195", "D1196"),
    ] {
        let mut m = Memory::default();
        m.set_word(d(low), 31);
        m.set_word(d(high), 0);
        let seen = run(&format!("DBON H80000000 M80E0 {name}"), &mut m);
        assert!(m.bit(d("M80")));
        assert!(seen.contains(&d(low)) && seen.contains(&d(high)));
        m.set_word(d(high), 1);
        m.set_internal(d("M80"), false);
        run(&format!("DBON H80000000 M80E0 {name}"), &mut m);
        assert!(!m.bit(d("M80")));
        assert_eq!(m.word(d("D1068")), 9);
    }
}

#[test]
fn bon_single_bit_writer_aliases_only_selected_flag_and_has_neighbor_guards() {
    let mut m = Memory::default();
    m.set_system(1020, true);
    m.set_system(1021, true);
    m.set_word(d("D0"), i16::MIN);
    run("BON D0E0 M1022F0 K15", &mut m);
    assert_eq!(flags(&m), [true; 3]);
    m.set_internal(d("M79"), true);
    m.set_internal(d("M81"), true);
    run("DBON H80000000 M80E0 K31", &mut m);
    assert!(m.bit(d("M79")) && m.bit(d("M80")) && m.bit(d("M81")));
}

#[test]
fn source_clock_failure_precedes_later_bon_count_domain_error() {
    use crate::system_relays::ClockProfile;
    let mut m = Memory::default();
    m.system_relays.set_profile(ClockProfile::InstalledFixed);
    m.set_word(d("D30"), -1);
    let mut seen = BTreeSet::new();
    a("BON K1M1011E0 M80F0 D30E1").execute(&mut m, true, &mut seen);
    assert!(!m.execution_limitations.is_empty());
    assert!(!m.system_bit(1067));
    assert!(
        !seen.contains(&d("D1182")) && !seen.contains(&d("D30")) && !seen.contains(&d("D1029"))
    );
}

#[test]
fn mean_caches_base_before_count_and_snapshots_before_target_index_overwrite() {
    let mut m = Memory::default();
    m.set_word(d("D1028"), 2);
    m.set_word(d("D2"), 5);
    m.set_word(d("D3"), 9);
    m.set_word(d("D7"), 99);
    let seen = run("MEAN D0E0 E0 E0", &mut m);
    assert_eq!(m.word(d("D1028")), 7);
    assert!(seen.contains(&d("D2")) && seen.contains(&d("D3")));
    assert!(!seen.contains(&d("D7")));
}

#[test]
fn mean_invalid_n_reads_base_modifier_but_skips_payload_and_destination() {
    for (wide, n, offset) in [
        (false, 0, 5),
        (false, -1, 5),
        (false, 65, 5),
        (true, 0, 9),
        (true, 65536, 9),
    ] {
        let mut m = Memory::default();
        seed_flags(&mut m);
        m.set_double_word(d("D30"), n);
        m.set_word(d("D1182"), -1);
        let seen = run(
            &format!("{}MEAN D10E0 D0E1 D30", if wide { "D" } else { "" }),
            &mut m,
        );
        assert!(seen.contains(&d("D1028")));
        assert!(!seen.contains(&d("D10")) && !seen.contains(&d("D1182")));
        assert_eq!(m.word(d("D1068")), offset);
        assert_eq!(flags(&m), [true; 3]);
    }
    let mut m = Memory::default();
    m.set_word(d("D1028"), -1);
    let mut seen = BTreeSet::new();
    a("MEAN D0E0 D20 D30").execute(&mut m, true, &mut seen);
    assert!(!m.execution_limitations.is_empty());
    assert!(!m.system_bit(1067));
    assert!(!seen.contains(&d("D30")));
}

#[test]
fn mean_all_payload_is_read_before_overlap_and_wide_keeps_per_term_rounding() {
    let mut m = Memory::default();
    for (i, v) in [4, 8, 12, 16].into_iter().enumerate() {
        m.set_word(
            Device {
                area: Area::D,
                index: i as u16,
            },
            v,
        );
    }
    run("MEAN D0E0 D1F0 K4", &mut m);
    assert_eq!(
        (
            m.word(d("D0")),
            m.word(d("D1")),
            m.word(d("D2")),
            m.word(d("D3"))
        ),
        (4, 10, 12, 16)
    );
    m.set_double_word(d("D0"), 4);
    m.set_double_word(d("D2"), -1);
    run("DMEAN D0E0 D1F0 K2", &mut m);
    assert_eq!(m.double_word(d("D1")), 2);
}

#[test]
fn dmean_counter_cursor_handles_pair_low_only_then_natural32() {
    let mut m = Memory::default();
    m.set_double_word(d("C197"), 4);
    m.set_word(d("C199"), 5);
    m.set_double_word(d("C200"), 9);
    let seen = run("DMEAN C197E0 D20F0 K3", &mut m);
    assert_eq!(m.double_word(d("D20")), 6);
    for x in ["C197", "C198", "C199", "C200"] {
        assert!(seen.contains(&d(x)));
    }
    assert!(!seen.contains(&d("C201")));
}

#[test]
fn dmean_c199_is_unsigned_low_and_c198_advances_to_natural_c200() {
    let mut m = Memory::default();
    m.set_word(d("D1028"), -1);
    m.set_word(d("C199"), -1);
    m.set_double_word(d("C200"), 1);
    run("DMEAN C200E0 D20F0 K2", &mut m);
    assert_eq!(m.double_word(d("D20")), 32768);
    m.set_word(d("D1028"), 0);
    m.set_double_word(d("C198"), 4);
    m.set_double_word(d("C200"), 9);
    run("DMEAN C198E0 D20F0 K2", &mut m);
    assert_eq!(m.double_word(d("D20")), 6);
}

#[test]
fn mean_group_stride_signedness_and_short_destination_truncation_are_literal() {
    let mut m = Memory::default();
    bits(&mut m, d("M128"), 12, 0x753);
    m.set_internal(d("M159"), true);
    m.set_internal(d("M164"), true);
    run("MEAN K1M128E0 K1M160F0 K3", &mut m);
    assert_eq!(packed(&m, d("M160"), 4), 5);
    assert!(m.bit(d("M159")) && m.bit(d("M164")));
    bits(&mut m, d("M128"), 32, (-4i32) as u32);
    bits(&mut m, d("M160"), 32, 1);
    run("DMEAN K8M128E0 D20F0 K2", &mut m);
    assert_eq!(m.double_word(d("D20")), -2);
    bits(&mut m, d("M128"), 8, 0xf1);
    run("DMEAN K1M128E0 D20F0 K2", &mut m);
    assert_eq!(m.double_word(d("D20")), 8);
}

#[test]
fn mean_local_partial_endpoints_remain_admitted_but_not_native_qualified() {
    for (last, limit) in [(4999, 4999), (9999, 9999)] {
        let mut m = Memory::default();
        m.data_register_limit = Some(limit);
        m.set_word(
            Device {
                area: Area::D,
                index: last,
            },
            -1,
        );
        let body = format!("DMEAN D{last}E0 D20F0 K2");
        run(&body, &mut m);
        assert_eq!(m.double_word(d("D20")), 32767);
        assert!(a(&body).qualify_native(&m).is_err());
        assert_eq!(a(&body).native_words().unwrap().len(), 13);
        m.set_word(d("D1028"), -1);
        m.set_word(
            Device {
                area: Area::D,
                index: last - 1,
            },
            2,
        );
        m.set_word(
            Device {
                area: Area::D,
                index: last,
            },
            1,
        );
        let body = format!("DMEAN D{last}E0 D20F0 K1");
        run(&body, &mut m);
        assert_eq!(m.double_word(d("D20")), 65538);
        assert!(a(&body).qualify_native(&m).is_ok());
    }
    let mut m = Memory::default();
    m.set_word(d("T255"), i16::MIN);
    run("DMEAN T255E0 D20F0 K1", &mut m);
    assert_eq!(m.double_word(d("D20")), 32768);
    assert!(a("DMEAN T255E0 D20F0 K1").qualify_native(&m).is_err());
}

#[test]
fn mean_later_partial_group_keeps_requested_denominator_and_native_gap_is_explicit() {
    let mut m = Memory::default();
    bits(&mut m, d("Y370"), 8, 0x42);
    run("MEAN K1Y370E0 D20F0 K3", &mut m);
    assert_eq!(m.word(d("D20")), 2);
    assert!(a("MEAN K1Y370E0 D20F0 K3").qualify_native(&m).is_err());
    // Direct special group initial resolution is valid; later raw pointer gap
    // is the limitation. This is not a modified low-M remapping fixture.
    let action = a("MEAN K1M1532 D20E0 K2");
    assert!(action.qualify_native(&m).is_err());
}

#[test]
fn static_inventory_preserves_mean_low_only_exception_and_complete_other_roles() {
    let mut ds = BTreeSet::new();
    a("DMEAN D4999 D20E0 K2").devices(&mut ds);
    assert!(ds.contains(&d("D4999")));
    assert!(!ds.contains(&d("D5000")));
    let mut ds = BTreeSet::new();
    a("DSUM D4999 D20E0").devices(&mut ds);
    assert!(ds.contains(&d("D5000")));
    let mut ds = BTreeSet::new();
    a("DMEAN D0E0 D4999 F7").devices(&mut ds);
    for name in ["D4999", "D5000", "D1195", "D1196"] {
        assert!(ds.contains(&d(name)));
    }
    let mut ds = BTreeSet::new();
    a("DECO D0E0 M80 K4").devices(&mut ds);
    assert!(ds.contains(&d("M95")));
    assert!(!ds.contains(&d("M96")));
}

#[test]
fn actual_native_count_capture_drives_payload_span_without_mutating_local_inputs() {
    let action = a("MEAN D10E0 D20F0 D30E1");
    let mut local = Memory::default();
    local.set_word(d("D30"), 1);
    local.set_word(d("D10"), 111);
    local.set_word(d("D20"), 777);
    let mut native = Memory::default();
    native.set_word(d("D30"), 2);
    native.set_word(d("D10"), 10);
    native.set_word(d("D11"), 20);
    let mut fetched = BTreeSet::<Device>::new();
    action
        .qualify_native_with(&local, |clone, ds| {
            fetched.extend(ds);
            hydrate(clone, &native, ds);
            Ok(())
        })
        .unwrap();
    assert!(fetched.contains(&d("D11")));
    assert_eq!(local.word(d("D30")), 1);
    assert_eq!(local.word(d("D10")), 111);
    assert_eq!(local.word(d("D20")), 777);
    native.set_word(d("D30"), 0);
    native.set_word(d("D1029"), -300);
    let mut fetched = BTreeSet::<Device>::new();
    action
        .qualify_native_with(&local, |clone, ds| {
            fetched.extend(ds);
            hydrate(clone, &native, ds);
            Ok(())
        })
        .unwrap();
    assert!(!fetched.contains(&d("D10")) && !fetched.contains(&d("D1029")));
}

#[test]
fn actual_native_empty_enco_skips_bad_target_even_when_local_source_is_nonzero() {
    let action = a("ENCO D10E0 D0F0 K4");
    let mut local = Memory::default();
    local.set_word(d("D10"), 8);
    local.set_word(d("D1029"), -1);
    let mut native = local.clone();
    native.set_word(d("D10"), 0);
    let mut fetched = BTreeSet::<Device>::new();
    action
        .qualify_native_with(&local, |clone, ds| {
            fetched.extend(ds);
            hydrate(clone, &native, ds);
            Ok(())
        })
        .unwrap();
    assert!(!fetched.contains(&d("D1029")));
    assert_eq!(local.word(d("D10")), 8);
    assert!(!local.system_bit(1067));
}

#[test]
fn mean_monitor_requires_actual_full_width_count_rows_and_uses_model_partial_span() {
    let mut m = Memory::default();
    m.data_register_limit = Some(4999);
    m.set_word(d("D1195"), 2);
    m.set_word(d("D1196"), 0);
    let action = a("DMEAN D4999E0 D20F0 F7");
    let required = action.monitor_input_devices(&m).unwrap();
    for name in ["D1028", "D1029", "D1195", "D1196"] {
        assert!(required.contains(&d(name)));
    }
    let candidates = action.capture_devices(&m).unwrap();
    assert!(candidates.contains(&d("D4999")));
    assert!(!candidates.contains(&d("D5000")));
    m.set_word(d("D1196"), 1);
    let candidates = action.capture_devices(&m).unwrap();
    assert!(!candidates.contains(&d("D4999")));
}

#[test]
fn native_clock_storage_fetch_never_fabricates_installed_clock_seed() {
    use crate::system_relays::ClockProfile;
    let mut local = Memory::default();
    local
        .system_relays
        .set_profile(ClockProfile::InstalledFixed);
    let mut native = local.clone();
    bits(&mut native, d("M1011"), 4, 5);
    let result = a("SUM K1M1011E0 D20F0").qualify_native_with(&local, |clone, ds| {
        hydrate(clone, &native, ds);
        Ok(())
    });
    assert!(result.is_err());
    assert!(local.system_relays.clock_levels().is_none());
    assert!(!local.system_bit(1067));
}

#[test]
fn seeded_and_documented_clock_reads_retain_literal_results() {
    use crate::system_relays::{ClockProfile, ClockState};
    for (profile, expected) in [
        (ClockProfile::Documented, 0),
        (ClockProfile::InstalledFixed, 2),
    ] {
        let mut m = Memory::default();
        m.system_relays.set_profile(profile);
        if profile == ClockProfile::InstalledFixed {
            ClockState::seed_installed_levels(&mut m, [true, false, true, false]).unwrap();
        }
        run("SUM K1M1011E0 D20F0", &mut m);
        assert_eq!(m.word(d("D20")), expected);
    }
}

#[test]
fn all_family_scratch_and_off_contracts_remain_unchanged() {
    for body in [
        "DECO D0E0 M80F0 K2",
        "ENCO D0E0 D20F0 K2",
        "SUM D0E0 D20F0",
        "DSUM D0E0 D20F0",
        "BON D0E0 M80F0 K0",
        "DBON D0E0 M80F0 K0",
        "MEAN D0E0 D20F0 K1",
        "DMEAN D0E0 D20F0 K1",
    ] {
        let mut m = Memory::default();
        m.set_word(d("D0"), 1);
        m.native_numeric_scratch = Some(1234);
        run(body, &mut m);
        assert_eq!(m.native_numeric_scratch, Some(1234));
        m.set_word(d("D1028"), -30000);
        m.set_word(d("D1029"), -30000);
        let mut seen = BTreeSet::new();
        a(body).execute(&mut m, false, &mut seen);
        assert!(seen.is_empty());
        assert!(m.execution_limitations.is_empty());
    }
}

#[test]
fn pulse_vm_sites_skip_held_reads_and_rearm_after_off_with_original_next_pc() {
    let p = compile_il("LD X0\nSUMP D0E0 D20F0\nLDI X7\nMOV K1234 D60\nEND").unwrap();
    let mut sim = Simulator::default();
    for (on, source, offset, expected) in [
        (false, 1, 0, 0),
        (true, 7, 0, 3),
        (true, 15, -30000, 3),
        (false, 31, 0, 3),
        (true, 31, 0, 5),
    ] {
        sim.memory.set_input(d("X0"), on);
        sim.memory.set_word(d("D0"), source);
        sim.memory.set_word(d("D1028"), offset);
        complete(&mut sim, &p);
        assert_eq!(sim.memory.word(d("D20")), expected);
        assert_eq!(sim.memory.word(d("D60")), 1234);
    }
}

#[test]
fn semantic_error_pc_is_absolute_and_following_marker_executes() {
    let p = compile_il("LD X0\nDBON HFFFFFFFF M80E1 D30E0\nLDI X7\nMOV K1234 D60\nEND").unwrap();
    let image = crate::es2_encoder::encode(&p).unwrap();
    assert_eq!(image.action_pc[0], 1);
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim.memory.set_word(d("D30"), 32);
    sim.memory.set_word(d("D1182"), -100);
    complete(&mut sim, &p);
    assert_eq!(sim.memory.word(d("D1068")), 10);
    assert_eq!(sim.memory.word(d("D60")), 1234);
}

#[test]
fn loop_stack_master_and_inactive_stl_keep_checked_read_gates() {
    let p=compile_il("LD X0\nFOR K3\nLD X0\nSUMP D0E0 D20F0\nNEXT\nLD X0\nMPS\nMEAN D0E0 D40F0 K2\nMRD\nMOV K1 E1\nMPP\nOUT Y0\nLD X7\nMC N0\nLDI X7\nSUM D0E1 D80F0\nMCR N0\nLDI X7\nMOV K1234 D60\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim.memory.set_word(d("D0"), 3);
    sim.memory.set_word(d("D1"), 9);
    complete(&mut sim, &p);
    assert_eq!(sim.memory.word(d("D20")), 2);
    assert_eq!(sim.memory.word(d("D40")), 6);
    assert_eq!(sim.memory.word(d("D80")), 0);
    assert_eq!(sim.memory.word(d("D60")), 1234);
    let p = compile_il("STL S0\nLDI X7\nSUM D0E0 D20F0\nRET\nLDI X7\nMOV K1234 D60\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_word(d("D0"), 7);
    complete(&mut sim, &p);
    assert_eq!(sim.memory.word(d("D20")), 0);
    assert_eq!(sim.memory.word(d("D60")), 1234);
}
