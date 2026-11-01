//! Independent compiled-VM expectations for indexed BCD/BIN source and target
//! roles. Constants below are literal conversion results, not encoder output.
use crate::plc::{Device, Memory, Program, ScanStatus, Simulator, compile_il};

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}

fn enabled() -> Simulator {
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim
}

fn run(sim: &mut Simulator, program: &Program) {
    sim.scan(program, 0);
    assert_eq!(sim.last_scan, ScanStatus::Complete);
}

fn bits(memory: &Memory, base: u16, count: u16) -> u32 {
    (0..count).fold(0, |value, bit| {
        value
            | (u32::from(memory.bit(Device {
                area: crate::plc::Area::M,
                index: base + bit,
            })) << bit)
    })
}

fn set_bits(memory: &mut Memory, base: u16, count: u16, value: u32) {
    for bit in 0..count {
        memory.set_internal(
            Device {
                area: crate::plc::Area::M,
                index: base + bit,
            },
            value & (1 << bit) != 0,
        );
    }
}

#[test]
fn all_eight_forms_compile_encode_and_preserve_flags_with_literal_outputs() {
    for (name, wide, header, input, expected) in [
        ("BCD", false, 0x3400, 1234, 0x1234),
        ("BCDP", false, 0x3410, 1234, 0x1234),
        ("DBCD", true, 0x3500, 12_345_678, 0x1234_5678),
        ("DBCDP", true, 0x3510, 12_345_678, 0x1234_5678),
        ("BIN", false, 0x3600, 0x9876, 9876),
        ("BINP", false, 0x3610, 0x9876, 9876),
        ("DBIN", true, 0x3700, 0x9876_5432u32 as i32, 98_765_432),
        ("DBINP", true, 0x3710, 0x9876_5432u32 as i32, 98_765_432),
    ] {
        let program = compile_il(&format!("LD X0\n{name} D10E1 D100F0\nEND")).unwrap();
        assert!(program.requires_vm());
        let encoded = crate::es2_encoder::encode(&program).unwrap();
        let pc = usize::from(encoded.action_pc[0]);
        let expected_words = if wide {
            vec![
                header, 0x14e6, 0x8086, 0x0080, 0x0080, 0xc8f6, 0x0086, 0x0080, 0x0080,
            ]
        } else {
            vec![header, 0x14e6, 0x8086, 0xc8f6, 0x0086]
        };
        assert_eq!(
            &encoded.words[pc..pc + expected_words.len()],
            expected_words,
            "{name}"
        );
        let mut sim = enabled();
        sim.memory.set_word(d("D1182"), 1);
        sim.memory.set_word(d("D1029"), 2);
        if wide {
            sim.memory.set_double_word(d("D11"), input);
        } else {
            sim.memory.set_word(d("D11"), input as i16);
        }
        sim.memory.set_word(d("D101"), 77);
        sim.memory.set_word(d("D104"), 88);
        for (flag, value) in [(1020, true), (1021, false), (1022, true)] {
            sim.memory.set_system(flag, value);
        }
        run(&mut sim, &program);
        assert_eq!(
            if wide {
                sim.memory.double_word(d("D102"))
            } else {
                i32::from(sim.memory.word(d("D102")))
            },
            expected,
            "{name}"
        );
        assert_eq!(sim.memory.word(d("D101")), 77);
        assert_eq!(sim.memory.word(d("D104")), 88);
        assert_eq!(
            [1020, 1021, 1022].map(|n| sim.memory.system_bit(n)),
            [true, false, true]
        );
        assert!(!program.devices.contains(&d("D100")));
        assert!(sim.observed_devices.contains(&d("D102")));
    }
}

