//! DRAFT: independent block/cursor/exchange contracts; no native execution.
use crate::indexed_block_exchange::IndexedBlockExchangeAction;
use crate::plc::{Area, Device, Memory, ScanStatus, Simulator, compile_il};
use std::collections::BTreeSet;

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn args(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(str::to_owned).collect()
}
fn action(body: &str) -> IndexedBlockExchangeAction {
    let words = args(body);
    IndexedBlockExchangeAction::parse(&words[0], &words[1..]).unwrap()
}
fn run(body: &str, m: &mut Memory) -> BTreeSet<Device> {
    let mut seen = BTreeSet::new();
    action(body).execute(m, true, &mut seen);
    assert!(
        m.execution_limitations.is_empty(),
        "{body}: {:?}",
        m.execution_limitations
    );
    seen
}
fn flags(m: &Memory) -> [bool; 3] {
    [1020, 1021, 1022].map(|n| m.system_bit(n))
}
fn seed_flags(m: &mut Memory) {
    m.set_system(1020, true);
    m.set_system(1021, false);
    m.set_system(1022, true);
}
fn write_bits(m: &mut Memory, base: Device, width: u16, value: u32) {
    for n in 0..width {
        let bit = Device {
            index: base.index + n,
            ..base
        };
        if bit.area == Area::X {
            m.set_input(bit, value & (1 << n) != 0);
        } else {
            m.set_internal(bit, value & (1 << n) != 0);
        }
    }
}
fn read_bits(m: &Memory, base: Device, width: u16) -> u32 {
    (0..width).fold(0, |v, n| {
        v | (u32::from(m.bit(Device {
            index: base.index + n,
            ..base
        })) << n)
    })
}
fn complete(sim: &mut Simulator, p: &crate::plc::Program) {
    sim.scan(p, 0);
    assert_eq!(sim.last_scan, ScanStatus::Complete);
}

#[test]
fn all_ten_forms_keep_literal_original_headers_fields_and_lengths() {
    for (name, header, wide, count) in [
        ("BMOV", 0x2e00, false, true),
        ("FMOV", 0x3000, false, true),
        ("DFMOV", 0x3100, true, true),
        ("XCH", 0x3200, false, false),
        ("DXCH", 0x3300, true, false),
    ] {
        for pulse in [false, true] {
            let body = format!(
                "{name}{} D10E0 D20F1{}",
                if pulse { "P" } else { "" },
                if count { " K3" } else { "" }
            );
            let a = action(&body);
            let mut expected = vec![header | if pulse { 0x10 } else { 0 }, 0x14e6, 0x0086];
            if wide {
                expected.extend([0x0080, 0x0080]);
            }
            expected.extend([0x28f6, 0x8086]);
            if wide {
                expected.extend([0x0080, 0x0080]);
            }
            if count {
                expected.extend([0x0380, 0x0080]);
                if wide {
                    expected.extend([0x0080, 0x0080]);
                }
            }
            assert_eq!(a.native_words().unwrap(), expected, "{body}");
            assert_eq!(
                expected.len(),
                match (count, wide) {
                    (true, true) => 13,
                    (true, false) => 7,
                    (false, true) => 9,
                    _ => 5,
                }
            );
            assert_eq!(a.label(), body);
            assert_eq!(a.wide(), wide);
            assert_eq!(a.modifier_devices(), [d("D1028"), d("D1183")]);
            assert_eq!(
                a.control_devices(),
                if count { vec![] } else { vec![d("M1303")] }
            );
        }
    }
    assert_eq!(
        action("BMOV K1X0E1 K1M128F7 D2").native_words().unwrap(),
        [0x2e00, 0x00e4, 0x2482, 0x80f4, 0xe882, 0x0486, 0x0086]
    );
    assert_eq!(
        action("DXCH C199E1 E1").native_words().unwrap(),
        [
            0x3300, 0x8ee6, 0x2184, 0x0080, 0x0080, 0x6c86, 0x0180, 0x0080, 0x0080
        ]
    );
}

