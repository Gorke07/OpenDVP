//! Literal PDF192/196/222..229 scalar operand and storage-width expectations.
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
fn enabled() -> Simulator {
    let mut s = Simulator::default();
    s.memory.set_input(dev("X200"), true);
    s.memory.set_system(1020, true);
    s.memory.set_system(1022, true);
    s
}

#[test]
fn all_move_complement_logic_forms_cover_source_areas_targets_and_rearm() {
    for (word, double, first, changed) in [
        ("MOV", "DMOV", 13, 6),
        ("CML", "DCML", 2, 9),
        ("WAND", "DAND", 1, 2),
        ("WOR", "DOR", 15, 7),
        ("WXOR", "DXOR", 14, 5),
    ] {
        for op in [word, double] {
            for suffix in ["", "P"] {
                for source in ["X0", "Y0", "M0", "S0"] {
                    for target in ["Y200", "M128", "S128"] {
                        let second = if matches!(word, "MOV" | "CML") {
                            ""
                        } else {
                            " K3"
                        };
                        let body = format!("{op}{suffix} K1{source}{second} K1{target}");
                        let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
                        let mut s = enabled();
                        put(&mut s.memory, source, 4, 13);
                        put(&mut s.memory, target, 8, 0xa5);
                        scan(&mut s, &p);
                        assert_eq!(get(&s.memory, target, 8), 0xa0 | first, "{body}");
                        assert_eq!(flags(&s), [true, false, true], "{body}");
                        put(&mut s.memory, source, 4, 6);
                        scan(&mut s, &p);
                        let held = if suffix.is_empty() { changed } else { first };
                        assert_eq!(get(&s.memory, target, 8), 0xa0 | held, "{body}");
                        s.memory.set_input(dev("X200"), false);
                        scan(&mut s, &p);
                        assert_eq!(get(&s.memory, target, 8), 0xa0 | held, "{body}");
                        s.memory.set_input(dev("X200"), true);
                        scan(&mut s, &p);
                        assert_eq!(get(&s.memory, target, 8), 0xa0 | changed, "{body}");
                        assert_eq!(flags(&s), [true, false, true], "{body}");
                    }
                }
            }
        }
    }
}

#[test]
fn negation_all_forms_are_twos_complement_with_pulse_hold() {
    for op in ["NEG", "DNEG"] {
        for suffix in ["", "P"] {
            for target in ["Y200", "M128", "S128"] {
                let body = format!("{op}{suffix} K1{target}");
                let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
                let mut s = enabled();
                put(&mut s.memory, target, 8, 0xa1);
                scan(&mut s, &p);
                assert_eq!(get(&s.memory, target, 8), 0xaf, "{body}");
                scan(&mut s, &p);
                let held = if suffix.is_empty() { 0xa1 } else { 0xaf };
                assert_eq!(get(&s.memory, target, 8), held, "{body}");
                s.memory.set_input(dev("X200"), false);
                scan(&mut s, &p);
                assert_eq!(get(&s.memory, target, 8), held, "{body}");
                s.memory.set_input(dev("X200"), true);
                scan(&mut s, &p);
                assert_eq!(
                    get(&s.memory, target, 8),
                    if suffix.is_empty() { 0xaf } else { 0xa1 },
                    "{body}"
                );
                assert_eq!(flags(&s), [true, false, true]);
            }
        }
    }
}

#[test]
fn short_source_extends_before_complement_and_preserves_word_storage_width() {
    for (body, expected_low, expected_high) in [
        ("MOV K1M0 D0", 5, 0xaaaa),
        ("DMOV K1M0 D0", 5, 0),
        ("CML K1M0 D0", 0xfffa, 0xaaaa),
        ("DCML K1M0 D0", 0xfffa, 0xffff),
        ("MOV K4M0 D0", 0xfff5, 0xaaaa),
        ("DMOV K4M0 D0", 0xfff5, 0),
        ("DAND K4M0 HFFFF0000 D0", 0, 0),
        ("DOR K4M0 H80000000 D0", 0xfff5, 0x8000),
        ("DXOR K4M0 HFFFF0000 D0", 0xfff5, 0xffff),
    ] {
        let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
        let mut s = enabled();
        put(&mut s.memory, "M0", 16, 0xfff5);
        s.memory.set_word(dev("D0"), -1);
        s.memory.set_word(dev("D1"), 0xaaaa_u16 as i16);
        scan(&mut s, &p);
        assert_eq!(s.memory.word(dev("D0")) as u16, expected_low, "{body}");
        assert_eq!(s.memory.word(dev("D1")) as u16, expected_high, "{body}");
        assert_eq!(flags(&s), [true, false, true], "{body}");
    }
    let p = compile_il("LD X200\nMOV K1M0 K4M0\nEND").unwrap();
    let mut s = enabled();
    put(&mut s.memory, "M0", 32, 0x1234_abf5);
    scan(&mut s, &p);
    assert_eq!(get(&s.memory, "M0", 32), 0x1234_0005);
    let p = compile_il("LD X200\nDMOV K1M0 K8M0\nEND").unwrap();
    scan(&mut s, &p);
    assert_eq!(get(&s.memory, "M0", 32), 5);
}

