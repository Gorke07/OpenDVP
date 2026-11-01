//! DRAFT admission tests: independent literals and compiled visits for the
//! twelve ABS/Gray forms. These tests require the future wrapper integration
//! and its two shared integer seams; no current production file is changed.
use crate::bit_tools::BitAction;
use crate::indexed_abs_gray::{IndexedAbsGrayAction, PreparedAbsGray};
use crate::plc::{Area, Device, Memory, Program, ScanStatus, Simulator, compile_il};
use std::collections::BTreeSet;

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn args(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(str::to_owned).collect()
}
fn action(raw: &str) -> IndexedAbsGrayAction {
    let words = args(raw);
    IndexedAbsGrayAction::parse(&words[0], &words[1..]).unwrap()
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
fn flags(m: &Memory) -> [bool; 3] {
    [1020, 1021, 1022].map(|n| m.system_bit(n))
}
fn seed(m: &mut Memory) {
    for (flag, value) in [(1020, true), (1021, false), (1022, true)] {
        m.set_system(flag, value);
    }
}
fn packed(m: &Memory, base: Device, width: u16) -> u32 {
    (0..width).fold(0, |v, bit| {
        v | (u32::from(m.bit(Device {
            index: base.index + bit,
            ..base
        })) << bit)
    })
}
fn put_group(m: &mut Memory, base: Device, width: u16, value: u32) {
    for bit in 0..width {
        let device = Device {
            index: base.index + bit,
            ..base
        };
        let on = value & (1u32 << bit) != 0;
        if device.area == Area::X {
            m.set_input(device, on);
        } else {
            m.set_internal(device, on);
        }
    }
}

#[test]
fn twelve_literal_headers_original_fields_lengths_and_following_markers() {
    for (name, header, wide, input, expected) in [
        ("ABS", 0xbe00, false, -17, 17),
        ("ABSP", 0xbe10, false, -17, 17),
        ("DABS", 0xbf00, true, -17, 17),
        ("DABSP", 0xbf10, true, -17, 17),
        ("GRY", 0x7201, false, 6513, 0x15c9),
        ("GRYP", 0x7211, false, 6513, 0x15c9),
        ("DGRY", 0x7301, true, 6513, 0x15c9),
        ("DGRYP", 0x7311, true, 6513, 0x15c9),
        ("GBIN", 0x7401, false, 0x15c9, 6513),
        ("GBINP", 0x7411, false, 0x15c9, 6513),
        ("DGBIN", 0x7501, true, 0x15c9, 6513),
        ("DGBINP", 0x7511, true, 0x15c9, 6513),
    ] {
        let abs = name.contains("ABS");
        let text = if abs {
            format!("{name} D20F7")
        } else {
            format!("{name} D10E1 D20F7")
        };
        let a = action(&text);
        let mut literal = vec![header];
        if !abs {
            literal.extend([0x14e6, 0x8086]);
            if wide {
                literal.extend([0x0080, 0x0080]);
            }
        }
        literal.extend([0x28f6, 0x8089]);
        if wide {
            literal.extend([0x0080, 0x0080]);
        }
        assert_eq!(a.native_words().unwrap(), literal, "{text}");
        assert_eq!(a.label(), text);
        assert_eq!(a.source_count(), 1);
        assert_eq!(a.wide(), wide);
        let p = compile_il(&format!("LD X0\n{text}\nMOV K7 D300\nOUT Y0\nEND")).unwrap();
        assert!(p.requires_vm());
        let encoded = crate::es2_encoder::encode(&p).unwrap();
        let pc = usize::from(encoded.action_pc[0]);
        assert_eq!(&encoded.words[pc..pc + literal.len()], literal);
        assert_eq!(usize::from(encoded.action_pc[1]), pc + literal.len());
        let mut sim = enabled();
        sim.memory.set_word(d("D1182"), 1);
        sim.memory.set_word(d("D1195"), 2);
        let source = if abs { d("D22") } else { d("D11") };
        if wide {
            sim.memory.set_double_word(source, input);
        } else {
            sim.memory.set_word(source, input as i16);
        }
        seed(&mut sim.memory);
        run(&mut sim, &p);
        assert_eq!(
            if wide {
                sim.memory.double_word(d("D22"))
            } else {
                i32::from(sim.memory.word(d("D22")))
            },
            expected
        );
        assert_eq!(flags(&sim.memory), [true, false, true]);
        assert_eq!(sim.memory.word(d("D300")), 7);
        assert!(sim.memory.bit(d("Y0")));
        assert!(!p.devices.contains(&d("D20")));
        assert!(sim.observed_devices.contains(&d("D22")));
    }
    assert_eq!(
        action("GRY K4X0E1 K1M128F7").native_words().unwrap(),
        [0x7201, 0x00e4, 0x2488, 0x80f4, 0xe882]
    );
    assert_eq!(
        action("DABS K8M128F7").native_words().unwrap(),
        [0xbf00, 0x80f4, 0xe890, 0x0080, 0x0080]
    );
}

#[test]
fn every_modifier_bank_is_a_signed_word_even_for_double_values() {
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
        for (name, input, expected) in [
            ("ABS", -2, 2),
            ("DABS", -2, 2),
            ("GRY", 2, 3),
            ("DGRY", 2, 3),
            ("GBIN", 3, 2),
            ("DGBIN", 3, 2),
        ] {
            let mut m = Memory::default();
            m.set_word(d(alias), -2);
            let neighbor = Device {
                index: d(alias).index + 1,
                ..d(alias)
            };
            m.set_word(neighbor, i16::MIN);
            let abs = name.contains("ABS");
            let source = if abs { d("D18") } else { d("D8") };
            if name.starts_with('D') {
                m.set_double_word(source, input);
            } else {
                m.set_word(source, input as i16);
            }
            let a = action(&if abs {
                format!("{name} D20{suffix}")
            } else {
                format!("{name} D10{suffix} D20{suffix}")
            });
            let mut observed = BTreeSet::new();
            a.execute(&mut m, true, &mut observed);
            assert_eq!(m.word(d("D18")), expected);
            assert_eq!(a.modifier_devices(), vec![d(alias)]);
            assert!(observed.contains(&d(alias)));
            assert!(!observed.contains(&neighbor), "{name} {suffix}");
        }
    }
}

