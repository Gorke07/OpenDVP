//! Independent PDF777..782/788..792 calibrated CAN-servo acceptance.
//! Peer ACKs and sensor events are explicit inputs, not simulated native success.
use crate::canopen::{CanCpu, CanProfile, CanReply, CanServoModel};
use crate::coordinated_motion::{HomeEvent, ServoCalibration, ServoIntent, ServoReply};
use crate::plc::{Device, Program, ScanStatus, Simulator, compile_il};
fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn sim() -> Simulator {
    let mut s = Simulator::default();
    s.memory
        .canopen
        .set_profile(CanProfile {
            cpu: CanCpu::Es2C,
            firmware_major: 3,
            firmware_minor: 60,
            servo: CanServoModel::A3M,
        })
        .unwrap();
    s.memory.canopen.configure_node(1, true).unwrap();
    s.memory
        .coordinated_motion
        .configure_servo(
            1,
            ServoCalibration::Rotary {
                puu_per_revolution: 60.0,
            },
            0,
        )
        .unwrap();
    s
}
fn scan(s: &mut Simulator, p: &Program) {
    s.scan(p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
}
fn advance(s: &mut Simulator, us: u64) {
    crate::timing_io::advance_by(&mut s.memory, us);
}
fn servo_ack(s: &mut Simulator, reply: ServoReply) {
    let r = s.memory.coordinated_motion.pending().unwrap().clone();
    let at = s.memory.timing_io.now_us + 1;
    s.memory
        .coordinated_motion
        .queue_reply(at, r, reply)
        .unwrap();
    advance(s, 1);
}
const SETUP: &str =
    "LD X20\nSET M1614\nINITC K1\nLD X21\nASDON K1 K1\nLD X22\nCASD K1 K1000 K1000\n";
fn prepared(s: &mut Simulator, p: &Program) {
    s.memory.set_input(d("X20"), true);
    scan(s, p);
    for _ in 0..512 {
        let Some(r) = s.memory.canopen.pending().cloned() else {
            break;
        };
        let at = s.memory.timing_io.now_us + 1;
        s.memory
            .canopen
            .queue_reply(at, r.id, r.epoch, CanReply::Ack)
            .unwrap();
        advance(s, 1);
    }
    assert!(s.memory.system_bit(1615));
    assert!(s.memory.canopen.pending().is_none());
    s.memory.set_input(d("X20"), false);
    s.memory.set_input(d("X21"), true);
    scan(s, p);
    let r = s.memory.canopen.pending().unwrap().clone();
    let at = s.memory.timing_io.now_us + 1;
    s.memory
        .canopen
        .queue_reply(at, r.id, r.epoch, CanReply::Ack)
        .unwrap();
    advance(s, 1);
    assert!(s.memory.system_bit(1640));
    s.memory.set_input(d("X21"), false);
    s.memory.set_input(d("X22"), true);
    scan(s, p);
    servo_ack(s, ServoReply::Accepted);
    s.memory.set_input(d("X22"), false);
    assert!(
        s.memory.execution_limitations.is_empty(),
        "{:?}",
        s.memory.execution_limitations
    );
}
fn position(s: &Simulator) -> i32 {
    s.memory.double_word(d("D6032"))
}
#[test]
fn all_six_spellings_have_genuine_headers_and_widths_but_guarded_native_upload() {
    for (body, head, width) in [
        ("PLSVC K1 K600", 0xb802, 5),
        ("DPLSVC K1 K600", 0xb902, 9),
        ("ZRNC K1 K3000 K100", 0xba02, 7),
        ("DZRNC K1 K3000 K100", 0xbb02, 13),
        ("EMER K1 D100 D200 M10 M11", 0xc202, 11),
        ("ZRNM K1 H23 M10 M11", 0xc402, 9),
    ] {
        let p = compile_il(&format!("LD X0\n{body}\nMOV K7 D400\nEND")).unwrap();
        let l = crate::es2_encoder::local_execution_layout(&p).unwrap();
        assert_eq!(l.known_word_at(1), Some(head));
        assert_eq!(l.action_pc, vec![1, 1 + width]);
        assert_eq!(l.word_count, 1 + width + 6);
        assert!(l.unencoded_ranges.is_empty());
        assert!(crate::es2_encoder::encode(&p).is_err());
    }
}
#[test]
fn matrices_reject_p_forms_dynamic_motion_node_and_non_m_completion_flags() {
    for body in [
        "PLSVCP K1 K600",
        "DPLSVCP K1 K600",
        "DZRNC K1 K8M0 K10",
        "ZRNCP K1 K100 K10",
        "PLSVC D0 K600",
        "ZRNC K1 X0 K10",
        "ZRNMP K1 H23 M10 M11",
        "ZRNM K1 H23 Y0 M11",
        "EMER K1 D100 K1 M10 M11",
    ] {
        assert!(
            compile_il(&format!("LD X0\n{body}\nEND")).is_err(),
            "{body}"
        );
    }
}
#[test]
fn speed_cannot_move_without_real_init_servo_on_and_ramp_ack() {
    let p = compile_il("LD X0\nDPLSVC K1 K600\nEND").unwrap();
    let mut s = sim();
    s.memory.set_input(d("X0"), true);
    scan(&mut s, &p);
    advance(&mut s, 1_000_000);
    assert_eq!(position(&s), 0);
    assert!(!s.memory.execution_limitations.is_empty());
    assert!(!s.memory.system_bit(1624));
}
#[test]
fn speed_real_ack_calibration_and_false_deceleration_have_literal_positions() {
    let p = compile_il(&format!("{SETUP}LD X23\nDPLSVC K1 K600\nEND")).unwrap();
    let mut s = sim();
    prepared(&mut s, &p);
    s.memory.set_input(d("X23"), true);
    scan(&mut s, &p);
    advance(&mut s, 100000);
    assert_eq!(position(&s), 0, "pending command ACK cannot move plant");
    servo_ack(&mut s, ServoReply::Accepted);
    advance(&mut s, 510000);
    assert_eq!(position(&s), 30);
    assert!(
        !s.memory.system_bit(1624),
        "continuous speed has no reached-target completion"
    );
    s.memory.set_input(d("X23"), false);
    scan(&mut s, &p);
    advance(&mut s, 20000);
    assert_eq!(position(&s), 31);
    advance(&mut s, 1000000);
    assert_eq!(position(&s), 31);
    assert!(!s.memory.system_bit(1624));
}
#[test]
fn per_node_stop_flag_decelerates_speed_and_does_not_fabricate_target_done() {
    let p = compile_il(&format!("{SETUP}LD X23\nDPLSVC K1 K-600\nEND")).unwrap();
    let mut s = sim();
    prepared(&mut s, &p);
    s.memory.set_input(d("X23"), true);
    scan(&mut s, &p);
    servo_ack(&mut s, ServoReply::Accepted);
    advance(&mut s, 510000);
    assert_eq!(position(&s), -30);
    s.memory.set_bit(d("M1632"), true);
    scan(&mut s, &p);
    advance(&mut s, 20000);
    assert_eq!(position(&s), -31);
    advance(&mut s, 1000000);
    assert_eq!(position(&s), -31);
    assert!(!s.memory.system_bit(1624));
}
#[test]
fn speed_rearm_waits_for_busy_can_owner_and_requires_its_own_ack() {
    for mnemonic in ["PLSVC", "DPLSVC"] {
        let p = compile_il(&format!(
            "{SETUP}LD X24\nZRNM K1 H23 M30 M31\nLD X23\n{mnemonic} K1 K600\nEND"
        ))
        .unwrap();
        let mut s = sim();
        prepared(&mut s, &p);
        s.memory.set_input(d("X23"), true);
        scan(&mut s, &p);
        servo_ack(&mut s, ServoReply::Accepted);
        advance(&mut s, 510000);
        s.memory.set_input(d("X23"), false);
        scan(&mut s, &p);
        advance(&mut s, 20000);
        let stopped = position(&s);

        // A different real request owns the CAN exchange while speed rearms.
        s.memory.set_input(d("X24"), true);
        scan(&mut s, &p);
        assert!(matches!(
            s.memory.coordinated_motion.pending().unwrap().intent,
            ServoIntent::HomingMode { .. }
        ));
        s.memory.set_input(d("X23"), true);
        scan(&mut s, &p);
        servo_ack(&mut s, ServoReply::Accepted);
        advance(&mut s, 100000);
        assert_eq!(position(&s), stopped);

        scan(&mut s, &p);
        assert!(
            s.memory
                .coordinated_motion
                .pending()
                .is_some_and(|r| matches!(r.intent, ServoIntent::Speed { speed: 600 })),
            "{mnemonic} must retry rearm after the other owner releases CAN"
        );
        advance(&mut s, 100000);
        assert_eq!(position(&s), stopped, "mode ACK is not speed ACK");
        servo_ack(&mut s, ServoReply::Accepted);
        advance(&mut s, 510000);
        assert!(position(&s) > stopped);
        assert!(!s.memory.system_bit(1624));
    }
}
#[test]
fn homing_mode_is_sent_unclamped_and_false_suppresses_late_ok() {
    let p = compile_il(&format!("{SETUP}LD X23\nZRNM K1 HFFFF M30 M31\nEND")).unwrap();
    let mut s = sim();
    prepared(&mut s, &p);
    s.memory.set_input(d("X23"), true);
    scan(&mut s, &p);
    let r = s.memory.coordinated_motion.pending().unwrap().clone();
    assert!(matches!(r.intent, ServoIntent::HomingMode { mode: 0xffff }));
    assert!(!s.memory.bit(d("M30")));
    s.memory.set_input(d("X23"), false);
    scan(&mut s, &p);
    let at = s.memory.timing_io.now_us + 1;
    s.memory
        .coordinated_motion
        .queue_reply(at, r, ServoReply::Fault { code: 0x0609 })
        .unwrap();
    advance(&mut s, 1);
    assert!(!s.memory.bit(d("M30")));
    assert!(s.memory.system_bit(1616));
    assert_eq!(s.memory.word(d("D6000")), 1);
    assert_eq!(s.memory.word(d("D6001")), 0x0609);
}
#[test]
fn emergency_uses_real_messages_count_and_four_little_endian_words_each() {
    let p = compile_il(&format!("{SETUP}LD X23\nEMER K1 D100 D200 M30 M31\nEND")).unwrap();
    let mut s = sim();
    prepared(&mut s, &p);
    s.memory.set_input(d("X23"), true);
    scan(&mut s, &p);
    assert!(!s.memory.bit(d("M30")));
    assert_eq!(s.memory.word(d("D200")), 0);
    let at = s.memory.timing_io.now_us + 1;
    s.memory
        .coordinated_motion
        .queue_emergency(at, 1, [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88])
        .unwrap();
    s.memory
        .coordinated_motion
        .queue_emergency(at + 1, 1, [1, 2, 3, 4, 5, 6, 7, 8])
        .unwrap();
    advance(&mut s, 2);
    scan(&mut s, &p);
    assert!(s.memory.bit(d("M30")));
    assert!(!s.memory.bit(d("M31")));
    assert_eq!(s.memory.word(d("D200")), 2);
    let expected = [
        0x2211, 0x4433, 0x6655, 0x8877, 0x0201, 0x0403, 0x0605, 0x0807,
    ];
    for (i, v) in expected.into_iter().enumerate() {
        assert_eq!(s.memory.word(d(&format!("D{}", 100 + i))) as u16, v);
    }
}
#[test]
fn homing_never_finishes_from_elapsed_time_without_origin_or_z_event() {
    let p = compile_il(&format!("{SETUP}LD X23\nDZRNC K1 K600 K100\nEND")).unwrap();
    let mut s = sim();
    prepared(&mut s, &p);
    s.memory
        .coordinated_motion
        .configure_homing(1, 0x0023, 0, 0)
        .unwrap();
    s.memory.set_input(d("X23"), true);
    scan(&mut s, &p);
    servo_ack(&mut s, ServoReply::Accepted);
    advance(&mut s, 1_000_000);
    assert!(!s.memory.system_bit(1624));
    assert!(position(&s) < 0);
    // A real reverse-ORGP event initiates the next phase; it is not a completion ACK.
    let at = s.memory.timing_io.now_us + 1;
    s.memory
        .coordinated_motion
        .queue_home_event(at, 1, HomeEvent::Origin(true))
        .unwrap();
    advance(&mut s, 1);
    assert!(!s.memory.system_bit(1624));
}

#[test]
fn emergency_ds301_listener_does_not_require_delta_init_or_servo_on() {
    let p = compile_il("LD X23\nEMER K1 D100 D200 M30 M31\nEND").unwrap();
    let mut s = sim();
    assert!(!s.memory.system_bit(1614));
    assert!(!s.memory.system_bit(1615));
    assert!(!s.memory.system_bit(1640));
    s.memory.set_input(d("X23"), true);
    scan(&mut s, &p);
    assert!(!s.memory.bit(d("M30")));
    let at = s.memory.timing_io.now_us + 1;
    s.memory
        .coordinated_motion
        .queue_emergency(at, 1, [0x21, 0x43, 0x65, 0x87, 0xa9, 0xcb, 0xed, 0x0f])
        .unwrap();
    advance(&mut s, 1);
    scan(&mut s, &p);
    assert!(s.memory.bit(d("M30")));
    assert!(!s.memory.bit(d("M31")));
    assert_eq!(s.memory.word(d("D200")), 1);
    assert_eq!(s.memory.word(d("D100")) as u16, 0x4321);
    assert_eq!(s.memory.word(d("D103")) as u16, 0x0fed);
    assert!(
        s.memory.execution_limitations.is_empty(),
        "{:?}",
        s.memory.execution_limitations
    );
}

#[test]
fn homing_current_point_mode_uses_explicit_origin_definition_only_after_ack() {
    let p = compile_il(&format!("{SETUP}LD X23\nDZRNC K1 K600 K100\nEND")).unwrap();
    let mut s = sim();
    prepared(&mut s, &p);
    s.memory
        .coordinated_motion
        .configure_homing(1, 0x0028, 0, 123)
        .unwrap();
    s.memory.set_input(d("X23"), true);
    scan(&mut s, &p);
    assert_eq!(position(&s), 0);
    assert!(!s.memory.system_bit(1624));
    servo_ack(&mut s, ServoReply::Accepted);
    assert_eq!(position(&s), 123);
    assert!(s.memory.system_bit(1624));
    advance(&mut s, 1000000);
    assert_eq!(position(&s), 123);
    assert!(
        s.memory.execution_limitations.is_empty(),
        "{:?}",
        s.memory.execution_limitations
    );
}

#[test]
fn cancelled_homing_rearms_only_after_false_and_full_stop() {
    let p = compile_il(&format!("{SETUP}LD X23\nDZRNC K1 K600 K100\nEND")).unwrap();
    let mut s = sim();
    prepared(&mut s, &p);
    s.memory
        .coordinated_motion
        .configure_homing(1, 0x0023, 0, 0)
        .unwrap();
    s.memory.set_input(d("X23"), true);
    scan(&mut s, &p);
    servo_ack(&mut s, ServoReply::Accepted);
    advance(&mut s, 510000);
    s.memory.set_input(d("X23"), false);
    scan(&mut s, &p);
    advance(&mut s, 20000);
    let stopped = position(&s);
    advance(&mut s, 500000);
    assert_eq!(position(&s), stopped);
    assert!(!s.memory.system_bit(1624));
    s.memory.set_input(d("X23"), true);
    scan(&mut s, &p);
    assert!(s.memory.coordinated_motion.pending().is_some());
    servo_ack(&mut s, ServoReply::Accepted);
    advance(&mut s, 510000);
    assert!(position(&s) < stopped);
    assert!(!s.memory.system_bit(1624));
}
