//! Compiler/VM/event integration with declared virtual time, not PLC hardware
//! timing claims. Ramp assertions use final count only, not an invented curve.
use crate::execution_plan::PlanOp;
use crate::plc::{Device, Program, ScanStatus, Simulator, compile_il};

fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn scan(s: &mut Simulator, p: &Program, ms: u64) {
    s.scan(p, ms);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(
        s.memory.execution_limitations.is_empty(),
        "{:?}",
        s.memory.execution_limitations
    );
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
        .unwrap_or_else(|| panic!("missing action {label}"))
}
fn until(s: &mut Simulator, p: &Program, target: u16) {
    let mut n = 0;
    while s.current_plan_pc() != Some(target) {
        assert!(s.current_plan_pc().is_some());
        s.execute_plan_step(p).unwrap();
        n += 1;
        assert!(n < 2000);
    }
}
fn finish(s: &mut Simulator, p: &Program) {
    let mut n = 0;
    while s.current_plan_pc().is_some() {
        s.execute_plan_step(p).unwrap();
        n += 1;
        assert!(n < 2000);
    }
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(
        s.memory.execution_limitations.is_empty(),
        "{:?}",
        s.memory.execution_limitations
    );
}
fn enabled() -> Simulator {
    let mut s = Simulator::default();
    s.memory.set_input(d("X7"), true);
    // Explicit modern virtual REF-position capability, not an assumption about
    // the hash-bound legacy simulator or every hardware firmware revision.
    s.memory.refresh_io.profile.pulse_refresh = true;
    s
}

