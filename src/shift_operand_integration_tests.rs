//! Value and ordering oracles independent of the native bytecode fixtures.
use crate::plc::{Area, Device, Memory, Program, ScanStatus, Simulator, compile_il};
fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn at(base: Device, offset: u16) -> Device {
    Device {
        index: base.index + offset,
        ..base
    }
}
fn put(m: &mut Memory, base: Device, bits: u16, value: u32) {
    for n in 0..bits {
        let bit = at(base, n);
        if bit.area == Area::X {
            m.set_input(bit, value & (1 << n) != 0);
        } else {
            m.set_internal(bit, value & (1 << n) != 0);
        }
    }
}
fn get(m: &Memory, base: Device, bits: u16) -> u32 {
    (0..bits).fold(0, |v, n| v | (u32::from(m.bit(at(base, n))) << n))
}
fn cells(m: &mut Memory, base: Device, digits: u16, values: &[u32]) {
    for (i, value) in values.iter().enumerate() {
        put(m, at(base, i as u16 * digits * 4), digits * 4, *value);
    }
}
fn read_cells(m: &Memory, base: Device, digits: u16, count: u16) -> Vec<u32> {
    (0..count)
        .map(|i| get(m, at(base, i * digits * 4), digits * 4))
        .collect()
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
fn rotation(
    value: u64,
    carry: bool,
    width: u16,
    n: u16,
    left: bool,
    through_carry: bool,
) -> (u32, bool) {
    let ring_width = width + u16::from(through_carry);
    let mask = (1u64 << ring_width) - 1;
    let ring = value
        | if through_carry {
            u64::from(carry) << width
        } else {
            0
        };
    let rotated = if left {
        (ring << n) | (ring >> (ring_width - n))
    } else {
        (ring >> n) | (ring << (ring_width - n))
    } & mask;
    let out = if through_carry {
        rotated & (1 << width) != 0
    } else if left {
        value & (1 << (width - n)) != 0
    } else {
        value & (1 << (n - 1)) != 0
    };
    ((rotated & ((1 << width) - 1)) as u32, out)
}

#[test]
fn all_sixteen_rotations_match_closed_rings_and_pulse_rearming() {
    for wide in [false, true] {
        let width = if wide { 32 } else { 16 };
        for name in ["ROR", "ROL", "RCR", "RCL"] {
            for pulse in [false, true] {
                for area in ["Y200", "M128", "S128"] {
                    for count in [1, width] {
                        for carry in [false, true] {
                            let mnemonic = format!(
                                "{}{name}{}",
                                if wide { "D" } else { "" },
                                if pulse { "P" } else { "" }
                            );
                            let p = compile_il(&format!(
                                "LD X200\n{mnemonic} K{}{area} K{count}\nEND",
                                width / 4
                            ))
                            .unwrap();
                            let mut s = active();
                            let initial = if wide { 0x87654321 } else { 0x8123 };
                            put(&mut s.memory, d(area), width, initial);
                            s.memory.set_system(1022, carry);
                            s.memory.set_internal(at(d(area), width), true);
                            let step = |value, carry| {
                                rotation(
                                    u64::from(value),
                                    carry,
                                    width,
                                    count,
                                    name.ends_with('L'),
                                    name.starts_with("RC"),
                                )
                            };
                            let first = step(initial, carry);
                            scan(&mut s, &p);
                            assert_eq!(
                                (get(&s.memory, d(area), width), s.memory.system_bit(1022)),
                                first,
                                "{mnemonic} {area} K{count}"
                            );
                            let held = if pulse { first } else { step(first.0, first.1) };
                            scan(&mut s, &p);
                            assert_eq!(
                                (get(&s.memory, d(area), width), s.memory.system_bit(1022)),
                                held
                            );
                            s.memory.set_input(d("X200"), false);
                            scan(&mut s, &p);
                            assert_eq!(
                                (get(&s.memory, d(area), width), s.memory.system_bit(1022)),
                                held
                            );
                            s.memory.set_input(d("X200"), true);
                            scan(&mut s, &p);
                            assert_eq!(
                                (get(&s.memory, d(area), width), s.memory.system_bit(1022)),
                                step(held.0, held.1)
                            );
                            assert!(s.memory.bit(at(d(area), width)));
                            assert!(s.memory.system_bit(1020));
                            assert!(!s.memory.system_bit(1021));
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn grouped_word_shifts_move_all_bits_before_live_source_injection() {
    for digits in 1..=4 {
        for (name, expected) in [("WSFR", vec![3, 4, 5, 5, 5]), ("WSFL", vec![1, 2, 1, 2, 3])] {
            let source = at(d("M128"), digits * 8);
            let p = compile_il(&format!(
                "LD X200\n{name} K{digits}{source} K{digits}M128 K5 K2\nEND"
            ))
            .unwrap();
            let mut s = active();
            cells(&mut s.memory, d("M128"), digits, &[1, 2, 3, 4, 5]);
            s.memory.set_internal(d("M127"), true);
            s.memory.set_internal(at(d("M128"), digits * 20), true);
            scan(&mut s, &p);
            assert_eq!(
                read_cells(&s.memory, d("M128"), digits, 5),
                expected,
                "{name} K{digits}"
            );
            assert!(s.memory.bit(d("M127")) && s.memory.bit(at(d("M128"), digits * 20)));
        }
        for source in ["X0", "Y0", "M0", "S0"] {
            for target in ["Y200", "M128", "S128"] {
                for (name, expected) in [
                    ("WSFR", vec![3, 4, 5, 10, 11]),
                    ("WSFL", vec![10, 11, 1, 2, 3]),
                ] {
                    let p = compile_il(&format!(
                        "LD X200\n{name} K{digits}{source} K{digits}{target} K5 K2\nEND"
                    ))
                    .unwrap();
                    let mut s = active();
                    cells(&mut s.memory, d(source), digits, &[10, 11]);
                    cells(&mut s.memory, d(target), digits, &[1, 2, 3, 4, 5]);
                    scan(&mut s, &p);
                    assert_eq!(read_cells(&s.memory, d(target), digits, 5), expected);
                }
            }
        }
    }
}

#[test]
fn packed_fifo_push_full_pop_empty_and_last_slot_retention() {
    for digits in 1..=4 {
        for area in ["Y200", "M128", "S128"] {
            let push = compile_il(&format!("LD X200\nSFWR E1 K{digits}{area} K4\nEND")).unwrap();
            let pop = compile_il(&format!("LD X200\nSFRD K{digits}{area} F2 K4\nEND")).unwrap();
            let mut s = active();
            cells(&mut s.memory, d(area), digits, &[0, 7, 8, 9]);
            s.memory.set_internal(at(d(area), digits * 16), true);
            for value in [11, 12, 13, 14] {
                s.memory.set_word(d("D1182"), value);
                scan(&mut s, &push);
            }
            assert_eq!(
                read_cells(&s.memory, d(area), digits, 4),
                vec![3, 11, 12, 13]
            );
            assert!(s.memory.system_bit(1022));
            for (value, empty, queue) in [
                (11, false, vec![2, 12, 13, 13]),
                (12, false, vec![1, 13, 13, 13]),
                (13, true, vec![0, 13, 13, 13]),
                (13, true, vec![0, 13, 13, 13]),
            ] {
                scan(&mut s, &pop);
                assert_eq!(s.memory.word(d("D1185")), value);
                assert_eq!(s.memory.system_bit(1020), empty);
                assert!(s.memory.system_bit(1022));
                assert_eq!(read_cells(&s.memory, d(area), digits, 4), queue);
            }
            assert!(s.memory.bit(at(d(area), digits * 16)));
        }
    }
}

#[test]
fn unaligned_word_group_injection_reads_each_preceding_write() {
    for (name, source, initial, expected) in [
        ("WSFL", "M126", 0x0a59, 0x059a),
        ("WSFR", "M134", 0x0b59, 0x0ab5),
    ] {
        let p = compile_il(&format!("LD X200\n{name} K1{source} K1M128 K3 K1\nEND")).unwrap();
        let mut s = active();
        put(&mut s.memory, d("M128"), 12, initial);
        s.memory.set_internal(d("M126"), false);
        s.memory.set_internal(d("M127"), true);
        scan(&mut s, &p);
        assert_eq!(get(&s.memory, d("M128"), 12), expected, "{name}");
    }
    for name in ["WSFR", "WSFL"] {
        let p = compile_il(&format!("LD X200\n{name} K2M128 K2M129 K1 K1\nEND")).unwrap();
        let mut s = active();
        s.memory.set_internal(d("M128"), true);
        scan(&mut s, &p);
        assert_eq!(get(&s.memory, d("M129"), 8), 255);
    }
}

#[test]
fn fifo_pointer_wrap_uses_untruncated_next_for_payload_and_snapshot_for_source() {
    let p = compile_il("LD X200\nSFWR K9 K1M128 K18\nEND").unwrap();
    let mut s = active();
    cells(&mut s.memory, d("M128"), 1, &[15]);
    put(&mut s.memory, at(d("M128"), 17 * 4), 4, 7);
    scan(&mut s, &p);
    assert_eq!(get(&s.memory, d("M128"), 4), 0);
    assert_eq!(get(&s.memory, at(d("M128"), 16 * 4), 4), 9);
    assert_eq!(get(&s.memory, at(d("M128"), 17 * 4), 4), 7);
    scan(&mut s, &p);
    assert_eq!(get(&s.memory, d("M128"), 4), 1);
    assert_eq!(get(&s.memory, at(d("M128"), 4), 4), 9);
    let p = compile_il("LD X200\nSFWR K1M128 K1M128 K4\nEND").unwrap();
    cells(&mut s.memory, d("M128"), 1, &[1, 10, 11, 12]);
    scan(&mut s, &p);
    assert_eq!(read_cells(&s.memory, d("M128"), 1, 4), vec![2, 10, 1, 12]);
}

#[test]
fn fifo_pop_output_alias_changes_later_shift_reads() {
    let p = compile_il("LD X200\nSFRD K1M128 K1M136 K4\nEND").unwrap();
    let mut s = active();
    cells(&mut s.memory, d("M128"), 1, &[2, 11, 12, 13]);
    scan(&mut s, &p);
    // Output overwrites slot2 before it is read for the slot2->slot1 shift.
    assert_eq!(read_cells(&s.memory, d("M128"), 1, 4), vec![1, 11, 13, 13]);
    let p = compile_il("LD X200\nSFRD K1M128 K1M128 K4\nEND").unwrap();
    cells(&mut s.memory, d("M128"), 1, &[2, 11, 12, 13]);
    scan(&mut s, &p);
    assert_eq!(read_cells(&s.memory, d("M128"), 1, 4), vec![11, 12, 13, 13]);
    assert!(!s.memory.system_bit(1020));
}

#[test]
fn negative_fifo_headers_preserve_payload_and_latch_their_own_error_offsets() {
    for (body, offset, zero) in [
        ("SFWR K9 K4M128 K3", 3, true),
        ("SFRD K4M128 K1M256 K3", 1, false),
    ] {
        let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
        let mut s = active();
        cells(&mut s.memory, d("M128"), 4, &[0xffff, 11, 12]);
        put(&mut s.memory, d("M256"), 8, 0xa5);
        scan(&mut s, &p);
        assert_eq!(read_cells(&s.memory, d("M128"), 4, 3), vec![0xffff, 11, 12]);
        assert_eq!(get(&s.memory, d("M256"), 8), 0xa5);
        assert_eq!(s.memory.system_bit(1020), zero);
        assert!(s.memory.system_bit(1022));
        assert_eq!(s.memory.word(d("D1067")), 0xe1a);
        assert_eq!(s.memory.word(d("D1068")), 1 + offset);
    }
}

#[test]
fn digit_transfer_modes_validate_full_source_and_truncate_only_the_write() {
    for (hex, input, old, expected) in [(false, 1234, 5678, 5128), (true, 0x1234, 0x5678, 0x5128)] {
        for source in ["X0", "Y0", "M0", "S0"] {
            for target in ["Y200", "M128", "S128"] {
                let p = compile_il(&format!(
                    "LD X200\nSMOV K4{source} K4 K2 K4{target} K3\nEND"
                ))
                .unwrap();
                let mut s = active();
                s.memory.set_system(1168, hex);
                put(&mut s.memory, d(source), 16, input);
                put(&mut s.memory, d(target), 16, old);
                scan(&mut s, &p);
                assert_eq!(get(&s.memory, d(target), 16), expected);
                let short = compile_il(&format!(
                    "LD X200\nSMOV K4{source} K4 K2 K1{target} K3\nEND"
                ))
                .unwrap();
                put(&mut s.memory, d(target), 16, 0xabc8);
                scan(&mut s, &short);
                assert_eq!(
                    get(&s.memory, d(target), 16),
                    if hex { 0xabc8 } else { 0xabc0 }
                );
            }
        }
    }
    let p = compile_il("LD X200\nSMOV K4M0 K2 K1 K1M128 K1\nEND").unwrap();
    let mut s = active();
    put(&mut s.memory, d("M0"), 16, 0xffff);
    put(&mut s.memory, d("M128"), 16, 0xabc7);
    scan(&mut s, &p);
    assert_eq!(get(&s.memory, d("M128"), 16), 0xabc7);
    assert_eq!(s.memory.word(d("D1067")), 0xe18);
    assert_eq!(s.memory.word(d("D1068")), 12);
}

#[test]
fn direct_indices_alias_real_words_and_matrix_limits_remain_instruction_specific() {
    for body in [
        "ROR F1 K1",
        "DRCL E1 K32",
        "SFWR F1 K1M128 K3",
        "SFRD K1M128 E7 K3",
        "SMOV F1 K4 K2 E2 K3",
    ] {
        let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
        let label = p.networks[0].actions[0].label();
        let restored = compile_il(&format!("LD X200\n{label}\nEND")).unwrap();
        assert_eq!(p.networks[0].actions, restored.networks[0].actions);
        assert_eq!(p.devices, restored.devices);
    }
    let p = compile_il("LD X200\nSMOV F1 K4 K2 E2 K3\nEND").unwrap();
    let mut s = active();
    s.memory.set_word(d("D1183"), 1234);
    s.memory.set_word(d("D1184"), 5678);
    s.memory.set_word(d("D1185"), 77);
    scan(&mut s, &p);
    assert_eq!(s.memory.word(d("D1184")), 5128);
    assert_eq!(s.memory.word(d("D1185")), 77);
    for body in [
        "ROR K1M0 K1",
        "ROR K3M0 K1",
        "DROR K7M0 K1",
        "DROR F1 K1",
        "ROR E1 D0",
        "WSFR K1M0 D0 K3 K1",
        "WSFL D0 K1M0 K3 K1",
        "WSFR K1M0 K2Y0 K3 K1",
        "WSFR E1 D0 K3 K1",
        "SFWR K1 E1 K3",
        "SFWR K1 K1X0 K3",
        "SFRD K1X0 D0 K3",
        "SMOV K1234 K4 K2 E1 K3",
        "SMOV E1 D0 K1 E2 K1",
        "SFWR K1 K4M4080 K3",
    ] {
        assert!(
            compile_il(&format!("LD X200\n{body}\nEND")).is_err(),
            "{body}"
        );
    }
}
