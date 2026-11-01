//! DRAFT independent tests for local-only ASC destination modifiers.
//! Pending parent module/seam/VM admission; no Cargo/native run by author.
use crate::indexed_asc::IndexedAscAction;
use crate::plc::{Action, Area, Device, Memory, ScanStatus, Simulator, compile_il};
use crate::text_encoding::TextAction;
use std::collections::BTreeSet;

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn args(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(str::to_owned).collect()
}
fn a(raw: &str) -> IndexedAscAction {
    IndexedAscAction::parse("ASC", &args(raw)).unwrap()
}
fn words(m: &Memory, base: Device, count: u16) -> Vec<u16> {
    (0..count)
        .map(|i| {
            m.word(Device {
                index: base.index + i,
                ..base
            }) as u16
        })
        .collect()
}
fn put_words(m: &mut Memory, base: Device, count: u16, value: i16) {
    for i in 0..count {
        m.set_word(
            Device {
                index: base.index + i,
                ..base
            },
            value,
        );
    }
}
fn run(action: &IndexedAscAction, m: &mut Memory) -> BTreeSet<Device> {
    let mut seen = BTreeSet::new();
    action.execute(m, true, &mut seen);
    assert!(
        m.execution_limitations.is_empty(),
        "{:?}",
        m.execution_limitations
    );
    seen
}

#[test]
fn literal_forms_preserve_exact_case_and_only_the_last_argument_has_a_suffix() {
    let canonical = a("AbCdEfGh D100E1");
    for literal in ["AbCdEfGh", "\"AbCdEfGh\"", "'AbCdEfGh'", "A b C d E f G h"] {
        assert_eq!(a(&format!("{literal} D100E1")), canonical);
    }
    assert_eq!(canonical.label(), "ASC \"AbCdEfGh\" D100E1");
    assert!(!IndexedAscAction::has_modifiers(&args("ABCDEFGH D100")));
    assert!(!IndexedAscAction::has_modifiers(&args(
        "A B C D E F G H T20"
    )));
    assert!(IndexedAscAction::has_modifiers(&args("ABCDEFGH C20F7")));
    assert!(IndexedAscAction::supports("asc"));
}

#[test]
fn one_form_and_one_gray_tcd_destination_role_only() {
    for target in ["D0", "T0", "C0", "D9999E7", "T255F0", "C199E1"] {
        assert!(IndexedAscAction::parse("ASC", &args(&format!("ABCDEFGH {target}"))).is_ok());
    }
    for target in [
        "E0", "F7", "E0E1", "K0", "H1234", "X0", "Y0", "M0", "S0", "K4M0E0", "C200", "C200E0",
        "D0E8", "D0F8",
    ] {
        assert!(
            IndexedAscAction::parse("ASC", &args(&format!("ABCDEFGH {target}"))).is_err(),
            "{target}"
        );
    }
    for name in ["ASCP", "DASC", "DASCP", "ASCI"] {
        assert!(!IndexedAscAction::supports(name));
        assert!(IndexedAscAction::parse(name, &args("ABCDEFGH D0E0")).is_err());
    }
    for literal in [
        "ABCDEFG",
        "ABCDEFGHI",
        "ABCD1234",
        "ABC_DEFH",
        "ABCDEFé",
        "\"ABCDEFGH'",
        "\"ABCDEFGH",
    ] {
        assert!(
            IndexedAscAction::parse("ASC", &args(&format!("{literal} D0E0"))).is_err(),
            "{literal}"
        );
    }
    assert!(IndexedAscAction::parse("ASC", &args("ABCDEFGH")).is_err());
}

#[test]
fn manual_packed_and_low_byte_outputs_are_independent_literal_oracles() {
    for (mode, expected) in [
        (false, vec![0x4241, 0x4443, 0x4645, 0x4847]),
        (true, vec![0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48]),
    ] {
        let mut m = Memory::default();
        m.set_system(1161, mode);
        m.set_word(d("D1182"), 1);
        put_words(&mut m, d("D100"), 10, -1);
        let seen = run(&a("ABCDEFGH D100E1"), &mut m);
        assert_eq!(words(&m, d("D101"), expected.len() as u16), expected);
        assert_eq!(m.word(d("D100")), -1);
        assert_eq!(
            m.word(Device {
                area: Area::D,
                index: 101 + expected.len() as u16
            }),
            -1
        );
        assert!(seen.contains(&d("M1161")) && seen.contains(&d("D1182")));
        assert_eq!(seen.len(), expected.len() + 2);
    }
}

