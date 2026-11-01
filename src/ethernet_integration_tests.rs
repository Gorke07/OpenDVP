//! PDF490..492 + independently literal Modbus/TCP frames on a virtual peer.
//! No physical socket/hardware or installed F200 fallback parity is implied.
use crate::ethernet::{BitPacking, CompletionRegister, EthernetModel, EthernetProfile};
use crate::file_memory::FirmwareVersion;
use crate::plc::{Device, Program, ScanStatus, Simulator, compile_il};
fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn sim() -> Simulator {
    let mut s = Simulator::default();
    s.memory.set_input(d("X20"), true);
    s.memory.ethernet.profile = EthernetProfile {
        model: EthernetModel::Se,
        firmware: FirmwareVersion::new(2, 4),
        ..EthernetProfile::default()
    };
    s.memory.ethernet.configure_port(108).unwrap();
    s.memory.set_word(d("D1349"), 100);
    for (i, v) in [
        (100, 2),
        (101, 0xc0a8u16 as i16),
        (102, 108),
        (103, 1),
        (104, 3),
    ] {
        s.memory.set_word(d(&format!("D{i}")), v);
    }
    s
}
fn program() -> Program {
    compile_il("LD X20\nETHRW D100 H1000 D10 K2\nEND").unwrap()
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
fn receive(s: &mut Simulator, at: u64, bytes: &[u8]) {
    let req = s.memory.ethernet.outgoing().back().unwrap().clone();
    s.memory
        .ethernet
        .queue_received(req.port, req.id, at, bytes.to_vec())
        .unwrap();
}
const READ_REQUEST: [u8; 12] = [0, 1, 0, 0, 0, 6, 1, 3, 0x10, 0, 0, 2];
const READ_RESPONSE: [u8; 13] = [0, 1, 0, 0, 0, 7, 1, 3, 4, 0, 42, 255, 255];
#[test]
fn genuine_nine_word_layout_later_pc_and_installed_guard() {
    let p = compile_il("LD X20\nETHRW D100 H1000 D10 K2\nMOV K7 D200\nEND").unwrap();
    let e = crate::es2_encoder::local_execution_layout(&p).unwrap();
    assert_eq!(e.known_word_at(1), Some(0xf200));
    assert_eq!(e.action_pc, vec![1, 10]);
    assert!(crate::es2_encoder::encode(&p).is_err());
}
#[test]
fn descriptor_is_snapshotted_and_busy_port_rejects_second_owner() {
    let p = compile_il("LD X20\nETHRW D100 H1000 D10 K2\nETHRW D110 H2000 D20 K2\nEND").unwrap();
    let mut s = sim();
    for (i, v) in [
        (110, 3),
        (111, 0xc0a8u16 as i16),
        (112, 108),
        (113, 2),
        (114, 4),
    ] {
        s.memory.set_word(d(&format!("D{i}")), v);
    }
    scan(&mut s, &p);
    assert_eq!(s.memory.ethernet.outgoing().len(), 1);
    let req = s.memory.ethernet.outgoing().front().unwrap();
    assert_eq!(req.bytes, READ_REQUEST);
    assert_eq!(req.remote, [192, 168, 0, 2]);
    assert_eq!(req.tcp_port, 502);
    s.memory.set_word(d("D100"), 99);
    s.memory.set_word(d("D103"), 77);
    scan(&mut s, &p);
    assert_eq!(s.memory.ethernet.outgoing().len(), 1);
    assert_eq!(
        s.memory.ethernet.outgoing().front().unwrap().bytes,
        READ_REQUEST
    );
    receive(&mut s, 1000, &READ_RESPONSE);
    advance(&mut s, 1000);
    assert_eq!(s.memory.word(d("D10")), 42);
    assert_eq!(s.memory.word(d("D11")), -1);
    assert_eq!(s.memory.word(d("D20")), 0);
}
#[test]
fn partial_mbap_reply_never_commits_or_signals_success_early() {
    let mut s = sim();
    s.memory.set_word(d("D10"), 111);
    s.memory.set_word(d("D11"), 222);
    scan(&mut s, &program());
    receive(&mut s, 1000, &READ_RESPONSE[..9]);
    receive(&mut s, 2000, &READ_RESPONSE[9..]);
    advance(&mut s, 1000);
    assert_eq!(s.memory.word(d("D10")), 111);
    assert_eq!(s.memory.word(d("D11")), 222);
    assert_eq!(s.memory.word(d("D1395")) & 256, 0);
    advance(&mut s, 1000);
    assert_eq!(s.memory.word(d("D10")), 42);
    assert_eq!(s.memory.word(d("D11")), -1);
    assert_eq!(s.memory.word(d("D1395")) & 256, 256);
    assert_eq!(s.memory.word(d("D1396")) & 256, 0);
}
#[test]
fn explicit_timeout_uses_d1349_and_preserves_result_data() {
    let mut s = sim();
    s.memory.set_word(d("D1349"), 5);
    s.memory.set_word(d("D10"), 321);
    scan(&mut s, &program());
    advance(&mut s, 4999);
    assert_eq!(s.memory.word(d("D1396")) & 256, 0);
    advance(&mut s, 1);
    assert_eq!(s.memory.word(d("D1396")) & 256, 256);
    assert_eq!(s.memory.word(d("D1395")) & 256, 0);
    assert_eq!(s.memory.word(d("D10")), 321);
}
#[test]
fn false_contact_cancels_late_reply_without_done_or_error() {
    let p = program();
    let mut s = sim();
    s.memory.set_word(d("D10"), 321);
    scan(&mut s, &p);
    receive(&mut s, 1000, &READ_RESPONSE);
    s.memory.set_input(d("X20"), false);
    scan(&mut s, &p);
    advance(&mut s, 1000000);
    assert_eq!(s.memory.word(d("D10")), 321);
    assert_eq!(s.memory.word(d("D1395")) & 256, 0);
    assert_eq!(s.memory.word(d("D1396")) & 256, 0);
}
#[test]
fn bit_packing_requires_a_policy_and_keeps_two_candidate_orders_distinct() {
    let p = compile_il("LD X20\nETHRW D100 H0400 D10 K16\nEND").unwrap();
    let mut s = sim();
    s.memory.set_word(d("D104"), 2);
    s.scan(&p, 0);
    assert!(!s.memory.execution_limitations.is_empty());
    assert!(s.memory.ethernet.outgoing().is_empty());
    for (policy, value) in [
        (BitPacking::LowBitFirst, 0x0201),
        (BitPacking::SwapBytesCandidate, 0x0102),
    ] {
        let mut s = sim();
        s.memory.ethernet.profile.bit_packing = policy;
        s.memory.set_word(d("D104"), 2);
        scan(&mut s, &p);
        receive(&mut s, 1000, &[0, 1, 0, 0, 0, 5, 1, 2, 2, 1, 2]);
        advance(&mut s, 1000);
        assert_eq!(s.memory.word(d("D10")), value);
    }
}
#[test]
fn completion_register_conflict_is_selected_explicitly_without_dual_writes() {
    for (policy, expected1394, expected1395) in [
        (CompletionRegister::Explanation1395, 1, 256),
        (CompletionRegister::Example1394, 257, 0),
    ] {
        let mut s = sim();
        s.memory.ethernet.profile.completion_register = policy;
        s.memory.set_word(d("D1394"), 1);
        scan(&mut s, &program());
        receive(&mut s, 1000, &READ_RESPONSE);
        advance(&mut s, 1000);
        assert_eq!(s.memory.word(d("D1394")), expected1394);
        assert_eq!(s.memory.word(d("D1395")), expected1395);
    }
}
#[test]
fn function_model_caps_and_custom_tcp_port_follow_documented_firmware_gates() {
    let p = compile_il("LD X20\nETHRW D100 H0400 D10 K16\nEND").unwrap();
    let mut s = sim();
    s.memory.ethernet.profile.firmware = FirmwareVersion::new(1, 85);
    s.memory.ethernet.profile.bit_packing = BitPacking::LowBitFirst;
    s.memory.set_word(d("D104"), 2);
    s.scan(&p, 0);
    assert!(s.memory.ethernet.outgoing().is_empty());
    assert!(!s.memory.execution_limitations.is_empty());
    for (fw, port) in [
        (FirmwareVersion::new(2, 3), 502),
        (FirmwareVersion::new(2, 4), 1502),
    ] {
        let mut s = sim();
        s.memory.ethernet.profile.firmware = fw;
        s.memory.set_word(d("D1111"), 1502);
        scan(&mut s, &program());
        assert_eq!(s.memory.ethernet.outgoing().back().unwrap().tcp_port, port);
    }
}
#[test]
fn full_descriptor_and_result_ranges_are_preflighted_without_partial_mutation() {
    for line in ["ETHRW D4997 H1000 D10 K2", "ETHRW D100 H1000 D4999 K2"] {
        let p = compile_il(&format!("LD X20\n{line}\nEND")).unwrap();
        let mut s = sim();
        s.memory.data_register_limit = Some(4999);
        s.memory.set_word(d("D10"), 111);
        s.memory.set_word(d("D4999"), 222);
        s.scan(&p, 0);
        assert!(!s.memory.execution_limitations.is_empty());
        assert!(s.memory.ethernet.outgoing().is_empty());
        assert_eq!(s.memory.word(d("D10")), 111);
        assert_eq!(s.memory.word(d("D4999")), 222);
        assert_eq!(s.memory.word(d("D1395")), 0);
        assert_eq!(s.memory.word(d("D1396")), 0);
    }
    for (n, count) in [(0, 1), (97, 96)] {
        let p = compile_il(&format!("LD X20\nETHRW D100 H1000 D10 K{n}\nEND")).unwrap();
        let mut s = sim();
        scan(&mut s, &p);
        assert_eq!(
            &s.memory.ethernet.outgoing().back().unwrap().bytes[10..12],
            &[0, count]
        );
    }
}
#[test]
fn fc06_exact_ack_echo_preserves_source_and_adjacent_data() {
    let p = compile_il("LD X20\nETHRW D100 H1000 D10 K1\nEND").unwrap();
    let mut s = sim();
    s.memory.set_word(d("D104"), 6);
    s.memory.set_word(d("D10"), 42);
    s.memory.set_word(d("D11"), 777);
    scan(&mut s, &p);
    const ACK: [u8; 12] = [0, 1, 0, 0, 0, 6, 1, 6, 0x10, 0, 0, 42];
    assert_eq!(s.memory.ethernet.outgoing().back().unwrap().bytes, ACK);
    receive(&mut s, 1000, &ACK);
    advance(&mut s, 1000);
    assert_eq!(s.memory.word(d("D10")), 42);
    assert_eq!(s.memory.word(d("D11")), 777);
    assert_eq!(s.memory.word(d("D1395")) & 256, 256);
}