#[test]
fn role_matrices_counts_and_complete_double_targets_are_not_widened() {
    for body in [
        "BMOV E0 D0E1 K1",
        "BMOV K1 D0E1 K1",
        "BMOV D0E1 E0 K1",
        "BMOV K1M0E0 K2M80F0 K2",
        "BMOV D0E0 D20F0 D2E0",
        "BMOV D0E0 D20F0 E0",
        "BMOV D0E0 D20F0 K1M0",
        "BMOV C200E0 D0 K1",
        "BMOV D0E0 C200 K1",
        "FMOV K1 E0 K1",
        "DFMOV K1 F0 K1",
        "DFMOV F0 D20E0 K1",
        "FMOV K1 D20E0 D2",
        "DFMOV K1 D20E0 D2",
        "XCH K1 D0E0",
        "XCH K1X0E0 D20",
        "DXCH F1 D20E0",
        "DFMOV K1 T255 K1",
        "DFMOV K1 D9999 K1",
        "DFMOV K1 C255 K2",
    ] {
        let words = args(body);
        assert!(
            IndexedBlockExchangeAction::parse(&words[0], &words[1..]).is_err(),
            "{body}"
        );
    }
    for name in ["BMOV", "FMOV", "DFMOV"] {
        for n in ["K0", "K-1", "K513", "HFFFF"] {
            let body = if name == "BMOV" {
                format!("D0E0 D20F0 {n}")
            } else {
                format!("K1 D20F0 {n}")
            };
            assert!(IndexedBlockExchangeAction::parse(name, &args(&body)).is_err());
        }
    }
    for body in [
        "FMOV F7 D20E0 K2",
        "DFMOV E7 D20E0 K2",
        "XCH F7 D20E0",
        "DXCH E7 D20E0",
        "BMOV D0E0 D20F0 T0",
        "BMOV D0E0 D20F0 C199",
    ] {
        action(body);
    }
    assert!(!IndexedBlockExchangeAction::supports("DBMOV"));
    assert!(!IndexedBlockExchangeAction::supports("FMOVPP"));
}

#[test]
fn every_modifier_bank_is_signed_word_and_fill_caches_the_effective_cursor() {
    for (prefix, aliases) in [
        ('E', [1028, 1182, 1184, 1186, 1188, 1190, 1192, 1194]),
        ('F', [1029, 1183, 1185, 1187, 1189, 1191, 1193, 1195]),
    ] {
        for (bank, alias) in aliases.into_iter().enumerate() {
            for wide in [false, true] {
                let mut m = Memory::default();
                seed_flags(&mut m);
                m.set_word(d(&format!("D{alias}")), -2);
                m.set_word(d(&format!("D{}", alias + 1)), 0x4567);
                m.set_word(d("D37"), 111);
                m.set_word(d("D42"), 222);
                let body = if wide {
                    format!("DFMOV H12345678 D40{prefix}{bank} K2")
                } else {
                    format!("FMOV H5678 D40{prefix}{bank} K2")
                };
                run(&body, &mut m);
                if wide {
                    assert_eq!(m.double_word(d("D38")), 0x12345678);
                    assert_eq!(m.double_word(d("D40")), 0x12345678);
                } else {
                    assert_eq!(m.word(d("D38")), 0x5678);
                    assert_eq!(m.word(d("D39")), 0x5678);
                    assert_eq!(m.word(d("D40")), 0);
                }
                assert_eq!(m.word(d("D37")), 111);
                assert_eq!(m.word(d("D42")), 222);
                assert_eq!(flags(&m), [true, false, true]);
            }
        }
    }
}

