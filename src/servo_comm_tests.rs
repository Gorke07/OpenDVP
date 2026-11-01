use super::*;
use crate::serial_io::{PortConfig, SerialIoAction, SerialPort};
use crate::serial_protocol::{self, WireMode};
fn d(index: u16) -> Device {
    Device {
        area: Area::D,
        index,
    }
}
fn a(selector: &str, data: &str) -> ServoAction {
    ServoAction::parse("ASDRW", &["K1".into(), selector.into(), data.into()]).unwrap()
}
fn profile(model: ServoModel) -> ServoProfile {
    ServoProfile {
        model,
        carrier: ServoCarrier::StandardModbusCandidate,
        ..ServoProfile::default()
    }
}
fn memory(model: ServoModel) -> Memory {
    let mut m = Memory::default();
    m.serial_io
        .configure(SerialPort::Com2, PortConfig::rs485())
        .unwrap();
    m.serial_io.servo_profile = profile(model);
    m.set_word(d(1120), 0x86);
    m.set_word(d(1129), 100);
    m
}
fn start(m: &mut Memory, action: &ServoAction) {
    m.set_system(1122, true);
    action.execute(m, true, 30);
}
fn advance(m: &mut Memory, at: u64) {
    m.timing_io.now_us = at;
    SerialBus::advance(m, at);
}
fn reply(m: &mut Memory, bytes: &[u8]) {
    let at = m
        .serial_io
        .outgoing_frames()
        .back()
        .unwrap()
        .completed_at_us
        + 2000;
    m.serial_io
        .queue_received(SerialPort::Com2, at, bytes.to_vec())
        .unwrap();
    advance(m, at);
}

