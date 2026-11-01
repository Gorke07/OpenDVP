//! DRAFT: independent rotation vectors and addressed integration contracts.
//! Requires root admission of indexed_rotation and the unchanged shift kernel
//! extraction. No native or Cargo execution is claimed by this draft.
use crate::indexed_rotation::IndexedRotationAction;
use crate::plc::{Device, Memory, ScanStatus, Simulator, compile_il};
use std::collections::BTreeSet;

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn args(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(str::to_owned).collect()
}
fn action(body: &str) -> IndexedRotationAction {
    let words = args(body);
    IndexedRotationAction::parse(&words[0], &words[1..]).unwrap()
}
fn run(body: &str, memory: &mut Memory) -> BTreeSet<Device> {
    let mut observed = BTreeSet::new();
    action(body).execute(memory, true, &mut observed);
    assert!(
        memory.execution_limitations.is_empty(),
        "{body}: {:?}",
        memory.execution_limitations
    );
    observed
}
fn flags(memory: &Memory) -> [bool; 3] {
    [1020, 1021, 1022].map(|n| memory.system_bit(n))
}
fn seed_flags(memory: &mut Memory, carry: bool) {
    memory.set_system(1020, true);
    memory.set_system(1021, false);
    memory.set_system(1022, carry);
}
fn bits(memory: &Memory, base: Device, width: u16) -> u32 {
    (0..width).fold(0, |v, bit| {
        v | (u32::from(memory.bit(Device {
            index: base.index + bit,
            ..base
        })) << bit)
    })
}
fn set_bits(memory: &mut Memory, base: Device, width: u16, value: u32) {
    for bit in 0..width {
        memory.set_internal(
            Device {
                index: base.index + bit,
                ..base
            },
            value & (1 << bit) != 0,
        );
    }
}
fn complete(sim: &mut Simulator, program: &crate::plc::Program) {
    sim.scan(program, 0);
    assert_eq!(sim.last_scan, ScanStatus::Complete);
}
const OPS: [(&str, u16, bool); 8] = [
    ("ROR", 0x4c00, false),
    ("ROL", 0x4e00, false),
    ("RCR", 0x5000, false),
    ("RCL", 0x5200, false),
    ("DROR", 0x4d00, true),
    ("DROL", 0x4f00, true),
    ("DRCR", 0x5100, true),
    ("DRCL", 0x5300, true),
];
const E_ALIAS: [u16; 8] = [1028, 1182, 1184, 1186, 1188, 1190, 1192, 1194];
const F_ALIAS: [u16; 8] = [1029, 1183, 1185, 1187, 1189, 1191, 1193, 1195];

#[test]
fn all_sixteen_spellings_have_literal_original_fields_and_exact_lengths() {
    for (name, header, wide) in OPS {
        for pulse in [false, true] {
            let body = format!("{name}{} D20E0 K1", if pulse { "P" } else { "" });
            let a = action(&body);
            let mut expected = vec![header | if pulse { 0x10 } else { 0 }, 0x28e6, 0x0086];
            if wide {
                expected.extend([0x0080, 0x0080]);
            }
            expected.extend([0x0180, 0x0080]);
            if wide {
                expected.extend([0x0080, 0x0080]);
            }
            assert_eq!(a.native_words().unwrap(), expected, "{body}");
            assert_eq!(expected.len(), if wide { 9 } else { 5 });
            assert_eq!(a.label(), body);
            assert_eq!(a.source_count(), 1);
            assert_eq!(a.source_modifier_devices(0), [d("D1028")]);
            assert!(a.source_modifier_devices(1).is_empty());
            assert!(a.resolved_source(1, &Memory::default()).is_err());
            assert_eq!(a.target_modifier_devices(), a.modifier_devices());
        }
    }
}

#[test]
fn group_timer_counter_and_bank_descriptors_are_independent_literals() {
    for (body, expected) in [
        (
            "ROR K4Y20E1 H10",
            vec![0x4c00, 0x10e4, 0x2588, 0x1080, 0x0080],
        ),
        (
            "DROL K8M128F7 K32",
            vec![
                0x4f00, 0x80f4, 0xe890, 0x0080, 0x0080, 0x2080, 0x0080, 0x0080, 0x0080,
            ],
        ),
        (
            "RCR K4S128E0 K1",
            vec![0x5000, 0x80e4, 0x0088, 0x0180, 0x0080],
        ),
        ("RCL T20F2 K1", vec![0x5200, 0x28f6, 0x4082, 0x0180, 0x0080]),
        (
            "DRCR C199E1 K1",
            vec![
                0x5100, 0x8ee6, 0x2184, 0x0080, 0x0080, 0x0180, 0x0080, 0x0080, 0x0080,
            ],
        ),
    ] {
        assert_eq!(action(body).native_words().unwrap(), expected, "{body}");
    }
}