#[test]
fn exact_role_matrices_do_not_open_wide_f_bare_bits_or_input_writes() {
    for name in ["ABS", "ABSP", "DABS", "DABSP"] {
        let wide = name.starts_with('D');
        for target in [
            "K1Y20E0", "K3M80F7", "K1S80E1", "T255E0", "C199F0", "D9999E1", "E1",
        ] {
            assert!(
                IndexedAbsGrayAction::parse(name, &args(target)).is_ok(),
                "{name} {target}"
            );
        }
        for target in [
            "K1X0E0", "X0E0", "Y0E0", "M0", "K1", "HFFFF", "E1E0", "D0F8",
        ] {
            assert!(
                IndexedAbsGrayAction::parse(name, &args(target)).is_err(),
                "{name} {target}"
            );
        }
        assert_eq!(
            IndexedAbsGrayAction::parse(name, &args("F1")).is_ok(),
            !wide
        );
        assert_eq!(
            IndexedAbsGrayAction::parse(name, &args("C200E0")).is_ok(),
            wide
        );
        assert_eq!(
            IndexedAbsGrayAction::parse(name, &args("K8M0E0")).is_ok(),
            wide
        );
    }
    for name in [
        "GRY", "GRYP", "DGRY", "DGRYP", "GBIN", "GBINP", "DGBIN", "DGBINP",
    ] {
        let wide = name.starts_with('D');
        for source in [
            "K2", "H1234", "K1X0E0", "K1Y0F7", "K1M0E1", "K1S0F1", "T255E0", "C199F0", "D9999E0",
            "E1",
        ] {
            assert!(IndexedAbsGrayAction::parse(name, &args(&format!("{source} D20E2"))).is_ok());
        }
        for raw in [
            "X0E0 D20E0",
            "K2E0 D20E0",
            "E1F0 D20E0",
            "D0E0 K1X0",
            "D0E0 M0E0",
            "D0E0 H1",
            "D0E8 D20",
        ] {
            assert!(
                IndexedAbsGrayAction::parse(name, &args(raw)).is_err(),
                "{name} {raw}"
            );
        }
        for raw in ["F1 D20E0", "D0E0 F7"] {
            assert_eq!(IndexedAbsGrayAction::parse(name, &args(raw)).is_ok(), !wide);
        }
    }
    assert!(!IndexedAbsGrayAction::supports("ABSPP"));
    assert!(!IndexedAbsGrayAction::supports("BSET"));
    assert!(!IndexedAbsGrayAction::has_modifiers(&args("HEF E1")));
    assert!(IndexedAbsGrayAction::has_modifiers(&args("K1M0F7")));
}

