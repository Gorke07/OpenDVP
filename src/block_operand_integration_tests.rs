use crate::plc::{Area, Device, Memory, Program, ScanStatus, Simulator, compile_il};

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn put(m: &mut Memory, raw: &str, bits: u16, value: u32) {
    let base = d(raw);
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
fn get(m: &Memory, raw: &str, bits: u16) -> u32 {
    let base = d(raw);
    (0..bits).fold(0, |v, n| {
        v | (u32::from(m.bit(Device {
            area: base.area,
            index: base.index + n,
        })) << n)
    })
}
fn active() -> Simulator {
    let mut s = Simulator::default();
    s.memory.set_input(d("X200"), true);
    s.memory.set_system(1020, true);
    s.memory.set_system(1022, true);
    s
}
fn scan(s: &mut Simulator, p: &Program) {
    s.scan(p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
}

#[test]
fn fill_group_stride_partial_neighbors_and_pulse_rearm() {
    for op in ["FMOV", "DFMOV"] {
        for pulse in ["", "P"] {
            for target in ["Y200", "M128", "S128"] {
                let body = format!("{op}{pulse} K4M0 K2{target} K3");
                let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
                let mut s = active();
                put(&mut s.memory, "M0", 16, 0x1234);
                put(&mut s.memory, target, 32, 0xaa555555);
                scan(&mut s, &p);
                assert_eq!(get(&s.memory, target, 32), 0xaa343434, "{body}");
                put(&mut s.memory, "M0", 16, 0x5678);
                scan(&mut s, &p);
                let held = if pulse.is_empty() {
                    0xaa787878
                } else {
                    0xaa343434
                };
                assert_eq!(get(&s.memory, target, 32), held, "{body}");
                s.memory.set_input(d("X200"), false);
                scan(&mut s, &p);
                assert_eq!(get(&s.memory, target, 32), held);
                s.memory.set_input(d("X200"), true);
                scan(&mut s, &p);
                assert_eq!(get(&s.memory, target, 32), 0xaa787878);
                assert_eq!(
                    [1020, 1021, 1022].map(|n| s.memory.system_bit(n)),
                    [true, false, true]
                );
            }
        }
    }
}

#[test]
fn fill_reads_source_once_and_supports_index_aliases_and_short_zero_extension() {
    for (body, expected) in [
        ("FMOV K1M0 K4M0 K2", 0x00050005),
        ("DFMOV K1M0 K8M0 K1", 5),
        ("DFMOV E1 K3M0 K2", 0xab567567),
        ("FMOV F1 K2M0 K3", 0xab787878),
    ] {
        let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
        let mut s = active();
        put(&mut s.memory, "M0", 32, 0xabcdef05);
        s.memory.set_double_word(d("D1182"), 0x00780567);
        scan(&mut s, &p);
        assert_eq!(get(&s.memory, "M0", 32), expected, "{body}");
    }
    for target in ["D9996", "T252", "C200", "C254"] {
        let p = compile_il(&format!("LD X200\nDFMOV E1 {target} K2\nEND")).unwrap();
        let mut s = active();
        s.memory.set_double_word(d("D1182"), 0x12345678);
        scan(&mut s, &p);
        assert_eq!(s.memory.double_word(d(target)), 0x12345678);
        let first = d(target);
        let next = Device {
            area: first.area,
            index: first.index + if first.area == Area::C { 1 } else { 2 },
        };
        assert_eq!(s.memory.double_word(next), 0x12345678);
    }
}

#[test]
fn block_move_group_overlap_is_forward_and_mixed_widths_convert_each_element() {
    for (body, expected) in [
        ("BMOV K1M0 K1M4 K3", 0x1111),
        ("BMOV K1M4 K1M0 K3", 0x4432),
        ("BMOV K1M0 K1M0 K4", 0x4321),
    ] {
        let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
        let mut s = active();
        put(&mut s.memory, "M0", 16, 0x4321);
        scan(&mut s, &p);
        assert_eq!(get(&s.memory, "M0", 16), expected, "{body}");
    }
    let p = compile_il("LD X200\nBMOV K2M0 D0 K3\nBMOV D10 K2M128 K3\nEND").unwrap();
    let mut s = active();
    put(&mut s.memory, "M0", 24, 0x563412);
    put(&mut s.memory, "M128", 32, 0xaa000000);
    for (name, v) in [("D10", 0x1234), ("D11", 0xabcd_u16 as i16), ("D12", 0x5678)] {
        s.memory.set_word(d(name), v);
    }
    scan(&mut s, &p);
    assert_eq!(
        ["D0", "D1", "D2"].map(|name| s.memory.word(d(name))),
        [0x12, 0x34, 0x56]
    );
    assert_eq!(get(&s.memory, "M128", 32), 0xaa78cd34);
}

#[test]
fn dynamic_block_count_is_sampled_once_and_implicit_bits_stay_observed() {
    let p = compile_il("LD X200\nBMOV K1M0 K1M128 D0\nMOV K0 D0\nEND").unwrap();
    let mut s = active();
    put(&mut s.memory, "M0", 16, 0x4321);
    s.memory.set_word(d("D0"), 3);
    scan(&mut s, &p);
    assert_eq!(get(&s.memory, "M128", 16), 0x321);
    for n in 128..140 {
        assert!(s.observed_devices.contains(&d(&format!("M{n}"))));
    }
    assert!(!s.observed_devices.contains(&d("M140")));
    s.memory.set_input(d("X200"), false);
    scan(&mut s, &p);
    assert!(s.observed_devices.contains(&d("M139")));
    let p = compile_il("LD X200\nBMOV K1M0 D0 D0\nEND").unwrap();
    s.memory.set_input(d("X200"), true);
    s.memory.set_word(d("D0"), 3);
    scan(&mut s, &p);
    assert_eq!(
        ["D0", "D1", "D2"].map(|name| s.memory.word(d(name))),
        [1, 2, 3]
    );
}

#[test]
fn all_readable_group_areas_and_writable_areas_are_supported() {
    for source in ["X0", "Y0", "M0", "S0"] {
        for target in ["Y200", "M128", "S128"] {
            for suffix in ["", "P"] {
                let body = format!("BMOV{suffix} K1{source} K1{target} K3");
                let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
                let mut s = active();
                put(&mut s.memory, source, 12, 0x321);
                put(&mut s.memory, target, 16, 0xa555);
                scan(&mut s, &p);
                assert_eq!(get(&s.memory, target, 16), 0xa321, "{body}");
            }
        }
    }
}

#[test]
fn block_and_fill_matrices_reject_invalid_forms_and_span_crossings() {
    for body in [
        "BMOV E1 D0 K1",
        "BMOV D0 E1 K1",
        "BMOV K1 D0 K1",
        "BMOV K1M0 K2M128 K1",
        "BMOV D0 K1X0 K1",
        "BMOV D0 D10 E1",
        "BMOV D0 D10 K1M0",
        "BMOV K1M4092 D0 K2",
        "FMOV E1 E2 K1",
        "FMOV F1 K1X0 K1",
        "FMOV D0 K4M4080 K2",
        "FMOV D0 K4M992 K2",
        "DFMOV F1 D0 K1",
        "DFMOV E1 E2 K1",
        "DFMOV E1 K8S1000 K1",
        "DFMOV E1 D0 E2",
        "FMOV E1 K1S0 K0",
        "DFMOV E1 K1S0 K513",
    ] {
        assert!(
            compile_il(&format!("LD X200\n{body}\nEND")).is_err(),
            "{body}"
        );
    }
    for body in [
        "BMOV K1X370 K1Y370 K2",
        "BMOV K1M4092 D0 K1",
        "BMOV K4M0 D0 D1",
        "FMOV F7 K4S1008 K1",
        "DFMOV E7 K8S992 K1",
        "DFMOV K1 C255 K1",
        "DFMOV K1 C199 K1",
        "FMOV K1 K1M2000 K512",
    ] {
        assert!(
            compile_il(&format!("LD X200\n{body}\nEND")).is_ok(),
            "{body}"
        );
    }
}

#[test]
fn block_labels_roundtrip_and_track_full_constant_ranges() {
    for body in [
        "BMOV K2X0 K2Y200 K3",
        "BMOVP D0 K4S976 K3",
        "FMOV E7 K3M128 K3",
        "FMOVP F7 T253 K3",
        "DFMOV E7 C253 K3",
        "DFMOVP K2X0 K8S928 K3",
    ] {
        let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
        let label = p.networks[0].actions[0].label();
        let restored = compile_il(&format!("LD X200\n{label}\nEND")).unwrap();
        assert_eq!(p.networks[0].actions, restored.networks[0].actions);
        assert_eq!(p.devices, restored.devices);
    }
    let p = compile_il("LD X200\nFMOV E7 K3M128 K3\nEND").unwrap();
    for n in 128..164 {
        assert!(p.devices.contains(&d(&format!("M{n}"))));
    }
    assert!(!p.devices.contains(&d("M164")));
    assert!(p.devices.contains(&d("D1194")));
}

#[test]
fn shared_scratch_bytes_follow_native_group_and_numeric_paths() {
    for (body, expected) in [
        ("BMOV K1M0 K1M128 K1", 0x1234),
        ("BMOV D0 K1M128 K1", 0x8634),
        ("BMOV K1M0 T0 K1", 0x1282),
        ("BMOV D0 C0 K1", 0x8684),
        ("FMOV K1 K1M128 K1", 0x1282),
        ("FMOV K1 K2M128 K1", 0x1284),
        ("FMOV K1 K3M128 K1", 0x1286),
        ("FMOV K1 K4M128 K1", 0x1280),
        ("DFMOV K1 K1M128 K1", 0x1234),
        ("DFMOV K1 D0 K1", 0x8634),
    ] {
        let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
        let mut s = active();
        s.memory.native_numeric_scratch = Some(0x1234);
        scan(&mut s, &p);
        assert_eq!(
            s.memory.native_numeric_scratch,
            Some(expected as u16 as i16),
            "{body}"
        );
    }
}

#[test]
fn dynamic_group_terminal_boundary_has_distinct_source_and_destination_effects() {
    for (body, count, expected_word, expected_bits, error) in [
        ("BMOV K1S1020 D20 D0", 1, 5, 5, false),
        ("BMOV K1S1020 D20 D0", 2, 9, 5, true),
        ("BMOV D20 K1S1020 D0", 1, 9, 9, false),
        ("BMOV D20 K1S1020 D0", 2, 9, 9, true),
    ] {
        let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
        let mut s = active();
        s.memory.set_word(d("D0"), count);
        s.memory.set_word(d("D20"), 9);
        put(&mut s.memory, "S1020", 4, 5);
        scan(&mut s, &p);
        assert_eq!(
            s.memory.word(d("D20")),
            expected_word,
            "{body} count{count}"
        );
        assert_eq!(get(&s.memory, "S1020", 4), expected_bits);
        assert_eq!(s.memory.system_bit(1067), error);
        if error {
            assert_eq!(s.memory.word(d("D1067")), 0xe1a);
            assert_eq!(s.memory.word(d("D1068")), 8);
        }
    }
}
