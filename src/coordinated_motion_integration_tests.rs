//! Literal coordinated geometry and acknowledged calibrated virtual-servo tests.
//! Installed DCIM linear fallback is deliberately not a circular-motion oracle.
use crate::canopen::{CanCpu, CanProfile, CanReply, CanServoModel};
use crate::coordinated_motion::{ServoCalibration, ServoIntent, ServoReply};
use crate::plc::{Device, Program, ScanStatus, Simulator, compile_il};
fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn sim() -> Simulator {
    let mut s = Simulator::default();
    s.memory.set_input(d("X0"), true);
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
fn advance(s: &mut Simulator, us: u64) {
    crate::timing_io::advance_by(&mut s.memory, us);
}
fn xy(s: &Simulator) -> [i32; 2] {
    [
        s.memory.pulse_output.axis_position(0, &s.memory),
        s.memory.pulse_output.axis_position(2, &s.memory),
    ]
}
const SERVO: &str = "LD X20\nSET M1614\nINITC K1\nLD M1615\nAND X21\nASDON K1 K1\nLD X22\nCASD K1 K1000 K1000\nLD X23\nDDRVIC K1 K60 K600\nLD X25\nDDRVAC K1 K10 K600\nLD X24\nDELAY K1000\nDELAY K1000\nDELAY K1\nEND";
fn servo_sim() -> Simulator {
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
    s.memory.set_input(d("X20"), true);
    s
}
fn can_ack(s: &mut Simulator) {
    let r = s.memory.canopen.pending().unwrap().clone();
    let at = s.memory.timing_io.now_us + 1;
    s.memory
        .canopen
        .queue_reply(at, r.id, r.epoch, CanReply::Ack)
        .unwrap();
    advance(s, 1);
}
fn servo_ack(s: &mut Simulator) {
    let r = s.memory.coordinated_motion.pending().unwrap().clone();
    let at = s.memory.timing_io.now_us + 1;
    s.memory
        .coordinated_motion
        .queue_reply(at, r, ServoReply::Accepted)
        .unwrap();
    advance(s, 1);
}
fn initialized(s: &mut Simulator, p: &Program) {
    scan(s, p);
    for _ in 0..512 {
        if s.memory.canopen.pending().is_none() {
            break;
        }
        can_ack(s);
    }
    assert!(s.memory.system_bit(1615));
    assert!(s.memory.canopen.pending().is_none());
    s.memory.set_input(d("X20"), false);
    s.memory.set_input(d("X21"), true);
    scan(s, p);
    assert!(!s.memory.system_bit(1640));
    can_ack(s);
    assert!(s.memory.system_bit(1640));
    s.memory.set_input(d("X21"), false);
}
fn calibrated(s: &mut Simulator, p: &Program) {
    initialized(s, p);
    s.memory.set_input(d("X22"), true);
    scan(s, p);
    assert!(!s.memory.system_bit(1615));
    assert!(matches!(
        s.memory.coordinated_motion.pending().unwrap().intent,
        ServoIntent::Ramp {
            acceleration_ms: 1000,
            deceleration_ms: 1000
        }
    ));
    servo_ack(s);
    assert!(s.memory.system_bit(1615));
    s.memory.set_input(d("X22"), false);
}
#[test]
fn genuine_headers_nominal_word_lengths_and_later_pcs_are_not_native_semantics() {
    for (line, head, words) in [
        ("DPPMR K4 K3 K1000 Y0", 0x9d01, 17),
        ("DPPMA K4 K3 K1000 Y0", 0x9f01, 17),
        ("DCIMR K1000 K1000 D10 Y0", 0xa101, 17),
        ("DCIMA K1000 K1000 D10 Y0", 0xa301, 17),
        ("CASD K1 K1000 K1000", 0xb202, 7),
        ("DDRVIC K1 K60 K600", 0xb502, 13),
        ("DDRVAC K1 K60 K600", 0xb702, 13),
    ] {
        let q = compile_il(&format!("LD X0\n{line}\nMOV K7 D400\nEND")).unwrap();
        let l = crate::es2_encoder::local_execution_layout(&q).unwrap();
        assert_eq!(l.known_word_at(1), Some(head));
        assert_eq!(l.action_pc, vec![1, 1 + words]);
        assert!(crate::es2_encoder::encode(&q).is_err(), "{line}");
    }
}
#[test]
fn strict_documented_matrices_reject_unlisted_p_forms_and_arc_constant_control() {
    for line in [
        "DPPMRP K4 K3 K1000 Y0",
        "DCIMRP K1000 K1000 D10 Y0",
        "CASDP K1 K1 K1",
        "DDRVICP K1 K60 K600",
        "DDRVACP K1 K60 K600",
        "CASD D0 K1 K1",
        "DDRVIC D0 K1 K600",
        "DCIMR K1000 K1000 K0 Y0",
        "DPPMR K4 K3 K1000 M0",
        "DPPMR K4M0 K3 K1000 Y0",
    ] {
        assert!(
            compile_il(&format!("LD X0\n{line}\nEND")).is_err(),
            "{line}"
        );
    }
}
#[test]
fn short_two_axis_moves_use_one_actual_four_ms_schedule_and_absolute_difference() {
    for (absolute, start, end) in [(false, [0, 0], [4, 3]), (true, [2, -1], [6, 2])] {
        let op = if absolute { "DPPMA" } else { "DPPMR" };
        let q = compile_il(&format!(
            "LD X0\n{op} K{} K{} K1000 Y0\nEND",
            end[0], end[1]
        ))
        .unwrap();
        let mut s = sim();
        s.memory.set_double_word(d("D1030"), start[0]);
        s.memory.set_double_word(d("D1336"), start[1]);
        scan(&mut s, &q);
        advance(&mut s, 3999);
        assert_eq!(xy(&s), [start[0] + 3, start[1] + 2]);
        assert!(!s.memory.system_bit(1029));
        advance(&mut s, 1);
        assert_eq!(xy(&s), end);
        assert!(s.memory.system_bit(1029));
    }
}
#[test]
fn twenty_segment_quarter_arcs_have_opposite_literal_midpoints_not_linear_fallback() {
    for op in ["DCIMR", "DCIMA"] {
        for (direction, mid) in [(0, [293, 707]), (1, [707, 293])] {
            let q = compile_il(&format!("LD X0\n{op} K1000 K1000 D10 Y0\nEND")).unwrap();
            let mut s = sim();
            s.memory.set_word(d("D10"), direction);
            s.memory.set_word(d("D11"), 2);
            scan(&mut s, &q);
            advance(&mut s, 100000);
            let actual = xy(&s);
            assert!(
                (actual[0] - mid[0]).abs() <= 2,
                "{op} direction{direction} midpoint{actual:?}"
            );
            assert!((actual[1] - mid[1]).abs() <= 2);
            assert!(!s.memory.system_bit(1029));
            advance(&mut s, 100000);
            assert_eq!(xy(&s), [1000, 1000]);
            assert!(s.memory.system_bit(1029));
        }
    }
}
#[test]
fn four_quarters_return_to_start_in_both_directions_for_relative_and_absolute_arcs() {
    for op in ["DCIMR", "DCIMA"] {
        for direction in [0, 1] {
            let q = compile_il(&format!("LD X0\n{op} D0 D2 D10 Y0\nEND")).unwrap();
            let mut s = sim();
            s.memory.set_word(d("D10"), direction);
            s.memory.set_word(d("D11"), 2);
            let endpoints = if direction == 0 {
                [[1000, 1000], [2000, 0], [1000, -1000], [0, 0]]
            } else {
                [[1000, -1000], [2000, 0], [1000, 1000], [0, 0]]
            };
            let mut previous = [0, 0];
            for end in endpoints {
                let values = if op == "DCIMA" {
                    end
                } else {
                    [end[0] - previous[0], end[1] - previous[1]]
                };
                s.memory.set_double_word(d("D0"), values[0]);
                s.memory.set_double_word(d("D2"), values[1]);
                s.memory.set_system(1029, false);
                s.memory.set_input(d("X0"), true);
                scan(&mut s, &q);
                advance(&mut s, 200000);
                assert_eq!(xy(&s), end);
                assert!(s.memory.system_bit(1029));
                s.memory.set_input(d("X0"), false);
                scan(&mut s, &q);
                previous = end;
            }
            assert_eq!(xy(&s), [0, 0]);
        }
    }
}
#[test]
fn second_axis_conflict_cannot_partially_claim_or_move_the_free_first_axis() {
    let q = compile_il("LD X1\nDPLSY K1000 K0 Y2\nLD X0\nDPPMR K4 K3 K1000 Y0\nEND").unwrap();
    let mut s = sim();
    s.memory.set_input(d("X0"), false);
    s.memory.set_input(d("X1"), true);
    scan(&mut s, &q);
    let owner = s.memory.pulse_output.axis_owner(2);
    assert!(owner.is_some());
    s.memory.set_input(d("X0"), true);
    s.scan(&q, 0);
    assert!(s.memory.pulse_output.axis_owner(0).is_none());
    assert_eq!(s.memory.pulse_output.axis_owner(2), owner);
    assert!(!s.memory.execution_limitations.is_empty());
    advance(&mut s, 10000);
    assert_eq!(xy(&s), [0, 10]);
    assert!(!s.memory.system_bit(1029));
}
#[test]
fn actual_init_servo_and_ramp_acks_precede_calibrated_sixty_puu_trajectory() {
    let q = compile_il(SERVO).unwrap();
    let mut s = servo_sim();
    calibrated(&mut s, &q);
    s.memory.set_input(d("X23"), true);
    scan(&mut s, &q);
    assert!(matches!(
        s.memory.coordinated_motion.pending().unwrap().intent,
        ServoIntent::Move {
            target: 60,
            speed: 600,
            absolute: false
        }
    ));
    advance(&mut s, 1000);
    assert_eq!(s.memory.coordinated_motion.servo_position(1), Some(0));
    assert!(!s.memory.system_bit(1624));
    servo_ack(&mut s);
    // 60PUU/rev,60rpm =>60PUU/s;1000ms at3000rpm =>20ms ramps.
    // Total(60/60)+0.020=1.020s; symmetric midpoint30PUU at0.510s.
    advance(&mut s, 510000);
    assert_eq!(s.memory.double_word(d("D6032")), 30);
    assert!(!s.memory.system_bit(1624));
    advance(&mut s, 510000);
    assert_eq!(s.memory.double_word(d("D6032")), 60);
    assert!(s.memory.system_bit(1624));
    s.memory.set_input(d("X23"), false);
    s.memory.set_input(d("X25"), true);
    scan(&mut s, &q);
    servo_ack(&mut s);
    advance(&mut s, 2000000);
    assert_eq!(s.memory.double_word(d("D6032")), 10);
    assert!(s.memory.system_bit(1624));
}
#[test]
fn servo_stop_flag_decelerates_without_fabricating_target_completion() {
    let source = SERVO.replace("DDRVIC K1 K60 K600", "DDRVIC K1 K600 K600");
    let q = compile_il(&source).unwrap();
    let mut s = servo_sim();
    calibrated(&mut s, &q);
    s.memory.set_input(d("X23"), true);
    scan(&mut s, &q);
    servo_ack(&mut s);
    advance(&mut s, 500000);
    s.memory.set_system(1632, true);
    scan(&mut s, &q);
    advance(&mut s, 21000);
    assert_eq!(s.memory.double_word(d("D6032")), 30);
    assert!(!s.memory.system_bit(1624));
    advance(&mut s, 1000000);
    assert_eq!(s.memory.double_word(d("D6032")), 30);
}
#[test]
fn cpu_restart_invalidates_old_servo_ack_and_releases_can_exchange_owner() {
    let q = compile_il(SERVO).unwrap();
    let mut s = servo_sim();
    initialized(&mut s, &q);
    s.memory.set_input(d("X22"), true);
    scan(&mut s, &q);
    let old = s.memory.coordinated_motion.pending().unwrap().clone();
    assert!(s.memory.canopen.exchange_owner().is_some());
    s.restart_preserving_file_store();
    assert!(s.memory.coordinated_motion.pending().is_none());
    assert!(s.memory.canopen.exchange_owner().is_none());
    s.memory
        .coordinated_motion
        .queue_reply(1, old, ServoReply::Accepted)
        .unwrap();
    advance(&mut s, 1);
    assert!(!s.memory.system_bit(1615));
    assert!(!s.memory.system_bit(1624));
}
#[test]
fn vm_watchdog_abort_stops_both_pulse_axes_and_calibrated_servo_plant() {
    let q = compile_il(
        "LD X0\nDPPMR K1000000 K1000000 K1000 Y0\nDELAY K1000\nDELAY K1000\nDELAY K1\nEND",
    )
    .unwrap();
    let mut s = sim();
    s.scan(&q, 0);
    assert_ne!(s.last_scan, ScanStatus::Complete);
    assert!(s.memory.pulse_output.axis_owner(0).is_none());
    assert!(s.memory.pulse_output.axis_owner(2).is_none());
    let before = xy(&s);
    advance(&mut s, 1000000);
    assert_eq!(xy(&s), before);
    let q = compile_il(&SERVO.replace("DDRVIC K1 K60 K600", "DDRVIC K1 K600 K600")).unwrap();
    let mut s = servo_sim();
    calibrated(&mut s, &q);
    s.memory.set_input(d("X23"), true);
    scan(&mut s, &q);
    servo_ack(&mut s);
    advance(&mut s, 100000);
    s.memory.set_input(d("X24"), true);
    s.scan(&q, 0);
    assert_ne!(s.last_scan, ScanStatus::Complete);
    let before = s.memory.coordinated_motion.servo_position(1);
    advance(&mut s, 1000000);
    assert_eq!(s.memory.coordinated_motion.servo_position(1), before);
    assert!(s.memory.canopen.exchange_owner().is_none());
    assert!(!s.memory.system_bit(1624));
}
