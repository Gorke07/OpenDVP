use super::*;
fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn a(name: &str, args: &[&str]) -> PulseAction {
    PulseAction::parse(name, &args.iter().map(|v| (*v).into()).collect::<Vec<_>>()).unwrap()
}
fn run(m: &mut Memory, pc: u16, name: &str, args: &[&str]) {
    a(name, args).execute(m, true, pc);
}
fn advance(m: &mut Memory, t: u64) {
    m.timing_io.now_us = t;
    PulseState::advance(m, t);
}
fn pos(m: &Memory, y: u8) -> i32 {
    m.pulse_output.positions.get(&y).copied().unwrap_or(0)
}
fn clean(m: &Memory) {
    assert!(
        m.execution_limitations.is_empty(),
        "{:?}",
        m.execution_limitations
    );
}

#[test]
fn five_actual_forms_headers_lengths_and_f_width_matrix() {
    for (name, args, h, w) in [
        ("PLSY", vec!["K1000", "K1", "Y0"], 0x8200, 7),
        ("DPLSY", vec!["K100000", "K2147483647", "Y0"], 0x8300, 13),
        ("PWM", vec!["F1", "K10", "Y1"], 0x8400, 7),
        ("PLSR", vec!["K1000", "K1", "K20", "Y0"], 0x8600, 9),
        ("DPLSR", vec!["K100000", "K1", "K20", "Y0"], 0x8700, 17),
    ] {
        let v = a(name, &args);
        assert_eq!(v.header(), h);
        assert_eq!(v.words(), w);
    }
    for name in ["DPWM", "PLSYP", "PLSRP"] {
        assert!(PulseAction::parse(name, &["K1".into(), "K2".into(), "Y0".into()]).is_err());
    }
    assert!(PulseAction::parse("DPLSY", &["F1".into(), "K1".into(), "Y0".into()]).is_err());
}
#[test]
fn plsy_finite_manual_period_count_completion_and_false_rearm() {
    let mut m = Memory::default();
    let v = a("PLSY", &["K1000", "K3", "Y0"]);
    v.execute(&mut m, true, 1);
    advance(&mut m, 2500);
    assert_eq!(pos(&m, 0), 2);
    assert!(!m.system_bit(1029));
    advance(&mut m, 3000);
    assert_eq!(pos(&m, 0), 3);
    assert!(m.system_bit(1029));
    assert!(!m.physical_output(d("Y0")));
    advance(&mut m, 100_000);
    assert_eq!(pos(&m, 0), 3);
    v.execute(&mut m, false, 1);
    assert!(!m.system_bit(1029));
    v.execute(&mut m, true, 1);
    advance(&mut m, 103_000);
    assert_eq!(pos(&m, 0), 6);
    clean(&m);
}
#[test]
fn plsy_s2_latched_s1_live_on_actual_visit_and_zero_disables_without_done() {
    let mut m = Memory::default();
    m.set_word(d("D0"), 1000);
    m.set_word(d("D1"), 10);
    let v = a("PLSY", &["D0", "D1", "Y0"]);
    v.execute(&mut m, true, 1);
    advance(&mut m, 1000);
    m.set_word(d("D0"), 2000);
    m.set_word(d("D1"), 1);
    advance(&mut m, 2000);
    assert_eq!(pos(&m, 0), 2);
    v.execute(&mut m, true, 1);
    advance(&mut m, 3000);
    assert_eq!(pos(&m, 0), 4);
    assert!(!m.system_bit(1029));
    m.set_word(d("D0"), 0);
    v.execute(&mut m, true, 1);
    advance(&mut m, 10000);
    assert_eq!(pos(&m, 0), 4);
    assert!(!m.system_bit(1029));
    clean(&m);
}
#[test]
fn continuous_signed_minimum_magnitude_and_direction_pin_are_not_abs_overflow() {
    let mut m = Memory::default();
    m.set_word(d("D1220"), 1);
    run(&mut m, 1, "DPLSY", &["K1000", "K-2147483648", "Y0"]);
    assert!(m.physical_output(d("Y1")));
    advance(&mut m, 10000);
    assert_eq!(pos(&m, 0), -10);
    assert!(!m.system_bit(1029));
    clean(&m);
    let mut c = Memory::default();
    run(&mut c, 1, "PLSY", &["K1000", "K0", "Y0"]);
    advance(&mut c, 100000);
    assert_eq!(pos(&c, 0), 100);
    assert!(!c.system_bit(1029));
}
#[test]
fn limits_are_model_width_channel_specific_and_do_not_loop_per_pulse() {
    for (model, y, wide, expected) in [
        (PulseModel::Es2, "Y0", true, 100000),
        (PulseModel::Es2, "Y1", true, 10000),
        (PulseModel::Ss2, "Y0", true, 10000),
        (PulseModel::Ec5, "Y4", true, 50000),
    ] {
        let mut m = Memory::default();
        m.pulse_output.model = model;
        run(
            &mut m,
            1,
            if wide { "DPLSY" } else { "PLSY" },
            &["K1000000", "K0", y],
        );
        advance(&mut m, 1_000_000);
        assert_eq!(pos(&m, d(y).index as u8), expected);
        clean(&m);
    }
}
#[test]
fn plsy_low_frequency_is_an_explicit_candidate_not_an_unstated_scale() {
    let mut m = Memory::default();
    m.set_system(1190, true);
    run(&mut m, 1, "PLSY", &["K100", "K0", "Y0"]);
    assert_eq!(m.pulse_output.active_count(), 0);
    assert!(!m.execution_limitations.is_empty());
    m.execution_limitations.clear();
    m.pulse_output.low_frequency = LowFrequency::HundredthsHertzCandidate;
    run(&mut m, 1, "PLSY", &["K100", "K0", "Y0"]);
    advance(&mut m, 1_000_000);
    assert_eq!(pos(&m, 0), 1);
    clean(&m);
}
#[test]
fn pwm_default_units_live_width_and_latched_unit_flags() {
    let mut m = Memory::default();
    m.set_word(d("D0"), 2);
    m.set_word(d("D1"), 10);
    let v = a("PWM", &["D0", "D1", "Y0"]);
    v.execute(&mut m, true, 1);
    advance(&mut m, 199);
    assert!(m.physical_output(d("Y0")));
    advance(&mut m, 200);
    assert!(!m.physical_output(d("Y0")));
    m.set_system(1112, true);
    m.set_word(d("D0"), 5);
    v.execute(&mut m, true, 1);
    advance(&mut m, 499);
    assert!(m.physical_output(d("Y0")));
    advance(&mut m, 500);
    assert!(!m.physical_output(d("Y0")));
    advance(&mut m, 1000);
    assert_eq!(pos(&m, 0), 1);
    clean(&m);
}
#[test]
fn pwm_equal_width_is_high_invalid_width_produces_no_counts_or_error_flags() {
    let mut m = Memory::default();
    run(&mut m, 1, "PWM", &["K10", "K10", "Y1"]);
    advance(&mut m, 25_000);
    assert!(m.physical_output(d("Y1")));
    assert_eq!(pos(&m, 1), 2);
    let mut invalid = Memory::default();
    invalid.set_system(1029, true);
    run(&mut invalid, 1, "PWM", &["K11", "K10", "Y0"]);
    advance(&mut invalid, 1_000_000);
    assert_eq!(pos(&invalid, 0), 0);
    assert!(!invalid.physical_output(d("Y0")));
    assert!(!invalid.system_bit(1067));
    assert!(!invalid.system_bit(1068));
    assert!(invalid.system_bit(1029));
    clean(&invalid);
}
#[test]
fn ss2_microsecond_pwm_hardware_minimum_is_not_silently_treated_as_ideal() {
    let mut m = Memory::default();
    m.pulse_output.model = PulseModel::Ss2;
    m.set_system(1116, true);
    run(&mut m, 1, "PWM", &["K20", "K30", "Y0"]);
    assert!(!m.execution_limitations.is_empty());
    assert_eq!(m.pulse_output.active_count(), 0);
}
#[test]
fn immediate_pause_holds_fraction_and_count_then_resumes() {
    let mut m = Memory::default();
    run(&mut m, 1, "PLSY", &["K1000", "K10", "Y0"]);
    advance(&mut m, 1500);
    m.set_system(1078, true);
    advance(&mut m, 1500);
    assert!(m.system_bit(1538));
    assert_eq!(m.pulse_output.next_deadline_us(), None);
    advance(&mut m, 9500);
    assert_eq!(pos(&m, 0), 1);
    m.set_system(1078, false);
    advance(&mut m, 10000);
    assert_eq!(pos(&m, 0), 2);
    clean(&m);
}
#[test]
fn done_is_latched_but_manual_clear_is_not_reasserted_on_advance() {
    let mut m = Memory::default();
    let v = a("PLSY", &["K1000", "K1", "Y0"]);
    v.execute(&mut m, true, 1);
    advance(&mut m, 1000);
    assert!(m.system_bit(1029));
    m.set_system(1029, false);
    advance(&mut m, 5000);
    v.execute(&mut m, true, 1);
    assert!(!m.system_bit(1029));
    assert_eq!(pos(&m, 0), 1);
}
#[test]
fn auto_restart_waits_one_full_idle_scan_without_false_condition() {
    let mut m = Memory::default();
    m.set_system(1347, true);
    let v = a("PLSY", &["K1000", "K1", "Y0"]);
    PulseState::begin_scan(&mut m);
    v.execute(&mut m, true, 1);
    advance(&mut m, 1000);
    PulseState::end_scan(&mut m);
    PulseState::begin_scan(&mut m);
    v.execute(&mut m, true, 1);
    advance(&mut m, 2000);
    assert_eq!(pos(&m, 0), 1);
    PulseState::end_scan(&mut m);
    PulseState::begin_scan(&mut m);
    v.execute(&mut m, true, 1);
    advance(&mut m, 3000);
    assert_eq!(pos(&m, 0), 2);
    clean(&m);
}
#[test]
fn same_channel_first_visited_wins_instead_of_generic_conflict_rejection() {
    let mut m = Memory::default();
    PulseState::begin_scan(&mut m);
    run(&mut m, 1, "PLSY", &["K1000", "K0", "Y1"]);
    run(&mut m, 2, "PWM", &["K1", "K10", "Y1"]);
    advance(&mut m, 10000);
    assert_eq!(pos(&m, 1), 10);
    assert_eq!(m.pulse_output.active_count(), 1);
    clean(&m);
}
#[test]
fn foreign_csfo_owner_blocks_and_false_releases_only_own_claims() {
    let mut m = Memory::default();
    m.timing_io
        .output_owners
        .insert(d("Y0"), ResourceOwner::Csfo(7));
    run(&mut m, 1, "PLSY", &["K1000", "K0", "Y0"]);
    assert_eq!(m.pulse_output.active_count(), 0);
    assert!(!m.execution_limitations.is_empty());
    assert_eq!(m.timing_io.output_owners[&d("Y0")], ResourceOwner::Csfo(7));
    let mut own = Memory::default();
    let v = a("PLSY", &["K1000", "K0", "Y1"]);
    v.execute(&mut own, true, 2);
    v.execute(&mut own, false, 2);
    assert!(own.pulse_output.claimed_outputs().is_empty());
}
#[test]
fn advance_works_with_timing_state_taken_out_and_abort_cannot_resume_asynchronously() {
    let mut m = Memory::default();
    run(&mut m, 1, "PLSY", &["K1000", "K0", "Y0"]);
    let timing = std::mem::take(&mut m.timing_io);
    PulseState::advance(&mut m, 1000);
    assert_eq!(pos(&m, 0), 1);
    m.timing_io = timing;
    let level = m.physical_output(d("Y0"));
    PulseState::abort(&mut m);
    PulseState::advance(&mut m, 10000);
    assert_eq!(pos(&m, 0), 1);
    assert_eq!(m.physical_output(d("Y0")), level);
    assert!(m.pulse_output.claimed_outputs().is_empty());
}
#[test]
fn public_pv_is_visit_refreshed_and_manual_offset_is_preserved() {
    let mut m = Memory::default();
    let v = a("PLSY", &["K1000", "K0", "Y0"]);
    v.execute(&mut m, true, 1);
    advance(&mut m, 10000);
    assert_eq!(m.double_word(d("D1030")), 0);
    assert_eq!(pos(&m, 0), 10);
    m.set_double_word(d("D1030"), 100);
    v.execute(&mut m, true, 1);
    assert_eq!(m.double_word(d("D1030")), 110);
    advance(&mut m, 20000);
    assert_eq!(pos(&m, 0), 120);
    m.set_double_word(d("D1030"), 120);
    v.execute(&mut m, true, 1);
    assert_eq!(
        pos(&m, 0),
        120,
        "REF-current value must not be interpreted as another coordinate offset"
    );
    clean(&m);
}
#[test]
fn plsr_has_twenty_ten_steps_latches_all_operands_and_finishes_exact_count() {
    for (y, steps) in [("Y0", 20), ("Y1", 10)] {
        let mut m = Memory::default();
        m.set_word(d("D0"), 1000);
        m.set_word(d("D1"), 100);
        m.set_word(d("D2"), 100);
        let v = a("PLSR", &["D0", "D1", "D2", y]);
        v.execute(&mut m, true, 1);
        let r = &m.pulse_output.runs[&1];
        assert_eq!(
            r.segments
                .iter()
                .filter(|s| s.phase == MotionPhase::RampUp)
                .count()
                + 1,
            steps
        );
        m.set_word(d("D0"), 1);
        m.set_word(d("D1"), 1);
        v.execute(&mut m, true, 1);
        advance(&mut m, 10_000_000);
        assert_eq!(pos(&m, d(y).index as u8), 100);
        assert!(m.system_bit(port(d(y).index as u8).unwrap().complete));
        clean(&m);
    }
}
#[test]
fn plsr_s_curve_and_unlisted_mode_never_silently_become_a_straight_ramp() {
    let mut m = Memory::default();
    m.set_system(1257, true);
    run(&mut m, 1, "PLSR", &["K1000", "K100", "K100", "Y0"]);
    assert_eq!(m.pulse_output.active_count(), 0);
    assert!(
        m.execution_limitations
            .iter()
            .any(|v| v.contains("S-eğrisi"))
    );
    let mut zero = Memory::default();
    run(&mut zero, 1, "PLSR", &["K1000", "K0", "K100", "Y0"]);
    assert_eq!(zero.pulse_output.active_count(), 0);
    assert!(!zero.execution_limitations.is_empty());
}
#[test]
fn asymmetric_and_fixed_slope_times_have_explicit_channel_fields() {
    let mut m = Memory::default();
    m.set_system(1534, true);
    m.set_word(d("D1348"), 200);
    m.set_word(d("D1340"), 100);
    m.set_system(1604, true);
    m.set_double_word(d("D1410"), 1900);
    run(&mut m, 1, "PLSR", &["K1000", "K10000", "K100", "Y0"]);
    let r = &m.pulse_output.runs[&1];
    assert_eq!(r.up_us, 50000);
    assert_eq!(r.down_us, 100000);
    clean(&m);
}
#[test]
fn mark_masks_are_strict_boundaries_and_negative_ss2_masks_are_blocked() {
    let mut m = Memory::default();
    m.set_system(1156, true);
    m.set_double_word(d("D1026"), 2);
    m.set_double_word(d("D1100"), 8);
    m.set_double_word(d("D1232"), 1);
    run(&mut m, 1, "PLSR", &["K1000", "K100", "K20", "Y0"]);
    let mut r = m.pulse_output.runs.remove(&1).unwrap();
    r.emitted = 2;
    m.pulse_output.runs.insert(1, r);
    PulseState::input_edge(&mut m, d("X4"), false, true, 0);
    assert!(!m.pulse_output.runs[&1].mark_stop);
    m.pulse_output.runs.get_mut(&1).unwrap().emitted = 3;
    PulseState::input_edge(&mut m, d("X4"), false, true, 0);
    assert!(m.pulse_output.runs[&1].mark_stop);
    assert!(m.system_bit(1108));
    let mut ss = Memory::default();
    ss.pulse_output.model = PulseModel::Ss2;
    ss.set_system(1156, true);
    ss.set_double_word(d("D1026"), -1);
    ss.set_double_word(d("D1232"), 1);
    run(&mut ss, 1, "PLSR", &["K1000", "K100", "K20", "Y0"]);
    PulseState::input_edge(&mut ss, d("X4"), false, true, 0);
    assert!(ss.execution_limitations.iter().any(|v| v.contains("SS2")));
}
#[test]
fn negative_mark_stop_is_immediate_and_zero_mark_conflict_is_profiled() {
    let mut m = Memory::default();
    m.set_system(1156, true);
    m.set_double_word(d("D1232"), -1);
    run(&mut m, 1, "PLSR", &["K1000", "K100", "K20", "Y0"]);
    PulseState::input_edge(&mut m, d("X4"), false, true, 0);
    assert!(m.system_bit(1538));
    assert!(!m.physical_output(d("Y0")));
    assert!(!m.system_bit(1029));
    let mut zero = Memory::default();
    zero.set_system(1156, true);
    run(&mut zero, 1, "PLSR", &["K1000", "K100", "K20", "Y0"]);
    PulseState::input_edge(&mut zero, d("X4"), false, true, 0);
    assert!(!zero.pulse_output.runs[&1].mark_stop);
}
#[test]
fn implicit_mark_inputs_are_in_program_device_projection() {
    let v = a("PLSR", &["K1000", "K10", "K20", "Y0"]);
    let mut set = BTreeSet::new();
    v.devices(&mut set);
    assert!(set.contains(&d("X4")));
    assert!(set.contains(&dev(Area::M, 1156)));
    assert!(set.contains(&d("D1232")));
}
#[test]
fn control_service_reacts_at_zero_time_without_repainting_unrelated_logical_out() {
    let mut m = Memory::default();
    run(&mut m, 1, "PLSY", &["K1000", "K0", "Y0"]);
    m.set_internal(d("Y0"), false);
    PulseState::service_controls(&mut m);
    assert!(!m.bit(d("Y0")));
    m.set_system(1078, true);
    PulseState::service_controls(&mut m);
    assert!(!m.physical_output(d("Y0")));
    assert!(m.system_bit(1538));
    m.set_system(1078, false);
    PulseState::service_controls(&mut m);
    assert!(m.physical_output(d("Y0")));
}
#[test]
fn invalid_later_owner_does_not_steal_previous_valid_channel() {
    let mut m = Memory::default();
    run(&mut m, 1, "PLSY", &["K1000", "K0", "Y0"]);
    PulseState::begin_scan(&mut m);
    m.set_system(1257, true);
    run(&mut m, 2, "PLSR", &["K1000", "K100", "K20", "Y0"]);
    assert!(m.pulse_output.runs[&1].active);
    assert!(!m.pulse_output.runs.contains_key(&2));
    assert_eq!(
        m.timing_io.output_owners[&d("Y0")],
        ResourceOwner::External("PULSE:1".into())
    );
}
#[test]
fn live_zero_frequency_and_pwm_updates_change_levels_on_the_actual_visit() {
    let mut m = Memory::default();
    m.set_word(d("D0"), 1000);
    let v = a("PLSY", &["D0", "K0", "Y0"]);
    v.execute(&mut m, true, 1);
    assert!(m.physical_output(d("Y0")));
    m.set_word(d("D0"), 0);
    v.execute(&mut m, true, 1);
    assert!(!m.physical_output(d("Y0")));
    let mut pwm = Memory::default();
    pwm.set_word(d("D0"), 1);
    let v = a("PWM", &["D0", "K10", "Y0"]);
    v.execute(&mut pwm, true, 1);
    advance(&mut pwm, 200);
    assert!(!pwm.physical_output(d("Y0")));
    pwm.set_word(d("D0"), 5);
    v.execute(&mut pwm, true, 1);
    assert!(pwm.physical_output(d("Y0")));
}

