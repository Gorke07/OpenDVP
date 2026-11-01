use super::*;
fn dev(s: &str) -> Device {
    s.parse().unwrap()
}
fn action(s: &str) -> EthernetConfigAction {
    let mut p = s.split_whitespace();
    EthernetConfigAction::parse(p.next().unwrap(), &p.map(str::to_owned).collect::<Vec<_>>())
        .unwrap()
}
fn peer() -> Endpoint {
    Endpoint {
        ip: [192, 168, 1, 100],
        port: 3000,
    }
}
fn setup(tcp: bool, server: bool, rx: u16) -> (Memory, EthernetConfigAction) {
    let mut m = Memory::default();
    m.ethernet_config.profile = EthernetConfigProfile::Se2;
    m.set_word(dev("D0"), if server { 0 } else { 0xc0a8u16 as i16 });
    m.set_word(dev("D1"), if server { 0 } else { 0x0164 });
    m.set_internal(dev("M0"), tcp);
    m.set_internal(dev("M1"), server);
    let conf = action(&format!(
        "ETHCONF K1 D0 K{} K{} M0 M1 M2 K{rx} K5 M10 M11 D10",
        if server { 0 } else { 3000 },
        if server { 2000 } else { 0 }
    ));
    conf.execute(&mut m, true, 1);
    assert!(m.bit(dev("M10")));
    (
        m,
        action("ETHMSG K1 M20 D100 K3 M21 M22 D20 M23 M24 D200 D21 M25 D22"),
    )
}
fn open(m: &mut Memory, a: &EthernetConfigAction) -> ConfigRequest {
    m.set_internal(dev("M20"), true);
    a.execute(m, true, 2);
    m.ethernet_config.outgoing().back().unwrap().clone()
}
fn event(m: &mut Memory, r: &ConfigRequest, event: ConfigEvent, at: u64) {
    m.ethernet_config
        .queue_event(at, r.id, r.epoch, event)
        .unwrap();
    EthernetConfigState::advance(m, at);
}
#[test]
fn literal_headers_matrices_and_lengths() {
    let (m, a) = setup(true, false, 10);
    assert_eq!(a.header(), Some(0x1d02));
    assert_eq!(a.words(), 27);
    assert_eq!(a.args.len(), 13);
    let c = action("ETHCONF K1 D0 HBB8 K0 M0 M1 M2 K10 K5 M10 M11 D10");
    assert_eq!(c.header(), Some(0xe600));
    assert_eq!(c.words(), 25);
    assert!(c.validate(&m).is_ok());
    assert!(EthernetConfigAction::parse("ETHMSGP", &[]).is_err());
    assert!(
        EthernetConfigAction::parse(
            "ETHCONF",
            &[
                "K1", "D0", "K3000", "K0", "Y0", "M1", "M2", "K10", "K5", "M10", "M11", "D10"
            ]
            .map(str::to_owned)
        )
        .is_err()
    );
}
#[test]
fn tcp_needs_open_event_and_full_ack_for_segmented_send() {
    let (mut m, a) = setup(true, false, 10);
    let r = open(&mut m, &a);
    assert_eq!(m.word(dev("D20")), 1);
    assert!(!m.bit(dev("M23")));
    event(&mut m, &r, ConfigEvent::Opened { peer: Some(peer()) }, 1);
    m.set_word(dev("D100"), 0x4241);
    m.set_word(dev("D101"), 0x4443);
    m.set_internal(dev("M21"), true);
    a.execute(&mut m, true, 2);
    let tx = m.ethernet_config.outgoing().back().unwrap().clone();
    let ConfigRequestKind::Send { bytes, .. } = &tx.kind else {
        panic!("send")
    };
    assert_eq!(bytes, &[65, 66, 67]);
    assert!(!m.bit(dev("M23")));
    event(&mut m, &tx, ConfigEvent::Sent, 2);
    assert!(m.bit(dev("M23")));
    a.execute(&mut m, true, 2);
    assert_eq!(m.ethernet_config.outgoing().len(), 2);
}
#[test]
fn receive_odd_append_and_overflow_are_atomic() {
    let (mut m, a) = setup(true, false, 4);
    let r = open(&mut m, &a);
    event(&mut m, &r, ConfigEvent::Opened { peer: Some(peer()) }, 1);
    m.set_word(dev("D200"), 0x7f00);
    event(
        &mut m,
        &r,
        ConfigEvent::Packet {
            peer: peer(),
            bytes: vec![1],
        },
        2,
    );
    assert_eq!(m.word(dev("D200")), 0x7f01);
    event(
        &mut m,
        &r,
        ConfigEvent::Packet {
            peer: peer(),
            bytes: vec![2, 3],
        },
        3,
    );
    assert_eq!(m.word(dev("D200")), 0x0201);
    assert_eq!(m.word(dev("D201")), 3);
    assert_eq!(m.word(dev("D21")), 3);
    event(
        &mut m,
        &r,
        ConfigEvent::Packet {
            peer: peer(),
            bytes: vec![4, 5],
        },
        4,
    );
    assert_eq!(m.word(dev("D200")), 0x0201);
    assert_eq!(m.word(dev("D21")), 3);
    assert_eq!(m.word(dev("D22")), 14);
    m.set_internal(dev("M22"), true);
    a.execute(&mut m, true, 2);
    assert_eq!(m.word(dev("D21")), 0);
    assert!(!m.bit(dev("M24")));
}
#[test]
fn udp_server_learns_peer_but_tcp_server_locks_it() {
    for tcp in [false, true] {
        let (mut m, a) = setup(tcp, true, 10);
        let r = open(&mut m, &a);
        event(
            &mut m,
            &r,
            ConfigEvent::Opened {
                peer: tcp.then_some(peer()),
            },
            1,
        );
        let other = Endpoint {
            ip: [10, 0, 0, 2],
            port: 4000,
        };
        event(
            &mut m,
            &r,
            ConfigEvent::Packet {
                peer: other,
                bytes: vec![99],
            },
            2,
        );
        assert_eq!(m.word(dev("D21")), if tcp { 0 } else { 1 });
        assert_eq!(
            m.ethernet_config.socket(1).unwrap().peer,
            Some(if tcp { peer() } else { other })
        );
    }
}
#[test]
fn controls_precede_packets_and_restart_invalidates_event_epochs() {
    let (mut m, a) = setup(true, false, 10);
    let r = open(&mut m, &a);
    event(&mut m, &r, ConfigEvent::Opened { peer: Some(peer()) }, 1);
    m.ethernet_config
        .queue_event(
            2,
            r.id,
            r.epoch,
            ConfigEvent::Packet {
                peer: peer(),
                bytes: vec![99],
            },
        )
        .unwrap();
    m.ethernet_config.link_up = false;
    EthernetConfigState::advance(&mut m, 2);
    assert_eq!(m.word(dev("D200")), 0);
    assert_eq!(m.word(dev("D22")), 10);
    m.ethernet_config.reset_instruction_state();
    assert!(m.ethernet_config.socket(1).is_some());
    m.ethernet_config.link_up = true;
    let new = open(&mut m, &a);
    event(&mut m, &r, ConfigEvent::Opened { peer: Some(peer()) }, 1);
    assert_eq!(m.word(dev("D20")), 1);
    assert_ne!(r.epoch, new.epoch);
}
#[test]
fn keepalive_is_a_request_not_a_synthetic_peer_reply() {
    let (mut m, a) = setup(true, false, 10);
    m.set_internal(dev("M2"), true);
    let conf = action("ETHCONF K1 D0 K3000 K0 M0 M1 M2 K10 K5 M10 M11 D10");
    conf.execute(&mut m, true, 9);
    let r = open(&mut m, &a);
    event(&mut m, &r, ConfigEvent::Opened { peer: Some(peer()) }, 1);
    EthernetConfigState::advance(&mut m, 2_500_001);
    assert!(matches!(
        m.ethernet_config.outgoing().back().unwrap().kind,
        ConfigRequestKind::KeepAlive { .. }
    ));
    EthernetConfigState::advance(&mut m, 5_000_001);
    assert_eq!(m.word(dev("D20")), 0);
    EthernetConfigState::advance(&mut m, 10_000_001);
    assert!(matches!(
        m.ethernet_config.outgoing().back().unwrap().kind,
        ConfigRequestKind::Open { .. }
    ));
}
#[test]
fn full_capacity_boundary_is_checked_before_connect_output_alias_mutation() {
    let (mut m, a) = setup(true, false, 4096);
    m.data_register_limit = Some(210);
    m.set_word(dev("D20"), 123);
    m.set_internal(dev("M20"), true);
    a.execute(&mut m, true, 2);
    assert_eq!(m.word(dev("D20")), 123);
    assert!(m.ethernet_config.outgoing().is_empty());
    assert!(!m.execution_limitations.is_empty());
}
#[test]
fn udp_requires_real_bind_ack_and_client_transmit_before_receive() {
    let (mut m, a) = setup(false, false, 10);
    let r = open(&mut m, &a);
    m.set_internal(dev("M21"), true);
    a.execute(&mut m, true, 2);
    assert_eq!(m.word(dev("D22")), 11);
    event(&mut m, &r, ConfigEvent::Opened { peer: None }, 1);
    event(
        &mut m,
        &r,
        ConfigEvent::Packet {
            peer: peer(),
            bytes: vec![99],
        },
        2,
    );
    assert_eq!(m.word(dev("D21")), 0);
    m.set_internal(dev("M21"), false);
    a.execute(&mut m, true, 2);
    m.set_internal(dev("M21"), true);
    a.execute(&mut m, true, 2);
    let tx = m.ethernet_config.outgoing().back().unwrap().clone();
    event(&mut m, &tx, ConfigEvent::Sent, 3);
    event(
        &mut m,
        &r,
        ConfigEvent::Packet {
            peer: peer(),
            bytes: vec![99],
        },
        4,
    );
    assert_eq!(m.word(dev("D21")), 1);
}

