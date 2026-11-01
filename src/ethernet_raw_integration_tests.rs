//! Independent PDF533..543 raw Ethernet virtual-event acceptance.
//! Packet is an explicit message event, not a recovered TCP stream boundary.
use crate::ethernet_raw::{
    Endpoint, Mode4Revision, RawEthernetModel, RawEthernetProfile, RawEvent, RawRequestKind,
};
use crate::file_memory::FirmwareVersion;
use crate::plc::{Device, Program, ScanStatus, Simulator, compile_il};
fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn peer() -> Endpoint {
    Endpoint {
        ip: [192, 168, 1, 18],
        port: 10000,
    }
}
fn sim(mode: i16, tcp: bool) -> Simulator {
    let mut s = Simulator::default();
    s.memory.set_input(d("X20"), true);
    s.memory.ethernet_raw.profile = RawEthernetProfile {
        model: RawEthernetModel::Se12,
        firmware: FirmwareVersion::new(1, 95),
        ..RawEthernetProfile::default()
    };
    s.memory.ethernet_raw.link_up = true;
    for (i, v) in [
        (0, 1024),
        (1, 0x0112),
        (2, 0xc0a8u16 as i16),
        (3, 10000),
        (4, i16::from(tcp)),
        (5, mode),
        (6, 100),
        (100, 0xaa41u16 as i16),
        (101, 0xbb42u16 as i16),
        (200, 0),
    ] {
        s.memory.set_word(d(&format!("D{i}")), v);
    }
    s
}
fn p() -> Program {
    compile_il("LD X20\nETHRS D0 D5 D100 K2 D200 M100\nEND").unwrap()
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
fn event(s: &mut Simulator, event: RawEvent) {
    let r = s.memory.ethernet_raw.outgoing().back().unwrap().clone();
    let at = s.memory.timing_io.now_us + 1;
    s.memory
        .ethernet_raw
        .queue_event(at, r.id, r.epoch, event)
        .unwrap();
    advance(s, 1);
}
fn opened(s: &mut Simulator) {
    event(s, RawEvent::Opened { peer: Some(peer()) });
}
fn sent(s: &mut Simulator) {
    event(s, RawEvent::Sent);
}
fn packet(s: &mut Simulator, bytes: &[u8]) {
    event(
        s,
        RawEvent::Packet {
            peer: peer(),
            bytes: bytes.to_vec(),
        },
    );
}
#[test]
fn genuine_six_operand_layout_is_thirteen_words_and_native_guarded() {
    let q = compile_il("LD X20\nETHRS D0 D5 D100 K2 D200 M100\nMOV K7 D300\nEND").unwrap();
    let e = crate::es2_encoder::local_execution_layout(&q).unwrap();
    assert_eq!(e.known_word_at(1), Some(0xc002));
    assert_eq!(e.action_pc, vec![1, 14]);
    assert!(crate::es2_encoder::encode(&q).is_err());
}
#[test]
fn configured_model_and_link_are_required_before_any_socket_success() {
    for disabled in [true, false] {
        let mut s = sim(1, true);
        if disabled {
            s.memory.ethernet_raw.profile.model = RawEthernetModel::Disabled;
        } else {
            s.memory.ethernet_raw.link_up = false;
        }
        s.scan(&p(), 0);
        assert!(s.memory.ethernet_raw.outgoing().is_empty());
        assert!(!s.memory.bit(d("M100")));
        assert!(!s.memory.execution_limitations.is_empty() || s.memory.system_bit(1198));
    }
}
#[test]
fn client0_uses_one_low_byte_per_d_and_waits_for_actual_send_ack() {
    let mut s = sim(0, false);
    scan(&mut s, &p());
    let r = s.memory.ethernet_raw.outgoing().back().unwrap();
    match &r.kind {
        RawRequestKind::Open { config } => {
            assert!(!config.tcp);
            assert_eq!(config.local_port, 1024);
            assert_eq!(config.remote.ip, [192, 168, 1, 18]);
            assert!(!config.server);
        }
        _ => panic!("expectedOpen"),
    };
    assert!(!s.memory.bit(d("M100")));
    opened(&mut s);
    assert!(
        matches!(&s.memory.ethernet_raw.outgoing().back().unwrap().kind,RawRequestKind::Send{bytes,..} if bytes==&[0x41,0x42])
    );
    assert!(!s.memory.bit(d("M100")));
    sent(&mut s);
    assert!(s.memory.bit(d("M100")));
    assert!(s.memory.system_bit(1197));
    assert!(!matches!(
        s.memory.ethernet_raw.outgoing().back().unwrap().kind,
        RawRequestKind::Receive
    ));
}
#[test]
fn client1_send_receive_commits_explicit_packet_length_then_individual_low_bytes() {
    let mut s = sim(1, true);
    scan(&mut s, &p());
    opened(&mut s);
    sent(&mut s);
    assert!(matches!(
        s.memory.ethernet_raw.outgoing().back().unwrap().kind,
        RawRequestKind::Receive
    ));
    assert!(!s.memory.bit(d("M100")));
    packet(&mut s, &[0x10, 0x20, 0xff]);
    assert_eq!(s.memory.word(d("D200")), 3);
    for (i, v) in [(201, 0x10), (202, 0x20), (203, 0xff)] {
        assert_eq!((s.memory.word(d(&format!("D{i}"))) as u16) & 255, v);
    }
    assert!(s.memory.bit(d("M100")));
    assert!(s.memory.system_bit(1197));
    assert!(!s.memory.system_bit(1198));
}
#[test]
fn zero_length_client1_is_receive_only_in_new_firmware_but_not_old_12se() {
    let q = compile_il("LD X20\nETHRS D0 D5 D100 K0 D200 M100\nEND").unwrap();
    let mut s = sim(1, true);
    scan(&mut s, &q);
    opened(&mut s);
    assert!(matches!(
        s.memory.ethernet_raw.outgoing().back().unwrap().kind,
        RawRequestKind::Receive
    ));
    packet(&mut s, &[42]);
    assert_eq!(s.memory.word(d("D200")), 1);
    let mut old = sim(1, true);
    old.memory.ethernet_raw.profile.firmware = FirmwareVersion::new(1, 90);
    old.scan(&q, 0);
    assert!(old.memory.ethernet_raw.outgoing().is_empty());
    assert!(!old.memory.bit(d("M100")));
}
#[test]
fn server2_receives_before_sending_prepared_reply_even_with_source_alias() {
    let q = compile_il("LD X20\nETHRS D0 D5 D201 K1 D200 M100\nEND").unwrap();
    let mut s = sim(2, true);
    s.memory.set_word(d("D201"), 0x41);
    s.memory.set_system(1196, true);
    scan(&mut s, &q);
    opened(&mut s);
    assert!(matches!(
        s.memory.ethernet_raw.outgoing().back().unwrap().kind,
        RawRequestKind::Receive
    ));
    assert!(!s.memory.bit(d("M100")));
    packet(&mut s, &[0x99]);
    assert!(
        matches!(&s.memory.ethernet_raw.outgoing().back().unwrap().kind,RawRequestKind::Send{bytes,..} if bytes==&[0x41])
    );
    assert!(!s.memory.bit(d("M100")));
    sent(&mut s);
    assert!(s.memory.bit(d("M100")));
}
#[test]
fn server3_does_not_read_or_range_check_unused_tx_source_and_length() {
    let q = compile_il("LD X20\nETHRS D0 D5 D4999 D4999 D200 M100\nEND").unwrap();
    let mut s = sim(3, false);
    s.memory.data_register_limit = Some(4999);
    s.memory.set_word(d("D4999"), -1);
    s.memory.set_word(d("D6"), 0);
    scan(&mut s, &q);
    opened(&mut s);
    packet(&mut s, &[1, 2]);
    assert!(s.memory.bit(d("M100")));
    assert_eq!(s.memory.word(d("D200")), 2);
    assert!(
        s.memory
            .ethernet_raw
            .outgoing()
            .iter()
            .all(|r| !matches!(r.kind, RawRequestKind::Send { .. }))
    );
}
#[test]
fn server3_then4_uses_newly_prepared_payload_and_same_connection() {
    let q = p();
    let mut s = sim(3, true);
    s.memory.set_system(1196, true);
    scan(&mut s, &q);
    opened(&mut s);
    packet(&mut s, &[3]);
    assert!(s.memory.bit(d("M100")));
    let opens = s
        .memory
        .ethernet_raw
        .outgoing()
        .iter()
        .filter(|r| matches!(r.kind, RawRequestKind::Open { .. }))
        .count();
    s.memory.set_bit(d("M100"), false);
    s.memory.set_word(d("D5"), 4);
    s.memory.set_word(d("D100"), 0x43);
    s.memory.set_word(d("D101"), 0x44);
    scan(&mut s, &q);
    assert!(
        matches!(&s.memory.ethernet_raw.outgoing().back().unwrap().kind,RawRequestKind::Send{bytes,..} if bytes==&[0x43,0x44])
    );
    assert_eq!(
        s.memory
            .ethernet_raw
            .outgoing()
            .iter()
            .filter(|r| matches!(r.kind, RawRequestKind::Open { .. }))
            .count(),
        opens
    );
    sent(&mut s);
    assert!(s.memory.bit(d("M100")));
}
#[test]
fn tuple_preflight_rejects_client_zero_ip_remote_zero_and_server_local_zero() {
    for (mode, field, value) in [(1, 1, 0), (1, 3, 0), (2, 0, 0)] {
        let mut s = sim(mode, true);
        s.memory.set_word(d(&format!("D{field}")), value);
        if field == 1 {
            s.memory.set_word(d("D2"), 0);
        }
        s.scan(&p(), 0);
        assert!(s.memory.ethernet_raw.outgoing().is_empty());
        assert!(!s.memory.bit(d("M100")));
    }
}
#[test]
fn reserved_port_namespaces_are_protocol_specific() {
    let mut tcp = sim(0, true);
    tcp.memory.set_word(d("D0"), 502);
    tcp.scan(&p(), 0);
    assert!(tcp.memory.ethernet_raw.outgoing().is_empty());
    assert!(tcp.memory.system_bit(1198));
    assert_eq!(tcp.memory.word(d("D1176")) as u16, 0x600c);
    let mut udp = sim(0, false);
    udp.memory.set_word(d("D0"), 502);
    scan(&mut udp, &p());
    assert!(matches!(
        udp.memory.ethernet_raw.outgoing().back().unwrap().kind,
        RawRequestKind::Open { .. }
    ));
}
#[test]
fn tcp_timeout_starts_after_send_ack_and_requires_false_rearm() {
    let q = p();
    let mut s = sim(1, true);
    s.memory.set_system(1196, true);
    scan(&mut s, &q);
    opened(&mut s);
    advance(&mut s, 1000);
    assert!(!s.memory.system_bit(1198));
    sent(&mut s);
    advance(&mut s, 99999);
    assert!(!s.memory.system_bit(1198));
    advance(&mut s, 1);
    assert!(s.memory.system_bit(1198));
    assert_eq!(s.memory.word(d("D1176")) as u16, 0x6212);
    assert!(!s.memory.bit(d("M100")));
    let n = s.memory.ethernet_raw.outgoing().len();
    scan(&mut s, &q);
    assert_eq!(s.memory.ethernet_raw.outgoing().len(), n);
}
#[test]
fn false_instruction_cancels_payload_commit_but_m1196_holds_tcp_connection() {
    let q = p();
    let mut s = sim(1, true);
    s.memory.set_system(1196, true);
    s.memory.set_word(d("D200"), 77);
    scan(&mut s, &q);
    opened(&mut s);
    sent(&mut s);
    let old = s.memory.ethernet_raw.outgoing().back().unwrap().clone();
    s.memory.set_input(d("X20"), false);
    scan(&mut s, &q);
    let at = s.memory.timing_io.now_us + 1;
    s.memory
        .ethernet_raw
        .queue_event(
            at,
            old.id,
            old.epoch,
            RawEvent::Packet {
                peer: peer(),
                bytes: vec![42],
            },
        )
        .unwrap();
    advance(&mut s, 1);
    assert_eq!(s.memory.word(d("D200")), 77);
    assert!(!s.memory.bit(d("M100")));
    assert!(!s.memory.system_bit(1198));
    assert!(s.memory.system_bit(1197));
}
#[test]
fn mode4_firmware_conflict_is_an_explicit_profile_choice() {
    let mut strict = sim(4, true);
    strict.memory.ethernet_raw.profile.model = RawEthernetModel::Es2E;
    strict.memory.ethernet_raw.profile.firmware = FirmwareVersion::new(1, 46);
    strict.scan(&p(), 0);
    assert!(strict.memory.ethernet_raw.outgoing().is_empty());
    let mut detail = sim(4, true);
    detail.memory.ethernet_raw.profile.model = RawEthernetModel::Es2E;
    detail.memory.ethernet_raw.profile.firmware = FirmwareVersion::new(1, 46);
    detail.memory.ethernet_raw.profile.es2e_mode4 = Mode4Revision::Detail146;
    scan(&mut detail, &p());
    opened(&mut detail);
    assert!(matches!(
        detail.memory.ethernet_raw.outgoing().back().unwrap().kind,
        RawRequestKind::Send { .. }
    ));
}
#[test]
fn restart_discards_old_socket_epoch_before_a_new_request_can_commit() {
    let q = p();
    let mut s = sim(1, true);
    scan(&mut s, &q);
    let old = s.memory.ethernet_raw.outgoing().back().unwrap().clone();
    s.restart_preserving_file_store();
    // Recreate public RAM descriptor after CPU reset; preserve configured peer environment.
    for (i, v) in [
        (0, 1024),
        (1, 0x0112),
        (2, 0xc0a8u16 as i16),
        (3, 10000),
        (4, 1),
        (5, 1),
        (6, 100),
        (100, 0x41),
        (101, 0x42),
    ] {
        s.memory.set_word(d(&format!("D{i}")), v);
    }
    s.memory.set_input(d("X20"), true);
    scan(&mut s, &q);
    let current = s.memory.ethernet_raw.outgoing().back().unwrap().clone();
    assert_ne!(current.epoch, old.epoch);
    let at = s.memory.timing_io.now_us + 1;
    s.memory
        .ethernet_raw
        .queue_event(
            at,
            old.id,
            old.epoch,
            RawEvent::Opened { peer: Some(peer()) },
        )
        .unwrap();
    advance(&mut s, 1);
    assert_eq!(
        s.memory.ethernet_raw.outgoing().back().unwrap().id,
        current.id
    );
    assert!(!s.memory.bit(d("M100")));
}

#[test]
fn whole_spans_preflight_atomically_and_only_one_ethrs_job_owns_execution() {
    for line in [
        "ETHRS D4997 D5 D100 K2 D200 M100",
        "ETHRS D0 D5 D100 K2 D4999 M100",
    ] {
        let q = compile_il(&format!("LD X20\n{line}\nEND")).unwrap();
        let mut s = sim(1, true);
        s.memory.data_register_limit = Some(4999);
        s.memory.set_word(d("D200"), 123);
        s.memory.set_word(d("D4999"), 456);
        s.scan(&q, 0);
        assert!(s.memory.ethernet_raw.outgoing().is_empty());
        assert_eq!(s.memory.word(d("D200")), 123);
        assert_eq!(s.memory.word(d("D4999")), 456);
        assert!(!s.memory.bit(d("M100")));
    }
    let q =
        compile_il("LD X20\nETHRS D0 D5 D100 K2 D200 M100\nETHRS D10 D5 D100 K2 D300 M101\nEND")
            .unwrap();
    let mut s = sim(1, true);
    for (i, v) in [
        (10, 2048),
        (11, 0x0113),
        (12, 0xc0a8u16 as i16),
        (13, 10001),
        (14, 1),
    ] {
        s.memory.set_word(d(&format!("D{i}")), v);
    }
    s.scan(&q, 0);
    assert_eq!(s.memory.ethernet_raw.outgoing().len(), 1);
    assert_eq!(s.memory.ethernet_raw.outgoing().front().unwrap().pc, 1);
    assert!(!s.memory.bit(d("M101")));
}