#[test]
fn absolute_signed_minima_and_short_unsigned_groups_keep_flags() {
    for (name, input, expected) in [
        ("ABS", 0, 0),
        ("ABS", -1, 1),
        ("ABS", 1, 1),
        ("ABS", -32768, -32768),
        ("DABS", 0, 0),
        ("DABS", -1, 1),
        ("DABS", -32768, 32768),
        ("DABS", i32::MIN, i32::MIN),
    ] {
        let mut m = Memory::default();
        if name == "ABS" {
            m.set_word(d("D20"), input as i16);
        } else {
            m.set_double_word(d("D20"), input);
        }
        seed(&mut m);
        action(&format!("{name} D20E0")).execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(
            if name == "ABS" {
                i32::from(m.word(d("D20")))
            } else {
                m.double_word(d("D20"))
            },
            expected
        );
        assert_eq!(flags(&m), [true, false, true]);
    }
    for (name, digits, raw, expected) in [
        ("ABS", 1, 15, 15),
        ("ABS", 4, 0xffff, 1),
        ("DABS", 4, 0xffff, 0xffff),
        ("DABS", 8, 0x80000000, 0x80000000),
    ] {
        let mut m = Memory::default();
        m.set_word(d("D1028"), 1);
        put_group(&mut m, d("M81"), digits * 4, raw);
        m.set_internal(d("M80"), true);
        let end = Device {
            index: 81 + digits * 4,
            ..d("M80")
        };
        m.set_internal(end, true);
        action(&format!("{name} K{digits}M80E0")).execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(packed(&m, d("M81"), digits * 4), expected);
        assert!(m.bit(d("M80")) && m.bit(end));
    }
}