#[test]
fn all_sixteen_modifier_aliases_are_signed_words_even_for_double_rotation() {
    for (prefix, aliases) in [('E', E_ALIAS), ('F', F_ALIAS)] {
        for (bank, alias) in aliases.into_iter().enumerate() {
            for wide in [false, true] {
                let mut m = Memory::default();
                m.set_word(d(&format!("D{alias}")), -2);
                // An unrelated following high word must not turn the index into a DWORD.
                m.set_word(d(&format!("D{}", alias + 1)), 0x4567);
                m.set_word(d("D38"), 0x8001u16 as i16);
                m.set_word(d("D39"), 0x8000u16 as i16);
                m.set_word(d("D40"), 0x1234);
                seed_flags(&mut m, false);
                let body = format!("{}ROR D40{prefix}{bank} K1", if wide { "D" } else { "" });
                let a = action(&body);
                let source = a.resolved_source(0, &m).unwrap();
                assert_eq!(source.base(), Some(d("D38")));
                let observed = run(&body, &mut m);
                if wide {
                    assert_eq!(m.double_word(d("D38")) as u32, 0xc000_4000);
                    assert!(observed.contains(&d("D39")));
                } else {
                    assert_eq!(m.word(d("D38")) as u16, 0xc000);
                    assert_eq!(m.word(d("D39")) as u16, 0x8000);
                    assert!(!observed.contains(&d("D39")));
                }
                assert_eq!(m.word(d("D40")), 0x1234);
                assert_eq!(flags(&m), [true, false, true]);
                assert!(observed.contains(&d(&format!("D{alias}"))));
            }
        }
    }
}

#[test]
fn parser_preserves_full_groups_constant_counts_and_direct_index_roles() {
    for wide in [false, true] {
        let name = if wide { "DROR" } else { "ROR" };
        let width = if wide { 32 } else { 16 };
        let full = if wide { 8 } else { 4 };
        for area in ["Y20", "M128", "S128"] {
            assert!(
                IndexedRotationAction::parse(name, &args(&format!("K{full}{area}F7 K{width}")))
                    .is_ok()
            );
            for short in 1..full {
                assert!(
                    IndexedRotationAction::parse(name, &args(&format!("K{short}{area}F7 K1")))
                        .is_err()
                );
            }
        }
        for n in ["K0", "K-1", "HFFFF", "D0", "E0", "K1E0", "K1M0"] {
            assert!(
                IndexedRotationAction::parse(name, &args(&format!("D20E0 {n}"))).is_err(),
                "{name} {n}"
            );
        }
        assert!(
            IndexedRotationAction::parse(name, &args(&format!("D20E0 K{}", width + 1))).is_err()
        );
        for target in ["K1", "H1234", "M128E0", "K4X0E0", "E1E0", "F1F0"] {
            assert!(
                IndexedRotationAction::parse(name, &args(&format!("{target} K1"))).is_err(),
                "{name} {target}"
            );
        }
    }
    for body in ["ROR E0 K1", "ROR F7 K16", "DROR E7 K32"] {
        let a = action(body);
        assert!(!IndexedRotationAction::has_modifiers(&args(body)[1..]));
        assert!(a.qualify_target(&Memory::default()).is_ok());
    }
    assert!(IndexedRotationAction::parse("DROR", &args("F0 K1")).is_err());
    assert!(IndexedRotationAction::parse("ROR", &args("C200E0 K1")).is_err());
    assert!(!IndexedRotationAction::supports("RORPP"));
    assert!(IndexedRotationAction::parse("ROR", &[]).is_err());
}

