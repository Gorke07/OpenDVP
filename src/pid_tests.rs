use super::*;
fn action(wide: bool) -> PidAction {
    PidAction::parse(
        if wide { "DPID" } else { "PID" },
        &["D0", if wide { "D2" } else { "D1" }, "D100", "D50"].map(str::to_owned),
    )
    .unwrap()
}
fn setup(wide: bool) -> (Memory, PidAction) {
    let mut m = Memory::default();
    let a = action(wide);
    for (n, v) in [(0, 10), (1, 100), (2, 0), (3, 0), (4, 0)] {
        m.set_word(d(100 + n), v);
    }
    let (mh, ml, ih, il) = if wide { (7, 9, 11, 13) } else { (6, 7, 8, 9) };
    for (n, v) in [
        (mh, if wide { 2_000_000_000 } else { 32767 }),
        (ml, if wide { -2_000_000_000 } else { -32768 }),
        (ih, 1_000_000),
        (il, -1_000_000),
    ] {
        a.write_numeric_field(&mut m, n, if wide { v } else { v.clamp(-10000, 10000) });
    }
    PidState::begin_scan(&mut m, 100_000);
    (m, a)
}
fn put(m: &mut Memory, a: &PidAction, sv: i32, pv: i32) {
    if a.wide {
        m.set_double_word(a.setpoint, sv);
        m.set_double_word(a.process, pv);
    } else {
        m.set_word(a.setpoint, sv as i16);
        m.set_word(a.process, pv as i16);
    }
}
fn at(m: &mut Memory, a: &PidAction, now: u64) {
    m.timing_io.now_us = now;
    a.execute(m, true, 10);
}
fn visible(m: &Memory, a: &PidAction) -> Vec<i16> {
    let mut devices = BTreeSet::new();
    a.devices(&mut devices);
    devices.into_iter().map(|v| m.word(v)).collect()
}

