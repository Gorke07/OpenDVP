//! Independent admission tests for SEGD's two gray address roles.
//! The ordinary Kn/E/F matrix predates this draft. No native run is claimed.
use crate::calendar_operand::CalendarIndices;
use crate::indexed_segment::IndexedSegmentAction;
use crate::plc::{Area, Device, Memory, Program, ScanStatus, Simulator, compile_il};
use crate::system_relays::{ClockProfile, ClockState};
use crate::text_encoding::TextAction;
use std::collections::BTreeSet;

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn args(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(str::to_owned).collect()
}
fn action(raw: &str) -> IndexedSegmentAction {
    let words = args(raw);
    IndexedSegmentAction::parse(&words[0], &words[1..]).unwrap()
}
fn execute(a: &IndexedSegmentAction, m: &mut Memory) -> BTreeSet<Device> {
    let mut seen = BTreeSet::new();
    a.execute(m, true, &mut seen);
    assert!(
        m.execution_limitations.is_empty(),
        "{:?}",
        m.execution_limitations
    );
    seen
}
fn flags(m: &Memory) -> [bool; 3] {
    [1020, 1021, 1022].map(|n| m.system_bit(n))
}
fn seed_flags(m: &mut Memory) {
    for (n, on) in [(1020, true), (1021, false), (1022, true)] {
        m.set_system(n, on);
    }
}
fn group(m: &Memory, base: Device, bits: u16) -> u16 {
    (0..bits).fold(0, |value, bit| {
        value
            | (u16::from(m.bit(Device {
                index: base.index + bit,
                ..base
            })) << bit)
    })
}
fn put_group(m: &mut Memory, base: Device, bits: u16, value: u16) {
    for bit in 0..bits {
        let at = Device {
            index: base.index + bit,
            ..base
        };
        let on = value & (1u16 << bit) != 0;
        if at.area == Area::X {
            m.set_input(at, on);
        } else {
            m.set_internal(at, on);
        }
    }
}
fn complete(sim: &mut Simulator, p: &Program) {
    sim.scan(p, 0);
    assert_eq!(sim.last_scan, ScanStatus::Complete);
}

#[test]
fn original_five_word_headers_and_literal_suffix_descriptors() {
    for (name, header) in [("SEGD", 0xa200), ("SEGDP", 0xa210)] {
        let a = action(&format!("{name} D5E1 K2M128F7"));
        assert_eq!(
            a.native_words().unwrap(),
            [header, 0x0ae6, 0x8086, 0x80f4, 0xe884]
        );
        assert_eq!(a.nominal_words(), 5);
        assert_eq!(a.source_count(), 1);
        assert_eq!(a.label(), format!("{name} D5E1 K2M128F7"));
        assert_eq!(a.dest(), d("M128"));
    }
    assert_eq!(
        action("SEGD K4X0E1 D20F7").native_words().unwrap(),
        [0xa200, 0x00e4, 0x2488, 0x28f6, 0x8089]
    );
    assert_eq!(
        action("SEGD E1 F7").native_words().unwrap(),
        [0xa200, 0x6c86, 0x0180, 0x8686, 0x0180]
    );
    assert_eq!(
        action("SEGD T255F2 C199E7").native_words().unwrap(),
        [0xa200, 0xfef6, 0x4182, 0x8ee6, 0xe184]
    );
}