#[test]
fn bmov_copies_forward_live_items_and_mixed_width_strides() {
    for (body, expected) in [
        ("BMOV D0E0 D1F0 K3", [1, 1, 1, 1]),
        ("BMOV D1E0 D0F0 K3", [2, 3, 4, 4]),
    ] {
        let mut m = Memory::default();
        for (i, v) in [1, 2, 3, 4].into_iter().enumerate() {
            m.set_word(d(&format!("D{i}")), v);
        }
        run(body, &mut m);
        assert_eq!([0, 1, 2, 3].map(|n| m.word(d(&format!("D{n}")))), expected);
    }
    let mut m = Memory::default();
    write_bits(&mut m, d("M128"), 16, 0x4321);
    run("BMOV K1M128E0 K1M132F0 K3", &mut m);
    assert_eq!(read_bits(&m, d("M128"), 16), 0x1111);
    m.set_word(d("D0"), 0x1234);
    m.set_word(d("D1"), 0x5678);
    run("BMOV D0E0 K1M128F0 K2", &mut m);
    assert_eq!(read_bits(&m, d("M128"), 8), 0x84);
    run("BMOV K1M128E0 D20F0 K2", &mut m);
    assert_eq!([m.word(d("D20")), m.word(d("D21"))], [4, 8]);
}

#[test]
fn bmov_freezes_both_bases_and_count_before_payload_overwrites_their_aliases() {
    let mut m = Memory::default();
    m.set_word(d("D1028"), 3);
    m.set_word(d("D1029"), 0);
    for (i, v) in [(3, 1), (4, 2), (5, 3)] {
        m.set_word(d(&format!("D{i}")), v);
    }
    m.set_word(d("D1031"), 99);
    let a = action("BMOV D0E0 D1028F0 D1028");
    let captured = a.capture_devices(&m).unwrap();
    assert!(captured.contains(&d("D3")) && captured.contains(&d("D5")));
    a.qualify_native(&m).unwrap();
    assert_eq!(m.word(d("D1028")), 3);
    run("BMOV D0E0 D1028F0 D1028", &mut m);
    assert_eq!(
        [m.word(d("D1028")), m.word(d("D1029")), m.word(d("D1030"))],
        [1, 2, 3]
    );
    assert_eq!(m.word(d("D1031")), 99);
}

#[test]
fn invalid_dynamic_counts_keep_staged_scratch_but_do_not_visit_payload() {
    for n in [0, -1, 513] {
        for (body, expected) in [
            ("BMOV D0E0 D20F0 D2", 0x8686u16),
            ("BMOV K1M0E0 D20F0 D2", 0xaa86),
            ("BMOV D0E0 K1M80F0 D2", 0x8655),
            ("BMOV K1M0E0 K1M80F0 D2", 0xaa55),
        ] {
            let mut m = Memory::default();
            m.native_numeric_scratch = Some(0xaa55u16 as i16);
            m.set_word(d("D2"), n);
            m.set_word(d("D20"), 777);
            let a = action(body);
            assert!(a.capture_devices(&m).unwrap().is_empty());
            a.qualify_native(&m).unwrap();
            let seen = run(body, &mut m);
            assert_eq!(m.native_numeric_scratch.unwrap() as u16, expected);
            assert_eq!(m.word(d("D20")), 777);
            assert!(
                seen.contains(&d("D2")) && seen.contains(&d("D1028")) && seen.contains(&d("D1029"))
            );
            assert!(
                !seen.contains(&d("D0"))
                    && !seen.contains(&d("D20"))
                    && !seen.contains(&d("M0"))
                    && !seen.contains(&d("M80"))
            );
            assert!(!m.system_bit(1067) && !m.system_bit(1068));
        }
    }
    let mut m = Memory::default();
    m.native_numeric_scratch = Some(0xaa55u16 as i16);
    m.set_word(d("D1029"), -30000);
    let a = action("BMOV D0E0 D20F0 D2");
    assert!(a.capture_devices(&m).is_err());
    a.execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.native_numeric_scratch.unwrap() as u16, 0x8655);
    assert!(!m.execution_limitations.is_empty());
    assert!(!m.system_bit(1067));
}

