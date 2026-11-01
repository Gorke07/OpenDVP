//! Draft independent shift/FIFO contracts; parent runs tests after integration.
use crate::indexed_shift_fifo::IndexedShiftFifoAction;
use crate::plc::{Area, Device, Memory, ScanStatus, Simulator, compile_il};
use crate::shift::ShiftAction;
use std::collections::BTreeSet;

fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn args(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}
fn a(s: &str) -> IndexedShiftFifoAction {
    let v = args(s);
    IndexedShiftFifoAction::parse(&v[0], &v[1..]).unwrap()
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
fn bits(m: &mut Memory, start: u16, n: u16, value: u32) {
    for i in 0..n {
        m.set_internal(
            Device {
                area: Area::M,
                index: start + i,
            },
            value & (1u32 << i) != 0,
        );
    }
}
fn packed(m: &Memory, start: u16, n: u16) -> u32 {
    (0..n).fold(0, |v, i| {
        v | (u32::from(m.bit(Device {
            area: Area::M,
            index: start + i,
        })) << i)
    })
}
fn flags(m: &Memory) -> [bool; 3] {
    [1020, 1021, 1022].map(|i| m.system_bit(i))
}
fn seed_flags(m: &mut Memory) {
    m.set_system(1020, true);
    m.set_system(1021, true);
    m.set_system(1022, true);
}
fn hydrate(dst: &mut Memory, original: &Memory, devices: &BTreeSet<Device>) {
    for &device in devices {
        match device.area {
            Area::X | Area::Y | Area::M | Area::S => dst.set_bit(device, original.bit(device)),
            Area::C if device.index >= 200 => {
                dst.set_double_word_raw(device, original.double_word(device))
            }
            _ => dst.set_word(device, original.word(device)),
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
fn sixteen_spellings_preserve_original_headers_fields_and_lengths() {
    for (name, operands, header, len) in [
        ("SMOV", "D10E0 K4 K2 D20F1 K3", 0x2a00, 11),
        ("SFTR", "M0E0 M80F7 K8 K2", 0x5400, 9),
        ("SFTL", "M0E0 M80F7 K8 K2", 0x5600, 9),
        ("WSFR", "D10E0 D20F1 K5 K2", 0x5800, 9),
        ("WSFL", "D10E0 D20F1 K5 K2", 0x5a00, 9),
        ("SFWR", "D10E0 D20F1 K4", 0x5c00, 7),
        ("SFRD", "D10E0 D20F1 K4", 0x5e00, 7),
        ("ZRST", "C199E0 C200F1", 0x6000, 5),
    ] {
        for p in ["", "P"] {
            let body = format!("{name}{p} {operands}");
            let action = a(&body);
            let words = action.native_words().unwrap();
            assert_eq!(words.len(), len);
            assert_eq!(words[0], header | if p.is_empty() { 0 } else { 0x10 });
            assert_eq!(action.label(), body);
        }
    }
    assert_eq!(
        a("SMOV D10E0 K4 K2 D20F1 K3").native_words().unwrap(),
        [
            0x2a00, 0x14e6, 0x0086, 0x0480, 0x0080, 0x0280, 0x0080, 0x28f6, 0x8086, 0x0380, 0x0080
        ]
    );
    assert_eq!(
        a("SFTR M0E0 M80F7 K8 K2").native_words().unwrap(),
        [
            0x5400, 0x00e4, 0x0880, 0x50f4, 0xe880, 0x0880, 0x0080, 0x0280, 0x0080
        ]
    );
    assert_eq!(
        a("SFTL X0E1 Y200F2 K8 K2").native_words().unwrap()[1..5],
        [0x00e4, 0x2480, 0x80f4, 0x4580]
    );
    assert_eq!(
        a("ZRST T20F1 T21E0").native_words().unwrap()[1..3],
        [0x28f6, 0x2082]
    );
    assert_eq!(
        a("ZRST C200E1 C255F0").native_words().unwrap()[1..3],
        [0x00e6, 0x2284]
    );
    assert_eq!(
        a("ZRST C200 C255E0").native_words().unwrap()[1..3],
        [0xc880, 0x0084]
    );
}

#[test]
fn pdf_roles_counts_and_readonly_reset_holes_remain_rejected() {
    for body in [
        "SMOV K1 K4 K2 D10E0 K3",
        "SMOV X0E0 K4 K2 D10 K3",
        "SMOV D0E0 K2 K3 D10 K4",
        "SMOV D0E0 K4 K2 D10 K1",
        "SMOV D0E0 D2 K2 D10 K3",
        "SMOV D0E0 K4 K2 K1X0 K3",
        "SFTR K1M0E0 M80 K4 K1",
        "SFTL D0E0 M80 K4 K1",
        "SFTR M0E0 X0 K4 K1",
        "SFTR M0E0 M80 K0 K1",
        "SFTR M0E0 M80 K1025 K1",
        "WSFR E0 D20F0 K4 K1",
        "WSFL K1M0E0 K2M80 K4 K1",
        "WSFR D0E0 K1M80 K4 K1",
        "WSFR D0E0 D20 K4 D2",
        "WSFR C200E0 D20 K4 K1",
        "SFWR K1 E0 K4",
        "SFWR K1 D20E0 K1",
        "SFWR K1 D20E0 K513",
        "SFRD K1X0E0 D20 K4",
        "SFRD D20E0 X0 K4",
        "ZRST E0 E1",
        "ZRST X0E0 X1",
        "ZRST M0E0 S10",
        "ZRST K1M0E0 K1M10",
        "ZRST M999 M1003",
    ] {
        let v = args(body);
        assert!(
            IndexedShiftFifoAction::parse(&v[0], &v[1..]).is_err(),
            "{body}"
        );
    }
    let mut m = Memory::default();
    a("ZRST M999E0 M1003F1").execute(&mut m, true, &mut BTreeSet::new());
    assert!(!m.execution_limitations.is_empty());
    for body in [
        "SMOV F7 K4 K2 D20E0 K3",
        "SFWR F7 D20E0 K4",
        "SFRD D20E0 F7 K4",
        "ZRST C199E0 C255F7",
        "WSFR K1X0E0 K1M128F0 K4 K1",
    ] {
        a(body);
    }
    for name in ["DSFWR", "DZRST", "SMOVPP", "WSFRPP"] {
        assert!(!IndexedShiftFifoAction::supports(name));
    }
}

#[test]
fn every_modifier_bank_is_a_signed_word_for_both_roles() {
    for (bank, aliases) in [
        ('E', [1028, 1182, 1184, 1186, 1188, 1190, 1192, 1194]),
        ('F', [1029, 1183, 1185, 1187, 1189, 1191, 1193, 1195]),
    ] {
        for (n, alias) in aliases.into_iter().enumerate() {
            for offset in [-1, 1] {
                let suffix = format!("{bank}{n}");
                for body in [
                    format!("SMOV D10{suffix} K4 K2 D40{suffix} K3"),
                    format!("SFTR M10{suffix} M80{suffix} K8 K2"),
                    format!("SFTL M10{suffix} M80{suffix} K8 K2"),
                    format!("WSFR D10{suffix} D40{suffix} K5 K2"),
                    format!("WSFL D10{suffix} D40{suffix} K5 K2"),
                    format!("SFWR D10{suffix} D40{suffix} K4"),
                    format!("SFRD D10{suffix} D40{suffix} K4"),
                    format!("ZRST D10{suffix} D14{suffix}"),
                ] {
                    let mut m = Memory::default();
                    m.set_word(
                        Device {
                            area: Area::D,
                            index: alias,
                        },
                        offset,
                    );
                    if bank == 'F' && n > 0 && n < 7 {
                        m.set_word(
                            Device {
                                area: Area::D,
                                index: alias + 1,
                            },
                            0x6a5a,
                        );
                    }
                    m.set_word(d("D9"), 1);
                    m.set_word(d("D10"), 1);
                    m.set_word(d("D11"), 1);
                    m.set_word(d("D39"), 0);
                    m.set_word(d("D41"), 0);
                    let seen = run(&body, &mut m);
                    assert!(
                        seen.contains(&Device {
                            area: Area::D,
                            index: alias
                        }),
                        "{body}"
                    );
                }
            }
        }
    }
}

#[test]
fn smov_decimal_hex_and_invalid_old_value_keep_exact_scratch_and_flags() {
    for (hex, source, old, result, scratch, error) in [
        (false, 1234, 5678, 5128, 1, false),
        (true, 0x1234, 0x5678, 0x5128, 0x1234, false),
        (false, -1, 77, 77, -1, true),
        (false, 1234, 10000, 10000, 1234, true),
    ] {
        let mut m = Memory::default();
        seed_flags(&mut m);
        m.set_system(1168, hex);
        m.set_word(d("D0"), source);
        m.set_word(d("D20"), old);
        let seen = run("SMOV D0E0 K4 K2 D20F0 K3", &mut m);
        assert_eq!(m.word(d("D20")), result);
        assert_eq!(m.native_numeric_scratch, Some(scratch));
        assert_eq!(flags(&m), [true; 3]);
        assert!(seen.contains(&d("D0")) && seen.contains(&d("D20")) && seen.contains(&d("M1168")));
        assert_eq!(m.system_bit(1067), error);
        if error {
            assert_eq!(m.word(d("D1067")) as u16, 0xe18);
            assert_eq!(m.word(d("D1068")), 11);
        }
    }
}

#[test]
fn smov_uses_snapshots_truncates_short_target_and_preserves_neighbor_index() {
    let mut m = Memory::default();
    m.set_system(1168, true);
    m.set_word(d("D10"), 0x1234);
    bits(&mut m, 128, 8, 0xa5);
    m.set_internal(d("M127"), true);
    run("SMOV D10E0 K4 K2 K1M128F0 K2", &mut m);
    assert_eq!(packed(&m, 128, 8), 0xa2);
    assert!(m.bit(d("M127")));
    m.set_system(1168, false);
    m.set_word(d("D10"), 1234);
    m.set_word(d("D1182"), 0);
    m.set_word(d("D1183"), 77);
    run("SMOV D10E0 K4 K4 D1182E1 K4", &mut m);
    assert_eq!(m.word(d("D1182")), 1234);
    assert_eq!(m.word(d("D1183")), 77);
}

#[test]
fn raw_shifts_read_after_movement_and_keep_source_direction() {
    for (op, expected) in [("SFTR", 0b01101), ("SFTL", 0b10101)] {
        let mut m = Memory::default();
        seed_flags(&mut m);
        m.native_numeric_scratch = Some(0x55aa);
        bits(&mut m, 128, 5, 0b10101);
        run(&format!("{op} M128E0 M128F0 K5 K2"), &mut m);
        assert_eq!(packed(&m, 128, 5), expected);
        assert_eq!(flags(&m), [true; 3]);
        assert_eq!(m.native_numeric_scratch, Some(0x55aa));
    }
    // With no shift remainder, left raw injection is descending: source
    // M128..135 into M129..136 does not propagate bit0 to every later bit.
    let mut m = Memory::default();
    bits(&mut m, 128, 9, 1);
    run("SFTL M128E0 M129F0 K8 K8", &mut m);
    assert_eq!(packed(&m, 128, 9), 3);
}

#[test]
fn grouped_word_left_injection_is_ascending_and_differs_from_raw_left() {
    for op in ["WSFR", "WSFL"] {
        let mut m = Memory::default();
        bits(&mut m, 128, 9, 1);
        run(&format!("{op} K2M128E0 K2M129F0 K1 K1"), &mut m);
        assert_eq!(packed(&m, 128, 9), 0x1ff);
        assert_eq!(m.native_numeric_scratch, Some(0x880));
    }
    let mut m = Memory::default();
    bits(&mut m, 128, 20, 0x38421);
    run("WSFR K1M136E0 K1M128F0 K5 K2", &mut m);
    assert_eq!(packed(&m, 128, 20), 0x33384);
    assert_eq!(m.native_numeric_scratch, Some(0x888));
}

#[test]
fn numeric_word_shifts_keep_column_order_and_snapshot_only_each_live_read() {
    for (op, expected) in [
        ("WSFR", [30, 40, 50, 30, 30]),
        ("WSFL", [10, 20, 10, 20, 30]),
    ] {
        let mut m = Memory::default();
        for (i, v) in [10, 20, 30, 40, 50].into_iter().enumerate() {
            m.set_word(
                Device {
                    area: Area::D,
                    index: i as u16,
                },
                v,
            );
        }
        run(&format!("{op} D2E0 D0F0 K5 K2"), &mut m);
        for (i, v) in expected.into_iter().enumerate() {
            assert_eq!(
                m.word(Device {
                    area: Area::D,
                    index: i as u16
                }),
                v
            );
        }
        assert_eq!(m.native_numeric_scratch, Some(2));
    }
}

#[test]
fn shift_cursors_remain_cached_after_destination_overwrites_their_indices() {
    let mut m = Memory::default();
    m.set_word(d("D1182"), 0);
    m.set_word(d("D1183"), 1);
    m.set_word(d("D1184"), 77);
    m.set_word(d("D1185"), 88);
    m.set_word(d("D10"), 9);
    run("WSFR D10F0 D1182E1 K3 K1", &mut m);
    assert_eq!(
        [
            m.word(d("D1182")),
            m.word(d("D1183")),
            m.word(d("D1184")),
            m.word(d("D1185"))
        ],
        [1, 77, 9, 88]
    );
}

#[test]
fn push_snapshots_source_then_rebases_after_generic_header_store() {
    let mut m = Memory::default();
    m.set_word(d("D1182"), 0);
    m.set_word(d("D1183"), 77);
    m.set_word(d("D1184"), 88);
    run("SFWR K9 D1182E1 K4", &mut m);
    assert_eq!(
        [m.word(d("D1182")), m.word(d("D1183")), m.word(d("D1184"))],
        [1, 77, 9]
    );
    assert_eq!(m.native_numeric_scratch, Some(9));
    let mut m = Memory::default();
    bits(&mut m, 1019, 4, 8);
    run("SFWR K1M1019E0 D20F0 K4", &mut m);
    assert_eq!(m.word(d("D21")), 8);
    assert_eq!(m.native_numeric_scratch, Some(8));
    assert!(!m.system_bit(1022));
}

#[test]
fn short_head_wrap_keeps_untruncated_payload_slot_and_guards() {
    let mut m = Memory::default();
    bits(&mut m, 128, 4, 15);
    bits(&mut m, 132, 4, 6);
    bits(&mut m, 192, 4, 2);
    m.set_internal(d("M127"), true);
    m.set_internal(d("M196"), true);
    run("SFWR K9 K1M128E0 K17", &mut m);
    assert_eq!(packed(&m, 128, 4), 0);
    assert_eq!(packed(&m, 132, 4), 6);
    assert_eq!(packed(&m, 192, 4), 9);
    assert!(m.bit(d("M127")) && m.bit(d("M196")));
    assert!(!m.system_bit(1022));
}

#[test]
fn fifo_full_empty_negative_and_word_wrap_have_distinct_flag_order() {
    let mut m = Memory::default();
    seed_flags(&mut m);
    m.set_word(d("D20"), 3);
    run("SFWR K7 D20E0 K4", &mut m);
    assert_eq!(m.word(d("D20")), 3);
    assert!(m.system_bit(1022));
    assert_eq!(m.native_numeric_scratch, Some(7));
    m.set_word(d("D20"), -1);
    run("SFWR K9 D20E0 K4", &mut m);
    assert!(m.system_bit(1022));
    assert_eq!(m.word(d("D1067")) as u16, 0xe1a);
    assert_eq!(m.word(d("D1068")), 3);
    assert_eq!(m.native_numeric_scratch, Some(9));
    let mut m = Memory::default();
    seed_flags(&mut m);
    m.set_word(d("D20"), -1);
    run("SFRD D20E0 D40F0 K4", &mut m);
    assert!(!m.system_bit(1020));
    assert_eq!(m.word(d("D1068")), 1);
    assert!(m.system_bit(1021) && m.system_bit(1022));
    let mut m = Memory::default();
    m.set_word(d("D1029"), -30000);
    m.set_word(d("D40"), 77);
    let seen = run("SFRD D20E0 D40F0 K4", &mut m);
    assert!(m.system_bit(1020));
    assert!(!seen.contains(&d("D1029")));
    assert_eq!(m.word(d("D40")), 77);
    let mut m = Memory::default();
    m.set_word(d("D20"), 32767);
    assert!(a("SFWR K9 D20E0 K4").qualify_native(&m).is_err());
    run("SFWR K9 D20E0 K4", &mut m);
    assert_eq!(m.word(d("D20")), 9);
    assert!(!m.system_bit(1022));
}

#[test]
fn pop_decrements_then_rebases_and_shifts_capacity_with_last_slot_retained() {
    let mut m = Memory::default();
    m.set_word(d("D1182"), 99);
    m.set_word(d("D1183"), 1);
    m.set_word(d("D1184"), 77);
    m.set_word(d("D1185"), 88);
    run("SFRD D1182F1 D20E0 K4", &mut m);
    assert_eq!(m.word(d("D20")), 0);
    assert_eq!(
        [
            m.word(d("D1182")),
            m.word(d("D1183")),
            m.word(d("D1184")),
            m.word(d("D1185"))
        ],
        [99, 77, 88, 88]
    );
    assert!(m.system_bit(1020));
    let mut m = Memory::default();
    for (i, v) in [1, 11, 22, 33, 44].into_iter().enumerate() {
        m.set_word(
            Device {
                area: Area::D,
                index: 20 + i as u16,
            },
            v,
        );
    }
    run("SFRD D20E0 D23F0 K5", &mut m);
    assert_eq!(
        (21..=24)
            .map(|index| m.word(Device {
                area: Area::D,
                index
            }))
            .collect::<Vec<_>>(),
        [22, 11, 44, 44]
    );
}

#[test]
fn native_capture_preserves_header_output_and_same_value_store_overlay() {
    let mut actual = Memory::default();
    actual.set_word(d("D1182"), 99);
    actual.set_word(d("D1183"), 1);
    actual.set_word(d("D1184"), 77);
    actual.set_word(d("D1185"), 88);
    let mut stale = Memory::default();
    stale.set_word(d("D1183"), -30000);
    let mut requests = Vec::new();
    let mut witnessed = false;
    a("SFRD D1182F1 D1184 K4")
        .qualify_native_with(&stale, |clone, ds| {
            if ds.contains(&d("D1185")) {
                assert_eq!(clone.word(d("D1183")), 0);
                assert_eq!(clone.word(d("D1184")), 0);
                witnessed = true;
            }
            requests.extend(ds.iter().copied());
            hydrate(clone, &actual, ds);
            Ok(())
        })
        .unwrap();
    assert!(witnessed);
    assert_eq!(requests.iter().filter(|x| **x == d("D1183")).count(), 1);
    assert!(!requests.contains(&d("D1184")));
    assert_eq!(stale.word(d("D1183")), -30000);
    assert_eq!(stale.word(d("D1184")), 0);
    // First same-value payload store still owns the cell, so a later live
    // shift cannot overwrite it with the original native backing value.
    let mut actual = Memory::default();
    for (i, v) in [1, 7, 7, 11].into_iter().enumerate() {
        actual.set_word(
            Device {
                area: Area::D,
                index: 20 + i as u16,
            },
            v,
        );
    }
    let mut same = Memory::default();
    same.set_word(d("D22"), 7);
    let mut fetched = Vec::new();
    a("SFRD D20E0 D22F0 K4")
        .qualify_native_with(&same, |clone, ds| {
            fetched.extend(ds.iter().copied());
            hydrate(clone, &actual, ds);
            Ok(())
        })
        .unwrap();
    assert!(!fetched.contains(&d("D22")));
}

#[test]
fn native_capture_does_not_restore_zero_flag_over_a_written_payload_bit() {
    let mut actual = Memory::default();
    bits(&mut actual, 1015, 4, 1);
    bits(&mut actual, 1019, 4, 0);
    let mut fetched = BTreeSet::<Device>::new();
    let mut saw_payload = false;
    a("SFRD K1M1015E0 D20F0 K2")
        .qualify_native_with(&Memory::default(), |clone, ds| {
            if ds.contains(&d("M1019")) {
                assert!(clone.system_bit(1020));
                saw_payload = true;
            }
            fetched.extend(ds);
            hydrate(clone, &actual, ds);
            Ok(())
        })
        .unwrap();
    assert!(saw_payload);
    assert!(!fetched.contains(&d("M1020")));
    run("SFRD K1M1015E0 D20F0 K2", &mut actual);
    assert_eq!(actual.word(d("D20")), 2);
}

#[test]
fn native_capture_uses_actual_mode_and_late_unsafe_target_never_commits() {
    let mut actual = Memory::default();
    actual.set_system(1168, true);
    actual.set_word(d("D0"), 0x1234);
    actual.set_word(d("D20"), 0x5678);
    let stale = Memory::default();
    let mut saw_mode = false;
    a("SMOV D0E0 K4 K2 D20F0 K3")
        .qualify_native_with(&stale, |clone, ds| {
            saw_mode |= ds.contains(&d("M1168"));
            hydrate(clone, &actual, ds);
            Ok(())
        })
        .unwrap();
    assert!(saw_mode);
    assert!(!stale.system_bit(1168));
    assert_eq!(stale.word(d("D20")), 0);
    // Header store sets its own E1=1. The lexical late queue is still valid
    // here; use a bounded model limit to expose a rebase beyond the last word.
    let mut m = Memory::default();
    m.data_register_limit = Some(1183);
    m.set_word(d("D1182"), 0);
    assert!(a("SFWR K9 D1182E1 K4").qualify_native(&m).is_err());
    assert_eq!(m.word(d("D1182")), 0);
}

#[test]
fn unknown_clock_limits_at_actual_reads_and_preserves_preceding_live_moves() {
    use crate::system_relays::ClockProfile;
    let mut m = Memory::default();
    m.system_relays.set_profile(ClockProfile::InstalledFixed);
    m.native_numeric_scratch = Some(77);
    m.set_word(d("D1029"), -30000);
    let mut seen = BTreeSet::new();
    a("SMOV K1M1011E0 K4 K2 D20F0 K3").execute(&mut m, true, &mut seen);
    assert!(!m.execution_limitations.is_empty());
    assert!(!seen.contains(&d("D1029")));
    assert_eq!(m.native_numeric_scratch, Some(77));
    assert!(!m.system_bit(1067));
    let mut m = Memory::default();
    m.system_relays.set_profile(ClockProfile::InstalledFixed);
    bits(&mut m, 128, 8, 0xa5);
    a("WSFR K1M1011E0 K1M128F0 K2 K1").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(packed(&m, 128, 8), 0xaa);
    assert_eq!(m.native_numeric_scratch, Some(0xbf3));
    assert!(!m.execution_limitations.is_empty());
    assert!(!m.system_bit(1067));
    let mut m = Memory::default();
    m.system_relays.set_profile(ClockProfile::InstalledFixed);
    bits(&mut m, 128, 4, 9);
    a("SFTR M1011E0 M128F0 K4 K1").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(packed(&m, 128, 4), 12);
    assert!(!m.execution_limitations.is_empty());
}

#[test]
fn seeded_and_documented_clock_sources_keep_literal_values_without_fabricating_seeds() {
    use crate::system_relays::{ClockProfile, ClockState};
    for (profile, expected) in [
        (ClockProfile::Documented, 0),
        (ClockProfile::InstalledFixed, 5),
    ] {
        let mut m = Memory::default();
        m.system_relays.set_profile(profile);
        if profile == ClockProfile::InstalledFixed {
            ClockState::seed_installed_levels(&mut m, [true, false, true, false]).unwrap();
        }
        run("SFWR K1M1011E0 D20F0 K4", &mut m);
        assert_eq!(m.word(d("D21")), expected);
        run("SFTR M1011E0 M128F0 K4 K4", &mut m);
        assert_eq!(packed(&m, 128, 4), expected as u32);
        run("WSFL K1M1011E0 K1M160F0 K1 K1", &mut m);
        assert_eq!(packed(&m, 160, 4), expected as u32);
        run("SMOV K1M1011E0 K1 K1 D40F0 K1", &mut m);
        assert_eq!(m.word(d("D40")), expected);
    }
    let mut unknown = Memory::default();
    unknown
        .system_relays
        .set_profile(ClockProfile::InstalledFixed);
    let mut actual = Memory::default();
    for index in 1011..=1014 {
        actual.set_system(index, true);
    }
    assert!(
        a("SFWR K1M1011E0 D20F0 K4")
            .qualify_native_with(&unknown, |clone, ds| {
                hydrate(clone, &actual, ds);
                Ok(())
            })
            .is_err()
    );
    assert!(unknown.system_relays.clock_levels().is_none());
}

#[test]
fn zrsts_cache_both_endpoints_reset_natural_counters_and_keep_scratch() {
    let mut m = Memory::default();
    m.native_numeric_scratch = Some(123);
    m.set_word(d("D1182"), 0);
    m.set_word(d("D1183"), 2);
    m.set_word(d("D1184"), 77);
    m.set_word(d("D1185"), 88);
    run("ZRST D1182E1 D1182F1", &mut m);
    assert_eq!(
        [
            m.word(d("D1182")),
            m.word(d("D1183")),
            m.word(d("D1184")),
            m.word(d("D1185"))
        ],
        [0, 0, 0, 88]
    );
    assert_eq!(m.native_numeric_scratch, Some(123));
    m.set_word(d("C199"), 77);
    m.set_double_word(d("C200"), 0x12345678);
    m.set_double_word(d("C201"), 0x76543210);
    run("ZRST C199E0 C201F0", &mut m);
    assert_eq!(m.word(d("C199")), 0);
    assert_eq!(m.double_word(d("C200")), 0);
    assert_eq!(m.double_word(d("C201")), 0);
    m.set_word(d("D10"), 10);
    m.set_word(d("D20"), 20);
    run("ZRST D20E0 D10F0", &mut m);
    assert_eq!(m.word(d("D10")), 0);
    assert_eq!(m.word(d("D20")), 20);
}

#[test]
fn full_model_spans_static_direct_roles_and_native_gap_limits_are_distinct() {
    for body in [
        "WSFR D4998 D0E0 K3 K3",
        "WSFL D0E0 D4998 K3 K1",
        "SFWR D0E0 D4998 K3",
        "SFRD D4998 D0E0 K3",
    ] {
        let mut ds = BTreeSet::new();
        a(body).devices(&mut ds);
        assert!(ds.contains(&d("D5000")), "{body}");
    }
    for last in [4999, 9999] {
        let mut m = Memory::default();
        m.data_register_limit = Some(last);
        m.set_word(d("D1028"), -1);
        run(&format!("WSFR D0F0 D{last}E0 K2 K1"), &mut m);
        assert!(
            a(&format!("WSFR D0F0 D{last}F0 K2 K1"))
                .qualify_native(&m)
                .is_err()
        );
    }
    for body in [
        "SFTR M128E0 M1532 K8 K1",
        "WSFL K1M128E0 K1M1532 K4 K1",
        "SFWR K1 K1M1532 K4",
        "SFRD K1M1532 D0E0 K4",
    ] {
        let mut m = Memory::default();
        bits(&mut m, 1532, 4, 1);
        assert!(a(body).qualify_native(&m).is_err(), "{body}");
    }
    for body in ["SFTR M1280E0 M80 K1 K1", "WSFR K1M1280E0 K1M80 K1 K1"] {
        assert!(a(body).qualify_native(&Memory::default()).is_err());
    }
    // ZRST owns a documented reverse gap repair, unlike raw pointer shifts.
    let mut m = Memory::default();
    m.set_internal(d("M1534"), true);
    m.set_internal(d("M1537"), true);
    a("ZRST M1534 M1537E0").qualify_native(&m).unwrap();
    run("ZRST M1534 M1537E0", &mut m);
    assert!(!m.bit(d("M1534")) && !m.bit(d("M1537")));
}

#[test]
fn ordinary_shared_scratch_kernel_wrap_and_reset_timer_behavior_are_preserved() {
    assert!(ShiftAction::parse("ZRST", &args("M999 M1003")).is_err());
    assert!(ShiftAction::parse("ZRST", &args("T0 T0")).is_ok());
    assert!(ShiftAction::parse("ZRST", &args("C199 C201")).is_ok());
    for (body, expected) in [
        ("SMOV D0 K4 K2 D20 K3", 1),
        ("WSFR K1M128 K1M160 K2 K1", 0x880),
        ("SFWR D0 D40 K4", 1234),
    ] {
        let mut m = Memory::default();
        m.set_word(d("D0"), 1234);
        m.set_word(d("D20"), 5678);
        let v = args(body);
        ShiftAction::parse(&v[0], &v[1..]).unwrap().execute(&mut m);
        assert_eq!(m.native_numeric_scratch, Some(expected), "{body}");
    }
    let mut m = Memory::default();
    m.set_word(d("D20"), 32767);
    ShiftAction::parse("SFWR", &args("K9 D20 K4"))
        .unwrap()
        .execute(&mut m);
    assert_eq!(m.word(d("D20")), 9);
    assert!(!m.system_bit(1022));
    let p = compile_il("LD X0\nTMR T0 K5\nZRST T0E0 T0F0\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim.scan(&p, 100);
    assert_eq!(sim.memory.word(d("T0")), 1);
}

#[test]
fn off_visits_skip_invalid_addresses_and_p_vm_sites_rearm_only_after_off() {
    for body in [
        "SMOV D0E0 K4 K2 D20F0 K3",
        "SFTR M0E0 M80F0 K8 K2",
        "SFTL M0E0 M80F0 K8 K2",
        "WSFR D0E0 D20F0 K5 K2",
        "WSFL D0E0 D20F0 K5 K2",
        "SFWR D0E0 D20F0 K4",
        "SFRD D0E0 D20F0 K4",
        "ZRST D0E0 D20F0",
    ] {
        let mut m = Memory::default();
        m.set_word(d("D1028"), -30000);
        m.set_word(d("D1029"), -30000);
        m.native_numeric_scratch = Some(77);
        let mut seen = BTreeSet::new();
        a(body).execute(&mut m, false, &mut seen);
        assert!(seen.is_empty() && m.execution_limitations.is_empty());
        assert_eq!(m.native_numeric_scratch, Some(77));
    }
    let p = compile_il("LD X0\nSFWRP K7 D20E0 K4\nLDI X7\nMOV K1234 D60\nEND").unwrap();
    let mut sim = Simulator::default();
    for (on, head) in [(false, 0), (true, 1), (true, 1), (false, 1), (true, 2)] {
        sim.memory.set_input(d("X0"), on);
        complete(&mut sim, &p);
        assert_eq!(sim.memory.word(d("D20")), head);
        assert_eq!(sim.memory.word(d("D60")), 1234);
    }
}

#[test]
fn loop_branch_master_and_step_vm_use_original_instruction_boundaries() {
    let p=compile_il("LD X0\nFOR K3\nLD X0\nSFWRP K7 D20E0 K8\nNEXT\nLD X0\nMPS\nWSFR D0E0 D40F0 K3 K1\nMRD\nMOV K1 E1\nMPP\nOUT Y0\nLD X7\nMC N0\nLDI X7\nSFWR K9 D80E0 K4\nMCR N0\nLDI X7\nMOV K1234 D60\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    complete(&mut sim, &p);
    assert_eq!(sim.memory.word(d("D20")), 1);
    assert_eq!(sim.memory.word(d("D80")), 0);
    assert_eq!(sim.memory.word(d("D60")), 1234);
    let p =
        compile_il("STL S0\nLDI X7\nSFWR K9 D20E0 K4\nRET\nLDI X7\nMOV K1234 D60\nEND").unwrap();
    let mut sim = Simulator::default();
    complete(&mut sim, &p);
    assert_eq!(sim.memory.word(d("D20")), 0);
    assert_eq!(sim.memory.word(d("D60")), 1234);
}