#[test]
fn documented_read_write_matrix_and_white_roles_are_not_broadened() {
    for source in [
        "K-1", "HABCD", "H1F7", "E0", "F7", "T255", "C199", "D9999", "T255E0", "C199F7", "D9999E7",
    ] {
        assert!(
            IndexedSegmentAction::parse("SEGD", &args(&format!("{source} D20E0"))).is_ok(),
            "{source}"
        );
    }
    for digits in 1..=4 {
        for area in ["X0", "Y200", "M128", "S128"] {
            assert!(
                IndexedSegmentAction::parse("SEGD", &args(&format!("K{digits}{area}F7 D20")))
                    .is_ok()
            );
        }
        for area in ["Y200", "M128", "S128"] {
            assert!(
                IndexedSegmentAction::parse("SEGDP", &args(&format!("K7 K{digits}{area}E0")))
                    .is_ok()
            );
        }
    }
    for target in [
        "E0", "F7", "T255", "C199", "D9999", "T255E0", "C199F7", "D9999E7",
    ] {
        assert!(IndexedSegmentAction::parse("SEGD", &args(&format!("D10E1 {target}"))).is_ok());
    }
    for raw in [
        "K1E0 D20",
        "E1F1 D20",
        "F0E0 D20",
        "X0E0 D20",
        "Y0 D20",
        "M80E0 D20",
        "K5M128E0 D20",
        "C200E0 D20",
        "C200 D20",
        "D10 K1X0E0",
        "D10 K1",
        "D10 H7",
        "D10 M80E0",
        "D10 C200E0",
        "D10 E1F1",
    ] {
        assert!(
            IndexedSegmentAction::parse("SEGD", &args(raw)).is_err(),
            "{raw}"
        );
    }
    assert!(!IndexedSegmentAction::supports("DSEGD"));
    assert!(IndexedSegmentAction::parse("DSEGD", &args("D10 D20")).is_err());
    assert!(IndexedSegmentAction::parse("SEGD", &args("D10")).is_err());
    assert!(!IndexedSegmentAction::has_modifiers(&args("HFE E7")));
    assert!(IndexedSegmentAction::has_modifiers(&args("K1M128F7 E7")));
}

#[test]
fn all_sixteen_modifier_words_are_signed_and_ignore_the_neighbor_word() {
    for high_bank in [false, true] {
        for bank in 0..8 {
            let suffix = format!("{}{bank}", if high_bank { 'F' } else { 'E' });
            let alias = CalendarIndices::alias_device(bank, high_bank).unwrap();
            let neighbor = Device {
                index: alias.index + 1,
                ..alias
            };
            for offset in [-2, 3] {
                let mut m = Memory::default();
                m.set_word(alias, offset);
                m.set_word(neighbor, i16::MIN);
                let source = Device {
                    index: (10 + offset) as u16,
                    ..d("D10")
                };
                let target = Device {
                    index: (20 + offset) as u16,
                    ..d("D20")
                };
                m.set_word(source, 7);
                m.set_word(target, -1);
                let seen = execute(&action(&format!("SEGD D10{suffix} D20{suffix}")), &mut m);
                assert_eq!(m.word(target), 0x27, "{suffix}/{offset}");
                assert_eq!(m.word(neighbor), i16::MIN);
                assert!(seen.contains(&alias) && seen.contains(&source) && seen.contains(&target));
                assert!(!seen.contains(&neighbor));
            }
        }
    }
}

#[test]
fn independent_table_low_nibble_and_whole_word_zero_extension() {
    // Literal oracle from the manual/native table; never call segment_result here.
    let expected = [
        0x3f, 0x06, 0x5b, 0x4f, 0x66, 0x6d, 0x7d, 0x27, 0x7f, 0x6f, 0x77, 0x7c, 0x39, 0x5e, 0x79,
        0x71,
    ];
    for (nibble, value) in expected.into_iter().enumerate() {
        let mut m = Memory::default();
        m.set_word(d("D10"), (0xabc0u16 | nibble as u16) as i16);
        m.set_word(d("D20"), -1);
        m.set_word(d("D19"), 123);
        m.set_word(d("D21"), 456);
        seed_flags(&mut m);
        execute(&action("SEGD D10E0 D20F0"), &mut m);
        assert_eq!(m.word(d("D20")), value);
        assert_eq!((m.word(d("D19")), m.word(d("D21"))), (123, 456));
        assert_eq!(flags(&m), [true, false, true]);
    }
}

