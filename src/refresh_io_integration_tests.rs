//! Actual compiler and VM integration; all time/input/matrix stimuli are explicit.
use crate::execution_plan::PlanOp;
use crate::plc::{Device, Program, ScanStatus, Simulator, compile_il};
use crate::refresh_io::{FilterProfile, RefreshState};

fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn pc(p: &Program, label: &str) -> u16 {
    p.execution_plan
        .nodes
        .iter()
        .find_map(|n| match n.op {
            PlanOp::Action {
                network, action, ..
            } if p.networks[network].actions[action].label() == label => Some(n.pc),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {label}"))
}
fn until(sim: &mut Simulator, p: &Program, target: u16) {
    let mut n = 0;
    while sim.current_plan_pc() != Some(target) {
        assert!(sim.current_plan_pc().is_some(), "targetPC not visited");
        sim.execute_plan_step(p).unwrap();
        n += 1;
        assert!(n < 1000);
    }
}
fn finish(sim: &mut Simulator, p: &Program) {
    let mut n = 0;
    while sim.current_plan_pc().is_some() {
        sim.execute_plan_step(p).unwrap();
        n += 1;
        assert!(n < 1000);
    }
    assert_eq!(sim.last_scan, ScanStatus::Complete);
    assert!(
        sim.memory.execution_limitations.is_empty(),
        "{:?}",
        sim.memory.execution_limitations
    );
}
fn scan(sim: &mut Simulator, p: &Program, ms: u64) {
    sim.scan(p, ms);
    assert_eq!(sim.last_scan, ScanStatus::Complete);
    assert!(
        sim.memory.execution_limitations.is_empty(),
        "{:?}",
        sim.memory.execution_limitations
    );
}

#[test]
fn zrst_filter_register_writes_zero_instead_of_reapplying_cold_default() {
    let p = compile_il("LD X0\nZRST D1020 D1020\nEND").unwrap();
    let mut s = Simulator::default();
    assert_eq!(s.memory.word(d("D1020")), 10);
    s.memory.set_input(d("X0"), true);
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.word(d("D1020")), 0);
}

#[test]
fn reff_encoding_matches_original_uploaded_and_readback_program() {
    // Independent original program/readback captured in
    // ES2/SS2 REFF K10.
    let p = compile_il("LD X1\nMOV K333 D1020\nMOV K444 D1021\nLD X0\nREFF K10\nEND").unwrap();
    assert_eq!(
        crate::es2_encoder::encode(&p).unwrap().words,
        vec![
            0x0124, 0x2800, 0x4D80, 0x0180, 0xF886, 0x0786, 0x2800, 0xBC80, 0x0180, 0xFA86, 0x0786,
            0x0024, 0x7600, 0x0A80, 0x0080, 0x0F00,
        ]
    );
}

#[test]
fn ref_queued_inputs_refresh_selected_bank_without_refreshing_the_other() {
    let p=compile_il("LD X7\nDELAY K10\nREF X0 K8\nLD X0\nOUT Y0\nLD X10\nOUT Y1\nLD X7\nREF X10 K8\nLD X10\nOUT Y2\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X7"), true);
    for input in ["X0", "X10"] {
        s.memory
            .timing_io
            .enqueue_input_edge(500, d(input), true)
            .unwrap();
    }
    s.begin_plan_scan(&p, 0, false).unwrap();
    until(&mut s, &p, pc(&p, "REF X0 K8"));
    assert!(!s.memory.bit(d("X0")));
    assert!(!s.memory.bit(d("X10")));
    s.execute_plan_step(&p).unwrap();
    assert!(s.memory.bit(d("X0")));
    assert!(!s.memory.bit(d("X10")));
    finish(&mut s, &p);
    assert!(s.memory.bit(d("Y0")));
    assert!(!s.memory.bit(d("Y1")));
    assert!(s.memory.bit(d("Y2")));
}
#[test]
fn ref_output_publication_precedes_end_and_other_output_waits_for_end() {
    let p = compile_il("LD X0\nOUT Y0\nOUT Y10\nREF Y0 K8\nMOV K1 D0\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X0"), true);
    s.begin_plan_scan(&p, 0, false).unwrap();
    until(&mut s, &p, pc(&p, "REF Y0 K8"));
    assert!(s.memory.bit(d("Y0")));
    assert!(s.memory.bit(d("Y10")));
    assert!(!s.memory.physical_output(d("Y0")));
    assert!(!s.memory.physical_output(d("Y10")));
    s.execute_plan_step(&p).unwrap();
    assert!(s.memory.physical_output(d("Y0")));
    assert!(!s.memory.physical_output(d("Y10")));
    finish(&mut s, &p);
    assert!(s.memory.physical_output(d("Y10")));
}
#[test]
fn reff_register_changes_now_but_filter_duration_changes_only_at_end() {
    let p = compile_il("LD X7\nREFF K5\nMOV D1020 D0\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X7"), true);
    s.begin_plan_scan(&p, 0, false).unwrap();
    until(&mut s, &p, pc(&p, "MOV D1020 D0"));
    assert_eq!(s.memory.word(d("D1020")), 5);
    assert_eq!(s.memory.refresh_io.active_filter_ms(0), Some(10));
    finish(&mut s, &p);
    assert_eq!(s.memory.word(d("D0")), 5);
    assert_eq!(s.memory.refresh_io.active_filter_ms(0), Some(5));
}
#[test]
fn queued_bounce_is_filtered_while_explicit_input_override_is_immediate() {
    let setup = compile_il("LD X7\nREFF K5\nEND").unwrap();
    let p = compile_il("LD X7\nDELAY K100\nREF X0 K8\nLD X0\nOUT Y0\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X7"), true);
    scan(&mut s, &setup, 0);
    let start = s.memory.timing_io.now_us;
    for (dt, v) in [(1000, true), (4000, false), (8000, true)] {
        s.memory
            .timing_io
            .enqueue_input_edge(start + dt, d("X0"), v)
            .unwrap();
    }
    scan(&mut s, &p, 0);
    assert!(!s.memory.bit(d("Y0")), "only2ms stable after last bounce");
    scan(&mut s, &p, 0);
    assert!(s.memory.bit(d("Y0")), "stable high accepted at13ms");
    s.memory.set_input(d("X0"), false);
    assert!(!s.memory.bit(d("X0")));
    let no_delay = compile_il("LD X7\nREF X0 K8\nLD X0\nOUT Y0\nEND").unwrap();
    scan(&mut s, &no_delay, 0);
    assert!(!s.memory.bit(d("Y0")));
}
#[test]
fn interrupt_owned_input_bypasses_software_filter_but_keeps_real_irq() {
    let p = compile_il(
        "EI\nLD X7\nDELAY K10\nREF X0 K8\nLD X0\nOUT Y0\nFEND\nI001\nLD M1000\nINC D0\nIRET\nEND",
    )
    .unwrap();
    let mut s = Simulator::default();
    s.memory.refresh_io.filter_profile = FilterProfile::DocumentedSoftware;
    RefreshState::power_on(&mut s.memory);
    s.memory.set_input(d("X7"), true);
    s.memory
        .timing_io
        .enqueue_input_edge(500, d("X0"), true)
        .unwrap();
    scan(&mut s, &p, 0);
    assert!(s.memory.bit(d("Y0")));
    assert_eq!(s.memory.word(d("D0")), 1);
}
#[test]
fn reffp_held_power_does_not_rewrite_direct_value_until_rearm() {
    let p = compile_il("LD X0\nREFFP K5\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X0"), true);
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.word(d("D1020")), 5);
    s.memory.set_word(d("D1020"), 12);
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.word(d("D1020")), 12);
    s.memory.set_input(d("X0"), false);
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.word(d("D1020")), 12);
    s.memory.set_input(d("X0"), true);
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.word(d("D1020")), 5);
}
#[test]
fn refp_held_visit_does_not_refresh_an_edge_that_arrived_during_delay() {
    let p = compile_il("LD X7\nDELAY K20\nREFP X0 K8\nLD X0\nOUT Y0\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X7"), true);
    scan(&mut s, &p, 0);
    let now = s.memory.timing_io.now_us;
    s.memory
        .timing_io
        .enqueue_input_edge(now + 500, d("X0"), true)
        .unwrap();
    scan(&mut s, &p, 0);
    assert!(
        !s.memory.bit(d("Y0")),
        "held REFP skips refresh, X image stays at scan-start value"
    );
    s.memory.set_input(d("X7"), false);
    scan(&mut s, &p, 0);
    s.memory.set_input(d("X7"), true);
    scan(&mut s, &p, 0);
    assert!(s.memory.bit(d("Y0")));
}
#[test]
fn published_example_compiles_with_an_explicit_virtual_matrix_and_native_boundary() {
    let p = compile_il(include_str!("../examples/refresh-matrix.il")).unwrap();
    let mut s = Simulator::default();
    let site = pc(&p, "MTR X40 Y40 M10 K2");
    s.memory
        .refresh_io
        .set_matrix_rows(usize::from(site), vec![0x81, 0x42])
        .unwrap();
    s.memory.set_input(d("X7"), true);
    for _ in 0..4 {
        scan(&mut s, &p, 25);
    }
    assert!(
        crate::es2_encoder::encode(&p).is_err(),
        "installed REF/MTR stubs are not usable encodings"
    );
    let reff = compile_il("LD X0\nREFFP K5\nEND").unwrap();
    assert!(crate::es2_encoder::encode(&reff).is_ok());
}
#[test]
fn mtr_configuration_uses_actual_word_pc_and_deadlines_during_delay() {
    let p=compile_il("LD X7\nMOV K99 D9\nMTR X40 Y40 M10 K2\nDELAY K250\nMOV K1 D0\nDELAY K250\nLD M1029\nOUT Y0\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X7"), true);
    let site = pc(&p, "MTR X40 Y40 M10 K2");
    assert!(site > 1);
    s.memory
        .refresh_io
        .set_matrix_rows(usize::from(site), vec![0x81, 0x42])
        .unwrap();
    s.begin_plan_scan(&p, 0, false).unwrap();
    until(&mut s, &p, pc(&p, "MOV K1 D0"));
    assert_eq!(s.memory.refresh_io.matrix_owner(), Some(usize::from(site)));
    assert_eq!(s.memory.timing_io.now_us, 25000);
    assert!(s.memory.bit(d("M10")));
    assert!(s.memory.bit(d("M17")));
    assert!(!s.memory.bit(d("M21")));
    assert!(s.memory.physical_output(d("Y41")));
    finish(&mut s, &p);
    assert_eq!(s.memory.timing_io.now_us, 50000);
    assert!(s.memory.bit(d("M21")));
    assert!(s.memory.bit(d("M26")));
    assert!(s.memory.bit(d("Y0")));
    assert!(s.memory.system_bit(1029));
    assert!(!s.memory.bit(d("M18")));
    s.memory.set_input(d("X7"), false);
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.refresh_io.matrix_owner(), None);
    assert!(!s.memory.physical_output(d("Y40")));
    assert!(!s.memory.physical_output(d("Y41")));
}
fn assert_watchdog_y_off(s: &Simulator) {
    assert!(matches!(s.last_scan, ScanStatus::Fault(_)));
    assert!(s.memory.system_bit(1008));
    for index in 0..256 {
        let d = Device {
            area: crate::plc::Area::Y,
            index,
        };
        assert!(!s.memory.bit(d), "logical {d}");
        assert!(!s.memory.physical_output(d), "physical {d}");
    }
}
#[test]
fn watchdog_abort_stops_matrix_but_retains_rows_filters_and_queued_input_time() {
    let p = compile_il("LD X7\nMTR X40 Y40 M10 K2\nDELAY K20\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X7"), true);
    s.memory.set_word(d("D1000"), 1);
    s.memory.refresh_io.filter_profile = FilterProfile::DocumentedSoftware;
    let site = usize::from(pc(&p, "MTR X40 Y40 M10 K2"));
    s.memory
        .refresh_io
        .set_matrix_rows(site, vec![0x81, 0x42])
        .unwrap();
    s.memory
        .timing_io
        .enqueue_input_edge(10000, d("X0"), true)
        .unwrap();
    s.scan(&p, 0);
    assert_watchdog_y_off(&s);
    assert_eq!(s.memory.refresh_io.matrix_owner(), None);
    crate::timing_io::advance_by(&mut s.memory, 50000);
    assert_watchdog_y_off(&s);
    assert_eq!(s.memory.timing_io.now_us, 52000);
    assert_eq!(s.memory.timing_io.pending_input_events(), 0);
    assert_eq!(
        s.memory.timing_io.physical_inputs.get(&d("X0")),
        Some(&true)
    );
    assert!(!s.memory.bit(d("M10")));
    assert!(
        !s.memory.bit(d("M21")),
        "aborted matrix never samples another row"
    );
    assert!(
        !s.memory.bit(d("X0")),
        "filter acceptance alone does not refresh image"
    );
    crate::timing_io::refresh_inputs(&mut s.memory);
    assert!(s.memory.bit(d("X0")));
    assert_watchdog_y_off(&s);
    // Wiring survived abort; explicit restart of the watchdog state is not
    // manufactured here. A new powered matrix visit can reuse its row fixture.
    assert!(
        s.memory
            .refresh_io
            .set_matrix_rows(site, vec![0x81, 0x42])
            .is_ok()
    );
}
#[test]
fn watchdog_abort_disarms_csfo_and_preserves_external_reservations_and_feedback() {
    let p = compile_il("LD X7\nFOR K2\nLD X7\nCSFO X0 D0 D10\nMOV D1 D20\nDELAY K30\nNEXT\nEND")
        .unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X7"), true);
    s.memory.set_word(d("D1000"), 4);
    s.memory.set_word(d("D0"), 1);
    s.memory.set_word(d("D10"), 100);
    s.memory
        .timing_io
        .reserve_input_pair(1, "external-fixture")
        .unwrap();
    s.memory
        .timing_io
        .reserve_output_pair(1, "external-fixture")
        .unwrap();
    for (at, level) in [
        (1000, true),
        (1100, false),
        (2000, true),
        (2100, false),
        (8000, true),
        (8100, false),
        (9000, true),
        (9100, false),
    ] {
        s.memory
            .timing_io
            .enqueue_input_edge(at, d("X0"), level)
            .unwrap();
    }
    s.scan(&p, 0);
    assert_watchdog_y_off(&s);
    assert_eq!(s.memory.word(d("D20")), 1000);
    assert_eq!(s.memory.double_word(d("D2")), 2);
    assert!(s.memory.timing_io.pulse_outputs.is_empty());
    let feedback = [
        s.memory.word(d("D1")),
        s.memory.word(d("D2")),
        s.memory.word(d("D3")),
        s.memory.word(d("D11")),
        s.memory.word(d("D12")),
    ];
    let trace = s.memory.timing_io.pulse_trace.len();
    crate::timing_io::advance_by(&mut s.memory, 5000);
    assert_watchdog_y_off(&s);
    assert!(s.memory.timing_io.pulse_outputs.is_empty());
    assert_eq!(s.memory.timing_io.pulse_trace.len(), trace);
    assert_eq!(
        feedback,
        [
            s.memory.word(d("D1")),
            s.memory.word(d("D2")),
            s.memory.word(d("D3")),
            s.memory.word(d("D11")),
            s.memory.word(d("D12"))
        ]
    );
    assert_eq!(s.memory.timing_io.pending_input_events(), 0);
    assert!(s.memory.timing_io.input_owners.values().all(
        |o| matches!(o,crate::timing_io::ResourceOwner::External(name) if name=="external-fixture")
    ));
    assert_eq!(s.memory.timing_io.input_owners.len(), 2);
    assert_eq!(s.memory.timing_io.output_owners.len(), 2);
    // A direct new capture visit, not an old asynchronous engine, starts fresh.
    // It preserves the latched sample values until that explicit visit occurs.
}
#[test]
fn non_watchdog_source_abort_freezes_existing_output_and_disarms_pwm() {
    let p = compile_il("LD X7\nGPWM D0 D10 Y0\nP0\nLD X7\nCJ P0\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X7"), true);
    s.memory.set_word(d("D0"), 10);
    s.memory.set_word(d("D10"), 20);
    s.control.hard_step_limit = 20;
    s.scan(&p, 0);
    assert!(matches!(s.last_scan, ScanStatus::Fault(_)));
    assert!(!s.memory.system_bit(1008));
    assert!(
        s.memory.physical_output(d("Y0")),
        "source budget abort holds current output; only WDT forces all off"
    );
    assert!(s.memory.timing_io.output_owners.is_empty());
    crate::timing_io::advance_by(&mut s.memory, 25000);
    assert!(s.memory.physical_output(d("Y0")));
}

#[test]
fn old_matrix_completion_does_not_clear_a_later_same_value_source_write() {
    for writer in ["SET M1029", "OUT M1029"] {
        let p = compile_il(&format!(
            "LD X7\nMTR X40 Y40 M10 K2\nLD X6\nDELAY K500\nLD X5\n{writer}\nEND"
        ))
        .unwrap();
        let mut s = Simulator::default();
        let site = usize::from(pc(&p, "MTR X40 Y40 M10 K2"));
        s.memory
            .refresh_io
            .set_matrix_rows(site, vec![1, 2])
            .unwrap();
        s.memory.set_input(d("X7"), true);
        s.memory.set_input(d("X6"), true);
        s.memory.set_input(d("X5"), true);
        scan(&mut s, &p, 0);
        assert!(s.memory.system_bit(1029));
        s.memory.set_input(d("X7"), false);
        s.memory.set_input(d("X6"), false);
        scan(&mut s, &p, 0);
        // No new row completion after cancellation. This source's same-value
        // write is newer than MTR's old completion, and must retain its value.
        let hold = compile_il("LD X4\nOUT Y20\nEND").unwrap();
        scan(&mut s, &hold, 0);
        assert!(s.memory.system_bit(1029), "later {writer} was erased");
    }
}