#[test]
fn ef_and_d_aliases_stay_coherent_through_multiple_operations() {
    let p = compile_il("LD X200\nMOV K-1 E2\nMOV K1 F2\nCML E2 E3\nWAND F2 K3 F3\nDMOV E2 E4\nDCML E4 E5\nDXOR E4 E5 E6\nDNEG E6\nNEG F2\nEND").unwrap();
    let mut s = enabled();
    s.memory.set_double_word(dev("D1194"), 0x1234_5678);
    scan(&mut s, &p);
    for (alias, expected) in [
        ("D1184", -1),
        ("D1186", 65536),
        ("D1188", 131071),
        ("D1190", -131072),
        ("D1192", 1),
        ("D1194", 0x1234_5678),
    ] {
        assert_eq!(s.memory.double_word(dev(alias)), expected, "{alias}");
    }
    assert_eq!(flags(&s), [true, false, true]);
}

#[test]
fn overlapping_word_and_packed_destinations_snapshot_sources() {
    let p = compile_il("LD X200\nDMOV D0 D1\nEND").unwrap();
    let mut s = enabled();
    for (d, value) in [("D0", 0x1122), ("D1", 0x3344), ("D2", 0x5566)] {
        s.memory.set_word(dev(d), value);
    }
    scan(&mut s, &p);
    assert_eq!(s.memory.word(dev("D1")), 0x1122);
    assert_eq!(s.memory.word(dev("D2")), 0x3344);

    let p = compile_il("LD X200\nWOR K4M0 K4M8 K4M4\nEND").unwrap();
    put(&mut s.memory, "M0", 32, 0x0001_2345);
    scan(&mut s, &p);
    assert_eq!(get(&s.memory, "M0", 32), 0x0002_3675);

    let p = compile_il("LD X200\nDXOR K8M0 K8M16 K8M8\nEND").unwrap();
    put(&mut s.memory, "M0", 32, 0x89ab_cdef);
    put(&mut s.memory, "M32", 32, 0x4567_1234);
    scan(&mut s, &p);
    assert_eq!(get(&s.memory, "M0", 32), 0x9f44_44ef);
    assert_eq!(get(&s.memory, "M32", 32), 0x4567_129b);
    assert_eq!(flags(&s), [true, false, true]);
}

#[test]
fn numeric_timer_counter_destinations_keep_their_storage_widths() {
    for target in ["T255", "C199", "D9999"] {
        let p = compile_il(&format!("LD X200\nMOV K5 {target}\nCML {target} {target}\nWAND {target} H00FF {target}\nWOR {target} H0100 {target}\nWXOR {target} H00F0 {target}\nNEG {target}\nEND")).unwrap();
        let mut s = enabled();
        scan(&mut s, &p);
        assert_eq!(s.memory.word(dev(target)), -266, "{target}");
        assert_eq!(flags(&s), [true, false, true]);
    }
    for target in ["C200", "C255", "T254", "D9998"] {
        let p = compile_il(&format!("LD X200\nDMOV K65535 {target}\nDCML {target} {target}\nDXOR {target} H0000FFFF {target}\nDNEG {target}\nEND")).unwrap();
        let mut s = enabled();
        s.memory.set_double_word(dev("C201"), 123456789);
        scan(&mut s, &p);
        assert_eq!(s.memory.double_word(dev(target)), 1, "{target}");
        assert_eq!(s.memory.double_word(dev("C201")), 123456789);
        assert!(!p.devices.contains(&dev("C201")));
        assert_eq!(flags(&s), [true, false, true]);
    }
}