#[test]
fn all_group_source_areas_read_exact_width_with_bit_unit_offsets() {
    for base in ["X100", "Y200", "M128", "S128"] {
        for digits in 1..=4 {
            let mut m = Memory::default();
            m.set_word(d("D1182"), 1);
            let original = d(base);
            let effective = Device {
                index: original.index + 1,
                ..original
            };
            put_group(&mut m, effective, digits * 4, 0xfffau16);
            let seen = execute(&action(&format!("SEGD K{digits}{base}E1 D20")), &mut m);
            assert_eq!(m.word(d("D20")), 0x77);
            assert!(!seen.contains(&original));
            for bit in 0..digits * 4 {
                assert!(seen.contains(&Device {
                    index: effective.index + bit,
                    ..effective
                }));
            }
            assert!(!seen.contains(&Device {
                index: effective.index + digits * 4,
                ..effective
            }));
        }
    }
}

#[test]
fn every_short_group_target_truncates_and_retains_both_guards() {
    for base in ["Y200", "M128", "S128"] {
        for digits in 1..=4 {
            let mut m = Memory::default();
            m.set_word(d("D1183"), 1);
            let original = d(base);
            let effective = Device {
                index: original.index + 1,
                ..original
            };
            let after = Device {
                index: effective.index + digits * 4,
                ..effective
            };
            m.set_internal(original, true);
            m.set_internal(after, true);
            put_group(&mut m, effective, digits * 4, u16::MAX);
            let seen = execute(&action(&format!("SEGD K0 K{digits}{base}F1")), &mut m);
            assert_eq!(
                group(&m, effective, digits * 4),
                if digits == 1 { 0xf } else { 0x3f }
            );
            assert!(m.bit(original) && m.bit(after));
            assert!(!seen.contains(&original) && !seen.contains(&after));
        }
    }
}

#[test]
fn overlapping_groups_use_one_complete_source_snapshot() {
    let mut m = Memory::default();
    m.set_word(d("D1182"), 1);
    put_group(&mut m, d("M128"), 8, 0xba);
    m.set_internal(d("M127"), true);
    m.set_internal(d("M133"), true);
    execute(&action("SEGD K2M128 K1M128E1"), &mut m);
    // A -> segment77 -> low nibble7 at M129..132. A live nibble-at-a-time
    // reader would consume its own target mutation instead of the snapshot.
    assert_eq!(group(&m, d("M129"), 4), 7);
    assert!(!m.bit(d("M128")));
    assert!(m.bit(d("M127")) && m.bit(d("M133")));
}

#[test]
fn prepared_value_and_effective_target_remain_fixed_until_terminal_store() {
    let mut m = Memory::default();
    m.set_word(d("D10"), 7);
    m.set_word(d("D1182"), 0);
    m.set_word(d("D1184"), 0);
    m.set_word(d("D1183"), 321);
    m.set_word(d("D1187"), 654);
    let prepared = action("SEGD D10E2 D1182E1")
        .prepare(&m, &mut BTreeSet::new())
        .unwrap();
    m.set_word(d("D10"), 0);
    m.set_word(d("D1182"), 5);
    prepared.write(&mut m);
    assert_eq!(m.word(d("D1182")), 0x27);
    assert_eq!(m.word(d("D1183")), 321);
    assert_eq!(m.word(d("D1187")), 654);
}

#[test]
fn direct_e_f_scalar_and_destination_aliases_remain_canonical_word_only() {
    for bank in 0..8 {
        for high in [false, true] {
            let raw = format!("{}{bank}", if high { 'F' } else { 'E' });
            let alias = CalendarIndices::alias_device(bank, high).unwrap();
            let next = Device {
                index: alias.index + 1,
                ..alias
            };
            let mut m = Memory::default();
            m.set_word(alias, 6);
            m.set_word(next, 444);
            execute(&action(&format!("SEGD {raw} D20")), &mut m);
            assert_eq!(m.word(d("D20")), 0x7d);
            execute(&action(&format!("SEGD K9 {raw}")), &mut m);
            assert_eq!(m.word(alias), 0x6f);
            assert_eq!(m.word(next), 444);
        }
    }
}

#[test]
fn m1019_target_changes_only_the_declared_four_flag_bits() {
    for indexed in [false, true] {
        let mut m = Memory::default();
        m.set_word(d("D1182"), 1);
        m.set_internal(d("M1018"), true);
        m.set_internal(d("M1019"), false);
        m.set_system(1020, false);
        m.set_system(1021, false);
        m.set_system(1022, true);
        let target = if indexed { "K1M1018E1" } else { "K1M1019" };
        execute(&action(&format!("SEGD K7 {target}")), &mut m);
        assert_eq!(group(&m, d("M1019"), 4), 7);
        assert_eq!(flags(&m), [true, true, false]);
        assert!(m.bit(d("M1018")));
    }
}

