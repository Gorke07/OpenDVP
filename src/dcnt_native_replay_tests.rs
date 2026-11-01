//! Replay original observations through the real Rust source VM. Only rung,
//! direction and D-preset inputs are supplied; no observed output/state/PC is
//! written into local memory or used to select a local execution path.
use crate::counter::{CounterModel, CounterProfile};
use crate::execution_plan::PlanOp;
use crate::plc::{Device, Program, ScanStatus, Simulator, compile_il};
use serde_json::Value;
use sha2::{Digest, Sha256};

const EVIDENCE_SHA: &str = "6082c75002ab1bc9a6258282f240b1ba7d14c47793df2dcce675b95edf418c4f";
fn d(text: &str) -> Device {
    text.parse().unwrap()
}
fn signed(v: &Value) -> i32 {
    i32::try_from(v.as_i64().unwrap()).unwrap()
}
fn observed_snapshot(sim: &Simulator, counter: Device, actual: &Value, label: &str) {
    assert_eq!(
        sim.memory.double_word(counter),
        signed(&actual["signed_value"]),
        "{label} value"
    );
    assert_eq!(
        sim.memory.bit(counter),
        actual["contact"].as_bool().unwrap(),
        "{label} contact"
    );
    let bits = u32::from_str_radix(actual["raw_u32_hex"].as_str().unwrap(), 16).unwrap();
    assert_eq!(
        sim.memory.double_word(counter) as u32,
        bits,
        "{label} raw32"
    );
    let errors = actual["D1067_D1068"].as_array().unwrap();
    for (i, error) in errors.iter().enumerate() {
        assert_eq!(
            sim.memory.word(d(&format!("D{}", 1067 + i))) as u16,
            error.as_u64().unwrap() as u16,
            "{label} errors"
        );
    }
}
fn cycle(sim: &mut Simulator, p: &Program, counter: Device, actual: &Value, label: &str) -> usize {
    sim.begin_plan_scan(p, 100, true).unwrap();
    let mut predicted = Vec::new();
    let mut checked = false;
    while let Some(pc) = sim.current_plan_pc() {
        let node = &p.execution_plan.nodes[p.execution_plan.by_pc[&pc]];
        if matches!(node.op, PlanOp::End) {
            // The original probe observed data after DCNT and BEFORE END.
            observed_snapshot(sim, counter, actual, label);
            checked = true;
        }
        let step = sim.execute_plan_step(p).unwrap();
        predicted.push(step.next_pc);
        assert!(predicted.len() < 100, "unexpected local path: {label}");
    }
    assert!(checked);
    assert_eq!(sim.last_scan, ScanStatus::Complete);
    assert!(
        sim.memory.execution_limitations.is_empty(),
        "{label}: {:?}",
        sim.memory.execution_limitations
    );
    let observed: Vec<_> = actual["actual_PCs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
        .collect();
    assert_eq!(
        predicted, observed,
        "independently predicted source PC path: {label}"
    );
    predicted.len()
}

#[test]
fn original_dcnt_32_cases_172_scans_replay_actual_values_contacts_words_and_pcs() {
    let evidence = fixture!("dcnt-runtime-first.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(&evidence)),
        EVIDENCE_SHA,
        "raw observation authority changed; review new capture independently"
    );
    let report: Value = serde_json::from_slice(&evidence).unwrap();
    let runs = report["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 32);
    let mut scans = 0;
    let mut visits = 0;
    for run in runs {
        assert!(run["error"].is_null());
        assert_eq!(run["listener_pid_verified"], true);
        assert_eq!(run["owned_child_closed"], true);
        assert_eq!(run["listener_gone_after_close"], true);
        let model = run["model"].as_str().unwrap();
        let expected_sha = match model {
            "ES2" => crate::es2_bridge::TARGET_SHA256,
            "SS2" => crate::es2_bridge::SS2_SHA256,
            _ => panic!("unexpected model"),
        };
        assert_eq!(run["sha256"].as_str().unwrap(), expected_sha);
        let observation = &run["observations"];
        assert_eq!(observation["verified_readback"], true);
        assert_eq!(observation["literal_expectations_all_match"], true);
        let source = observation["source"].as_str().unwrap();
        let p = compile_il(source).unwrap();
        let encoded = crate::es2_encoder::encode(&p).unwrap();
        let captured: Vec<_> = observation["program_words_hex"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| u16::from_str_radix(v.as_str().unwrap(), 16).unwrap())
            .collect();
        // This immutable historical capture uploaded the tolerated C-tag80.
        // The independently recovered WPL compiler uses tag83. Normalize only
        // the operand at an actual DCNT source instruction, never arbitrary
        // data words; the old record remains runtime evidence, not evidence
        // of canonical compiler encoding. Lengths and PC paths are unchanged.
        let mut canonical = captured.clone();
        let mut adjusted = 0;
        for (action, pc) in p
            .networks
            .iter()
            .flat_map(|n| &n.actions)
            .zip(&encoded.action_pc)
        {
            if let crate::plc::Action::DoubleCounter(a) = action {
                let at = usize::from(*pc);
                assert_eq!(captured[at], 0x0600);
                assert_eq!(captured[at + 1], (a.counter.index << 8) | 0x80);
                canonical[at + 1] |= 3;
                adjusted += 1;
            }
        }
        assert_eq!(adjusted, 1, "historical probe has exactly one DCNT");
        assert_eq!(
            encoded.words, canonical,
            "{} compiler-normalized upload WORDs",
            run["label"]
        );
        let c = observation["counter"].as_u64().unwrap();
        assert!(matches!(c, 200 | 234));
        let counter = d(&format!("C{c}"));
        let mut sim = Simulator::default();
        sim.control.profile = crate::program_control::ControlProfile::InstalledLegacy;
        sim.memory.counters.profile = CounterProfile::InstalledLegacy;
        sim.memory.counters.model = if model == "ES2" {
            CounterModel::Es2
        } else {
            CounterModel::Ss2
        };
        sim.memory.data_register_limit = Some(if model == "ES2" { 9999 } else { 4999 });
        sim.memory.watchdog.profile = crate::interrupt::WatchdogProfile::Disabled;
        // External input Dpair; counter seed itself is executed by the source's
        // genuine DMOV rung, exactly as the original initialization scan.
        sim.memory
            .set_double_word(d("D0"), signed(&observation["preset"]));
        sim.memory.set_input(d("X1"), true);
        sim.memory.set_input(d("X0"), false);
        visits += cycle(
            &mut sim,
            &p,
            counter,
            &observation["initialization"],
            run["label"].as_str().unwrap(),
        );
        scans += 1;
        sim.memory.set_input(d("X1"), false);
        for (i, step) in observation["steps"].as_array().unwrap().iter().enumerate() {
            sim.memory
                .set_input(d("X0"), step["rung"].as_bool().unwrap());
            sim.memory
                .set_system(1000 + c as u16, step["direction_down"].as_bool().unwrap());
            if observation["dynamic_D_pair"].as_bool().unwrap() {
                sim.memory
                    .set_double_word(d("D0"), signed(&step["preset_input"]));
            }
            let label = format!("{model} {} step{i}", run["label"].as_str().unwrap());
            visits += cycle(&mut sim, &p, counter, &step["actual"], &label);
            scans += 1;
        }
    }
    assert_eq!(scans, 172);
    assert_eq!(visits, 860);
}

#[test]
fn native_dcnt_guard_keeps_physical_classes_local_and_preserves_six_word_layout() {
    for counter in 200..=234 {
        let p = compile_il(&format!("LD X0\nDCNT C{counter} K-1\nEND")).unwrap();
        let encoded = crate::es2_encoder::encode(&p).unwrap();
        assert_eq!(
            &encoded.words[1..7],
            &[
                0x0600,
                (counter << 8) | 0x0083,
                0xff80,
                0xff80,
                0xff80,
                0xff80
            ]
        );
    }
    for counter in 235..=254 {
        let p = compile_il(&format!("LD X0\nDCNT C{counter} D0\nEND")).unwrap();
        assert!(
            crate::es2_encoder::encode(&p)
                .unwrap_err()
                .contains("C235..C254")
        );
        assert_eq!(p.native_action_pc, [1]);
        assert_eq!(p.native_program_words, 8);
        let local = crate::es2_encoder::local_execution_layout(&p).unwrap();
        assert_eq!(
            (1..7).map(|pc| local.known_word_at(pc)).collect::<Vec<_>>(),
            [
                0x0600,
                (counter << 8) | 0x0083,
                0x0086,
                0x0086,
                0x0080,
                0x0080
            ]
            .map(Some)
        );
    }
}
