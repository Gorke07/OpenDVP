//! Literal IEEE patterns and target guards independent of the native corpus.
use crate::float_ops::FloatAction;
use crate::plc::{Area, Device, Memory, Simulator, compile_il};
use std::collections::BTreeSet;

fn d(text: &str) -> Device {
    text.parse().unwrap()
}
fn action(value: &str, target: &str) -> FloatAction {
    FloatAction::parse("DMOVR", &[value.to_string(), target.to_string()]).unwrap()
}
fn packed(m: &Memory, base: Device, width: u16) -> u32 {
    (0..width).fold(0, |bits, i| {
        bits | (u32::from(m.bit(Device {
            index: base.index + i,
            ..base
        })) << i)
    })
}

#[test]
fn every_packed_width_preserves_literal_low_bits_neighbors_and_flags() {
    for (area, base) in [(Area::Y, 16), (Area::M, 128), (Area::S, 500)] {
        for digits in 1..=8 {
            let base = Device { area, index: base };
            let target = format!("K{digits}{base}");
            for (literal, bits) in [
                ("F0", 0u32),
                ("F-0.0", 0x80000000),
                ("F1.2", 0x3f99999a),
                ("F-1.2", 0xbf99999a),
                ("F1e-45", 1),
            ] {
                let mut memory = Memory::default();
                for i in base.index - 1..=base.index + 32 {
                    memory.set_internal(Device { index: i, ..base }, true);
                }
                memory.set_system(1020, true);
                memory.set_system(1021, false);
                memory.set_system(1022, true);
                let op = action(literal, &target);
                op.execute(&mut memory);
                let mask = u32::MAX >> (32 - digits * 4);
                assert_eq!(
                    packed(&memory, base, digits * 4),
                    bits & mask,
                    "{literal} {target}"
                );
                assert!(memory.bit(Device {
                    index: base.index - 1,
                    ..base
                }));
                for i in digits * 4..=32 {
                    assert!(memory.bit(Device {
                        index: base.index + i,
                        ..base
                    }));
                }
                assert!(
                    memory.system_bit(1020) && !memory.system_bit(1021) && memory.system_bit(1022)
                );
                assert!(!memory.system_bit(1067) && !memory.system_bit(1068));
                assert_eq!(
                    op.display_value(&memory),
                    f32::from_bits(bits & mask).to_string()
                );
                assert_eq!(
                    op.label(),
                    format!("DMOVR {} {target}", literal.to_uppercase())
                );
                let mut devices = BTreeSet::new();
                op.devices(&mut devices);
                for i in 0..digits * 4 {
                    assert!(devices.contains(&Device {
                        index: base.index + i,
                        ..base
                    }));
                }
                assert!(!devices.contains(&Device {
                    index: base.index + digits * 4,
                    ..base
                }));
            }
        }
    }
}

#[test]
fn ordinary_and_pulse_moves_hold_off_and_pulse_waits_for_a_new_edge() {
    for op in ["DMOVR", "DMOVRP"] {
        let program = compile_il(&format!("LD X0\n{op} F-1.2 K8M128\nEND")).unwrap();
        let mut simulator = Simulator::default();
        simulator.scan(&program, 0);
        assert_eq!(packed(&simulator.memory, d("M128"), 32), 0);
        simulator.memory.set_input(d("X0"), true);
        simulator.scan(&program, 0);
        assert_eq!(packed(&simulator.memory, d("M128"), 32), 0xbf99999a);
        simulator.memory.set_internal(d("M128"), true);
        simulator.scan(&program, 0);
        assert_eq!(
            packed(&simulator.memory, d("M128"), 32),
            if op.ends_with('P') {
                0xbf99999b
            } else {
                0xbf99999a
            }
        );
        simulator.memory.set_input(d("X0"), false);
        simulator.memory.set_internal(d("M128"), true);
        simulator.scan(&program, 0);
        assert_eq!(packed(&simulator.memory, d("M128"), 32), 0xbf99999b);
        simulator.memory.set_input(d("X0"), true);
        simulator.scan(&program, 0);
        assert_eq!(packed(&simulator.memory, d("M128"), 32), 0xbf99999a);
    }
}

#[test]
fn grouped_move_encoding_uses_literal_destination_descriptors() {
    for (target, descriptor) in [
        ("K1Y20", [0x1084, 0x0582, 0x0080, 0x0080]),
        ("K8M128", [0x8084, 0x0890, 0x0080, 0x0080]),
        ("K6S500", [0xf484, 0x018c, 0x0080, 0x0080]),
    ] {
        for (op, header) in [("DMOVR", 0xf100), ("DMOVRP", 0xf110)] {
            let p = compile_il(&format!("LD X0\n{op} F-0.0 {target}\nEND")).unwrap();
            let encoded = crate::es2_encoder::encode(&p).unwrap();
            assert_eq!(encoded.words[1], header);
            assert_eq!(&encoded.words[2..6], &[0x0080, 0x0080, 0x0080, 0x8080]);
            assert_eq!(&encoded.words[6..10], &descriptor);
            assert_eq!(encoded.next_pc, [1, 10, 0]);
        }
    }
}

#[test]
fn packed_target_permission_is_scoped_to_literal_moves() {
    for text in [
        "DMOVR D0 K8M0",
        "DMOVR K1 K8M0",
        "DMOVR F1 K8X0",
        "DMOVR F1 E0",
        "DMOVR F1 F0",
        "DMOVR F1 M0",
        "DMOVR F1 K0M0",
        "DMOVR F1 K9M0",
        "DMOVR F1 K8S1000",
        "DMOVR F1 K8Y377",
        "DMOVR F1 K8M4095",
        "FLT K8M0 D0",
        "INT K8M0 D0",
        "DINT D0 K8M0",
        "DEADD D0 D2 K8M0",
    ] {
        assert!(
            compile_il(&format!("LD X0\n{text}\nEND")).is_err(),
            "{text}"
        );
    }
}
