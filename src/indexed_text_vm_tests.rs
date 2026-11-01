//! Independent literal oracles; requires parent module/parser/VM admission.
use crate::calendar_operand::CalendarIndices;
use crate::indexed_text::{IndexedTextAction, TextOutcome as Outcome};
use crate::plc::{Area, Device, Memory, Program, ScanStatus, Simulator, compile_il};
use crate::system_relays::{ClockProfile, ClockState};
use crate::text_encoding::TextAction;
use std::collections::BTreeSet;

#[test]
fn masked_hex_padding_never_becomes_an_out_of_area_payload_read() {
    for (source, offset, low, seed, expected, error) in [
        ("K1S1020E1", 0, false, 0x000f, 0x5678, true),
        ("K2S1016E1", 1, false, 0x0041, 10, false),
        ("K2S1016E1", 0, true, 0x0041, 10, false),
    ] {
        let op = a(&format!("HEX {source} D20 K1"));
        let mut m = Memory::default();
        m.set_word(d("D1182"), offset);
        m.set_word(d("D20"), 0x5678);
        m.set_system(1161, low);
        let first = if source.starts_with("K1") {
            1020
        } else {
            1016 + offset as u16
        };
        for bit in 0..=1023 - first {
            m.set_internal(
                Device {
                    area: Area::S,
                    index: first + bit,
                },
                seed & (1 << bit) != 0,
            );
        }
        let mut seen = BTreeSet::new();
        op.execute(&mut m, true, &mut seen);
        assert!(
            m.execution_limitations.is_empty(),
            "{:?}",
            m.execution_limitations
        );
        assert_eq!(m.word(d("D20")) as u16, expected);
        assert_eq!(m.system_bit(1067), error);
        assert!(seen.iter().all(|d| d.area != Area::S || d.index <= 1023));
    }
    // The following character needs real out-of-area bits. Qualification
    // must still reject it, and disposable replay must preserve local output.
    let op = a("HEX K4S1008E1 D20 K2");
    let mut m = Memory::default();
    m.set_word(d("D1182"), 9);
    m.set_word(d("D20"), 0x5678);
    for bit in 0..16 {
        m.set_internal(
            Device {
                area: Area::S,
                index: 1008 + bit,
            },
            0x8200u16 & (1 << bit) != 0,
        );
    }
    let visit = op.qualify_native(&m);
    assert!(matches!(visit.outcome, Outcome::Limitation(_)));
    assert_eq!(m.word(d("D20")), 0x5678);
    assert!(!m.system_bit(1067));
    assert!(
        visit
            .observed
            .iter()
            .all(|d| d.area != Area::S || d.index <= 1023)
    );
}

#[test]
fn typed_legacy_text_programs_use_the_source_vm_for_normal_and_pulse_forms() {
    for name in ["ASCI", "HEX", "CCD"] {
        for pulse in [false, true] {
            let mut p = compile_il(&format!("LD X0\n{name} D0 D20 K2\nEND")).unwrap();
            let raw = ["D0", "D20", "K2"].map(str::to_owned);
            let legacy = crate::plc::Action::Text(TextAction::parse(name, &raw).unwrap());
            p.networks[0].actions[0] = if pulse {
                crate::plc::Action::Pulse(Box::new(legacy))
            } else {
                legacy
            };
            assert!(p.requires_vm());
            let mut sim = Simulator::default();
            sim.memory.set_input(d("X0"), true);
            sim.memory
                .set_word(d("D0"), if name == "HEX" { 0x3130 } else { 0x0201 });
            complete(&mut sim, &p);
            assert!(sim.memory.native_numeric_scratch.is_some());
        }
    }
}

fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn args(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}
fn a(s: &str) -> IndexedTextAction {
    let words = args(s);
    IndexedTextAction::parse(&words[0], &words[1..]).unwrap()
}
fn run(s: &str, m: &mut Memory) -> (Outcome, BTreeSet<Device>) {
    let mut seen = BTreeSet::new();
    let result = a(s).execute(m, true, &mut seen);
    (result, seen)
}
fn put(m: &mut Memory, first: u16, values: &[u16]) {
    for (i, value) in values.iter().enumerate() {
        m.set_word(
            Device {
                area: Area::D,
                index: first + i as u16,
            },
            *value as i16,
        );
    }
}
fn words(m: &Memory, first: u16, n: u16) -> Vec<u16> {
    (0..n)
        .map(|i| {
            m.word(Device {
                area: Area::D,
                index: first + i,
            }) as u16
        })
        .collect()
}
fn group(m: &mut Memory, base: Device, value: u16) {
    for i in 0..16 {
        let target = Device {
            index: base.index + i,
            ..base
        };
        if target.area == Area::X {
            m.set_input(target, value & (1 << i) != 0);
        } else {
            m.set_internal(target, value & (1 << i) != 0);
        }
    }
}
fn bits(m: &Memory, base: Device) -> u16 {
    (0..16).fold(0, |v, i| {
        v | (u16::from(m.bit(Device {
            index: base.index + i,
            ..base
        })) << i)
    })
}
fn flags(m: &Memory) -> [bool; 3] {
    [1020, 1021, 1022].map(|v| m.system_bit(v))
}
fn seed_flags(m: &mut Memory) {
    m.set_system(1020, true);
    m.set_system(1021, false);
    m.set_system(1022, true);
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
fn six_headers_aliases_and_original_seven_word_descriptors() {
    for (name, canonical, header) in [
        ("ASCI", "ASCI", 0xb400),
        ("ASCII", "ASCI", 0xb400),
        ("ASCIP", "ASCIP", 0xb410),
        ("ASCIIP", "ASCIP", 0xb410),
        ("HEX", "HEX", 0xb600),
        ("HEXP", "HEXP", 0xb610),
        ("CCD", "CCD", 0xb800),
        ("CCDP", "CCDP", 0xb810),
    ] {
        let op = a(&format!("{name} D5E1 D20F7 K2"));
        assert_eq!(
            op.native_words().unwrap(),
            [header, 0x0ae6, 0x8086, 0x28f6, 0x8089, 0x0280, 0x0080]
        );
        assert_eq!(op.label(), format!("{canonical} D5E1 D20F7 K2"));
        assert_eq!(op.nominal_words(), 7);
    }
    assert!(!IndexedTextAction::supports("ASC"));
    assert!(!IndexedTextAction::supports("DCCD"));
}

#[test]
fn roles_cover_all_documented_data_categories_without_white_count_suffixes() {
    for name in ["ASCI", "ASCIP", "HEX", "HEXP", "CCD", "CCDP"] {
        for source in [
            "K1X0E0", "K2Y200F1", "K3M128E7", "K4S128F7", "T255E1", "C199F1", "D9999E0",
        ] {
            for target in ["K1M256E0", "K2S256F7", "T255F1", "C199E1", "D9999F0"] {
                assert!(
                    IndexedTextAction::parse(name, &args(&format!("{source} {target} K1"))).is_ok()
                );
            }
        }
        for bad in [
            "E1 D20 K1",
            "F1 D20 K1",
            "D0 E1 K1",
            "D0 F1 K1",
            "M0 D20 K1",
            "D0 Y0 K1",
            "D0 K1X0 K1",
            "K5M0 D20 K1",
            "D0 K5M128 K1",
            "C200 D20 K1",
            "C200E0 D20 K1",
            "D0 C200 K1",
            "D0 D20 K0",
            "D0 D20 K257",
            "D0 D20 D40E0",
        ] {
            assert!(
                IndexedTextAction::parse(name, &args(bad)).is_err(),
                "{name} {bad}"
            );
        }
    }
    for name in ["ASCI", "HEX"] {
        assert!(IndexedTextAction::parse(name, &args("H0123 K1Y200E0 K256")).is_ok());
        assert!(IndexedTextAction::parse(name, &args("D0 D20 D40")).is_err());
    }
    assert!(IndexedTextAction::parse("CCD", &args("D0 D20 D40")).is_ok());
    assert!(IndexedTextAction::parse("CCD", &args("K1 D20 K1")).is_err());
    assert!(IndexedTextAction::parse("CCD", &args("D0 K1Y200E0 K1")).is_err());
}

#[test]
fn every_modifier_bank_is_one_signed_word_for_both_data_roles() {
    for high in [false, true] {
        for bank in 0..8 {
            let suffix = format!("{}{bank}", if high { 'F' } else { 'E' });
            let offset = if bank % 2 == 0 { -1 } else { 1 };
            for (name, input, expected) in [
                ("ASCI", [0x0123, 0], [0x3130, 0x3332]),
                ("HEX", [0x3231, 0x3433], [0x1234, 0x5555]),
                ("CCD", [0x0201, 0], [3, 3]),
            ] {
                let mut m = Memory::default();
                m.set_word(CalendarIndices::alias_device(bank, high).unwrap(), offset);
                // Neighbor poison must not become the high half of the modifier.
                if high {
                    let alias = CalendarIndices::alias_device(bank, true).unwrap();
                    m.set_word(
                        Device {
                            index: alias.index + 1,
                            ..alias
                        },
                        0x4000,
                    );
                }
                let s = (10i16 + offset) as u16;
                let t = (20i16 + offset) as u16;
                put(&mut m, s, &input);
                put(&mut m, t, &[0xaaaa, 0x5555]);
                let n = if name == "CCD" { 2 } else { 4 };
                assert_eq!(
                    run(&format!("{name} D10{suffix} D20{suffix} K{n}"), &mut m).0,
                    Outcome::Complete
                );
                assert_eq!(words(&m, t, 2), expected, "{name}/{suffix}");
            }
        }
    }
}

#[test]
fn literal_ascii_and_hex_repeat_the_word_without_reading_extra_source() {
    let mut m = Memory::default();
    assert_eq!(run("ASCI H0123 D20E0 K8", &mut m).0, Outcome::Complete);
    assert_eq!(words(&m, 20, 4), [0x3130, 0x3332, 0x3130, 0x3332]);
    assert_eq!(run("HEX H3231 D30F0 K8", &mut m).0, Outcome::Complete);
    assert_eq!(words(&m, 30, 2), [0x1212, 0x1212]);
}

#[test]
fn source_kind_scratch_high_byte_precedes_target_and_preserves_low() {
    for (name, source, expected) in [
        ("ASCI", "H0123", 0x01cd),
        ("HEX", "H3231", 0x01cd),
        ("ASCI", "D10E0", 0x00cd),
        ("HEX", "D10E0", 0x00cd),
    ] {
        let mut m = Memory::default();
        m.native_numeric_scratch = Some(0xabcd_u16 as i16);
        m.set_word(d("D1029"), -1);
        let (out, seen) = run(&format!("{name} {source} D0F0 K1"), &mut m);
        assert_eq!(out, Outcome::PlcError { offset: 3 });
        assert_eq!(m.native_numeric_scratch, Some(expected));
        assert!(!seen.contains(&d("D10"))); // binding is not payload snapshot
    }
    let mut m = Memory::default();
    m.native_numeric_scratch = Some(0x3456);
    m.set_word(d("D1028"), -1);
    assert_eq!(
        run("ASCI D0E0 D20F0 K1", &mut m).0,
        Outcome::PlcError { offset: 1 }
    );
    assert_eq!(m.native_numeric_scratch, Some(0x3456));
}

#[test]
fn unknown_private_low_byte_remains_unknown_without_blocking_public_math() {
    for (body, expected) in [("ASCI H1 D20E0 K1", 0x31), ("HEX H31 D20E0 K1", 1)] {
        let mut m = Memory::default();
        assert_eq!(m.native_numeric_scratch, None);
        assert_eq!(run(body, &mut m).0, Outcome::Complete);
        assert_eq!(m.word(d("D20")), expected);
        assert_eq!(m.native_numeric_scratch, None);
    }
}

#[test]
fn asci_rereads_each_numeric_nibble_after_overlapping_store() {
    let mut m = Memory::default();
    put(&mut m, 10, &[0x0123, 0xaaaa]);
    assert_eq!(run("ASCI D10E0 D10F0 K4", &mut m).0, Outcome::Complete);
    // Fourth nibble is now 3, changed by the previous high-byte ASCII write.
    assert_eq!(words(&m, 10, 2), [0x3133, 0x3332]);
}

#[test]
fn hex_rereads_each_numeric_character_and_preserves_partial_output() {
    let mut m = Memory::default();
    put(&mut m, 10, &[0x3231, 0x3433]);
    let (out, seen) = run("HEX D10E0 D10F0 K4", &mut m);
    assert_eq!(out, Outcome::PlcError { offset: 7 });
    assert_eq!(m.word(d("D10")) as u16, 0x1031);
    assert!(!seen.contains(&d("D11")));
}

#[test]
fn numeric_and_group_hex_have_different_pending_lower_nibble_effects() {
    for grouped in [false, true] {
        let mut m = Memory::default();
        put(&mut m, 10, &[0x5a31]);
        put(&mut m, 20, &[0xffff]);
        group(&mut m, d("M128"), 0xffff);
        let target = if grouped { "K4M128F0" } else { "D20F0" };
        assert_eq!(
            run(&format!("HEX D10E0 {target} K4"), &mut m).0,
            Outcome::PlcError { offset: 7 }
        );
        assert_eq!(
            if grouped {
                bits(&m, d("M128"))
            } else {
                m.word(d("D20")) as u16
            },
            if grouped { 0x1fff } else { 0x10ff }
        );
    }
}

#[test]
fn grouped_hex_ignores_ascii_bit_seven_but_numeric_does_not() {
    let mut m = Memory::default();
    group(&mut m, d("M128"), 0x00c1);
    put(&mut m, 10, &[0xc1]);
    assert_eq!(run("HEX K2M128E0 D20F0 K1", &mut m).0, Outcome::Complete);
    assert_eq!(m.word(d("D20")), 10);
    assert_eq!(
        run("HEX D10E0 D21F0 K1", &mut m).0,
        Outcome::PlcError { offset: 7 }
    );
}

#[test]
fn all_short_group_sources_zero_extend_in_fixed_sixteen_bit_slots() {
    for (digits, expected) in [
        (1, [0x3030, 0x4430]),
        (2, [0x3030, 0x4443]),
        (3, [0x4230, 0x4443]),
        (4, [0x4241, 0x4443]),
    ] {
        for base in [d("X0"), d("Y200"), d("M128"), d("S128")] {
            let mut m = Memory::default();
            group(&mut m, base, 0xabcd);
            assert_eq!(
                run(&format!("ASCI K{digits}{base}E0 D20F0 K4"), &mut m).0,
                Outcome::Complete
            );
            assert_eq!(words(&m, 20, 2), expected, "K{digits}{base}");
        }
    }
}

#[test]
fn ascii_odd_tail_short_group_masks_and_neighbor_guards_are_literal() {
    for (digits, first, second) in [
        (1, 0x0001, 0xab03),
        (2, 0x0031, 0xab33),
        (3, 0x0231, 0xa033),
        (4, 0x3231, 0x0033),
    ] {
        for base in [d("Y200"), d("M128"), d("S128")] {
            let mut m = Memory::default();
            group(&mut m, base, 0xabcd);
            group(
                &mut m,
                Device {
                    index: base.index + 16,
                    ..base
                },
                0xabcd,
            );
            let before = Device {
                index: base.index - 1,
                ..base
            };
            let after = Device {
                index: base.index + 32,
                ..base
            };
            m.set_internal(before, true);
            m.set_internal(after, true);
            assert_eq!(
                run(&format!("ASCI H123 K{digits}{base}E0 K3"), &mut m).0,
                Outcome::Complete
            );
            assert_eq!(bits(&m, base), first);
            assert_eq!(
                bits(
                    &m,
                    Device {
                        index: base.index + 16,
                        ..base
                    }
                ),
                second
            );
            assert!(m.bit(before) && m.bit(after));
        }
    }
}

#[test]
fn low_only_group_ascii_clears_full_slots_beyond_declared_width() {
    for (digits, expected) in [(1, 1), (2, 0x31), (3, 0x31), (4, 0x31)] {
        let mut m = Memory::default();
        m.set_system(1161, true);
        group(&mut m, d("M128"), 0xffff);
        assert_eq!(
            run(&format!("ASCI H1 K{digits}M128E0 K1"), &mut m).0,
            Outcome::Complete
        );
        assert_eq!(bits(&m, d("M128")), expected);
    }
}

#[test]
fn ccd_literal_sum_xor_and_all_short_output_widths_clear_two_full_windows() {
    for digits in 1..=4 {
        let mut m = Memory::default();
        put(&mut m, 10, &[0x6f64, 0xca78, 0xd37b]);
        group(&mut m, d("M128"), 0xffff);
        group(&mut m, d("M144"), 0xffff);
        assert_eq!(
            run(&format!("CCD D10E0 K{digits}M128F0 K6"), &mut m).0,
            Outcome::Complete
        );
        assert_eq!(bits(&m, d("M128")), [3, 0x63, 0x363, 0x363][digits - 1]);
        assert_eq!(bits(&m, d("M144")), [1, 0x11, 0x11, 0x11][digits - 1]);
        assert_eq!(m.native_numeric_scratch, Some(867));
    }
}

#[test]
fn source_and_destination_indices_are_cached_across_self_writes() {
    for name in ["ASCI", "HEX", "CCD"] {
        let mut m = Memory::default();
        put(
            &mut m,
            10,
            match name {
                "ASCI" => &[0x0123, 0][..],
                "HEX" => &[0x3231, 0x3433][..],
                _ => &[0x0201, 0][..],
            },
        );
        let count = if name == "CCD" { 2 } else { 4 };
        assert_eq!(
            run(&format!("{name} D10E1 D1182E1 K{count}"), &mut m).0,
            Outcome::Complete
        );
        assert_eq!(
            words(&m, 1182, 2),
            match name {
                "ASCI" => vec![0x3130, 0x3332],
                "HEX" => vec![0x1234, 0],
                _ => vec![3, 3],
            }
        );
        assert_eq!(m.word(d("D1184")), 0);
    }
}

#[test]
fn ccd_snapshots_all_payload_before_late_target_and_count_alias_writes() {
    let mut m = Memory::default();
    put(&mut m, 10, &[0x0201, 0x0403]);
    m.set_word(d("D40"), 4);
    assert_eq!(run("CCD D10E0 D10F0 D40", &mut m).0, Outcome::Complete);
    assert_eq!(words(&m, 10, 2), [10, 4]);
    put(&mut m, 10, &[0x0201, 0x0403]);
    assert_eq!(run("CCD D10E0 D40F0 D40", &mut m).0, Outcome::Complete);
    assert_eq!(words(&m, 40, 2), [10, 4]);
}

#[test]
fn asci_and_hex_walk_physical_counter_halves_on_reads_and_writes() {
    let mut m = Memory::default();
    m.set_word(d("C199"), 0x0123);
    m.set_double_word(d("C200"), 0x55554567);
    assert_eq!(run("ASCI C199E0 D20F0 K8", &mut m).0, Outcome::Complete);
    assert_eq!(words(&m, 20, 4), [0x3534, 0x3736, 0x3130, 0x3332]);
    m.set_word(d("C199"), 0x3231);
    m.set_double_word(d("C200"), 0x36353433);
    assert_eq!(run("HEX C199E0 D30F0 K6", &mut m).0, Outcome::Complete);
    assert_eq!(words(&m, 30, 2), [0x3456, 0x0012]);
    m.set_double_word(d("C201"), 0x5555aaaa);
    assert_eq!(run("ASCI H0123 C199F0 K8", &mut m).0, Outcome::Complete);
    assert_eq!(m.word(d("C199")) as u16, 0x3130);
    assert_eq!(m.double_word(d("C200")) as u32, 0x31303332);
    assert_eq!(m.double_word(d("C201")) as u32, 0x55553332);
    put(
        &mut m,
        10,
        &[0x3231, 0x3433, 0x3635, 0x3837, 0x4139, 0x4342],
    );
    assert_eq!(run("HEX D10E0 C199F0 K12", &mut m).0, Outcome::Complete);
    assert_eq!(m.word(d("C199")) as u16, 0x9abc);
    assert_eq!(m.double_word(d("C200")) as u32, 0x12345678);
}

#[test]
fn effective_natural_counter_starts_at_low_half_for_asci_hex_only() {
    let mut m = Memory::default();
    m.set_word(d("D1028"), 1);
    m.set_double_word(d("C200"), 0x01233231);
    assert_eq!(run("HEX C199E0 D20 K2", &mut m).0, Outcome::Complete);
    assert_eq!(m.word(d("D20")), 0x12);
    assert_eq!(run("ASCI H1 C199E0 K1", &mut m).0, Outcome::Complete);
    assert_eq!(m.double_word(d("C200")) as u32, 0x01230031);
    assert_eq!(
        run("CCD C199E0 D20 K1", &mut m).0,
        Outcome::PlcError { offset: 1 }
    );
}

#[test]
fn ccd_counter_transition_and_initial_natural_target_have_distinct_stages() {
    let mut m = Memory::default();
    m.set_word(d("C199"), 0x0201);
    m.native_numeric_scratch = Some(99);
    m.set_word(d("D20"), 777);
    assert_eq!(run("CCD C199E0 D20F0 K2", &mut m).0, Outcome::Complete);
    assert_eq!(words(&m, 20, 2), [3, 3]);
    m.native_numeric_scratch = Some(99);
    let (out, seen) = run("CCD C199E0 D20F0 K3", &mut m);
    assert_eq!(out, Outcome::PlcError { offset: 5 });
    assert_eq!(m.native_numeric_scratch, Some(99));
    assert!(!seen.contains(&d("D1029")));
    put(&mut m, 10, &[0x0201]);
    m.set_double_word(d("C200"), 0x12345678);
    assert_eq!(
        run("CCD D10E0 C199F0 K2", &mut m).0,
        Outcome::PlcError { offset: 3 }
    );
    assert_eq!(m.word(d("C199")), 3);
    assert_eq!(m.double_word(d("C200")), 0x12345678);
    m.set_word(d("D1029"), 1);
    m.native_numeric_scratch = Some(77);
    assert_eq!(
        run("CCD D10E0 C199F0 K2", &mut m).0,
        Outcome::PlcError { offset: 7 }
    );
    assert_eq!(m.double_word(d("C200")), 0x12345678);
    assert_eq!(m.native_numeric_scratch, Some(3));
}

#[test]
fn checked_final_group_rollovers_distinguish_asci_from_ccd() {
    let mut m = Memory::default();
    m.set_internal(d("S1020"), true);
    m.set_word(d("D20"), 777);
    assert_eq!(
        run("ASCI K1S1020E0 D20F0 K1", &mut m).0,
        Outcome::PlcError { offset: 7 }
    );
    assert_eq!(m.word(d("D20")), 777);
    m.set_internal(d("S1016"), true);
    assert_eq!(run("CCD K1S1016E0 D20F0 K1", &mut m).0, Outcome::Complete);
    assert_eq!(words(&m, 20, 2), [1, 1]);
    m.set_system(1161, true);
    m.native_numeric_scratch = Some(77);
    assert_eq!(
        run("CCD K1S1016E0 D20F0 K1", &mut m).0,
        Outcome::PlcError { offset: 5 }
    );
    assert_eq!(m.native_numeric_scratch, Some(77));
}

#[test]
fn ccd_group_partial_first_store_and_final_xor_no_extra_advance() {
    let mut m = Memory::default();
    put(&mut m, 10, &[0x0201]);
    assert_eq!(
        run("CCD D10E0 K1S1008F0 K2", &mut m).0,
        Outcome::PlcError { offset: 3 }
    );
    assert_eq!(bits(&m, d("S1008")), 3);
    assert_eq!(m.native_numeric_scratch, Some(3));
    assert_eq!(run("CCD D10E0 K1S992F0 K2", &mut m).0, Outcome::Complete);
    assert_eq!(bits(&m, d("S992")), 3);
    assert_eq!(bits(&m, d("S1008")), 3);
}

#[test]
fn model_endpoints_and_negative_offsets_remain_real_words_not_zero_padding() {
    for last in [4999, 9999] {
        let mut m = Memory::default();
        m.data_register_limit = Some(last);
        m.set_word(
            Device {
                area: Area::D,
                index: last,
            },
            0x0201,
        );
        assert_eq!(
            run(&format!("CCD D{last}E0 D20F0 K2"), &mut m).0,
            Outcome::Complete
        );
        assert_eq!(words(&m, 20, 2), [3, 3]);
        assert_eq!(
            run(&format!("CCD D{last}E0 D20F0 K3"), &mut m).0,
            Outcome::PlcError { offset: 5 }
        );
        put(&mut m, 10, &[0x0201]);
        assert_eq!(
            run(&format!("CCD D10E0 D{last}F0 K2"), &mut m).0,
            Outcome::PlcError { offset: 3 }
        );
        assert_eq!(
            m.word(Device {
                area: Area::D,
                index: last
            }),
            3
        );
        m.set_word(d("D1029"), -1);
        assert_eq!(
            run(&format!("ASCI H123 D{last}F0 K4"), &mut m).0,
            Outcome::Complete
        );
        assert_eq!(words(&m, last - 1, 2), [0x3130, 0x3332]);
    }
}

#[test]
fn source_count_target_priority_and_visited_storage_are_explicit() {
    let mut m = Memory::default();
    m.set_word(d("D1028"), -1);
    m.set_word(d("D1029"), -1);
    m.set_word(d("D40"), 0);
    let (out, seen) = run("CCD D0E0 D0F0 D40", &mut m);
    assert_eq!(out, Outcome::PlcError { offset: 1 });
    assert!(!seen.contains(&d("D40")));
    assert!(!seen.contains(&d("D1029")));
    m.set_word(d("D1028"), 0);
    let (out, seen) = run("CCD D10E0 D0F0 D40", &mut m);
    assert_eq!(out, Outcome::PlcError { offset: 5 });
    assert!(!seen.contains(&d("D10")));
    assert!(!seen.contains(&d("D1029")));
    assert!(!seen.contains(&d("M1161")));
    put(&mut m, 10, &[0x5a]);
    let (out, seen) = run("HEX D10E0 D0F0 K1", &mut m);
    assert_eq!(out, Outcome::PlcError { offset: 3 });
    assert!(!seen.contains(&d("D10")));
    assert!(!seen.contains(&d("M1161")));
}

#[test]
fn flags_hold_and_sticky_error_code_pc_are_independently_latched() {
    for (latch_code, latch_pc, code, pc) in [
        (false, false, 0xe1a, 7),
        (true, false, 0x3456, 7),
        (false, true, 0xe1a, 123),
        (true, true, 0x3456, 123),
    ] {
        let mut m = Memory::default();
        seed_flags(&mut m);
        put(&mut m, 10, &[0x5a]);
        m.set_system(1067, latch_code);
        m.set_system(1068, latch_pc);
        m.set_word(d("D1067"), 0x3456);
        m.set_word(d("D1068"), 123);
        assert_eq!(
            run("HEX D10E0 D20F0 K1", &mut m).0,
            Outcome::PlcError { offset: 7 }
        );
        assert_eq!(m.word(d("D1067")) as u16, code);
        assert_eq!(m.word(d("D1068")), pc);
        assert_eq!(flags(&m), [true, false, true]);
    }
}

#[test]
fn real_clock_reads_require_declared_levels_but_disabled_visit_reads_nothing() {
    let op = a("ASCI K1M1011E0 D20F0 K1");
    let mut m = Memory::default();
    m.system_relays.set_profile(ClockProfile::InstalledFixed);
    let mut seen = BTreeSet::new();
    assert_eq!(op.execute(&mut m, false, &mut seen), Outcome::Nominal);
    assert!(seen.is_empty());
    assert!(matches!(
        op.execute(&mut m, true, &mut seen),
        Outcome::Limitation(_)
    ));
    assert!(seen.contains(&d("M1011")));
    assert!(!seen.contains(&d("M1012")));
    m.execution_limitations.clear();
    ClockState::seed_installed_levels(&mut m, [true, false, true, false]).unwrap();
    assert_eq!(
        op.execute(&mut m, true, &mut BTreeSet::new()),
        Outcome::Complete
    );
    assert_eq!(m.word(d("D20")), 0x35);
}

#[test]
fn unqualified_raw_m_gap_and_unchecked_spills_are_limitations_not_plc_errors() {
    for body in [
        "HEX K1M1532E0 D20 K2",
        "ASCI H1234 D9999E0 K8",
        "HEX D9999E0 D20 K4",
    ] {
        let mut m = Memory::default();
        m.set_word(d("D9999"), 0x3231);
        let out = run(body, &mut m).0;
        assert!(matches!(out, Outcome::Limitation(_)), "{body}: {out:?}");
        assert!(!m.system_bit(1067));
    }
}

#[test]
fn native_capture_order_uses_actual_sources_and_never_commits_clone_outputs() {
    let mut local = Memory::default();
    put(&mut local, 10, &[0x0201]);
    local.set_word(d("D40"), 1);
    local.native_numeric_scratch = Some(777);
    let mut fetched = Vec::new();
    let visit = a("CCD D10E0 D20F0 D40").qualify_native_with(&local, |m, ds| {
        fetched.extend(ds.iter().copied());
        for value in ds {
            if *value == d("D40") {
                m.set_word(*value, 2);
            } else if *value == d("D10") {
                m.set_word(*value, 0x0403);
            }
        }
        Ok(())
    });
    assert_eq!(visit.outcome, Outcome::Complete);
    let pos = |s| fetched.iter().position(|v| *v == d(s)).unwrap();
    assert!(
        pos("D1028") < pos("D40")
            && pos("D40") < pos("M1161")
            && pos("M1161") < pos("D10")
            && pos("D10") < pos("D1029")
    );
    assert_eq!(local.word(d("D20")), 0);
    assert_eq!(local.native_numeric_scratch, Some(777));
    assert_eq!(local.word(d("D40")), 1);
}

#[test]
fn capture_overlay_does_not_restore_payload_modified_by_prior_output() {
    let local = Memory::default();
    let mut fetched = BTreeSet::new();
    let visit = a("HEX D10E0 D10F0 K4").qualify_native_with(&local, |m, ds| {
        for value in ds {
            assert!(fetched.insert(*value), "recapture {value}");
            if *value == d("D10") {
                m.set_word(*value, 0x3231);
            }
        }
        Ok(())
    });
    assert_eq!(visit.outcome, Outcome::PlcError { offset: 7 });
    assert!(!fetched.contains(&d("D11")));
    assert_eq!(local.word(d("D10")), 0);
}

#[test]
fn callback_failure_is_a_limitation_and_unvisited_target_is_never_requested() {
    let local = Memory::default();
    let mut fetched = BTreeSet::<Device>::new();
    let visit = a("CCD D10E0 D20F0 D40").qualify_native_with(&local, |_, ds| {
        fetched.extend(ds);
        if ds.contains(&d("D40")) {
            Err("capture intentionally unavailable".into())
        } else {
            Ok(())
        }
    });
    assert_eq!(
        visit.outcome,
        Outcome::Limitation("capture intentionally unavailable".into())
    );
    assert!(!fetched.contains(&d("D1029")));
    assert!(!local.system_bit(1067));
}

#[test]
fn monitor_candidates_require_mode_count_and_modifiers_and_keep_natural_counter_rows() {
    let mut m = Memory::default();
    m.set_word(d("D40"), 2);
    let op = a("CCD D10E1 D20F7 D40");
    assert_eq!(
        op.monitor_input_devices(&m).unwrap(),
        BTreeSet::from([d("D1182"), d("D1195"), d("D40"), d("M1161")])
    );
    let c = a("ASCI C199E0 D20F0 K8").capture_devices(&m).unwrap();
    assert!(c.contains(&d("C199")) && c.contains(&d("C200")));
    assert!(!c.contains(&d("C201")));
    m.set_word(d("D40"), 0);
    assert!(!op.capture_devices(&m).unwrap().contains(&d("D10")));
}

#[test]
fn typed_legacy_adapter_and_public_engine_share_scratch_and_live_behavior() {
    for (name, body, source, expected) in [
        (
            "ASCI",
            "D10 D10 K4",
            vec![0x0123, 0xaaaa],
            vec![0x3133, 0x3332],
        ),
        ("HEX", "D10 D20 K4", vec![0x3231, 0x3433], vec![0x1234, 0]),
        ("CCD", "D10 D20 K2", vec![0x0201], vec![3, 3]),
    ] {
        let old = TextAction::parse(name, &args(body)).unwrap();
        let wrapped = IndexedTextAction::from_legacy(&old).unwrap();
        let mut m = Memory::default();
        put(&mut m, 10, &source);
        m.native_numeric_scratch = Some(0x12ab);
        let mut copy = m.clone();
        old.execute(&mut m);
        assert_eq!(
            wrapped.execute(&mut copy, true, &mut BTreeSet::new()),
            Outcome::Complete
        );
        let target = if name == "ASCI" { 10 } else { 20 };
        assert_eq!(words(&m, target, 2), expected);
        assert_eq!(words(&copy, target, 2), expected);
        assert_eq!(m.native_numeric_scratch, copy.native_numeric_scratch);
    }
}

#[test]
fn public_vm_all_six_forms_and_aliases_keep_seven_words_pulse_rearm_and_markers() {
    for name in [
        "ASCI", "ASCIP", "ASCII", "ASCIIP", "HEX", "HEXP", "CCD", "CCDP",
    ] {
        let n = if name.starts_with("CCD") { 2 } else { 1 };
        let p = compile_il(&format!("LD X0\n{name} D10E0 D20F0 K{n}\nINC D300\nEND")).unwrap();
        assert!(p.requires_vm());
        let encoded = crate::es2_encoder::encode(&p).unwrap();
        assert_eq!(encoded.action_pc[1], encoded.action_pc[0] + 7);
        let mut sim = Simulator::default();
        sim.memory.set_word(d("D20"), 777);
        for (on, value, expected_first, markers) in [
            (false, 1, 777, 0),
            (true, 1, 1, 1),
            (true, 2, if name.ends_with('P') { 1 } else { 2 }, 2),
            (false, 2, if name.ends_with('P') { 1 } else { 2 }, 2),
            (true, 2, 2, 3),
        ] {
            sim.memory.set_input(d("X0"), on);
            sim.memory.set_word(
                d("D10"),
                if name.starts_with("HEX") {
                    0x30 + value
                } else {
                    value
                },
            );
            complete(&mut sim, &p);
            let expected = if expected_first == 777 {
                777
            } else if name.starts_with("ASCI") {
                0x30 + expected_first
            } else {
                expected_first
            };
            assert_eq!(sim.memory.word(d("D20")), expected, "{name}");
            assert_eq!(sim.memory.word(d("D300")), markers);
        }
    }
}

#[test]
fn public_vm_error_pc_uses_fresh_original_stage_and_always_executes_successor() {
    for (body, offset) in [
        ("ASCI D0E0 D20 K1", 1),
        ("ASCI H1 D0F0 K1", 3),
        ("CCD D10E1 D20 D40", 5),
        ("HEX D10E1 D20 K1", 7),
    ] {
        let p = compile_il(&format!("LD X0\n{body}\nMOV K1234 D300\nEND")).unwrap();
        let encoded = crate::es2_encoder::encode(&p).unwrap();
        assert_eq!(encoded.action_pc[0], 1);
        let mut sim = Simulator::default();
        sim.memory.set_input(d("X0"), true);
        sim.memory.set_word(d("D1028"), -1);
        sim.memory.set_word(d("D1029"), -1);
        sim.memory.set_word(d("D10"), 0x5a);
        complete(&mut sim, &p);
        assert_eq!(sim.memory.word(d("D1068")), 1 + offset);
        assert_eq!(sim.memory.word(d("D300")), 1234);
    }
}

#[test]
fn public_vm_same_scan_mode_indices_and_master_false_action_skip() {
    let p=compile_il("LD X7\nMC N0\nLDI X7\nASCIP D0E0 D0F0 K1\nMCR N0\nLD X0\nMOV K1 E0\nASCI H123 D20E0 K4\nSET M1161\nMOV K2 E0\nASCI H123 D20E0 K4\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim.memory.set_word(d("D1028"), -1);
    sim.memory.set_word(d("D1029"), -1);
    complete(&mut sim, &p);
    assert!(!sim.memory.system_bit(1067));
    assert_eq!(words(&sim.memory, 21, 5), [0x3130, 0x30, 0x31, 0x32, 0x33]);
}

#[test]
fn public_vm_cj_skips_all_inputs_and_out_context_does_not_gate_body() {
    let p = compile_il("LD X1\nCJ P0\nLD X0\nCCD D0E0 D0F0 D40\nP0\nLD X0\nASCI H1 D20E1 K1\nEND")
        .unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X1"), true);
    sim.memory.set_input(d("X0"), true);
    sim.memory.set_word(d("D1028"), -1);
    sim.memory.set_word(d("D1029"), -1);
    complete(&mut sim, &p);
    assert!(!sim.memory.system_bit(1067));
    assert!(!sim.observed_devices.contains(&d("D40")));
    let p = compile_il("LD X0\nASCI H1 D20E0 K1\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim.begin_plan_scan(&p, 0, true).unwrap();
    sim.execute_plan_step(&p).unwrap();
    sim.control.out_enabled = false;
    sim.control.master_gate = 1;
    sim.execute_plan_step(&p).unwrap();
    assert_eq!(sim.current_plan_pc(), Some(8));
    assert_eq!(sim.memory.word(d("D20")), 0x31);
}
