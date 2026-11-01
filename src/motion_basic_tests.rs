use super::*;
fn dev(v: &str) -> Device {
    v.parse().unwrap()
}
fn action(name: &str, args: &[&str]) -> MotionAction {
    MotionAction::parse(
        name,
        &args.iter().map(|v| v.to_string()).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn memory() -> Memory {
    let mut m = Memory::default();
    PulseState::set_running(&mut m, true);
    m.set_word(dev("D1340"), 100);
    m.set_word(dev("D1343"), 100);
    m
}
fn execute(m: &mut Memory, a: &MotionAction, power: bool, pc: u16) {
    a.execute(m, power, pc);
}
fn advance(m: &mut Memory, at: u64) {
    PulseState::advance(m, at);
    MotionBasicState::advance(m, at, &BTreeMap::new());
    m.timing_io.now_us = at;
}
fn no_limits(m: &Memory) {
    assert!(
        m.execution_limitations.is_empty(),
        "{:?}",
        m.execution_limitations
    );
}
#[test]
fn genuine_forms_keep_wide_bit_layout_and_operand_order() {
    for (name, args, h, w) in [
        ("DABSR", vec!["X0", "Y4", "D0"], 0x5701, 13),
        ("DZRN", vec!["K1000", "K100", "X4", "Y0"], 0x5901, 17),
        ("PLSV", vec!["K100", "Y0", "Y1"], 0x5a01, 7),
        ("DPLSV", vec!["K100", "Y0", "M0"], 0x5b01, 13),
        ("DDRVI", vec!["K100", "K1000", "Y0", "Y1"], 0x5d01, 17),
        ("DDRVA", vec!["K100", "K1000", "Y0", "Y1"], 0x5f01, 17),
        ("DICF", vec!["D0", "K0", "Y0"], 0xad01, 13),
    ] {
        let a = action(name, &args);
        assert_eq!(a.name(), name);
        assert_eq!(a.header(), h);
        assert_eq!(a.words(), w);
        assert_eq!(a.operands().len(), args.len());
    }
}
#[test]
fn shared_backend_counts_signed_pulses_and_preserves_direction_on_completion() {
    let mut m = memory();
    let a = action("DDRVI", &["K-20", "K100", "Y0", "M0"]);
    execute(&mut m, &a, true, 1);
    advance(&mut m, 1_000_000);
    assert_eq!(m.pulse_output.positions[&0], -20);
    assert!(m.bit(dev("M0")));
    assert!(m.system_bit(1029));
    execute(&mut m, &a, false, 1);
    assert!(!m.bit(dev("M0")));
    no_limits(&m);
}
#[test]
fn absolute_move_uses_signed_wide_distance_not_wrapping_subtraction() {
    let mut m = memory();
    m.set_double_word_raw(dev("D1030"), i32::MIN);
    let a = action("DDRVA", &["K2147483647", "K1000", "Y0", "Y1"]);
    execute(&mut m, &a, true, 1);
    let s = m.pulse_output.axis_snapshot(1).unwrap();
    assert_eq!(s.position, i32::MIN);
    assert!(!m.bit(dev("Y1")));
    no_limits(&m);
}
#[test]
fn velocity_reverses_after_exactly_one_new_scan_and_keeps_position_continuous() {
    let mut m = memory();
    m.set_word(dev("D0"), 100);
    let a = action("PLSV", &["D0", "Y0", "Y1"]);
    MotionBasicState::begin_scan(&mut m);
    execute(&mut m, &a, true, 1);
    advance(&mut m, 100_000);
    assert_eq!(m.pulse_output.positions[&0], 10);
    m.set_word(dev("D0"), -100);
    execute(&mut m, &a, true, 1);
    advance(&mut m, 200_000);
    assert_eq!(m.pulse_output.positions[&0], 10);
    execute(&mut m, &a, true, 1);
    assert_eq!(m.pulse_output.positions[&0], 10);
    MotionBasicState::begin_scan(&mut m);
    execute(&mut m, &a, true, 1);
    advance(&mut m, 300_000);
    assert_eq!(m.pulse_output.positions[&0], 0);
    assert!(m.bit(dev("Y1")));
    no_limits(&m);
}
#[test]
fn velocity_false_stops_without_completion_and_releases_shared_axis() {
    let mut m = memory();
    let a = action("DPLSV", &["K1000", "Y0", "Y1"]);
    execute(&mut m, &a, true, 1);
    advance(&mut m, 10_000);
    execute(&mut m, &a, false, 1);
    advance(&mut m, 20_000);
    assert_eq!(m.pulse_output.positions[&0], 10);
    assert!(m.pulse_output.axis_owner(0).is_none());
    assert!(!m.system_bit(1029));
}
#[test]
fn active_position_operands_are_latched_until_false() {
    let mut m = memory();
    m.set_double_word_raw(dev("D0"), 10);
    m.set_double_word_raw(dev("D2"), 100);
    let a = action("DDRVI", &["D0", "D2", "Y0", "Y1"]);
    execute(&mut m, &a, true, 1);
    m.set_double_word_raw(dev("D0"), 200);
    execute(&mut m, &a, true, 1);
    advance(&mut m, 1_000_000);
    assert_eq!(m.pulse_output.positions[&0], 10);
    no_limits(&m);
}
#[test]
fn two_speed_relative_sections_add_distances_and_clear_mode_only_when_complete() {
    let mut m = memory();
    m.set_system(1119, true);
    for (i, v) in [(0, 10), (2, 20), (10, 100), (12, 200)] {
        m.set_double_word_raw(d(Area::D, i), v);
    }
    let a = action("DDRVI", &["D0", "D10", "Y0", "Y1"]);
    execute(&mut m, &a, true, 1);
    advance(&mut m, 2_000_000);
    assert_eq!(m.pulse_output.positions[&0], 30);
    assert!(!m.system_bit(1119));
    no_limits(&m);
}
#[test]
fn same_axis_first_active_owner_blocks_later_site_without_repainting() {
    let mut m = memory();
    let a = action("PLSV", &["K100", "Y0", "Y1"]);
    let b = action("PLSV", &["K200", "Y0", "Y1"]);
    execute(&mut m, &a, true, 1);
    execute(&mut m, &b, true, 2);
    assert_eq!(m.pulse_output.axis_owner(0), Some(1));
    no_limits(&m);
    advance(&mut m, 100_000);
    assert_eq!(m.pulse_output.positions[&0], 10);
}
#[test]
fn foreign_output_owner_blocks_axis_transaction_before_mutation() {
    let mut m = memory();
    m.timing_io
        .output_owners
        .insert(dev("Y1"), ResourceOwner::External("EXTIO:50".into()));
    let a = action("PLSV", &["K100", "Y0", "Y1"]);
    execute(&mut m, &a, true, 1);
    assert!(m.pulse_output.axis_owner(0).is_none());
    assert!(!m.bit(dev("Y0")));
    assert!(!m.execution_limitations.is_empty());
}
#[test]
fn homing_waits_for_real_dog_edges_and_clears_position_at_final_event() {
    let mut m = memory();
    m.set_double_word_raw(dev("D1030"), 500);
    let a = action("DZRN", &["K1000", "K100", "X4", "Y0"]);
    execute(&mut m, &a, true, 1);
    advance(&mut m, 100_000);
    assert_eq!(m.pulse_output.positions[&0], 400);
    MotionBasicState::input_edge(&mut m, dev("X4"), false, true, 100_000);
    advance(&mut m, 150_000);
    assert_eq!(m.pulse_output.positions[&0], 395);
    assert!(!m.system_bit(1029));
    MotionBasicState::input_edge(&mut m, dev("X4"), true, false, 150_000);
    assert_eq!(m.pulse_output.positions[&0], 0);
    assert!(m.system_bit(1029));
    no_limits(&m);
}
#[test]
fn homing_clear_pulse_defers_completion_twenty_milliseconds() {
    let mut m = memory();
    m.set_system(1346, true);
    let a = action("DZRN", &["K1000", "K100", "X4", "Y0"]);
    execute(&mut m, &a, true, 1);
    MotionBasicState::input_edge(&mut m, dev("X4"), false, true, 0);
    MotionBasicState::input_edge(&mut m, dev("X4"), true, false, 0);
    assert!(m.physical_output(dev("Y4")));
    assert!(!m.system_bit(1029));
    advance(&mut m, 19_999);
    assert!(!m.system_bit(1029));
    advance(&mut m, 20_000);
    assert!(!m.physical_output(dev("Y4")));
    assert!(m.system_bit(1029));
    no_limits(&m);
}
#[test]
fn homing_z_phase_counts_only_rising_edges_after_dog_exit() {
    let mut m = memory();
    m.set_word(dev("D1312"), -2);
    let a = action("DZRN", &["K1000", "K100", "X4", "Y0"]);
    execute(&mut m, &a, true, 1);
    MotionBasicState::input_edge(&mut m, dev("X2"), false, true, 0);
    MotionBasicState::input_edge(&mut m, dev("X4"), false, true, 0);
    MotionBasicState::input_edge(&mut m, dev("X4"), true, false, 0);
    MotionBasicState::input_edge(&mut m, dev("X2"), false, true, 0);
    assert!(!m.system_bit(1029));
    MotionBasicState::input_edge(&mut m, dev("X2"), true, false, 0);
    assert!(!m.system_bit(1029));
    MotionBasicState::input_edge(&mut m, dev("X2"), false, true, 0);
    assert!(m.system_bit(1029));
}
#[test]
fn homing_extra_count_uses_shared_pulse_deadline() {
    let mut m = memory();
    m.set_word(dev("D1312"), -10);
    m.set_system(1308, true);
    let a = action("DZRN", &["K1000", "K100", "X4", "Y0"]);
    execute(&mut m, &a, true, 1);
    MotionBasicState::input_edge(&mut m, dev("X4"), false, true, 0);
    MotionBasicState::input_edge(&mut m, dev("X4"), true, false, 0);
    advance(&mut m, 99_999);
    assert!(!m.system_bit(1029));
    advance(&mut m, 100_000);
    assert!(m.system_bit(1029));
    assert_eq!(m.pulse_output.positions[&0], 0);
}
#[test]
fn servo_success_requires_an_explicit_valid_response_not_elapsed_time_alone() {
    let mut m = memory();
    let a = action("DABSR", &["X0", "Y4", "D0"]);
    execute(&mut m, &a, true, 1);
    advance(&mut m, 5_000_000);
    assert!(!m.system_bit(1029));
    m.motion_basic.set_servo_frame(
        1,
        ServoFrame::Mitsubishi {
            position: -123456,
            check_valid: true,
        },
    );
    advance(&mut m, 5_000_000);
    advance(&mut m, 7_000_000);
    assert_eq!(m.double_word(dev("D0")), -123456);
    assert!(m.system_bit(1029));
    no_limits(&m);
}
#[test]
fn invalid_servo_frame_holds_destination_and_delta_error_is_explicit() {
    let mut m = memory();
    m.set_system(1177, true);
    m.set_word(dev("D0"), 123);
    m.motion_basic.set_servo_frame(
        1,
        ServoFrame::Delta {
            status: 0,
            multiturn: 0,
            puu: 10,
            valid: false,
        },
    );
    let a = action("DABSR", &["X0", "Y4", "D0"]);
    execute(&mut m, &a, true, 1);
    advance(&mut m, 0);
    advance(&mut m, 2_000_000);
    assert_eq!(m.word(dev("D0")), 123);
    assert!(m.system_bit(1581));
    assert!(!m.system_bit(1580));
}
#[test]
fn axis_api_rejects_conflict_and_preserves_fraction_across_frequency_change() {
    let mut m = memory();
    PulseState::submit_axis(
        &mut m,
        AxisRequest {
            owner_pc: 1,
            channel: 0,
            direction: dev("Y1"),
            negative: false,
            invert_direction: false,
            target_pulses: None,
            segments: constant(3),
            completion: false,
            paused: false,
        },
        0,
    )
    .unwrap();
    advance(&mut m, 100_000);
    let mut change = revision();
    change.frequency_millihz = Some(7000);
    PulseState::revise_axis(&mut m, 1, change, 100_000).unwrap();
    advance(&mut m, 200_000);
    assert_eq!(m.pulse_output.positions[&0], 1);
}
#[test]
fn abort_discards_lifecycle_but_preserves_environment_servo_provider() {
    let mut m = memory();
    m.motion_basic.set_servo_frame(
        1,
        ServoFrame::Mitsubishi {
            position: 10,
            check_valid: true,
        },
    );
    let a = action("PLSV", &["K100", "Y0", "Y1"]);
    execute(&mut m, &a, true, 1);
    PulseState::abort(&mut m);
    MotionBasicState::abort(&mut m);
    assert!(m.motion_basic.runs.is_empty());
    assert!(m.motion_basic.servo_frames.contains_key(&1));
}

#[test]
fn dabrs_rejects_index_register_targets_even_when_constructed_directly() {
    for target in ["E0", "F0"] {
        assert!(MotionAction::parse("DABSR", &["X0".into(), "Y4".into(), target.into()]).is_err());
    }
    let a = MotionAction::AbsoluteRead {
        source: dev("X0"),
        control: dev("Y4"),
        value: CalendarValue::Index {
            index: 0,
            wide: true,
            high_bank: false,
        },
    };
    let mut m = memory();
    execute(&mut m, &a, true, 1);
    assert!(!m.execution_limitations.is_empty());
    assert!(!m.bit(dev("Y4")));
    assert!(m.motion_basic.runs.is_empty());
}
#[test]
fn readonly_direction_and_control_bases_are_rejected_by_parser() {
    for (name, args) in [
        ("DABSR", vec!["X0", "M1000", "D0"]),
        ("PLSV", vec!["K100", "Y0", "M1011"]),
        ("DDRVI", vec!["K10", "K100", "Y0", "M1002"]),
    ] {
        assert!(
            MotionAction::parse(
                name,
                &args.into_iter().map(str::to_string).collect::<Vec<_>>()
            )
            .is_err()
        );
    }
}
#[test]
fn whole_control_span_is_validated_before_servo_control_mutation() {
    let mut m = memory();
    let a = action("DABSR", &["X0", "M998", "D0"]);
    execute(&mut m, &a, true, 1);
    assert!(!m.execution_limitations.is_empty());
    assert!(!m.bit(dev("M998")));
    assert!(!m.bit(dev("M999")));
    assert!(m.motion_basic.runs.is_empty());
    // The ASDA profile uses only two controls: M998/M999 are both writable.
    m.execution_limitations.clear();
    m.set_system(1177, true);
    execute(&mut m, &a, true, 1);
    no_limits(&m);
    assert!(m.bit(dev("M998")));
    assert!(m.bit(dev("M999")));
}
#[test]
fn sparse_implicit_control_and_source_addresses_do_not_bypass_device_validation() {
    let mut m = memory();
    let a = action("DABSR", &["X0", "M1072", "D0"]); // M1073 is not in sparse supported system bank.
    execute(&mut m, &a, true, 1);
    assert!(!m.execution_limitations.is_empty());
    assert!(!m.bit(dev("M1072")));
    m.execution_limitations.clear();
    let a = action("DABSR", &["M1072", "Y4", "D0"]);
    execute(&mut m, &a, true, 1);
    assert!(!m.execution_limitations.is_empty());
    assert!(!m.bit(dev("Y4")));
}
#[test]
fn raw_readonly_direction_cannot_be_enabled_by_bypassing_parser() {
    let mut m = memory();
    let a = MotionAction::Velocity {
        wide: false,
        frequency: CalendarValue::parse("K100", false).unwrap(),
        output: dev("Y0"),
        direction: dev("M1000"),
    };
    execute(&mut m, &a, true, 1);
    assert!(!m.execution_limitations.is_empty());
    assert!(m.pulse_output.axis_owner(0).is_none());
}
#[test]
fn asda_four_fields_require_real_word_storage_not_packed_bits_or_natural_c() {
    for target in ["K8M0", "C198", "C200"] {
        let mut m = memory();
        m.set_system(1177, true);
        let a = action("DABSR", &["X0", "Y4", target]);
        execute(&mut m, &a, true, 1);
        assert!(!m.execution_limitations.is_empty(), "{target}");
        assert!(!m.bit(dev("Y4")));
        assert!(m.motion_basic.runs.is_empty());
    }
}
#[test]
fn asda_four_word_extent_is_atomic_at_model_boundary() {
    let mut m = memory();
    m.data_register_limit = Some(4999);
    m.set_system(1177, true);
    m.set_word(dev("D4997"), 77);
    let a = action("DABSR", &["X0", "Y4", "D4997"]);
    execute(&mut m, &a, true, 1);
    assert!(!m.execution_limitations.is_empty());
    assert_eq!(m.word(dev("D4997")), 77);
    assert!(!m.bit(dev("Y4")));
}
#[test]
fn candidate_claim_cannot_be_hidden_by_a_higher_pc_existing_run() {
    let mut m = memory();
    m.set_system(1346, true);
    let home = action("DZRN", &["K1000", "K100", "X4", "Y0"]);
    execute(&mut m, &home, true, 9);
    no_limits(&m);
    let servo = action("DABSR", &["X10", "Y4", "D0"]);
    execute(&mut m, &servo, true, 1);
    assert!(!m.execution_limitations.is_empty());
    assert!(!m.motion_basic.runs.contains_key(&1));
    assert!(m.motion_basic.runs.contains_key(&9));
    assert!(!m.bit(dev("Y4")));
}
#[test]
fn active_mtr_pins_remain_protected_when_timing_owner_snapshot_is_taken() {
    let mut m = memory();
    let matrix = crate::refresh_io::RefreshAction::parse(
        "MTR",
        &["X40".into(), "Y10".into(), "M200".into(), "K2".into()],
    )
    .unwrap();
    m.refresh_io.set_matrix_rows(77, vec![0, 0]).unwrap();
    matrix.execute(&mut m, true, 77);
    m.timing_io.input_owners.clear();
    m.timing_io.output_owners.clear();
    let servo = action("DABSR", &["X0", "Y10", "D0"]);
    execute(&mut m, &servo, true, 1);
    assert!(!m.execution_limitations.is_empty());
    assert!(m.motion_basic.runs.is_empty());
    assert_eq!(m.refresh_io.matrix_owner(), Some(77));
}
#[test]
fn zero_first_two_speed_section_takes_direction_from_negative_second_section() {
    for absolute in [false, true] {
        let mut m = memory();
        m.set_system(1119, true);
        m.set_double_word(dev("D0"), 0);
        m.set_double_word(dev("D2"), -10);
        m.set_double_word(dev("D10"), 100);
        m.set_double_word(dev("D12"), 100);
        let a = action(
            if absolute { "DDRVA" } else { "DDRVI" },
            &["D0", "D10", "Y0", "Y1"],
        );
        execute(&mut m, &a, true, 1);
        assert!(m.bit(dev("Y1")));
        advance(&mut m, 1_000_000);
        assert_eq!(m.pulse_output.axis_position(0, &m), -10);
        assert!(m.system_bit(1029));
        no_limits(&m);
    }
}
#[test]
fn modern_odd_pair_is_not_a_velocity_route_but_is_a_finite_move_route() {
    let mut m = memory();
    let a = action("DPLSV", &["K100", "Y1", "Y5"]);
    execute(&mut m, &a, true, 1);
    assert!(!m.execution_limitations.is_empty());
    assert!(m.pulse_output.axis_owner(1).is_none());
    m.execution_limitations.clear();
    let a = action("DDRVI", &["K10", "K100", "Y1", "Y5"]);
    execute(&mut m, &a, true, 2);
    no_limits(&m);
    assert_eq!(m.pulse_output.axis_owner(1), Some(2));
}
#[test]
fn physical_servo_control_extent_uses_configured_output_point_count() {
    let mut m = memory();
    m.refresh_io.profile.output_points = 6;
    let a = action("DABSR", &["X0", "Y4", "D0"]);
    execute(&mut m, &a, true, 1);
    assert!(!m.execution_limitations.is_empty());
    assert!(!m.bit(dev("Y4")));
    assert!(m.motion_basic.runs.is_empty());
}
