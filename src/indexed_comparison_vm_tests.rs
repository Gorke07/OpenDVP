//! Independent end-to-end checks for dynamic comparisons and control flow.
use crate::execution_plan::PlanOp;
use crate::plc::{Action, Device, Expr, ScanStatus, Simulator, compile_il};

fn d(name: &str) -> Device {
    name.parse().unwrap()
}
fn enabled() -> Simulator {
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim
}
fn scan(sim: &mut Simulator, source: &str) {
    let program = compile_il(source).unwrap();
    sim.scan(&program, 0);
    assert_eq!(sim.last_scan, ScanStatus::Complete, "{source}");
}

#[test]
fn all_eight_actions_write_only_three_live_target_bits() {
    for name in [
        "CMP", "CMPP", "DCMP", "DCMPP", "ZCP", "ZCPP", "DZCP", "DZCPP",
    ] {
        let zone = name.contains("ZCP");
        let args = if zone {
            "D100E0 D200F0 D300E1"
        } else {
            "D100E0 D200F0"
        };
        let source = format!("LD X0\n{name} {args} M120F1\nEND");
        let program = compile_il(&source).unwrap();
        assert!(program.requires_vm());
        assert_eq!(program.networks[0].end_line, 2);
        assert!(
            !program.devices.contains(&d("M120")),
            "unresolved target is only a source label"
        );
        let mut sim = enabled();
        for (name, value) in [
            ("D1028", 1),
            ("D1029", 2),
            ("D1182", 3),
            ("D1183", 2),
            ("D101", 3),
            ("D202", 5),
            ("D303", 4),
        ] {
            sim.memory.set_word(d(name), value);
        }
        sim.memory.set_internal(d("M121"), true);
        sim.memory.set_internal(d("M125"), true);
        sim.scan(&program, 0);
        assert_eq!(sim.last_scan, ScanStatus::Complete);
        let actual = ["M122", "M123", "M124"].map(|n| sim.memory.bit(d(n)));
        assert_eq!(
            actual,
            if zone {
                [false, true, false]
            } else {
                [false, false, true]
            },
            "{name}"
        );
        assert!(sim.memory.bit(d("M121")) && sim.memory.bit(d("M125")));
        assert!(sim.observed_devices.contains(&d("M124")));
        let action = &program.networks[0].actions[0];
        let inner = if let Action::Pulse(a) = action {
            a.as_ref()
        } else {
            action
        };
        let Action::IndexedComparison(indexed) = inner else {
            panic!("typed comparison");
        };
        assert_eq!(
            indexed.display_value(&sim.memory),
            if zone { "2" } else { "4" }
        );
    }
}

#[test]
fn same_scan_index_updates_change_contact_sources_and_observed_addresses() {
    let mut sim = enabled();
    sim.memory.set_word(d("D101"), 7);
    sim.memory.set_word(d("D102"), 19);
    scan(
        &mut sim,
        "LD X0\nMOV K1 E0\nLD= D100E0 K7\nOUT Y0\nLD X0\nINC E0\nLD= D100E0 K19\nOUT Y1\nEND",
    );
    assert!(sim.memory.bit(d("Y0")) && sim.memory.bit(d("Y1")));
    assert!(sim.observed_devices.contains(&d("D101")) && sim.observed_devices.contains(&d("D102")));
    assert!(!sim.observed_devices.contains(&d("D100")));
}

#[test]
fn invalid_predicate_stops_before_inversion_and_subsequent_actions() {
    let program = compile_il("LD<> D100E0 K0\nINV\nOUT Y0\nMOV K99 D20\nEND").unwrap();
    let mut sim = enabled();
    sim.memory.set_word(d("D1028"), -30000);
    sim.memory.set_word(d("D20"), 17);
    sim.scan(&program, 0);
    assert!(matches!(sim.last_scan, ScanStatus::Fault(_)));
    assert!(!sim.memory.execution_limitations.is_empty());
    assert!(!sim.memory.bit(d("Y0")));
    assert_eq!(sim.memory.word(d("D20")), 17);
    assert!(
        !sim.memory.system_bit(1067),
        "unsupported range is not a fabricated native E1A"
    );
}

