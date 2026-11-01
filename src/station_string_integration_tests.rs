//! Shared parser/VM/project/encoder integration, not a native parity claim.
use crate::plc::{self, Device, ScanStatus, Simulator};

fn d(text: &str) -> Device {
    text.parse().unwrap()
}
fn scan(sim: &mut Simulator, program: &plc::Program) {
    sim.scan(program, 100);
    assert_eq!(sim.last_scan, ScanStatus::Complete);
}

#[test]
fn string_pulse_first_held_false_and_rearm_use_real_vm_gate() {
    let p = plc::compile_il("LD X0\n$MOVP D0 D100\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_word(d("D0"), 0x4241);
    sim.memory.set_word(d("D100"), 123);
    scan(&mut sim, &p);
    assert_eq!(sim.memory.word(d("D100")), 123);
    sim.memory.set_input(d("X0"), true);
    scan(&mut sim, &p);
    assert_eq!(sim.memory.word(d("D100")), 0x4241);
    sim.memory.set_word(d("D0"), 0x4443);
    scan(&mut sim, &p);
    assert_eq!(
        sim.memory.word(d("D100")),
        0x4241,
        "held pulse must not recopy live source"
    );
    sim.memory.set_input(d("X0"), false);
    scan(&mut sim, &p);
    assert_eq!(sim.memory.word(d("D100")), 0x4241);
    sim.memory.set_input(d("X0"), true);
    scan(&mut sim, &p);
    assert_eq!(sim.memory.word(d("D100")), 0x4443);
    assert_eq!(sim.scans, 5);
}

#[test]
fn quoted_comments_case_label_and_real_project_save_roundtrip() {
    let source = "; outside\nld x0\n$movp \"Ab ; // cD\" d100 ; actual comment\nEND\n";
    let p = plc::compile_il(source).unwrap();
    let label = p.networks[0].actions[0].label();
    assert_eq!(label, "$MOVP \"Ab ; // cD\" D100");
    let rebuilt = plc::compile_il(&format!("LD X0\n{label}\nEND")).unwrap();
    assert_eq!(rebuilt.networks[0].actions[0].label(), label);
    let project = crate::project::Project {
        source: source.into(),
        name: "Quoted station strings".into(),
        ..Default::default()
    };
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let folder = std::env::temp_dir().join(format!(
        "opendvp-station-string-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir(&folder).unwrap();
    let path = folder.join("quoted.odvp");
    project.save(&path).unwrap();
    let loaded = crate::project::Project::load(&path).unwrap();
    assert_eq!(loaded.source, source);
    assert_eq!(
        loaded.compile().unwrap().networks[0].actions[0].label(),
        label
    );
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    scan(&mut sim, &loaded.compile().unwrap());
    let bytes: Vec<_> = (0..5)
        .flat_map(|i| (sim.memory.word(d(&format!("D{}", 100 + i))) as u16).to_le_bytes())
        .collect();
    assert_eq!(&bytes[..10], b"Ab ; // cD");
    assert_eq!(sim.memory.word(d("D105")) as u16 & 0xff, 0);
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir(&folder).unwrap();
}

#[test]
fn local_instruction_offsets_stay_real_while_native_encoding_rejects() {
    let source = "LD X0\n$MOV \"abc\" D100\nXCMP X4 C251 D0 D500 D2000\nYOUT C251 D0 D2000 Y0\n$MOV \"AB\" D4999\nEND";
    let p = plc::compile_il(source).unwrap();
    assert_eq!(p.native_action_pc, [1, 7, 18, 27]);
    assert_eq!(p.native_program_words, 33);
    let action_sites: Vec<_> = p
        .execution_plan
        .nodes
        .iter()
        .filter_map(|n| {
            matches!(n.op, crate::execution_plan::PlanOp::Action { .. })
                .then_some((n.pc, n.fallthrough))
        })
        .collect();
    assert_eq!(action_sites, [(1, 7), (7, 18), (18, 27), (27, 32)]);
    let mut sim = Simulator::default();
    sim.memory.data_register_limit = Some(4999);
    sim.memory.set_input(d("X0"), true);
    sim.memory.set_word(d("D4999"), 321);
    scan(&mut sim, &p);
    assert_eq!(sim.memory.word(d("D4999")), 321);
    assert_eq!(sim.memory.word(d("D1067")), 0xe1a);
    assert_eq!(sim.memory.word(d("D1068")), 27);
    assert!(sim.memory.system_bit(1067) && sim.memory.system_bit(1068));
    assert_eq!(sim.memory.word(d("D100")), 0x6261);
    for (instruction, expected) in [
        ("$MOV \"A\" D0", "$MOV"),
        ("$MOVP D0 D10", "$MOV"),
        ("XCMP X4 C251 D0 D500 D2000", "XCMP"),
        ("YOUT C251 D0 D2000 Y0", "YOUT"),
    ] {
        let p = plc::compile_il(&format!("LD X0\n{instruction}\nEND")).unwrap();
        let err = crate::es2_encoder::encode(&p).unwrap_err();
        assert!(err.contains(expected), "{instruction}: {err}");
    }
}

#[test]
fn capture_output_source_order_is_visible_in_same_vm_scan() {
    let capture = "XCMP X4 C251 D0 D500 D2000";
    let output = "YOUT C251 D0 D2000 Y0";
    for (body, immediate) in [
        (format!("{capture}\n{output}"), true),
        (format!("{output}\n{capture}"), false),
    ] {
        let p = plc::compile_il(&format!("LD X0\n{body}\nEND")).unwrap();
        let mut sim = Simulator::default();
        sim.memory.set_input(d("X0"), true);
        sim.memory.set_word(d("D0"), 1);
        sim.memory.set_word(d("D1"), 2);
        sim.memory.set_double_word(d("D500"), 1);
        sim.memory.set_double_word(d("D502"), -1);
        scan(&mut sim, &p); // establish initial sampled triggerOFF
        sim.memory.set_double_word(d("C251"), 100);
        sim.memory.set_input(d("X4"), true);
        scan(&mut sim, &p);
        assert_eq!(sim.memory.bit(d("Y0")), immediate);
        assert_eq!(sim.memory.word(d("D2000")), 1);
        scan(&mut sim, &p);
        assert!(sim.memory.bit(d("Y0")));
    }
}

#[test]
fn published_station_example_runs_through_compiler_and_vm_with_external_counter() {
    let p =
        plc::compile_il("LD X0\nXCMP X4 C251 D0 D500 D2000\nYOUT C251 D0 D2000 Y0\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim.memory.set_word(d("D0"), 3);
    sim.memory.set_word(d("D1"), 4);
    for (i, v) in [2000, 3000, 4000, 100, 120, 130, 50, -20, 20]
        .into_iter()
        .enumerate()
    {
        sim.memory
            .set_double_word(d(&format!("D{}", 500 + 2 * i)), v);
    }
    scan(&mut sim, &p);
    for (level, count) in [(true, 3000), (false, 3500), (true, 4500), (false, 5000)] {
        sim.memory.set_input(d("X4"), level);
        // Explicit deterministic encoder position; no physical encoder/DCNT
        // simulator equivalence is inferred from this memory input.
        sim.memory.set_double_word(d("C251"), count);
        scan(&mut sim, &p);
    }
    assert_eq!(sim.memory.double_word(d("D3")), 4500);
    assert_eq!(sim.memory.double_word(d("D5")), 5000);
    for (count, outputs, tails) in [
        (5200, [true, false, false], [2, 1, 1]),
        (6200, [false, true, false], [3, 2, 1]),
        (6800, [true, false, false], [4, 3, 1]),
        (7300, [false, false, true], [4, 3, 2]),
        (7700, [false, true, false], [4, 4, 3]),
        (8000, [false, false, false], [4, 4, 3]),
        (8700, [false, false, true], [4, 4, 4]),
    ] {
        sim.memory.set_double_word(d("C251"), count);
        scan(&mut sim, &p);
        for j in 0..3 {
            assert_eq!(
                sim.memory.bit(d(&format!("Y{j}"))),
                outputs[j],
                "counter={count}"
            );
            assert_eq!(
                sim.memory.word(d(&format!("D{}", 2003 + j))),
                tails[j],
                "counter={count}"
            );
        }
    }
    assert!(sim.memory.execution_limitations.is_empty());
}

#[test]
fn runtime_station_extent_failure_is_atomic_and_does_not_invent_plc_error() {
    let p = plc::compile_il("LD X0\nXCMP X4 C251 D0 D500 D4998\nMOV K77 D50\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.data_register_limit = Some(4999);
    sim.memory.set_input(d("X0"), true);
    sim.memory.set_word(d("D0"), 1);
    sim.memory.set_word(d("D1"), 1);
    sim.memory.set_word(d("D4998"), 1);
    sim.memory.set_word(d("D4999"), 1);
    sim.memory.set_double_word(d("D3"), 1234);
    scan(&mut sim, &p);
    assert_eq!(sim.memory.word(d("D4998")), 1);
    assert_eq!(sim.memory.word(d("D4999")), 1);
    assert_eq!(sim.memory.double_word(d("D3")), 1234);
    assert!(!sim.memory.system_bit(1067));
    assert!(!sim.memory.system_bit(1068));
    assert!(!sim.memory.execution_limitations.is_empty());
    assert_eq!(sim.memory.word(d("D50")), 77);
}

#[test]
fn example_compiles_and_false_inputs_do_not_initialize_queue_or_strings() {
    let p = plc::compile_il(include_str!("../examples/station-string.il")).unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_word(d("D2000"), 5);
    sim.memory.set_word(d("D100"), 123);
    scan(&mut sim, &p);
    assert_eq!(sim.memory.word(d("D2000")), 5);
    assert_eq!(sim.memory.word(d("D100")), 123);
    assert!(sim.memory.execution_limitations.is_empty());
    assert!(crate::es2_encoder::encode(&p).is_err());
}