#[test]
fn fill_source_snapshot_and_cached_destination_do_not_follow_rewritten_index_words() {
    for wide in [false, true] {
        let mut m = Memory::default();
        seed_flags(&mut m);
        if wide {
            m.set_double_word(d("D0"), 0x12345678);
        } else {
            m.set_word(d("D0"), 7);
        }
        m.set_word(d("D1032"), 999);
        let body = if wide {
            "DFMOV D0 D1028E0 K2"
        } else {
            "FMOV D0 D1028E0 K3"
        };
        run(body, &mut m);
        if wide {
            assert_eq!(m.double_word(d("D1028")), 0x12345678);
            assert_eq!(m.double_word(d("D1030")), 0x12345678);
        } else {
            assert_eq!(
                [m.word(d("D1028")), m.word(d("D1029")), m.word(d("D1030"))],
                [7, 7, 7]
            );
        }
        assert_eq!(m.word(d("D1032")), 999);
        assert_eq!(flags(&m), [true, false, true]);
    }
    let mut m = Memory::default();
    m.set_word(d("D0"), 9);
    run("FMOV D0 D0E1 K3", &mut m);
    assert_eq!(
        [m.word(d("D0")), m.word(d("D1")), m.word(d("D2"))],
        [9, 9, 9]
    );
}

#[test]
fn packed_fill_kinds_strides_and_guard_bits_are_independent_of_numeric_width() {
    for (wide, max) in [(false, 4), (true, 8)] {
        for digits in 1..=max {
            let mut m = Memory::default();
            m.native_numeric_scratch = Some(0xaa55u16 as i16);
            seed_flags(&mut m);
            let width = digits * 4;
            let after = Device {
                index: 128 + width * 2,
                ..d("M128")
            };
            m.set_internal(d("M127"), true);
            m.set_internal(after, true);
            let body = format!(
                "{}FMOV {} K{digits}M128E0 K2",
                if wide { "D" } else { "" },
                if wide { "H12345678" } else { "H5678" }
            );
            run(&body, &mut m);
            let mask = if width == 32 {
                u32::MAX
            } else {
                (1u32 << width) - 1
            };
            let expected = if wide { 0x12345678 } else { 0x5678 } & mask;
            assert_eq!(read_bits(&m, d("M128"), width), expected);
            assert_eq!(
                read_bits(
                    &m,
                    Device {
                        index: 128 + width,
                        ..d("M128")
                    },
                    width
                ),
                expected
            );
            assert!(m.bit(d("M127")) && m.bit(after));
            assert_eq!(flags(&m), [true, false, true]);
            assert_eq!(
                m.native_numeric_scratch.unwrap() as u16,
                if wide {
                    0xaa55
                } else {
                    0xaa00 | [0x82, 0x84, 0x86, 0x80][(digits - 1) as usize]
                }
            );
        }
    }
    let mut m = Memory::default();
    m.native_numeric_scratch = Some(0xaa55u16 as i16);
    run("DFMOV K1 D20E0 K2", &mut m);
    assert_eq!(m.native_numeric_scratch.unwrap() as u16, 0x8655);
}

#[test]
fn dfmov_complete_counter_cursor_reclassifies_c197_c198_c199_and_natural_counters() {
    for base in [197, 198, 199] {
        for indexed in [false, true] {
            let mut m = Memory::default();
            seed_flags(&mut m);
            m.set_double_word(d("C201"), 99);
            let count = if base == 197 { 3 } else { 2 };
            let body = format!(
                "DFMOV H12345678 C{base}{} K{count}",
                if indexed { "E0" } else { "" }
            );
            let a = action(&body);
            a.qualify_native(&m).unwrap();
            run(&body, &mut m);
            if base == 197 {
                assert_eq!(m.word(d("C197")) as u16, 0x5678);
                assert_eq!(m.word(d("C198")) as u16, 0x1234);
            }
            if base == 198 {
                assert_eq!(m.word(d("C198")) as u16, 0x5678);
                assert_eq!(m.word(d("C199")) as u16, 0x1234);
            } else {
                assert_eq!(m.word(d("C199")) as u16, 0x5678);
            }
            assert_eq!(m.double_word(d("C200")), 0x12345678);
            assert_eq!(m.double_word(d("C201")), 99);
            assert_eq!(flags(&m), [true, false, true]);
        }
    }
    let mut m = Memory::default();
    m.set_word(d("C199"), -1);
    m.set_double_word(d("C200"), 0x76543210);
    run("DFMOV C199 D20E0 K2", &mut m);
    assert_eq!(m.double_word(d("D20")), 65535);
    assert_eq!(m.double_word(d("D22")), 65535);
    run("DFMOV H87654321 C255E0 K1", &mut m);
    assert_eq!(m.double_word(d("C255")) as u32, 0x87654321);
}

