//! Actual compiler/VM tests. Event/cost inputs are explicitly virtual, never
//! observations of physical PLC or installed simulator interrupt performance.
use crate::execution_plan::PlanOp;
use crate::interrupt::InterruptId;
use crate::plc::{Device, Program, ScanStatus, Simulator, compile_il};

fn d(text: &str) -> Device {
    text.parse().unwrap()
}
fn scan(sim: &mut Simulator, p: &Program, ms: u64) {
    sim.scan(p, ms);
    assert_eq!(sim.last_scan, ScanStatus::Complete);
    assert!(
        sim.memory.execution_limitations.is_empty(),
        "unexpected limitation in valid scan: {:?}",
        sim.memory.execution_limitations
    );
}
fn finish_steps(sim: &mut Simulator, p: &Program) -> u64 {
    let mut visits = 0;
    while sim.current_plan_pc().is_some() {
        let step = sim.execute_plan_step(p).unwrap();
        visits += 1;
        assert!(visits < 1000, "fixture unexpectedly failed to finish");
        if step.scan_boundary {
            assert_eq!(step.next_pc, 0);
        }
    }
    assert_eq!(sim.last_scan, ScanStatus::Complete);
    assert!(
        sim.memory.execution_limitations.is_empty(),
        "unexpected limitation in valid phased scan: {:?}",
        sim.memory.execution_limitations
    );
    visits
}
fn until(sim: &mut Simulator, p: &Program, pc: u16) -> u64 {
    let mut visits = 0;
    while sim.current_plan_pc() != Some(pc) {
        sim.execute_plan_step(p).unwrap();
        visits += 1;
        assert!(
            visits < 1000 && sim.current_plan_pc().is_some(),
            "fixture did not reach PC{pc}"
        );
    }
    visits
}
fn action_pc(p: &Program, label: &str) -> u16 {
    p.execution_plan
        .nodes
        .iter()
        .find_map(|n| match n.op {
            PlanOp::Action {
                network, action, ..
            } if p.networks[network].actions[action].label() == label => Some(n.pc),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing action {label}"))
}

#[test]
fn physical_edge_during_delay_dispatches_before_next_action_without_image_refresh() {
    let p=compile_il("EI\nLD X7\nDELAY K20\nMOV K11 D10\nFEND\nI001\nLD M1000\nMOV D10 D20\nLD X0\nOUT Y0\nIRET\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X7"), true);
    sim.memory.set_word(d("D10"), 7);
    sim.memory
        .timing_io
        .enqueue_input_edge(500, d("X0"), true)
        .unwrap();
    scan(&mut sim, &p, 0);
    assert_eq!(sim.memory.word(d("D20")), 7, "ISR ran before following MOV");
    assert_eq!(sim.memory.word(d("D10")), 11);
    assert!(
        !sim.memory.bit(d("Y0")),
        "ordinary LD X uses the latched image without REF"
    );
    assert!(!sim.memory.bit(d("X0")));
    scan(&mut sim, &p, 0);
    assert!(sim.memory.bit(d("X0")));
    assert_eq!(
        sim.memory.word(d("D20")),
        7,
        "image refresh must not synthesize another rising IRQ"
    );
}

#[test]
fn di_memorizes_edge_and_ei_delivers_it_before_following_mov() {
    let p=compile_il("DI\nLD X7\nDELAY K20\nMOV D20 D10\nEI\nMOV D20 D11\nFEND\nI001\nLD M1000\nINC D20\nIRET\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X7"), true);
    sim.memory
        .timing_io
        .enqueue_input_edge(500, d("X0"), true)
        .unwrap();
    scan(&mut sim, &p, 0);
    assert_eq!(sim.memory.word(d("D10")), 0);
    assert_eq!(sim.memory.word(d("D11")), 1);
    assert_eq!(sim.memory.word(d("D20")), 1);
}