#[test]
fn genuine_form_and_strict_scalar_matrix() {
    let action = a("K0", "D0");
    assert_eq!(action.header(), 0xba01);
    assert_eq!(action.words(), 7);
    assert_eq!(action.operands().len(), 3);
    for args in [["T0", "K0", "D0"], ["K1", "K4M0", "D0"], ["K1", "K0", "M0"]] {
        assert!(ServoAction::parse("ASDRW", &args.map(String::from)).is_err());
    }
    assert!(ServoAction::parse("ASDRWP", &["K1".into(), "K0".into(), "D0".into()]).is_err());
}
#[test]
fn no_default_model_or_carrier_creates_a_success() {
    let m = Memory::default();
    assert!(a("K0", "D0").prepare(&m, ServoProfile::default()).is_err());
    assert!(
        a("K0", "D0")
            .prepare(
                &m,
                ServoProfile {
                    model: ServoModel::LegacyA,
                    ..ServoProfile::default()
                }
            )
            .is_err()
    );
}
#[test]
fn full_legacy_and_a2_selector_maps_match_pdf_addresses_and_counts() {
    let mut m = Memory::default();
    m.set_word(d(0), 1);
    for (model, selector, address, count, read) in [
        (ServoModel::LegacyA, 0, 4, 5, true),
        (ServoModel::Ab, 1, 9, 8, true),
        (ServoModel::APlus, 2, 9, 8, false),
        (ServoModel::LegacyA, 3, 0x405, 1, false),
        (ServoModel::B, 4, 0x21e, 1, false),
        (ServoModel::B, 5, 0x109, 3, false),
        (ServoModel::B, 6, 0x10c, 3, false),
        (ServoModel::A2, 16, 0x12, 10, true),
        (ServoModel::A2, 17, 0x22, 8, false),
        (ServoModel::A2, 18, 0x32, 8, false),
        (ServoModel::A2, 19, 0x40a, 1, false),
        (ServoModel::A2, 20, 0x23c, 1, false),
        (ServoModel::A2, 21, 0x112, 6, false),
        (ServoModel::A2, 22, 0x118, 6, false),
    ] {
        let mut p = profile(model);
        p.pair_order = ServoPairOrder::LowWordFirst;
        let (frame, dest) = a(&format!("K{selector}"), "D0").prepare(&m, p).unwrap();
        assert_eq!(&frame.data[..2], &(address as u16).to_be_bytes());
        assert_eq!(dest.is_some(), read);
        assert_eq!(
            frame.function,
            if read {
                3
            } else if count == 1 {
                6
            } else {
                16
            }
        );
        if count != 1 {
            assert_eq!(&frame.data[2..4], &(count as u16).to_be_bytes());
        }
    }
    for selector in [1, 2] {
        assert!(
            a(&format!("K{selector}"), "D0")
                .prepare(&m, profile(ServoModel::B))
                .is_err()
        );
    }
}
#[test]
fn a2_mapping23_requires_an_explicit_read_or_write_choice() {
    let m = Memory::default();
    let action = a("K23", "D0");
    let mut p = profile(ServoModel::A2);
    assert!(action.prepare(&m, p).is_err());
    for (direction, function) in [(Mapping23::Read, 3), (Mapping23::Write, 16)] {
        p.mapping23 = direction;
        let (f, _) = action.prepare(&m, p).unwrap();
        assert_eq!(f.function, function);
        assert_eq!(&f.data[..4], &[0, 0x46, 0, 8]);
    }
}
#[test]
fn signed_value_ranges_and_explicit_pair_order_are_checked_before_tx() {
    let mut m = Memory::default();
    let action = a("K5", "D0");
    m.set_word(d(2), 5001);
    assert!(action.prepare(&m, profile(ServoModel::LegacyA)).is_err());
    m.set_word(d(2), -5000);
    assert!(action.prepare(&m, profile(ServoModel::LegacyA)).is_ok());
    let action = a("K21", "D0");
    m.set_double_word(d(0), -60000);
    m.set_double_word(d(2), 60000);
    m.set_double_word(d(4), 0);
    let mut p = profile(ServoModel::A2);
    assert!(action.prepare(&m, p).is_err());
    p.pair_order = ServoPairOrder::LowWordFirst;
    let (low, _) = action.prepare(&m, p).unwrap();
    p.pair_order = ServoPairOrder::HighWordFirst;
    let (high, _) = action.prepare(&m, p).unwrap();
    assert_eq!(&low.data[5..9], &[0x15, 0xa0, 0xff, 0xff]);
    assert_eq!(&high.data[5..9], &[0xff, 0xff, 0x15, 0xa0]);
    m.set_double_word(d(4), 60001);
    assert!(action.prepare(&m, p).is_err());
}
#[test]
fn jog_ranges_and_servo_on_normalization_do_not_mutate_source() {
    let mut m = Memory::default();
    m.set_word(d(0), 4000);
    assert!(
        a("K3", "D0")
            .prepare(&m, profile(ServoModel::LegacyA))
            .is_err()
    );
    assert!(a("K19", "D0").prepare(&m, profile(ServoModel::A2)).is_ok());
    m.set_word(d(0), -1);
    let (f, _) = a("K4", "D0")
        .prepare(&m, profile(ServoModel::LegacyA))
        .unwrap();
    assert_eq!(f.data, vec![2, 0x1e, 0, 0]);
    assert_eq!(m.word(d(0)), -1);
}
#[test]
fn literal_ascii_read_frame_commits_direct_converted_payload() {
    let mut m = memory(ServoModel::LegacyA);
    m.set_word(d(1070), 999);
    m.set_system(1131, true);
    m.set_system(1029, true);
    start(&mut m, &a("K0", "D0"));
    assert_eq!(
        m.serial_io.outgoing_frames().back().unwrap().bytes,
        b":010300040005F3\r\n"
    );
    reply(&mut m, b":01030A00010002000300040005E3\r\n");
    assert_eq!(
        (0..5).map(|i| m.word(d(i))).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );
    assert_eq!(m.word(d(1070)), 999);
    assert!(m.system_bit(1127));
    assert!(m.system_bit(1131));
    assert!(m.system_bit(1029));
}
#[test]
fn rtu_read_uses_same_converted_bank_and_waits_actual_frame_silence() {
    let mut m = memory(ServoModel::A2);
    m.set_system(1143, true);
    m.set_word(d(1120), 0x87);
    start(&mut m, &a("K16", "D20"));
    let frame = Frame {
        station: 1,
        function: 3,
        data: std::iter::once(20)
            .chain((1u16..=10).flat_map(u16::to_be_bytes))
            .collect(),
    };
    let wire = serial_protocol::encode(WireMode::Rtu, &frame).unwrap();
    reply(&mut m, &wire);
    assert!(!m.system_bit(1127));
    let eof = m.serial_io.now_us + 4011;
    advance(&mut m, eof);
    assert!(m.system_bit(1127));
    assert_eq!(m.word(d(29)), 10);
}
#[test]
fn complete_read_span_preflight_is_atomic_and_broadcast_read_is_explicit() {
    let mut m = memory(ServoModel::A2);
    m.data_register_limit = Some(27);
    start(&mut m, &a("K16", "D20"));
    assert!(m.serial_io.outgoing_frames().is_empty());
    assert!(m.system_bit(1122));
    let action = ServoAction::parse("ASDRW", &["K0".into(), "K0".into(), "D0".into()]).unwrap();
    let m = Memory::default();
    let mut p = profile(ServoModel::LegacyA);
    assert!(action.prepare(&m, p).is_err());
    p.broadcast_read = BroadcastRead::TransmitWithoutReplyCandidate;
    assert!(action.prepare(&m, p).is_ok());
}
#[test]
fn shared_owner_false_contact_timeout_and_restart_are_real_serial_lifecycle() {
    let mut m = memory(ServoModel::LegacyA);
    m.serial_io.false_contact_policy = crate::serial_io::FalseContactPolicy::ContinueLatched;
    let action = a("K0", "D0");
    start(&mut m, &action);
    action.execute(&mut m, false, 30);
    let other =
        SerialIoAction::parse("MODRD", &["K1".into(), "H2000".into(), "K2".into()]).unwrap();
    other.execute(&mut m, true, 50);
    assert_eq!(m.serial_io.transaction(SerialPort::Com2).unwrap().pc, 30);
    assert_eq!(m.serial_io.outgoing_frames().len(), 1);
    let end = m
        .serial_io
        .outgoing_frames()
        .back()
        .unwrap()
        .completed_at_us;
    advance(&mut m, end + 101000);
    assert!(m.system_bit(1129));
    assert!(!m.system_bit(1127));
    assert_eq!(m.word(d(0)), 0);
    m.serial_io.restart_cpu_clock(m.serial_io.now_us).unwrap();
    assert_eq!(m.serial_io.configured_ports().count(), 1);
    assert_eq!(m.serial_io.servo_profile.model, ServoModel::LegacyA);
    assert!(m.serial_io.outgoing_frames().is_empty());
}

#[test]
fn default_false_contact_cancels_its_servo_owner_without_completion() {
    let mut m = memory(ServoModel::LegacyA);
    let action = a("K0", "D0");
    start(&mut m, &action);
    action.execute(&mut m, false, 99);
    assert_eq!(m.serial_io.transaction(SerialPort::Com2).unwrap().pc, 30);
    action.execute(&mut m, false, 30);
    assert!(m.serial_io.transaction(SerialPort::Com2).is_none());
    assert!(!m.system_bit(1122));
    assert!(!m.system_bit(1127));
}
#[test]
fn broadcast_write_completes_without_fabricating_a_response_or_motor_state() {
    let mut m = memory(ServoModel::LegacyA);
    m.set_word(d(0), 1);
    let action = ServoAction::parse("ASDRW", &["K0".into(), "K4".into(), "D0".into()]).unwrap();
    start(&mut m, &action);
    let end = m
        .serial_io
        .outgoing_frames()
        .back()
        .unwrap()
        .completed_at_us;
    advance(&mut m, end);
    assert!(m.system_bit(1127));
    assert_eq!(m.word(d(0)), 1);
    assert!(m.serial_io.transaction(SerialPort::Com2).is_none());
}