#[test]
fn all_sixteen_index_aliases_are_signed_words_and_ignore_the_neighbor() {
    for bank in ['E', 'F'] {
        for index in 0..8u16 {
            let alias = if index == 0 { 1028 } else { 1180 + index * 2 } + u16::from(bank == 'F');
            let mut m = Memory::default();
            m.set_word(
                Device {
                    area: Area::D,
                    index: alias,
                },
                -1,
            );
            m.set_word(
                Device {
                    area: Area::D,
                    index: alias + 1,
                },
                0x1234,
            );
            put_words(&mut m, d("D198"), 6, -1);
            let action = a(&format!("AbCdEfGh D200{bank}{index}"));
            run(&action, &mut m);
            assert_eq!(words(&m, d("D199"), 4), [0x6241, 0x6443, 0x6645, 0x6847]);
            assert_eq!(m.word(d("D198")), -1);
            assert_eq!(m.word(d("D203")), -1);
            assert_eq!(
                action.modifier_devices(),
                [Device {
                    area: Area::D,
                    index: alias
                }]
            );
        }
    }
}

#[test]
fn complete_timer_counter_endpoints_preserve_natural_counter_guard() {
    for (mode, count) in [(false, 4u16), (true, 8)] {
        for (area, limit) in [(Area::T, 255u16), (Area::C, 199)] {
            let base = Device {
                area,
                index: limit + 1 - count,
            };
            let mut m = Memory::default();
            m.set_system(1161, mode);
            m.set_double_word(d("C200"), 0x12345678);
            run(&a(&format!("ABCDEFGH {base}E0")), &mut m);
            assert_eq!(
                m.word(Device { area, index: limit }) as u16,
                if mode { 0x48 } else { 0x4847 }
            );
            assert_eq!(m.double_word(d("C200")), 0x12345678);
        }
    }
}

#[test]
fn effective_d_model_endpoints_use_actual_mode_width() {
    for last in [4999u16, 9999] {
        for (mode, count) in [(false, 4u16), (true, 8)] {
            let mut m = Memory::default();
            m.data_register_limit = Some(last);
            m.set_system(1161, mode);
            m.set_word(d("D1182"), -(count as i16 - 1));
            let seen = run(&a(&format!("ABCDEFGH D{last}E1")), &mut m);
            assert_eq!(
                m.word(Device {
                    area: Area::D,
                    index: last
                }) as u16,
                if mode { 0x48 } else { 0x4847 }
            );
            assert!(!seen.contains(&Device {
                area: Area::D,
                index: last + 1
            }));
        }
    }
}

#[test]
fn incomplete_spans_are_local_limitations_without_any_partial_write_or_e1a() {
    for target in ["D4997E0", "T253E0", "C197E0"] {
        let mut m = Memory::default();
        m.data_register_limit = Some(4999);
        let base: Device = target[..target.len() - 2].parse().unwrap();
        m.set_word(base, 0x1234);
        let mut seen = BTreeSet::new();
        a(&format!("ABCDEFGH {target}")).execute(&mut m, true, &mut seen);
        assert!(!m.execution_limitations.is_empty());
        assert_eq!(m.word(base), 0x1234);
        assert_eq!(seen, BTreeSet::from([d("M1161"), d("D1028")]));
        assert!(!m.system_bit(1067) && !m.system_bit(1068));
    }
    let mut m = Memory::default();
    m.data_register_limit = Some(4999);
    m.set_system(1161, true);
    m.set_word(d("D4996"), 77);
    a("ABCDEFGH D4996E0").execute(&mut m, true, &mut BTreeSet::new());
    assert_eq!(m.word(d("D4996")), 77);
    assert!(!m.execution_limitations.is_empty());
}

#[test]
fn invalid_effective_address_has_no_write_and_off_does_not_read_mode_or_index() {
    for index in [-101i16, i16::MAX] {
        let mut m = Memory::default();
        m.set_word(d("D1182"), index);
        m.set_word(d("D100"), 55);
        let mut seen = BTreeSet::new();
        a("ABCDEFGH D100E1").execute(&mut m, false, &mut seen);
        assert!(seen.is_empty() && m.execution_limitations.is_empty());
        a("ABCDEFGH D100E1").execute(&mut m, true, &mut seen);
        assert!(!m.execution_limitations.is_empty());
        assert_eq!(m.word(d("D100")), 55);
        assert_eq!(seen, BTreeSet::from([d("M1161"), d("D1182")]));
    }
}