#[test]
fn invalid_destinations_and_widths_fail_but_valid_boundaries_compile() {
    for body in [
        "MOV K1 K1X0",
        "CML K1 M0",
        "WAND X0 K1 D0",
        "WOR K1 K2 K1",
        "WXOR K1 K2 C200",
        "NEG K1X0",
        "NEG K1",
        "NEG C200",
        "DMOV F0 D0",
        "DCML E0 F0",
        "DAND F7 K1 E0",
        "DOR E8 K1 D0",
        "DXOR E0 K1 T255",
        "DNEG D9999",
        "MOV K5M0 D0",
        "DMOV K9M0 D0",
        "MOV K1 K4M4081",
        "DMOV K1 K8S993",
        "CML K1 K4M998",
    ] {
        assert!(
            compile_il(&format!("LD X200\n{body}\nEND")).is_err(),
            "{body}"
        );
    }
    for body in [
        "MOV K4X360 K4Y360",
        "CML F7 E7",
        "NEG K4S1008",
        "DMOV K8M4064 K8Y340",
        "DCML E7 K8S992",
        "DAND E7 HFFFFFFFF C255",
        "DOR C200 E7 D9998",
        "DXOR E0 K8X340 T254",
        "DNEG C199",
    ] {
        assert!(
            compile_il(&format!("LD X200\n{body}\nEND")).is_ok(),
            "{body}"
        );
    }
}

#[test]
fn labels_and_tracking_include_entire_destinations_and_index_pairs() {
    for body in [
        "MOVP K1X7 F7",
        "CMLP E7 K4Y360",
        "WANDP K4M0 F7 K4S1008",
        "WORP F0 K1 E0",
        "WXORP K1M0 E7 F7",
        "NEGP K4M4080",
        "DMOVP K8M4064 E7",
        "DCMLP E7 K8Y340",
        "DANDP E7 HFFFFFFFF K8S992",
        "DORP C200 E7 D9998",
        "DXORP E0 K8X340 C255",
        "DNEGP C199",
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
    let p = compile_il("LD X200\nDMOV K8M4064 E7\nDCML E7 K8Y340\nEND").unwrap();
    for index in 4064..=4095 {
        assert!(p.devices.contains(&dev(&format!("M{index}"))));
    }
    for index in 224..=255 {
        assert!(p.devices.contains(&dev(&format!("Y{index:o}"))));
    }
    assert!(p.devices.contains(&dev("D1194")) && p.devices.contains(&dev("D1195")));
}

#[test]
fn full_width_negation_preserves_minimum_and_every_flag_pattern() {
    for (body, width, initial, expected) in [
        ("NEG K4M0", 16, 0x8000, 0x8000),
        ("DNEG K8M0", 32, 0x8000_0000, 0x8000_0000),
        ("NEG K4M0", 16, 5, 0xfffb),
        ("DNEG K8M0", 32, 5, 0xffff_fffb),
        ("NEG K4M0", 16, 0, 0),
        ("DNEG K8M0", 32, 0, 0),
    ] {
        let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
        for mask in 0..8 {
            let mut s = enabled();
            let pattern = [mask & 1 != 0, mask & 2 != 0, mask & 4 != 0];
            for (index, value) in pattern.into_iter().enumerate() {
                s.memory.set_system(1020 + index as u16, value);
            }
            put(&mut s.memory, "M0", width, initial);
            scan(&mut s, &p);
            assert_eq!(get(&s.memory, "M0", width), expected, "{body}");
            assert_eq!(flags(&s), pattern, "{body}");
        }
    }
}

#[test]
fn zero_and_nonzero_transfer_logic_results_preserve_all_flag_patterns() {
    for body in [
        "MOV K0 D0",
        "DMOV K0 D0",
        "CML HFFFF D0",
        "DCML HFFFFFFFF D0",
        "WAND K1 K2 D0",
        "DAND K1 K2 D0",
        "WOR K0 K0 D0",
        "DOR K0 K0 D0",
        "WXOR K1 K1 D0",
        "DXOR K1 K1 D0",
        "MOV K-1 D0",
        "DMOV K-1 D0",
        "CML K0 D0",
        "DCML K0 D0",
        "WAND K3 K1 D0",
        "DAND K3 K1 D0",
        "WOR K1 K2 D0",
        "DOR K1 K2 D0",
        "WXOR K1 K2 D0",
        "DXOR K1 K2 D0",
    ] {
        let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
        for mask in 0..8 {
            let mut s = enabled();
            let pattern = [mask & 1 != 0, mask & 2 != 0, mask & 4 != 0];
            for (index, value) in pattern.into_iter().enumerate() {
                s.memory.set_system(1020 + index as u16, value);
            }
            scan(&mut s, &p);
            assert_eq!(flags(&s), pattern, "{body}");
        }
    }
}