#[test]
fn source_flag_group_is_snapshotted_without_implicit_flag_changes() {
    let mut m = Memory::default();
    put_group(&mut m, d("M1019"), 4, 0xa);
    execute(&action("SEGD K1M1019E0 D20"), &mut m);
    assert_eq!(m.word(d("D20")), 0x77);
    assert_eq!(group(&m, d("M1019"), 4), 0xa);
}

#[test]
fn word_endpoints_are_complete_and_negative_offsets_rescue_lexical_ends() {
    for last in [4999, 9999] {
        for offset in [0, -1] {
            let mut m = Memory::default();
            m.data_register_limit = Some(last);
            m.set_word(d("D1182"), offset);
            m.set_word(d("D1195"), offset);
            let actual = Device {
                area: Area::D,
                index: (last as i32 + i32::from(offset)) as u16,
            };
            m.set_word(actual, 3);
            execute(&action(&format!("SEGD D{last}E1 D20")), &mut m);
            assert_eq!(m.word(d("D20")), 0x4f);
            execute(&action(&format!("SEGD K4 D{last}F7")), &mut m);
            assert_eq!(m.word(actual), 0x66);
        }
    }
    for target in ["T255E0", "C199E0"] {
        let mut m = Memory::default();
        m.set_double_word(d("C200"), 0x1234_5678);
        execute(&action(&format!("SEGD K8 {target}")), &mut m);
        assert_eq!(m.word(d(&target[..target.len() - 2])), 0x7f);
        assert_eq!(m.double_word(d("C200")), 0x1234_5678);
    }
}

#[test]
fn bad_effective_source_stops_before_target_modifier_and_plc_error_is_not_invented() {
    for source in ["C199E1", "D4999E1", "T255E1"] {
        let mut m = Memory::default();
        m.data_register_limit = Some(4999);
        m.set_word(d("D1182"), 1);
        m.set_word(d("D1195"), -1);
        m.set_word(d("D20"), 123);
        seed_flags(&mut m);
        let a = action(&format!("SEGD {source} D0F7"));
        let mut seen = BTreeSet::new();
        a.execute(&mut m, true, &mut seen);
        assert!(!m.execution_limitations.is_empty());
        assert!(seen.contains(&d("D1182")));
        assert!(!seen.contains(&d("D1195")));
        assert_eq!(m.word(d("D20")), 123);
        assert_eq!(flags(&m), [true, false, true]);
        assert!(!m.system_bit(1067) && !m.system_bit(1068));
        assert_eq!((m.word(d("D1067")), m.word(d("D1068"))), (0, 0));
    }
}

#[test]
fn bad_effective_target_is_checked_after_the_complete_source_read() {
    let mut m = Memory::default();
    m.set_word(d("D1182"), 0);
    m.set_word(d("D1195"), 1);
    m.set_word(d("D10"), 7);
    m.set_word(d("C199"), 55);
    let mut seen = BTreeSet::new();
    action("SEGD D10E1 C199F7").execute(&mut m, true, &mut seen);
    assert_eq!(seen, BTreeSet::from([d("D1182"), d("D10"), d("D1195")]));
    assert_eq!(m.word(d("C199")), 55);
    assert!(!m.execution_limitations.is_empty());
    assert!(!m.system_bit(1067));
}

#[test]
fn generic_remap_and_bank_crossing_remain_explicit_limitations() {
    let mut m = Memory::default();
    m.set_word(d("D1182"), 0);
    assert!(action("SEGD K1M1284E1 D20").qualify_native(&m).is_err());
    assert!(action("SEGD K7 K1M1284E1").qualify_native(&m).is_err());
    m.set_word(d("D1182"), 16);
    assert!(action("SEGD K1M1520E1 D20").qualify_native(&m).is_err());
    assert!(!m.system_bit(1067));
}