#[test]
fn eager_contacts_read_deciding_and_or_but_difference_contacts_skip() {
    for (op, gate, reads) in [
        ("AND=", false, true),
        ("OR=", true, true),
        ("AND&", false, true),
        ("OR^", true, true),
        ("ANDZ=", false, false),
        ("ORZ=", true, false),
        ("DANDZ<>", false, false),
        ("DORZ<>", true, false),
    ] {
        let tail = if op.contains('Z') { " K0" } else { "" };
        let source = format!("LD X0\n{op} D100E0 K0{tail}\nOUT Y0\nEND");
        let program = compile_il(&source).unwrap();
        let mut sim = Simulator::default();
        sim.memory.set_input(d("X0"), gate);
        sim.memory.set_word(d("D1028"), -30000);
        sim.scan(&program, 0);
        if reads {
            assert!(matches!(sim.last_scan, ScanStatus::Fault(_)), "{op}");
        } else {
            assert_eq!(sim.last_scan, ScanStatus::Complete, "{op}");
            assert!(sim.memory.execution_limitations.is_empty());
            assert_eq!(sim.memory.bit(d("Y0")), gate);
        }
    }
}

#[test]
fn word_orz_shortcut_skips_six_padding_words_in_independent_vm() {
    let program = compile_il("LD X0\nORZ= D100E0 K0 K0\nOUT Y0\nEND").unwrap();
    let predicate = program
        .execution_plan
        .nodes
        .iter()
        .find(|n| {
            matches!(
                &n.op,
                PlanOp::Predicate {
                    expr: Expr::IndexedComparison(_),
                    ..
                }
            )
        })
        .unwrap();
    assert_eq!(predicate.pc, 1);
    assert_eq!(predicate.fallthrough, 8);
    assert_eq!(predicate.shortcut_pc, Some(14));
    for gate in [false, true] {
        let mut sim = Simulator::default();
        sim.memory.set_input(d("X0"), gate);
        sim.memory.set_word(d("D100"), 1);
        sim.begin_plan_scan(&program, 0, true).unwrap();
        let mut visited = Vec::new();
        while let Some(pc) = sim.current_plan_pc() {
            visited.push(pc);
            sim.execute_plan_step(&program).unwrap();
        }
        assert_eq!(sim.last_scan, ScanStatus::Complete);
        assert_eq!(sim.memory.bit(d("Y0")), gate);
        for padding in 8..14 {
            assert_eq!(visited.contains(&padding), !gate);
        }
        assert_eq!(sim.observed_devices.contains(&d("D100")), !gate);
    }
}

#[test]
fn ordinary_orz_keeps_its_shortcut_in_a_program_using_indexed_actions() {
    let program =
        compile_il("LD X0\nCMP D100E0 K1 M100\nLD X0\nORZ= D0 K0 K0\nOUT Y0\nEND").unwrap();
    assert!(program.requires_vm());
    let ordinary = program
        .execution_plan
        .nodes
        .iter()
        .find(|n| {
            matches!(
                &n.op,
                PlanOp::Predicate {
                    expr: Expr::Numeric(_),
                    ..
                }
            )
        })
        .unwrap();
    assert_eq!(ordinary.shortcut_pc, Some(ordinary.fallthrough + 6));
    let mut sim = enabled();
    sim.scan(&program, 0);
    assert_eq!(sim.last_scan, ScanStatus::Complete);
    assert!(sim.memory.bit(d("Y0")));
}

#[test]
fn pulse_hold_skips_new_invalid_target_until_a_new_rising_edge() {
    let program = compile_il("LD X0\nCMPP K3 K2 M100E0\nEND").unwrap();
    let mut sim = enabled();
    sim.scan(&program, 0);
    assert!(sim.memory.bit(d("M100")));
    sim.memory.set_word(d("D1028"), -30000);
    sim.scan(&program, 0);
    assert!(sim.memory.execution_limitations.is_empty());
    sim.memory.set_input(d("X0"), false);
    sim.scan(&program, 0);
    assert!(sim.memory.execution_limitations.is_empty());
    sim.memory.set_input(d("X0"), true);
    sim.scan(&program, 0);
    assert!(!sim.memory.execution_limitations.is_empty());
    assert!(sim.memory.bit(d("M100")));
}

#[test]
fn source_snapshot_precedes_overlapping_indexed_three_bit_result() {
    for (body, before, expected) in [
        ("CMP K1M32E0 K2 M32E0", 3u8, 1u8),
        ("ZCP K1M32E0 K7 K6 M32E0", 5u8, 2u8),
    ] {
        let mut sim = enabled();
        for bit in 0..4 {
            sim.memory
                .set_internal(d(&format!("M{}", 32 + bit)), before & (1 << bit) != 0);
        }
        sim.memory.set_internal(d("M31"), true);
        sim.memory.set_internal(d("M36"), true);
        scan(&mut sim, &format!("LD X0\n{body}\nEND"));
        let value = (0..3).fold(0, |v, bit| {
            v | (u8::from(sim.memory.bit(d(&format!("M{}", 32 + bit)))) << bit)
        });
        assert_eq!(value, expected);
        assert_eq!(sim.memory.bit(d("M35")), before & 8 != 0);
        assert!(sim.memory.bit(d("M31")) && sim.memory.bit(d("M36")));
    }
}
