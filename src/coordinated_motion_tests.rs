use super::*;
use crate::canopen::{CanProfile, CanReply, CanopenAction, CanopenNetwork};
fn action(name: &str, args: &[&str]) -> CoordinatedAction {
    CoordinatedAction::parse(
        name,
        &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn advance(m: &mut Memory, at: u64) {
    m.timing_io.now_us = at;
    PulseState::advance(m, at);
    CoordinatedState::advance(m, at);
    CoordinatedState::service_controls(m);
}
fn ready_servo(initial: i32) -> Memory {
    let mut m = Memory::default();
    m.canopen
        .set_profile(CanProfile {
            cpu: CanCpu::Es2C,
            firmware_major: 3,
            firmware_minor: 60,
            servo: CanServoModel::A3M,
        })
        .unwrap();
    m.canopen.configure_node(1, true).unwrap();
    m.set_system(1614, true);
    CanopenAction::parse("INITC", &["K1".into()])
        .unwrap()
        .execute(&mut m, true, 1);
    for _ in 0..128 {
        let Some(r) = m.canopen.pending().cloned() else {
            break;
        };
        let at = m.canopen.now_us + 1;
        m.canopen
            .queue_reply(at, r.id, r.epoch, CanReply::Ack)
            .unwrap();
        CanopenNetwork::advance(&mut m, at);
    }
    m.timing_io.now_us = m.canopen.now_us;
    CanopenAction::parse("ASDON", &["K1".into(), "K1".into()])
        .unwrap()
        .execute(&mut m, true, 4);
    let r = m.canopen.pending().unwrap().clone();
    let at = m.canopen.now_us + 1;
    m.canopen
        .queue_reply(at, r.id, r.epoch, CanReply::Ack)
        .unwrap();
    CanopenNetwork::advance(&mut m, at);
    m.timing_io.now_us = at;
    m.coordinated_motion
        .configure_servo(
            1,
            ServoCalibration::Rotary {
                puu_per_revolution: 600.,
            },
            initial,
        )
        .unwrap();
    m
}
fn reply(m: &mut Memory) {
    let r = m.coordinated_motion.pending().unwrap().clone();
    let at = m.coordinated_motion.now_us + 1;
    m.coordinated_motion
        .queue_reply(at, r, ServoReply::Accepted)
        .unwrap();
    advance(m, at);
}
#[test]
fn genuine_headers_lengths_and_exact_operand_matrices() {
    for (name, args, header, words) in [
        ("CASD", vec!["K1", "K3", "D0"], 0xb202, 7),
        ("DDRVIC", vec!["K1", "D0", "K1000"], 0xb502, 13),
        ("DDRVAC", vec!["K1", "D0", "K1000"], 0xb702, 13),
        ("DPPMR", vec!["D0", "K2", "K1000", "Y0"], 0x9d01, 17),
        ("DPPMA", vec!["K1", "K2", "D0", "Y0"], 0x9f01, 17),
        ("DCIMR", vec!["K1", "K2", "D0", "Y0"], 0xa101, 17),
        ("DCIMA", vec!["K1", "K2", "D0", "Y0"], 0xa301, 17),
    ] {
        let a = action(name, &args);
        assert_eq!(a.header(), Some(header));
        assert_eq!(a.words(), words);
        assert_eq!(a.operands().len(), args.len());
    }
    for (name, args) in [
        ("CASD", vec!["D0", "K1", "K2"]),
        ("DDRVIC", vec!["K1", "K4M0", "K1000"]),
        ("DCIMR", vec!["K1", "K2", "K0", "Y0"]),
        ("DPPMRP", vec!["K1", "K2", "K1000", "Y0"]),
    ] {
        assert!(
            CoordinatedAction::parse(
                name,
                &args.iter().map(|s| s.to_string()).collect::<Vec<_>>()
            )
            .is_err()
        );
    }
}
#[test]
fn quarter_ellipse_has_literal_clockwise_and_counterclockwise_midpoints() {
    let cw = quarter_points([1000, 1000], true);
    let ccw = quarter_points([1000, 1000], false);
    assert_eq!(cw.len(), 20);
    assert_eq!(cw[9], [293, 707]);
    assert_eq!(ccw[9], [707, 293]);
    assert_eq!(cw[19], [1000, 1000]);
    for (delta, clockwise) in [
        ([1000, -1000], true),
        ([-1000, -1000], true),
        ([-1000, 1000], true),
        ([1000, 1000], false),
    ] {
        let p = quarter_points(delta, clockwise);
        assert_eq!(p.last(), Some(&delta));
        for axis in 0..2 {
            assert!(p.windows(2).all(|w| (w[1][axis] - w[0][axis]).signum()
                == delta[axis].signum()
                || w[1][axis] == w[0][axis]));
        }
    }
}
#[test]
fn short_axis_linear_fallback_and_degenerate_path_are_finite() {
    assert_eq!(quarter_points([400, 1000], true)[9], [200, 500]);
    assert_eq!(quarter_points([0, 1000], false)[9], [0, 500]);
    let plans = arc_segments([0, 0], true, 200000, 100000);
    assert_eq!(plans[0].len(), 20);
    assert!(
        plans
            .iter()
            .flatten()
            .all(|s| s.duration_us > 0 && s.frequency_millihz == 0)
    );
}
#[test]
fn shared_pulse_engine_finishes_line_axes_together_with_exact_counts() {
    let mut m = Memory::default();
    let a = action("DPPMR", &["K4", "K3", "K1000", "Y0"]);
    a.execute(&mut m, true, 10);
    advance(&mut m, 3000);
    assert!(!m.system_bit(1029));
    advance(&mut m, 4000);
    assert_eq!(m.pulse_output.axis_position(0, &m), 4);
    assert_eq!(m.pulse_output.axis_position(2, &m), 3);
    assert!(m.system_bit(1029));
    a.execute(&mut m, false, 10);
    assert!(m.pulse_output.axis_owner(0).is_none());
    assert!(m.pulse_output.axis_owner(2).is_none());
}
#[test]
fn twenty_chord_arc_reaches_nonlinear_midpoint_and_exact_endpoint() {
    let mut m = Memory::default();
    m.set_word(d(0), 0);
    m.set_word(d(1), 2);
    let a = action("DCIMR", &["K1000", "K1000", "D0", "Y0"]);
    a.execute(&mut m, true, 10);
    advance(&mut m, 100000);
    assert!((m.pulse_output.axis_position(0, &m) - 293).abs() <= 1);
    assert!((m.pulse_output.axis_position(2, &m) - 707).abs() <= 1);
    advance(&mut m, 200000);
    assert_eq!(m.pulse_output.axis_position(0, &m), 1000);
    assert_eq!(m.pulse_output.axis_position(2, &m), 1000);
    assert!(m.system_bit(1029));
}
#[test]
fn second_axis_conflict_is_preflighted_before_any_first_axis_mutation() {
    let mut m = Memory::default();
    let occupied = AxisRequest {
        owner_pc: 100,
        channel: 2,
        direction: Device {
            area: Area::Y,
            index: 3,
        },
        negative: false,
        invert_direction: false,
        target_pulses: None,
        segments: VecDeque::from([PulseSegment {
            duration_us: 0,
            frequency_millihz: 1000000,
            phase: MotionPhase::Cruise,
        }]),
        completion: false,
        paused: false,
    };
    PulseState::submit_axis(&mut m, occupied, 0).unwrap();
    let prior = m.physical_outputs.clone();
    action("DPPMR", &["K100", "K100", "K1000", "Y0"]).execute(&mut m, true, 10);
    assert_eq!(m.physical_outputs, prior);
    assert!(m.pulse_output.axis_owner(0).is_none());
    assert_eq!(m.pulse_output.axis_owner(2), Some(100));
    assert!(!m.execution_limitations.is_empty());
}
#[test]
fn full_signed_relative_pulses_do_not_overflow_absolute_value() {
    let mut m = Memory::default();
    action("DPPMR", &["K-2147483648", "K0", "K100000", "Y0"]).execute(&mut m, true, 10);
    assert!(
        m.execution_limitations.is_empty(),
        "{:?}",
        m.execution_limitations
    );
    assert!(m.pulse_output.axis_owner(0).is_some());
    advance(&mut m, 1);
    assert!(m.pulse_output.axis_position(0, &m) <= 0);
}
#[test]
fn casd_clamps_outside_to_one_and_waits_correlated_ack() {
    let mut m = ready_servo(0);
    action("CASD", &["K1", "K0", "K-1"]).execute(&mut m, true, 10);
    assert!(!m.system_bit(1615));
    assert_eq!(
        m.coordinated_motion.pending().unwrap().intent,
        ServoIntent::Ramp {
            acceleration_ms: 1,
            deceleration_ms: 1
        }
    );
    reply(&mut m);
    assert!(m.system_bit(1615));
    assert_eq!(m.coordinated_motion.plants[&1].acceleration_ms, Some(1));
}
#[test]
fn calibrated_servo_has_real_acceleration_and_exact_target_after_ack() {
    let mut m = ready_servo(0);
    action("CASD", &["K1", "K3000", "K9000"]).execute(&mut m, true, 10);
    reply(&mut m);
    let a = action("DDRVIC", &["K1", "K100000", "K10000"]);
    a.execute(&mut m, true, 20);
    let before = m.coordinated_motion.now_us;
    advance(&mut m, before + 100000);
    assert_eq!(m.coordinated_motion.servo_position(1), Some(0));
    assert!(!m.system_bit(1624));
    reply(&mut m);
    let start = m.coordinated_motion.now_us;
    advance(&mut m, start + 500000);
    assert_eq!(m.coordinated_motion.servo_position(1), Some(1250));
    advance(&mut m, start + 12000000);
    assert_eq!(m.coordinated_motion.servo_position(1), Some(100000));
    assert!(m.system_bit(1624));
}
#[test]
fn deceleration_stop_does_not_claim_original_target_reached() {
    let mut m = ready_servo(0);
    action("CASD", &["K1", "K3000", "K9000"]).execute(&mut m, true, 10);
    reply(&mut m);
    action("DDRVIC", &["K1", "K100000", "K10000"]).execute(&mut m, true, 20);
    reply(&mut m);
    let start = m.coordinated_motion.now_us;
    advance(&mut m, start + 500000);
    m.set_system(1632, true);
    CoordinatedState::service_controls(&mut m);
    advance(&mut m, start + 2000000);
    assert_eq!(m.coordinated_motion.servo_position(1), Some(5000));
    assert!(!m.system_bit(1624));
}
#[test]
fn absolute_reciprocation_returns_to_zero_not_initial_position() {
    let mut m = ready_servo(20000);
    action("CASD", &["K1", "K1", "K1"]).execute(&mut m, true, 10);
    reply(&mut m);
    m.set_system(1648, true);
    action("DDRVAC", &["K1", "K100000", "K10000"]).execute(&mut m, true, 20);
    reply(&mut m);
    let first = m.coordinated_motion.servo_runs[&20].path.deadline();
    advance(&mut m, first);
    assert_eq!(m.coordinated_motion.servo_position(1), Some(100000));
    assert!(!m.system_bit(1656));
    let second = m.coordinated_motion.servo_runs[&20].path.deadline();
    advance(&mut m, second);
    assert_eq!(m.coordinated_motion.servo_position(1), Some(0));
    assert!(m.system_bit(1656));
}
#[test]
fn cpu_epoch_rejects_stale_casd_ack_without_claiming_completion() {
    let mut m = ready_servo(0);
    let a = action("CASD", &["K1", "K3000", "K9000"]);
    a.execute(&mut m, true, 10);
    let old = m.coordinated_motion.pending().unwrap().clone();
    CoordinatedState::abort(&mut m);
    m.set_system(1615, true);
    a.execute(&mut m, true, 10);
    let at = m.coordinated_motion.now_us + 1;
    m.coordinated_motion
        .queue_reply(at, old, ServoReply::Accepted)
        .unwrap();
    advance(&mut m, at);
    assert!(!m.system_bit(1615));
    assert!(m.coordinated_motion.pending().is_some());
}
#[test]
fn servo_next_genuine_metadata_and_strict_forms() {
    for (name, args, header, words) in [
        ("PLSVC", vec!["K1", "K600"], 0xb802, 5),
        ("DPLSVC", vec!["K1", "K60000"], 0xb902, 9),
        ("ZRNC", vec!["K1", "K3000", "K100"], 0xba02, 7),
        ("DZRNC", vec!["K1", "K60000", "K100"], 0xbb02, 13),
        ("ZRNM", vec!["D0", "HFFFF", "M30", "M31"], 0xc402, 9),
        ("EMER", vec!["D0", "D100", "D200", "M30", "M31"], 0xc202, 11),
    ] {
        let a = action(name, &args);
        assert_eq!(a.header(), Some(header));
        assert_eq!(a.words(), words);
        assert_eq!(a.operands().len(), args.len());
    }
    for (name, args) in [
        ("PLSVC", vec!["D0", "K600"]),
        ("ZRNM", vec!["K1", "K0", "Y0", "M0"]),
        ("EMER", vec!["K1", "C0", "D0", "M0", "M1"]),
    ] {
        assert!(
            CoordinatedAction::parse(
                name,
                &args.iter().map(|s| s.to_string()).collect::<Vec<_>>()
            )
            .is_err()
        );
    }
}
#[test]
fn servo_speed_false_before_ack_does_not_start_local_motion() {
    let mut m = ready_servo(40);
    action("CASD", &["K1", "K1000", "K1000"]).execute(&mut m, true, 10);
    reply(&mut m);
    let speed = action("DPLSVC", &["K1", "K600"]);
    speed.execute(&mut m, true, 20);
    assert!(m.coordinated_motion.pending().is_some());
    speed.execute(&mut m, false, 20);
    reply(&mut m);
    let at = m.coordinated_motion.now_us;
    advance(&mut m, at + 1_000_000);
    assert_eq!(m.coordinated_motion.servo_position(1), Some(40));
    assert!(!m.system_bit(1624));
}
#[test]
fn servo_current_origin_requires_real_ack_and_applies_definition() {
    let mut m = ready_servo(99);
    action("CASD", &["K1", "K1000", "K1000"]).execute(&mut m, true, 10);
    reply(&mut m);
    m.coordinated_motion
        .configure_homing(1, 0x0028, 0, 123)
        .unwrap();
    action("DZRNC", &["K1", "K600", "K100"]).execute(&mut m, true, 20);
    assert_eq!(m.coordinated_motion.servo_position(1), Some(99));
    assert!(!m.system_bit(1624));
    reply(&mut m);
    assert_eq!(m.coordinated_motion.servo_position(1), Some(123));
    assert!(m.system_bit(1624));
}
#[test]
fn emergency_destination_full_capacity_failure_preserves_packet() {
    let mut m = ready_servo(0);
    // Keep the CAN diagnostic registers D6000..D6002 available while the
    // requested twenty-word receive buffer crosses the configured boundary.
    m.data_register_limit = Some(9999);
    action("EMER", &["K1", "D9990", "D200", "M30", "M31"]).execute(&mut m, true, 20);
    assert!(!m.system_bit(30));
    assert!(!m.execution_limitations.is_empty());
    let at = m.coordinated_motion.now_us + 1;
    m.coordinated_motion
        .queue_emergency(at, 1, [1, 2, 3, 4, 5, 6, 7, 8])
        .unwrap();
    advance(&mut m, at);
    action("EMER", &["K1", "D100", "D200", "M30", "M31"]).execute(&mut m, true, 21);
    assert_eq!(m.word(d(100)), 0x0201);
    assert_eq!(m.word(d(200)), 1);
    assert!(m.bit(relay(30)));
}
#[test]
fn home_mode_literal_invalid_nibbles_are_peer_owned_not_preclamped() {
    let mut m = ready_servo(0);
    let a = action("ZRNM", &["K1", "HFFFF", "M30", "M31"]);
    a.execute(&mut m, true, 20);
    assert!(matches!(
        m.coordinated_motion.pending().unwrap().intent,
        ServoIntent::HomingMode { mode: 0xffff }
    ));
    a.execute(&mut m, false, 20);
    reply(&mut m);
    assert!(!m.bit(relay(30)));
}