#[test]
fn model_endpoints_word_fill_hold_and_incomplete_double_limitations_are_distinct() {
    for last in [4999, 9999] {
        let mut m = Memory::default();
        m.data_register_limit = Some(last);
        m.set_word(d(&format!("D{last}")), 123);
        let a = action(&format!("FMOV K7 D{last}E0 K2"));
        assert!(a.qualify_native(&m).is_err());
        a.execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(m.word(d(&format!("D{last}"))), 123);
        assert!(m.execution_limitations.is_empty());
        assert!(!m.system_bit(1067));
        let a = action(&format!("DFMOV K7 D{last}E0 K1"));
        assert!(a.qualify_native(&m).is_err());
        a.execute(&mut m, true, &mut BTreeSet::new());
        assert!(!m.execution_limitations.is_empty());
        assert_eq!(m.word(d(&format!("D{last}"))), 123);
        assert!(!m.system_bit(1067));
        m.execution_limitations.clear();
        m.set_word(d("D1028"), -1);
        run(&format!("DFMOV K7 D{last}E0 K1"), &mut m);
        assert_eq!(m.double_word(d(&format!("D{}", last - 1))), 7);
    }
}

#[test]
fn exchange_snapshots_both_values_but_resolves_first_store_after_second_changes_its_index() {
    let mut m = Memory::default();
    m.set_word(d("D10"), 1);
    m.set_word(d("D11"), 77);
    seed_flags(&mut m);
    let a = action("XCH D10E1 D1182");
    let before = m.word(d("D11"));
    a.qualify_native(&m).unwrap();
    assert_eq!(m.word(d("D11")), before);
    let seen = run("XCH D10E1 D1182", &mut m);
    assert_eq!(m.word(d("D1182")), 1);
    assert_eq!(m.word(d("D10")), 1);
    assert_eq!(m.word(d("D11")), 0);
    assert!(seen.contains(&d("D11")));
    assert_eq!(flags(&m), [true, false, true]);
    let mut m = Memory::default();
    m.set_double_word(d("D10"), 1);
    m.set_word(d("D12"), 77);
    m.set_word(d("D13"), 88);
    m.set_word(d("D1183"), 0x1234);
    run("DXCH D10E1 D1182", &mut m);
    assert_eq!(m.double_word(d("D1182")), 1);
    assert_eq!(m.word(d("D10")), 1);
    assert_eq!(m.word(d("D11")), 0);
    assert_eq!(m.word(d("D12")), 0x1234);
    assert_eq!(m.word(d("D13")), 88);
}

#[test]
fn exchange_qualification_replays_without_committing_and_offline_late_failure_is_not_atomic() {
    let mut m = Memory::default();
    m.set_word(d("D10"), -30000);
    m.set_word(d("D11"), 77);
    let a = action("XCH D10E1 D1182");
    assert!(a.prepare(&m, &mut BTreeSet::new()).is_ok());
    assert!(a.qualify_native(&m).is_err());
    assert_eq!(m.word(d("D1182")), 0);
    assert_eq!(m.word(d("D10")), -30000);
    a.execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.word(d("D1182")), -30000);
    assert_eq!(m.word(d("D11")), 77);
    assert!(!m.execution_limitations.is_empty());
    assert!(!m.system_bit(1067) && !m.system_bit(1068));
}