#[test]
fn off_visit_skips_all_invalid_addresses_and_preserves_sticky_state() {
    let mut m = Memory::default();
    m.set_word(d("D1182"), -1);
    m.set_word(d("D1195"), -1);
    m.set_system(1067, true);
    m.set_word(d("D1067"), 0xe19u16 as i16);
    m.set_word(d("D1068"), 91);
    seed_flags(&mut m);
    let mut seen = BTreeSet::new();
    action("SEGDP D0E1 D0F7").execute(&mut m, false, &mut seen);
    assert!(seen.is_empty() && m.execution_limitations.is_empty());
    assert_eq!(flags(&m), [true, false, true]);
    assert_eq!(
        (m.word(d("D1067")), m.word(d("D1068"))),
        (0xe19u16 as i16, 91)
    );
}

#[test]
fn successful_segment_holds_sticky_errors_scratch_and_ignores_text_byte_mode() {
    for byte_mode in [false, true] {
        let mut m = Memory::default();
        m.set_system(1161, byte_mode);
        m.set_system(1067, true);
        m.set_system(1068, true);
        m.set_word(d("D1067"), 0xe19u16 as i16);
        m.set_word(d("D1068"), 91);
        m.native_numeric_scratch = Some(-123);
        seed_flags(&mut m);
        let seen = execute(&action("SEGD HFFF7 D20E1"), &mut m);
        assert_eq!(m.word(d("D20")), 0x27);
        assert_eq!(flags(&m), [true, false, true]);
        assert!(m.system_bit(1067) && m.system_bit(1068));
        assert_eq!(
            (m.word(d("D1067")), m.word(d("D1068"))),
            (0xe19u16 as i16, 91)
        );
        assert_eq!(m.native_numeric_scratch, Some(-123));
        assert!(!seen.contains(&Memory::system_device(1161)));
    }
}

#[test]
fn shared_unknown_clock_guard_precedes_destination_and_seeded_zero_is_known() {
    let a = action("SEGD K1M1011E0 D20F7");
    let mut m = Memory::default();
    m.system_relays.set_profile(ClockProfile::InstalledFixed);
    m.set_word(d("D1195"), 0);
    m.set_word(d("D20"), 123);
    let mut seen = BTreeSet::new();
    assert!(a.prepare(&m, &mut seen).is_err());
    for n in 1011..=1014 {
        assert!(seen.contains(&Memory::system_device(n)));
    }
    assert!(!seen.contains(&d("D1195")));
    assert_eq!(m.word(d("D20")), 123);
    ClockState::seed_installed_levels(&mut m, [false; 4]).unwrap();
    execute(&a, &mut m);
    assert_eq!(m.word(d("D20")), 0x3f);
    let mut documented = Memory::default();
    execute(&a, &mut documented);
    assert_eq!(documented.word(d("D20")), 0x3f);
}

