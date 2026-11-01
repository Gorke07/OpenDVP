//! PDF544..557 literal packed-byte and explicit peer-event acceptance.
//! Virtual endpoint events do not establish physical Ethernet/native parity.
use crate::ethernet_config::{
    ConfigEvent, ConfigRequest, ConfigRequestKind, EthernetConfigProfile,
};
use crate::ethernet_raw::Endpoint;
use crate::plc::{Device, Program, ScanStatus, Simulator, compile_il};
fn d(s: &str) -> Device {
    s.parse().unwrap()
}
const CONF: &str = "LD X20\nETHCONF K1 D0 K10000 K0 M0 M1 M2 K8 K10 M100 M101 D2000\n";
const MSG: &str = "LD X21\nETHMSG K1 M10 D100 D10 M11 M12 D200 M102 M103 D300 D210 M104 D2002\nEND";
fn program() -> Program {
    compile_il(&format!("{CONF}{MSG}")).unwrap()
}
fn sim(tcp: bool) -> Simulator {
    let mut s = Simulator::default();
    s.memory.ethernet_config.profile = EthernetConfigProfile::Se2;
    s.memory.ethernet_config.link_up = true;
    s.memory.set_bit(d("M0"), tcp);
    for (address, value) in [
        ("D0", 0xc0a8u16 as i16),
        ("D1", 0x0102),
        ("D10", 3),
        ("D100", 0x4241),
        ("D101", 0x4443),
    ] {
        s.memory.set_word(d(address), value);
    }
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
fn request(s: &Simulator) -> ConfigRequest {
    s.memory.ethernet_config.outgoing().back().unwrap().clone()
}
fn event(s: &mut Simulator, r: &ConfigRequest, e: ConfigEvent) {
    let at = s.memory.timing_io.now_us + 1;
    s.memory
        .ethernet_config
        .queue_event(at, r.id, r.epoch, e)
        .unwrap();
    advance(s, 1);
}
fn configured(s: &mut Simulator, p: &Program) {
    s.memory.set_input(d("X20"), true);
    scan(s, p);
    assert!(s.memory.bit(d("M100")));
    assert!(!s.memory.bit(d("M101")));
    s.memory.set_input(d("X20"), false);
    s.memory.set_input(d("X21"), true);
    s.memory.set_bit(d("M10"), true);
}
fn opened(s: &mut Simulator, p: &Program) -> ConfigRequest {
    configured(s, p);
    scan(s, p);
    let r = request(s);
    assert!(matches!(r.kind, ConfigRequestKind::Open { .. }));
    assert!(!s.memory.bit(d("M102")));
    event(s, &r, ConfigEvent::Opened { peer: Some(peer()) });
    r
}
fn peer() -> Endpoint {
    Endpoint {
        ip: [192, 168, 1, 2],
        port: 10000,
    }
}
#[test]
fn genuine_config_message_headers_lengths_and_following_pc_keep_upload_guard() {
    for (body, head, width) in [
        (
            "ETHCONF K1 D0 K10000 K0 M0 M1 M2 K8 K10 M100 M101 D2000",
            0xe600,
            25,
        ),
        (
            "ETHMSG K1 M10 D100 K3 M11 M12 D200 M102 M103 D300 D210 M104 D2002",
            0x1d02,
            27,
        ),
    ] {
        let p = compile_il(&format!("LD X0\n{body}\nMOV K7 D400\nEND")).unwrap();
        let l = crate::es2_encoder::local_execution_layout(&p).unwrap();
        assert_eq!(l.known_word_at(1), Some(head));
        assert_eq!(l.action_pc, vec![1, 1 + width]);
        assert_eq!(l.word_count, 1 + width + 5 + 1);
        assert!(l.unencoded_ranges.is_empty());
        assert!(crate::es2_encoder::encode(&p).is_err());
    }
}
#[test]
fn strict_m_only_flags_d_only_buffers_and_no_p_forms() {
    for body in [
        "ETHCONFP K1 D0 K10000 K0 M0 M1 M2 K8 K10 M100 M101 D2000",
        "ETHCONF K1 D0 K10000 K0 Y0 M1 M2 K8 K10 M100 M101 D2000",
        "ETHCONF K1 K0 K10000 K0 M0 M1 M2 K8 K10 M100 M101 D2000",
        "ETHMSGP K1 M10 D100 K3 M11 M12 D200 M102 M103 D300 D210 M104 D2002",
        "ETHMSG K1 M10 D100 K3 M11 M12 D200 Y0 M103 D300 D210 M104 D2002",
    ] {
        assert!(
            compile_il(&format!("LD X0\n{body}\nEND")).is_err(),
            "{body}"
        );
    }
}
#[test]
fn no_configuration_cannot_open_or_fabricate_success() {
    let mut s = sim(true);
    s.memory.set_input(d("X21"), true);
    s.memory.set_bit(d("M10"), true);
    scan(&mut s, &compile_il(MSG).unwrap());
    assert!(s.memory.ethernet_config.outgoing().is_empty());
    assert!(s.memory.bit(d("M104")));
    assert_eq!(s.memory.word(d("D2002")), 8);
    assert!(!s.memory.bit(d("M102")));
}
#[test]
fn tcp_send_is_packed_snapshot_and_requires_real_ack_then_false_rearm() {
    let p = program();
    let mut s = sim(true);
    opened(&mut s, &p);
    s.memory.set_bit(d("M11"), true);
    scan(&mut s, &p);
    let tx = request(&s);
    match &tx.kind {
        ConfigRequestKind::Send {
            peer: p,
            bytes,
            segment_index,
            segment_count,
        } => {
            assert_eq!(*p, peer());
            assert_eq!(bytes, &vec![0x41, 0x42, 0x43]);
            assert_eq!((*segment_index, *segment_count), (1, 1));
        }
        _ => panic!("expected send"),
    }
    assert!(!s.memory.bit(d("M102")));
    s.memory.set_word(d("D100"), 0);
    event(&mut s, &tx, ConfigEvent::Sent);
    assert!(s.memory.bit(d("M102")));
    let count = s.memory.ethernet_config.outgoing().len();
    scan(&mut s, &p);
    assert_eq!(s.memory.ethernet_config.outgoing().len(), count);
    s.memory.set_bit(d("M11"), false);
    scan(&mut s, &p);
    assert!(!s.memory.bit(d("M102")));
    s.memory.set_bit(d("M11"), true);
    scan(&mut s, &p);
    match &request(&s).kind {
        ConfigRequestKind::Send { bytes, .. } => assert_eq!(bytes, &vec![0, 0, 0x43]),
        _ => panic!("expected rearmed send"),
    }
}
#[test]
fn receive_appends_at_odd_byte_then_clear_resets_length_without_erasing_ram() {
    let p = program();
    let mut s = sim(true);
    let session = opened(&mut s, &p);
    s.memory.set_word(d("D300"), 0x7777);
    s.memory.set_word(d("D301"), 0x6666);
    event(
        &mut s,
        &session,
        ConfigEvent::Packet {
            peer: peer(),
            bytes: vec![0x11, 0x22, 0x33],
        },
    );
    assert_eq!(s.memory.word(d("D300")), 0x2211);
    assert_eq!(s.memory.word(d("D210")), 3);
    event(
        &mut s,
        &session,
        ConfigEvent::Packet {
            peer: peer(),
            bytes: vec![0x44],
        },
    );
    assert_eq!(s.memory.word(d("D301")), 0x4433);
    assert_eq!(s.memory.word(d("D210")), 4);
    s.memory.set_bit(d("M12"), true);
    scan(&mut s, &p);
    assert_eq!(s.memory.word(d("D210")), 0);
    assert!(!s.memory.bit(d("M103")));
    assert_eq!(s.memory.word(d("D300")), 0x2211);
    event(
        &mut s,
        &session,
        ConfigEvent::Packet {
            peer: peer(),
            bytes: vec![0xaa, 0xbb],
        },
    );
    assert_eq!(s.memory.word(d("D300")) as u16, 0xbbaa);
    assert_eq!(s.memory.word(d("D210")), 2);
    scan(&mut s, &p);
    assert_eq!(
        s.memory.word(d("D210")),
        2,
        "Clear held must not clear a second time"
    );
}
#[test]
fn receive_over_capacity_is_atomic_for_data_and_accumulated_length() {
    let p = program();
    let mut s = sim(true);
    let session = opened(&mut s, &p);
    event(
        &mut s,
        &session,
        ConfigEvent::Packet {
            peer: peer(),
            bytes: vec![1, 2, 3, 4, 5, 6],
        },
    );
    let before: Vec<_> = (300..304)
        .map(|n| s.memory.word(d(&format!("D{n}"))))
        .collect();
    event(
        &mut s,
        &session,
        ConfigEvent::Packet {
            peer: peer(),
            bytes: vec![7, 8, 9],
        },
    );
    assert_eq!(
        (300..304)
            .map(|n| s.memory.word(d(&format!("D{n}"))))
            .collect::<Vec<_>>(),
        before
    );
    assert_eq!(s.memory.word(d("D210")), 6);
    assert!(s.memory.bit(d("M104")));
    assert_eq!(s.memory.word(d("D2002")), 14);
}
#[test]
fn tcp_connect_waits_ack_and_connect_false_rejects_late_packet() {
    let p = program();
    let mut s = sim(true);
    configured(&mut s, &p);
    scan(&mut s, &p);
    let session = request(&s);
    assert_eq!(s.memory.word(d("D200")), 1);
    advance(&mut s, 1000);
    assert_eq!(s.memory.word(d("D200")), 1);
    event(&mut s, &session, ConfigEvent::Opened { peer: Some(peer()) });
    assert_eq!(s.memory.word(d("D200")), 2);
    s.memory.set_bit(d("M10"), false);
    scan(&mut s, &p);
    assert_eq!(s.memory.word(d("D200")), 0);
    event(
        &mut s,
        &session,
        ConfigEvent::Packet {
            peer: peer(),
            bytes: vec![0x41, 0x42],
        },
    );
    assert_eq!(s.memory.word(d("D210")), 0);
    assert!(!s.memory.bit(d("M103")));
}
#[test]
fn reserved_server_ports_and_invalid_receive_limits_have_documented_error_codes() {
    for (tcp, port, cap, code) in [
        (true, 502, 8, 4),
        (true, 44818, 8, 4),
        (false, 67, 8, 4),
        (false, 2222, 8, 4),
        (false, 20006, 8, 4),
        (false, 1024, 4097, 6),
    ] {
        let p = compile_il(&format!(
            "LD X20\nETHCONF K1 D0 K0 H{port:X} M0 M1 M2 K{cap} K10 M100 M101 D2000\nEND"
        ))
        .unwrap();
        let mut s = sim(tcp);
        s.memory.set_word(d("D0"), 0);
        s.memory.set_word(d("D1"), 0);
        s.memory.set_bit(d("M1"), true);
        s.memory.set_input(d("X20"), true);
        scan(&mut s, &p);
        assert!(!s.memory.bit(d("M100")));
        assert!(s.memory.bit(d("M101")));
        assert_eq!(s.memory.word(d("D2000")), code);
    }
}
#[test]
fn udp_server_needs_received_peer_and_replies_to_that_peer() {
    let p = compile_il(&format!(
        "LD X20\nETHCONF K1 D0 K0 K1024 M0 M1 M2 K8 K10 M100 M101 D2000\n{MSG}"
    ))
    .unwrap();
    let mut s = sim(false);
    s.memory.set_word(d("D0"), 0);
    s.memory.set_word(d("D1"), 0);
    s.memory.set_bit(d("M1"), true);
    configured(&mut s, &p);
    scan(&mut s, &p);
    let session = request(&s);
    event(&mut s, &session, ConfigEvent::Opened { peer: None });
    assert_eq!(s.memory.word(d("D200")), 1);
    s.memory.set_bit(d("M11"), true);
    scan(&mut s, &p);
    assert_eq!(s.memory.word(d("D2002")), 11);
    assert!(!s.memory.bit(d("M102")));
    s.memory.set_bit(d("M11"), false);
    scan(&mut s, &p);
    let sender = Endpoint {
        ip: [10, 20, 30, 40],
        port: 32100,
    };
    event(
        &mut s,
        &session,
        ConfigEvent::Packet {
            peer: sender,
            bytes: vec![0x77],
        },
    );
    s.memory.set_bit(d("M11"), true);
    scan(&mut s, &p);
    let tx = request(&s);
    match &tx.kind {
        ConfigRequestKind::Send { peer, bytes, .. } => {
            assert_eq!(*peer, sender);
            assert_eq!(bytes, &vec![0x41, 0x42, 0x43]);
        }
        _ => panic!("expected UDP reply"),
    }
    assert!(!s.memory.bit(d("M102")));
    event(&mut s, &tx, ConfigEvent::Sent);
    assert!(s.memory.bit(d("M102")));
}

#[test]
fn byte_length_clamps_and_4096_byte_message_uses_three_acknowledged_segments() {
    let p = compile_il(&format!(
        "{CONF}LD X21\nETHMSG K1 M10 D3000 D10 M11 M12 D200 M102 M103 D300 D210 M104 D2002\nEND"
    ))
    .unwrap();
    let mut s = sim(true);
    for n in 3000..5048 {
        s.memory.set_word(d(&format!("D{n}")), 0x4142);
    }
    opened(&mut s, &p);
    s.memory.set_word(d("D10"), 0);
    s.memory.set_bit(d("M11"), true);
    scan(&mut s, &p);
    let one = request(&s);
    match &one.kind {
        ConfigRequestKind::Send { bytes, .. } => assert_eq!(bytes, &vec![0x42]),
        _ => panic!("expected minimum send"),
    };
    event(&mut s, &one, ConfigEvent::Sent);
    s.memory.set_bit(d("M11"), false);
    scan(&mut s, &p);
    s.memory.set_word(d("D10"), 4097);
    s.memory.set_bit(d("M11"), true);
    scan(&mut s, &p);
    for (index, length) in [(1, 1460), (2, 1460), (3, 1176)] {
        let r = request(&s);
        match &r.kind {
            ConfigRequestKind::Send {
                bytes,
                segment_index,
                segment_count,
                ..
            } => {
                assert_eq!((*segment_index, *segment_count), (index, 3));
                assert_eq!(bytes.len(), length);
                assert!(
                    bytes
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .all(|pair| *pair == [0x42, 0x41])
                );
            }
            _ => panic!("expected segmented send"),
        };
        assert!(!s.memory.bit(d("M102")));
        event(&mut s, &r, ConfigEvent::Sent);
    }
    assert!(s.memory.bit(d("M102")));
}