#[test]
fn chronological_pending_irqs_do_not_nest_even_when_new_edge_arrives_in_isr() {
    let source = "EI\nLD X7\nDELAY K30\nFEND\nI001\nLD M1000\nINC D50\nMOV D50 D100\nINC D50\nMOV D50 D110\nIRET\nI101\nLD M1000\nINC D50\nMOV D50 D101\nDELAY K10\nINC D50\nMOV D50 D111\nIRET\nI201\nLD M1000\nINC D50\nMOV D50 D102\nINC D50\nMOV D50 D112\nIRET\nI301\nLD M1000\nINC D50\nMOV D50 D103\nINC D50\nMOV D50 D113\nIRET\nEND";
    let p = compile_il(source).unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X7"), true);
    for (time, input) in [(1000, "X0"), (500, "X1"), (1500, "X2"), (3500, "X3")] {
        sim.memory
            .timing_io
            .enqueue_input_edge(time, d(input), true)
            .unwrap();
    }
    scan(&mut sim, &p, 0);
    for (address, expected) in [
        ("D101", 1),
        ("D111", 2),
        ("D100", 3),
        ("D110", 4),
        ("D102", 5),
        ("D112", 6),
        ("D103", 7),
        ("D113", 8),
    ] {
        assert_eq!(sim.memory.word(d(address)), expected, "{address}");
    }
    assert!(sim.memory.interrupts.active.is_none());
    assert!(sim.memory.interrupts.pending().is_empty());
}

#[test]
fn simultaneous_qualified_requests_use_numeric_pointer_priority_in_vm() {
    let p=compile_il("EI\nLD X7\nMOV K1 D0\nFEND\nI001\nLD M1000\nINC D50\nMOV D50 D10\nIRET\nI010\nLD M1000\nINC D50\nMOV D50 D11\nIRET\nI101\nLD M1000\nINC D50\nMOV D50 D12\nIRET\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X7"), true);
    sim.begin_plan_scan(&p, 0, false).unwrap();
    until(&mut sim, &p, action_pc(&p, "MOV K1 D0"));
    for label in ["I101", "I010", "I001"] {
        sim.memory
            .interrupts
            .request(label.parse::<InterruptId>().unwrap(), 0)
            .unwrap();
    }
    finish_steps(&mut sim, &p);
    assert_eq!(
        [
            sim.memory.word(d("D10")),
            sim.memory.word(d("D11")),
            sim.memory.word(d("D12"))
        ],
        [1, 2, 3]
    );
}

#[test]
fn isr_return_preserves_active_call_and_for_frames_and_global_visit_count() {
    let p=compile_il("EI\nLD X7\nFOR K3\nCALL P0\nNEXT\nLD X7\nMOV D0 D1\nFEND\nP0\nLD X7\nINC D0\nSRET\nI001\nLD M1000\nINC D10\nIRET\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X7"), true);
    sim.begin_plan_scan(&p, 0, false).unwrap();
    let before = until(&mut sim, &p, action_pc(&p, "INC D0"));
    sim.memory.set_input(d("X0"), true);
    let after = finish_steps(&mut sim, &p);
    assert_eq!(sim.memory.word(d("D0")), 3);
    assert_eq!(sim.memory.word(d("D1")), 3);
    assert_eq!(sim.memory.word(d("D10")), 1);
    assert_eq!(
        sim.control.steps,
        before + after,
        "context restoration cannot discard ISR instruction visits"
    );
}

#[test]
fn mps_word_and_branch_snapshot_survive_isr_logic_changes() {
    let p=compile_il("EI\nLD X7\nMC N0\nLD X7\nMPS\nAND X6\nOUT Y0\nMPP\nOUT Y1\nMCR N0\nFEND\nI001\nLD M1000\nOUT Y2\nIRET\nEND").unwrap();
    let restore = p
        .execution_plan
        .nodes
        .iter()
        .find(|n| matches!(n.op, PlanOp::Restore(true)))
        .unwrap()
        .pc;
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X7"), true);
    sim.begin_plan_scan(&p, 0, false).unwrap();
    until(&mut sim, &p, restore);
    sim.memory.set_input(d("X0"), true);
    finish_steps(&mut sim, &p);
    assert!(!sim.memory.bit(d("Y0")));
    assert!(sim.memory.bit(d("Y1")));
    assert!(sim.memory.bit(d("Y2")));
}

