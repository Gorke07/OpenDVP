use super::*;
use crate::serial_io::{PortConfig, SerialIoAction, SerialPort, SerialProfile};
fn d(index: u16) -> Device {
    Device {
        area: Area::D,
        index,
    }
}
fn a(name: &str, args: &[&str]) -> DriveAction {
    DriveAction::parse(
        name,
        &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn memory() -> Memory {
    let mut m = Memory::default();
    m.serial_io
        .configure(SerialPort::Com2, PortConfig::rs485())
        .unwrap();
    m.set_system(1177, true);
    m.set_word(d(1120), 0x86);
    m.set_word(d(1129), 100);
    m
}
fn start(m: &mut Memory, a: &DriveAction) {
    m.set_system(1122, true);
    a.execute(m, true, 10);
}
fn advance(m: &mut Memory, at: u64) {
    m.timing_io.now_us = at;
    SerialBus::advance(m, at);
}
fn response(m: &mut Memory, bytes: &[u8]) {
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
fn exact_forms_are_narrow_khd_only() {
    for (name, args, header, words) in [
        ("FWD", vec!["K1", "K500", "K0"], 0xc600, 7),
        ("REV", vec!["K1", "D0", "H1"], 0xc800, 7),
        ("STOP", vec!["K1", "K0", "K0"], 0xca00, 7),
        ("RDST", vec!["K1", "K0"], 0xcc00, 5),
        ("RSTEF", vec!["K1", "K0"], 0xce00, 5),
    ] {
        let v = a(name, &args);
        assert_eq!(v.header(), header);
        assert_eq!(v.words(), words);
        assert_eq!(v.operands().len(), args.len());
        let mut bad = args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        for source in ["T0", "K4M0", "E0"] {
            bad[0] = source.into();
            assert!(DriveAction::parse(name, &bad).is_err());
        }
        assert!(DriveAction::parse(&format!("{name}P"), &bad).is_err());
    }
}

#[test]
fn reserved_operands_do_not_read_unavailable_model_registers() {
    for (name, args) in [
        ("RDST", vec!["K1", "D6000"]),
        ("RSTEF", vec!["K1", "D6000"]),
        ("STOP", vec!["K1", "D6001", "D6000"]),
    ] {
        let mut m = memory();
        m.data_register_limit = Some(4999);
        m.serial_io.drive_profile.carrier = DriveCarrier::FactoryRegisterCandidate;
        start(&mut m, &a(name, &args));
        assert_eq!(m.serial_io.outgoing_frames().len(), 1, "{name}");
        assert!(
            m.execution_limitations.is_empty(),
            "{:?}",
            m.execution_limitations
        );
    }
}
#[test]
fn published_fwd_frame_and_echo_ack_use_shared_done_not_completion() {
    let mut m = memory();
    m.set_system(1029, true);
    m.set_system(1140, true);
    m.set_system(1141, true);
    start(&mut m, &a("FWD", &["K1", "K500", "K0"]));
    assert_eq!(
        m.serial_io.outgoing_frames().back().unwrap().bytes,
        b":01102000000204001201F4C2\r\n"
    );
    response(&mut m, b":011020000002CD\r\n");
    assert!(m.system_bit(1127));
    assert!(m.system_bit(1029));
    assert!(m.system_bit(1140));
    assert!(m.system_bit(1141));
    assert!(m.serial_io.transaction(SerialPort::Com2).is_none());
}
#[test]
fn rdst_reply_is_five_direct_words_and_holds_other_banks() {
    let mut m = memory();
    m.set_word(d(1050), 777);
    start(&mut m, &a("RDST", &["K1", "K0"]));
    assert_eq!(
        m.serial_io.outgoing_frames().back().unwrap().bytes,
        b":010321000005D6\r\n"
    );
    response(&mut m, b":01030A00C87C083E0093AB00002A\r\n");
    assert_eq!(
        (1070..1075)
            .map(|i| m.word(d(i)) as u16)
            .collect::<Vec<_>>(),
        vec![0xc8, 0x7c08, 0x3e00, 0x93ab, 0]
    );
    assert_eq!(m.word(d(1050)), 777);
}
#[test]
fn reset_echo_and_broadcast_are_real_transactions() {
    let mut m = memory();
    start(&mut m, &a("RSTEF", &["K1", "K0"]));
    assert_eq!(
        m.serial_io.outgoing_frames().back().unwrap().bytes,
        b":010620020002D5\r\n"
    );
    response(&mut m, b":010620020002D5\r\n");
    assert!(m.system_bit(1127));
    m.set_system(1127, false);
    start(&mut m, &a("RSTEF", &["K0", "K0"]));
    let end = m
        .serial_io
        .outgoing_frames()
        .back()
        .unwrap()
        .completed_at_us;
    advance(&mut m, end);
    assert!(m.system_bit(1127));
    assert!(m.serial_io.transaction(SerialPort::Com2).is_none());
}
#[test]
fn rev_stop_and_jog_require_explicit_candidate_and_ignore_unused_frequency() {
    for (name, n, word) in [
        ("REV", "K0", 0x22),
        ("REV", "K1", 0x23),
        ("FWD", "K1", 0x13),
        ("STOP", "K0", 1),
    ] {
        let mut m = memory();
        let action = a(name, &["K1", "K500", n]);
        start(&mut m, &action);
        assert!(m.serial_io.outgoing_frames().is_empty());
        m.serial_io.drive_profile.carrier = DriveCarrier::FactoryRegisterCandidate;
        action.execute(&mut m, true, 10);
        let wire = &m.serial_io.outgoing_frames().back().unwrap().bytes;
        let f =
            crate::serial_protocol::decode(crate::serial_protocol::WireMode::Ascii, wire).unwrap();
        if n == "K1" || name == "STOP" {
            assert_eq!(f.function, 6);
            assert_eq!(f.data, vec![0x20, 0, 0, word]);
        } else {
            assert_eq!(f.function, 16);
            assert_eq!(&f.data[5..7], &[0, word]);
        }
    }
}
#[test]
fn vfd_a_never_silently_falls_back_and_polarity_is_named() {
    let mut m = memory();
    m.set_system(1177, false);
    start(&mut m, &a("FWD", &["K0", "K500", "K1"]));
    assert!(m.serial_io.outgoing_frames().is_empty());
    assert!(m.execution_limitations.iter().any(|s| s.contains("C/Q")));
    assert!(m.system_bit(1122));
    m.serial_io.drive_profile.polarity = DriveFamilyPolarity::SystemTablePolarity;
    a("RSTEF", &["K1", "K0"]).execute(&mut m, true, 10);
    assert_eq!(m.serial_io.outgoing_frames().len(), 1);
}
#[test]
fn existing_serial_owner_cannot_be_stolen_by_drive_and_reverse() {
    let mut m = memory();
    let read = SerialIoAction::parse("MODRD", &["K1".into(), "H2000".into(), "K2".into()]).unwrap();
    m.set_system(1122, true);
    read.execute(&mut m, true, 30);
    a("RSTEF", &["K1", "K0"]).execute(&mut m, true, 10);
    assert_eq!(m.serial_io.transaction(SerialPort::Com2).unwrap().pc, 30);
    assert_eq!(m.serial_io.outgoing_frames().len(), 1);
    SerialBus::abort(&mut m);
    m.set_system(1122, true);
    a("RSTEF", &["K1", "K0"]).execute(&mut m, true, 10);
    read.execute(&mut m, true, 30);
    assert_eq!(m.serial_io.transaction(SerialPort::Com2).unwrap().pc, 10);
}
#[test]
fn wrong_station_retains_owner_and_original_deadline_then_good_reply_commits() {
    let mut m = memory();
    start(&mut m, &a("RSTEF", &["K1", "K0"]));
    response(&mut m, b":020620020002D4\r\n");
    assert!(!m.system_bit(1127));
    assert!(m.serial_io.transaction(SerialPort::Com2).is_some());
    let at = m.serial_io.now_us + 1000;
    m.serial_io
        .queue_received(SerialPort::Com2, at, b":010620020002D5\r\n".to_vec())
        .unwrap();
    advance(&mut m, at);
    assert!(m.system_bit(1127));
}
#[test]
fn timeout_holds_rdst_values_and_foreign_error_flags() {
    let mut m = memory();
    m.set_word(d(1070), 1234);
    m.set_system(1140, true);
    start(&mut m, &a("RDST", &["K1", "K0"]));
    let end = m
        .serial_io
        .outgoing_frames()
        .back()
        .unwrap()
        .completed_at_us;
    advance(&mut m, end + 101000);
    assert!(m.system_bit(1129));
    assert_eq!(m.word(d(1130)), 1);
    assert_eq!(m.word(d(1070)), 1234);
    assert!(m.system_bit(1140));
    assert!(!m.system_bit(1127));
}
#[test]
fn false_contact_continues_snapshot_and_restart_discards_old_epoch() {
    let mut m = memory();
    m.serial_io.false_contact_policy = crate::serial_io::FalseContactPolicy::ContinueLatched;
    let action = a("RDST", &["D0", "K0"]);
    m.set_word(d(0), 1);
    start(&mut m, &action);
    m.set_word(d(0), 2);
    action.execute(&mut m, false, 10);
    assert!(m.serial_io.transaction(SerialPort::Com2).is_some());
    let end = m
        .serial_io
        .outgoing_frames()
        .back()
        .unwrap()
        .completed_at_us;
    m.serial_io
        .queue_received(
            SerialPort::Com2,
            end + 2000,
            b":01030A00C87C083E0093AB00002A\r\n".to_vec(),
        )
        .unwrap();
    m.serial_io.restart_cpu_clock(0).unwrap();
    advance(&mut m, end + 2000);
    assert!(!m.system_bit(1127));
    assert_eq!(m.word(d(1070)), 0);
    assert_eq!(m.serial_io.configured_ports().count(), 1);
}
#[test]
fn port_interface_and_whole_reply_span_fail_before_claim() {
    let mut m = memory();
    m.data_register_limit = Some(1073);
    start(&mut m, &a("RDST", &["K1", "K0"]));
    assert!(m.serial_io.outgoing_frames().is_empty());
    assert!(m.system_bit(1122));
    let mut m = Memory::default();
    assert!(
        m.serial_io
            .configure(SerialPort::Com2, PortConfig::rs232())
            .is_err()
    );
    m.set_system(1177, true);
    start(&mut m, &a("RSTEF", &["K1", "K0"]));
    assert!(m.serial_io.outgoing_frames().is_empty());
}

#[test]
fn default_false_contact_cancels_only_the_owning_drive_site() {
    let mut m = memory();
    let action = a("RDST", &["K1", "K0"]);
    start(&mut m, &action);
    action.execute(&mut m, false, 20);
    assert!(m.serial_io.transaction(SerialPort::Com2).is_some());
    action.execute(&mut m, false, 10);
    assert!(m.serial_io.transaction(SerialPort::Com2).is_none());
    assert!(!m.system_bit(1122));
    assert!(!m.system_bit(1127));
}
#[test]
fn com3_uses_explicit_hardware_and_candidate_priority() {
    let mut m = memory();
    m.serial_io
        .set_profile(SerialProfile {
            has_com3: true,
            ..SerialProfile::default()
        })
        .unwrap();
    m.serial_io
        .configure(SerialPort::Com3, PortConfig::rs485())
        .unwrap();
    m.serial_io.drive_profile.ports = DrivePortPolicy::Com3ThenCom2Candidate;
    m.set_word(d(1109), 0x86);
    m.set_word(d(1252), 50);
    m.set_system(1316, true);
    start(&mut m, &a("RSTEF", &["K1", "K0"]));
    assert_eq!(
        m.serial_io.outgoing_frames().back().unwrap().port,
        SerialPort::Com3
    );
    assert!(m.system_bit(1122));
}