#[test]
fn resumed_ramp_cannot_reclaim_mark_input_acquired_by_spd_while_inactive() {
    let mut m = Memory::default();
    m.set_system(1156, true);
    let ramp = a("PLSR", &["K1000", "K1000", "K100", "Y0"]);
    ramp.execute(&mut m, true, 1);
    advance(&mut m, 20_000);
    ramp.execute(&mut m, false, 1);
    assert!(!m.timing_io.input_owners.contains_key(&d("X4")));
    let speed = crate::high_speed::HighSpeedAction::parse(
        "SPD",
        &["X4".into(), "K10".into(), "D200".into()],
    )
    .unwrap();
    speed.execute(&mut m, true, 9);
    let position = pos(&m, 0);
    ramp.execute(&mut m, true, 1);
    assert!(!m.pulse_output.runs[&1].active);
    assert!(m.pulse_output.claimed_inputs().is_empty());
    assert_eq!(
        m.timing_io.input_owners[&d("X4")],
        ResourceOwner::External("SPD:9".into())
    );
    assert!(m.execution_limitations.iter().any(|s| s.contains("pasif")));
    advance(&mut m, 30_000);
    assert_eq!(pos(&m, 0), position);
}

#[test]
fn ss2_live_one_microsecond_pwm_rejects_bandwidth_violation_and_holds_old_width() {
    let mut m = Memory::default();
    m.pulse_output.model = PulseModel::Ss2;
    m.set_system(1116, true);
    m.set_word(d("D0"), 21);
    let pwm = a("PWM", &["D0", "K40", "Y0"]);
    pwm.execute(&mut m, true, 1);
    advance(&mut m, 20);
    assert!(m.physical_output(d("Y0")));
    for invalid in [20, 1] {
        m.set_word(d("D0"), invalid);
        pwm.execute(&mut m, true, 1);
        assert_eq!(m.pulse_output.runs[&1].pwm.unwrap().0, 21);
        assert!(m.physical_output(d("Y0")));
    }
    assert!(m.execution_limitations.iter().any(|s| s.contains("SS2")));
    assert!(!m.system_bit(1067));
    assert!(!m.system_bit(1068));
    advance(&mut m, 21);
    assert!(!m.physical_output(d("Y0")));
    m.execution_limitations.clear();
    m.set_word(d("D0"), 22);
    pwm.execute(&mut m, true, 1);
    assert_eq!(m.pulse_output.runs[&1].pwm.unwrap().0, 22);
    assert!(m.physical_output(d("Y0")));
    clean(&m);
}