#[test]
fn m1303_identity_preserves_direct_index_provenance_and_ignores_kn_width() {
    for (body, expected) in [
        ("XCH K1M128E1 K4M128F1", 0x3412),
        ("XCH K4M128E1 K1M128F1", 0x1230),
    ] {
        let mut m = Memory::default();
        m.set_system(1303, true);
        write_bits(&mut m, d("M128"), 16, 0x1234);
        run(body, &mut m);
        assert_eq!(read_bits(&m, d("M128"), 16), expected);
    }
    for (body, expected) in [
        ("DXCH K1M128E1 K8M128F1", 0x56781234),
        ("DXCH K8M128E1 K1M128F1", 0x12345670),
    ] {
        let mut m = Memory::default();
        m.set_system(1303, true);
        write_bits(&mut m, d("M128"), 32, 0x12345678);
        run(body, &mut m);
        assert_eq!(read_bits(&m, d("M128"), 32), expected);
    }
    let mut m = Memory::default();
    m.set_system(1303, true);
    m.set_word(d("D1182"), 0x1234);
    m.set_word(d("D182"), 0x5678);
    run("XCH E1 D182E0", &mut m);
    assert_eq!(m.word(d("D182")) as u16, 0x7856);
    assert_eq!(m.word(d("D1182")), 0x1234);
    run("XCH E1 D1182E0", &mut m);
    assert_eq!(m.word(d("D1182")), 0x1234);
}

#[test]
fn dynamic_bmov_known_boundary_errors_remain_partial_and_at_header_plus_seven() {
    for (body, source, expected_dest) in [
        ("BMOV K1S1020E0 D20F0 D2", true, 77),
        ("BMOV D20E0 K1S1020F0 D2", false, 15),
    ] {
        let p = compile_il(&format!("LD X0\n{body}\nEND")).unwrap();
        assert_eq!(crate::es2_encoder::encode(&p).unwrap().action_pc[0], 1);
        let mut sim = Simulator::default();
        sim.memory.set_input(d("X0"), true);
        sim.memory.set_word(d("D2"), 2);
        sim.memory.set_word(d("D20"), if source { 77 } else { 15 });
        write_bits(&mut sim.memory, d("S1020"), 4, 0xa);
        complete(&mut sim, &p);
        assert_eq!(sim.memory.word(d("D1067")) as u16, 0xe1a);
        assert_eq!(sim.memory.word(d("D1068")), 8);
        if source {
            assert_eq!(sim.memory.word(d("D20")), expected_dest);
        } else {
            assert_eq!(read_bits(&sim.memory, d("S1020"), 4), expected_dest as u32);
        }
        assert!(sim.memory.execution_limitations.is_empty());
    }
}

#[test]
fn unknown_clock_read_is_a_limitation_not_bmov_e1a_and_failure_order_is_actual() {
    use crate::system_relays::ClockProfile;
    let mut m = Memory::default();
    m.system_relays.set_profile(ClockProfile::InstalledFixed);
    m.set_word(d("D20"), 77);
    m.native_numeric_scratch = Some(0xaa55u16 as i16);
    let a = action("BMOV K1M1011E0 D20F0 K1");
    let mut seen = BTreeSet::new();
    assert!(a.prepare(&m, &mut BTreeSet::new()).is_ok());
    a.execute(&mut m, true, &mut seen);
    assert!(!m.execution_limitations.is_empty());
    assert_eq!(m.word(d("D20")), 77);
    assert!(!m.system_bit(1067) && !m.system_bit(1068));
    assert!(seen.contains(&d("M1011")) && !seen.contains(&d("D20")));
    assert_eq!(m.native_numeric_scratch.unwrap() as u16, 0xaa86);
    let mut m = Memory::default();
    m.system_relays.set_profile(ClockProfile::InstalledFixed);
    m.native_numeric_scratch = Some(0xaa55u16 as i16);
    m.set_word(d("D1029"), -30000);
    let mut seen = BTreeSet::new();
    action("FMOV K1M1011E0 D20F0 K1").execute(&mut m, true, &mut seen);
    assert!(!seen.contains(&d("D1029")));
    assert_eq!(m.native_numeric_scratch.unwrap() as u16, 0xaa55);
    assert!(!m.system_bit(1067));
}

