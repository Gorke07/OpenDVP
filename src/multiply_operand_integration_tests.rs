//! Independent result oracles for complete and truncated arithmetic operands.
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
fn put(m: &mut Memory, raw: &str, bits: u16, value: u64) {
    let base = d(raw);
    for n in 0..bits {
        let bit = at(base, n);
        if bit.area == Area::X {
            m.set_input(bit, value & (1 << n) != 0);
        } else {
            m.set_internal(bit, value & (1 << n) != 0);
        }
    }
}
fn get(m: &Memory, raw: &str, bits: u16) -> u64 {
    (0..bits).fold(0, |v, n| v | (u64::from(m.bit(at(d(raw), n))) << n))
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
fn all_forms_pack_groups_preserve_neighbors_and_rearm_pulses() {
    for name in [
        "MUL", "DIV", "DMUL", "DDIV", "MUL16", "DIV16", "MUL32", "DIV32",
    ] {
        let full = matches!(name, "MUL" | "DIV" | "DMUL" | "DDIV");
        let division = name.contains("DIV");
        for pulse in [false, true] {
            for source in ["X0", "Y0", "M0", "S0"] {
                for target in ["Y200", "M128", "S128"] {
                    let mnemonic = format!("{name}{}", if pulse { "P" } else { "" });
                    let p = compile_il(&format!(
                        "LD X200\n{mnemonic} K1{source} K3 K1{target}\nEND"
                    ))
                    .unwrap();
                    let mut s = active();
                    put(&mut s.memory, source, 4, 11);
                    put(&mut s.memory, target, 16, 0xa5a5);
                    let width = if full { 8 } else { 4 };
                    let mask = (1u64 << width) - 1;
                    let expected = |a: u64| {
                        let low = if division { a / 3 } else { a * 3 };
                        let high = if division { a % 3 } else { 0 };
                        (0xa5a5 & !mask) | (low & 15) | if full { (high & 15) << 4 } else { 0 }
                    };
                    scan(&mut s, &p);
                    assert_eq!(
                        get(&s.memory, target, 16),
                        expected(11),
                        "{mnemonic} {source} {target}"
                    );
                    put(&mut s.memory, source, 4, 14);
                    scan(&mut s, &p);
                    assert_eq!(
                        get(&s.memory, target, 16),
                        expected(if pulse { 11 } else { 14 })
                    );
                    s.memory.set_input(d("X200"), false);
                    scan(&mut s, &p);
                    assert_eq!(
                        get(&s.memory, target, 16),
                        expected(if pulse { 11 } else { 14 })
                    );
                    s.memory.set_input(d("X200"), true);
                    scan(&mut s, &p);
                    assert_eq!(get(&s.memory, target, 16), expected(14));
                    assert!(s.memory.system_bit(1020));
                    assert!(!s.memory.system_bit(1021));
                    assert_eq!(s.memory.system_bit(1022), full || division);
                }
            }
        }
    }
}

#[test]
fn full_group_chunks_truncate_each_result_separately_at_every_width() {
    for (name, max_digits, low, high, args) in [
        ("MUL", 4, 0x5f90u64, 1u64, "K300 K300"),
        ("DIV", 4, 0xfffb, 0xfffe, "K-17 K3"),
        ("DMUL", 8, 0x540be400, 2, "K100000 K100000"),
        ("DDIV", 8, 0xfffffffb, 0xfffffffe, "K-17 K3"),
    ] {
        for digits in 1..=max_digits {
            let p = compile_il(&format!("LD X200\n{name} {args} K{digits}M128\nEND")).unwrap();
            let mut s = active();
            s.memory.set_internal(d("M127"), true);
            let end = at(d("M128"), 8 * digits);
            s.memory.set_internal(end, true);
            scan(&mut s, &p);
            let bits = 4 * digits;
            let mask = (1u64 << bits) - 1;
            assert_eq!(
                get(&s.memory, "M128", 2 * bits),
                (low & mask) | ((high & mask) << bits),
                "{name} K{digits}"
            );
            assert!(s.memory.bit(d("M127")) && s.memory.bit(end));
            for offset in 0..2 * bits {
                assert!(p.devices.contains(&at(d("M128"), offset)));
            }
        }
    }
}

#[test]
fn full_group_overlap_reads_both_sources_before_either_chunk_write() {
    for (name, digits, bits, expected) in [
        ("MUL", 4, 16, 90000u64),
        ("DIV", 4, 16, (2u64 << 16) | 5),
        ("DMUL", 8, 32, 10_000_000_000),
        ("DDIV", 8, 32, (2u64 << 32) | 5),
    ] {
        let second = at(d("M128"), bits);
        let p = compile_il(&format!(
            "LD X200\n{name} K{digits}M128 K{digits}{second} K{digits}M128\nEND"
        ))
        .unwrap();
        let mut s = active();
        let (a, b) = if name.contains("DIV") {
            (17, 3)
        } else if bits == 16 {
            (300, 300)
        } else {
            (100000, 100000)
        };
        put(&mut s.memory, "M128", bits, a);
        put(&mut s.memory, &second.to_string(), bits, b);
        scan(&mut s, &p);
        assert_eq!(get(&s.memory, "M128", 2 * bits), expected, "{name}");
    }
}

#[test]
fn zero_divisor_preserves_both_chunks_and_latches_exact_error_pc() {
    for (name, offset, full) in [
        ("DIV", 5, true),
        ("DDIV", 9, true),
        ("DIV16", 5, false),
        ("DIV32", 9, false),
    ] {
        let p = compile_il(&format!("LD X200\n{name} K17 K1M0 K2M128\nEND")).unwrap();
        let mut s = active();
        put(&mut s.memory, "M128", 24, 0xa1b2c3);
        scan(&mut s, &p);
        assert_eq!(get(&s.memory, "M128", 24), 0xa1b2c3);
        assert!(s.memory.system_bit(1067) && s.memory.system_bit(1068));
        assert_eq!(s.memory.word(d("D1067")), 0xe19);
        assert_eq!(s.memory.word(d("D1068")), 1 + offset);
        put(&mut s.memory, "M0", 4, 3);
        scan(&mut s, &p);
        assert_eq!(
            get(&s.memory, "M128", 24),
            if full { 0xa10205 } else { 0xa1b205 }
        );
        assert_eq!(s.memory.word(d("D1068")), 1 + offset);
    }
}

#[test]
fn truncated_carry_uses_full_product_and_group_error_rules() {
    for (name, a, b) in [("MUL16", "K300", "K300"), ("MUL32", "K100000", "K100000")] {
        for error in [None, Some(1067), Some(1068)] {
            for target in ["D0", "K1M128"] {
                let p = compile_il(&format!("LD X200\n{name} {a} {b} {target}\nEND")).unwrap();
                let mut s = active();
                if let Some(flag) = error {
                    s.memory.set_system(flag, true);
                }
                scan(&mut s, &p);
                assert_eq!(
                    s.memory.system_bit(1022),
                    name == "MUL32" || target.starts_with('K') || error.is_none()
                );
                assert!(s.memory.system_bit(1020));
                assert!(!s.memory.system_bit(1021));
                let fit = compile_il(&format!("LD X200\n{name} K3 K3 {target}\nEND")).unwrap();
                scan(&mut s, &fit);
                assert!(!s.memory.system_bit(1022));
            }
        }
    }
}

#[test]
fn e_sources_and_destinations_keep_canonical_aliases_and_role_restrictions() {
    for name in ["MUL", "DIV", "MUL16", "DIV16", "MUL32", "DIV32"] {
        let p = compile_il(&format!("LD X200\n{name} E1 K3 E1\nEND")).unwrap();
        let mut s = active();
        s.memory.set_double_word(d("D1182"), 17);
        s.memory.set_double_word(d("D182"), 123456);
        scan(&mut s, &p);
        let expected = if name == "DIV" {
            (2 << 16) | 5
        } else if name.contains("DIV") {
            5
        } else {
            51
        };
        assert_eq!(s.memory.double_word(d("D1182")), expected, "{name}");
        // MUL32/DIV32's installed simulator writes D182 instead; retain this
        // deliberate canonical-alias result and qualify native parity separately.
        assert_eq!(s.memory.double_word(d("D182")), 123456);
    }
    for name in ["MUL", "DIV"] {
        let p = compile_il(&format!("LD X200\n{name} F1 K3 E2\nEND")).unwrap();
        let mut s = active();
        s.memory.set_word(d("D1183"), 17);
        scan(&mut s, &p);
        assert_eq!(
            s.memory.double_word(d("D1184")),
            if name == "MUL" { 51 } else { (2 << 16) | 5 }
        );
    }
    for body in [
        "DMUL K1 K2 E1",
        "DDIV E1 K2 E1",
        "MUL K1 K2 F1",
        "DIV K1 K2 F1",
        "MUL16 F1 K2 D0",
        "DIV16 K1 F1 D0",
        "MUL16 K1 K2 F1",
        "DIV32 E1 K2 F1",
        "DMUL F1 K2 D0",
        "MUL K1 K2 K1X0",
    ] {
        assert!(
            compile_il(&format!("LD X200\n{body}\nEND")).is_err(),
            "{body}"
        );
    }
}

#[test]
fn complete_result_ranges_are_validated_and_labels_roundtrip() {
    for body in [
        "MUL K1 K2 K1M4092",
        "DMUL K1 K2 K8M4064",
        "DIV K1 K2 K1S1020",
        "DDIV K1 K2 C255",
        "MUL K1 K2 D9999",
        "DMUL K1 K2 D9997",
        "MUL K1 K2 T255",
    ] {
        assert!(
            compile_il(&format!("LD X200\n{body}\nEND")).is_err(),
            "{body}"
        );
    }
    for body in [
        "MULP K1X0 F7 K1M4088",
        "DIVP E1 K3 E7",
        "DMULP K8M0 E7 K8S960",
        "DDIVP E1 K3 C254",
        "MUL16P K4X0 E7 K4Y360",
        "DIV32P E7 K3 K8M4064",
    ] {
        let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
        let label = p.networks[0].actions[0].label();
        let restored = compile_il(&format!("LD X200\n{label}\nEND")).unwrap();
        assert_eq!(p.networks[0].actions, restored.networks[0].actions);
        assert_eq!(p.devices, restored.devices);
    }
}

#[test]
fn full_word_outputs_use_natural_counters_without_admitting_word_counter_sources() {
    for name in ["MUL", "DIV"] {
        for counter in [199, 200, 255] {
            let args = if name == "MUL" {
                "K300 K300"
            } else {
                "K-17 K3"
            };
            let p = compile_il(&format!("LD X200\n{name} {args} C{counter}\nEND")).unwrap();
            let mut s = active();
            s.memory.set_double_word(d("C200"), 0x12345678);
            s.memory.set_double_word(d("C201"), 0x3456789a);
            s.memory.set_double_word(d("C254"), 0x456789ab);
            scan(&mut s, &p);
            let value = if name == "MUL" {
                90000
            } else {
                0xfffefffbu32 as i32
            };
            if counter == 199 {
                assert_eq!(s.memory.word(d("C199")), value as i16);
                assert_eq!(s.memory.double_word(d("C200")), 0x12345678);
            } else {
                assert_eq!(s.memory.double_word(d(&format!("C{counter}"))), value);
            }
            assert_eq!(s.memory.double_word(d("C201")), 0x3456789a);
            assert_eq!(s.memory.double_word(d("C254")), 0x456789ab);
        }
        assert!(compile_il(&format!("LD X200\n{name} C200 K3 D0\nEND")).is_err());
    }
}

#[test]
fn wide_full_output_counter_transitions_match_independent_halfword_layouts() {
    for (name, args, low, high) in [
        ("DMUL", "K100000 K100000", 0x540be400u32 as i32, 2),
        ("DDIV", "K-17 K3", -5, -2),
    ] {
        for base in 197..=200 {
            let p = compile_il(&format!("LD X200\n{name} {args} C{base}\nEND")).unwrap();
            let mut s = active();
            for n in 197..=199 {
                s.memory.set_word(d(&format!("C{n}")), 77);
            }
            s.memory.set_double_word(d("C200"), 0x12345678);
            s.memory.set_double_word(d("C201"), 0x3456789a);
            s.memory.set_double_word(d("C202"), 0x456789ab);
            scan(&mut s, &p);
            match base {
                197 => {
                    assert_eq!(s.memory.word(d("C197")), low as i16);
                    assert_eq!(s.memory.word(d("C198")), (low >> 16) as i16);
                    assert_eq!(s.memory.word(d("C199")), high as i16);
                    assert_eq!(s.memory.double_word(d("C200")), 0x12345678);
                }
                198 => {
                    assert_eq!(s.memory.word(d("C198")), low as i16);
                    assert_eq!(s.memory.word(d("C199")), (low >> 16) as i16);
                    assert_eq!(s.memory.double_word(d("C200")), high);
                }
                199 => {
                    assert_eq!(s.memory.word(d("C199")), low as i16);
                    assert_eq!(s.memory.double_word(d("C200")), high);
                }
                _ => {
                    assert_eq!(s.memory.double_word(d("C200")), low);
                    assert_eq!(s.memory.double_word(d("C201")), high);
                }
            }
            if base != 200 {
                assert_eq!(s.memory.double_word(d("C201")), 0x3456789a);
            }
            assert_eq!(s.memory.double_word(d("C202")), 0x456789ab);
        }
    }
}
