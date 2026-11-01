//! Literal conversion and operand limits from the Delta manual, pages 3-64/65.
use crate::plc::{Area, Device, Memory, Program, ScanStatus, Simulator, compile_il};

fn dev(s: &str) -> Device {
    s.parse().unwrap()
}
fn put(m: &mut Memory, base: &str, bits: u16, value: u32) {
    let base = dev(base);
    for n in 0..bits {
        let d = Device {
            area: base.area,
            index: base.index + n,
        };
        if d.area == Area::X {
            m.set_input(d, value & (1 << n) != 0);
        } else {
            m.set_internal(d, value & (1 << n) != 0);
        }
    }
}
fn get(m: &Memory, base: &str, bits: u16) -> u32 {
    let base = dev(base);
    (0..bits).fold(0, |v, n| {
        v | (u32::from(m.bit(Device {
            area: base.area,
            index: base.index + n,
        })) << n)
    })
}
fn scan(s: &mut Simulator, p: &Program) {
    s.scan(p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
}
fn enabled() -> Simulator {
    let mut s = Simulator::default();
    s.memory.set_input(dev("X200"), true);
    s.memory.set_system(1020, true);
    s.memory.set_system(1022, true);
    s
}
fn flags(s: &Simulator) -> [bool; 3] {
    [1020, 1021, 1022].map(|n| s.memory.system_bit(n))
}

#[test]
fn all_conversion_forms_cover_group_areas_partial_targets_and_pulse_rearm() {
    for (op, initial, changed, first, second) in [
        ("BCD", 1234, 5678, 0x34, 0x78),
        ("DBCD", 1234, 5678, 0x34, 0x78),
        ("BIN", 0x1234, 0x5678, 0xd2, 0x2e),
        ("DBIN", 0x1234, 0x5678, 0xd2, 0x2e),
    ] {
        for suffix in ["", "P"] {
            for source in ["X0", "Y0", "M0", "S0"] {
                for target in ["Y200", "M128", "S128"] {
                    let body = format!("{op}{suffix} K4{source} K2{target}");
                    let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
                    let mut s = enabled();
                    put(&mut s.memory, source, 16, initial);
                    put(&mut s.memory, target, 16, 0xaa55);
                    scan(&mut s, &p);
                    assert_eq!(get(&s.memory, target, 16), 0xaa00 | first, "{body}");
                    put(&mut s.memory, source, 16, changed);
                    scan(&mut s, &p);
                    let held = if suffix.is_empty() { second } else { first };
                    assert_eq!(get(&s.memory, target, 16), 0xaa00 | held, "{body}");
                    s.memory.set_input(dev("X200"), false);
                    scan(&mut s, &p);
                    assert_eq!(get(&s.memory, target, 16), 0xaa00 | held, "{body}");
                    s.memory.set_input(dev("X200"), true);
                    scan(&mut s, &p);
                    assert_eq!(get(&s.memory, target, 16), 0xaa00 | second, "{body}");
                    assert_eq!(flags(&s), [true, false, true], "{body}");
                    assert!(!s.memory.system_bit(1067), "{body}");
                }
            }
        }
    }
}

#[test]
fn binary_conversion_validates_full_source_before_truncating_target() {
    for (op, bits, valid, packed, invalid) in [
        ("BCD", 16, 9999, 0x9999, 10000),
        ("DBCD", 32, 99_999_999, 0x9999_9999, 100_000_000),
    ] {
        for input in [valid, invalid, u32::MAX] {
            let p = compile_il(&format!("LD X200\n{op} K{}M0 K1M128\nEND", bits / 4)).unwrap();
            let mut s = enabled();
            put(&mut s.memory, "M0", bits, input);
            put(&mut s.memory, "M128", 16, 0xabcd);
            scan(&mut s, &p);
            let expected = if input == valid {
                0xabc0 | (packed & 15)
            } else {
                0xabcd
            };
            assert_eq!(get(&s.memory, "M128", 16), expected, "{op} {input}");
            assert_eq!(s.memory.system_bit(1067), input != valid);
            if input != valid {
                assert_eq!(s.memory.word(dev("D1067")), 0xe18);
            }
            assert_eq!(flags(&s), [true, false, true]);
        }
    }
}

#[test]
fn every_invalid_bcd_nibble_holds_target_and_valid_maximum_converts() {
    for (op, digits, valid, expected) in [
        ("BIN", 4, 0x9999, 9999),
        ("DBIN", 8, 0x9999_9999, 99_999_999),
    ] {
        let p = compile_il(&format!("LD X200\n{op} K{}M0 K1M128\nEND", digits)).unwrap();
        for position in 0..digits {
            for digit in 10..=15 {
                let mut s = enabled();
                let input = (valid & !(15 << (position * 4))) | (digit << (position * 4));
                put(&mut s.memory, "M0", digits * 4, input);
                put(&mut s.memory, "M128", 16, 0xabcd);
                scan(&mut s, &p);
                assert_eq!(get(&s.memory, "M128", 16), 0xabcd, "{op} {input:08x}");
                assert_eq!(s.memory.word(dev("D1067")), 0xe18);
                assert!(s.memory.system_bit(1067) && s.memory.system_bit(1068));
            }
        }
        let mut s = enabled();
        put(&mut s.memory, "M0", digits * 4, valid);
        put(&mut s.memory, "M128", 16, 0xabcd);
        scan(&mut s, &p);
        assert_eq!(get(&s.memory, "M128", 16), 0xabc0 | (expected & 15));
        assert!(!s.memory.system_bit(1067));
    }
}

#[test]
fn invalid_pulse_waits_for_rearm_and_error_code_pc_remain_latched() {
    let p = compile_il("LD X200\nBCDP K4M0 F1\nEND").unwrap();
    let mut s = enabled();
    put(&mut s.memory, "M0", 16, 10000);
    s.memory.set_word(dev("D1183"), 0x55aa);
    scan(&mut s, &p);
    assert_eq!(s.memory.word(dev("D1183")), 0x55aa);
    assert_eq!(s.memory.word(dev("D1067")), 0xe18);
    assert_eq!(s.memory.word(dev("D1068")), 2); // LD is one step, operand is action+1.
    put(&mut s.memory, "M0", 16, 1234);
    scan(&mut s, &p);
    assert_eq!(s.memory.word(dev("D1183")), 0x55aa);
    s.memory.set_input(dev("X200"), false);
    scan(&mut s, &p);
    s.memory.set_input(dev("X200"), true);
    scan(&mut s, &p);
    assert_eq!(s.memory.word(dev("D1183")), 0x1234);
    assert_eq!(s.memory.word(dev("D1067")), 0xe18);
    assert_eq!(s.memory.word(dev("D1068")), 2);
}

#[test]
fn aliases_and_overlapping_groups_snapshot_sources() {
    let p = compile_il("LD X200\nBCD E1 F1\nBIN F1 E2\nDBCD E3 E3\nDBIN E3 E4\nBCD K4M0 K4M4\nEND")
        .unwrap();
    let mut s = enabled();
    s.memory.set_word(dev("D1182"), 1234);
    s.memory.set_double_word(dev("D1186"), 12_345_678);
    put(&mut s.memory, "M0", 32, 0xabcd_04d2);
    scan(&mut s, &p);
    assert_eq!(s.memory.word(dev("D1183")), 0x1234);
    assert_eq!(s.memory.word(dev("D1184")), 1234);
    assert_eq!(s.memory.double_word(dev("D1186")), 0x1234_5678);
    assert_eq!(s.memory.double_word(dev("D1188")), 12_345_678);
    assert_eq!(get(&s.memory, "M0", 32), 0xabc1_2342);
}

#[test]
fn short_groups_zero_extend_while_full_word_bcd_is_signed() {
    for (body, input, bits, expected, error) in [
        ("BCD K4M0 D0", 65535, 16, 77, true),
        ("DBCD K4M0 D0", 65535, 16, 0x0006_5535, false),
        ("BCD K1M0 D0", 15, 4, 0x15, false),
        ("DBCD K1M0 D0", 15, 4, 0x15, false),
        ("BIN K1M0 D0", 15, 4, 77, true),
        ("DBIN K1M0 D0", 9, 4, 9, false),
    ] {
        let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
        let mut s = enabled();
        put(&mut s.memory, "M0", bits, input);
        s.memory.set_double_word(dev("D0"), 77);
        scan(&mut s, &p);
        assert_eq!(s.memory.double_word(dev("D0")), expected, "{body}");
        assert_eq!(s.memory.system_bit(1067), error, "{body}");
    }
}

#[test]
fn conversion_timer_counter_destinations_respect_storage_width() {
    for target in ["T255", "C199", "D9999"] {
        let p = compile_il(&format!(
            "LD X200\nBCD E1 {target}\nBIN {target} {target}\nEND"
        ))
        .unwrap();
        let mut s = enabled();
        s.memory.set_word(dev("D1182"), 1234);
        scan(&mut s, &p);
        assert_eq!(s.memory.word(dev(target)), 1234, "{target}");
    }
    for target in ["T254", "C200", "C255", "D9998"] {
        let p = compile_il(&format!(
            "LD X200\nDBCD E1 {target}\nDBIN {target} {target}\nEND"
        ))
        .unwrap();
        let mut s = enabled();
        s.memory.set_double_word(dev("D1182"), 12_345_678);
        s.memory.set_double_word(dev("C201"), 987_654_321);
        scan(&mut s, &p);
        assert_eq!(s.memory.double_word(dev(target)), 12_345_678, "{target}");
        assert_eq!(s.memory.double_word(dev("C201")), 987_654_321);
        assert!(!p.devices.contains(&dev("C201")));
    }
}

#[test]
fn conversion_rejects_constants_readonly_groups_and_invalid_widths() {
    for body in [
        "BCD K1 D0",
        "BIN H1234 D0",
        "DBCD K1 E1",
        "DBIN H12345678 E1",
        "BCD X0 D0",
        "BIN M0 D0",
        "BCD D0 K1X0",
        "BIN D0 M0",
        "BCD D0 K1",
        "DBCD F0 D0",
        "DBIN E0 F1",
        "BCD K5M0 E0",
        "DBCD K9M0 E0",
        "BCD D0 C200",
        "DBIN E0 D9999",
        "DBCD E0 T255",
        "BCD D0 K4M998",
        "DBCD K8S993 E0",
        "BCD E8 F0",
    ] {
        assert!(
            compile_il(&format!("LD X200\n{body}\nEND")).is_err(),
            "{body}"
        );
    }
    for body in [
        "BCD K4X360 K4Y360",
        "BIN F7 E7",
        "DBCD E7 C255",
        "DBIN C200 E7",
        "DBCD K8M4064 K8S992",
        "BIN K4M4080 K4S1008",
        "DBIN C199 D9998",
    ] {
        assert!(
            compile_il(&format!("LD X200\n{body}\nEND")).is_ok(),
            "{body}"
        );
    }
}

#[test]
fn labels_roundtrip_and_tracking_includes_all_bits_and_index_pair() {
    for body in [
        "BCD K4X360 F7",
        "BCDP F7 K4Y360",
        "BIN E7 K4S1008",
        "BINP F0 E0",
        "DBCD K8M4064 E7",
        "DBCDP E7 C255",
        "DBIN E7 K8Y340",
        "DBINP C200 D9998",
    ] {
        let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
        let label = p.networks[0].actions[0].label();
        let restored = compile_il(&format!("LD X200\n{label}\nEND")).unwrap();
        assert_eq!(p.networks[0].actions, restored.networks[0].actions);
        assert_eq!(p.devices, restored.devices);
    }
    let p = compile_il("LD X200\nDBCD K8M4064 E7\nDBIN E7 K8Y340\nEND").unwrap();
    for index in 4064..=4095 {
        assert!(p.devices.contains(&dev(&format!("M{index}"))));
    }
    for index in 224..=255 {
        assert!(p.devices.contains(&dev(&format!("Y{index:o}"))));
    }
    for d in ["D1194", "D1195"] {
        assert!(p.devices.contains(&dev(d)));
    }
}
