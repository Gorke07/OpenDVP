use crate::plc::{Area, Device, Memory, Simulator, compile_il};
fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn put(m: &mut Memory, base: u16, value: u32) {
    for n in 0..32 {
        m.set_internal(
            Device {
                area: Area::M,
                index: base + n,
            },
            value & (1 << n) != 0,
        );
    }
}
fn get(m: &Memory, base: u16) -> u32 {
    (0..32).fold(0, |v, n| {
        v | (u32::from(m.bit(Device {
            area: Area::M,
            index: base + n,
        })) << n)
    })
}
fn run(body: &str, initial: u32, swap: bool) -> Simulator {
    let p = compile_il(&format!("LD X0\n{body}\nEND")).unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X0"), true);
    s.memory.set_system(1303, swap);
    put(&mut s.memory, 0, initial);
    s.scan(&p, 0);
    s
}

#[test]
fn exchange_same_base_swap_uses_second_operand_width_and_snapshot() {
    for (body, input, expected) in [
        ("XCH K1M0 K4M0", 0x1234, 0x3412),
        ("XCH K4M0 K1M0", 0x1234, 0x1230),
        ("DXCH K4M0 K8M0", 0x12345678, 0x56781234),
        ("DXCH K8M0 K4M0", 0x12345678, 0x12340000),
    ] {
        assert_eq!(get(&run(body, input, true).memory, 0), expected, "{body}");
    }
}

#[test]
fn exchange_overlapping_targets_write_second_then_first_from_snapshots() {
    assert_eq!(
        get(&run("XCH K4M0 K4M8", 0x00123456, false).memory, 0),
        0x00341234
    );
    let p = compile_il("LD X0\nDXCH K8M0 K8M16\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X0"), true);
    put(&mut s.memory, 0, 0x89abcdef);
    put(&mut s.memory, 32, 0x12345678);
    s.scan(&p, 0);
    assert_eq!(get(&s.memory, 0), 0x567889ab);
    assert_eq!(get(&s.memory, 32), 0x123489ab);
}

#[test]
fn index_storage_aliases_keep_distinct_native_exchange_keys() {
    for (body, initial, expected, wide) in [
        ("XCH E1 D1182", 0x1234, 0x1234, false),
        ("XCH F1 D1183", 0x5678, 0x5678, false),
        ("DXCH E1 D1182", 0x12345678, 0x12345678, true),
        ("XCH E1 E1", 0x1234, 0x3412, false),
        ("XCH F1 F1", 0x5678, 0x7856, false),
        ("DXCH E1 E1", 0x12345678, 0x56781234, true),
    ] {
        let p = compile_il(&format!("LD X0\n{body}\nEND")).unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.memory.set_system(1303, true);
        let alias = if body.contains("F1") {
            d("D1183")
        } else {
            d("D1182")
        };
        if wide {
            s.memory.set_double_word(alias, initial);
        } else {
            s.memory.set_word(alias, initial as i16);
        }
        s.scan(&p, 0);
        assert_eq!(
            if wide {
                s.memory.double_word(alias)
            } else {
                i32::from(s.memory.word(alias) as u16)
            },
            expected,
            "{body}"
        );
    }
}

#[test]
fn pulse_exchange_holds_and_rearms_while_ordinary_exchange_runs_each_scan() {
    for op in ["XCH", "DXCH"] {
        for suffix in ["", "P"] {
            let p = compile_il(&format!("LD X0\n{op}{suffix} K1M0 K1M128\nEND")).unwrap();
            let mut s = Simulator::default();
            put(&mut s.memory, 0, 0xa1);
            put(&mut s.memory, 128, 0xb2);
            s.memory.set_input(d("X0"), true);
            s.scan(&p, 0);
            assert_eq!((get(&s.memory, 0), get(&s.memory, 128)), (0xa2, 0xb1));
            s.scan(&p, 0);
            let held = if suffix.is_empty() {
                (0xa1, 0xb2)
            } else {
                (0xa2, 0xb1)
            };
            assert_eq!((get(&s.memory, 0), get(&s.memory, 128)), held);
            s.memory.set_input(d("X0"), false);
            s.scan(&p, 0);
            assert_eq!((get(&s.memory, 0), get(&s.memory, 128)), held);
            s.memory.set_input(d("X0"), true);
            s.scan(&p, 0);
            assert_eq!(
                (get(&s.memory, 0), get(&s.memory, 128)),
                if suffix.is_empty() {
                    (0xa2, 0xb1)
                } else {
                    (0xa1, 0xb2)
                }
            );
        }
    }
}

#[test]
fn exchange_validates_both_writes_and_tracks_implicit_operands() {
    for body in [
        "XCH K1 D0",
        "XCH K1X0 D0",
        "XCH D0 K1X0",
        "DXCH F0 E0",
        "DXCH E0 F0",
        "XCH K4M998 D0",
        "DXCH D0 K8M4080",
        "XCH K5M0 D0",
    ] {
        assert!(
            compile_il(&format!("LD X0\n{body}\nEND")).is_err(),
            "{body}"
        );
    }
    for body in [
        "XCHP F7 K4Y360",
        "XCH E7 T255",
        "DXCH E7 C255",
        "DXCHP K8M4064 K8S992",
    ] {
        let p = compile_il(&format!("LD X0\n{body}\nEND")).unwrap();
        let label = p.networks[0].actions[0].label();
        let restored = compile_il(&format!("LD X0\n{label}\nEND")).unwrap();
        assert_eq!(p.devices, restored.devices);
        assert_eq!(p.networks[0].actions, restored.networks[0].actions);
    }
    let p = compile_il("LD X0\nDXCH K8M4064 E7\nEND").unwrap();
    for n in 4064..=4095 {
        assert!(p.devices.contains(&d(&format!("M{n}"))));
    }
    for name in ["D1194", "D1195", "M1303"] {
        assert!(p.devices.contains(&d(name)));
    }
}