#[test]
fn disabled_main_master_is_restored_after_independent_handler_execution() {
    let p=compile_il("EI\nLD X7\nMC N0\nLD X6\nOUT Y0\nMCR N0\nLD X6\nOUT Y1\nFEND\nI001\nLD M1000\nOUT Y2\nIRET\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X6"), true);
    sim.begin_plan_scan(&p, 0, false).unwrap();
    until(&mut sim, &p, action_pc(&p, "OUT Y0"));
    sim.memory.set_input(d("X0"), true);
    finish_steps(&mut sim, &p);
    assert!(!sim.memory.bit(d("Y0")));
    assert!(sim.memory.bit(d("Y1")));
    assert!(sim.memory.bit(d("Y2")));
}

#[test]
fn malformed_regions_and_cross_region_jumps_are_compile_errors() {
    for source in [
        "I001\nLD X0\nOUT Y0\nIRET\nEND",
        "IRET\nEND",
        "EI\nFEND\nI001\nLD X0\nOUT Y0\nEND",
        "EI\nFEND\nI001\nIRET\nI001\nIRET\nEND",
        "EI\nFEND\nI001\nMC N0\nMCR N0\nIRET\nEND",
        "EI\nFEND\nI001\nSTL S20\nRET\nIRET\nEND",
        "EI\nFEND\nI001\nFOR K2\nIRET\nNEXT\nEND",
        "EI\nLD X0\nCJ P0\nFEND\nI001\nP0\nLD X0\nOUT Y0\nIRET\nEND",
        "EI\nFEND\nI001\nFEND\nIRET\nEND",
    ] {
        assert!(
            compile_il(source).is_err(),
            "accepted invalid region:\n{source}"
        );
    }
}

#[test]
fn watchdog_normal_and_pulse_have_independent_real_vm_edges() {
    for (instruction, refreshes) in [("WDT", [0, 1, 2, 2, 3]), ("WDTP", [0, 1, 1, 1, 2])] {
        let p = compile_il(&format!("LD X0\n{instruction}\nEND")).unwrap();
        let mut sim = Simulator::default();
        for (power, expected) in [false, true, true, false, true].into_iter().zip(refreshes) {
            sim.memory.set_input(d("X0"), power);
            scan(&mut sim, &p, 0);
            assert_eq!(sim.memory.watchdog.refreshes, expected, "{instruction}");
        }
    }
}

