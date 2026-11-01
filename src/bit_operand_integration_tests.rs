//! Independent value/bit oracles for the PDF operand matrices, not bytecode
//! roundtrips. Native differential fixtures separately qualify the encoder.
use crate::plc::{Area, Device, Memory, Program, ScanStatus, Simulator, compile_il};
fn d(s: &str) -> Device {
    s.parse().unwrap()
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
fn flags(s: &Simulator) -> [bool; 3] {
    [1020, 1021, 1022].map(|n| s.memory.system_bit(n))
}

#[test]
fn gray_all_forms_accept_each_source_area_and_preserve_partial_target_neighbors() {
    for (op, first, changed, out1, out2) in [
        ("GRY", 3, 4, 2, 6),
        ("DGRY", 3, 4, 2, 6),
        ("GBIN", 2, 6, 3, 4),
        ("DGBIN", 2, 6, 3, 4),
    ] {
        for suffix in ["", "P"] {
            for source in ["X0", "Y0", "M0", "S0"] {
                for target in ["Y200", "M128", "S128"] {
                    let body = format!("{op}{suffix} K1{source} K2{target}");
                    let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
                    let mut s = active();
                    put(&mut s.memory, source, 4, first);
                    put(&mut s.memory, target, 16, 0xaa55);
                    scan(&mut s, &p);
                    assert_eq!(get(&s.memory, target, 16), 0xaa00 | out1, "{body}");
                    put(&mut s.memory, source, 4, changed);
                    scan(&mut s, &p);
                    let held = if suffix.is_empty() { out2 } else { out1 };
                    assert_eq!(get(&s.memory, target, 16), 0xaa00 | held);
                    s.memory.set_input(d("X200"), false);
                    scan(&mut s, &p);
                    assert_eq!(get(&s.memory, target, 16), 0xaa00 | held);
                    s.memory.set_input(d("X200"), true);
                    scan(&mut s, &p);
                    assert_eq!(get(&s.memory, target, 16), 0xaa00 | out2);
                    assert_eq!(flags(&s), [true, false, true]);
                }
            }
        }
    }
}

#[test]
fn absolute_packed_signed_width_short_zero_extension_and_pulse_edges() {
    for (op, bits, negative, min) in [
        ("ABS", 16, 0xfffb, 0x8000),
        ("DABS", 32, 0xffff_fffb, 0x8000_0000),
    ] {
        for suffix in ["", "P"] {
            for target in ["Y200", "M128", "S128"] {
                let p = compile_il(&format!("LD X200\n{op}{suffix} K{}{target}\nEND", bits / 4))
                    .unwrap();
                let mut s = active();
                put(&mut s.memory, target, bits, negative);
                scan(&mut s, &p);
                assert_eq!(get(&s.memory, target, bits), 5);
                let minus_seven = if bits == 16 { 0xfff9 } else { 0xfffffff9 };
                put(&mut s.memory, target, bits, minus_seven);
                scan(&mut s, &p);
                assert_eq!(
                    get(&s.memory, target, bits),
                    if suffix.is_empty() { 7 } else { minus_seven }
                );
                s.memory.set_input(d("X200"), false);
                scan(&mut s, &p);
                put(&mut s.memory, target, bits, min);
                s.memory.set_input(d("X200"), true);
                scan(&mut s, &p);
                assert_eq!(get(&s.memory, target, bits), min);
                assert_eq!(flags(&s), [true, false, true]);
            }
        }
    }
    for op in ["ABS", "DABS"] {
        let p = compile_il(&format!("LD X200\n{op} K1M0\nEND")).unwrap();
        let mut s = active();
        put(&mut s.memory, "M0", 16, 0xaaff);
        scan(&mut s, &p);
        assert_eq!(get(&s.memory, "M0", 16), 0xaaff);
    }
}

#[test]
fn gray_validates_full_signed_source_and_preserves_error_target_and_latches() {
    for (op, bits, source) in [
        ("GRY", 16, 0x8000),
        ("GBIN", 16, 0xffff),
        ("DGRY", 32, 0x80000000),
        ("DGBIN", 32, 0xffffffff),
    ] {
        let p = compile_il(&format!("LD X200\n{op} K{}M0 K1M128\nEND", bits / 4)).unwrap();
        let mut s = active();
        put(&mut s.memory, "M0", bits, source);
        put(&mut s.memory, "M128", 16, 0xabcd);
        scan(&mut s, &p);
        assert_eq!(get(&s.memory, "M128", 16), 0xabcd);
        assert_eq!(s.memory.word(d("D1067")), 0xe1a);
        assert_eq!(s.memory.word(d("D1068")), 2);
        put(&mut s.memory, "M0", bits, 0);
        scan(&mut s, &p);
        assert_eq!(get(&s.memory, "M128", 16), 0xabc0);
        assert!(s.memory.system_bit(1067) && s.memory.system_bit(1068));
        assert_eq!(flags(&s), [true, false, true]);
    }
    let p = compile_il("LD X200\nDGRY K4M0 D0\nEND").unwrap();
    let mut s = active();
    put(&mut s.memory, "M0", 16, 0x8000);
    scan(&mut s, &p);
    assert_eq!(s.memory.double_word(d("D0")), 0xc000);
    assert!(!s.memory.system_bit(1067));
}

#[test]
fn direct_indices_and_overlapping_gray_groups_use_source_snapshots() {
    let p=compile_il("LD X200\nABS E1\nGRY E1 F1\nGBIN F1 E2\nDABS E3\nDGRY E3 E4\nDGBIN E4 E5\nGRY K4M0 K4M4\nEND").unwrap();
    let mut s = active();
    s.memory.set_word(d("D1182"), -6513);
    s.memory.set_double_word(d("D1186"), -65535);
    put(&mut s.memory, "M0", 32, 0xabc01971);
    scan(&mut s, &p);
    assert_eq!(s.memory.word(d("D1182")), 6513);
    assert_eq!(s.memory.word(d("D1183")), 0x15c9);
    assert_eq!(s.memory.word(d("D1184")), 6513);
    assert_eq!(s.memory.double_word(d("D1186")), 65535);
    assert_eq!(s.memory.double_word(d("D1188")), 0x8000);
    assert_eq!(s.memory.double_word(d("D1190")), 65535);
    assert_eq!(get(&s.memory, "M0", 32), 0xabc15c91);
}

#[test]
fn indexed_group_write_preserves_neighbors_and_bout_clears_on_false_power() {
    for prefix in ["", "D"] {
        for target in ["Y200", "M128", "S128"] {
            let p =
                compile_il(&format!("LD X200\n{prefix}BOUT K1{target} E1\nOUT Y0\nEND")).unwrap();
            let mut s = active();
            put(&mut s.memory, target, 16, 0xaaf0);
            s.memory.set_double_word(d("D1182"), 1);
            scan(&mut s, &p);
            assert_eq!(get(&s.memory, target, 16), 0xaaf2);
            assert!(s.memory.bit(d("Y0")));
            s.memory.set_input(d("X200"), false);
            scan(&mut s, &p);
            assert_eq!(get(&s.memory, target, 16), 0xaaf0);
            assert!(!s.memory.bit(d("Y0")));
            s.memory.set_double_word(d("D1182"), 15);
            s.memory.set_input(d("X200"), true);
            scan(&mut s, &p);
            assert_eq!(
                get(&s.memory, target, 16),
                0xaaf0,
                "bit outside declared group must not alter neighbors"
            );
            assert_eq!(flags(&s), [true, false, true]);
        }
    }
}

#[test]
fn indexed_group_index_wrap_alias_and_set_reset_hold() {
    for (prefix, bits, index) in [("", 16, 0xffff), ("D", 32, 0xffffffff)] {
        let p = compile_il(&format!(
            "LD X200\n{prefix}BSET K{}M128 K{}M0\nEND",
            bits / 4,
            bits / 4
        ))
        .unwrap();
        let mut s = active();
        put(&mut s.memory, "M0", bits, index);
        scan(&mut s, &p);
        assert_eq!(get(&s.memory, "M128", bits), 1 << (bits - 1));
        s.memory.set_input(d("X200"), false);
        scan(&mut s, &p);
        assert_eq!(get(&s.memory, "M128", bits), 1 << (bits - 1));
        let reset = compile_il(&format!(
            "LD X200\n{prefix}BRST K{}M128 K{}M0\nEND",
            bits / 4,
            bits / 4
        ))
        .unwrap();
        scan(&mut s, &reset);
        assert_eq!(get(&s.memory, "M128", bits), 1 << (bits - 1));
        s.memory.set_input(d("X200"), true);
        scan(&mut s, &reset);
        assert_eq!(get(&s.memory, "M128", bits), 0);
        assert!(!s.memory.system_bit(1067));
    }
    let p = compile_il("LD X200\nBSET K4M0 K4M0\nDBSET K8M32 K8M32\nEND").unwrap();
    let mut s = active();
    put(&mut s.memory, "M0", 16, 1);
    put(&mut s.memory, "M32", 32, 1);
    scan(&mut s, &p);
    assert_eq!(get(&s.memory, "M0", 16), 3);
    assert_eq!(get(&s.memory, "M32", 32), 3);
}

#[test]
fn all_indexed_contacts_apply_polarity_gates_and_short_source_zero_extension() {
    for prefix in ["", "D"] {
        for name in ["BLD", "BLDI", "BAND", "BANI", "BOR", "BORI"] {
            let inverted = matches!(name, "BLDI" | "BANI" | "BORI");
            for source in ["Y200", "M128", "S128"] {
                for index in [1, 7, 17] {
                    for gate in [false, true] {
                        let lead = if name.starts_with("BLD") {
                            ""
                        } else {
                            "LD X200\n"
                        };
                        let p =
                            compile_il(&format!("{lead}{prefix}{name} K1{source} E1\nOUT Y0\nEND"))
                                .unwrap();
                        let mut s = active();
                        put(&mut s.memory, source, 16, 0xff02);
                        s.memory.set_double_word(d("D1182"), index);
                        s.memory.set_input(d("X200"), gate);
                        scan(&mut s, &p);
                        let selected = if prefix.is_empty() {
                            index & 15
                        } else {
                            index & 31
                        };
                        let test = (selected == 1) ^ inverted;
                        let expected = if name.starts_with("BLD") {
                            test
                        } else if name.starts_with("BA") {
                            gate && test
                        } else {
                            gate || test
                        };
                        assert_eq!(
                            s.memory.bit(d("Y0")),
                            expected,
                            "{prefix}{name} {source} index{index} gate{gate}"
                        );
                        assert_eq!(flags(&s), [true, false, true]);
                    }
                }
            }
        }
    }
}

#[test]
fn operand_matrices_keep_indexed_sources_distinct_and_reject_unlisted_p_forms() {
    for body in [
        "ABS K1X0",
        "ABS K1",
        "DABS F1",
        "GRY X0 D0",
        "GBIN E1 K1X0",
        "DGRY E1 F1",
        "BOUT E1 K1",
        "DBSET E1 K1",
        "BRST K1X0 E1",
        "BOUT K1M0 K16",
        "DBOUT K1M0 K32",
        "BLD K1X0 K0\nOUT Y0",
        "BLD E1 K0\nOUT Y0",
        "BLD K1 K0\nOUT Y0",
        "BOUTP D0 K1",
        "DBSETP D0 K1",
        "BRSTP D0 K1",
        "BLDP D0 K0\nOUT Y0",
    ] {
        let lead = if body.starts_with("BLD") || body.starts_with("DBLD") {
            ""
        } else {
            "LD X200\n"
        };
        assert!(compile_il(&format!("{lead}{body}\nEND")).is_err(), "{body}");
    }
    for body in [
        "ABSP F7",
        "DABSP K8S992",
        "GRYP K4X360 F7",
        "DGBINP E7 K8Y340",
        "BOUT K4Y360 K1X0",
        "DBSET C255 E7",
        "BRST K4M4080 F7",
        "BLD K1M1000 F7\nOUT Y0",
        "DBLD K8S992 E7\nOUT Y0",
        "DBLD K1M0 F1\nOUT Y0",
        "DBSET K8M0 F7",
    ] {
        // Indexed contacts start their own rung, avoiding a dangling LD.
        let lead = if body.starts_with("BLD") || body.starts_with("DBLD") {
            ""
        } else {
            "LD X200\n"
        };
        assert!(compile_il(&format!("{lead}{body}\nEND")).is_ok(), "{body}");
    }
}

#[test]
fn packed_device_tracking_and_action_labels_roundtrip() {
    for body in [
        "ABSP F7",
        "DABSP K8S992",
        "GRYP K4X360 F7",
        "DGBINP E7 K8Y340",
        "BOUT K4Y360 K1X0",
        "DBSET C255 E7",
        "BRST K4M4080 F7",
    ] {
        let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
        let label = p.networks[0].actions[0].label();
        let restored = compile_il(&format!("LD X200\n{label}\nEND")).unwrap();
        assert_eq!(p.networks[0].actions, restored.networks[0].actions);
        assert_eq!(p.devices, restored.devices);
    }
    let p = compile_il("LD X200\nDGRY K8M4064 E7\nDGBIN E7 K8Y340\nEND").unwrap();
    for n in 4064..=4095 {
        assert!(p.devices.contains(&d(&format!("M{n}"))));
    }
    for n in 224..=255 {
        assert!(p.devices.contains(&d(&format!("Y{n:o}"))));
    }
    for name in ["D1194", "D1195"] {
        assert!(p.devices.contains(&d(name)));
    }
}

#[test]
fn double_f_index_snapshots_alias_and_masks_signed_full_width() {
    // F1 aliases the first word of the destination. Each scan selects from
    // the pre-write value; false BOUT clears the currently selected bit only.
    let p = compile_il("LD X200\nDBOUT D1183 F1\nEND").unwrap();
    let mut s = active();
    s.memory.set_double_word(d("D1183"), 1);
    scan(&mut s, &p);
    assert_eq!(s.memory.double_word(d("D1183")), 3);
    scan(&mut s, &p);
    assert_eq!(s.memory.double_word(d("D1183")), 11);
    s.memory.set_input(d("X200"), false);
    scan(&mut s, &p);
    assert_eq!(s.memory.double_word(d("D1183")), 11);
    assert_eq!(flags(&s), [true, false, true]);

    for (index, alias) in [("F0", "D1029"), ("F1", "D1183"), ("F7", "D1195")] {
        let p = compile_il(&format!(
            "DBLD K8M128 {index}\nOUT Y0\nDBLDI K8M128 {index}\nOUT Y1\nEND"
        ))
        .unwrap();
        let mut s = active();
        put(&mut s.memory, "M128", 32, 0x80000000);
        s.memory.set_double_word(d(alias), -1);
        scan(&mut s, &p);
        assert!(s.memory.bit(d("Y0")));
        assert!(!s.memory.bit(d("Y1")));
        assert!(p.devices.contains(&d(alias)));
        assert!(p.devices.contains(&Device {
            area: Area::D,
            index: d(alias).index + 1
        }));
        assert_eq!(flags(&s), [true, false, true]);
    }
}