#[test]
fn one_bit_literal_results_distinguish_rotation_from_carry_rotation() {
    for (name, wide, carry, expected) in [
        ("ROR", false, false, 0xc000),
        ("ROL", false, false, 3),
        ("RCR", false, false, 0x4000),
        ("RCL", false, false, 2),
        ("RCR", false, true, 0xc000),
        ("RCL", false, true, 3),
        ("DROR", true, false, 0xc000_0000),
        ("DROL", true, false, 3),
        ("DRCR", true, false, 0x4000_0000),
        ("DRCL", true, false, 2),
        ("DRCR", true, true, 0xc000_0000),
        ("DRCL", true, true, 3),
    ] {
        let mut m = Memory::default();
        seed_flags(&mut m, carry);
        if wide {
            m.set_double_word(d("D20"), 0x8000_0001u32 as i32);
        } else {
            m.set_word(d("D20"), 0x8001u16 as i16);
        }
        run(&format!("{name} D20E0 K1"), &mut m);
        let value = if wide {
            m.double_word(d("D20")) as u32
        } else {
            u32::from(m.word(d("D20")) as u16)
        };
        assert_eq!(value, expected, "{name}, carry={carry}");
        assert_eq!(flags(&m), [true, false, true]);
    }
}

#[test]
fn maximum_count_is_width_for_plain_rotation_but_not_the_carry_ring() {
    for (wide, plain, right, left, seed, right_value, left_value) in [
        (false, "ROR", "RCR", "RCL", 0x8001u32, 3u32, 0xc000u32),
        (true, "DROR", "DRCR", "DRCL", 0x8000_0001, 3, 0xc000_0000),
    ] {
        let n = if wide { 32 } else { 16 };
        for (name, expected) in [(plain, seed), (right, right_value), (left, left_value)] {
            let mut m = Memory::default();
            seed_flags(&mut m, true);
            if wide {
                m.set_double_word(d("D20"), seed as i32);
            } else {
                m.set_word(d("D20"), seed as i16);
            }
            run(&format!("{name} D20E0 K{n}"), &mut m);
            let actual = if wide {
                m.double_word(d("D20")) as u32
            } else {
                u32::from(m.word(d("D20")) as u16)
            };
            assert_eq!(actual, expected, "{name}");
            assert_eq!(flags(&m), [true, false, true]);
        }
    }
}

#[test]
fn carry_rotations_follow_an_independent_closed_ring_for_each_legal_count() {
    for (wide, width) in [(false, 16u32), (true, 32)] {
        let data_mask = (1u64 << width) - 1;
        let ring_mask = (1u64 << (width + 1)) - 1;
        for raw in [0u32, 1, 0x8765_4321, u32::MAX] {
            for carry in [false, true] {
                for left in [false, true] {
                    for n in 1..=width {
                        let ring = (u64::from(carry) << width) | (u64::from(raw) & data_mask);
                        let expected = if left {
                            ((ring << n) | (ring >> (width + 1 - n))) & ring_mask
                        } else {
                            ((ring >> n) | (ring << (width + 1 - n))) & ring_mask
                        };
                        let mut m = Memory::default();
                        seed_flags(&mut m, carry);
                        if wide {
                            m.set_double_word(d("D20"), raw as i32);
                        } else {
                            m.set_word(d("D20"), raw as i16);
                        }
                        let body = format!(
                            "{}{} D20E0 K{n}",
                            if wide { "D" } else { "" },
                            if left { "RCL" } else { "RCR" }
                        );
                        run(&body, &mut m);
                        let actual = if wide {
                            u64::from(m.double_word(d("D20")) as u32)
                        } else {
                            u64::from(m.word(d("D20")) as u16)
                        };
                        assert_eq!(actual, expected & data_mask, "{body}");
                        assert_eq!(flags(&m), [true, false, expected >> width != 0]);
                    }
                }
            }
        }
    }
}

#[test]
fn full_group_stores_preserve_adjacent_bits_in_each_writable_area() {
    for area in ["Y", "M", "S"] {
        let base = d(&format!(
            "{area}{}",
            if area == "Y" { "200" } else { "128" }
        ));
        for wide in [false, true] {
            let width = if wide { 32 } else { 16 };
            let raw = if wide { 0x8000_0001 } else { 0x8001 };
            let mut m = Memory::default();
            seed_flags(&mut m, false);
            set_bits(&mut m, base, width, raw);
            let before = Device {
                index: base.index - 1,
                ..base
            };
            let after = Device {
                index: base.index + width,
                ..base
            };
            m.set_internal(before, true);
            m.set_internal(after, true);
            let body = format!(
                "{}RCL K{}{base}E0 K1",
                if wide { "D" } else { "" },
                if wide { 8 } else { 4 }
            );
            let observed = run(&body, &mut m);
            assert_eq!(bits(&m, base, width), 2);
            assert!(m.bit(before) && m.bit(after));
            assert!(!observed.contains(&before) && !observed.contains(&after));
            assert_eq!(flags(&m), [true, false, true]);
        }
    }
}