#[test]
fn own_index_destination_is_cached_across_all_four_or_eight_writes() {
    for mode in [false, true] {
        let mut m = Memory::default();
        m.set_system(1161, mode);
        m.set_word(d("D1182"), 0);
        m.set_word(d("D1190"), 777);
        run(&a("ABCDEFGH D1182E1"), &mut m);
        assert_eq!(m.word(d("D1182")) as u16, if mode { 0x41 } else { 0x4241 });
        assert_eq!(
            m.word(d(if mode { "D1189" } else { "D1185" })) as u16,
            if mode { 0x48 } else { 0x4847 }
        );
        assert_eq!(m.word(d("D1190")), 777);
    }
}

#[test]
fn prepared_result_freezes_mode_and_target_without_mutating_later_control_input() {
    let action = a("ABCDEFGH D100E1");
    let mut m = Memory::default();
    let prepared = action.prepare(&m, &mut BTreeSet::new()).unwrap();
    m.set_system(1161, true);
    m.set_word(d("D1182"), 5);
    m.set_word(d("D104"), 999);
    m.set_word(d("D105"), 888);
    prepared.write(&mut m);
    assert_eq!(words(&m, d("D100"), 4), [0x4241, 0x4443, 0x4645, 0x4847]);
    assert_eq!(m.word(d("D104")), 999);
    assert_eq!(m.word(d("D105")), 888);
    assert!(m.system_bit(1161));
    assert_eq!(m.word(d("D1182")), 5);
}

#[test]
fn flags_errors_and_shared_scratch_are_unchanged_by_literal_output() {
    let mut m = Memory::default();
    for (bit, value) in [
        (1020, true),
        (1021, false),
        (1022, true),
        (1067, true),
        (1068, true),
    ] {
        m.set_system(bit, value);
    }
    m.set_word(d("D1067"), 0xe19u16 as i16);
    m.set_word(d("D1068"), 123);
    m.native_numeric_scratch = Some(-77);
    run(&a("ABCDEFGH D100F7"), &mut m);
    assert_eq!(
        [1020, 1021, 1022, 1067, 1068].map(|i| m.system_bit(i)),
        [true, false, true, true, true]
    );
    assert_eq!(m.word(d("D1067")) as u16, 0xe19);
    assert_eq!(m.word(d("D1068")), 123);
    assert_eq!(m.native_numeric_scratch, Some(-77));
}

#[test]
fn static_inventory_does_not_invent_maximum_width_and_monitor_is_mode_sensitive() {
    let action = a("ABCDEFGH D4996E1");
    let mut static_ds = BTreeSet::new();
    action.devices(&mut static_ds);
    assert_eq!(static_ds, BTreeSet::from([d("D1182"), d("M1161")]));
    let mut m = Memory::default();
    m.data_register_limit = Some(4999);
    let candidates = action.capture_devices(&m).unwrap();
    assert!(candidates.contains(&d("D4999")) && !candidates.contains(&d("D5000")));
    m.set_system(1161, true);
    assert!(action.capture_devices(&m).is_err());
    let action = a("ABCDEFGH D100E1");
    m.set_word(d("D1182"), 1);
    run(&action, &mut m);
    assert_eq!(
        action.display_value(&m),
        "0041 0042 0043 0044 0045 0046 0047 0048"
    );
    assert_eq!(
        action.monitor_input_devices(&m).unwrap(),
        BTreeSet::from([d("D1182"), d("M1161")])
    );
}

#[test]
fn typed_legacy_adapter_preserves_bytes_and_shared_execution_on_valid_spans() {
    for mode in [false, true] {
        for target in ["D100", "T20", "C20"] {
            let old = TextAction::parse("ASC", &args(&format!("AbCdEfGh {target}"))).unwrap();
            let new = IndexedAscAction::from_legacy(&old).unwrap();
            let mut ordinary = Memory::default();
            ordinary.set_system(1161, mode);
            let mut addressed = ordinary.clone();
            old.execute(&mut ordinary);
            run(&new, &mut addressed);
            assert_eq!(
                words(&ordinary, d(target), 8),
                words(&addressed, d(target), 8)
            );
            assert_eq!(new.label(), old.label());
        }
    }
    let wrong = TextAction::parse("SEGD", &args("K0 D100")).unwrap();
    assert!(IndexedAscAction::from_legacy(&wrong).is_err());
    let malformed = TextAction::Literal {
        letters: *b"ABCD1234",
        dest: d("D100"),
    };
    assert!(IndexedAscAction::from_legacy(&malformed).is_err());
}