#[test]
fn unrelated_false_pulse_visit_does_not_repaint_another_logical_y_at_same_time() {
    let program = crate::plc::compile_il(
        "LD X7\nPWM K10 K10 Y0\nLDI X7\nOUT Y0\nLD X6\nPWM K1 K2 Y1\nLD Y0\nOUT Y10\nEND",
    )
    .unwrap();
    let mut sim = crate::plc::Simulator::default();
    sim.memory.set_input(d("X7"), true);
    sim.scan(&program, 0);
    assert_eq!(sim.last_scan, crate::plc::ScanStatus::Complete);
    assert!(sim.memory.physical_output(d("Y0")));
    assert!(
        !sim.memory.bit(d("Y10")),
        "false PWM Y1 must not reassert logical Y0 before the following LD"
    );
    assert_eq!(sim.memory.pulse_output.active_count(), 1);
    clean(&sim.memory);
}

#[test]
fn stopped_pulse_backend_does_not_capture_mark_edges_until_running_again() {
    let mut m = Memory::default();
    m.set_system(1156, true);
    m.set_double_word(d("D1232"), -1);
    run(&mut m, 1, "PLSR", &["K1000", "K100", "K20", "Y0"]);
    let level = m.physical_output(d("Y0"));
    PulseState::set_running(&mut m, false);
    PulseState::input_edge(&mut m, d("X4"), false, true, 0);
    assert!(!m.pulse_output.runs[&1].mark_stop);
    assert!(!m.system_bit(1108));
    assert!(!m.system_bit(1538));
    assert_eq!(m.physical_output(d("Y0")), level);
    PulseState::set_running(&mut m, true);
    PulseState::input_edge(&mut m, d("X4"), false, true, 0);
    assert!(m.pulse_output.runs[&1].mark_stop);
    assert!(m.system_bit(1108));
    assert!(m.system_bit(1538));
    clean(&m);
}