#[test]
fn tcp_server_relistens_immediately_but_client_retries_after_five_seconds() {
    for server in [false, true] {
        let (mut m, a) = setup(true, server, 10);
        let r = open(&mut m, &a);
        event(&mut m, &r, ConfigEvent::Opened { peer: Some(peer()) }, 1);
        event(&mut m, &r, ConfigEvent::Closed, 2);
        assert_eq!(m.word(dev("D20")), if server { 1 } else { 0 });
        if server {
            assert!(matches!(
                m.ethernet_config.outgoing().back().unwrap().kind,
                ConfigRequestKind::Open { .. }
            ));
            assert_ne!(m.ethernet_config.outgoing().back().unwrap().id, r.id);
        } else {
            assert!(matches!(
                m.ethernet_config.outgoing().back().unwrap().kind,
                ConfigRequestKind::Close
            ));
            EthernetConfigState::advance(&mut m, 5_000_001);
            assert_eq!(m.word(dev("D20")), 0);
            EthernetConfigState::advance(&mut m, 5_000_002);
            assert_eq!(m.word(dev("D20")), 1);
        }
    }
}
#[test]
fn tcp_idle_timeout_still_applies_when_keepalive_probes_are_disabled() {
    let (mut m, a) = setup(true, false, 10);
    assert!(!m.ethernet_config.socket(1).unwrap().config.keep_alive);
    let r = open(&mut m, &a);
    event(&mut m, &r, ConfigEvent::Opened { peer: Some(peer()) }, 1);
    EthernetConfigState::advance(&mut m, 2_500_001);
    assert!(
        !m.ethernet_config
            .outgoing()
            .iter()
            .any(|r| matches!(r.kind, ConfigRequestKind::KeepAlive { .. }))
    );
    EthernetConfigState::advance(&mut m, 5_000_001);
    assert_eq!(m.word(dev("D20")), 0);
}