#[test]
fn native_qualification_uses_actual_staged_inputs_without_local_output_feedback() {
    let a = action("SEGD D10E1 D20F7");
    let mut local = Memory::default();
    local.set_word(d("D1182"), 0);
    local.set_word(d("D1195"), 0);
    local.set_word(d("D10"), 1);
    local.set_word(d("D20"), 777);
    let mut requested = Vec::new();
    let seen = a
        .qualify_native_with(&local, |clone, devices| {
            if devices.is_empty() {
                return Ok(());
            }
            requested.push(devices.clone());
            for device in devices {
                let value = match device.to_string().as_str() {
                    "D1182" => 1,
                    "D11" => 7,
                    "D1195" => 2,
                    other => panic!("unexpected native input capture: {other}"),
                };
                clone.set_word(*device, value);
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(
        requested,
        vec![
            BTreeSet::from([d("D1182")]),
            BTreeSet::from([d("D11")]),
            BTreeSet::from([d("D1195")])
        ]
    );
    assert!(seen.contains(&d("D22")) && !seen.contains(&d("D20")));
    assert_eq!(local.word(d("D20")), 777);
    assert_eq!(local.word(d("D1182")), 0);
    assert_eq!(local.word(d("D1195")), 0);
    assert_eq!(local.stored_word(d("D11")), None);
}

#[test]
fn native_bad_source_and_capture_failures_never_touch_target_metadata() {
    for fail_fetch in [false, true] {
        let a = action("SEGD C199E1 D20F7");
        let m = Memory::default();
        let mut calls = Vec::new();
        let result = a.qualify_native_with(&m, |clone, devices| {
            if devices.is_empty() {
                return Ok(());
            }
            calls.extend(devices.iter().copied());
            assert!(!devices.contains(&d("D1195")));
            if fail_fetch {
                return Err("declared native input unavailable".into());
            }
            clone.set_word(d("D1182"), 1);
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(calls, [d("D1182")]);
    }
}

#[test]
fn native_alias_capture_is_deduplicated_and_target_old_storage_is_not_read() {
    let a = action("SEGD E1 D20E1");
    let m = Memory::default();
    let mut counts = 0;
    let seen = a
        .qualify_native_with(&m, |clone, devices| {
            for device in devices {
                assert_eq!(*device, d("D1182"));
                counts += 1;
                clone.set_word(*device, 7);
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(counts, 1);
    assert_eq!(seen, BTreeSet::from([d("D1182"), d("D27")]));
}

#[test]
fn monitor_candidates_are_separate_from_actual_visited_storage() {
    let a = action("SEGD D10E1 K1M128F7");
    let mut m = Memory::default();
    m.set_word(d("D1182"), 2);
    m.set_word(d("D1195"), 3);
    m.set_word(d("D12"), 7);
    let mut static_devices = BTreeSet::new();
    a.devices(&mut static_devices);
    assert!(!static_devices.contains(&d("D10")) && !static_devices.contains(&d("M128")));
    assert!(static_devices.contains(&d("D1182")) && static_devices.contains(&d("D1195")));
    assert_eq!(
        a.monitor_input_devices(&m).unwrap(),
        BTreeSet::from([d("D1182"), d("D1195")])
    );
    assert!(a.control_devices().is_empty());
    let candidates = a.capture_devices(&m).unwrap();
    let seen = execute(&a, &mut m);
    assert_eq!(candidates, seen);
    assert!(seen.contains(&d("D12")) && seen.contains(&d("M131")) && seen.contains(&d("M134")));
    assert!(!seen.contains(&d("M130")) && !seen.contains(&d("M135")));
    assert_eq!(a.display_value(&m), "7");
}

#[test]
fn typed_legacy_adapter_retains_every_existing_valid_constructor_shape() {
    for raw in [
        "K10 D20",
        "HABCD K1M128",
        "K2X0 E1",
        "F7 T255",
        "C199 K4S128",
    ] {
        let old = TextAction::parse("SEGD", &args(raw)).unwrap();
        let new = IndexedSegmentAction::from_legacy(&old).unwrap();
        let mut old_memory = Memory::default();
        old_memory.set_word(d("C199"), 7);
        old_memory.set_word(d("D1195"), 9);
        put_group(&mut old_memory, d("X0"), 8, 0xa);
        let mut new_memory = old_memory.clone();
        old.execute(&mut old_memory);
        execute(&new, &mut new_memory);
        let mut devices = BTreeSet::new();
        new.devices(&mut devices);
        for device in devices {
            assert_eq!(
                old_memory.word(device),
                new_memory.word(device),
                "{raw}/{device}"
            );
            assert_eq!(
                old_memory.bit(device),
                new_memory.bit(device),
                "{raw}/{device}"
            );
        }
        assert_eq!(new.nominal_words(), 5);
    }
}

#[test]
fn public_vm_original_lengths_following_markers_and_normal_pulse_lifecycle() {
    for (name, header) in [("SEGD", 0xa200), ("SEGDP", 0xa210)] {
        let p = compile_il(&format!("LD X0\n{name} D10E0 D20F0\nINC D300\nOUT Y0\nEND")).unwrap();
        assert!(p.requires_vm());
        let encoded = crate::es2_encoder::encode(&p).unwrap();
        let pc = usize::from(encoded.action_pc[0]);
        assert_eq!(encoded.words[pc], header);
        assert_eq!(usize::from(encoded.action_pc[1]), pc + 5);
        let mut sim = Simulator::default();
        sim.memory.set_word(d("D10"), 1);
        sim.memory.set_word(d("D20"), -1);
        for (on, input, normal, pulse, visits) in [
            (false, 1, -1, -1, 0),
            (true, 1, 0x06, 0x06, 1),
            (true, 7, 0x27, 0x06, 2),
            (false, 7, 0x27, 0x06, 2),
            (true, 7, 0x27, 0x27, 3),
        ] {
            sim.memory.set_input(d("X0"), on);
            sim.memory.set_word(d("D10"), input);
            complete(&mut sim, &p);
            assert_eq!(
                sim.memory.word(d("D20")),
                if name == "SEGD" { normal } else { pulse }
            );
            assert_eq!(sim.memory.word(d("D300")), visits);
            assert_eq!(sim.memory.bit(d("Y0")), on);
        }
    }
}

#[test]
fn public_vm_reads_same_scan_index_updates_and_keeps_previous_target() {
    let p =
        compile_il("LD X0\nMOV K1 E1\nSEGD D10E1 D20E1\nMOV K2 E1\nSEGD D10E1 D20E1\nEND").unwrap();
    assert!(p.requires_vm());
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim.memory.set_word(d("D11"), 2);
    sim.memory.set_word(d("D12"), 3);
    sim.memory.set_word(d("D20"), 99);
    complete(&mut sim, &p);
    assert_eq!(sim.memory.word(d("D20")), 99);
    assert_eq!(sim.memory.word(d("D21")), 0x5b);
    assert_eq!(sim.memory.word(d("D22")), 0x4f);
}

#[test]
fn public_vm_master_off_skips_sources_then_master_restore_allows_segment() {
    let p = compile_il("LD X1\nMC N0\nLDI X7\nSEGD D0E1 D20F7\nMCR N0\nLD X0\nSEGD K7 D30E0\nEND")
        .unwrap();
    assert!(p.requires_vm());
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim.memory.set_input(d("X1"), false);
    sim.memory.set_word(d("D1182"), -1);
    sim.memory.set_word(d("D1195"), -1);
    sim.memory.set_word(d("D20"), 99);
    complete(&mut sim, &p);
    assert!(sim.memory.execution_limitations.is_empty());
    assert_eq!(sim.memory.word(d("D20")), 99);
    assert_eq!(sim.memory.word(d("D30")), 0x27);
    assert!(!sim.observed_devices.contains(&d("D1182")));
    assert!(!sim.observed_devices.contains(&d("D1195")));
}

#[test]
fn isolated_vm_gate_uses_master_even_when_output_context_is_disabled() {
    for name in ["SEGD", "SEGDP"] {
        for (master, output, active) in [(1, false, true), (0, true, false)] {
            let p = compile_il(&format!("LD X0\n{name} D10E1 D20F7\nEND")).unwrap();
            let mut sim = Simulator::default();
            sim.memory.set_input(d("X0"), true);
            sim.memory.set_word(d("D1182"), 1);
            sim.memory.set_word(d("D1195"), 2);
            sim.memory.set_word(d("D11"), 7);
            sim.memory.set_word(d("D22"), 99);
            sim.begin_plan_scan(&p, 0, true).unwrap();
            sim.execute_plan_step(&p).unwrap();
            assert_eq!(sim.current_plan_pc(), Some(1));
            assert_ne!(sim.control.logic_word & 1, 0);
            sim.control.master_gate = master;
            sim.control.out_enabled = output;
            sim.execute_plan_step(&p).unwrap();
            assert_eq!(
                sim.memory.word(d("D22")),
                if active { 0x27 } else { 99 },
                "{name}"
            );
            assert!(sim.memory.execution_limitations.is_empty());
            for raw in ["D1182", "D1195", "D11", "D22"] {
                assert_eq!(
                    sim.observed_devices.contains(&d(raw)),
                    active,
                    "{name}/{raw}"
                );
            }
        }
    }
}
