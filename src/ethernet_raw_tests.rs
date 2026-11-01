use super::*;
#[test]
fn raw_and_se2_message_sockets_share_virtual_local_port_ownership() {
    use crate::ethernet_config::{
        EthernetConfigAction, EthernetConfigProfile, EthernetConfigState,
    };
    let conf = EthernetConfigAction::parse(
        "ETHCONF",
        &[
            "K1", "D20", "K0", "K1024", "M0", "M1", "M2", "K8", "K10", "M20", "M21", "D2000",
        ]
        .map(str::to_owned),
    )
    .unwrap();
    let msg = EthernetConfigAction::parse(
        "ETHMSG",
        &[
            "K1", "M10", "D100", "K1", "M11", "M12", "D200", "M22", "M23", "D300", "D210", "M24",
            "D2002",
        ]
        .map(str::to_owned),
    )
    .unwrap();
    for config_first in [false, true] {
        let (mut m, raw) = memory(3, true);
        m.ethernet_config.profile = EthernetConfigProfile::Se2;
        for name in ["M0", "M1", "M10"] {
            m.set_internal(device(name), true);
        }
        if config_first {
            m.ethernet_raw.reset_instruction_state();
        }
        conf.execute(&mut m, true, 50);
        msg.execute(&mut m, true, 51);
        if config_first {
            assert_eq!(m.ethernet_config.socket(1).unwrap().status, 1);
            raw.execute(&mut m, true, 1);
            assert!(m.ethernet_raw.outgoing().is_empty());
            assert!(
                m.execution_limitations
                    .iter()
                    .any(|e| e.contains("yerel portu"))
            );
            EthernetConfigState::abort(&mut m);
            raw.execute(&mut m, true, 1);
            assert!(matches!(
                m.ethernet_raw.outgoing().back().unwrap().kind,
                RawRequestKind::Open { .. }
            ));
        } else {
            assert!(m.ethernet_config.outgoing().is_empty());
            assert_eq!(m.word(d(2002)), 16);
            assert!(m.ethernet_raw.reserved_ports().contains(&(true, 1024)));
        }
    }
}
fn device(s: &str) -> Device {
    s.parse().unwrap()
}
fn peer(last: u8) -> Endpoint {
    Endpoint {
        ip: [192, 168, 1, last],
        port: 9000,
    }
}
fn memory(mode: i16, tcp: bool) -> (Memory, RawEthernetAction) {
    let mut m = Memory::default();
    m.ethernet_raw.profile = RawEthernetProfile {
        model: RawEthernetModel::Se12,
        firmware: FirmwareVersion::new(1, 95),
        ..Default::default()
    };
    m.set_system(1196, true);
    for (index, value) in [
        (0, 1024),
        (1, if mode < 2 { 0x0102 } else { 0 }),
        (2, if mode < 2 { 0xc0a8u16 as i16 } else { 0 }),
        (3, if mode < 2 { 9000 } else { 0 }),
        (4, i16::from(tcp)),
        (5, mode),
        (6, 100),
        (100, 0x41),
        (200, 777),
        (201, 0xab44u16 as i16),
    ] {
        m.set_word(d(index), value);
    }
    let a = RawEthernetAction::parse(
        "ETHRS",
        &["D0", "D5", "D100", "K1", "D200", "M100"].map(str::to_owned),
    )
    .unwrap();
    a.execute(&mut m, true, 1);
    let request = m.ethernet_raw.outgoing().back().unwrap().clone();
    m.ethernet_raw
        .queue_event(
            1,
            request.id,
            request.epoch,
            RawEvent::Opened {
                peer: Some(peer(2)),
            },
        )
        .unwrap();
    RawEthernetState::advance(&mut m, 1);
    (m, a)
}
fn queue_current(m: &mut Memory, at: u64, event: RawEvent) {
    let request = m.ethernet_raw.outgoing().back().unwrap().clone();
    m.ethernet_raw
        .queue_event(at, request.id, request.epoch, event)
        .unwrap();
}
#[test]
fn link_loss_invalidates_queued_packet_and_send_ack_before_commit() {
    for mode in [0, 3] {
        let (mut m, _) = memory(mode, true);
        queue_current(
            &mut m,
            2,
            if mode == 0 {
                RawEvent::Sent
            } else {
                RawEvent::Packet {
                    peer: peer(2),
                    bytes: vec![0x99],
                }
            },
        );
        m.ethernet_raw.link_up = false;
        RawEthernetState::advance(&mut m, 2);
        assert_eq!(m.word(d(200)), 777);
        assert_eq!(m.word(d(201)) as u16, 0xab44);
        assert!(!m.bit(device("M100")));
        assert!(m.system_bit(1198));
        assert_eq!(m.word(d(1176)) as u16, 0x600d);
        assert!(matches!(
            m.ethernet_raw.outgoing().back().unwrap().kind,
            RawRequestKind::Close
        ));
        assert!(!m.system_bit(1197));
    }
}
#[test]
fn m1196_falling_edge_invalidates_queued_tcp_receive_and_send_completion() {
    for mode in [0, 3] {
        let (mut m, _) = memory(mode, true);
        queue_current(
            &mut m,
            2,
            if mode == 0 {
                RawEvent::Sent
            } else {
                RawEvent::Packet {
                    peer: peer(2),
                    bytes: vec![0x99],
                }
            },
        );
        m.set_system(1196, false);
        RawEthernetState::advance(&mut m, 2);
        assert_eq!(m.word(d(200)), 777);
        assert_eq!(m.word(d(201)) as u16, 0xab44);
        assert!(!m.bit(device("M100")));
        assert!(!m.system_bit(1198));
        assert!(!m.system_bit(1197));
        assert!(matches!(
            m.ethernet_raw.outgoing().back().unwrap().kind,
            RawRequestKind::Close
        ));
    }
}
#[test]
fn wildcard_tcp_keeps_established_peer_but_udp_accepts_a_new_sender() {
    for tcp in [true, false] {
        let (mut m, _) = memory(3, tcp);
        queue_current(
            &mut m,
            2,
            RawEvent::Packet {
                peer: peer(3),
                bytes: vec![0x99],
            },
        );
        RawEthernetState::advance(&mut m, 2);
        if tcp {
            assert_eq!(m.word(d(200)), 777);
            assert_eq!(m.word(d(201)) as u16, 0xab44);
            assert!(!m.bit(device("M100")));
            assert_eq!(
                m.ethernet_raw.connection.as_ref().unwrap().peer,
                Some(peer(2))
            );
            // The correct peer still owns the same pending Receive phase.
            queue_current(
                &mut m,
                3,
                RawEvent::Packet {
                    peer: peer(2),
                    bytes: vec![0x55],
                },
            );
            RawEthernetState::advance(&mut m, 3);
            assert_eq!(m.word(d(201)) as u16, 0xab55);
        } else {
            assert_eq!(m.word(d(201)) as u16, 0xab99);
            assert_eq!(
                m.ethernet_raw.connection.as_ref().unwrap().peer,
                Some(peer(3))
            );
            assert_eq!(m.word(d(1227)) as u16, 0x0103);
        }
        assert_eq!(m.word(d(200)), 1);
        assert!(m.bit(device("M100")));
        assert!(!m.system_bit(1198));
    }
}