#[test]
fn all_sixteen_index_aliases_remain_signed_words_for_both_data_widths() {
    for (suffix, alias) in [
        ("E0", "D1028"),
        ("F0", "D1029"),
        ("E1", "D1182"),
        ("F1", "D1183"),
        ("E2", "D1184"),
        ("F2", "D1185"),
        ("E3", "D1186"),
        ("F3", "D1187"),
        ("E4", "D1188"),
        ("F4", "D1189"),
        ("E5", "D1190"),
        ("F5", "D1191"),
        ("E6", "D1192"),
        ("F6", "D1193"),
        ("E7", "D1194"),
        ("F7", "D1195"),
    ] {
        for name in ["BCD", "DBCD", "BIN", "DBIN"] {
            let program =
                compile_il(&format!("LD X0\n{name} D10{suffix} D100{suffix}\nEND")).unwrap();
            let mut sim = enabled();
            sim.memory.set_word(d(alias), -2);
            let neighbor = Device {
                area: crate::plc::Area::D,
                index: d(alias).index + 1,
            };
            sim.memory.set_word(neighbor, i16::MIN);
            let bcd = name.contains("BCD");
            if name.starts_with('D') {
                sim.memory
                    .set_double_word(d("D8"), if bcd { 5678 } else { 0x5678 });
            } else {
                sim.memory
                    .set_word(d("D8"), if bcd { 5678 } else { 0x5678 });
            }
            run(&mut sim, &program);
            assert_eq!(
                sim.memory.word(d("D98")),
                if bcd { 0x5678 } else { 5678 },
                "{name} {suffix}"
            );
            assert!(
                sim.observed_devices.contains(&d(alias)) && sim.observed_devices.contains(&d("D8"))
            );
            assert!(
                !sim.observed_devices.contains(&neighbor),
                "index is word-only: {name} {suffix}"
            );
        }
    }
}

#[test]
fn invalid_value_precedes_bad_target_index_and_preserves_sticky_error_pc() {
    for (name, input) in [
        ("BCD", -1),
        ("BIN", 0xa001),
        ("DBCD", 100_000_000),
        ("DBIN", 0xa000_0001u32 as i32),
    ] {
        let program = compile_il(&format!("LD X0\n{name} D10E0 D100F7\nEND")).unwrap();
        let mut sim = enabled();
        sim.memory.set_word(d("D1028"), 1);
        sim.memory.set_word(d("D1195"), -30000);
        if name.starts_with('D') {
            sim.memory.set_double_word(d("D11"), input);
        } else {
            sim.memory.set_word(d("D11"), input as i16);
        }
        sim.memory.set_word(d("D100"), 77);
        sim.memory.set_system(1068, true);
        sim.memory.set_word(d("D1068"), 321);
        run(&mut sim, &program);
        assert!(sim.memory.execution_limitations.is_empty());
        assert_eq!(sim.memory.word(d("D1067")), 0xe18);
        assert_eq!(sim.memory.word(d("D1068")), 321);
        assert_eq!(sim.memory.word(d("D100")), 77);
        assert!(!sim.observed_devices.contains(&d("D1195")));
        sim.memory.set_system(1068, false);
        run(&mut sim, &program);
        assert_eq!(
            sim.memory.word(d("D1068")),
            (program.native_action_pc[0] + 1) as i16
        );
        assert!(!sim.observed_devices.contains(&d("D1195")));
    }
}

#[test]
fn pulses_skip_disabled_and_held_addresses_then_rearm_on_the_next_edge() {
    for name in ["BCDP", "BINP", "DBCDP", "DBINP"] {
        let program = compile_il(&format!("LD X0\n{name} D10E0 D100F0\nEND")).unwrap();
        let mut sim = Simulator::default();
        sim.memory.set_word(d("D1028"), -30000);
        run(&mut sim, &program);
        assert!(sim.memory.execution_limitations.is_empty());
        let bcd = name.contains("BCD");
        sim.memory.set_word(d("D1028"), 0);
        sim.memory
            .set_double_word(d("D10"), if bcd { 12 } else { 0x12 });
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &program);
        let first = sim.memory.double_word(d("D100"));
        assert_eq!(first, if bcd { 0x12 } else { 12 });
        sim.memory.set_word(d("D1028"), -30000);
        run(&mut sim, &program);
        assert!(sim.memory.execution_limitations.is_empty());
        assert_eq!(sim.memory.double_word(d("D100")), first);
        sim.memory.set_input(d("X0"), false);
        run(&mut sim, &program);
        sim.memory.set_word(d("D1028"), 1);
        sim.memory
            .set_double_word(d("D11"), if bcd { 34 } else { 0x34 });
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &program);
        assert_eq!(
            sim.memory.double_word(d("D100")),
            if bcd { 0x34 } else { 34 }
        );
    }
}

