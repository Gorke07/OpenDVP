//! PDF213/215/220/221 operands with literal results and retained ALU rules.
//! Local operand checks do not assert new physical/native simulator parity.
use crate::plc::{Area, Device, Memory, Program, ScanStatus, Simulator, compile_il};

fn dev(s: &str) -> Device {
    s.parse().unwrap()
}
fn put(m: &mut Memory, base: &str, bits: u16, value: u32) {
    let base = dev(base);
    for n in 0..bits {
        let bit = Device {
            area: base.area,
            index: base.index + n,
        };
        if bit.area == Area::X {
            m.set_input(bit, value & (1 << n) != 0);
        } else {
            m.set_internal(bit, value & (1 << n) != 0);
        }
    }
}
fn get(m: &Memory, base: &str, bits: u16) -> u32 {
    let base = dev(base);
    (0..bits).fold(0, |value, n| {
        value
            | (u32::from(m.bit(Device {
                area: base.area,
                index: base.index + n,
            })) << n)
    })
}
fn scan(s: &mut Simulator, p: &Program) {
    s.scan(p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
}
fn flags(s: &Simulator) -> [bool; 3] {
    [1020, 1021, 1022].map(|n| s.memory.system_bit(n))
}

#[test]
fn add_sub_all_forms_read_each_bit_area_and_write_only_destination_group() {
    for prefix in ["", "D"] {
        let bits = if prefix.is_empty() { 16 } else { 32 };
        for op in ["ADD", "SUB"] {
            for pulse in ["", "P"] {
                for bases in [["X0", "X40"], ["Y0", "Y40"], ["M0", "M32"], ["S0", "S32"]] {
                    for target in ["Y200", "M128", "S128"] {
                        let body = format!(
                            "{prefix}{op}{pulse} K{}{} K{}{} K1{target}",
                            bits / 4,
                            bases[0],
                            bits / 4,
                            bases[1]
                        );
                        let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
                        let mut s = Simulator::default();
                        put(&mut s.memory, bases[0], bits, 13);
                        put(&mut s.memory, bases[1], bits, 4);
                        put(&mut s.memory, target, 8, 0xa5);
                        s.memory.set_input(dev("X200"), true);
                        scan(&mut s, &p);
                        let first = if op == "ADD" { 0xa1 } else { 0xa9 };
                        assert_eq!(get(&s.memory, target, 8), first, "{body}");
                        assert_eq!(flags(&s), [false; 3]);
                        put(&mut s.memory, bases[0], bits, 0);
                        scan(&mut s, &p);
                        let changed = if op == "ADD" { 0xa4 } else { 0xac };
                        let held = if pulse.is_empty() { changed } else { first };
                        assert_eq!(get(&s.memory, target, 8), held, "{body}");
                        s.memory.set_input(dev("X200"), false);
                        scan(&mut s, &p);
                        assert_eq!(get(&s.memory, target, 8), held, "{body}");
                        s.memory.set_input(dev("X200"), true);
                        scan(&mut s, &p);
                        assert_eq!(get(&s.memory, target, 8), changed, "{body}");
                    }
                }
            }
        }
    }
}

#[test]
fn inc_dec_all_forms_preserve_other_bits_flags_and_pulse_rearm() {
    for prefix in ["", "D"] {
        for op in ["INC", "DEC"] {
            for pulse in ["", "P"] {
                for target in ["Y200", "M128", "S128"] {
                    let body = format!("{prefix}{op}{pulse} K1{target}");
                    let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
                    let mut s = Simulator::default();
                    let increment = op == "INC";
                    put(
                        &mut s.memory,
                        target,
                        8,
                        if increment { 0xaf } else { 0xa0 },
                    );
                    for n in [1020, 1022] {
                        s.memory.set_system(n, true);
                    }
                    s.memory.set_input(dev("X200"), true);
                    scan(&mut s, &p);
                    let first = if increment { 0xa0 } else { 0xaf };
                    assert_eq!(get(&s.memory, target, 8), first, "{body}");
                    scan(&mut s, &p);
                    let second = if !pulse.is_empty() {
                        first
                    } else if increment {
                        0xa1
                    } else {
                        0xae
                    };
                    assert_eq!(get(&s.memory, target, 8), second, "{body}");
                    s.memory.set_input(dev("X200"), false);
                    scan(&mut s, &p);
                    assert_eq!(get(&s.memory, target, 8), second, "{body}");
                    s.memory.set_input(dev("X200"), true);
                    scan(&mut s, &p);
                    let third = if !pulse.is_empty() {
                        if increment { 0xa1 } else { 0xae }
                    } else if increment {
                        0xa2
                    } else {
                        0xad
                    };
                    assert_eq!(get(&s.memory, target, 8), third, "{body}");
                    assert_eq!(flags(&s), [true, false, true], "{body}");
                }
            }
        }
    }
}

#[test]
fn arithmetic_flags_follow_full_width_result_before_short_group_truncation() {
    for (instruction, expected_bits, expected_flags) in [
        ("ADD K15 K1 K1M128", 0, [false, false, false]),
        ("ADD K32767 K1 K1M128", 0, [true, false, true]),
        ("ADD K-32768 K-1 K1M128", 0, [true, true, false]),
        ("ADD K-32768 K-32768 K4M128", 0x8001, [false, true, false]),
        ("SUB K-32768 K1 K4M128", 0, [true, true, false]),
        ("SUB K0 K-32768 K4M128", 0x8000, [false, false, false]),
        ("DADD K65535 K1 K4M128", 0, [false, false, false]),
        ("DADD K2147483647 K1 K8M128", 0, [true, false, true]),
        ("DADD K-2147483648 K-1 K8M128", 0, [true, true, false]),
        (
            "DADD K-2147483648 K-2147483648 K8M128",
            0x8000_0001,
            [false, true, false],
        ),
        ("DSUB K2147483647 K-1 K8M128", 0, [true, false, true]),
        (
            "DSUB K0 K-2147483648 K8M128",
            0x8000_0000,
            [false, false, false],
        ),
    ] {
        let p = compile_il(&format!("LD X200\n{instruction}\nEND")).unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(dev("X200"), true);
        scan(&mut s, &p);
        assert_eq!(get(&s.memory, "M128", 32), expected_bits, "{instruction}");
        assert_eq!(flags(&s), expected_flags, "{instruction}");
    }
}

#[test]
fn direct_index_writes_alias_the_real_words_and_are_visible_in_the_same_scan() {
    let p = compile_il("LD X200\nADD E7 F7 E7\nSUB F7 E7 F7\nINC E7\nDEC F7\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(dev("X200"), true);
    s.memory.set_word(dev("D1194"), -3);
    s.memory.set_word(dev("D1195"), 2);
    scan(&mut s, &p);
    assert_eq!(s.memory.word(dev("D1194")), 0);
    assert_eq!(s.memory.word(dev("D1195")), 2);
    let p = compile_il("LD X200\nDADD E7 K65535 E7\nDINC E7\nDSUB E7 K1 E7\nDDEC E7\nEND").unwrap();
    scan(&mut s, &p);
    assert_eq!(s.memory.word(dev("D1194")), -2);
    assert_eq!(s.memory.word(dev("D1195")), 2);
    assert!(p.devices.contains(&dev("D1194")) && p.devices.contains(&dev("D1195")));
}

#[test]
fn overlapping_groups_snapshot_both_sources_before_any_write() {
    let p = compile_il("LD X200\nADD K4M0 K4M8 K4M4\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(dev("X200"), true);
    put(&mut s.memory, "M0", 32, 0x0001_2345);
    scan(&mut s, &p);
    assert_eq!(get(&s.memory, "M0", 32), 0x0002_4685);

    let p = compile_il("LD X200\nDSUB K8M0 K8M16 K8M8\nEND").unwrap();
    put(&mut s.memory, "M0", 32, 0x0001_2345);
    put(&mut s.memory, "M32", 32, 0x0001_0001);
    scan(&mut s, &p);
    assert_eq!(get(&s.memory, "M0", 32), 0x0023_4445);
    assert_eq!(get(&s.memory, "M32", 32), 0x0001_0000);
    assert_eq!(flags(&s), [false; 3]);
}

#[test]
fn numeric_register_targets_and_natural_counters_keep_their_storage_width() {
    for target in ["T0", "C0", "D0"] {
        let p = compile_il(&format!("LD X200\nADD K3 K4 {target}\nINC {target}\nEND")).unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(dev("X200"), true);
        scan(&mut s, &p);
        assert_eq!(s.memory.word(dev(target)), 8, "{target}");
    }
    let p = compile_il("LD X200\nDADD K8M0 K1 C200\nDINC C200\nDSUB C200 K1 C255\nDDEC C255\nEND")
        .unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(dev("X200"), true);
    put(&mut s.memory, "M0", 32, 4);
    s.memory.set_double_word(dev("C201"), 123456789);
    scan(&mut s, &p);
    assert_eq!(s.memory.double_word(dev("C200")), 6);
    assert_eq!(s.memory.double_word(dev("C255")), 4);
    assert_eq!(s.memory.double_word(dev("C201")), 123456789);
    assert!(!p.devices.contains(&dev("C201")));
}

#[test]
fn invalid_write_sources_and_group_spans_are_rejected() {
    for body in [
        "ADD K1 K2 K1X0",
        "DADD K1 K2 K8X0",
        "INC K1X0",
        "DEC K1",
        "DINC F0",
        "DSUB F7 K1 E0",
        "ADD E8 K1 E0",
        "ADD K1 K2 M0",
        "ADD K1 K2 C200",
        "INC C200",
        "ADD K1 K2 K4M4081",
        "DINC K8S993",
        "ADD K1 K2 K4M998",
        "ADD K1 K2 D0E8",
    ] {
        assert!(
            compile_il(&format!("LD X200\n{body}\nEND")).is_err(),
            "{body}"
        );
    }
    for body in [
        "ADD K4X360 F7 K4Y360",
        "DADD K8M4064 E7 K8S992",
        "INC K4M4080",
        "DDEC K8Y340",
        "ADD K1 K2 E0",
    ] {
        assert!(
            compile_il(&format!("LD X200\n{body}\nEND")).is_ok(),
            "{body}"
        );
    }
}

#[test]
fn arithmetic_labels_and_device_tracking_preserve_packed_and_index_targets() {
    for body in [
        "ADDP K1X7 F7 K4M128",
        "SUBP E7 K1Y7 F7",
        "INCP K1S1020",
        "DECP E7",
        "DADDP K8M4064 E7 K8Y340",
        "DSUBP C200 E7 E7",
        "DINCP K8S992",
        "DDECP C255",
    ] {
        let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
        let label = p.networks[0].actions[0].label();
        let restored = compile_il(&format!("LD X200\n{label}\nEND")).unwrap();
        assert_eq!(
            p.networks[0].actions, restored.networks[0].actions,
            "{body}"
        );
        assert_eq!(p.devices, restored.devices, "{body}");
    }
    let p = compile_il("LD X200\nDADD K8M4064 E7 K8Y340\nEND").unwrap();
    for index in 4064..=4095 {
        assert!(p.devices.contains(&dev(&format!("M{index}"))));
    }
    for index in 224..=255 {
        assert!(p.devices.contains(&dev(&format!("Y{index:o}"))));
    }
    assert!(p.devices.contains(&dev("D1194")) && p.devices.contains(&dev("D1195")));
}

#[test]
fn short_source_zero_extension_depends_on_instruction_width() {
    for (instruction, expected, expected_flags) in [
        ("ADD K1M0 K-16 D0", 0xffff, [false, false, false]),
        ("ADD K4M0 K1 D0", 0, [true, false, false]),
        ("DADD K4M0 K1 K4M32", 0, [false, false, false]),
    ] {
        let p = compile_il(&format!("LD X200\n{instruction}\nEND")).unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(dev("X200"), true);
        put(&mut s.memory, "M0", 16, 0xffff);
        scan(&mut s, &p);
        let actual = if instruction.starts_with('D') {
            get(&s.memory, "M32", 16)
        } else {
            u32::from(s.memory.word(dev("D0")) as u16)
        };
        assert_eq!(actual, expected, "{instruction}");
        assert_eq!(flags(&s), expected_flags, "{instruction}");
    }
}

#[test]
fn full_width_unary_wrap_preserves_flags_and_adjacent_index_pair() {
    for (instruction, base, bits, initial, expected) in [
        ("INC K4M0", "M0", 16, 0x7fff, 0x8000),
        ("DEC K4M0", "M0", 16, 0x8000, 0x7fff),
        ("DINC K8M0", "M0", 32, 0x7fff_ffff, 0x8000_0000),
        ("DDEC K8M0", "M0", 32, 0x8000_0000, 0x7fff_ffff),
    ] {
        let p = compile_il(&format!("LD X200\n{instruction}\nEND")).unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(dev("X200"), true);
        for n in [1020, 1021, 1022] {
            s.memory.set_system(n, true);
        }
        put(&mut s.memory, base, bits, initial);
        scan(&mut s, &p);
        assert_eq!(get(&s.memory, base, bits), expected, "{instruction}");
        assert_eq!(flags(&s), [true; 3]);
    }
    let p = compile_il("LD X200\nDINC E2\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(dev("X200"), true);
    s.memory.set_double_word(dev("D1184"), i32::MAX);
    s.memory.set_double_word(dev("D1186"), 123456789);
    s.memory.set_system(1021, true);
    scan(&mut s, &p);
    assert_eq!(s.memory.double_word(dev("D1184")), i32::MIN);
    assert_eq!(s.memory.double_word(dev("D1186")), 123456789);
    assert_eq!(flags(&s), [false, true, false]);
}