#[test]
fn natural_counter_and_c199_special_widths_survive_the_snapshot_and_store() {
    let mut m = Memory::default();
    seed_flags(&mut m, false);
    m.set_word(d("C199"), -1);
    m.set_double_word(d("C200"), 0x1234_5678);
    let observed = run("DROR C199E0 K1", &mut m);
    assert_eq!(m.word(d("C199")) as u16, 0x7fff);
    assert_eq!(m.double_word(d("C200")), 0x1234_5678);
    assert!(observed.contains(&d("C199")) && !observed.contains(&d("C200")));
    assert_eq!(flags(&m), [true, false, true]);
    for counter in [200, 255] {
        m.set_double_word(d(&format!("C{counter}")), 0x8000_0001u32 as i32);
        run(&format!("DROR C{counter}E0 K1"), &mut m);
        assert_eq!(m.double_word(d(&format!("C{counter}"))) as u32, 0xc000_0000);
    }
    m.set_word(d("D1028"), 1);
    m.set_double_word(d("C200"), 0x8000_0001u32 as i32);
    run("DROL C199E0 K1", &mut m);
    assert_eq!(m.double_word(d("C200")), 3);
    m.set_word(d("D1028"), -1);
    m.set_word(d("C199"), -1);
    run("DROL C200E0 K1", &mut m);
    assert_eq!(m.word(d("C199")) as u16, 0xfffe);
    assert_eq!(m.double_word(d("C200")), 3);
}

#[test]
fn original_endpoint_and_actual_model_pair_limits_are_separate() {
    for last in [4999, 9999] {
        let mut m = Memory::default();
        m.data_register_limit = Some(last);
        seed_flags(&mut m, true);
        m.set_word(d(&format!("D{last}")), 3);
        run(&format!("ROR D{last}E0 K1"), &mut m);
        assert_eq!(m.word(d(&format!("D{last}"))) as u16, 0x8001);
        let wide = action(&format!("DROR D{last}E0 K1"));
        assert!(wide.resolved_source(0, &m).is_err());
        assert!(wide.qualify_target(&m).is_err());
        m.set_word(d("D1028"), -1);
        m.set_double_word(d(&format!("D{}", last - 1)), 0x8000_0001u32 as i32);
        run(&format!("DROR D{last}E0 K1"), &mut m);
        assert_eq!(
            m.double_word(d(&format!("D{}", last - 1))) as u32,
            0xc000_0000
        );
    }
    let mut m = Memory::default();
    assert!(action("DROR T255E0 K1").resolved_source(0, &m).is_err());
    m.set_word(d("D1028"), -1);
    m.set_double_word(d("T254"), 0x8000_0001u32 as i32);
    run("DROR T255E0 K1", &mut m);
    assert_eq!(m.double_word(d("T254")) as u32, 0xc000_0000);
}

#[test]
fn pair_store_caches_every_self_index_alias_after_the_low_word_changes_it() {
    for (prefix, aliases) in [('E', E_ALIAS), ('F', F_ALIAS)] {
        for (bank, alias) in aliases.into_iter().enumerate() {
            let mut m = Memory::default();
            seed_flags(&mut m, false);
            m.set_word(d(&format!("D{alias}")), 0);
            m.set_word(d(&format!("D{}", alias + 1)), 0x8001u16 as i16);
            m.set_word(d(&format!("D{}", alias + 2)), 0x2345);
            run(&format!("DROR D{alias}{prefix}{bank} K1"), &mut m);
            assert_eq!(m.word(d(&format!("D{alias}"))) as u16, 0x8000);
            assert_eq!(m.word(d(&format!("D{}", alias + 1))) as u16, 0x4000);
            assert_eq!(m.word(d(&format!("D{}", alias + 2))), 0x2345);
            assert_eq!(flags(&m), [true, false, false]);
        }
    }
}

