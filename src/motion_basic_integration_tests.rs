//! Actual compiler/VM/event coverage for explicit documented virtual axes.
//! SharedTwentyStepCandidate is selected explicitly; intermediate ramp pulse
//! counts are not treated as hardware goldens. ServoFrame is an external,
//! prevalidated transaction input, not a recovered wire-checksum implementation.
use crate::execution_plan::PlanOp;
use crate::motion_basic::{ServoFrame, TrajectoryPolicy};
use crate::plc::{Action, Device, Program, ScanStatus, Simulator, compile_il};
fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn sim() -> Simulator {
    let mut s = Simulator::default();
    s.memory.set_input(d("X20"), true);
    s.memory.motion_basic.trajectory = TrajectoryPolicy::SharedTwentyStepCandidate;
    s
}
fn scan(s: &mut Simulator, p: &Program) {
    s.scan(p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(
        s.memory.execution_limitations.is_empty(),
        "{:?}",
        s.memory.execution_limitations
    );
}
fn pc(p: &Program, prefix: &str) -> u16 {
    p.execution_plan
        .nodes
        .iter()
        .find_map(|n| match n.op {
            PlanOp::Action {
                network, action, ..
            } if p.networks[network].actions[action]
                .label()
                .starts_with(prefix) =>
            {
                Some(n.pc)
            }
            _ => None,
        })
        .unwrap()
}
fn advance(s: &mut Simulator, us: u64) {
    crate::timing_io::advance_by(&mut s.memory, us);
    assert!(
        s.memory.execution_limitations.is_empty(),
        "{:?}",
        s.memory.execution_limitations
    );
}
fn position(s: &Simulator, y: u8) -> i32 {
    s.memory.pulse_output.axis_position(y, &s.memory)
}
#[test]
fn eight_genuine_headers_and_word_offsets_stay_local_while_native_is_guarded() {
    for (line, header, len) in [
        ("DABSR X0 Y4 D0", 0x5701, 13),
        ("DZRN K2000 K100 X4 Y0", 0x5901, 17),
        ("PLSV K1000 Y0 Y1", 0x5a01, 7),
        ("DPLSV K1000 Y0 Y1", 0x5b01, 13),
        ("DDRVI K2000 K1000 Y0 Y1", 0x5d01, 17),
        ("DDRVA K2000 K1000 Y0 Y1", 0x5f01, 17),
        ("DICF D0 K0 Y0", 0xad01, 13),
        ("DICFP D0 K0 Y0", 0xad11, 13),
    ] {
        let p = compile_il(&format!("LD X20\n{line}\nMOV K7 D100\nEND")).unwrap();
        let e = crate::es2_encoder::local_execution_layout(&p).unwrap();
        assert_eq!(e.known_word_at(e.action_pc[0]), Some(header), "{line}");
        assert_eq!(e.action_pc[1], e.action_pc[0] + len, "{line}");
        assert!(crate::es2_encoder::encode(&p).is_err());
    }
}
#[test]
fn only_documented_pulse_form_and_one_abs_read_site_are_accepted() {
    for line in [
        "ABSR X0 Y4 D0",
        "ZRNP K2000 K100 X4 Y0",
        "PLSVP K1000 Y0 Y1",
        "DPLSVP K1000 Y0 Y1",
        "DDRVIP K1 K1000 Y0 Y1",
        "DDRVAP K1 K1000 Y0 Y1",
        "DRVA K1 K1000 Y0 Y1",
    ] {
        assert!(
            compile_il(&format!("LD X20\n{line}\nEND")).is_err(),
            "{line}"
        );
    }
    assert!(compile_il("LD X20\nDABSR X0 Y4 D0\nLD X21\nDABSR X10 Y10 D10\nEND").is_err());
}
#[test]
fn positive_velocity_has_exact_constant_frequency_count_and_shared_pv() {
    let p = compile_il("LD X20\nDPLSV K20000 Y0 Y1\nEND").unwrap();
    let mut s = sim();
    scan(&mut s, &p);
    advance(&mut s, 100_000);
    assert_eq!(position(&s, 0), 2000);
    assert!(!s.memory.bit(d("Y1")));
    scan(&mut s, &p);
    assert_eq!(s.memory.double_word(d("D1030")), 2000);
    assert!(!s.memory.system_bit(1029));
}
#[test]
fn negative_velocity_direction_and_live_same_sign_speed_change() {
    let p = compile_il("LD X20\nDPLSV D0 Y0 Y1\nEND").unwrap();
    let mut s = sim();
    s.memory.set_double_word(d("D0"), -1000);
    scan(&mut s, &p);
    advance(&mut s, 100_000);
    assert_eq!(position(&s, 0), -100);
    assert!(s.memory.bit(d("Y1")));
    s.memory.set_double_word(d("D0"), -2000);
    scan(&mut s, &p);
    advance(&mut s, 100_000);
    assert_eq!(position(&s, 0), -300);
}
#[test]
fn sign_change_pauses_for_one_scan_not_one_for_visit() {
    let p = compile_il("LD X20\nFOR K3\nDPLSV D0 Y0 Y1\nNEXT\nEND").unwrap();
    let mut s = sim();
    s.memory.set_double_word(d("D0"), 1000);
    scan(&mut s, &p);
    advance(&mut s, 100_000);
    s.memory.set_double_word(d("D0"), -1000);
    scan(&mut s, &p);
    assert!(
        s.memory
            .pulse_output
            .axis_snapshot(pc(&p, "DPLSV"))
            .unwrap()
            .paused
    );
    advance(&mut s, 100_000);
    assert_eq!(position(&s, 0), 100);
    scan(&mut s, &p);
    advance(&mut s, 100_000);
    assert_eq!(position(&s, 0), 0);
}
#[test]
fn false_velocity_contact_stops_immediately_and_rearms() {
    let p = compile_il("LD X20\nPLSV K1000 Y0 Y1\nEND").unwrap();
    let mut s = sim();
    scan(&mut s, &p);
    advance(&mut s, 100_000);
    s.memory.set_input(d("X20"), false);
    scan(&mut s, &p);
    advance(&mut s, 100_000);
    assert_eq!(position(&s, 0), 100);
    assert!(!s.memory.physical_output(d("Y0")));
    s.memory.set_input(d("X20"), true);
    scan(&mut s, &p);
    advance(&mut s, 100_000);
    assert_eq!(position(&s, 0), 200);
}
#[test]
fn relative_move_latches_target_and_frequency_until_rearm() {
    let p = compile_il("LD X20\nDDRVI D0 D2 Y0 Y1\nEND").unwrap();
    let mut s = sim();
    s.memory.set_double_word(d("D0"), 3000);
    s.memory.set_double_word(d("D2"), 2000);
    scan(&mut s, &p);
    s.memory.set_double_word(d("D0"), 9999);
    s.memory.set_double_word(d("D2"), 100);
    scan(&mut s, &p);
    advance(&mut s, 10_000_000);
    assert_eq!(position(&s, 0), 3000);
    assert!(s.memory.system_bit(1029));
    scan(&mut s, &p);
    assert_eq!(s.memory.double_word(d("D1030")), 3000);
}
#[test]
fn absolute_target_uses_difference_from_current_pv_not_target_sign() {
    let p = compile_il("LD X20\nDDRVA K2000 K2000 Y0 Y1\nEND").unwrap();
    let mut s = sim();
    s.memory.set_double_word(d("D1030"), 5000);
    scan(&mut s, &p);
    assert!(s.memory.bit(d("Y1")));
    advance(&mut s, 10_000_000);
    assert_eq!(position(&s, 0), 2000);
    assert!(s.memory.system_bit(1029));
    assert!(s.memory.bit(d("Y1")));
    s.memory.set_input(d("X20"), false);
    scan(&mut s, &p);
    assert!(!s.memory.bit(d("Y1")));
}
#[test]
fn two_channels_share_position_banks_without_sharing_axis_owner() {
    let p = compile_il("LD X20\nDDRVI K2000 K2000 Y0 Y1\nDDRVI K-3000 K2000 Y2 Y3\nEND").unwrap();
    let mut s = sim();
    scan(&mut s, &p);
    advance(&mut s, 10_000_000);
    scan(&mut s, &p);
    assert_eq!(s.memory.double_word(d("D1030")), 2000);
    assert_eq!(s.memory.double_word(d("D1336")), -3000);
    assert!(s.memory.system_bit(1029));
    assert!(s.memory.system_bit(1102));
}
fn two_speed(absolute: bool) -> (Simulator, Program) {
    let p = compile_il(&format!(
        "LD X20\n{} D0 D10 Y0 Y1\nEND",
        if absolute { "DDRVA" } else { "DDRVI" }
    ))
    .unwrap();
    let mut s = sim();
    s.memory.set_system(1119, true);
    for (reg, value) in [
        ("D0", 100000),
        ("D2", 200000),
        ("D10", 100000),
        ("D12", 50000),
    ] {
        s.memory.set_double_word(d(reg), value);
    }
    scan(&mut s, &p);
    advance(&mut s, 20_000_000);
    scan(&mut s, &p);
    (s, p)
}
#[test]
fn published_two_speed_relative_totals_300000_and_clears_m1119() {
    let (s, _) = two_speed(false);
    assert_eq!(position(&s, 0), 300000);
    assert!(!s.memory.system_bit(1119));
    assert!(s.memory.system_bit(1029));
}
#[test]
fn published_two_speed_absolute_totals_200000_not_300000() {
    let (s, _) = two_speed(true);
    assert_eq!(position(&s, 0), 200000);
    assert!(!s.memory.system_bit(1119));
}
#[test]
fn home_on_dog_starts_at_jog_and_falling_edge_zeroes_position() {
    let p = compile_il("LD X20\nDZRN K2000 K1000 X4 Y0\nEND").unwrap();
    let mut s = sim();
    s.memory.set_input(d("X4"), true);
    scan(&mut s, &p);
    assert_eq!(
        s.memory
            .pulse_output
            .axis_snapshot(pc(&p, "DZRN"))
            .unwrap()
            .frequency_millihz,
        1_000_000
    );
    advance(&mut s, 10000);
    s.memory.set_input(d("X4"), false);
    assert_eq!(position(&s, 0), 0);
    assert_eq!(s.memory.double_word(d("D1030")), 0);
    assert!(s.memory.system_bit(1029));
}
#[test]
fn home_queued_dog_edges_use_actual_timestamps_inside_delay() {
    let p = compile_il("LD X20\nDZRN K2000 K1000 X4 Y0\nDELAY K100\nEND").unwrap();
    let mut s = sim();
    s.memory
        .timing_io
        .enqueue_input_edge(1000, d("X4"), true)
        .unwrap();
    s.memory
        .timing_io
        .enqueue_input_edge(5000, d("X4"), false)
        .unwrap();
    scan(&mut s, &p);
    assert_eq!(position(&s, 0), 0);
    assert!(s.memory.system_bit(1029));
}
#[test]
fn home_clear_output_defers_completion_until_20ms_falling_edge() {
    let p = compile_il("LD X20\nDZRN K2000 K1000 X4 Y0\nEND").unwrap();
    let mut s = sim();
    s.memory.set_system(1346, true);
    s.memory.set_input(d("X4"), true);
    scan(&mut s, &p);
    s.memory.set_input(d("X4"), false);
    assert!(s.memory.physical_output(d("Y4")));
    assert!(!s.memory.system_bit(1029));
    advance(&mut s, 19_999);
    assert!(!s.memory.system_bit(1029));
    advance(&mut s, 1);
    assert!(!s.memory.physical_output(d("Y4")));
    assert!(s.memory.system_bit(1029));
}
#[test]
fn home_second_z_counts_only_rising_edges_after_dog_release() {
    let p = compile_il("LD X20\nDZRN K2000 K1000 X4 Y0\nEND").unwrap();
    let mut s = sim();
    s.memory.set_word(d("D1312"), -2);
    s.memory.set_input(d("X4"), true);
    scan(&mut s, &p);
    s.memory.set_input(d("X4"), false);
    s.memory.set_input(d("X2"), true);
    assert!(!s.memory.system_bit(1029));
    s.memory.set_input(d("X2"), false);
    assert!(!s.memory.system_bit(1029));
    s.memory.set_input(d("X2"), true);
    assert!(s.memory.system_bit(1029));
    assert_eq!(position(&s, 0), 0);
}
#[test]
fn home_left_limit_turns_direction_before_returning_to_dog() {
    let p = compile_il("LD X20\nDZRN K2000 K1000 X4 Y0\nEND").unwrap();
    let mut s = sim();
    s.memory.set_system(1307, true);
    scan(&mut s, &p);
    assert!(s.memory.bit(d("Y1")));
    s.memory.set_input(d("X5"), true);
    assert!(!s.memory.bit(d("Y1")));
    s.memory.set_input(d("X5"), false);
    s.memory.set_input(d("X4"), true);
    assert!(s.memory.bit(d("Y1")));
    s.memory.set_input(d("X4"), false);
    assert!(s.memory.system_bit(1029));
}
#[test]
fn explicit_valid_mrj2_transaction_commits_low_high_words_only_after_deadline() {
    let p = compile_il("LD X20\nDABSR X0 Y4 D0\nEND").unwrap();
    let mut s = sim();
    s.memory.motion_basic.set_servo_frame(
        pc(&p, "DABSR"),
        ServoFrame::Mitsubishi {
            position: 0x12345678,
            check_valid: true,
        },
    );
    scan(&mut s, &p);
    advance(&mut s, 1_999_999);
    assert_eq!(s.memory.double_word(d("D0")), 0);
    assert!(!s.memory.system_bit(1029));
    advance(&mut s, 1);
    assert_eq!(s.memory.word(d("D0")) as u16, 0x5678);
    assert_eq!(s.memory.word(d("D1")) as u16, 0x1234);
    assert!(s.memory.system_bit(1029));
}
#[test]
fn explicit_asda_frame_preserves_four_fields_and_success_status() {
    let p = compile_il("LD X20\nDABSR X0 Y4 D0\nEND").unwrap();
    let mut s = sim();
    s.memory.set_system(1177, true);
    s.memory.motion_basic.set_servo_frame(
        pc(&p, "DABSR"),
        ServoFrame::Delta {
            status: 1,
            multiturn: 27,
            puu: 0x12345678,
            valid: true,
        },
    );
    scan(&mut s, &p);
    advance(&mut s, 2_000_000);
    assert_eq!(
        [
            s.memory.word(d("D0")) as u16,
            s.memory.word(d("D1")) as u16,
            s.memory.word(d("D2")) as u16,
            s.memory.word(d("D3")) as u16
        ],
        [1, 27, 0x5678, 0x1234]
    );
    assert!(s.memory.system_bit(1580));
    assert!(!s.memory.system_bit(1581));
}
#[test]
fn absent_servo_response_never_fabricates_zero_success_even_under_inhibit() {
    let p = compile_il("LD X20\nSET M1034\nDABSR X0 Y4 D0\nEND").unwrap();
    let mut s = sim();
    s.memory.set_double_word(d("D0"), 1234);
    scan(&mut s, &p);
    advance(&mut s, 10_000_000);
    assert_eq!(s.memory.double_word(d("D0")), 1234);
    assert!(!s.memory.system_bit(1029));
    assert!(!s.memory.physical_output(d("Y4")));
    assert!(!s.memory.physical_output(d("Y5")));
}
#[test]
fn false_abs_read_contact_cancels_a_configured_pending_transaction() {
    let p = compile_il("LD X20\nDABSR X0 Y4 D0\nEND").unwrap();
    let mut s = sim();
    s.memory.motion_basic.set_servo_frame(
        pc(&p, "DABSR"),
        ServoFrame::Mitsubishi {
            position: 77,
            check_valid: true,
        },
    );
    scan(&mut s, &p);
    s.memory.set_input(d("X20"), false);
    scan(&mut s, &p);
    advance(&mut s, 3_000_000);
    assert_eq!(s.memory.word(d("D0")), 0);
    assert!(!s.memory.system_bit(1029));
    assert!(!s.memory.physical_output(d("Y4")));
}
#[test]
fn dicfp_same_dplsy_source_updates_once_per_rising_contact_and_ignores_gap() {
    // A syntactically valid wide operand crosses the configured SS2 memory end.
    // DPLSY does not read DICF's gap; only DVSPO uses that operand.
    let p = compile_il("LD X22\nCJ P0\nLD X20\nDPLSY D0 K0 Y0\nP0\nLD X21\nDICFP D0 D4999 Y0\nEND")
        .unwrap();
    let mut s = sim();
    s.memory.data_register_limit = Some(4999);
    s.memory.pulse_output.model = crate::pulse_output::PulseModel::Ss2;
    s.memory.set_double_word(d("D0"), 1000);
    scan(&mut s, &p);
    advance(&mut s, 100000);
    // Skip DPLSY after arming: its own live S1 updates must not mask a
    // broken DICFP edge latch.
    s.memory.set_input(d("X22"), true);
    s.memory.set_double_word(d("D0"), 2000);
    s.memory.set_input(d("X21"), true);
    scan(&mut s, &p);
    advance(&mut s, 100000);
    assert_eq!(position(&s, 0), 300);
    s.memory.set_double_word(d("D0"), 3000);
    scan(&mut s, &p);
    advance(&mut s, 100000);
    assert_eq!(position(&s, 0), 500);
    s.memory.set_input(d("X21"), false);
    scan(&mut s, &p);
    s.memory.set_input(d("X21"), true);
    scan(&mut s, &p);
    advance(&mut s, 100000);
    assert_eq!(position(&s, 0), 800);
}
#[test]
fn dicf_equal_values_at_different_registers_are_not_same_source() {
    let p = compile_il("LD X20\nDPLSY D0 K0 Y0\nDICF D2 K0 Y0\nEND").unwrap();
    let mut s = sim();
    s.memory.set_double_word(d("D0"), 1000);
    s.memory.set_double_word(d("D2"), 1000);
    s.scan(&p, 0);
    assert!(!s.memory.execution_limitations.is_empty());
    assert_eq!(s.memory.pulse_output.axis_owner(0), Some(pc(&p, "DPLSY")));
}
#[test]
fn first_pulse_owner_wins_over_later_motion_without_duplicating_axis() {
    let p = compile_il("LD X20\nPLSY K1000 K0 Y0\nDPLSV K2000 Y0 Y1\nEND").unwrap();
    let mut s = sim();
    scan(&mut s, &p);
    advance(&mut s, 100000);
    assert_eq!(position(&s, 0), 100);
    assert_eq!(s.memory.pulse_output.axis_owner(0), Some(pc(&p, "PLSY")));
}
#[test]
fn external_peripheral_strobe_reservation_blocks_motion_transaction() {
    let p = compile_il("LD X20\nSEGL D0 Y0 K0\nDPLSV K1000 Y0 Y1\nEND").unwrap();
    let mut s = sim();
    s.scan(&p, 0);
    assert!(!s.memory.execution_limitations.is_empty());
    assert!(s.memory.pulse_output.axis_owner(0).is_none());
}
#[test]
fn mtr_reserved_pair_blocks_motion_without_mutating_matrix_results() {
    let p = compile_il("LD X20\nMTR X40 Y0 M200 K2\nDPLSV K1000 Y0 Y1\nEND").unwrap();
    let mut s = sim();
    s.memory
        .refresh_io
        .set_matrix_rows(pc(&p, "MTR") as usize, vec![1, 2])
        .unwrap();
    s.scan(&p, 0);
    assert!(!s.memory.execution_limitations.is_empty());
    assert!(s.memory.pulse_output.axis_owner(0).is_none());
    assert!(!s.memory.bit(d("M200")));
}
#[test]
fn m1034_blocks_physical_axis_while_virtual_count_remains_explicit() {
    let p = compile_il("LD X20\nDPLSV K1000 Y0 Y1\nSET M1034\nEND").unwrap();
    let mut s = sim();
    scan(&mut s, &p);
    advance(&mut s, 12345);
    assert!(!s.memory.physical_output(d("Y0")));
    assert!(!s.memory.physical_output(d("Y1")));
}
#[test]
fn watchdog_abort_stops_axis_and_future_output_events() {
    let p =
        compile_il("LD X20\nDPLSV K1000 Y0 Y1\nDELAY K1000\nDELAY K1000\nDELAY K1\nEND").unwrap();
    let mut s = sim();
    s.scan(&p, 0);
    assert_ne!(s.last_scan, ScanStatus::Complete);
    assert!(s.memory.system_bit(1008));
    let held = position(&s, 0);
    crate::timing_io::advance_by(&mut s.memory, 1_000_000);
    assert_eq!(position(&s, 0), held);
    assert!(!s.memory.physical_output(d("Y0")));
    assert!(s.memory.pulse_output.axis_owner(0).is_none());
}
#[test]
fn call_and_for_revisit_one_actual_motion_pc_without_restarting_relative_move() {
    let p = compile_il(
        "LD X20\nFOR K3\nCALL P0\nNEXT\nFEND\nP0\nLD X20\nDDRVI K3000 K2000 Y0 Y1\nSRET\nEND",
    )
    .unwrap();
    let mut s = sim();
    scan(&mut s, &p);
    assert_eq!(s.memory.pulse_output.axis_owner(0), Some(pc(&p, "DDRVI")));
    advance(&mut s, 10_000_000);
    scan(&mut s, &p);
    assert_eq!(position(&s, 0), 3000);
    assert_eq!(
        p.networks
            .iter()
            .flat_map(|n| &n.actions)
            .filter(|a| matches!(a, Action::MotionBasic(_)))
            .count(),
        1
    );
}

#[test]
fn ordinary_dicf_changes_same_source_frequency_at_this_visit_without_waiting_for_rearm() {
    let p = compile_il("LD X22\nCJ P0\nLD X20\nDPLSY D0 K0 Y0\nP0\nLD X21\nDICF D0 K0 Y0\nEND")
        .unwrap();
    let mut s = sim();
    s.memory.set_double_word(d("D0"), 1000);
    scan(&mut s, &p);
    advance(&mut s, 100000);
    s.memory.set_input(d("X22"), true);
    s.memory.set_input(d("X21"), true);
    s.memory.set_double_word(d("D0"), 2000);
    scan(&mut s, &p);
    advance(&mut s, 100000);
    assert_eq!(position(&s, 0), 300);
    s.memory.set_double_word(d("D0"), 3000);
    scan(&mut s, &p);
    advance(&mut s, 100000);
    assert_eq!(position(&s, 0), 600);
}