#[test]
fn parser_matrix_widths_and_complete_model_spans() {
    assert!(PidAction::parse("PIDP", &["D0", "D1", "D100", "D50"].map(str::to_owned)).is_err());
    assert!(PidAction::parse("PID", &["K1", "D1", "D100", "D50"].map(str::to_owned)).is_err());
    assert!(PidAction::parse("DPID", &["D0", "D2", "D9980", "D50"].map(str::to_owned)).is_err());
    let a = PidAction::parse("PID", &["D0", "D1", "D9980", "D50"].map(str::to_owned)).unwrap();
    assert_eq!(a.parameter_words(), 20);
    let mut m = Memory::default();
    m.data_register_limit = Some(4999);
    a.execute(&mut m, true, 1);
    assert!(!m.execution_limitations.is_empty());
}
#[test]
fn pi_equation_sampling_hold_and_public_integral_state() {
    let (mut m, a) = setup(false);
    m.set_word(d(102), 100);
    put(&mut m, &a, 100, 0);
    at(&mut m, &a, 0);
    assert_eq!(m.word(a.output), 110);
    assert_eq!(a.read_integral(&m), 10.0);
    at(&mut m, &a, 50_000);
    assert_eq!(m.word(a.output), 110);
    assert_eq!(a.read_integral(&m), 10.0);
    at(&mut m, &a, 100_000);
    assert_eq!(m.word(a.output), 120);
    assert_eq!(a.read_integral(&m), 20.0);
    a.write_integral(&mut m, 50.0);
    at(&mut m, &a, 200_000);
    assert_eq!(m.word(a.output), 160);
}
#[test]
fn derivative_is_process_value_not_setpoint_in_gain_modes() {
    let (mut m, a) = setup(false);
    m.set_word(d(103), -100);
    put(&mut m, &a, 100, 0);
    at(&mut m, &a, 0);
    assert_eq!(m.word(a.output), 100);
    put(&mut m, &a, 200, 0);
    at(&mut m, &a, 100_000);
    assert_eq!(m.word(a.output), 200);
    put(&mut m, &a, 200, 10);
    at(&mut m, &a, 200_000);
    assert_eq!(m.word(a.output), 90);
    assert_eq!(m.word(d(112)), 10);
}
#[test]
fn automatic_signed_forward_reverse_and_inclusive_deadband() {
    for (mode, sv, pv, result) in [
        (0, 0, 100, -100),
        (1, 0, 100, 0),
        (2, 0, 100, 100),
        (2, 100, 0, 0),
    ] {
        let (mut m, a) = setup(false);
        m.set_word(d(104), mode);
        put(&mut m, &a, sv, pv);
        at(&mut m, &a, 0);
        assert_eq!(m.word(a.output), result);
    }
    let (mut m, a) = setup(false);
    m.set_word(d(105), 5);
    put(&mut m, &a, 105, 100);
    at(&mut m, &a, 0);
    assert_eq!(m.word(a.output), 0);
    put(&mut m, &a, 106, 100);
    at(&mut m, &a, 100_000);
    assert_eq!(m.word(a.output), 6);
}
#[test]
fn mode5_directional_antiwindup_freezes_growth_allows_unwind() {
    let (mut m, a) = setup(false);
    m.set_word(d(104), 5);
    m.set_word(d(102), 100);
    m.set_word(d(106), 50);
    m.set_word(d(107), -50);
    put(&mut m, &a, 100, 0);
    at(&mut m, &a, 0);
    assert_eq!(m.word(a.output), 50);
    assert_eq!(a.read_integral(&m), 0.0);
    m.set_word(d(101), 0);
    a.write_integral(&mut m, 100.0);
    put(&mut m, &a, 0, 100);
    at(&mut m, &a, 100_000);
    assert_eq!(m.word(a.output), 50);
    assert_eq!(a.read_integral(&m), 90.0);
}
#[test]
fn mode9_backcalculates_integral_from_limited_mv() {
    let (mut m, a) = setup(false);
    m.set_word(d(104), 9);
    m.set_word(d(102), 100);
    m.set_word(d(106), 50);
    m.set_word(d(107), -50);
    put(&mut m, &a, 100, 0);
    at(&mut m, &a, 0);
    assert_eq!(m.word(a.output), 50);
    assert_eq!(a.read_integral(&m), -50.0);
}
#[test]
fn manual7_integrates_manual8_freezes_and_return5_is_bumpless() {
    let (mut m, a) = setup(false);
    m.set_word(d(101), 0);
    m.set_word(d(102), 100);
    m.set_word(d(104), 7);
    m.set_word(a.output, 777);
    put(&mut m, &a, 100, 0);
    at(&mut m, &a, 0);
    assert_eq!(m.word(a.output), 777);
    assert_eq!(a.read_integral(&m), 10.0);
    m.set_word(d(104), 8);
    m.set_word(a.output, 888);
    at(&mut m, &a, 100_000);
    assert_eq!(m.word(a.output), 888);
    assert_eq!(a.read_integral(&m), 10.0);
    m.set_word(d(104), 5);
    at(&mut m, &a, 200_000);
    assert_eq!(m.word(a.output), 888);
    assert_eq!(a.read_integral(&m), 888.0);
}
#[test]
fn manual8_really_freezes_user_integral_even_outside_bounds() {
    let (mut m, a) = setup(false);
    m.set_word(d(104), 8);
    m.set_word(d(108), 10);
    a.write_integral(&mut m, 25.0);
    m.set_word(a.output, 777);
    at(&mut m, &a, 0);
    assert_eq!(a.read_integral(&m), 25.0);
    assert_eq!(m.word(a.output), 777);
}
#[test]
fn ti_td_mode_uses_milliseconds_constants_and_error_derivative() {
    let (mut m, a) = setup(false);
    m.set_word(d(104), 10);
    m.set_word(d(102), 1000);
    m.set_word(d(103), 100);
    put(&mut m, &a, 100, 0);
    at(&mut m, &a, 0);
    assert_eq!(m.word(a.output), 110);
    put(&mut m, &a, 200, 0);
    at(&mut m, &a, 100_000);
    assert_eq!(m.word(a.output), 330);
    let (mut m, a) = setup(false);
    m.set_word(d(104), 10);
    m.pid.policies.zero_ti = ZeroTi::HoldWithLimitation;
    let old = visible(&m, &a);
    at(&mut m, &a, 0);
    assert_eq!(visible(&m, &a), old);
    assert!(!m.execution_limitations.is_empty());
}
#[test]
fn dpid_mixed_layout_full_width_values_and_reserved_tail() {
    let (mut m, a) = setup(true);
    m.set_word(d(102), 100);
    m.set_word(d(119), 777);
    m.set_word(d(120), 888);
    put(&mut m, &a, 100_000, 50_000);
    at(&mut m, &a, 0);
    assert_eq!(m.double_word(a.output), 55_000);
    assert_eq!(a.display_value(&m), "55000");
    assert_eq!(a.read_integral(&m), 5000.0);
    assert_eq!(m.double_word(d(117)), 50_000);
    assert_eq!(m.word(d(119)), 777);
    assert_eq!(m.word(d(120)), 888);
}
#[test]
fn parameter_clamps_bound_swaps_and_ts_minimum_are_explicit() {
    let (mut m, a) = setup(false);
    m.set_word(d(100), 1);
    m.set_word(d(101), 32767);
    m.set_word(d(102), -1);
    m.set_word(d(103), -32768);
    m.set_word(d(104), 99);
    m.set_word(d(106), -1);
    m.set_word(d(107), 1);
    m.set_word(d(108), -2);
    m.set_word(d(109), 2);
    at(&mut m, &a, 0);
    assert_eq!(
        [
            m.word(d(100)),
            m.word(d(101)),
            m.word(d(102)),
            m.word(d(103)),
            m.word(d(104))
        ],
        [10, 30000, 0, -30000, 0]
    );
    assert_eq!(
        [
            m.word(d(106)),
            m.word(d(107)),
            m.word(d(108)),
            m.word(d(109))
        ],
        [1, -1, 2, -2]
    );
    m.timing_io.now_us = 450_000;
    PidState::begin_scan(&mut m, 100_000);
    a.execute(&mut m, true, 10);
    assert_eq!(m.word(d(100)), 45);
}
#[test]
fn first_sample_policy_and_false_power_never_clear_public_state() {
    let (mut m, a) = setup(false);
    m.pid.policies.first_sample = FirstSample::AfterPeriod;
    m.set_word(d(102), 100);
    m.set_word(a.output, 777);
    put(&mut m, &a, 100, 0);
    at(&mut m, &a, 0);
    assert_eq!(m.word(a.output), 777);
    at(&mut m, &a, 99_999);
    assert_eq!(m.word(a.output), 777);
    at(&mut m, &a, 100_000);
    assert_eq!(m.word(a.output), 110);
    let old = visible(&m, &a);
    a.execute(&mut m, false, 10);
    assert_eq!(visible(&m, &a), old);
}
#[test]
fn elapsed_dt_and_missed_samples_are_named_policies_not_hidden_catchup() {
    for (policy, expected) in [
        (SampleDt::ConfiguredPeriod, 120),
        (SampleDt::ElapsedSincePreviousSample, 140),
    ] {
        let (mut m, a) = setup(false);
        m.pid.policies.sample_dt = policy;
        m.set_word(d(102), 100);
        put(&mut m, &a, 100, 0);
        at(&mut m, &a, 0);
        at(&mut m, &a, 300_000);
        assert_eq!(m.word(a.output), expected);
    }
    for (policy, expected) in [
        (MissedSamples::OneFromVisit, 120),
        (MissedSamples::OnePreservingPhase, 130),
    ] {
        let (mut m, a) = setup(false);
        m.pid.policies.missed_samples = policy;
        m.set_word(d(102), 100);
        put(&mut m, &a, 100, 0);
        at(&mut m, &a, 0);
        at(&mut m, &a, 350_000);
        at(&mut m, &a, 400_000);
        assert_eq!(m.word(a.output), expected);
    }
}
#[test]
fn unsupported_modes_nan_and_owner_collisions_hold_without_fake_plc_error() {
    for (wide, modes) in [(false, vec![3, 4, 6]), (true, vec![3, 4, 6, 7, 8, 9])] {
        for mode in modes {
            let (mut m, a) = setup(wide);
            m.set_word(d(104), mode);
            m.set_word(d(1067), 123);
            m.set_word(d(1068), 456);
            let old = visible(&m, &a);
            at(&mut m, &a, 0);
            assert_eq!(visible(&m, &a), old);
            assert!(!m.execution_limitations.is_empty());
            assert_eq!(m.word(d(1067)), 123);
            assert_eq!(m.word(d(1068)), 456);
        }
    }
    let (mut m, a) = setup(false);
    m.set_double_word(d(110), f32::NAN.to_bits() as i32);
    let old = visible(&m, &a);
    at(&mut m, &a, 0);
    assert_eq!(visible(&m, &a), old);
    assert!(!m.execution_limitations.is_empty());
    let (mut m, a) = setup(false);
    at(&mut m, &a, 0);
    let mut b = a.clone();
    b.output = d(60);
    assert!(PidAction::validate_blocks(&[a, b.clone()]).is_err());
    b.execute(&mut m, true, 20);
    assert_eq!(m.word(d(60)), 0);
    assert!(!m.execution_limitations.is_empty());
}
#[test]
fn output_alias_uses_documented_local_snapshot_then_write_order() {
    let (mut m, mut a) = setup(false);
    a.output = d(101);
    put(&mut m, &a, 10, 0);
    at(&mut m, &a, 0);
    assert_eq!(m.word(d(101)), 10);
    assert!(!m.pid.notices.is_empty());
}
#[test]
fn disabled_ts_and_repeated_same_timestamp_preserve_sample_count() {
    let (mut m, a) = setup(false);
    m.set_word(d(102), 100);
    put(&mut m, &a, 100, 0);
    at(&mut m, &a, 0);
    at(&mut m, &a, 0);
    at(&mut m, &a, 0);
    assert_eq!(a.read_integral(&m), 10.0);
    m.set_word(d(100), 0);
    let before = visible(&m, &a);
    at(&mut m, &a, 1_000_000);
    assert_eq!(visible(&m, &a), before);
}