#[test]
fn gray_short_sources_zero_extend_and_outputs_truncate_only_declared_bits() {
    // Hand-counted all-one binary and Gray vectors at each nibble width.
    for (digits, input, encoded, decoded) in [
        (1, 0xf, 0x8, 0xa),
        (2, 0xff, 0x80, 0xaa),
        (3, 0xfff, 0x800, 0xaaa),
        (4, 0xffff, 0x8000, 0xaaaa),
        (5, 0xfffff, 0x80000, 0xaaaaa),
        (6, 0xffffff, 0x800000, 0xaaaaaa),
        (7, 0xfffffff, 0x8000000, 0xaaaaaaa),
    ] {
        for source_base in ["X20", "Y20", "M80", "S80"] {
            for (target_base, name, expected) in [
                ("Y200", "DGRY", encoded),
                ("M200", "DGBIN", decoded),
                ("S200", "DGRY", encoded),
            ] {
                let mut m = Memory::default();
                m.set_word(d("D1028"), 1);
                let source = Device {
                    index: d(source_base).index + 1,
                    ..d(source_base)
                };
                let target = Device {
                    index: d(target_base).index + 1,
                    ..d(target_base)
                };
                put_group(&mut m, source, digits * 4, input);
                let end = Device {
                    index: target.index + digits * 4,
                    ..target
                };
                m.set_internal(d(target_base), true);
                m.set_internal(end, true);
                action(&format!(
                    "{name} K{digits}{source_base}E0 K{digits}{target_base}E0"
                ))
                .execute(&mut m, true, &mut BTreeSet::new());
                assert_eq!(
                    packed(&m, target, digits * 4),
                    expected,
                    "{name} {source_base} width {digits}"
                );
                assert!(m.bit(d(target_base)) && m.bit(end));
            }
        }
    }
    for (name, raw, result) in [
        ("GRY", 32767, 0x4000),
        ("GBIN", 32767, 0x5555),
        ("DGRY", i32::MAX, 0x40000000),
        ("DGBIN", i32::MAX, 0x55555555),
    ] {
        let mut m = Memory::default();
        m.set_double_word(d("D0"), raw);
        action(&format!("{name} D0E0 D20F0")).execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(m.double_word(d("D20")), result);
    }
    let mut m = Memory::default();
    m.set_word(d("D0"), 6513);
    m.set_internal(d("M84"), true);
    action("GRY D0E0 K1M80F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(packed(&m, d("M80"), 4), 9); // 0x15c9 truncated after conversion.
    assert!(m.bit(d("M84")));
}

#[test]
fn negative_gray_exits_before_bad_target_and_preserves_flags_and_sticky_pc() {
    for name in [
        "GRY", "GRYP", "DGRY", "DGRYP", "GBIN", "GBINP", "DGBIN", "DGBINP",
    ] {
        let p = compile_il(&format!("LD X0\n{name} D10E0 K1M80F7\nMOV K9 D300\nEND")).unwrap();
        let mut sim = enabled();
        sim.memory.set_word(d("D1028"), 1);
        sim.memory.set_word(d("D1195"), -30000);
        sim.memory.set_double_word(d("D11"), -1);
        put_group(&mut sim.memory, d("M80"), 4, 5);
        seed(&mut sim.memory);
        run(&mut sim, &p);
        assert_eq!(sim.memory.word(d("D1067")), 0xe1a);
        assert_eq!(
            sim.memory.word(d("D1068")),
            (p.native_action_pc[0] + 1) as i16
        );
        assert_eq!(packed(&sim.memory, d("M80"), 4), 5);
        assert!(sim.memory.execution_limitations.is_empty());
        assert!(!sim.observed_devices.contains(&d("D1195")));
        assert_eq!(flags(&sim.memory), [true, false, true]);
        assert_eq!(sim.memory.word(d("D300")), 9);
        // Explicitly existing error state remains first-error/first-PC latched.
        let mut m = Memory::default();
        m.operation_error(0xe19, 5);
        m.set_double_word(d("D11"), -1);
        m.set_word(d("D1028"), 1);
        m.set_word(d("D1195"), -30000);
        let a = action(&format!("{name} D10E0 D100F7"));
        assert_eq!(a.semantic_error(&m).unwrap().unwrap().code, 0xe1a);
        a.execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(m.word(d("D1067")), 0xe19);
        assert_eq!(m.word(d("D1068")), 5);
    }
    // Full-width negativity is not erased by a short destination.
    let mut m = Memory::default();
    put_group(&mut m, d("M0"), 32, 0x80000009);
    put_group(&mut m, d("M80"), 4, 5);
    action("DGBIN K8M0E0 K1M80F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(packed(&m, d("M80"), 4), 5);
    assert_eq!(m.word(d("D1067")), 0xe1a);
}

#[test]
fn shifted_group_overlap_and_self_indexed_pairs_use_frozen_sources_and_cursor() {
    let mut m = Memory::default();
    put_group(&mut m, d("M80"), 16, 6513);
    m.set_internal(d("M100"), true);
    action("GRY K4M80E0 K4M84F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(packed(&m, d("M84"), 16), 0x15c9);
    assert_eq!(packed(&m, d("M80"), 4), 1);
    assert!(m.bit(d("M100")));
    let mut m = Memory::default();
    m.set_double_word(d("D1028"), -1); // E0=-1, same source low word.
    m.set_word(d("D1030"), 88);
    action("DABS D1029E0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.double_word(d("D1028")), 1);
    assert_eq!(m.word(d("D1030")), 88);
    let mut m = Memory::default();
    m.set_word(d("D1028"), 1);
    m.set_double_word(d("D20"), 0x12345678);
    m.set_word(d("D1030"), 88);
    action("DGRY D20F0 D1027E0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.double_word(d("D1028")) as u32, 0x1b2e7d44);
    assert_eq!(m.word(d("D1030")), 88);
}

#[test]
fn direct_e_pairs_and_word_f_keep_canonical_aliases_in_mixed_instructions() {
    let mut m = Memory::default();
    m.set_word(d("D1183"), 2);
    m.set_word(d("D1184"), 77);
    action("GRY F1 D20E0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.word(d("D20")), 3);
    assert_eq!(m.word(d("D1184")), 77);
    m.set_word(d("D30"), 3);
    action("GBIN D30E0 F7").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.word(d("D1195")), 2);
    m.set_double_word(d("D1182"), 6513);
    action("DGRY E1 D40F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.double_word(d("D40")), 0x15c9);
    m.set_double_word(d("D50"), 0x15c9);
    action("DGBIN D50F0 E2").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.double_word(d("D1184")), 6513);
    assert_eq!(
        action("GRY F1 D20E0").native_words().unwrap(),
        [0x7201, 0x6e86, 0x0180, 0x28e6, 0x0086]
    );
    assert!(IndexedAbsGrayAction::parse("DGRY", &args("F1 D20E0")).is_err());
}

#[test]
fn counter_reclassification_and_timer_limits_follow_effective_storage_width() {
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
        m.set_double_word(d("C200"), 123456);
        m.set_double_word(d("C201"), 654321);
        let target = d(&format!("C{effective}"));
        m.set_double_word(target, -1);
        let mut observed = BTreeSet::new();
        action(&format!("DABS {base}E0")).execute(&mut m, true, &mut observed);
        assert_eq!(
            m.double_word(target),
            if effective == 199 { 65535 } else { 1 }
        );
        if effective == 199 {
            assert_eq!(m.double_word(d("C200")), 123456);
            assert!(!observed.contains(&d("C200")));
            action(&format!("DGRY {base}E0 D20F0")).execute(&mut m, true, &mut BTreeSet::new());
            assert_eq!(m.double_word(d("D20")), 32768);
        }
        if effective == 200 {
            assert_eq!(m.double_word(d("C201")), 654321);
        }
    }
    let mut m = Memory::default();
    m.set_word(d("D1028"), -1);
    m.set_double_word(d("C200"), 123456);
    action("DGRY K2147483647 C200E0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.word(d("C199")), 0); // 0x40000000 result, only low16 stored.
    assert_eq!(m.double_word(d("C200")), 123456);
    m.set_word(d("D1029"), 1);
    action("DGBIN K2147483647 C254F0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.double_word(d("C255")), 0x55555555);
    m.set_double_word(d("T254"), -2);
    action("DABS T255E0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.double_word(d("T254")), 2);
    m.set_word(d("D1028"), 0);
    assert!(
        action("DABS T255E0")
            .prepare(&m, &mut BTreeSet::new())
            .is_err()
    );
    assert!(
        action("ABS T255E0")
            .prepare(&m, &mut BTreeSet::new())
            .is_ok()
    );
    m.set_word(d("D1028"), 1);
    assert!(
        action("ABS C199E0")
            .prepare(&m, &mut BTreeSet::new())
            .is_err()
    );
}

#[test]
fn complete_spans_model_endpoints_and_low_m_mapping_fail_as_limitations() {
    for last in [4999, 9999] {
        let mut m = Memory::default();
        m.data_register_limit = Some(last);
        m.set_word(d("D1028"), -1);
        for name in ["DABS", "DGRY", "DGBIN"] {
            let a = action(&if name == "DABS" {
                format!("{name} D{last}E0")
            } else {
                format!("{name} D{last}E0 D{last}E0")
            });
            assert!(a.prepare(&m, &mut BTreeSet::new()).is_ok());
            m.set_word(d("D1028"), 0);
            assert!(a.prepare(&m, &mut BTreeSet::new()).is_err());
            m.set_word(d("D1028"), -1);
        }
        m.set_word(d("D1028"), 0);
        assert!(
            action(&format!("ABS D{last}E0"))
                .prepare(&m, &mut BTreeSet::new())
                .is_ok()
        );
        if last == 4999 {
            assert!(
                action("DGRY D4999 D20E0")
                    .prepare(&m, &mut BTreeSet::new())
                    .is_err()
            );
            assert!(
                action("DGRY D20E0 D4999")
                    .prepare(&m, &mut BTreeSet::new())
                    .is_err()
            );
        }
    }
    let mut m = Memory::default();
    m.relay_limit = Some(511);
    assert!(
        action("ABS K1M508E0")
            .prepare(&m, &mut BTreeSet::new())
            .is_ok()
    );
    assert!(
        action("ABS K1M509E0")
            .prepare(&m, &mut BTreeSet::new())
            .is_err()
    );
    for target in ["K1M1284E0", "K2M1532E0", "K2S1020E0", "K1Y377E0", "K1X0E0"] {
        if let Ok(a) = IndexedAbsGrayAction::parse("DGRY", &args(&format!("K1 {target}"))) {
            let mut m = Memory::default();
            a.execute(&mut m, true, &mut BTreeSet::new());
            assert!(!m.execution_limitations.is_empty(), "{target}");
            assert!(!m.system_bit(1067));
        } else {
            assert_eq!(target, "K1X0E0");
        }
    }
}

#[test]
fn all_six_pulse_forms_skip_bad_off_and_held_addresses_then_rearm() {
    for name in ["ABSP", "DABSP", "GRYP", "DGRYP", "GBINP", "DGBINP"] {
        let abs = name.contains("ABS");
        let encode = name.contains("GRY");
        let body = if abs {
            format!("{name} D20E0")
        } else {
            format!("{name} D10E0 D20F0")
        };
        let p = compile_il(&format!("LD X0\n{body}\nEND")).unwrap();
        let mut sim = Simulator::default();
        sim.memory.set_word(d("D1028"), -30000);
        run(&mut sim, &p);
        assert!(sim.memory.execution_limitations.is_empty());
        assert!(!sim.observed_devices.contains(&d("D1028")));
        sim.memory.set_word(d("D1028"), 0);
        let first_source = if abs {
            -2
        } else if encode {
            2
        } else {
            3
        };
        sim.memory
            .set_double_word(if abs { d("D20") } else { d("D10") }, first_source);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &p);
        let first = sim.memory.word(d("D20"));
        assert_eq!(first, if encode { 3 } else { 2 });
        sim.memory.set_word(d("D1028"), -30000);
        sim.observed_devices.clear();
        run(&mut sim, &p);
        assert_eq!(sim.memory.word(d("D20")), first);
        assert!(!sim.observed_devices.contains(&d("D1028")));
        assert!(sim.memory.execution_limitations.is_empty());
        sim.memory.set_input(d("X0"), false);
        run(&mut sim, &p);
        sim.memory.set_word(d("D1028"), 0);
        sim.memory.set_double_word(
            if abs { d("D20") } else { d("D10") },
            if abs {
                -4
            } else if encode {
                4
            } else {
                6
            },
        );
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &p);
        assert_eq!(sim.memory.word(d("D20")), if encode { 6 } else { 4 });
    }
}

#[test]
fn same_scan_loop_visits_and_stack_control_follow_live_offsets() {
    let p = compile_il("LD X0\nMOV K0 E0\nFOR K3\nDGRY D10E0 D100E0\nADD E0 K2 E0\nNEXT\nMOV K0 E0\nMPS\nAND X1\nABS D20E0\nMPP\nGBIN D30E0 D40F0\nEND").unwrap();
    let mut sim = enabled();
    for (base, input) in [("D10", 2), ("D12", 4), ("D14", 7)] {
        sim.memory.set_double_word(d(base), input);
    }
    sim.memory.set_word(d("D20"), -3);
    sim.memory.set_word(d("D30"), 3);
    run(&mut sim, &p);
    assert_eq!(
        ["D100", "D102", "D104"].map(|raw| sim.memory.double_word(d(raw))),
        [3, 6, 4]
    );
    assert_eq!(sim.memory.word(d("D20")), -3);
    assert_eq!(sim.memory.word(d("D40")), 2);
    assert!(sim.observed_devices.contains(&d("D104")) && sim.observed_devices.contains(&d("D105")));
    assert!(!sim.observed_devices.contains(&d("D20")));
}

#[test]
fn pure_preparation_actual_accesses_and_held_result_monitoring_are_distinct() {
    let mut m = Memory::default();
    m.set_word(d("D1028"), 1);
    m.set_word(d("D1029"), 2);
    m.set_double_word(d("D11"), 6513);
    m.set_double_word(d("D22"), 77);
    seed(&mut m);
    let a = action("DGRY D10E0 D20F0");
    let mut observed = BTreeSet::new();
    assert!(matches!(
        a.prepare(&m, &mut observed).unwrap(),
        PreparedAbsGray::Ready(_)
    ));
    assert_eq!(m.double_word(d("D22")), 77);
    assert_eq!(flags(&m), [true, false, true]);
    assert_eq!(
        observed,
        BTreeSet::from([
            d("D1028"),
            d("D1029"),
            d("D11"),
            d("D12"),
            d("D22"),
            d("D23")
        ])
    );
    observed.clear();
    a.execute(&mut m, false, &mut observed);
    assert!(observed.is_empty());
    m.set_double_word(d("D11"), -1);
    assert!(matches!(
        a.prepare(&m, &mut observed).unwrap(),
        PreparedAbsGray::SemanticError {
            code: 0xe1a,
            offset: 1
        }
    ));
    assert!(!observed.contains(&d("D1029")) && !observed.contains(&d("D22")));
    let mut ui = BTreeSet::new();
    a.observed_devices(&m, &mut ui);
    assert!(ui.contains(&d("D22")) && ui.contains(&d("D23")));
    assert_eq!(a.display_value(&m), "77");
    assert_eq!(a.dest(), d("D20"));
    assert!(a.resolved_source(1, &m).is_err());
    m.set_word(d("D1028"), -30000);
    observed.clear();
    a.execute(&mut m, true, &mut observed);
    assert_eq!(observed, BTreeSet::from([d("D1028")]));
    assert!(!m.system_bit(1067));
    assert!(!m.execution_limitations.is_empty());
}

#[test]
fn ordinary_algorithms_match_independent_vectors_and_explicit_flag_aliases_write() {
    for (name, raw, expected) in [
        ("ABS", -123, 123),
        ("DABS", -32768, 32768),
        ("GRY", 6513, 0x15c9),
        ("DGRY", 0x12345678, 0x1b2e7d44),
        ("GBIN", 0x15c9, 6513),
        ("DGBIN", 0x1b2e7d44, 0x12345678),
    ] {
        let mut ordinary = Memory::default();
        let abs = name.contains("ABS");
        let source = if abs { d("D20") } else { d("D10") };
        if name.starts_with('D') {
            ordinary.set_double_word(source, raw);
        } else {
            ordinary.set_word(source, raw as i16);
        }
        seed(&mut ordinary);
        let mut indexed = ordinary.clone();
        BitAction::parse(name, &args(if abs { "D20" } else { "D10 D20" }))
            .unwrap()
            .execute(&mut ordinary, true);
        action(&if abs {
            format!("{name} D20E0")
        } else {
            format!("{name} D10E0 D20F0")
        })
        .execute(&mut indexed, true, &mut BTreeSet::new());
        assert_eq!(
            if name.starts_with('D') {
                ordinary.double_word(d("D20"))
            } else {
                i32::from(ordinary.word(d("D20")))
            },
            expected
        );
        assert_eq!(indexed.word(d("D20")), ordinary.word(d("D20")));
        assert_eq!(indexed.word(d("D21")), ordinary.word(d("D21")));
        assert_eq!(flags(&indexed), [true, false, true]);
    }
    for (text, expected_bits, expected_flags) in [
        ("GRY K8 K1M1019E0", 12, [false, true, true]),
        ("GBIN K4 K1M1019E0", 7, [true, true, false]),
    ] {
        let mut m = Memory::default();
        seed(&mut m);
        action(text).execute(&mut m, true, &mut BTreeSet::new());
        assert_eq!(packed(&m, d("M1019"), 4), expected_bits);
        assert_eq!(flags(&m), expected_flags);
    }
}
