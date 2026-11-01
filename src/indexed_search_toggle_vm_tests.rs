//! Fixed-value independent contracts; no native process is launched here.
use crate::indexed_search_toggle::IndexedSearchToggleAction;
use crate::plc::{Area, Device, Memory, ScanStatus, Simulator, compile_il};
use std::collections::BTreeSet;
fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn args(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}
fn a(s: &str) -> IndexedSearchToggleAction {
    let v = args(s);
    IndexedSearchToggleAction::parse(&v[0], &v[1..]).unwrap()
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
fn words(m: &Memory, base: u16, n: u16) -> Vec<i16> {
    (base..base + n)
        .map(|index| {
            m.word(Device {
                area: Area::D,
                index,
            })
        })
        .collect()
}
fn results(m: &Memory, base: u16, wide: bool) -> Vec<i32> {
    (0..5)
        .map(|i| {
            let d = Device {
                area: Area::D,
                index: base + i * if wide { 2 } else { 1 },
            };
            if wide {
                m.double_word(d)
            } else {
                i32::from(m.word(d))
            }
        })
        .collect()
}
fn seed(m: &mut Memory, base: u16, values: &[i32], wide: bool) {
    for (i, &v) in values.iter().enumerate() {
        let d = Device {
            area: Area::D,
            index: base + i as u16 * if wide { 2 } else { 1 },
        };
        if wide {
            m.set_double_word(d, v)
        } else {
            m.set_word(d, v as i16)
        }
    }
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
fn six_spellings_preserve_original_headers_field_widths_and_bare_alt() {
    for (name, operands, header, len) in [
        ("SER", "D10E1 D20F2 D100E7 D30", 0x8a00, 9),
        ("DSER", "D10E1 D20F2 D100E7 D30", 0x8b00, 17),
        ("ALT", "Y0E1", 0x9400, 3),
    ] {
        for p in ["", "P"] {
            let body = format!("{name}{p} {operands}");
            let action = a(&body);
            let encoded = action.native_words().unwrap();
            assert_eq!(encoded[0], header | if p.is_empty() { 0 } else { 0x10 });
            assert_eq!(encoded.len(), len);
            assert_eq!(action.label(), body);
        }
    }
    assert_eq!(
        a("ALT Y0E1").native_words().unwrap(),
        [0x9400, 0x00e4, 0x2580]
    );
    assert_eq!(
        a("ALTP M80F7").native_words().unwrap(),
        [0x9410, 0x50f4, 0xe880]
    );
    assert_eq!(
        a("ALT M80").native_words().unwrap(),
        [0x9400, 0x5084, 0x0880]
    );
    assert_eq!(
        a("SER D10E1 D20F2 D100E7 D30").native_words().unwrap(),
        [
            0x8a00, 0x14e6, 0x8086, 0x28f6, 0x0087, 0xc8e6, 0x8089, 0x3c86, 0x0086
        ]
    );
    let w = a("DSER D10E7 H12345678 D100F3 D30").native_words().unwrap();
    assert_eq!(&w[5..9], &[0x7880, 0x5680, 0x3480, 0x1280]);
    assert_eq!(w[2] & 255, 0x89);
    assert_eq!(w[10] & 255, 0x87);
}

#[test]
fn white_count_and_exact_scalar_stack_alt_roles_remain_scoped() {
    for body in [
        "SER D0E0 K1 D20 D30E0",
        "DSER D0E0 K1 D20 D30F7",
        "SER D0E0 K1 D20 E0",
        "DSER D0E0 K1 D20 F7",
        "SER D0E0 K1 D20 T0",
        "SER D0E0 K1 D20 C0",
        "SER D0E0 K1 D20 K1M0",
        "SER K1 K1 D20E0 K1",
        "SER E0 K1 D20E0 K1",
        "SER M0E0 K1 D20 K1",
        "SER D0E0 K1 E0 K1",
        "SER D0E0 K1 K1X0 K1",
        "DSER D0E0 F0 D20 K1",
        "SER C200E0 K1 D20 K1",
        "SER D0E0 C200 D20 K1",
        "SER D0E0 K1 C200 K1",
        "ALT K1M80E0",
        "ALT X0E0",
        "ALT D0E0",
        "ALT E0",
        "ALT M1000",
    ] {
        let v = args(body);
        assert!(
            IndexedSearchToggleAction::parse(&v[0], &v[1..]).is_err(),
            "{body}"
        );
    }
    for body in [
        "SER D0E0 F7 D20 K1",
        "DSER C255 K1 C251 K1",
        "DSER C199E0 E7 D20F0 D30",
        "SER D0E0 K1 D20 K0",
        "SER D0E0 K1 D20 K-1",
    ] {
        a(body);
    }
    assert!(!IndexedSearchToggleAction::supports("DALT"));
    assert!(!IndexedSearchToggleAction::supports("ALTPP"));
}

#[test]
fn every_signed_word_modifier_bank_works_in_all_three_search_roles() {
    use crate::calendar_operand::CalendarIndices;
    for high in [false, true] {
        for bank in 0..8 {
            let suffix = format!("{}{}", if high { 'F' } else { 'E' }, bank);
            let index = CalendarIndices::alias_device(bank, high).unwrap();
            for wide in [false, true] {
                let mut m = Memory::default();
                m.set_word(index, -2);
                seed(&mut m, 8, &[4, 1, 4], wide);
                seed(&mut m, 18, &[4], wide);
                run(
                    &format!(
                        "{}SER D10{suffix} D20{suffix} D100{suffix} K3",
                        if wide { "D" } else { "" }
                    ),
                    &mut m,
                );
                assert_eq!(results(&m, 98, wide), [2, 0, 2, 1, 2]);
            }
        }
    }
}

#[test]
fn manual_vector_signed_extrema_tie_last_and_no_match_are_independent() {
    let mut m = Memory::default();
    seed(
        &mut m,
        10,
        &[88, 100, 110, 150, 100, 300, 100, 5, 100, 500],
        false,
    );
    run("SER D10E0 K100 D50F0 K10", &mut m);
    assert_eq!(results(&m, 50, false), [4, 1, 8, 7, 9]);
    seed(&mut m, 0, &[-2, 7, -2, 7], false);
    run("SER D0E0 K11 D50F0 K4", &mut m);
    assert_eq!(results(&m, 50, false), [0, 0, 0, 2, 3]);
    seed(&mut m, 0, &[i32::MIN, i32::MAX, i32::MIN, i32::MAX], true);
    run("DSER D0E0 H80000000 D50F0 K4", &mut m);
    assert_eq!(results(&m, 50, true), [2, 0, 2, 2, 3]);
}

#[test]
fn byte_match_count_wrap_and_double_output_high_halves_are_explicit() {
    let mut m = Memory::default();
    seed(&mut m, 0, &[7; 256], false);
    run("SER D0E0 K7 D500F0 K256", &mut m);
    assert_eq!(results(&m, 500, false), [0, 0, 255, 255, 255]);
    seed(&mut m, 0, &[-1; 128], true);
    for i in 500..510 {
        m.set_word(
            Device {
                area: Area::D,
                index: i,
            },
            -1,
        );
    }
    run("DSER D0E0 K-1 D500F0 K128", &mut m);
    assert_eq!(results(&m, 500, true), [128, 0, 127, 127, 127]);
    for i in [501, 503, 505, 507, 509] {
        assert_eq!(
            m.word(Device {
                area: Area::D,
                index: i
            }),
            0
        );
    }
}

#[test]
fn unequal_group_widths_cover_all_sizes_and_input_x_without_extra_result_bits() {
    for wide in [false, true] {
        for digits in 1..=if wide { 8 } else { 4 } {
            let mut m = Memory::default();
            for (i, value) in [3, 1, 3, 0].into_iter().enumerate() {
                bits(
                    &mut m,
                    Device {
                        area: Area::X,
                        index: i as u16 * digits * 4,
                    },
                    digits * 4,
                    value,
                );
            }
            bits(&mut m, d("M256"), 32, u32::MAX);
            m.set_internal(d("M255"), true);
            m.set_internal(d("M276"), true);
            run(
                &format!(
                    "{}SER K{digits}X0E0 K3 K1M256F0 K4",
                    if wide { "D" } else { "" }
                ),
                &mut m,
            );
            assert_eq!(packed(&m, d("M256"), 20), 0x23202);
            assert!(m.bit(d("M255")) && m.bit(d("M276")));
        }
    }
    let mut m = Memory::default();
    bits(&mut m, d("M128"), 16, 0x0313);
    for i in 0..5 {
        bits(
            &mut m,
            Device {
                area: Area::S,
                index: 128 + i * 32,
            },
            32,
            u32::MAX,
        );
    }
    run("DSER K1M128E0 K3 K8S128F0 K4", &mut m);
    for (i, expected) in [2, 0, 2, 3, 2].into_iter().enumerate() {
        assert_eq!(
            packed(
                &m,
                Device {
                    area: Area::S,
                    index: 128 + i as u16 * 32
                },
                32
            ),
            expected
        );
    }
}

#[test]
fn complete_table_needle_and_count_are_frozen_before_overlapping_results() {
    let mut m = Memory::default();
    seed(&mut m, 0, &[4, 2, 4, -7], false);
    run("SER D0E0 D3F0 D0E0 K4", &mut m);
    assert_eq!(results(&m, 0, false), [1, 3, 3, 3, 2]);
    seed(&mut m, 0, &[7, 0, 7], false);
    m.set_word(d("D30"), 3);
    run("SER D0E0 K7 D30F0 D30", &mut m);
    assert_eq!(results(&m, 30, false), [2, 0, 2, 1, 2]);
}

#[test]
fn result_overwriting_its_own_index_keeps_all_five_cached_pairs() {
    let mut m = Memory::default();
    seed(&mut m, 0, &[7, -1, 7], true);
    m.set_word(d("D1181"), 77);
    m.set_word(d("D1192"), 88);
    run("DSER D0F0 K7 D1182E1 K3", &mut m);
    assert_eq!(results(&m, 1182, true), [2, 0, 2, 1, 2]);
    assert_eq!(m.word(d("D1181")), 77);
    assert_eq!(m.word(d("D1192")), 88);
    assert_eq!(m.word(d("D1182")), 2);
}

#[test]
fn invalid_count_occurs_after_source_needle_target_base_and_scratch() {
    for body in [
        "SER D10E0 K-3 D4999 K0",
        "SER D10E0 K-3 D4999 K-1",
        "SER D10E0 K-3 D4999 K257",
    ] {
        let mut m = Memory::default();
        m.data_register_limit = Some(4999);
        m.set_word(d("D4999"), 777);
        let seen = run(body, &mut m);
        assert_eq!(m.native_numeric_scratch, Some(-3));
        assert_eq!(m.word(d("D1068")), 7);
        assert_eq!(m.word(d("D4999")), 777);
        assert!(!seen.contains(&d("D10")));
        assert!(a(body).qualify_native(&m).is_ok());
    }
    let mut m = Memory::default();
    m.set_word(d("D1182"), -300);
    m.set_word(d("D30"), 0);
    let mut seen = BTreeSet::new();
    a("SER D10E0 K-3 D20E1 D30").execute(&mut m, true, &mut seen);
    assert!(!m.execution_limitations.is_empty());
    assert!(!m.system_bit(1067));
    assert_eq!(m.native_numeric_scratch, Some(-3));
    assert!(seen.contains(&d("D1182")));
    assert!(!seen.contains(&d("D30")));
}

#[test]
fn dser_uses_original_descriptor_byte3_and_full32_count_on_error() {
    let mut m = Memory::default();
    m.set_double_word(d("D30"), 65537);
    let seen = run("DSER D10E7 K1 D100F3 D30", &mut m);
    assert_eq!(m.native_numeric_scratch, Some(0x8987u16 as i16));
    assert_eq!(m.word(d("D1068")), 13);
    assert!(seen.contains(&d("D31")));
    assert!(!seen.contains(&d("D10")));
    m.set_word(d("D1194"), 2);
    m.set_word(d("D1187"), 3);
    run("DSER D10E7 K1 D100F3 K129", &mut m);
    assert_eq!(m.native_numeric_scratch, Some(0x8987u16 as i16));
}

#[test]
fn dser_source_scratch_high_byte_changes_before_failed_needle_read() {
    use crate::system_relays::ClockProfile;
    for prior in [Some(0x1234), None] {
        let mut m = Memory::default();
        m.system_relays.set_profile(ClockProfile::InstalledFixed);
        m.native_numeric_scratch = prior;
        let mut seen = BTreeSet::new();
        a("DSER D10E7 K1M1011F0 D100F3 K1").execute(&mut m, true, &mut seen);
        assert!(!m.execution_limitations.is_empty());
        assert!(!m.system_bit(1067));
        assert_eq!(m.native_numeric_scratch, prior.map(|_| 0x8934u16 as i16));
        assert!(!seen.contains(&d("D1187")));
    }
    let mut m = Memory::default();
    m.native_numeric_scratch = Some(0x1234);
    m.set_word(d("D1194"), -20);
    let mut seen = BTreeSet::new();
    a("DSER D10E7 D20F0 D100F3 K1").execute(&mut m, true, &mut seen);
    assert_eq!(m.native_numeric_scratch, Some(0x1234));
    assert!(!seen.contains(&d("D1029")));
}

#[test]
fn dser_source_counter_cursor_synthesizes_only_c199_high_and_continues() {
    let mut m = Memory::default();
    m.set_double_word(d("C197"), 4);
    m.set_word(d("C199"), -1);
    m.set_double_word(d("C200"), 9);
    let seen = run("DSER C197E0 K65535 D100F0 K3", &mut m);
    assert_eq!(results(&m, 100, true), [1, 1, 1, 0, 1]);
    for x in ["C197", "C198", "C199", "C200"] {
        assert!(seen.contains(&d(x)));
    }
    assert!(!seen.contains(&d("C201")));
    m.set_double_word(d("C198"), 4);
    m.set_double_word(d("C200"), 9);
    run("DSER C198E0 K9 D100F0 K2", &mut m);
    assert_eq!(results(&m, 100, true), [1, 1, 1, 0, 1]);
    m.set_word(d("D1028"), -1);
    m.set_word(d("C199"), -1);
    m.set_double_word(d("C200"), 9);
    run("DSER C200E0 K65535 D100F0 K2", &mut m);
    assert_eq!(results(&m, 100, true), [1, 0, 0, 1, 0]);
}

#[test]
fn complete_natural_counter_stacks_include_c255_and_c251_result_start() {
    let mut m = Memory::default();
    m.set_double_word(d("C255"), -123);
    let seen = run("DSER C255 K-123 C251 K1", &mut m);
    assert_eq!(
        (251..256)
            .map(|index| m.double_word(Device {
                area: Area::C,
                index
            }))
            .collect::<Vec<_>>(),
        [1, 0, 0, 0, 0]
    );
    assert!(seen.contains(&d("C255")));
    assert!(a("DSER C255 K-123 C251 K1").qualify_native(&m).is_ok());
}

#[test]
fn c199_result_exit_is_after_each_half_store_and_never_resumes_at_c200() {
    for (start, expected) in [(197, vec![2, 0, 0]), (198, vec![2, 0]), (199, vec![2])] {
        let mut m = Memory::default();
        seed(&mut m, 0, &[7, -1, 7], true);
        for index in 197..200 {
            m.set_word(
                Device {
                    area: Area::C,
                    index,
                },
                777,
            );
        }
        m.set_double_word(d("C200"), 888);
        let body = format!("DSER D0E0 K7 C{start}F0 K3");
        run(&body, &mut m);
        assert_eq!(
            (start..200)
                .map(|index| m.word(Device {
                    area: Area::C,
                    index
                }))
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(m.double_word(d("C200")), 888);
        assert!(a(&body).qualify_native(&m).is_err());
    }
    let mut m = Memory::default();
    seed(&mut m, 0, &[7, -1, 7], true);
    run("DSER D0E0 K7 C190F0 K3", &mut m);
    assert_eq!(
        (0..5)
            .map(|i| m.double_word(Device {
                area: Area::C,
                index: 190 + i * 2
            }))
            .collect::<Vec<_>>(),
        [2, 0, 2, 1, 2]
    );
    assert!(a("DSER D0E0 K7 C190F0 K3").qualify_native(&m).is_ok());
    run("DSER D0E0 K7 C191F0 K3", &mut m);
    assert_eq!(m.word(d("C199")), 2);
    assert!(a("DSER D0E0 K7 C191F0 K3").qualify_native(&m).is_err());
}

#[test]
fn model_last_pairs_and_negative_lexical_endpoint_rescue_are_complete() {
    for last in [4999, 9999] {
        let mut m = Memory::default();
        m.data_register_limit = Some(last);
        m.set_word(d("D1028"), -1);
        m.set_word(d("D1029"), -9);
        m.set_double_word(
            Device {
                area: Area::D,
                index: last - 1,
            },
            42,
        );
        let body = format!("DSER D{last}E0 K42 D{last}F0 K1");
        run(&body, &mut m);
        assert_eq!(results(&m, last - 9, true), [1, 0, 0, 0, 0]);
        assert!(a(&body).qualify_native(&m).is_ok());
    }
}

#[test]
fn source_partial_t_or_d_is_never_mean_style_zero_fill() {
    for (body, limit) in [
        ("DSER T255E0 K1 D20F0 K1", 9999),
        ("DSER D4999E0 K1 D20F0 K1", 4999),
    ] {
        let mut m = Memory::default();
        m.data_register_limit = Some(limit);
        let mut seen = BTreeSet::new();
        a(body).execute(&mut m, true, &mut seen);
        assert!(!m.execution_limitations.is_empty());
        assert!(!m.system_bit(1067));
        assert_eq!(m.word(d("D20")), 0);
        assert!(a(body).qualify_native(&m).is_err());
    }
    let mut m = Memory::default();
    run("DSER T255E0 K1 T255F0 K0", &mut m);
    assert_eq!(m.word(d("D1068")), 13);
    assert!(a("DSER T255E0 K1 T255F0 K0").qualify_native(&m).is_ok());
}

#[test]
fn special_and_generic_low_m_qualifiers_remain_role_specific() {
    let mut m = Memory::default();
    m.set_word(d("D1028"), 3);
    let mut seen = BTreeSet::new();
    a("SER K1M1277E0 K0 D20F0 K1").execute(&mut m, true, &mut seen);
    assert!(!m.execution_limitations.is_empty());
    assert!(!m.system_bit(1067));
    // M1281..1283 are not backed by this model, so a full Kn scalar at
    // M1280 cannot establish numerical parity. Check only the proven raw
    // resolver boundary here, independently of that physical-hole policy.
    assert!(crate::indexed_multiply::qualify_group(d("M1277"), d("M1280"), 1, false).is_ok());
    assert!(crate::indexed_multiply::qualify_group(d("M1277"), d("M1280"), 1, true).is_err());
    let mut m = Memory::default();
    m.set_word(d("D1028"), 3);
    let mut seen = BTreeSet::new();
    a("ALT M1277E0").execute(&mut m, true, &mut seen);
    assert!(!m.execution_limitations.is_empty());
    assert!(
        a("SER K1M1532E0 K0 D20F0 K2")
            .qualify_native(&Memory::default())
            .is_err()
    );
}

#[test]
fn clock_payload_is_skipped_on_invalid_count_but_checked_after_target_on_valid_count() {
    use crate::system_relays::ClockProfile;
    let mut m = Memory::default();
    m.system_relays.set_profile(ClockProfile::InstalledFixed);
    let seen = run("SER K1M1011E0 K0 D20F0 K0", &mut m);
    assert_eq!(m.word(d("D1068")), 7);
    assert!(!seen.contains(&d("M1011")));
    let mut m = Memory::default();
    m.system_relays.set_profile(ClockProfile::InstalledFixed);
    m.set_word(d("D30"), 1);
    let mut seen = BTreeSet::new();
    a("SER K1M1011E0 K0 D20F0 D30").execute(&mut m, true, &mut seen);
    assert!(!m.execution_limitations.is_empty());
    assert!(seen.contains(&d("D1029")) && seen.contains(&d("D30")) && seen.contains(&d("M1011")));
    assert!(!m.system_bit(1067));
    assert_eq!(m.native_numeric_scratch, Some(0));
}

#[test]
fn native_capture_uses_actual_count_after_both_initial_bases_and_does_not_commit() {
    let action = a("SER D10E0 D20F0 D100E1 D30");
    let mut local = Memory::default();
    local.set_word(d("D30"), 3);
    local.set_word(d("D20"), 9);
    local.native_numeric_scratch = Some(77);
    let mut native = Memory::default();
    native.set_word(d("D30"), 0);
    native.set_word(d("D20"), 5);
    let mut order = Vec::new();
    let mut fetched = BTreeSet::<Device>::new();
    action
        .qualify_native_with(&local, |clone, ds| {
            order.extend(ds.iter().copied());
            fetched.extend(ds);
            hydrate(clone, &native, ds);
            Ok(())
        })
        .unwrap();
    let position = |name: &str| order.iter().position(|d0| *d0 == d(name)).unwrap();
    assert!(position("D1028") < position("D1029"));
    assert!(position("D20") < position("D1182"));
    assert!(position("D1182") < position("D30"));
    assert!(!fetched.contains(&d("D10")));
    assert_eq!(local.native_numeric_scratch, Some(77));
    assert!(!local.system_bit(1067));
    assert_eq!(local.word(d("D30")), 3);
    native.set_word(d("D1182"), -200);
    assert!(
        action
            .qualify_native_with(&local, |clone, ds| {
                hydrate(clone, &native, ds);
                Ok(())
            })
            .is_err()
    );
}

#[test]
fn monitor_needs_complete_plain_d_count_and_static_spans_are_not_guessed() {
    let mut m = Memory::default();
    m.set_double_word(d("D30"), 2);
    let action = a("DSER C199E0 K1 C200F0 D30");
    let required = action.monitor_input_devices(&m).unwrap();
    for s in ["D1028", "D1029", "D30", "D31"] {
        assert!(required.contains(&d(s)));
    }
    let ds = action.capture_devices(&m).unwrap();
    for s in ["C199", "C200", "C204"] {
        assert!(ds.contains(&d(s)));
    }
    assert!(!ds.contains(&d("C205")));
    m.set_word(d("D31"), 1);
    let ds = action.capture_devices(&m).unwrap();
    assert!(!ds.contains(&d("C199")) && !ds.contains(&d("C200")));
    let mut ds = BTreeSet::new();
    a("SER D4998 K1 D20E0 K3").devices(&mut ds);
    assert!(ds.contains(&d("D5000")));
    let mut ds = BTreeSet::new();
    a("DSER D4999 K1 D4999 D30").devices(&mut ds);
    assert!(!ds.contains(&d("D5000")));
    assert!(ds.contains(&d("D31")));
}

#[test]
fn ser_and_dser_scratch_changes_pause_and_resume_existing_word_sort() {
    use crate::ramp_sort::RampSortAction;
    let mut m = Memory::default();
    seed(&mut m, 0, &[30, 10, 20, 3, 1, 2], false);
    let sort = RampSortAction::parse("SORT", &args("D0 K3 K2 D20 K2")).unwrap();
    run("SER D10E0 K300 D100F0 K0", &mut m);
    sort.execute(&mut m, true, 1);
    assert_eq!(words(&m, 20, 6), [10, 30, 20, 1, 3, 2]);
    run("DSER D10E7 K0 D100F3 K0", &mut m);
    sort.execute(&mut m, true, 1);
    assert_eq!(words(&m, 20, 6), [10, 30, 20, 1, 3, 2]);
    run("SER D10E0 K300 D100F0 K0", &mut m);
    sort.execute(&mut m, true, 1);
    assert_eq!(words(&m, 20, 6), [10, 20, 30, 1, 2, 3]);
}

#[test]
fn alt_toggles_every_active_visit_and_moves_without_clearing_old_target() {
    let mut m = Memory::default();
    m.native_numeric_scratch = Some(123);
    m.set_internal(d("M79"), true);
    m.set_internal(d("M82"), true);
    run("ALT M80E0", &mut m);
    assert!(m.bit(d("M80")));
    run("ALT M80E0", &mut m);
    assert!(!m.bit(d("M80")));
    m.set_word(d("D1028"), 1);
    run("ALT M80E0", &mut m);
    assert!(m.bit(d("M81")));
    assert!(m.bit(d("M79")) && m.bit(d("M82")));
    assert_eq!(m.native_numeric_scratch, Some(123));
    m.set_word(d("D1028"), 0);
    m.set_system(1020, true);
    m.set_system(1021, true);
    m.set_system(1022, true);
    run("ALT M1022E0", &mut m);
    assert!(m.system_bit(1020) && m.system_bit(1021));
    assert!(!m.system_bit(1022));
}

#[test]
fn all_off_forms_skip_invalid_addresses_clock_reads_and_scratch_changes() {
    for body in [
        "SER D0E0 D20F0 D100E1 D30",
        "DSER D0E0 D20F0 D100E1 D30",
        "ALT M80E0",
    ] {
        let mut m = Memory::default();
        m.native_numeric_scratch = Some(77);
        m.set_word(d("D1028"), -30000);
        let mut seen = BTreeSet::new();
        a(body).execute(&mut m, false, &mut seen);
        assert!(seen.is_empty());
        assert!(m.execution_limitations.is_empty());
        assert_eq!(m.native_numeric_scratch, Some(77));
    }
}

#[test]
fn altp_header_history_skips_held_bad_index_and_rearms_only_after_visit_off() {
    let p = compile_il("LD X0\nALTP M80E0\nLDI X7\nMOV K1234 D60\nEND").unwrap();
    let mut sim = Simulator::default();
    for (on, offset, old, new) in [
        (false, 0, false, false),
        (true, 0, true, false),
        (true, -30000, true, false),
        (false, 1, true, false),
        (true, 1, true, true),
    ] {
        sim.memory.set_input(d("X0"), on);
        sim.memory.set_word(d("D1028"), offset);
        complete(&mut sim, &p);
        assert_eq!(sim.memory.bit(d("M80")), old);
        assert_eq!(sim.memory.bit(d("M81")), new);
        assert_eq!(sim.memory.word(d("D60")), 1234);
    }
}

#[test]
fn same_pc_loops_and_separate_occurrences_distinguish_alt_and_altp() {
    for (body, expected) in [("ALT M80E0", true), ("ALTP M80E0", true)] {
        let p = compile_il(&format!(
            "LD X0\nFOR K3\nLD X0\n{body}\nNEXT\nLDI X7\nMOV K1234 D60\nEND"
        ))
        .unwrap();
        let mut sim = Simulator::default();
        sim.memory.set_input(d("X0"), true);
        complete(&mut sim, &p);
        assert_eq!(sim.memory.bit(d("M80")), expected);
        complete(&mut sim, &p);
        assert_eq!(sim.memory.bit(d("M80")), body.starts_with("ALTP"));
    }
    let p = compile_il("LD X0\nALTP M80E0\nLD X0\nALTP M80E0\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    complete(&mut sim, &p);
    assert!(!sim.memory.bit(d("M80")));
}

#[test]
fn skipped_cj_does_not_rearm_altp_and_master_false_holds_target() {
    let p = compile_il("LD X1\nCJ P0\nLD X0\nALTP M80E0\nP0\nLDI X7\nMOV K1234 D60\nEND").unwrap();
    let mut sim = Simulator::default();
    for (on, skip, expected) in [
        (true, false, true),
        (false, true, true),
        (true, false, true),
        (false, false, true),
        (true, false, false),
    ] {
        sim.memory.set_input(d("X0"), on);
        sim.memory.set_input(d("X1"), skip);
        complete(&mut sim, &p);
        assert_eq!(sim.memory.bit(d("M80")), expected);
    }
    let p =
        compile_il("LD X7\nMC N0\nLDI X7\nALT M80E0\nMCR N0\nLDI X7\nMOV K1234 D60\nEND").unwrap();
    let mut sim = Simulator::default();
    complete(&mut sim, &p);
    assert!(!sim.memory.bit(d("M80")));
    assert_eq!(sim.memory.word(d("D60")), 1234);
}

#[test]
fn search_vm_error_next_pc_and_public_direct_natural_dser_are_integrated() {
    let p = compile_il(
        "LD X0\nDSER C255 K7 C251 K1\nLD X0\nSER D0E0 K9 D20F0 K0\nLDI X7\nMOV K1234 D60\nEND",
    )
    .unwrap();
    let encoded = crate::es2_encoder::encode(&p).unwrap();
    let error_pc = encoded.action_pc[1] + 7;
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim.memory.set_double_word(d("C255"), 7);
    complete(&mut sim, &p);
    assert_eq!(sim.memory.double_word(d("C251")), 1);
    assert_eq!(sim.memory.word(d("D1068")), error_pc as i16);
    assert_eq!(sim.memory.word(d("D60")), 1234);
}

#[test]
fn alt_all_sixteen_modifier_words_use_bit_offsets_and_ignore_neighbor_highword() {
    use crate::calendar_operand::CalendarIndices;
    for high in [false, true] {
        for bank in 0..8 {
            let suffix = format!("{}{}", if high { 'F' } else { 'E' }, bank);
            let index = CalendarIndices::alias_device(bank, high).unwrap();
            let mut m = Memory::default();
            m.set_word(index, -2);
            m.set_word(
                Device {
                    index: index.index + 1,
                    ..index
                },
                -1,
            );
            for (base, target) in [("Y200", "Y176"), ("M80", "M78"), ("S80", "S78")] {
                run(&format!("ALT {base}{suffix}"), &mut m);
                assert!(m.bit(d(target)), "{base}{suffix}");
            }
        }
    }
}

#[test]
fn serp_and_dserp_hold_snapshots_without_reading_bad_indices_until_rearm() {
    for wide in [false, true] {
        let name = if wide { "DSERP" } else { "SERP" };
        let p = compile_il(&format!(
            "LD X0\n{name} D0E0 K7 D100F0 D30\nLDI X7\nMOV K1234 D60\nEND"
        ))
        .unwrap();
        let mut sim = Simulator::default();
        sim.memory.set_double_word(d("D30"), 3);
        seed(&mut sim.memory, 0, &[7, 0, 7], wide);
        sim.memory.set_input(d("X0"), false);
        complete(&mut sim, &p);
        sim.memory.set_input(d("X0"), true);
        complete(&mut sim, &p);
        assert_eq!(results(&sim.memory, 100, wide), [2, 0, 2, 1, 2]);
        let scratch = sim.memory.native_numeric_scratch;
        sim.memory.set_word(d("D1028"), -30000);
        complete(&mut sim, &p);
        assert_eq!(results(&sim.memory, 100, wide), [2, 0, 2, 1, 2]);
        assert_eq!(sim.memory.native_numeric_scratch, scratch);
        sim.memory.set_word(d("D1028"), 0);
        sim.memory.set_input(d("X0"), false);
        complete(&mut sim, &p);
        seed(&mut sim.memory, 0, &[9, 9, 9], wide);
        sim.memory.set_input(d("X0"), true);
        complete(&mut sim, &p);
        assert_eq!(results(&sim.memory, 100, wide), [0, 0, 0, 2, 2]);
        assert_eq!(sim.memory.word(d("D60")), 1234);
    }
}

#[test]
fn isolated_vm_gate_uses_master_and_ignores_output_context() {
    for (op, operands) in [
        ("SER", "D10E0 K7 D20F0 K1"),
        ("DSER", "D10E0 K7 D20F0 K1"),
        ("ALT", "M80E0"),
    ] {
        for suffix in ["", "P"] {
            for (master, output, active) in [(1, false, true), (0, true, false)] {
                let p = compile_il(&format!("LD X0\n{op}{suffix} {operands}\nEND")).unwrap();
                let mut sim = Simulator::default();
                sim.memory.set_input(d("X0"), true);
                sim.memory.set_double_word(d("D10"), 7);
                sim.memory.set_double_word(d("D20"), 77);
                sim.begin_plan_scan(&p, 0, true).unwrap();
                sim.execute_plan_step(&p).unwrap();
                assert_eq!(sim.current_plan_pc(), Some(1));
                assert_ne!(sim.control.logic_word & 1, 0);
                sim.control.master_gate = master;
                sim.control.out_enabled = output;
                sim.execute_plan_step(&p).unwrap();
                if op == "ALT" {
                    assert_eq!(sim.memory.bit(d("M80")), active, "{op}{suffix}");
                } else {
                    assert_eq!(
                        sim.memory.word(d("D20")),
                        if active { 1 } else { 77 },
                        "{op}{suffix}"
                    );
                }
                assert!(sim.memory.execution_limitations.is_empty());
            }
        }
    }
}
