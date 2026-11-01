use super::*;
use crate::serial_io::{PortConfig, SerialBus, SerialProfile};
fn dev(s: &str) -> Device {
    s.parse().unwrap()
}
fn a() -> DmvAction {
    DmvAction::parse("DMVRW", &["D0", "D4", "D8", "Y0"].map(str::to_owned)).unwrap()
}
fn m() -> Memory {
    let mut m = Memory::default();
    m.serial_io
        .set_profile(SerialProfile {
            cpu: SerialCpu::Ss2,
            firmware_major: 3,
            firmware_minor: 2,
            has_com3: false,
        })
        .unwrap();
    m.serial_io
        .configure(SerialPort::Com2, PortConfig::rs485())
        .unwrap();
    m.dmv_comm
        .configure_peer(SerialPort::Com2, 1, VirtualDmvPeer::default())
        .unwrap();
    for (n, v) in [
        (0, 2),
        (1, 1),
        (4, 0),
        (5, 0x10d0),
        (6, 1),
        (7, 1),
        (8, 0x0888),
        (1129, 100),
    ] {
        m.set_word(d(Area::D, n), v);
    }
    m
}
fn advance(m: &mut Memory, t: u64) {
    SerialBus::advance(m, t);
    DmvState::advance(m, t);
}
#[test]
fn pdf_custom_write_0888_to_10d0() {
    let mut m = m();
    a().execute(&mut m, true, 12);
    assert!(m.bit(dev("Y0")));
    assert!(!m.bit(dev("Y1")));
    advance(&mut m, 1000);
    assert_eq!(
        m.dmv_comm
            .peer(SerialPort::Com2, 1)
            .unwrap()
            .registers
            .get(&0x10d0),
        Some(&0x888)
    );
    assert!(!m.bit(dev("Y0")));
    assert!(m.bit(dev("Y1")));
    assert!(!m.bit(dev("Y2")));
    assert_eq!(m.serial_io.external_owner(SerialPort::Com2), None);
}
#[test]
fn pdf_k3_program20_trigger_read_alias() {
    let mut m = m();
    m.set_word(dev("D4"), 3);
    m.set_word(dev("D7"), 2);
    m.set_word(dev("D8"), 20);
    m.dmv_comm.peer_mut(SerialPort::Com2, 1).unwrap().output = vec![0x1234, 0xabcd];
    a().execute(&mut m, true, 20);
    advance(&mut m, 1000);
    assert_eq!(
        m.dmv_comm
            .peer(SerialPort::Com2, 1)
            .unwrap()
            .selected_program,
        20
    );
    assert_eq!(m.word(dev("D8")), 20);
    advance(&mut m, 2000);
    assert_eq!(
        m.dmv_comm.peer(SerialPort::Com2, 1).unwrap().trigger_count,
        1
    );
    assert_eq!(m.word(dev("D8")), 20);
    advance(&mut m, 3000);
    assert_eq!(m.word(dev("D8")) as u16, 0x1234);
    assert_eq!(m.word(dev("D9")) as u16, 0xabcd);
    assert!(m.bit(dev("Y1")));
}
#[test]
fn every_combination_has_literal_documented_sequence() {
    let m = m();
    for (code, kinds) in [
        (1, vec!["trigger", "read"]),
        (2, vec!["program", "trigger"]),
        (3, vec!["program", "trigger", "read"]),
        (4, vec!["memory1", "trigger"]),
        (5, vec!["memory1", "trigger", "read"]),
        (6, vec!["memory2", "trigger"]),
        (7, vec!["memory2", "trigger", "read"]),
    ] {
        let mut m = m.clone();
        m.set_word(dev("D4"), code);
        m.set_word(dev("D8"), 20);
        let p = a().prepare(&m).unwrap();
        let got: Vec<_> = p
            .operations
            .iter()
            .map(|op| match op {
                DmvOperation::SelectProgram(_) => "program",
                DmvOperation::Trigger1 => "trigger",
                DmvOperation::ReadOutput { .. } => "read",
                DmvOperation::WriteMemory { bank: 1, .. } => "memory1",
                DmvOperation::WriteMemory { .. } => "memory2",
                _ => "unexpected",
            })
            .collect();
        assert_eq!(got, kinds);
    }
}
#[test]
fn internal_memory_source_pair_snapshotted_before_later_read() {
    let mut m = m();
    m.set_word(dev("D4"), 7);
    m.set_word(dev("D7"), 2);
    m.set_word(dev("D8"), 0x1111);
    m.set_word(dev("D9"), 0x2222);
    m.dmv_comm.peer_mut(SerialPort::Com2, 1).unwrap().output = vec![3, 4];
    a().execute(&mut m, true, 6);
    m.set_word(dev("D8"), 99);
    advance(&mut m, 3000);
    let peer = m.dmv_comm.peer(SerialPort::Com2, 1).unwrap();
    assert_eq!(peer.memory2, [0x1111, 0x2222]);
    assert_eq!(m.word(dev("D8")), 3);
    assert_eq!(m.word(dev("D9")), 4);
}
#[test]
fn busy_retries_same_operation_until_success() {
    let mut m = m();
    m.dmv_comm.peer_mut(SerialPort::Com2, 1).unwrap().replies =
        VecDeque::from([DmvReply::Busy, DmvReply::Busy, DmvReply::Ready]);
    a().execute(&mut m, true, 3);
    advance(&mut m, 4000);
    assert!(m.bit(dev("Y1")));
    assert_eq!(m.dmv_comm.outgoing_requests().len(), 3);
    assert_eq!(
        m.dmv_comm
            .peer(SerialPort::Com2, 1)
            .unwrap()
            .registers
            .get(&0x10d0),
        Some(&0x888)
    );
}
#[test]
fn exception_restarts_whole_combination_explicit_policy() {
    let mut m = m();
    m.set_word(dev("D4"), 3);
    m.set_word(dev("D8"), 20);
    m.dmv_comm.peer_mut(SerialPort::Com2, 1).unwrap().replies = VecDeque::from([
        DmvReply::Ready,
        DmvReply::Exception(6),
        DmvReply::Ready,
        DmvReply::Ready,
        DmvReply::Ready,
    ]);
    a().execute(&mut m, true, 3);
    advance(&mut m, 7000);
    let ops: Vec<_> = m
        .dmv_comm
        .outgoing_requests()
        .iter()
        .map(|r| r.operation.clone())
        .collect();
    assert!(matches!(ops[0], DmvOperation::SelectProgram(20)));
    assert!(matches!(ops[1], DmvOperation::Trigger1));
    assert!(matches!(ops[2], DmvOperation::SelectProgram(20)));
    assert!(m.bit(dev("Y1")));
}
#[test]
fn explicitly_silent_peer_times_out_at_100ms() {
    let mut m = m();
    m.dmv_comm
        .peer_mut(SerialPort::Com2, 1)
        .unwrap()
        .replies
        .push_back(DmvReply::Silent);
    a().execute(&mut m, true, 3);
    advance(&mut m, 99999);
    assert!(m.bit(dev("Y0")));
    assert!(!m.bit(dev("Y2")));
    advance(&mut m, 100000);
    assert!(!m.bit(dev("Y0")));
    assert!(m.bit(dev("Y2")));
    assert!(!m.bit(dev("Y1")));
}
#[test]
fn completed_held_does_not_repeat_until_false_rearm() {
    let mut m = m();
    m.set_word(dev("D4"), 1);
    let op = a();
    op.execute(&mut m, true, 1);
    advance(&mut m, 2000);
    op.execute(&mut m, true, 1);
    assert_eq!(
        m.dmv_comm.peer(SerialPort::Com2, 1).unwrap().trigger_count,
        1
    );
    op.execute(&mut m, false, 1);
    op.execute(&mut m, true, 1);
    advance(&mut m, 4000);
    assert_eq!(
        m.dmv_comm.peer(SerialPort::Com2, 1).unwrap().trigger_count,
        2
    );
}
#[test]
fn false_cancel_releases_port_and_future_response_cannot_write() {
    let mut m = m();
    let op = a();
    op.execute(&mut m, true, 1);
    op.execute(&mut m, false, 1);
    advance(&mut m, 100000);
    assert!(
        m.dmv_comm
            .peer(SerialPort::Com2, 1)
            .unwrap()
            .registers
            .is_empty()
    );
    assert_eq!(m.serial_io.external_owner(SerialPort::Com2), None);
    assert_eq!(m.dmv_comm.active_count(), 0);
}
#[test]
fn other_owner_waits_without_latching_or_flags() {
    let mut m = m();
    m.serial_io.reserve_external(SerialPort::Com2, 99).unwrap();
    a().execute(&mut m, true, 1);
    assert_eq!(m.dmv_comm.active_count(), 0);
    assert!(!m.bit(dev("Y0")));
    assert!(m.execution_limitations.is_empty());
    m.serial_io.release_external(99);
    a().execute(&mut m, true, 1);
    assert_eq!(m.dmv_comm.active_count(), 1);
}
#[test]
fn full_destination_extent_validation_is_atomic() {
    let mut m = m();
    m.data_register_limit = Some(4999);
    m.set_word(dev("D4"), 1);
    m.set_word(dev("D7"), 2);
    let bad = DmvAction {
        data: dev("D4999"),
        ..a()
    };
    bad.execute(&mut m, true, 1);
    assert!(!m.bit(dev("Y0")));
    assert_eq!(m.dmv_comm.active_count(), 0);
    assert_eq!(m.serial_io.external_owner(SerialPort::Com2), None);
    assert!(!m.execution_limitations.is_empty());
}
#[test]
fn malformed_read_reply_keeps_all_destination_words() {
    let mut m = m();
    m.set_word(dev("D6"), 0);
    m.set_word(dev("D7"), 2);
    m.set_word(dev("D9"), 777);
    m.dmv_comm
        .peer_mut(SerialPort::Com2, 1)
        .unwrap()
        .replies
        .push_back(DmvReply::Words(vec![1]));
    a().execute(&mut m, true, 1);
    advance(&mut m, 1000);
    assert_eq!(m.word(dev("D8")), 0x888);
    assert_eq!(m.word(dev("D9")), 777);
    assert!(m.bit(dev("Y2")));
}
#[test]
fn cpu_restart_retains_external_peer_but_clears_inflight() {
    let mut m = m();
    a().execute(&mut m, true, 1);
    m.serial_io.reset_instruction_state();
    m.dmv_comm.reset_instruction_state();
    assert!(m.dmv_comm.peer(SerialPort::Com2, 1).is_some());
    assert_eq!(m.dmv_comm.now_us, 0);
    assert_eq!(m.dmv_comm.active_count(), 0);
    assert!(m.dmv_comm.outgoing_requests().is_empty());
}
#[test]
fn explicit_candidate_map_reuses_real_modbus_codec_without_claiming_delta_wire() {
    let map = DmvWireMap {
        program: 0x1100,
        memory1: 0x1200,
        memory2: 0x1202,
        trigger: 0x1000,
        trigger_value: 1,
        output: 0x1010,
        write_function: 16,
    };
    let op = DmvOperation::ReadOutput { count: 2 };
    let bytes = map.encode(WireMode::Ascii, 1, &op).unwrap();
    assert_eq!(bytes, b":010310100002DA\r\n");
    let frame = serial_protocol::decode(WireMode::Ascii, &bytes).unwrap();
    assert_eq!(frame.data, [0x10, 0x10, 0, 2]);
}
#[test]
fn es2_and_old_ss2_do_not_masquerade_as_supported_models() {
    for (cpu, minor) in [(SerialCpu::Es2, 2), (SerialCpu::Ss2, 0)] {
        let mut m = m();
        m.serial_io
            .set_profile(SerialProfile {
                cpu,
                firmware_major: 3,
                firmware_minor: minor,
                has_com3: false,
            })
            .unwrap();
        a().execute(&mut m, true, 1);
        assert_eq!(m.dmv_comm.active_count(), 0);
        assert!(!m.execution_limitations.is_empty());
    }
}
#[test]
fn documented_operand_matrix_and_no_p_forms() {
    for (name, args) in [
        ("DMVRWP", vec!["D0", "D4", "D8", "Y0"]),
        ("DDMVRW", vec!["D0", "D4", "D8", "Y0"]),
        ("DMVRW", vec!["K0", "D4", "D8", "Y0"]),
        ("DMVRW", vec!["D0", "D4", "D8", "X0"]),
    ] {
        assert!(
            DmvAction::parse(
                name,
                &args.iter().map(|s| s.to_string()).collect::<Vec<_>>()
            )
            .is_err()
        );
    }
    assert_eq!(a().words(), 9);
    assert_eq!(a().header(), 0x6e02);
}