#[test]
fn prepare_is_pure_and_only_reads_carry_for_the_carry_ring_variants() {
    for (body, carry_read) in [("ROR D20E0 K1", false), ("RCR D20E0 K1", true)] {
        let mut m = Memory::default();
        seed_flags(&mut m, true);
        m.set_word(d("D20"), 2);
        let a = action(body);
        let mut observed = BTreeSet::new();
        let _prepared = a.prepare(&m, &mut observed).unwrap();
        a.qualify_target(&m).unwrap();
        assert_eq!(m.word(d("D20")), 2);
        assert_eq!(flags(&m), [true, false, true]);
        assert!(m.execution_limitations.is_empty());
        assert!(observed.contains(&d("D1028")) && observed.contains(&d("D20")));
        assert_eq!(observed.contains(&d("M1022")), carry_read);
        a.execute(&mut m, true, &mut observed);
        assert_eq!(m.word(d("D20")) as u16, if carry_read { 0x8001 } else { 1 });
        assert_eq!(flags(&m), [true, false, false]);
        assert!(observed.contains(&d("M1022")));
    }
}

#[test]
fn unsupported_spans_fail_without_inventing_a_plc_fault_or_widening_full_groups() {
    for body in [
        "ROR K4M1284E0 K1",
        "DROR K8M1520E0 K1",
        "ROR K4M1011E0 K1",
        "DROR K8M1011E0 K1",
        "ROR K4M1022E0 K1",
        "DROR K8M1022E0 K1",
        "ROR K4Y377E0 K1",
        "DROR D9999E0 K1",
    ] {
        let mut m = Memory::default();
        seed_flags(&mut m, true);
        let a = action(body);
        a.execute(&mut m, true, &mut BTreeSet::new());
        assert!(!m.execution_limitations.is_empty(), "{body}");
        assert_eq!(flags(&m), [true, false, true]);
        assert_eq!(m.word(d("D1067")), 0);
        assert!(!m.system_bit(1067) && !m.system_bit(1068));
    }
    // A complete K4/K8 group covering M1022 cannot pass the present special-M
    // whitelist/readonly rules. Carry-before-store is not advertised as a
    // positive public flag-overlap fixture by silently admitting short groups.
}

#[test]
fn false_and_invalid_source_visits_skip_carry_and_target_observation() {
    for (name, _, _) in OPS {
        let mut m = Memory::default();
        seed_flags(&mut m, true);
        m.set_word(d("D1028"), -30_000);
        m.set_double_word(d("D20"), 0x1234_5678);
        let a = action(&format!("{name} D20E0 K1"));
        let mut observed = BTreeSet::new();
        a.execute(&mut m, false, &mut observed);
        assert!(observed.is_empty() && m.execution_limitations.is_empty());
        assert_eq!(flags(&m), [true, false, true]);
        assert!(a.prepare(&m, &mut observed).is_err());
        assert!(observed.contains(&d("D1028")));
        assert!(!observed.contains(&d("D20")) && !observed.contains(&d("M1022")));
        assert_eq!(m.double_word(d("D20")), 0x1234_5678);
    }
}

#[test]
fn compiled_same_scan_index_changes_and_instruction_lengths_reach_the_marker() {
    let p =
        compile_il("LD X0\nMOV K2 E0\nROR D20E0 K1\nMOV K4 E0\nDROR D20E0 K1\nMOV K321 D300\nEND")
            .unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim.memory.set_word(d("D20"), 0x1234);
    sim.memory.set_word(d("D22"), 0x8001u16 as i16);
    sim.memory.set_double_word(d("D24"), 0x8000_0001u32 as i32);
    seed_flags(&mut sim.memory, false);
    complete(&mut sim, &p);
    assert_eq!(sim.memory.word(d("D20")), 0x1234);
    assert_eq!(sim.memory.word(d("D22")) as u16, 0xc000);
    assert_eq!(sim.memory.double_word(d("D24")) as u32, 0xc000_0000);
    assert_eq!(sim.memory.word(d("D300")), 321);
    assert_eq!(flags(&sim.memory), [true, false, true]);
    assert!(sim.observed_devices.contains(&d("D22")) && sim.observed_devices.contains(&d("D25")));
}

