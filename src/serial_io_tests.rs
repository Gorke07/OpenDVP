use super::*;
fn action(name: &str, args: &[&str]) -> SerialIoAction {
    SerialIoAction::parse(
        name,
        &args.iter().map(|v| v.to_string()).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn memory(port: SerialPort) -> Memory {
    let mut m = Memory::default();
    let profile = SerialProfile {
        has_com3: true,
        ..SerialProfile::default()
    };
    m.serial_io.set_profile(profile).unwrap();
    m.serial_io
        .configure(
            port,
            if port == SerialPort::Com1 {
                PortConfig::rs232()
            } else {
                PortConfig::rs485()
            },
        )
        .unwrap();
    m.set_word(d(flags(port).protocol), 0x87);
    m.set_word(d(flags(port).timeout), 100);
    m
}
fn start(m: &mut Memory, a: &SerialIoAction, port: SerialPort) {
    m.set_system(flags(port).request, true);
    a.execute(m, true, 10);
}
fn advance(m: &mut Memory, at: u64) {
    m.timing_io.now_us = at;
    SerialBus::advance(m, at);
}
fn ready(m: &mut Memory, port: SerialPort) -> u64 {
    let t = m.serial_io.ports[&port].active.as_ref().unwrap();
    let at = t.rx_at;
    advance(m, at);
    at
}
fn rx(m: &mut Memory, port: SerialPort, at: u64, bytes: &[u8]) {
    m.serial_io
        .queue_received(port, at, bytes.to_vec())
        .unwrap();
    advance(m, at);
    // Most fixtures describe one entire response, including its RTU EOF.
    // Boundary-specific regressions below deliberately queue/advance directly.
    if let Some(t) = m.serial_io.ports.get(&port).and_then(|p| p.active.as_ref())
        && matches!(
            t.plan,
            ReceivePlan::Modbus {
                mode: WireMode::Rtu | WireMode::RtuSwappedCrc,
                ..
            }
        )
    {
        let eof = at + t.frame_gap_us;
        advance(m, eof);
    }
}
fn clean(m: &Memory) {
    assert!(
        m.execution_limitations.is_empty(),
        "{:?}",
        m.execution_limitations
    );
}
fn modrw() -> SerialIoAction {
    action("MODRW", &["K1", "K3", "H2102", "D100", "K2"])
}

#[test]
fn four_actual_headers_and_operand_matrices() {
    for (name, args, h, w) in [
        ("RS", vec!["D0", "K1", "D100", "K1"], 0xb000, 9),
        ("MODRD", vec!["K1", "H2102", "K2"], 0xc200, 7),
        ("MODWR", vec!["K1", "H2000", "H12"], 0xc400, 7),
        ("MODRW", vec!["K1", "K3", "H2102", "D100", "K2"], 0x4c01, 11),
    ] {
        let a = action(name, &args);
        assert_eq!(a.header(), h);
        assert_eq!(a.words(), w);
    }
    for (name, args) in [
        ("RSP", vec!["D0", "K1", "D100", "K1"]),
        ("RS", vec!["K1", "K1", "D100", "K1"]),
        ("MODRD", vec!["E0", "K0", "K1"]),
        ("MODRW", vec!["K1", "K3", "K0", "K4M0", "K1"]),
    ] {
        assert!(
            SerialIoAction::parse(
                name,
                &args.iter().map(|v| v.to_string()).collect::<Vec<_>>()
            )
            .is_err()
        );
    }
}
#[test]
fn unconfigured_serial_request_does_not_echo_or_claim_success() {
    let mut m = Memory::default();
    m.set_system(1122, true);
    action("MODRD", &["K1", "K0", "K2"]).execute(&mut m, true, 10);
    assert!(m.serial_io.outgoing_frames().is_empty());
    assert!(!m.system_bit(1127));
    assert!(!m.execution_limitations.is_empty());
}
#[test]
fn rs_word_packing_is_low_high_and_odd_receive_preserves_last_high_byte() {
    let mut m = memory(SerialPort::Com2);
    m.set_word(d(0), 0x4241);
    m.set_word(d(1), 0x4443);
    m.set_word(d(100), 0xaa55u16 as i16);
    m.set_word(d(101), 0xcc66u16 as i16);
    start(
        &mut m,
        &action("RS", &["D0", "K3", "D100", "K3"]),
        SerialPort::Com2,
    );
    assert_eq!(m.serial_io.outgoing_frames()[0].bytes, b"ABC");
    let at = ready(&mut m, SerialPort::Com2);
    rx(&mut m, SerialPort::Com2, at + 1, &[0x11, 0x22, 0x33]);
    assert_eq!(m.word(d(100)) as u16, 0x2211);
    assert_eq!(m.word(d(101)) as u16, 0xcc33);
    assert!(m.system_bit(1123));
    assert!(!m.system_bit(1127));
    clean(&m);
}
#[test]
fn rs_low_byte_mode_uses_one_d_word_per_byte_and_snapshots_source() {
    let mut m = memory(SerialPort::Com2);
    m.set_system(1161, true);
    m.set_word(d(0), 0xaa41u16 as i16);
    m.set_word(d(1), 0xbb42u16 as i16);
    start(
        &mut m,
        &action("RS", &["D0", "K2", "D100", "K2"]),
        SerialPort::Com2,
    );
    m.set_word(d(0), 0x77);
    assert_eq!(m.serial_io.outgoing_frames()[0].bytes, b"AB");
    let at = ready(&mut m, SerialPort::Com2);
    rx(&mut m, SerialPort::Com2, at + 1, b"CD");
    assert_eq!(m.word(d(100)), 0x43);
    assert_eq!(m.word(d(101)), 0x44);
    clean(&m);
}
#[test]
fn com1_and_com3_force_low_byte_rs_and_ignore_com2_framing() {
    for port in [SerialPort::Com1, SerialPort::Com3] {
        let mut m = memory(port);
        m.set_word(d(0), 0x4241);
        m.set_word(d(1), 0x4443);
        m.set_word(d(1120), 0x787);
        m.set_system(1126, true);
        m.set_system(1130, true);
        start(&mut m, &action("RS", &["D0", "K2", "D100", "K1"]), port);
        assert_eq!(m.serial_io.outgoing_frames()[0].bytes, b"AC");
        let at = ready(&mut m, port);
        rx(&mut m, port, at + 1, b"Z");
        assert!(m.system_bit(flags(port).done));
        assert_eq!(m.word(d(100)), 90);
        clean(&m);
    }
}
#[test]
fn rs_auto_framing_is_excluded_from_m_but_included_in_rx_n() {
    let mut m = memory(SerialPort::Com2);
    m.set_word(d(1120), 0x787);
    m.set_system(1126, true);
    m.set_system(1130, true);
    m.set_system(1161, true);
    m.set_word(d(0), b'A' as i16);
    start(
        &mut m,
        &action("RS", &["D0", "K1", "D100", "K4"]),
        SerialPort::Com2,
    );
    assert_eq!(m.serial_io.outgoing_frames()[0].bytes, b":A\r\n");
    let at = ready(&mut m, SerialPort::Com2);
    rx(&mut m, SerialPort::Com2, at + 1, b":B\r\n");
    assert_eq!(m.word(d(100)), 58);
    assert_eq!(m.word(d(103)), 10);
    clean(&m);
}
#[test]
fn exact_tx_duration_and_one_millisecond_turnaround_are_explicit() {
    let mut m = memory(SerialPort::Com2);
    m.set_system(1161, true);
    start(
        &mut m,
        &action("RS", &["D0", "K2", "D100", "K1"]),
        SerialPort::Com2,
    );
    assert_eq!(m.serial_io.outgoing_frames()[0].completed_at_us, 2292);
    advance(&mut m, 2291);
    assert!(m.system_bit(1122));
    assert!(!m.system_bit(1121));
    advance(&mut m, 2292);
    assert!(!m.system_bit(1122));
    assert!(m.system_bit(1121));
    assert!(!m.system_bit(1124));
    advance(&mut m, 3292);
    assert!(m.system_bit(1124));
    clean(&m);
}
#[test]
fn no_rx_peer_yields_timeout_not_fabricated_success() {
    let mut m = memory(SerialPort::Com2);
    m.set_word(d(1050), 777);
    start(
        &mut m,
        &action("MODRD", &["K1", "H2102", "K2"]),
        SerialPort::Com2,
    );
    let at = ready(&mut m, SerialPort::Com2);
    advance(&mut m, at + 100_000);
    assert!(m.system_bit(1129));
    assert!(!m.system_bit(1127));
    assert_eq!(m.word(d(1130)), 1);
    assert_eq!(m.word(d(1050)), 777);
    assert_eq!(m.serial_io.active_count(), 0);
    clean(&m);
}
#[test]
fn rs_receive_only_silence_timer_starts_only_after_first_byte_and_auto_clears_mode() {
    let mut m = memory(SerialPort::Com2);
    m.set_system(1263, true);
    m.set_system(1161, true);
    m.set_word(d(1168), 5);
    m.set_word(d(1129), 0);
    start(
        &mut m,
        &action("RS", &["D0", "K0", "D100", "K20"]),
        SerialPort::Com2,
    );
    advance(&mut m, 1_000_000);
    assert!(!m.system_bit(1123));
    rx(&mut m, SerialPort::Com2, 1_000_001, b"AB");
    advance(&mut m, 1_005_001);
    assert!(!m.system_bit(1123));
    advance(&mut m, 1_005_002);
    assert!(m.system_bit(1123));
    assert!(!m.system_bit(1263));
    assert_eq!(m.word(d(1123)), 2);
    assert_eq!(m.word(d(100)), 65);
    clean(&m);
}
#[test]
fn source_and_complete_destination_bounds_are_checked_before_tx_mutation() {
    let mut m = memory(SerialPort::Com2);
    m.data_register_limit = Some(4999);
    m.set_system(1161, true);
    start(
        &mut m,
        &action("RS", &["D0", "K1", "D4999", "K2"]),
        SerialPort::Com2,
    );
    assert!(m.serial_io.outgoing_frames().is_empty());
    assert_eq!(m.serial_io.active_count(), 0);
    assert!(!m.execution_limitations.is_empty());
}
#[test]
fn modrd_ascii_matches_independent_manual_frame_and_fixed_converted_bank() {
    let mut m = memory(SerialPort::Com2);
    start(
        &mut m,
        &action("MODRD", &["K1", "H2102", "K2"]),
        SerialPort::Com2,
    );
    assert_eq!(
        m.serial_io.outgoing_frames()[0].bytes,
        b":010321020002D7\r\n"
    );
    assert_eq!(m.word(d(1089)) as u16, 0x3130);
    let at = ready(&mut m, SerialPort::Com2);
    rx(&mut m, SerialPort::Com2, at + 1, b":0103041770000071\r\n");
    assert_eq!(m.word(d(1050)), 0x1770);
    assert_eq!(m.word(d(1051)), 0);
    assert_eq!(m.word(d(1070)) as u16, 0x3130);
    assert!(m.system_bit(1127));
    assert!(!m.system_bit(1123));
    clean(&m);
}
#[test]
fn modrd_rtu_preserves_invalid_converted_bank_and_stores_raw_bytes() {
    let mut m = memory(SerialPort::Com2);
    m.set_system(1143, true);
    m.set_word(d(1050), 777);
    start(
        &mut m,
        &action("MODRD", &["K1", "H2102", "K2"]),
        SerialPort::Com2,
    );
    assert_eq!(
        m.serial_io.outgoing_frames()[0].bytes,
        [1, 3, 0x21, 2, 0, 2, 0x6f, 0xf7]
    );
    let at = ready(&mut m, SerialPort::Com2);
    rx(
        &mut m,
        SerialPort::Com2,
        at + 1,
        &[1, 3, 4, 0x17, 0x70, 0, 0, 0xfe, 0x5c],
    );
    assert_eq!(m.word(d(1050)), 777);
    assert_eq!(m.word(d(1073)), 0x17);
    assert!(m.system_bit(1127));
    clean(&m);
}
#[test]
fn invalid_crc_and_exception_hold_payload_and_raise_distinct_error_classes() {
    let mut m = memory(SerialPort::Com2);
    m.set_system(1143, true);
    m.set_word(d(1050), 777);
    let a = action("MODRD", &["K1", "H2102", "K2"]);
    start(&mut m, &a, SerialPort::Com2);
    let at = ready(&mut m, SerialPort::Com2);
    rx(
        &mut m,
        SerialPort::Com2,
        at + 1,
        &[1, 3, 4, 0x17, 0x70, 0, 0, 0, 0],
    );
    assert!(m.system_bit(1140));
    assert!(!m.system_bit(1127));
    assert_eq!(m.word(d(1050)), 777);
    start(&mut m, &a, SerialPort::Com2);
    let at = ready(&mut m, SerialPort::Com2);
    let bytes = serial_protocol::encode(
        WireMode::Rtu,
        &Frame {
            station: 1,
            function: 0x83,
            data: vec![2],
        },
    )
    .unwrap();
    rx(&mut m, SerialPort::Com2, at + 1, &bytes);
    assert!(m.system_bit(1141));
    assert_eq!(m.word(d(1130)), 2);
    assert_eq!(m.word(d(1050)), 777);
    clean(&m);
}
#[test]
fn com1_modrw_read_stores_converted_words_directly_not_raw_frame() {
    let mut m = memory(SerialPort::Com1);
    m.set_word(d(1296), 999);
    start(&mut m, &modrw(), SerialPort::Com1);
    let at = ready(&mut m, SerialPort::Com1);
    rx(&mut m, SerialPort::Com1, at + 1, b":0103041770000071\r\n");
    assert_eq!(m.word(d(100)), 0x1770);
    assert_eq!(m.word(d(101)), 0);
    assert_eq!(m.word(d(1296)), 999);
    assert!(m.system_bit(1314));
    clean(&m);
}
#[test]
fn modrw_port_priority_is_com1_then_com3_then_com2() {
    let mut m = memory(SerialPort::Com1);
    m.serial_io
        .configure(SerialPort::Com2, PortConfig::rs485())
        .unwrap();
    m.serial_io
        .configure(SerialPort::Com3, PortConfig::rs485())
        .unwrap();
    m.set_word(d(1120), 0x87);
    m.set_word(d(1109), 0x87);
    m.set_system(1122, true);
    m.set_system(1316, true);
    m.set_system(1312, true);
    modrw().execute(&mut m, true, 10);
    assert!(m.serial_io.transaction(SerialPort::Com1).is_some());
    assert!(m.serial_io.transaction(SerialPort::Com2).is_none());
    assert!(m.serial_io.transaction(SerialPort::Com3).is_none());
    clean(&m);
}
#[test]
fn com2_modrw_stores_raw_frame_and_separate_converted_data() {
    let mut m = memory(SerialPort::Com2);
    start(&mut m, &modrw(), SerialPort::Com2);
    let at = ready(&mut m, SerialPort::Com2);
    rx(&mut m, SerialPort::Com2, at + 1, b":0103041770000071\r\n");
    assert_eq!(m.word(d(100)) as u16, 0x3130);
    assert_eq!(m.word(d(1296)), 0x1770);
    assert_eq!(m.word(d(1297)), 0);
    clean(&m);
}
#[test]
fn h05_polarity_conflict_remains_two_explicit_profiles() {
    for (policy, expected) in [
        (CoilValuePolicy::ExamplesOneOn, 0xff00),
        (CoilValuePolicy::HeaderZeroOn, 0),
    ] {
        let mut m = memory(SerialPort::Com2);
        m.serial_io.coil_value_policy = policy;
        start(
            &mut m,
            &action("MODRW", &["K1", "K5", "H0500", "D0", "K1"]),
            SerialPort::Com2,
        );
        let frame =
            serial_protocol::decode(WireMode::Ascii, &m.serial_io.outgoing_frames()[0].bytes)
                .unwrap();
        assert_eq!(&frame.data[2..], &(expected as u16).to_be_bytes());
        clean(&m);
    }
}
#[test]
fn h0f_uses_documented_protocol_specific_source_packing() {
    for rtu in [false, true] {
        let mut m = memory(SerialPort::Com2);
        m.set_system(1143, rtu);
        if rtu {
            m.set_word(d(0), 0x34);
            m.set_word(d(1), 0x12);
        } else {
            m.set_word(d(0), 0x1234);
        }
        start(
            &mut m,
            &action("MODRW", &["K1", "K15", "H0500", "D0", "K16"]),
            SerialPort::Com2,
        );
        let mode = if rtu { WireMode::Rtu } else { WireMode::Ascii };
        let frame = serial_protocol::decode(mode, &m.serial_io.outgoing_frames()[0].bytes).unwrap();
        assert_eq!(&frame.data[5..], &[0x34, 0x12]);
        clean(&m);
    }
}
#[test]
fn h17_snapshots_all_three_descriptor_pairs_before_any_feedback_write() {
    let mut m = memory(SerialPort::Com2);
    for (index, value) in [
        (0, 0x1100),
        (1, 0x1000),
        (10, 3000),
        (11, 4000),
        (20, 2),
        (21, 2),
        (4000, 0x1770),
        (4001, 0x12),
    ] {
        m.set_word(d(index), value);
    }
    let a = action("MODRW", &["K1", "K23", "D0", "D10", "D20"]);
    start(&mut m, &a, SerialPort::Com2);
    assert_eq!(
        m.serial_io.outgoing_frames()[0].bytes,
        // PDF's 06 suffix contradicts its configured 1100/1000 addresses;
        // the independent byte-sum oracle gives LRC26 for this exact body.
        b":01171100000210000002041770001226\r\n"
    );
    m.set_word(d(10), 9999);
    let at = ready(&mut m, SerialPort::Com2);
    rx(&mut m, SerialPort::Com2, at + 1, b":0117040100176666\r\n");
    assert_eq!(m.word(d(3000)) as u16, 0x3130);
    assert_eq!(m.word(d(1296)), 0x0100);
    assert_eq!(m.word(d(1297)), 0x1766);
    clean(&m);
}
#[test]
fn h17_read_pointer_conflict_profile_is_explicit_for_com1() {
    for (policy, dest) in [
        (H17ReadPointerPolicy::DiagramReadIndex, 3000),
        (H17ReadPointerPolicy::ProseSecondIndex, 4000),
    ] {
        let mut m = memory(SerialPort::Com1);
        m.serial_io.h17_read_pointer_policy = policy;
        for (index, value) in [
            (0, 0x1100),
            (1, 0x1000),
            (10, 3000),
            (11, 4000),
            (20, 2),
            (21, 2),
            (4000, 0x1770),
            (4001, 0x12),
        ] {
            m.set_word(d(index), value);
        }
        start(
            &mut m,
            &action("MODRW", &["K1", "K23", "D0", "D10", "D20"]),
            SerialPort::Com1,
        );
        let at = ready(&mut m, SerialPort::Com1);
        rx(&mut m, SerialPort::Com1, at + 1, b":0117040100176666\r\n");
        assert_eq!(m.word(d(dest)), 0x0100);
        assert_eq!(m.word(d(dest + 1)), 0x1766);
        clean(&m);
    }
}
#[test]
fn false_contact_and_abort_do_not_invent_success_or_discard_environment_queue() {
    let mut m = memory(SerialPort::Com2);
    let a = action("RS", &["D0", "K1", "D100", "K1"]);
    start(&mut m, &a, SerialPort::Com2);
    m.serial_io
        .queue_received(SerialPort::Com2, 1_000_000, vec![1])
        .unwrap();
    a.execute(&mut m, false, 10);
    assert_eq!(m.serial_io.active_count(), 0);
    assert!(!m.system_bit(1123));
    assert_eq!(m.serial_io.rx_events.len(), 1);
    start(&mut m, &a, SerialPort::Com2);
    SerialBus::abort(&mut m);
    assert_eq!(m.serial_io.rx_events.len(), 1);
    assert_eq!(m.serial_io.active_count(), 0);
    clean(&m);
}
#[test]
fn invalid_function_does_not_wrap_u8_into_a_valid_function() {
    let mut m = memory(SerialPort::Com2);
    start(
        &mut m,
        &action("MODRW", &["K1", "K259", "H2102", "D100", "K2"]),
        SerialPort::Com2,
    );
    assert!(m.serial_io.outgoing_frames().is_empty());
    assert!(!m.execution_limitations.is_empty());
}
#[test]
fn read_broadcast_is_rejected_but_write_broadcast_completes_only_after_tx() {
    let mut m = memory(SerialPort::Com2);
    start(
        &mut m,
        &action("MODRD", &["K0", "K0", "K2"]),
        SerialPort::Com2,
    );
    assert!(m.serial_io.outgoing_frames().is_empty());
    m.execution_limitations.clear();
    start(
        &mut m,
        &action("MODWR", &["K0", "H2000", "H12"]),
        SerialPort::Com2,
    );
    let end = m.serial_io.outgoing_frames()[0].completed_at_us;
    assert!(!m.system_bit(1127));
    advance(&mut m, end);
    assert!(m.system_bit(1127));
    assert_eq!(m.serial_io.active_count(), 0);
    clean(&m);
}
#[test]
fn completion_flags_are_user_reset_latches_not_one_scan_pulses() {
    let mut m = memory(SerialPort::Com2);
    let a = action("RS", &["D0", "K0", "D100", "K1"]);
    start(&mut m, &a, SerialPort::Com2);
    rx(&mut m, SerialPort::Com2, 1, b"A");
    SerialBus::begin_scan(&mut m);
    SerialBus::end_scan(&mut m);
    assert!(m.system_bit(1123));
    let before = m.serial_io.outgoing_frames().len();
    start(&mut m, &a, SerialPort::Com2);
    assert_eq!(m.serial_io.outgoing_frames().len(), before);
    m.set_system(1123, false);
    a.execute(&mut m, true, 10);
    assert_eq!(m.serial_io.active_count(), 1);
    clean(&m);
}
#[test]
fn protocol_retention_preserves_active_snapshot_and_next_request_sees_new_setting() {
    let mut m = memory(SerialPort::Com2);
    m.set_system(1120, true);
    let a = action("RS", &["D0", "K1", "D100", "K1"]);
    start(&mut m, &a, SerialPort::Com2);
    let old = m.serial_io.ports[&SerialPort::Com2]
        .active
        .as_ref()
        .unwrap()
        .protocol;
    m.set_word(d(1120), 0x97);
    SerialBus::end_scan(&mut m);
    assert_eq!(
        m.serial_io.ports[&SerialPort::Com2]
            .active
            .as_ref()
            .unwrap()
            .protocol,
        old
    );
    SerialBus::abort(&mut m);
    m.set_system(1120, false);
    SerialBus::end_scan(&mut m);
    start(&mut m, &a, SerialPort::Com2);
    assert_eq!(
        m.serial_io.ports[&SerialPort::Com2]
            .active
            .as_ref()
            .unwrap()
            .protocol,
        0x97
    );
    clean(&m);
}

#[test]
fn cpu_restart_preserves_config_but_drops_old_epoch_transactions_and_wire_queues() {
    let mut m = memory(SerialPort::Com2);
    let a = action("RS", &["D0", "K1", "D100", "K1"]);
    start(&mut m, &a, SerialPort::Com2);
    m.serial_io
        .queue_received(SerialPort::Com2, 1_000_000, b"X".to_vec())
        .unwrap();
    advance(&mut m, 1000);
    m.serial_io.restart_cpu_clock(1000).unwrap();
    assert_eq!(m.serial_io.now_us, 0);
    assert_eq!(m.serial_io.configured_ports().count(), 1);
    assert_eq!(m.serial_io.profile().cpu, SerialCpu::Es2);
    assert_eq!(m.serial_io.active_count(), 0);
    assert!(m.serial_io.outgoing_frames().is_empty());
    assert!(m.serial_io.next_deadline_us().is_none());
    m.timing_io.now_us = 0;
    m.set_system(1123, false);
    m.set_system(1122, false);
    advance(&mut m, 1_000_000);
    assert!(!m.system_bit(1123));
    assert_eq!(m.word(d(100)), 0);
    clean(&m);
}

#[test]
fn stale_cpu_restart_timestamp_rejects_without_losing_active_configuration() {
    let mut m = memory(SerialPort::Com2);
    advance(&mut m, 1000);
    assert!(m.serial_io.restart_cpu_clock(999).is_err());
    assert_eq!(m.serial_io.now_us, 1000);
    assert_eq!(m.serial_io.configured_ports().count(), 1);
}

#[test]
fn receive_only_rs_never_invents_stx_etx_transmission() {
    let mut m = memory(SerialPort::Com2);
    m.set_word(d(1120), 0x787);
    m.set_system(1126, true);
    m.set_system(1130, true);
    start(
        &mut m,
        &action("RS", &["D0", "K0", "D100", "K1"]),
        SerialPort::Com2,
    );
    assert!(m.serial_io.outgoing_frames().is_empty());
    assert_eq!(
        m.serial_io.transaction(SerialPort::Com2).unwrap().phase,
        TransactionPhase::Receiving
    );
    assert!(m.system_bit(1124));
    clean(&m);
}

#[test]
fn rs_timeout_code_conflict_is_explicit_and_default_preserves_d1130() {
    for policy in [
        RsTimeoutCodePolicy::PreserveExcludedRegister,
        RsTimeoutCodePolicy::WriteK1Candidate,
    ] {
        let mut m = memory(SerialPort::Com2);
        m.serial_io.rs_timeout_code_policy = policy;
        m.set_word(d(1130), 123);
        start(
            &mut m,
            &action("RS", &["D0", "K0", "D100", "K1"]),
            SerialPort::Com2,
        );
        advance(&mut m, 100_000);
        assert!(m.system_bit(1129));
        assert!(!m.system_bit(1123));
        assert_eq!(
            m.word(d(1130)),
            if policy == RsTimeoutCodePolicy::WriteK1Candidate {
                1
            } else {
                123
            }
        );
        clean(&m);
    }
}

#[test]
fn ss2_profile_cannot_manufacture_a_com3_port() {
    let mut bus = SerialBus::default();
    assert!(
        bus.set_profile(SerialProfile {
            cpu: SerialCpu::Ss2,
            has_com3: true,
            ..SerialProfile::default()
        })
        .is_err()
    );
    bus.set_profile(SerialProfile {
        cpu: SerialCpu::Ss2,
        has_com3: false,
        ..SerialProfile::default()
    })
    .unwrap();
    assert!(
        bus.configure(SerialPort::Com3, PortConfig::rs485())
            .is_err()
    );
}

#[test]
fn modrd_single_register_requires_an_explicit_candidate_profile() {
    let mut m = memory(SerialPort::Com2);
    let a = action("MODRD", &["K1", "K0", "K1"]);
    start(&mut m, &a, SerialPort::Com2);
    assert!(m.serial_io.outgoing_frames().is_empty());
    assert!(!m.execution_limitations.is_empty());
    m.execution_limitations.clear();
    m.serial_io.modrd_count_policy = ModrdCountPolicy::SingleRegisterCandidate;
    a.execute(&mut m, true, 10);
    assert_eq!(m.serial_io.outgoing_frames().len(), 1);
    clean(&m);
}

#[test]
fn unconfigured_com2_control_hook_is_a_complete_noop_for_legacy_flag_programs() {
    let mut m = Memory::default();
    m.set_system(1125, true);
    m.set_system(1121, false);
    m.set_system(1127, true);
    m.set_system(1123, true);
    m.set_system(1124, true);
    m.set_system(1128, true);
    SerialBus::service_controls(&mut m);
    SerialBus::begin_scan(&mut m);
    SerialBus::end_scan(&mut m);
    assert!(!m.system_bit(1121));
    assert!(m.system_bit(1127));
    assert!(m.system_bit(1123));
    assert!(m.system_bit(1124));
    assert!(m.system_bit(1128));
    clean(&m);
}

fn rtu_read(port: SerialPort) -> (Memory, u64, Vec<u8>) {
    let mut m = memory(port);
    m.set_system(flags(port).rtu, true);
    let a = if port == SerialPort::Com2 {
        action("MODRD", &["K1", "H2102", "K2"])
    } else {
        modrw()
    };
    start(&mut m, &a, port);
    let at = ready(&mut m, port) + 100;
    (m, at, vec![1, 3, 4, 0x17, 0x70, 0, 0, 0xfe, 0x5c])
}

#[test]
fn rtu_valid_prefix_waits_for_t35_and_trailing_byte_prevents_false_success() {
    let (mut m, at, bytes) = rtu_read(SerialPort::Com2);
    m.set_word(d(1070), 777);
    m.serial_io
        .queue_received(SerialPort::Com2, at, bytes)
        .unwrap();
    advance(&mut m, at);
    assert!(!m.system_bit(1127));
    assert_eq!(m.word(d(1070)), 777);
    // A separate chunk is still part of THIS frame until t3.5 silence.
    m.serial_io
        .queue_received(SerialPort::Com2, at + 1000, vec![0xaa])
        .unwrap();
    advance(&mut m, at + 1000);
    assert!(!m.system_bit(1127));
    advance(&mut m, at + 5011);
    assert!(!m.system_bit(1127));
    assert!(m.system_bit(1140));
    assert_eq!(m.word(d(1070)), 777);
    clean(&m);
}

#[test]
fn rtu_9600_t35_uses_exact_11bit_standard_boundary() {
    let (mut m, at, bytes) = rtu_read(SerialPort::Com2);
    m.serial_io
        .queue_received(SerialPort::Com2, at, bytes)
        .unwrap();
    advance(&mut m, at + 4010);
    assert!(!m.system_bit(1127));
    advance(&mut m, at + 4011);
    assert!(m.system_bit(1127));
    assert!(!m.system_bit(1140));
    clean(&m);
}

#[test]
fn rtu_t15_violation_discards_a_crc_valid_split_frame() {
    for (gap, accepted) in [(1718, true), (1719, false)] {
        let (mut m, at, bytes) = rtu_read(SerialPort::Com2);
        m.serial_io
            .queue_received(SerialPort::Com2, at, bytes[..4].to_vec())
            .unwrap();
        m.serial_io
            .queue_received(SerialPort::Com2, at + gap, bytes[4..].to_vec())
            .unwrap();
        advance(&mut m, at + gap + 4011);
        assert_eq!(m.system_bit(1127), accepted);
        assert_eq!(m.system_bit(1140), !accepted);
        clean(&m);
    }
}

#[test]
fn rtu_high_baud_uses_fixed_750_1750us_timers() {
    for (gap, accepted) in [(750, true), (751, false)] {
        let mut m = memory(SerialPort::Com2);
        m.set_word(d(1120), 0xa7);
        m.set_system(1143, true);
        start(
            &mut m,
            &action("MODRD", &["K1", "H2102", "K2"]),
            SerialPort::Com2,
        );
        let at = ready(&mut m, SerialPort::Com2) + 100;
        let bytes = [1, 3, 4, 0x17, 0x70, 0, 0, 0xfe, 0x5c];
        m.serial_io
            .queue_received(SerialPort::Com2, at, bytes[..4].to_vec())
            .unwrap();
        m.serial_io
            .queue_received(SerialPort::Com2, at + gap, bytes[4..].to_vec())
            .unwrap();
        advance(&mut m, at + gap + 1749);
        assert!(!m.system_bit(1127));
        advance(&mut m, at + gap + 1750);
        assert_eq!(m.system_bit(1127), accepted);
        clean(&m);
    }
}

#[test]
fn delta_printed_rtu_gap_is_an_explicit_candidate_not_standard_timing() {
    let (mut m, at, bytes) = rtu_read(SerialPort::Com2);
    m.serial_io.rtu_timing_policy = RtuTimingPolicy::DeltaPrintedGapCandidate;
    // Timing is latched at transaction start, so re-arm under the selected profile.
    SerialBus::abort(&mut m);
    start(
        &mut m,
        &action("MODRD", &["K1", "H2102", "K2"]),
        SerialPort::Com2,
    );
    let at = ready(&mut m, SerialPort::Com2) + at.min(100);
    m.serial_io
        .queue_received(SerialPort::Com2, at, bytes)
        .unwrap();
    advance(&mut m, at + 1999);
    assert!(!m.system_bit(1127));
    advance(&mut m, at + 2000);
    assert!(m.system_bit(1127));
    clean(&m);
}

#[test]
fn wrong_station_reply_keeps_request_and_original_timeout_then_accepts_correct_reply() {
    for port in [SerialPort::Com1, SerialPort::Com2, SerialPort::Com3] {
        let (mut m, at, bytes) = rtu_read(port);
        let deadline = m.serial_io.ports[&port]
            .active
            .as_ref()
            .unwrap()
            .timeout_at
            .unwrap();
        let foreign = serial_protocol::encode(
            WireMode::Rtu,
            &Frame {
                station: 2,
                function: 3,
                data: vec![4, 0x17, 0x70, 0, 0],
            },
        )
        .unwrap();
        m.serial_io.queue_received(port, at, foreign).unwrap();
        advance(&mut m, at + 4011);
        assert!(m.serial_io.transaction(port).is_some());
        assert_eq!(
            m.serial_io.ports[&port].active.as_ref().unwrap().timeout_at,
            Some(deadline)
        );
        assert!(!m.system_bit(flags(port).done));
        assert!(!m.system_bit(if port == SerialPort::Com2 {
            1140
        } else {
            flags(port).error
        }));
        m.serial_io.queue_received(port, at + 5000, bytes).unwrap();
        advance(&mut m, at + 9011);
        assert!(m.system_bit(if port == SerialPort::Com2 {
            1127
        } else {
            flags(port).done
        }));
        clean(&m);
    }
}

#[test]
fn wrong_function_is_still_data_error_not_success() {
    let (mut m, at, _) = rtu_read(SerialPort::Com1);
    let frame = serial_protocol::encode(
        WireMode::Rtu,
        &Frame {
            station: 1,
            function: 4,
            data: vec![4, 0x17, 0x70, 0, 0],
        },
    )
    .unwrap();
    m.serial_io
        .queue_received(SerialPort::Com1, at, frame)
        .unwrap();
    advance(&mut m, at + 4011);
    assert!(!m.system_bit(1314));
    assert!(m.system_bit(1315));
    assert_eq!(m.word(d(1250)), 4);
    clean(&m);
}

#[test]
fn response_deadline_wins_tie_with_rtu_eof_but_one_us_earlier_succeeds() {
    for (earlier, success) in [(0, false), (1, true)] {
        let (mut m, _, bytes) = rtu_read(SerialPort::Com2);
        let deadline = m.serial_io.ports[&SerialPort::Com2]
            .active
            .as_ref()
            .unwrap()
            .timeout_at
            .unwrap();
        m.serial_io
            .queue_received(SerialPort::Com2, deadline - 4011 - earlier, bytes)
            .unwrap();
        advance(&mut m, deadline);
        assert_eq!(m.system_bit(1127), success);
        assert_eq!(m.system_bit(1129), !success);
        clean(&m);
    }
}
