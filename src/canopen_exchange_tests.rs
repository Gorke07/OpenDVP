use super::*;
use crate::canopen::{CanProfile, CanServoModel, CanopenAction};
fn a(name: &str, args: &[&str]) -> ExchangeAction {
    ExchangeAction::parse(
        name,
        &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn memory() -> Memory {
    let mut m = Memory::default();
    m.canopen
        .set_profile(CanProfile {
            cpu: CanCpu::Es2C,
            firmware_major: 3,
            firmware_minor: 60,
            servo: CanServoModel::A2M,
        })
        .unwrap();
    m.canopen.configure_node(1, true).unwrap();
    m.can_exchange
        .configure_raw_port(
            0,
            RawCanPort {
                cpu: RawCanCpu::Es2C,
                firmware_major: 3,
                firmware_minor: 60,
                module_firmware: None,
                module_master_without_slaves: true,
            },
        )
        .unwrap();
    m.set_system(1620, true);
    m.set_word(d(1177), 200);
    m
}
fn advance(m: &mut Memory, at: u64) {
    m.timing_io.now_us = at;
    CanExchangeState::advance(m, at);
}
fn sdo_reply(m: &mut Memory, result: SdoResult) {
    let r = m.can_exchange.pending_sdo().unwrap().clone();
    let at = m.can_exchange.now_us + 1;
    m.can_exchange.queue_sdo_reply(at, r, result).unwrap();
    advance(m, at);
}
fn tx_done(m: &mut Memory) {
    let tx = m.can_exchange.outgoing_frames().back().unwrap().clone();
    let at = m.can_exchange.now_us + 1;
    m.can_exchange
        .queue_transmitted(at, tx.id, tx.epoch)
        .unwrap();
    advance(m, at);
}
fn raw(m: &mut Memory, at: u64, id: u32, bytes: &[u8]) {
    m.can_exchange
        .queue_frame(
            at,
            0,
            CanFrame {
                id,
                standard: true,
                data: bytes.into(),
            },
        )
        .unwrap();
    advance(m, at);
}

#[test]
fn forms_and_strict_matrix_preserve_genuine_compiler_headers() {
    for (name, args, header, words) in [
        (
            "COPRW",
            vec!["K1", "H2B", "H220A", "K0", "D0", "M0"],
            Some(0xc802),
            13,
        ),
        ("COPWL", vec!["K1", "D0", "K2", "M0"], Some(0xbc02), 9),
        ("DCOPWL", vec!["K1", "D0", "K2", "M0"], Some(0xbd02), 17),
        (
            "CANRS",
            vec!["K0", "D0", "D10", "D20", "M0"],
            Some(0xc602),
            11,
        ),
    ] {
        let action = a(name, &args);
        assert_eq!(action.header(), header);
        assert_eq!(action.words(), words);
        assert_eq!(action.operands().len(), args.len());
    }
    for (name, args) in [
        ("COPRW", vec!["D0", "H40", "H220A", "K0", "D0", "M0"]),
        ("COPWL", vec!["K1", "D0", "K4M0", "M0"]),
        ("CANRSP", vec!["K0", "D0", "D10", "D20", "M0"]),
        ("CANRS", vec!["K0", "D0", "D10", "D20", "Y0"]),
    ] {
        assert!(
            ExchangeAction::parse(
                name,
                &args.iter().map(|s| s.to_string()).collect::<Vec<_>>()
            )
            .is_err()
        );
    }
}
#[test]
fn coprw_typed_write_waits_ack_and_completion_clear_rearms() {
    let mut m = memory();
    m.set_word(d(0), 0x1234);
    let action = a("COPRW", &["K1", "H2B", "H220A", "K0", "D0", "M0"]);
    action.execute(&mut m, true, 10);
    assert_eq!(
        m.can_exchange.pending_sdo().unwrap().write,
        Some(vec![0x34, 0x12])
    );
    assert!(!m.bit(bit(0)));
    sdo_reply(&mut m, SdoResult::WriteAck);
    assert!(!m.bit(bit(0)));
    action.execute(&mut m, true, 10);
    assert!(m.bit(bit(0)));
    assert!(m.canopen.exchange_owner().is_none());
    action.execute(&mut m, true, 10);
    assert_eq!(m.can_exchange.outgoing_sdo().len(), 1);
    m.set_internal(bit(0), false);
    action.execute(&mut m, true, 10);
    assert_eq!(m.can_exchange.outgoing_sdo().len(), 2);
}
#[test]
fn read_reply_commits_real_data_and_byte_read_preserves_upper_byte() {
    let mut m = memory();
    m.set_word(d(9999), 0xab00u16 as i16);
    let action = a("COPRW", &["K1", "H40", "H220A", "K0", "D9999", "M0"]);
    action.execute(&mut m, true, 10);
    sdo_reply(&mut m, SdoResult::Data(vec![0x7f]));
    action.execute(&mut m, true, 10);
    assert_eq!(m.word(d(9999)) as u16, 0xab7f);
    assert!(m.bit(bit(0)));
}
#[test]
fn wide_list_rows_are_snapshot_but_next_row_requires_actual_visit() {
    let mut m = memory();
    for (i, v) in [0x212c, 0, 0x1234, 0x5678, 0x212d, 0, 0xabcd, 0x0123]
        .into_iter()
        .enumerate()
    {
        m.set_word(d(i as u16), v as i16);
    }
    let action = a("DCOPWL", &["K1", "D0", "K2", "M20"]);
    action.execute(&mut m, true, 10);
    assert_eq!(
        m.can_exchange.pending_sdo().unwrap().write,
        Some(vec![0x34, 0x12, 0x78, 0x56])
    );
    m.set_word(d(6), 0);
    sdo_reply(&mut m, SdoResult::WriteAck);
    assert_eq!(m.can_exchange.outgoing_sdo().len(), 1);
    assert!(m.can_exchange.pending_sdo().is_none());
    action.execute(&mut m, true, 10);
    assert_eq!(m.can_exchange.pending_sdo().unwrap().index, 0x212d);
    assert_eq!(
        m.can_exchange.pending_sdo().unwrap().write,
        Some(vec![0xcd, 0xab, 0x23, 0x01])
    );
    sdo_reply(&mut m, SdoResult::WriteAck);
    action.execute(&mut m, true, 10);
    assert!(m.bit(bit(20)));
    assert!(m.canopen.exchange_owner().is_some());
    action.execute(&mut m, false, 10);
    assert!(!m.bit(bit(20)));
    assert!(m.canopen.exchange_owner().is_none());
}
#[test]
fn full_list_span_preflight_and_abort_retry_require_false_rearm() {
    let mut m = memory();
    let action = a("COPWL", &["K1", "D9997", "K2", "M20"]);
    action.execute(&mut m, true, 10);
    assert!(m.can_exchange.outgoing_sdo().is_empty());
    assert!(m.canopen.exchange_owner().is_none());
    m.set_word(d(0), 0x220a);
    let action = a("COPWL", &["K1", "D0", "K1", "M20"]);
    action.execute(&mut m, true, 10);
    sdo_reply(
        &mut m,
        SdoResult::Abort {
            sdo_code: 0x06020000,
            plc_code: Some(99),
        },
    );
    action.execute(&mut m, true, 10);
    assert_eq!(m.can_exchange.outgoing_sdo().len(), 1);
    assert_eq!(m.word(d(6001)), 99);
    action.execute(&mut m, false, 10);
    action.execute(&mut m, true, 10);
    assert_eq!(m.can_exchange.outgoing_sdo().len(), 2);
}
#[test]
fn initc_and_exchange_share_the_same_owner_in_both_directions() {
    let mut m = memory();
    m.set_system(1614, true);
    let init = CanopenAction::parse("INITC", &["K1".into()]).unwrap();
    init.execute(&mut m, true, 1);
    let read = a("COPRW", &["K1", "H40", "H220A", "K0", "D0", "M0"]);
    m.set_system(1614, false);
    read.execute(&mut m, true, 10);
    assert!(m.can_exchange.outgoing_sdo().is_empty());
    crate::canopen::CanopenNetwork::abort(&mut m);
    read.execute(&mut m, true, 10);
    assert!(m.canopen.exchange_owner().is_some());
    m.set_system(1614, true);
    init.execute(&mut m, true, 1);
    assert!(m.canopen.pending().is_none());
}
#[test]
fn wrong_sdo_reply_preserves_owner_and_deadline() {
    let mut m = memory();
    m.can_exchange.sdo_timeout_us = Some(10);
    let action = a("COPRW", &["K1", "H40", "H220A", "K0", "D0", "M0"]);
    action.execute(&mut m, true, 10);
    let mut wrong = m.can_exchange.pending_sdo().unwrap().clone();
    wrong.index = 0x220b;
    m.can_exchange
        .queue_sdo_reply(5, wrong, SdoResult::Data(vec![1, 0]))
        .unwrap();
    advance(&mut m, 5);
    assert!(m.can_exchange.pending_sdo().is_some());
    advance(&mut m, 10);
    assert!(m.system_bit(1616));
    assert!(!m.bit(bit(0)));
    assert!(m.canopen.exchange_owner().is_none());
}
#[test]
fn raw_master_requires_transmit_confirmation_then_matching_real_frame() {
    let mut m = memory();
    m.set_word(d(0), 0x700);
    m.set_word(d(1), 2);
    m.set_word(d(10), 0xab11u16 as i16);
    m.set_word(d(11), 0xcd22u16 as i16);
    m.set_word(d(20), 0x709);
    let action = a("CANRS", &["K0", "D0", "D10", "D20", "M0"]);
    action.execute(&mut m, true, 10);
    assert_eq!(
        m.can_exchange.outgoing_frames().back().unwrap().frame.data,
        vec![0x11, 0x22]
    );
    raw(&mut m, 1, 0x709, &[1]);
    assert_eq!(m.word(d(21)), 0);
    tx_done(&mut m);
    let now = m.can_exchange.now_us;
    raw(&mut m, now + 1, 0x708, &[2]);
    assert!(!m.bit(bit(0)));
    raw(&mut m, now + 2, 0x709, &[3, 4]);
    assert_eq!(m.word(d(22)), 3);
    action.execute(&mut m, true, 10);
    assert!(m.bit(bit(0)));
}
#[test]
fn slave_receives_before_response_and_zero_length_never_emits_empty_tx() {
    let mut m = memory();
    m.set_system(1621, true);
    m.set_word(d(0), 0x12);
    m.set_word(d(1), 1);
    m.set_word(d(10), 0x44);
    m.set_word(d(20), 0x12);
    let action = a("CANRS", &["K0", "D0", "D10", "D20", "M0"]);
    action.execute(&mut m, true, 10);
    assert!(m.can_exchange.outgoing_frames().is_empty());
    raw(&mut m, 1, 0x12, &[4]);
    assert_eq!(
        m.can_exchange.outgoing_frames().back().unwrap().frame.data,
        vec![0x44]
    );
    tx_done(&mut m);
    action.execute(&mut m, true, 10);
    assert!(m.bit(bit(0)));
    action.execute(&mut m, false, 10);
    m.set_word(d(1), 0);
    let count = m.can_exchange.outgoing_frames().len();
    action.execute(&mut m, true, 10);
    let now = m.can_exchange.now_us;
    raw(&mut m, now + 1, 0x12, &[]);
    action.execute(&mut m, true, 10);
    assert_eq!(m.can_exchange.outgoing_frames().len(), count);
    assert!(m.bit(bit(0)));
}
#[test]
fn broadcast_idle_timeout_and_packet_counter_publish_on_next_visit() {
    let mut m = memory();
    m.set_system(1622, true);
    m.set_word(d(0), 0x50);
    m.set_word(d(1), 1);
    m.set_word(d(1177), 1);
    let action = a("CANRS", &["K0", "D0", "D10", "D20", "M0"]);
    action.execute(&mut m, true, 10);
    tx_done(&mut m);
    raw(&mut m, 500, 0x201, &[1]);
    raw(&mut m, 900, 0x202, &[2]);
    advance(&mut m, 1500);
    assert!(!m.bit(bit(0)));
    advance(&mut m, 1900);
    action.execute(&mut m, true, 10);
    assert!(m.bit(bit(0)));
    assert_eq!(m.word(d(1175)), 2);
    assert_eq!(m.word(d(20)), 0x201);
    assert_eq!(m.word(d(30)), 0x202);
    assert!(m.system_bit(1623));
}
#[test]
fn hundred_packet_cap_never_writes_the_101st_record() {
    let mut m = memory();
    // D1020 has a CPU default; use an explicit sentinel beyond record 100.
    m.set_word(d(1020), -1234);
    m.set_system(1621, true);
    m.set_system(1622, true);
    m.set_word(d(1), 0);
    m.set_word(d(1177), 0);
    let action = a("CANRS", &["K0", "D0", "D10", "D20", "M0"]);
    action.execute(&mut m, true, 10);
    for i in 0..101 {
        raw(&mut m, i + 1, 0x200 + i as u32, &[i as u8]);
    }
    assert_eq!(m.word(d(1175)), 100);
    assert_eq!(m.word(d(1020)), -1234);
    action.execute(&mut m, true, 10);
    assert!(m.bit(bit(0)));
    assert!(m.system_bit(1623));
}
#[test]
fn unknown_extended_broadcast_layout_and_unconfigured_hardware_are_not_success() {
    let mut m = memory();
    m.set_system(1620, false);
    m.set_system(1622, true);
    let action = a("CANRS", &["K0", "D0", "D10", "D20", "M0"]);
    action.execute(&mut m, true, 10);
    assert!(m.canopen.exchange_owner().is_none());
    assert!(m.can_exchange.outgoing_frames().is_empty());
    assert!(
        m.can_exchange
            .configure_raw_port(
                1,
                RawCanPort {
                    cpu: RawCanCpu::Es2C,
                    firmware_major: 3,
                    firmware_minor: 60,
                    module_firmware: None,
                    module_master_without_slaves: true
                }
            )
            .is_err()
    );
}
#[test]
fn cpu_epoch_reset_releases_reservation_and_old_reply_does_not_commit() {
    let mut m = memory();
    let action = a("COPRW", &["K1", "H40", "H220A", "K0", "D0", "M0"]);
    action.execute(&mut m, true, 10);
    let old = m.can_exchange.pending_sdo().unwrap().clone();
    CanExchangeState::abort(&mut m);
    action.execute(&mut m, true, 10);
    m.can_exchange
        .queue_sdo_reply(1, old, SdoResult::Data(vec![99, 0]))
        .unwrap();
    advance(&mut m, 1);
    assert_eq!(m.word(d(0)), 0);
    assert!(m.can_exchange.pending_sdo().is_some());
}