#[test]
fn direct_fill_gap_crossing_stays_supported_but_bmov_destination_native_bug_is_excluded() {
    let mut m = Memory::default();
    let a = action("DFMOV H12345678 K8M1532 K2");
    a.qualify_native(&m).unwrap();
    run("DFMOV H12345678 K8M1532 K2", &mut m);
    assert_eq!(read_bits(&m, d("M1532"), 32), 0x12345678);
    assert_eq!(read_bits(&m, d("M1564"), 32), 0x12345678);
    let a = action("BMOV D0E0 K2M1532 K2");
    assert!(a.capture_devices(&m).is_err());
    // This is qualification, not an assertion that the historical native bug
    // has been fixed or that all direct packed generic helpers share this path.
    for body in ["DFMOV K1 K8M1284E0 K1", "BMOV K1M1284E0 D20 K1"] {
        assert!(action(body).capture_devices(&m).is_err());
    }
}

#[test]
fn all_ten_forms_skip_off_and_held_p_addresses_then_rearm_at_the_same_site() {
    for body in [
        "BMOV D0E0 D20F0 K1",
        "FMOV D0E0 D20F0 K1",
        "DFMOV D0E0 D20F0 K1",
        "XCH D0E0 D20F0",
        "DXCH D0E0 D20F0",
    ] {
        for pulse in [false, true] {
            let (name, rest) = body.split_once(' ').unwrap();
            let source = format!(
                "LD X0\n{name}{} {rest}\nLDI X7\nMOV K321 D300\nEND",
                if pulse { "P" } else { "" }
            );
            let p = compile_il(&source).unwrap();
            let mut sim = Simulator::default();
            sim.memory.set_double_word(d("D0"), 7);
            sim.memory.set_double_word(d("D20"), 11);
            sim.memory.set_word(d("D1028"), -30000);
            complete(&mut sim, &p);
            assert!(!sim.observed_devices.contains(&d("D1028")));
            assert!(sim.memory.execution_limitations.is_empty());
            sim.memory.set_word(d("D1028"), 0);
            sim.memory.set_input(d("X0"), true);
            complete(&mut sim, &p);
            assert_eq!(sim.memory.word(d("D20")), 7);
            if pulse {
                sim.memory.set_word(d("D1028"), -30000);
                sim.observed_devices.clear();
                complete(&mut sim, &p);
                assert!(!sim.observed_devices.contains(&d("D1028")));
                assert_eq!(sim.memory.word(d("D20")), 7);
            }
            sim.memory.set_input(d("X0"), false);
            complete(&mut sim, &p);
            sim.memory.set_word(d("D1028"), 0);
            sim.memory.set_double_word(d("D0"), 9);
            sim.memory.set_input(d("X0"), true);
            complete(&mut sim, &p);
            assert_eq!(sim.memory.word(d("D20")), 9);
            assert_eq!(sim.memory.word(d("D300")), 321);
            assert!(sim.memory.execution_limitations.is_empty());
        }
    }
}

#[test]
fn monitor_and_preflight_use_captured_offsets_without_reading_label_payloads() {
    let mut m = Memory::default();
    m.set_word(d("D1028"), 2);
    m.set_word(d("D1029"), 3);
    m.set_word(d("D2"), 7);
    m.set_word(d("D23"), 99);
    let a = action("BMOV D0E0 D20F0 K2");
    assert_eq!(a.display_value(&m), "99");
    let mut static_set = BTreeSet::new();
    a.devices(&mut static_set);
    assert!(!static_set.contains(&d("D0")) && !static_set.contains(&d("D20")));
    let captured = a.capture_devices(&m).unwrap();
    assert_eq!(captured, BTreeSet::from([d("D2"), d("D3")]));
    let mut held = BTreeSet::new();
    a.observed_devices(&m, &mut held);
    assert!(held.contains(&d("D23")) && held.contains(&d("D24")));
    assert!(!held.contains(&d("D0")) && !held.contains(&d("D20")));
    a.prepare(&m, &mut BTreeSet::new()).unwrap();
    a.qualify_native(&m).unwrap();
    assert_eq!(m.word(d("D23")), 99);
    assert_eq!(m.native_numeric_scratch, None);
}