#[test]
fn finite_plsy_matches_published_total_after_a_long_virtual_clock_jump() {
    let p = compile_il("LD X6\nSET M1672\nREF Y0 K8\nMOV D1030 D0\nLD X7\nPLSY K1000 K200 Y0\nEND")
        .unwrap();
    let mut s = enabled();
    scan(&mut s, &p, 0);
    assert!(!s.memory.system_bit(1029));
    crate::timing_io::advance_by(&mut s.memory, 1_000_000);
    assert!(s.memory.system_bit(1029));
    assert!(!s.memory.physical_output(d("Y0")));
    s.memory.set_input(d("X6"), true);
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.word(d("D0")), 200);
    scan(&mut s, &p, 0);
    assert_eq!(
        s.memory.word(d("D0")),
        200,
        "held command does not automatically repeat finite burst"
    );
}
#[test]
fn dplsy_signed_count_updates_position_in_the_selected_direction() {
    let p =
        compile_il("LD X6\nSET M1672\nREF Y0 K8\nDMOV D1030 D20\nLD X7\nDPLSY K1000 K-200 Y0\nEND")
            .unwrap();
    let mut s = enabled();
    s.memory.set_word(d("D1220"), 1);
    scan(&mut s, &p, 0);
    crate::timing_io::advance_by(&mut s.memory, 250000);
    s.memory.set_input(d("X6"), true);
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.double_word(d("D20")), -200);
    assert!(s.memory.system_bit(1029));
}
#[test]
fn continuous_plsy_zero_count_does_not_report_finite_completion() {
    let p = compile_il("LD X7\nPLSY K1000 K0 Y0\nEND").unwrap();
    let mut s = enabled();
    scan(&mut s, &p, 0);
    crate::timing_io::advance_by(&mut s.memory, 1_000_000);
    scan(&mut s, &p, 0);
    assert!(!s.memory.system_bit(1029));
    assert_eq!(s.memory.double_word(d("D1030")), 1000);
    s.memory.set_input(d("X7"), false);
    scan(&mut s, &p, 0);
    assert!(!s.memory.physical_output(d("Y0")));
}
#[test]
fn live_plsy_frequency_and_latched_count_are_sampled_at_actual_visits() {
    let p = compile_il("LD X7\nPLSY D0 D1 Y0\nEND").unwrap();
    let mut s = enabled();
    s.memory.set_word(d("D0"), 1000);
    s.memory.set_word(d("D1"), 200);
    scan(&mut s, &p, 0);
    crate::timing_io::advance_by(&mut s.memory, 50000);
    s.memory.set_word(d("D0"), 2000);
    s.memory.set_word(d("D1"), 1);
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.double_word(d("D1030")), 50);
    assert!(!s.memory.system_bit(1029));
    crate::timing_io::advance_by(&mut s.memory, 75000);
    scan(&mut s, &p, 0);
    assert!(s.memory.system_bit(1029));
    assert_eq!(s.memory.double_word(d("D1030")), 200);
}
#[test]
fn pwm_published_y1_example_runs_between_visits_with_one_second_high() {
    let p = compile_il("LD X7\nPWM K1000 K2000 Y1\nEND").unwrap();
    let mut s = enabled();
    scan(&mut s, &p, 0);
    assert!(s.memory.physical_output(d("Y1")));
    crate::timing_io::advance_by(&mut s.memory, 999999);
    assert!(s.memory.physical_output(d("Y1")));
    crate::timing_io::advance_by(&mut s.memory, 1);
    assert!(!s.memory.physical_output(d("Y1")));
    crate::timing_io::advance_by(&mut s.memory, 1_000_000);
    assert!(s.memory.physical_output(d("Y1")));
    s.memory.set_input(d("X7"), false);
    scan(&mut s, &p, 0);
    assert!(!s.memory.physical_output(d("Y1")));
}
#[test]
fn pwm_invalid_width_is_output_off_without_arithmetic_error_flags() {
    let p = compile_il("LD X7\nPWM D0 D1 Y0\nEND").unwrap();
    let mut s = enabled();
    for (width, period) in [(0, 10), (-1, 10), (11, 10), (1, 0)] {
        s.memory.set_word(d("D0"), width);
        s.memory.set_word(d("D1"), period);
        scan(&mut s, &p, 0);
        assert!(!s.memory.physical_output(d("Y0")));
        assert!(!s.memory.system_bit(1067));
        assert!(!s.memory.system_bit(1068));
    }
    s.memory.set_word(d("D0"), 10);
    s.memory.set_word(d("D1"), 10);
    scan(&mut s, &p, 0);
    crate::timing_io::advance_by(&mut s.memory, 1_000_000);
    assert!(s.memory.physical_output(d("Y0")));
}
#[test]
fn pulse_output_ownership_protects_physical_pin_from_ordinary_out_and_ref() {
    let p = compile_il("LD X7\nPWM K10 K10 Y0\nLDI X7\nOUT Y0\nLD X7\nREF Y0 K8\nOUT Y10\nEND")
        .unwrap();
    let mut s = enabled();
    s.begin_plan_scan(&p, 0, false).unwrap();
    until(&mut s, &p, pc(&p, "REF Y0 K8"));
    assert!(!s.memory.bit(d("Y0")));
    assert!(s.memory.physical_output(d("Y0")));
    s.execute_plan_step(&p).unwrap();
    assert!(s.memory.physical_output(d("Y0")));
    assert!(!s.memory.physical_output(d("Y10")));
    finish(&mut s, &p);
    assert!(s.memory.physical_output(d("Y0")));
    assert!(s.memory.physical_output(d("Y10")));
}
#[test]
fn both_plsr_forms_complete_the_latched_total_without_asserting_ramp_quantization() {
    for instruction in ["PLSR", "DPLSR"] {
        let p = compile_il(&format!("LD X7\n{instruction} K1000 D0 K20 Y0\nEND")).unwrap();
        let mut s = enabled();
        s.memory.set_double_word(d("D0"), 10000);
        scan(&mut s, &p, 0);
        s.memory.set_double_word(d("D0"), 1);
        crate::timing_io::advance_by(&mut s.memory, 1_000_000);
        scan(&mut s, &p, 0);
        assert!(
            !s.memory.system_bit(1029),
            "{instruction} keeps initial target"
        );
        crate::timing_io::advance_by(&mut s.memory, 20_000_000);
        scan(&mut s, &p, 0);
        assert!(s.memory.system_bit(1029));
        assert_eq!(s.memory.double_word(d("D1030")), 10000, "{instruction}");
    }
}
#[test]
fn immediate_pause_preserves_finite_remaining_count_and_resumes() {
    let p =
        compile_il("LD X1\nSET M1078\nLDI X1\nRST M1078\nLD X7\nPLSY K1000 K200 Y0\nEND").unwrap();
    let mut s = enabled();
    scan(&mut s, &p, 0);
    crate::timing_io::advance_by(&mut s.memory, 50000);
    s.memory.set_input(d("X1"), true);
    scan(&mut s, &p, 0);
    let held = s.memory.double_word(d("D1030"));
    crate::timing_io::advance_by(&mut s.memory, 500000);
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.double_word(d("D1030")), held);
    assert!(!s.memory.system_bit(1029));
    s.memory.set_input(d("X1"), false);
    scan(&mut s, &p, 0);
    crate::timing_io::advance_by(&mut s.memory, 250000);
    scan(&mut s, &p, 0);
    assert!(s.memory.system_bit(1029));
    assert_eq!(s.memory.double_word(d("D1030")), 200);
}
#[test]
fn completed_y2_flag_is_not_a_one_scan_mtr_pulse() {
    let p = compile_il("LD X7\nPLSY K1000 K10 Y2\nEND").unwrap();
    let mut s = enabled();
    scan(&mut s, &p, 0);
    crate::timing_io::advance_by(&mut s.memory, 20000);
    for _ in 0..3 {
        scan(&mut s, &p, 0);
        assert!(s.memory.system_bit(1102));
    }
    s.memory.set_input(d("X7"), false);
    scan(&mut s, &p, 0);
    assert!(!s.memory.system_bit(1102));
}
#[test]
fn newer_pulse_completion_survives_older_mtr_completion_aging() {
    let p = compile_il("LD X7\nMTR X40 Y40 M10 K2\nPLSY K1000 K60 Y0\nEND").unwrap();
    let mut s = enabled();
    let site = usize::from(pc(&p, "MTR X40 Y40 M10 K2"));
    s.memory
        .refresh_io
        .set_matrix_rows(site, vec![1, 2])
        .unwrap();
    scan(&mut s, &p, 50);
    assert!(s.memory.system_bit(1029));
    scan(&mut s, &p, 10);
    assert!(s.memory.system_bit(1029));
    scan(&mut s, &p, 0);
    assert!(
        s.memory.system_bit(1029),
        "older MTR pulse may not erase fresh retained Y0 completion"
    );
}
#[test]
fn call_for_revisits_one_pulse_site_without_restarting_its_latched_target() {
    let p = compile_il(
        "LD X7\nFOR K3\nCALL P0\nNEXT\nFEND\nP0\nLD X7\nPLSY K1000 K60 Y0\nDELAY K100\nSRET\nEND",
    )
    .unwrap();
    let mut s = enabled();
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.timing_io.now_us, 30000);
    assert!(!s.memory.system_bit(1029));
    scan(&mut s, &p, 0);
    assert!(s.memory.system_bit(1029));
    assert_eq!(
        s.memory.pulse_output.positions.get(&0),
        Some(&60),
        "one site reached its original60pulse total across CALL/FOR visits"
    );
}
#[test]
fn watchdog_abort_prevents_future_clock_events_from_reasserting_pulse_output() {
    let p = compile_il("LD X7\nPLSY K1000 K0 Y0\nDELAY K20\nEND").unwrap();
    let mut s = enabled();
    s.memory.set_word(d("D1000"), 1);
    s.scan(&p, 0);
    assert!(matches!(s.last_scan, ScanStatus::Fault(_)));
    assert!(s.memory.system_bit(1008));
    crate::timing_io::advance_by(&mut s.memory, 500000);
    assert!(!s.memory.physical_output(d("Y0")));
    assert!(!s.memory.bit(d("Y0")));
    s.restart_preserving_file_store();
    s.memory.set_input(d("X7"), true);
    s.memory.set_word(d("D1000"), 10);
    scan(&mut s, &p, 0);
    assert!(!s.memory.system_bit(1008));
}
#[test]
fn earlier_gpwm_reservation_is_not_stolen_by_a_later_train() {
    let p = compile_il("LD X7\nGPWM D0 D10 Y0\nPLSY K1000 K20 Y0\nEND").unwrap();
    let mut s = enabled();
    s.memory.set_word(d("D0"), 10);
    s.memory.set_word(d("D10"), 10);
    s.scan(&p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(!s.memory.execution_limitations.is_empty());
    assert_eq!(s.memory.pulse_output.active_count(), 0);
    assert!(s.memory.physical_output(d("Y0")));
    assert!(matches!(
        s.memory.timing_io.output_owners.get(&d("Y0")),
        Some(crate::timing_io::ResourceOwner::Gpwm(_))
    ));
}
#[test]
fn earlier_csfo_pair_claim_prevents_train_from_taking_its_pin() {
    let p = compile_il("LD X7\nCSFO X0 D0 D10\nPLSY K1000 K20 Y0\nEND").unwrap();
    let mut s = enabled();
    s.memory.set_word(d("D0"), 1);
    s.memory.set_word(d("D10"), 100);
    s.scan(&p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(!s.memory.execution_limitations.is_empty());
    assert_eq!(s.memory.pulse_output.active_count(), 0);
    assert!(matches!(
        s.memory.timing_io.output_owners.get(&d("Y0")),
        Some(crate::timing_io::ResourceOwner::Csfo(_))
    ));
}
#[test]
fn pulse_owned_strobe_prevents_mtr_from_mutating_any_matrix_cell() {
    let p = compile_il("LD X7\nPLSY K1000 K0 Y0\nMTR X40 Y0 M10 K2\nEND").unwrap();
    let mut s = enabled();
    let site = usize::from(pc(&p, "MTR X40 Y0 M10 K2"));
    s.memory
        .refresh_io
        .set_matrix_rows(site, vec![255, 255])
        .unwrap();
    s.scan(&p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(!s.memory.execution_limitations.is_empty());
    assert_eq!(s.memory.refresh_io.matrix_owner(), None);
    assert!(s.memory.pulse_output.is_busy(0));
    crate::timing_io::advance_by(&mut s.memory, 100000);
    assert!(!s.memory.bit(d("M10")));
    assert!(!s.memory.bit(d("M20")));
}
#[test]
fn virtual_mark_input_stops_after_its_explicit_additional_pulse_distance() {
    let p = compile_il("LD X7\nDPLSR K1000 K10000 K20 Y0\nEND").unwrap();
    let mut s = enabled();
    s.memory.set_system(1156, true);
    s.memory.set_double_word(d("D1232"), 10);
    scan(&mut s, &p, 0);
    crate::timing_io::advance_by(&mut s.memory, 50000);
    let at_mark = s.memory.pulse_output.positions.get(&0).copied().unwrap();
    s.memory
        .timing_io
        .enqueue_input_edge(50001, d("X4"), true)
        .unwrap();
    crate::timing_io::advance_by(&mut s.memory, 1);
    assert!(s.memory.system_bit(1108));
    crate::timing_io::advance_by(&mut s.memory, 1_000_000);
    assert!(s.memory.system_bit(1538));
    assert_eq!(
        s.memory.pulse_output.positions.get(&0),
        Some(&(at_mark + 10))
    );
    assert!(
        !s.memory.system_bit(1029),
        "mark is a pause, not completion of the10000pulse move"
    );
    assert!(!s.memory.physical_output(d("Y0")));
}
fn physical_bank_is_off(s: &Simulator) {
    for index in 0..256 {
        let device = Device {
            area: crate::plc::Area::Y,
            index,
        };
        assert!(
            !s.memory.physical_output(device),
            "{device} must remain inhibited"
        );
    }
}
#[test]
fn m1034_masks_owned_pwm_and_restores_publication_after_release() {
    let p = compile_il("LD X7\nPWM K10 K10 Y0\nLD X1\nSET M1034\nLDI X1\nRST M1034\nEND").unwrap();
    let mut s = enabled();
    scan(&mut s, &p, 0);
    assert!(s.memory.physical_output(d("Y0")));
    s.memory.set_input(d("X1"), true);
    s.begin_plan_scan(&p, 0, false).unwrap();
    until(&mut s, &p, pc(&p, "SET M1034"));
    s.execute_plan_step(&p).unwrap();
    assert!(s.memory.system_bit(1034));
    physical_bank_is_off(&s);
    finish(&mut s, &p);
    assert!(
        s.memory.pulse_output.is_busy(0),
        "output inhibit does not discard the virtual owner"
    );
    crate::timing_io::advance_by(&mut s.memory, 50000);
    physical_bank_is_off(&s);
    s.memory.set_input(d("X1"), false);
    scan(&mut s, &p, 0);
    crate::timing_io::advance_by(&mut s.memory, 1);
    assert!(!s.memory.system_bit(1034));
    assert!(s.memory.physical_output(d("Y0")));
}
#[test]
fn m1034_masks_mtr_strobe_transitions_without_stealing_matrix_owner() {
    let p =
        compile_il("LD X7\nMTR X40 Y40 M10 K2\nLD X1\nSET M1034\nLDI X1\nRST M1034\nEND").unwrap();
    let mut s = enabled();
    let site = usize::from(pc(&p, "MTR X40 Y40 M10 K2"));
    s.memory
        .refresh_io
        .set_matrix_rows(site, vec![1, 2])
        .unwrap();
    scan(&mut s, &p, 0);
    assert!(s.memory.physical_output(d("Y40")));
    s.memory.set_input(d("X1"), true);
    scan(&mut s, &p, 0);
    physical_bank_is_off(&s);
    crate::timing_io::advance_by(&mut s.memory, 25000);
    physical_bank_is_off(&s);
    assert_eq!(s.memory.refresh_io.matrix_owner(), Some(site));
    s.memory.set_input(d("X1"), false);
    scan(&mut s, &p, 0);
    crate::timing_io::advance_by(&mut s.memory, 25000);
    assert!(s.memory.physical_output(d("Y40")));
    assert!(!s.memory.physical_output(d("Y41")));
}
#[test]
fn m1034_masks_csfo_pulse_publication_across_new_input_edges_then_releases() {
    let p = compile_il("LD X7\nCSFO X0 D0 D10\nLD X1\nSET M1034\nLDI X1\nRST M1034\nEND").unwrap();
    let mut s = enabled();
    s.memory.set_word(d("D0"), 1);
    s.memory.set_word(d("D10"), 100);
    scan(&mut s, &p, 0);
    for (at, level) in [
        (1000, true),
        (1100, false),
        (2000, true),
        (2100, false),
        (4000, true),
        (4100, false),
        (5000, true),
        (5100, false),
    ] {
        s.memory
            .timing_io
            .enqueue_input_edge(at, d("X0"), level)
            .unwrap();
    }
    crate::timing_io::advance_by(&mut s.memory, 3000);
    assert_eq!(s.memory.timing_io.pulse_outputs[&0].frequency_hz, 1000);
    assert!(s.memory.physical_output(d("Y0")));
    s.memory.set_input(d("X1"), true);
    scan(&mut s, &p, 0);
    physical_bank_is_off(&s);
    crate::timing_io::advance_by(&mut s.memory, 3000);
    physical_bank_is_off(&s);
    assert!(!s.memory.timing_io.pulse_outputs.is_empty());
    s.memory.set_input(d("X1"), false);
    scan(&mut s, &p, 0);
    crate::timing_io::advance_by(&mut s.memory, 500);
    assert!(!s.memory.system_bit(1034));
    assert!(s.memory.physical_output(d("Y0")));
}
#[test]
fn same_time_pause_and_resume_are_visible_to_the_following_logic_load() {
    let p = compile_il(
        "LD X7\nPLSY K1000 K0 Y0\nSET M1078\nLD Y0\nOUT Y10\nLD X7\nRST M1078\nLD Y0\nOUT Y11\nEND",
    )
    .unwrap();
    let mut s = enabled();
    s.begin_plan_scan(&p, 0, false).unwrap();
    until(&mut s, &p, pc(&p, "SET M1078"));
    assert!(s.memory.physical_output(d("Y0")));
    s.execute_plan_step(&p).unwrap();
    assert!(!s.memory.physical_output(d("Y0")));
    assert!(!s.memory.bit(d("Y0")));
    assert!(s.memory.system_bit(1538));
    until(&mut s, &p, pc(&p, "RST M1078"));
    assert!(!s.memory.bit(d("Y10")));
    s.execute_plan_step(&p).unwrap();
    assert!(s.memory.physical_output(d("Y0")));
    assert!(s.memory.bit(d("Y0")));
    finish(&mut s, &p);
    assert!(!s.memory.bit(d("Y10")));
    assert!(s.memory.bit(d("Y11")));
}