#[test]
fn local_layout_keeps_eleven_unknown_words_and_never_exports_stub_as_computing() {
    let action = a("ABCDEFGH D100E1");
    assert_eq!(action.nominal_words(), 11);
    assert!(action.native_words().is_err());
    let p = compile_il("LD X0\nASC ABCDEFGH D100E1\nINC D300\nEND").unwrap();
    assert!(p.requires_vm());
    assert!(crate::es2_encoder::encode(&p).is_err());
    let layout = crate::es2_encoder::local_execution_layout(&p).unwrap();
    assert_eq!(layout.action_pc, [1, 12]);
    assert_eq!(layout.unencoded_ranges[&1].word_count, 11);
    for pc in 1..12 {
        assert_eq!(layout.known_word_at(pc), None);
    }
    assert!(layout.known_word_at(12).is_some());
    assert!(layout.execution_plan.operand_tail_owners.is_empty());
}

#[test]
fn public_parser_never_invents_ascp_and_manual_pulse_is_rejected_locally_too() {
    assert!(compile_il("LD X0\nASCP ABCDEFGH D100E1\nEND").is_err());
    let mut p = compile_il("LD X0\nASC ABCDEFGH D100E1\nEND").unwrap();
    let action = p.networks[0].actions[0].clone();
    p.networks[0].actions[0] = Action::Pulse(Box::new(action));
    assert!(crate::es2_encoder::local_execution_layout(&p).is_err());
    assert!(crate::es2_encoder::encode(&p).is_err());
}

#[test]
fn public_vm_repeats_each_powered_visit_with_fresh_mode_and_index() {
    let p = compile_il("LD X0\nASC AbCdEfGh D100E1\nINC D300\nEND").unwrap();
    let mut sim = Simulator::default();
    for (power, index, mode, counter) in [
        (false, 0, false, 0),
        (true, 0, false, 1),
        (true, 10, true, 2),
        (false, 20, false, 2),
        (true, 20, false, 3),
    ] {
        sim.memory.set_input(d("X0"), power);
        sim.memory.set_word(d("D1182"), index);
        sim.memory.set_system(1161, mode);
        sim.scan(&p, 0);
        assert_eq!(sim.last_scan, ScanStatus::Complete);
        assert_eq!(sim.memory.word(d("D300")), counter);
        if power {
            assert_eq!(
                sim.memory.word(Device {
                    area: Area::D,
                    index: 100 + index as u16
                }) as u16,
                if mode { 0x41 } else { 0x6241 }
            );
        }
    }
    assert_eq!(
        words(&sim.memory, d("D100"), 4),
        [0x6241, 0x6443, 0x6645, 0x6847]
    );
    assert_eq!(
        words(&sim.memory, d("D110"), 8),
        [0x41, 0x62, 0x43, 0x64, 0x45, 0x66, 0x47, 0x68]
    );
}

#[test]
fn public_vm_same_scan_index_mode_updates_and_following_instruction_use_real_span() {
    let p = compile_il("LD X0\nMOV K1 E1\nRST M1161\nASC ABCDEFGH D100E1\nMOV K10 E1\nSET M1161\nASC abcdefgh D100E1\nMOV K99 D300\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim.scan(&p, 0);
    assert_eq!(sim.last_scan, ScanStatus::Complete);
    assert_eq!(
        words(&sim.memory, d("D101"), 4),
        [0x4241, 0x4443, 0x4645, 0x4847]
    );
    assert_eq!(
        words(&sim.memory, d("D110"), 8),
        [0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68]
    );
    assert_eq!(sim.memory.word(d("D300")), 99);
}

#[test]
fn public_vm_master_false_skips_invalid_target_and_restores_following_actions() {
    let p = compile_il("LD X1\nMC N0\nLDI X7\nASC ABCDEFGH D0E1\nMCR N0\nLD X0\nMOV K99 D300\nEND")
        .unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim.memory.set_word(d("D1182"), -1);
    sim.scan(&p, 0);
    assert_eq!(sim.last_scan, ScanStatus::Complete);
    assert_eq!(sim.memory.word(d("D300")), 99);
    assert!(sim.memory.execution_limitations.is_empty());
    assert!(!sim.observed_devices.contains(&d("D1182")));
}