#[test]
fn source_group_overlap_is_snapshotted_and_short_targets_preserve_neighbors() {
    let program = compile_il("LD X0\nBIN K4M0E0 K4M4E0\nBCD D10E0 K1M128F0\nEND").unwrap();
    let mut sim = enabled();
    set_bits(&mut sim.memory, 0, 16, 0x1234);
    sim.memory.set_internal(d("M20"), true);
    sim.memory.set_word(d("D10"), 9999);
    sim.memory.set_word(d("D1029"), 1);
    sim.memory.set_internal(d("M128"), true);
    sim.memory.set_internal(d("M133"), true);
    run(&mut sim, &program);
    assert_eq!(bits(&sim.memory, 4, 16), 1234);
    assert_eq!(bits(&sim.memory, 0, 4), 4);
    assert!(sim.memory.bit(d("M20")));
    assert_eq!(bits(&sim.memory, 129, 4), 9);
    assert!(sim.memory.bit(d("M128")) && sim.memory.bit(d("M133")));

    // An invalid upper digit cannot be masked away by a four-bit destination.
    let invalid = compile_il("LD X0\nDBIN D20E0 K1M129E0\nEND").unwrap();
    sim.memory.set_double_word(d("D20"), 0xa000_0009u32 as i32);
    run(&mut sim, &invalid);
    assert_eq!(bits(&sim.memory, 129, 4), 9);
    assert_eq!(sim.memory.word(d("D1067")), 0xe18);
}

#[test]
fn wide_self_index_target_and_source_overlap_keep_one_resolved_cursor() {
    let program = compile_il("LD X0\nDBCD D10E0 D1028F0\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_double_word(d("D10"), 12_345_678);
    sim.memory.set_word(d("D1030"), 77);
    run(&mut sim, &program);
    assert_eq!(sim.memory.word(d("D1028")) as u16, 0x5678);
    assert_eq!(sim.memory.word(d("D1029")) as u16, 0x1234);
    assert_eq!(sim.memory.word(d("D1030")), 77);

    let overlap = compile_il("LD X0\nDBIN D20E1 D20E1\nEND").unwrap();
    sim.memory.set_word(d("D1182"), 1);
    sim.memory.set_double_word(d("D21"), 0x9999_9999u32 as i32);
    sim.memory.set_word(d("D23"), 88);
    run(&mut sim, &overlap);
    assert_eq!(sim.memory.double_word(d("D21")), 99_999_999);
    assert_eq!(sim.memory.word(d("D23")), 88);
}

#[test]
fn natural_counters_and_c199_low_only_keep_their_distinct_storage_widths() {
    let program =
        compile_il("LD X0\nDBCD C199E0 D20F0\nDBIN D30F0 C200E1\nDBCD D40F0 C255E2\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_word(d("D1028"), 1);
    sim.memory.set_word(d("D1182"), -1);
    sim.memory.set_double_word(d("C200"), 12_345_678);
    sim.memory.set_double_word(d("D30"), 0x0000_9999);
    sim.memory.set_double_word(d("D40"), 87_654_321);
    run(&mut sim, &program);
    assert_eq!(sim.memory.double_word(d("D20")), 0x1234_5678);
    assert_eq!(sim.memory.word(d("C199")), 9999);
    assert_eq!(sim.memory.double_word(d("C200")), 12_345_678);
    assert_eq!(sim.memory.double_word(d("C255")) as u32, 0x8765_4321);
}

#[test]
fn direct_e_pair_and_word_f_share_canonical_storage_without_modifier_confusion() {
    let program =
        compile_il("LD X0\nBCD F1 D20E0\nBIN D30E0 F7\nDBCD E2 D40F0\nDBIN D50F0 E3\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_word(d("D1183"), 4321);
    sim.memory.set_word(d("D30"), 0x5678);
    sim.memory.set_double_word(d("D1184"), 12_345_678);
    sim.memory.set_double_word(d("D50"), 0x8765_4321u32 as i32);
    run(&mut sim, &program);
    assert_eq!(sim.memory.word(d("D20")), 0x4321);
    assert_eq!(sim.memory.word(d("D1195")), 5678);
    assert_eq!(sim.memory.double_word(d("D40")), 0x1234_5678);
    assert_eq!(sim.memory.double_word(d("D1186")), 87_654_321);
}
