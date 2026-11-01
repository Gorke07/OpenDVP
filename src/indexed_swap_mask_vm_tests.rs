//! Independent byte/mask, storage and visit expectations for eight forms.
use crate::additional_processing::AdditionalAction;
use crate::indexed_swap_mask::IndexedSwapMaskAction;
use crate::plc::{Device, Memory, Program, ScanStatus, Simulator, compile_il};
use std::collections::BTreeSet;
fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn args(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(str::to_owned).collect()
}
fn action(raw: &str) -> IndexedSwapMaskAction {
    let mut parts = raw.split_whitespace();
    IndexedSwapMaskAction::parse(
        parts.next().unwrap(),
        &parts.map(str::to_owned).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn flags(m: &Memory) -> [bool; 3] {
    [1020, 1021, 1022].map(|n| m.system_bit(n))
}
fn seed(m: &mut Memory) {
    for n in 1020..=1022 {
        m.set_system(n, true);
    }
}
fn packed(m: &Memory, base: Device, width: u16) -> u32 {
    (0..width).fold(0, |v, n| {
        v | (u32::from(m.bit(Device {
            index: base.index + n,
            ..base
        })) << n)
    })
}
fn put_group(m: &mut Memory, base: Device, width: u16, value: u32) {
    for n in 0..width {
        let device = Device {
            index: base.index + n,
            ..base
        };
        let value = value & (1u32 << n) != 0;
        if base.area == crate::plc::Area::X {
            m.set_input(device, value);
        } else {
            m.set_internal(device, value);
        }
    }
}
fn enabled() -> Simulator {
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim
}
fn run(sim: &mut Simulator, p: &Program) {
    sim.scan(p, 0);
    assert_eq!(sim.last_scan, ScanStatus::Complete);
}

#[test]
fn eight_headers_original_descriptors_and_instruction_lengths_are_literal() {
    for (name, header, wide, swap) in [
        ("SWAP", 0x4601, false, true),
        ("SWAPP", 0x4611, false, true),
        ("DSWAP", 0x4701, true, true),
        ("DSWAPP", 0x4711, true, true),
        ("MVM", 0x6e01, false, false),
        ("MVMP", 0x6e11, false, false),
        ("DMVM", 0x6f01, true, false),
        ("DMVMP", 0x6f11, true, false),
    ] {
        let body = if swap {
            format!("{name} D20F7")
        } else {
            format!("{name} D10E1 K15 D20F7")
        };
        let a = action(&body);
        let mut expected = vec![header];
        if !swap {
            expected.extend([0x14e6, 0x8086]);
            if wide {
                expected.extend([0x0080, 0x0080]);
            }
            expected.extend([0x0f80, 0x0080]);
            if wide {
                expected.extend([0x0080, 0x0080]);
            }
        }
        expected.extend([0x28f6, 0x8089]);
        if wide {
            expected.extend([0x0080, 0x0080]);
        }
        assert_eq!(a.native_words().unwrap(), expected);
        assert_eq!(a.label(), body);
        assert_eq!(a.source_count(), if swap { 1 } else { 3 });
        let p = compile_il(&format!("LD X0\n{body}\nMOV K7 D300\nOUT Y0\nEND")).unwrap();
        assert!(p.requires_vm());
        let native = crate::es2_encoder::encode(&p).unwrap();
        assert_eq!(&native.words[1..1 + expected.len()], expected);
        assert_eq!(native.action_pc[1] as usize, 1 + expected.len());
        let mut sim = enabled();
        sim.memory.set_double_word(d("D10"), 0x12345678);
        sim.memory.set_double_word(d("D20"), 0x12345678);
        seed(&mut sim.memory);
        run(&mut sim, &p);
        if swap {
            assert_eq!(
                sim.memory.double_word(d("D20")) as u32,
                if wide { 0x34127856 } else { 0x12347856 }
            );
        } else {
            assert_eq!(sim.memory.double_word(d("D20")) as u32, 0x12345678);
        }
        assert_eq!(flags(&sim.memory), [true; 3]);
        assert_eq!(sim.memory.word(d("D300")), 7);
        assert!(sim.memory.bit(d("Y0")));
    }
}

#[test]
fn all_sixteen_modifier_banks_use_signed_word_not_a_neighboring_pair() {
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
        let mut m = Memory::default();
        m.set_word(d(alias), -2);
        let next = Device {
            index: d(alias).index + 1,
            ..d(alias)
        };
        m.set_word(next, i16::MIN);
        m.set_double_word(d("D18"), 0x12345678);
        let mut observed = BTreeSet::new();
        let a = action(&format!("DSWAP D20{suffix}"));
        a.execute(&mut m, true, &mut observed);
        assert_eq!(m.double_word(d("D18")) as u32, 0x34127856);
        assert_eq!(a.modifier_devices(), vec![d(alias)]);
        assert!(observed.contains(&d(alias)) && !observed.contains(&next));
        m.set_double_word(d("D8"), 0x12345678);
        m.set_double_word(d("D28"), 0x00ff00ff);
        m.set_double_word(d("D18"), 0xaabbccddu32 as i32);
        observed.clear();
        action(&format!("DMVM D10{suffix} D30{suffix} D20{suffix}")).execute(
            &mut m,
            true,
            &mut observed,
        );
        assert_eq!(m.double_word(d("D18")) as u32, 0xaa34cc78);
        assert!(!observed.contains(&next));
    }
}

#[test]
fn role_matrices_keep_scoped_f_width_and_disallow_writing_inputs() {
    for (name, wide) in [
        ("SWAP", false),
        ("SWAPP", false),
        ("DSWAP", true),
        ("DSWAPP", true),
    ] {
        for target in ["K1Y20E0", "K3M80F7", "T255E1", "C199F0", "D9999E0", "E1"] {
            assert!(IndexedSwapMaskAction::parse(name, &args(target)).is_ok());
        }
        assert_eq!(
            IndexedSwapMaskAction::parse(name, &args("F1")).is_ok(),
            !wide
        );
        for target in ["K1X0E0", "X0E0", "M0E0", "K1", "HFFFF", "D0E8", "E1E0"] {
            assert!(
                IndexedSwapMaskAction::parse(name, &args(target)).is_err(),
                "{name} {target}"
            );
        }
        assert_eq!(
            IndexedSwapMaskAction::parse(name, &args("C200E0")).is_ok(),
            wide
        );
        assert_eq!(
            IndexedSwapMaskAction::parse(name, &args("K8M0E0")).is_ok(),
            wide
        );
    }
    for name in ["MVM", "MVMP", "DMVM", "DMVMP"] {
        for raw in [
            "K1X0E0 K-1 D20F0",
            "E1 HFFFF D20E0",
            "D0E0 F1 F7",
            "D0E0 K1 K1Y20F0",
        ] {
            assert!(IndexedSwapMaskAction::parse(name, &args(raw)).is_ok());
        }
        for raw in [
            "K1 K1 D20E0",
            "H1 K1 D20E0",
            "D0E0 K1 K1X0",
            "D0E0 K1 X0E0",
            "D0E0 K1 M0E0",
            "D0E0 K1 E1F0",
            "D0E0 K1 K9M0",
            "D0E0 K1 D0F8",
        ] {
            assert!(
                IndexedSwapMaskAction::parse(name, &args(raw)).is_err(),
                "{name} {raw}"
            );
        }
    }
    assert!(!IndexedSwapMaskAction::supports("SWAPPP"));
    assert!(!IndexedSwapMaskAction::supports("RAND"));
}

#[test]
fn short_group_swap_zero_extends_then_truncates_without_touching_guards() {
    for base in ["Y20", "M80", "S80"] {
        for (name, raw, expected) in [
            ("SWAP", 0x1234u32, vec![0, 0, 0x402, 0x3412]),
            (
                "DSWAP",
                0x12345678,
                vec![0, 0, 0x806, 0x7856, 0x7856, 0x7856, 0x4027856, 0x34127856],
            ),
        ] {
            for (index, result) in expected.into_iter().enumerate() {
                let digits = index as u16 + 1;
                let width = digits * 4;
                let original = d(base);
                let effective = Device {
                    index: original.index + 1,
                    ..original
                };
                let mut m = Memory::default();
                m.set_word(d("D1028"), 1);
                put_group(&mut m, effective, width, raw);
                m.set_internal(original, true);
                m.set_internal(
                    Device {
                        index: effective.index + width,
                        ..effective
                    },
                    true,
                );
                action(&format!("{name} K{digits}{base}E0")).execute(
                    &mut m,
                    true,
                    &mut BTreeSet::new(),
                );
                assert_eq!(
                    packed(&m, effective, width),
                    result,
                    "{name} K{digits}{base}"
                );
                assert!(m.bit(original));
                assert!(m.bit(Device {
                    index: effective.index + width,
                    ..effective
                }));
            }
        }
    }
}

#[test]
fn mask_move_uses_full_width_source_mask_and_old_value_before_narrow_store() {
    for (name, wide) in [("MVM", false), ("DMVM", true)] {
        let mut m = Memory::default();
        m.set_double_word(d("D0"), 0x12345678);
        m.set_double_word(d("D20"), 0xaabbccddu32 as i32);
        seed(&mut m);
        let mask = if wide { "H00FF00FF" } else { "H00FF" };
        action(&format!("{name} D0E0 {mask} D20F0")).execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(
            m.double_word(d("D20")) as u32,
            if wide { 0xaa34cc78 } else { 0xaabbcc78 }
        );
        assert_eq!(flags(&m), [true; 3]);
    }
    let mut m = Memory::default();
    put_group(&mut m, d("X0"), 4, 0xf);
    m.set_double_word(d("D20"), -1);
    action("DMVM K1X0E0 HFFFFFFFF D20F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.double_word(d("D20")), 15);
    m.set_word(d("D0"), 0x1234);
    put_group(&mut m, d("M80"), 4, 0xa);
    m.set_internal(d("M79"), true);
    m.set_internal(d("M84"), true);
    action("MVM D0E0 H00F0 K1M80F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(packed(&m, d("M80"), 4), 0xa);
    assert!(m.bit(d("M79")) && m.bit(d("M84")));
    action("MVM D0E0 H000F K1M80F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(packed(&m, d("M80"), 4), 4);
}

#[test]
fn all_direct_wide_f_aliases_work_as_source_mask_and_destination_without_descriptor_loss() {
    for (index, low, word0, word1) in [
        (0, 1029, 0x3a86, 0x0080),
        (1, 1183, 0x6e86, 0x0180),
        (2, 1185, 0x7286, 0x0180),
        (3, 1187, 0x7686, 0x0180),
        (4, 1189, 0x7a86, 0x0180),
        (5, 1191, 0x7e86, 0x0180),
        (6, 1193, 0x8286, 0x0180),
        (7, 1195, 0x8686, 0x0180),
    ] {
        let mut m = Memory::default();
        let alias = d(&format!("D{low}"));
        m.set_double_word(alias, 0x12345678);
        let a = action(&format!("DMVM F{index} HFFFFFFFF D20E0"));
        assert_eq!(
            a.native_words().unwrap()[1..5],
            [word0, word1, 0x0080, 0x0080]
        );
        let mut observed = BTreeSet::new();
        a.execute(&mut m, true, &mut observed);
        assert_eq!(m.double_word(d("D20")) as u32, 0x12345678);
        assert!(
            observed.contains(&alias)
                && observed.contains(&Device {
                    index: low + 1,
                    ..alias
                })
        );
        m.set_double_word(alias, 0x00ff00ff);
        m.set_double_word(d("D0"), 0x12345678);
        m.set_double_word(d("D20"), 0xaabbccddu32 as i32);
        action(&format!("DMVM D0E0 F{index} D20E0")).execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(m.double_word(d("D20")) as u32, 0xaa34cc78);
        action(&format!("DMVM D0E0 HFFFFFFFF F{index}")).execute(
            &mut m,
            true,
            &mut BTreeSet::new(),
        );
        assert_eq!(m.double_word(alias) as u32, 0x12345678);
    }
}

#[test]
fn swap_direct_indices_keep_word_f_and_double_e_storage_contract() {
    let mut m = Memory::default();
    m.set_word(d("D1183"), 0x1234);
    m.set_word(d("D1184"), 77);
    action("SWAP F1").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.word(d("D1183")) as u16, 0x3412);
    assert_eq!(m.word(d("D1184")), 77);
    m.set_double_word(d("D1182"), 0x12345678);
    action("DSWAP E1").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.double_word(d("D1182")) as u32, 0x34127856);
    assert!(IndexedSwapMaskAction::parse("DSWAP", &args("F1")).is_err());
}

#[test]
fn wide_counter_boundaries_and_timer_pair_endpoints_use_reader_and_writer_width() {
    for (base, offset, effective) in [
        ("C198", 0, 198),
        ("C199", 0, 199),
        ("C199", 1, 200),
        ("C200", -1, 199),
        ("C200", 0, 200),
        ("C254", 1, 255),
    ] {
        let mut m = Memory::default();
        m.set_word(d("D1028"), offset);
        m.set_double_word(d("C200"), 0x11223344);
        m.set_double_word(d("C201"), 0x23456789);
        let target = d(&format!("C{effective}"));
        m.set_double_word(target, 0x12345678);
        let mut observed = BTreeSet::new();
        action(&format!("DSWAP {base}E0")).execute(&mut m, true, &mut observed);
        assert_eq!(
            m.double_word(target) as u32,
            if effective == 199 { 0x7856 } else { 0x34127856 }
        );
        if effective == 199 {
            assert_eq!(m.double_word(d("C200")), 0x11223344);
            assert!(!observed.contains(&d("C200")));
        }
        if effective == 200 {
            assert_eq!(m.double_word(d("C201")), 0x23456789);
        }
    }
    let mut m = Memory::default();
    m.set_word(d("D1028"), -1);
    m.set_double_word(d("T254"), 0x12345678);
    action("DSWAP T255E0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.double_word(d("T254")) as u32, 0x34127856);
    m.set_word(d("D1028"), 0);
    assert!(
        action("DSWAP T255E0")
            .prepare(&m, &mut BTreeSet::new())
            .is_err()
    );
    assert!(
        action("SWAP T255E0")
            .prepare(&m, &mut BTreeSet::new())
            .is_ok()
    );
}

#[test]
fn shifted_group_overlap_and_self_indexed_numeric_pair_are_snapshotted() {
    let mut m = Memory::default();
    put_group(&mut m, d("M80"), 32, 0xaabbccdd);
    put_group(&mut m, d("M112"), 32, 0x11223344);
    action("DMVM K8M80E0 H00FF00FF K8M84F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(packed(&m, d("M84"), 32), 0x4abbbcdd);
    assert_eq!(packed(&m, d("M80"), 4), 0xd);
    let mut m = Memory::default();
    m.set_word(d("D1028"), 1);
    m.set_word(d("D1029"), 0x1234);
    m.set_word(d("D1030"), 88);
    action("DSWAP D1027E0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(
        [m.word(d("D1028")) as u16, m.word(d("D1029")) as u16],
        [0x0100, 0x3412]
    );
    assert_eq!(m.word(d("D1030")), 88);
    let mut m = Memory::default();
    m.set_word(d("D1028"), 1);
    m.set_double_word(d("D0"), 0x12345678);
    m.set_word(d("D1030"), 88);
    action("DMVM D0F0 HFFFFFFFF D1027E0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.double_word(d("D1028")) as u32, 0x12345678);
    assert_eq!(m.word(d("D1030")), 88);
}

#[test]
fn all_pulse_forms_skip_invalid_off_held_addresses_and_rearm_once() {
    for (name, wide, swap) in [
        ("SWAPP", false, true),
        ("DSWAPP", true, true),
        ("MVMP", false, false),
        ("DMVMP", true, false),
    ] {
        let body = if swap {
            format!("{name} D20E0")
        } else {
            format!("{name} D0F0 K-1 D20E0")
        };
        let p = compile_il(&format!("LD X0\n{body}\nEND")).unwrap();
        let mut sim = Simulator::default();
        sim.memory.set_word(d("D1028"), -30000);
        seed(&mut sim.memory);
        run(&mut sim, &p);
        assert!(sim.memory.execution_limitations.is_empty());
        assert!(!sim.observed_devices.contains(&d("D1028")));
        sim.memory.set_word(d("D1028"), 0);
        sim.memory.set_double_word(d("D20"), 0x12345678);
        sim.memory.set_double_word(d("D0"), 0x12345678);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &p);
        let first = sim.memory.double_word(d("D20"));
        sim.memory.set_word(d("D1028"), -30000);
        sim.observed_devices.clear();
        run(&mut sim, &p);
        assert_eq!(sim.memory.double_word(d("D20")), first);
        assert!(!sim.observed_devices.contains(&d("D1028")));
        assert!(sim.memory.execution_limitations.is_empty());
        sim.memory.set_input(d("X0"), false);
        run(&mut sim, &p);
        sim.memory.set_word(d("D1028"), 0);
        sim.memory.set_input(d("X0"), true);
        sim.memory.set_double_word(d("D0"), 0xaabbccddu32 as i32);
        run(&mut sim, &p);
        assert_eq!(
            sim.memory.double_word(d("D20")) as u32,
            if swap {
                0x12345678
            } else if wide {
                0xaabbccdd
            } else {
                0x1234ccdd
            }
        );
        assert_eq!(flags(&sim.memory), [true; 3]);
    }
}

#[test]
fn source_mask_old_target_order_is_visible_on_structural_qualification_failures() {
    let a = action("DMVM D0E0 D10E1 D20F7");
    assert_eq!(
        a.modifier_devices(),
        vec![d("D1028"), d("D1182"), d("D1195")]
    );
    for stage in 0..3 {
        let mut m = Memory::default();
        let aliases = [d("D1028"), d("D1182"), d("D1195")];
        m.set_word(aliases[stage], -30000);
        seed(&mut m);
        let mut observed = BTreeSet::new();
        a.execute(&mut m, true, &mut observed);
        for (i, alias) in aliases.into_iter().enumerate() {
            assert_eq!(observed.contains(&alias), i <= stage);
        }
        assert_eq!(flags(&m), [true; 3]);
        assert!(!m.system_bit(1067));
        assert!(!m.execution_limitations.is_empty());
    }
    let mut m = Memory::default();
    m.set_word(d("D1028"), -30000);
    let mut observed = BTreeSet::new();
    action("MVM D0E0 K0 D20F7").execute(&mut m, true, &mut observed);
    assert!(!m.execution_limitations.is_empty());
    assert!(!observed.contains(&d("D1195")));
}

#[test]
fn same_scan_loop_changes_and_off_snapshot_preparation_are_separate() {
    let p = compile_il("LD X0\nMOV K0 E0\nFOR K3\nDSWAP D100E0\nADD E0 K2 E0\nNEXT\nEND").unwrap();
    let mut sim = enabled();
    for address in ["D100", "D102", "D104"] {
        sim.memory.set_double_word(d(address), 0x12345678);
    }
    run(&mut sim, &p);
    for address in ["D100", "D102", "D104"] {
        assert_eq!(sim.memory.double_word(d(address)) as u32, 0x34127856);
    }
    let mut m = Memory::default();
    m.set_double_word(d("D20"), 0x12345678);
    let a = action("DSWAP D20E0");
    let mut observed = BTreeSet::new();
    a.prepare(&m, &mut observed).unwrap();
    assert_eq!(m.double_word(d("D20")) as u32, 0x12345678);
    observed.clear();
    a.execute(&mut m, false, &mut observed);
    assert!(observed.is_empty());
}

#[test]
fn model_limits_low_m_qualification_and_monitoring_use_complete_actual_spans() {
    for last in [4999, 9999] {
        let mut m = Memory::default();
        m.data_register_limit = Some(last);
        m.set_word(d("D1028"), -1);
        let a = action(&format!("DSWAP D{last}E0"));
        assert!(a.prepare(&m, &mut BTreeSet::new()).is_ok());
        m.set_word(d("D1028"), 0);
        assert!(a.prepare(&m, &mut BTreeSet::new()).is_err());
        assert!(
            action(&format!("SWAP D{last}E0"))
                .prepare(&m, &mut BTreeSet::new())
                .is_ok()
        );
    }
    let mut m = Memory::default();
    m.relay_limit = Some(511);
    assert!(
        action("SWAP K1M508E0")
            .prepare(&m, &mut BTreeSet::new())
            .is_ok()
    );
    assert!(
        action("SWAP K1M509E0")
            .prepare(&m, &mut BTreeSet::new())
            .is_err()
    );
    for target in ["K1M1284E0", "K2M1532E0", "K2S1020E0", "K1Y377E0"] {
        assert!(
            action(&format!("DSWAP {target}"))
                .prepare(&Memory::default(), &mut BTreeSet::new())
                .is_err()
        );
    }
    let a = action("DMVM D0E0 K-1 D20F0");
    let mut m = Memory::default();
    m.set_word(d("D1028"), -30000);
    m.set_word(d("D1029"), 2);
    m.set_double_word(d("D22"), 123);
    let mut observed = BTreeSet::new();
    a.execute(&mut m, true, &mut observed);
    assert!(!observed.contains(&d("D1029")));
    let mut ui = BTreeSet::new();
    a.observed_devices(&m, &mut ui);
    assert!(ui.contains(&d("D22")) && ui.contains(&d("D23")));
    assert_eq!(a.display_value(&m), "123");
}

#[test]
fn raw_flag_alias_writes_and_ordinary_arithmetic_parity_do_not_add_hidden_flags() {
    for (name, raw, expected) in [("SWAP", 0x1234, 0x3412), ("DSWAP", 0x12345678, 0x34127856)] {
        let mut ordinary = Memory::default();
        ordinary.set_double_word(d("D20"), raw);
        seed(&mut ordinary);
        let mut indexed = ordinary.clone();
        AdditionalAction::parse(name, &args("D20"))
            .unwrap()
            .execute(&mut ordinary);
        action(&format!("{name} D20E0")).execute(&mut indexed, true, &mut BTreeSet::new());
        assert_eq!(ordinary.double_word(d("D20")), expected);
        assert_eq!(indexed.double_word(d("D20")), expected);
        assert_eq!(flags(&indexed), [true; 3]);
    }
    let mut m = Memory::default();
    m.set_word(d("D0"), 0xa);
    seed(&mut m);
    action("MVM D0E0 K15 K1M1019F0").execute(&mut m, true, &mut BTreeSet::new());
    assert!(!m.system_bit(1019));
    assert_eq!(flags(&m), [true, false, true]);
    seed(&mut m);
    action("SWAP K1M1019E0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(flags(&m), [false; 3]);
    assert!(!m.system_bit(1067));
}