#[test]
fn all_normal_and_pulse_forms_gate_rearm_and_hold_original_descriptor_identity() {
    for (name, _, wide) in OPS {
        // Independent first/second/third one-bit results for initial 0x8001
        // (or 0x80000001) and an initially clear carry. Every first visit sets
        // carry; the second and third clear it for these literal vectors.
        let values: [u32; 3] = match name {
            "ROR" => [0xc000, 0x6000, 0x3000],
            "ROL" => [3, 6, 12],
            "RCR" => [0x4000, 0xa000, 0x5000],
            "RCL" => [2, 5, 10],
            "DROR" => [0xc000_0000, 0x6000_0000, 0x3000_0000],
            "DROL" => [3, 6, 12],
            "DRCR" => [0x4000_0000, 0xa000_0000, 0x5000_0000],
            "DRCL" => [2, 5, 10],
            _ => unreachable!(),
        };
        for pulse in [false, true] {
            let p = compile_il(&format!(
                "LD X0\n{name}{} D20E0 K1\nLDI X7\nMOV K321 D300\nEND",
                if pulse { "P" } else { "" }
            ))
            .unwrap();
            let mut sim = Simulator::default();
            if wide {
                sim.memory.set_double_word(d("D20"), 0x8000_0001u32 as i32);
            } else {
                sim.memory.set_word(d("D20"), 0x8001u16 as i16);
            }
            seed_flags(&mut sim.memory, false);
            sim.memory.set_word(d("D1028"), -30_000);
            complete(&mut sim, &p); // OFF never resolves the invalid modifier.
            assert!(sim.memory.execution_limitations.is_empty());
            assert!(!sim.observed_devices.contains(&d("D1028")));
            sim.memory.set_word(d("D1028"), 0);
            sim.memory.set_input(d("X0"), true);
            complete(&mut sim, &p);
            let first = if wide {
                sim.memory.double_word(d("D20"))
            } else {
                i32::from(sim.memory.word(d("D20")))
            };
            let first_carry = sim.memory.system_bit(1022);
            assert_eq!(
                if wide {
                    first as u32
                } else {
                    u32::from(first as u16)
                },
                values[0]
            );
            assert!(first_carry);
            if pulse {
                sim.memory.set_word(d("D1028"), -30_000);
                sim.observed_devices.clear();
                complete(&mut sim, &p); // Held P does not resolve.
                assert!(sim.memory.execution_limitations.is_empty());
                assert!(!sim.observed_devices.contains(&d("D1028")));
                let held = if wide {
                    sim.memory.double_word(d("D20"))
                } else {
                    i32::from(sim.memory.word(d("D20")))
                };
                assert_eq!(held, first);
                assert_eq!(sim.memory.system_bit(1022), first_carry);
            } else {
                complete(&mut sim, &p);
            }
            sim.memory.set_input(d("X0"), false);
            complete(&mut sim, &p);
            sim.memory.set_word(d("D1028"), 0);
            sim.memory.set_input(d("X0"), true);
            complete(&mut sim, &p);
            let rearmed = if wide {
                sim.memory.double_word(d("D20")) as u32
            } else {
                u32::from(sim.memory.word(d("D20")) as u16)
            };
            assert_eq!(rearmed, values[if pulse { 1 } else { 2 }]);
            assert!(!sim.memory.system_bit(1022));
            assert!(sim.observed_devices.contains(&d("D20")));
            assert_eq!(sim.memory.word(d("D300")), 321);
            assert_eq!(
                [sim.memory.system_bit(1020), sim.memory.system_bit(1021)],
                [true, false]
            );
            assert!(sim.memory.execution_limitations.is_empty());
        }
    }
}

#[test]
fn monitor_uses_resolved_value_not_the_label_base_and_retains_context() {
    let a = action("DRCR D20E1 H1");
    assert_eq!(a.label(), "DRCR D20E1 K1");
    let mut m = Memory::default();
    m.set_word(d("D1182"), 2);
    m.set_double_word(d("D20"), 123);
    m.set_double_word(d("D22"), -456);
    m.set_system(1022, true);
    assert_eq!(a.display_value(&m), "-456 C:1");
    let mut static_devices = BTreeSet::new();
    a.devices(&mut static_devices);
    assert!(static_devices.contains(&d("D1182")) && !static_devices.contains(&d("D20")));
    let mut held = BTreeSet::new();
    a.observed_devices(&m, &mut held);
    assert!(held.contains(&d("D22")) && held.contains(&d("D23")));
    assert!(!held.contains(&d("D20")));
    m.set_word(d("D1182"), -30_000);
    assert!(a.display_value(&m).starts_with("İndeks:"));
}
