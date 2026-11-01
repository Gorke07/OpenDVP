//! Operand matrices from PDF190/191 and 738..740; independent local oracles.
//! These new combinations are not installed-simulator parity evidence.
use crate::plc::{Area, Device, Memory, Program, ScanStatus, Simulator, compile_il};

fn dev(name: &str) -> Device {
    name.parse().unwrap()
}

fn pack(m: &mut Memory, base: &str, bits: u16, value: u32) {
    let base = dev(base);
    for offset in 0..bits {
        let d = Device {
            area: base.area,
            index: base.index + offset,
        };
        let on = value & (1u32 << offset) != 0;
        if d.area == Area::X {
            m.set_input(d, on);
        } else {
            m.set_internal(d, on);
        }
    }
}

fn scan(s: &mut Simulator, p: &Program) {
    s.scan(p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
}

fn flags(s: &Simulator, base: &str) -> [bool; 3] {
    let base = dev(base);
    [0, 1, 2].map(|n| {
        s.memory.bit(Device {
            area: base.area,
            index: base.index + n,
        })
    })
}

#[test]
fn all_36_signed_contacts_accept_each_packed_area_and_gate_correctly() {
    for bases in [["X0", "X40"], ["Y0", "Y40"], ["M0", "M32"], ["S0", "S32"]] {
        for wide in [false, true] {
            let bits = if wide { 32 } else { 16 };
            let digits = bits / 4;
            let prefix = if wide { "D" } else { "" };
            let pairs = if wide {
                [(0, 0), (1, 2), (i32::MAX, i32::MIN), (-1, 0)]
            } else {
                [(0, 0), (1, 2), (32767, -32768), (-1, 0)]
            };
            for (a, b) in pairs {
                for (relation, expected) in [
                    ("=", a == b),
                    ("<>", a != b),
                    (">", a > b),
                    (">=", a >= b),
                    ("<", a < b),
                    ("<=", a <= b),
                ] {
                    for combination in ["LD", "AND", "OR"] {
                        let head = if combination == "LD" { "" } else { "LD X200\n" };
                        let source = format!(
                            "{head}{prefix}{combination}{relation} K{digits}{} K{digits}{}\nOUT Y200\nEND",
                            bases[0], bases[1]
                        );
                        let p = compile_il(&source).unwrap();
                        let mut s = Simulator::default();
                        pack(&mut s.memory, bases[0], bits, a as u32);
                        pack(&mut s.memory, bases[1], bits, b as u32);
                        s.memory.set_system(1020, true);
                        s.memory.set_system(1022, true);
                        for gate in [false, true] {
                            s.memory.set_input(dev("X200"), gate);
                            scan(&mut s, &p);
                            let wanted = match combination {
                                "AND" => gate && expected,
                                "OR" => gate || expected,
                                _ => expected,
                            };
                            assert_eq!(s.memory.bit(dev("Y200")), wanted, "{source}: {a}, {b}");
                            assert_eq!(flags(&s, "M1020"), [true, false, true]);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn cmp_widths_and_pulse_forms_preserve_hold_and_rearm() {
    for name in ["CMP", "CMPP", "DCMP", "DCMPP"] {
        let bits = if name.starts_with('D') { 32 } else { 16 };
        let digits = bits / 4;
        let source = format!("LD X200\n{name} K{digits}M0 K{digits}M32 M128\nEND");
        let p = compile_il(&source).unwrap();
        let mut s = Simulator::default();
        pack(&mut s.memory, "M0", bits, 3);
        pack(&mut s.memory, "M32", bits, 2);
        s.memory.set_input(dev("X200"), true);
        scan(&mut s, &p);
        assert_eq!(flags(&s, "M128"), [true, false, false], "{name}");
        pack(&mut s.memory, "M0", bits, 1);
        scan(&mut s, &p);
        let held = if name.ends_with("PP") {
            [true, false, false]
        } else {
            [false, false, true]
        };
        assert_eq!(flags(&s, "M128"), held, "{name}");
        s.memory.set_input(dev("X200"), false);
        scan(&mut s, &p);
        assert_eq!(flags(&s, "M128"), held, "{name}");
        s.memory.set_input(dev("X200"), true);
        scan(&mut s, &p);
        assert_eq!(flags(&s, "M128"), [false, false, true], "{name}");
        let action_text = p.networks[0].actions[0].label();
        let restored = compile_il(&format!("LD X200\n{action_text}\nEND")).unwrap();
        assert_eq!(
            p.networks[0].condition, restored.networks[0].condition,
            "{name}"
        );
        assert_eq!(
            p.networks[0].actions, restored.networks[0].actions,
            "{name}"
        );
        assert_eq!(p.devices, restored.devices, "{name}");
    }
}

#[test]
fn zone_widths_and_pulses_keep_inclusive_reversed_bounds_and_rearm() {
    for name in ["ZCP", "ZCPP", "DZCP", "DZCPP"] {
        let bits = if name.starts_with('D') { 32 } else { 16 };
        let digits = bits / 4;
        let p = compile_il(&format!(
            "LD X200\n{name} K{digits}M0 K{digits}M32 K{digits}M64 M128\nEND"
        ))
        .unwrap();
        let mut s = Simulator::default();
        pack(&mut s.memory, "M0", bits, 5);
        pack(&mut s.memory, "M32", bits, 3);
        pack(&mut s.memory, "M64", bits, 5);
        s.memory.set_input(dev("X200"), true);
        scan(&mut s, &p);
        assert_eq!(flags(&s, "M128"), [false, true, false], "{name}");
        pack(&mut s.memory, "M64", bits, 4);
        scan(&mut s, &p);
        let held = if name.ends_with("PP") {
            [false, true, false]
        } else {
            [true, false, false]
        };
        assert_eq!(flags(&s, "M128"), held, "{name}");
        s.memory.set_input(dev("X200"), false);
        scan(&mut s, &p);
        assert_eq!(flags(&s, "M128"), held, "{name}");
        s.memory.set_input(dev("X200"), true);
        scan(&mut s, &p);
        assert_eq!(flags(&s, "M128"), [true, false, false], "{name}");
        let action_text = p.networks[0].actions[0].label();
        let restored = compile_il(&format!("LD X200\n{action_text}\nEND")).unwrap();
        assert_eq!(
            p.networks[0].condition, restored.networks[0].condition,
            "{name}"
        );
        assert_eq!(
            p.networks[0].actions, restored.networks[0].actions,
            "{name}"
        );
        assert_eq!(p.devices, restored.devices, "{name}");
    }
}

#[test]
fn comparison_sources_are_sampled_before_overlapping_result_bits_are_written() {
    for prefix in ["", "D"] {
        for (body, expected) in [
            ("CMP K1M0 K1M32 M0", [true, false, false]),
            ("ZCP K1M0 K1M32 K1M64 M0", [false, true, false]),
        ] {
            let p = compile_il(&format!("LD X200\n{prefix}{body}\nEND")).unwrap();
            let mut s = Simulator::default();
            pack(&mut s.memory, "M0", 4, 3);
            pack(
                &mut s.memory,
                "M32",
                4,
                if body.starts_with("CMP") { 2 } else { 5 },
            );
            pack(&mut s.memory, "M64", 4, 4);
            s.memory.set_input(dev("X200"), true);
            scan(&mut s, &p);
            assert_eq!(flags(&s, "M0"), expected, "{prefix}{body}");
        }
    }
}

#[test]
fn direct_indices_and_packed_sources_observe_same_scan_writes() {
    let p = compile_il(
        "LD X200\nMOV K-3 D1194\nMOV K2 D1195\nSET M0\nSET M1\n\
         CMP E7 F7 M128\nZCP E7 F7 K1M0 M132\n\
         LD< E7 F7\nOUT Y200\n\
         LD X200\nDMOV K-2147483648 D1194\nSET M63\n\
         DCMP E7 K8M32 M136\nDZCP E7 K0 K8M32 M140\n\
         DLD= E7 K8M32\nOUT Y201\nEND",
    )
    .unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(dev("X200"), true);
    scan(&mut s, &p);
    assert_eq!(flags(&s, "M128"), [false, false, true]);
    assert_eq!(flags(&s, "M132"), [false, false, true]);
    assert_eq!(flags(&s, "M136"), [false, true, false]);
    assert_eq!(flags(&s, "M140"), [false, true, false]);
    assert!(s.memory.bit(dev("Y200")));
    assert!(s.memory.bit(dev("Y201")));
    assert!(p.devices.contains(&dev("D1194")) && p.devices.contains(&dev("D1195")));
    for n in 32..64 {
        assert!(p.devices.contains(&dev(&format!("M{n}"))));
    }
}

#[test]
fn comparison_group_bounds_and_direct_index_widths_are_checked() {
    for body in [
        "CMP K0M0 K0 M0",
        "CMP K5M0 K0 M0",
        "DCMP K9M0 K0 M0",
        "ZCP K4M4081 K0 K1 M0",
        "DZCP K8S993 K0 K1 M0",
        "LD= K4X361 K0",
        "DLD= K8Y341 K0",
        "CMP E8 K0 M0",
        "DCMP F0 K0 M0",
        "DZCP K0 F7 K1 M0",
        "DLD= F0 K0",
        "LD= D0E8 K0",
        "CMP X0 K0 M0",
        "CMP C200 K0 M0",
        "ZCP K0 K1 C200 M0",
    ] {
        let source = if body.starts_with("LD") || body.starts_with("DLD") {
            format!("{body}\nOUT Y0\nEND")
        } else {
            format!("LD X200\n{body}\nEND")
        };
        assert!(compile_il(&source).is_err(), "{source}");
    }
    for body in [
        "CMP K4X360 F7 Y375",
        "DCMP K8M4064 E7 S1021",
        "ZCP K4S1008 K0 E0 M4093",
        "DZCP E7 K8Y340 C255 M0",
    ] {
        assert!(
            compile_il(&format!("LD X200\n{body}\nEND")).is_ok(),
            "{body}"
        );
    }
}

#[test]
fn packed_sources_compare_with_natural_double_counters_without_reading_next_counter() {
    let p = compile_il(
        "DLD= C200 K8M0\nOUT Y0\nLD X200\nDCMP C200 E7 M128\n\
         DZCP K-2147483648 K0 C200 M132\nEND",
    )
    .unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(dev("X200"), true);
    s.memory.set_double_word(dev("C200"), i32::MIN);
    s.memory.set_double_word(dev("C201"), 123456789);
    s.memory.set_double_word(dev("D1194"), i32::MIN);
    pack(&mut s.memory, "M0", 32, 0x8000_0000);
    scan(&mut s, &p);
    assert!(s.memory.bit(dev("Y0")));
    assert_eq!(flags(&s, "M128"), [false, true, false]);
    assert_eq!(flags(&s, "M132"), [false, true, false]);
    assert_eq!(s.memory.double_word(dev("C201")), 123456789);
    assert!(p.devices.contains(&dev("C200")));
    assert!(!p.devices.contains(&dev("C201")));
}

#[test]
fn short_groups_remain_positive_at_their_highest_bit() {
    for (prefix, group, value) in [
        ("", "K1M0", 8),
        ("", "K3M0", 2048),
        ("D", "K1M0", 8),
        ("D", "K7M0", 134217728),
    ] {
        let p = compile_il(&format!(
            "{prefix}LD> {group} K0\nOUT Y0\nLD X200\n\
             {prefix}CMP {group} K{value} M128\nEND"
        ))
        .unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(dev("X200"), true);
        pack(&mut s.memory, "M0", 32, value as u32);
        scan(&mut s, &p);
        assert!(s.memory.bit(dev("Y0")));
        assert_eq!(flags(&s, "M128"), [false, true, false]);
    }
}