#[test]
fn tcp_client_recovers_after_link_returns_without_connect_edge() {
    // PDF 550 (3-412), explanation 2: Connect ON keeps retrying every 5 s.
    let (mut m, a) = setup(true, false, 10);
    let first = open(&mut m, &a);
    event(
        &mut m,
        &first,
        ConfigEvent::Opened { peer: Some(peer()) },
        1,
    );
    m.ethernet_config.link_up = false;
    EthernetConfigState::service_controls(&mut m);
    assert_eq!(m.word(dev("D20")), 0);
    assert_eq!(m.word(dev("D22")), 10);
    assert!(matches!(
        m.ethernet_config.outgoing().back().unwrap().kind,
        ConfigRequestKind::Close
    ));

    EthernetConfigState::advance(&mut m, 1_000_001);
    m.ethernet_config.link_up = true;
    a.execute(&mut m, true, 2);
    let before_retry = m.ethernet_config.outgoing().len();
    EthernetConfigState::advance(&mut m, 5_000_000);
    assert_eq!(m.ethernet_config.outgoing().len(), before_retry);
    EthernetConfigState::advance(&mut m, 5_000_001);
    let retry = m.ethernet_config.outgoing().back().unwrap().clone();
    assert!(matches!(retry.kind, ConfigRequestKind::Open { .. }));
    assert_ne!(retry.id, first.id);
    assert_eq!(m.word(dev("D20")), 1);
    // The retired connection cannot cancel its replacement.
    event(&mut m, &first, ConfigEvent::Closed, 5_000_002);
    assert_eq!(m.word(dev("D20")), 1);
    event(
        &mut m,
        &retry,
        ConfigEvent::Opened { peer: Some(peer()) },
        5_000_003,
    );
    assert_eq!(m.word(dev("D20")), 2);
}