#[test]
fn watchdog_can_segment_long_scan_but_held_wdtp_cannot_repeat_refresh() {
    // DELAY's supported legacy operand range is K1..K1000. Compose each
    // 150ms segment from two genuinely executed visits, rather than an
    // out-of-range instruction that only records a limitation.
    let p =
        compile_il("LD X0\nDELAY K1000\nDELAY K500\nWDT\nDELAY K1000\nDELAY K500\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    scan(&mut sim, &p, 0);
    assert_eq!(sim.memory.timing_io.now_us, 300_000);
    assert_eq!(sim.memory.word(d("D1000")), 200);
    let p =
        compile_il("LD X0\nDELAY K1000\nDELAY K500\nWDTP\nDELAY K1000\nDELAY K500\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    scan(&mut sim, &p, 0);
    sim.scan(&p, 0);
    assert!(matches!(sim.last_scan, ScanStatus::Fault(_)));
    assert!(sim.memory.system_bit(1008));
    assert_eq!(sim.scans, 1);
    assert!(
        sim.memory
            .execution_limitations
            .iter()
            .all(|message| message.contains("Watchdog"))
    );
}

#[test]
fn wdt_never_resets_global_hard_instruction_budget() {
    let p = compile_il("P0\nLD X0\nWDT\nCJ P0\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.control.hard_step_limit = 40;
    sim.memory.interrupts.header_instruction_cost_us = 1000;
    sim.memory.set_input(d("X0"), true);
    sim.scan(&p, 0);
    assert!(matches!(sim.last_scan, ScanStatus::Fault(_)));
    assert_eq!(sim.scans, 0);
    assert_eq!(sim.control.steps, 40);
    assert!(sim.memory.watchdog.refreshes > 1);
    assert!(!sim.memory.system_bit(1008));
    assert!(sim.memory.watchdog.fault.is_none());
}

#[test]
fn timeout_records_current_word_pc_clears_all_y_and_stays_latched() {
    let p = compile_il("LD X0\nSET Y0\nDELAY K1000\nDELAY K1000\nDELAY K1\nSET Y1\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    for name in ["Y7", "Y10", "Y377"] {
        sim.memory.set_bit(d(name), true);
    }
    sim.scan(&p, 0);
    assert!(matches!(sim.last_scan, ScanStatus::Fault(_)));
    assert!(sim.memory.system_bit(1008));
    assert_eq!(sim.scans, 0);
    assert_eq!(
        sim.memory.word(d("D1008")),
        action_pc(&p, "DELAY K1") as i16
    );
    for index in 0..256 {
        assert!(
            !sim.memory.bit(Device {
                area: crate::plc::Area::Y,
                index
            }),
            "Y index{index}"
        );
    }
    assert!(
        !sim.memory.system_bit(1067),
        "WDT is not an invented E1A arithmetic fault"
    );
    assert!(
        sim.memory
            .execution_limitations
            .iter()
            .all(|message| message.contains("Watchdog"))
    );
    sim.memory.watchdog.refresh(true);
    sim.scan(&p, 0);
    assert!(matches!(sim.last_scan, ScanStatus::Fault(_)));
    assert_eq!(sim.scans, 0);
    sim.restart_preserving_file_store();
    assert!(!sim.memory.system_bit(1008));
    assert!(sim.memory.watchdog.fault.is_none());
}

#[test]
fn recurring_timer_phase_survives_real_scan_boundaries() {
    let p = compile_il("EI\nLD X7\nMOV D20 D21\nFEND\nI699\nLD M1000\nINC D20\nIRET\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X7"), true);
    for expected in 0..4 {
        scan(&mut sim, &p, 100);
        assert_eq!(sim.memory.word(d("D20")), expected);
        assert_eq!(sim.memory.word(d("D21")), expected);
    }
    assert_eq!(sim.memory.timing_io.now_us, 400_000);
}

#[test]
fn configured_irq_overload_consumes_shared_budget_instead_of_livelock() {
    let p = compile_il("EI\nFEND\nI602\nLD M1000\nINC D20\nIRET\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.interrupts.header_instruction_cost_us = 1000;
    sim.control.hard_step_limit = 45;
    scan(&mut sim, &p, 0);
    sim.scan(&p, 0);
    assert!(matches!(sim.last_scan, ScanStatus::Fault(_)));
    assert_eq!(sim.scans, 1);
    assert_eq!(sim.control.steps, 45);
    assert!(sim.memory.word(d("D20")) > 1);
    assert!(!sim.memory.system_bit(1008));
}

#[test]
fn example_has_real_interrupt_regions_and_runs_as_declared_local_profile() {
    let p = compile_il(include_str!("../examples/interrupt-watchdog.il")).unwrap();
    assert_eq!(p.execution_plan.links.interrupts.len(), 2);
    let mut sim = Simulator::default();
    scan(&mut sim, &p, 20);
    sim.memory.set_input(d("X0"), true);
    scan(&mut sim, &p, 20);
    assert_eq!(sim.memory.word(d("D100")), 1);
    assert!(sim.memory.word(d("D110")) >= 1);
    assert!(sim.memory.execution_limitations.is_empty());
}