fn thermal_setup() -> (Memory, PidAction) {
    let (mut m, a) = setup(false);
    m.pid.policies.thermal = ThermalPolicy::RecoveredEh2Formula;
    m.set_word(d(104), 4);
    m.set_word(d(101), 20);
    m.set_word(d(102), 2);
    m.set_word(d(106), 1000);
    m.set_word(d(107), 0);
    put(&mut m, &a, 100, 90);
    m.set_word(d(112), 90);
    a.write_integral(&mut m, 1.0);
    (m, a)
}
#[test]
fn thermal_policy_is_opt_in_word_only_and_never_autotunes() {
    let (mut m, a) = setup(false);
    m.set_word(d(104), 4);
    let old = visible(&m, &a);
    at(&mut m, &a, 0);
    assert_eq!(visible(&m, &a), old);
    assert!(!m.execution_limitations.is_empty());
    for (wide, mode) in [(true, 4), (false, 3)] {
        let (mut m, a) = setup(wide);
        m.pid.policies.thermal = ThermalPolicy::RecoveredEh2Formula;
        m.set_word(d(104), mode);
        let old = visible(&m, &a);
        at(&mut m, &a, 0);
        assert_eq!(visible(&m, &a), old);
        assert!(!m.execution_limitations.is_empty());
    }
}
#[test]
fn thermal_threshold_equality_and_zero_kp_preserve_previous_pv() {
    for (pv, kp, expected) in [(80, 20, 100), (120, 20, 0), (100, 0, 100)] {
        let (mut m, a) = thermal_setup();
        m.set_word(d(101), kp);
        put(&mut m, &a, 100, pv);
        m.set_word(d(112), 77);
        a.write_integral(&mut m, 7.0);
        at(&mut m, &a, 0);
        assert_eq!(m.word(a.output), expected);
        assert_eq!(a.read_integral(&m), 7.0);
        assert_eq!(m.word(d(112)), 77);
    }
}
#[test]
fn thermal_interior_pi_matches_manual_ts_scaled_equation() {
    let (mut m, a) = thermal_setup();
    // E10; Ts10*0.01*E => I1+1=2; I/Ki=1;
    // ((10+1+0)/Kp20)*Ts10*10 =>55.
    at(&mut m, &a, 0);
    assert_eq!(m.word(a.output), 55);
    assert_eq!(a.read_integral(&m), 2.0);
    assert_eq!(m.word(d(112)), 90);
}
#[test]
fn thermal_derivative_uses_positive_pv_rate_inside_kp_division() {
    let (mut m, a) = thermal_setup();
    m.set_word(d(102), 0);
    m.set_word(d(103), 1);
    put(&mut m, &a, 100, 100);
    m.set_word(d(112), 99);
    a.write_integral(&mut m, 0.0);
    // deltaPV1*100/Ts10=10; Kd1*10. E0. 10/20*100=50.
    at(&mut m, &a, 0);
    assert_eq!(m.word(a.output), 50);
    assert_eq!(m.word(d(112)), 100);
    let (mut m, a) = thermal_setup();
    m.set_word(d(102), 0);
    m.set_word(d(103), -1);
    put(&mut m, &a, 100, 100);
    m.set_word(d(112), 99);
    a.write_integral(&mut m, 7.0);
    at(&mut m, &a, 0);
    assert_eq!(m.word(a.output), 0);
    assert_eq!(m.word(d(112)), 100);
    assert_eq!(a.read_integral(&m), 7.0);
}
#[test]
fn thermal_ki_zero_still_accumulates_raw_i_without_ordinary_i_clamp() {
    let (mut m, a) = thermal_setup();
    m.set_word(d(102), 0);
    m.set_word(d(108), 0);
    m.set_word(d(109), 0);
    a.write_integral(&mut m, 3.0);
    at(&mut m, &a, 0);
    assert_eq!(m.word(a.output), 50);
    assert_eq!(a.read_integral(&m), 4.0);
    assert_eq!(m.word(d(108)), 0);
    assert_eq!(m.word(d(109)), 0);
}
#[test]
fn thermal_integer_bound_equality_and_saturation_suppress_i_commit() {
    for (pv, lower, upper, expected) in [(90, 0, 50, 50), (90, 0, 25, 25), (110, -50, 1000, -50)] {
        let (mut m, a) = thermal_setup();
        m.set_word(d(102), 0);
        m.set_word(d(106), upper);
        m.set_word(d(107), lower);
        put(&mut m, &a, 100, pv);
        m.set_word(d(112), 85);
        a.write_integral(&mut m, 3.0);
        at(&mut m, &a, 0);
        assert_eq!(m.word(a.output), expected);
        assert_eq!(a.read_integral(&m), 3.0);
        assert_eq!(m.word(d(112)), pv as i16);
    }
}
#[test]
fn thermal_negative_integral_can_produce_negative_unclipped_word_mv() {
    let (mut m, a) = thermal_setup();
    m.set_word(d(102), 1);
    m.set_word(d(107), -100);
    a.write_integral(&mut m, -21.0);
    // I=-20, I/Ki=-20, E10 =>(-10/20)*100=-50.
    at(&mut m, &a, 0);
    assert_eq!(m.word(a.output), -50);
    assert_eq!(a.read_integral(&m), -20.0);
}
#[test]
fn thermal_formula_respects_local_first_phase_without_claiming_eh2_initialization() {
    let (mut m, a) = thermal_setup();
    m.pid.policies.first_sample = FirstSample::AfterPeriod;
    m.set_word(d(102), 0);
    m.set_word(a.output, 777);
    a.write_integral(&mut m, 5.0);
    at(&mut m, &a, 0);
    assert_eq!(m.word(a.output), 777);
    assert_eq!(a.read_integral(&m), 5.0);
    at(&mut m, &a, 99_999);
    assert_eq!(m.word(a.output), 777);
    // This named formula profile retains userI, unlike EH2 first-activation
    // clearing; it is deliberately not a full EH2 scheduler/ABI emulation.
    at(&mut m, &a, 100_000);
    assert_eq!(m.word(a.output), 50);
    assert_eq!(a.read_integral(&m), 6.0);
}
#[test]
fn thermal_uses_recovered_truncation_and_blocks_nonfinite_conversion() {
    let (mut m, a) = thermal_setup();
    m.pid.policies.output_rounding = OutputRounding::NearestTiesAway;
    m.set_word(d(101), 30);
    m.set_word(d(102), 0);
    put(&mut m, &a, 100, 98);
    m.set_word(d(112), 98);
    a.write_integral(&mut m, 0.0);
    at(&mut m, &a, 0);
    assert_eq!(m.word(a.output), 6);
    assert!((a.read_integral(&m) - 0.2).abs() < 1e-6);
    let (mut m, a) = thermal_setup();
    m.set_word(d(100), 2000);
    m.set_word(d(101), 1);
    m.set_word(d(102), 1);
    put(&mut m, &a, 100, 100);
    m.set_word(d(112), 99);
    m.set_word(a.output, 777);
    a.write_integral(&mut m, f64::from(f32::MAX));
    at(&mut m, &a, 0);
    assert_eq!(m.word(a.output), 777);
    assert_eq!(m.word(d(112)), 99);
    assert_eq!(a.read_integral(&m), f64::from(f32::MAX));
    assert!(!m.execution_limitations.is_empty());
}

#[test]
fn thermal_bounded_eh2_literal_output_witness_with_explicit_initial_projection() {
    // Independent original runtime evidence: pid-eh2-runtime-second.json,
    // EH2 SHA f911a844552b271ff34a17553c66364cc1d309038b280ca59eeafe068bfec26e.
    // This projects known first-activation I0/PV80 into public local fields;
    // it does not pretend the formula policy implements EH2's activation ABI.
    let (mut m, a) = thermal_setup();
    m.pid.policies.first_sample = FirstSample::AfterPeriod;
    m.set_word(d(101), 50);
    m.set_word(d(102), 10);
    put(&mut m, &a, 100, 80);
    m.set_word(d(112), 80);
    a.write_integral(&mut m, 0.0);
    m.set_word(a.output, 23100);
    for (visit, expected) in [23100, 40, 40, 41, 41, 42, 42].into_iter().enumerate() {
        at(&mut m, &a, visit as u64 * 100_000);
        assert_eq!(
            m.word(a.output),
            expected,
            "finite EH2 output projection visit{visit}"
        );
    }
}