#[test]
fn tcp_client_link_down_retries_keep_deadlines_and_stop_on_connect_off() {
    let (mut m, a) = setup(true, false, 10);
    m.ethernet_config.link_up = false;
    m.set_internal(dev("M20"), true);
    a.execute(&mut m, true, 2);
    assert!(m.ethernet_config.outgoing().is_empty());
    assert_eq!(m.ethernet_config.next_deadline_us(), Some(5_000_000));
    EthernetConfigState::advance(&mut m, 12_000_000);
    assert_eq!(m.ethernet_config.next_deadline_us(), Some(15_000_000));
    assert!(m.execution_limitations.is_empty());
    // Scans must not postpone the retry indefinitely while the cable is absent.
    a.execute(&mut m, true, 2);
    assert_eq!(m.ethernet_config.next_deadline_us(), Some(15_000_000));
    m.set_internal(dev("M20"), false);
    EthernetConfigState::service_controls(&mut m);
    assert_eq!(m.ethernet_config.next_deadline_us(), None);
    m.ethernet_config.link_up = true;
    EthernetConfigState::advance(&mut m, 25_000_000);
    assert!(m.ethernet_config.outgoing().is_empty());
    assert_eq!(m.word(dev("D20")), 0);
    m.set_internal(dev("M20"), true);
    a.execute(&mut m, true, 2);
    assert!(matches!(
        m.ethernet_config.outgoing().back().unwrap().kind,
        ConfigRequestKind::Open { .. }
    ));
}

#[test]
fn tcp_server_relistens_after_link_returns_without_connect_edge() {
    // PDF 550 (3-412), explanation 2: a server returns to listening.
    let (mut m, a) = setup(true, true, 10);
    let first = open(&mut m, &a);
    event(
        &mut m,
        &first,
        ConfigEvent::Opened { peer: Some(peer()) },
        1,
    );
    m.ethernet_config.link_up = false;
    EthernetConfigState::service_controls(&mut m);
    let closed_count = m.ethernet_config.outgoing().len();
    a.execute(&mut m, true, 2);
    assert_eq!(m.ethernet_config.outgoing().len(), closed_count);
    m.ethernet_config.link_up = true;
    EthernetConfigState::service_controls(&mut m);
    assert_eq!(m.word(dev("D20")), 1);
    assert!(matches!(
        m.ethernet_config.outgoing().back().unwrap().kind,
        ConfigRequestKind::Open { .. }
    ));
    m.set_internal(dev("M20"), false);
    EthernetConfigState::service_controls(&mut m);
    let cancelled_count = m.ethernet_config.outgoing().len();
    EthernetConfigState::advance(&mut m, 10_000_001);
    assert_eq!(m.word(dev("D20")), 0);
    assert_eq!(m.ethernet_config.outgoing().len(), cancelled_count);
}

#[test]
fn tcp_server_reports_remote_close_while_send_is_pending() {
    // PDF 552 (3-414), code 15: the remote closes TCP while data is sent.
    // Returning to listen must not erase that error before outputs are published.
    let (mut m, a) = setup(true, true, 10);
    let first = open(&mut m, &a);
    event(
        &mut m,
        &first,
        ConfigEvent::Opened { peer: Some(peer()) },
        1,
    );
    m.set_internal(dev("M21"), true);
    a.execute(&mut m, true, 2);
    assert!(matches!(
        m.ethernet_config.outgoing().back().unwrap().kind,
        ConfigRequestKind::Send { .. }
    ));
    event(&mut m, &first, ConfigEvent::Closed, 2);
    assert_eq!(m.word(dev("D20")), 1);
    assert!(!m.bit(dev("M23")));
    assert!(m.bit(dev("M25")));
    assert_eq!(m.word(dev("D22")), 15);

    // PDF 550 (3-412), explanation 2 explicitly clears outputs on Connect's edge.
    m.set_internal(dev("M20"), false);
    a.execute(&mut m, true, 2);
    m.set_internal(dev("M20"), true);
    a.execute(&mut m, true, 2);
    assert!(!m.bit(dev("M25")));
    assert_eq!(m.word(dev("D22")), 0);
}

#[test]
fn read_only_may_be_a_bool_source_but_not_a_status_output() {
    let args = [
        "K1", "D0", "K3000", "K0", "M1000", "M1001", "M1000", "K10", "K5", "M10", "M11", "D10",
    ]
    .map(str::to_owned);
    assert!(EthernetConfigAction::parse("ETHCONF", &args).is_ok());
    let mut invalid = args.clone();
    invalid[9] = "M1000".into();
    assert!(EthernetConfigAction::parse("ETHCONF", &invalid).is_err());
}
