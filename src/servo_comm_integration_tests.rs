//! Actual IL/VM acceptance under explicit virtual ASDA Modbus carrier.
//! Literal selector0 and selector4 wire frames; no native-stub/hardware parity.
use crate::plc::{Device, Program, ScanStatus, Simulator, compile_il};
use crate::serial_io::{PortConfig, SerialPort};
use crate::servo_comm::{ServoCarrier, ServoModel, ServoProfile};
fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn sim() -> Simulator {
    let mut s = Simulator::default();
    s.memory.set_input(d("X20"), true);
    s.memory
        .serial_io
        .configure(SerialPort::Com2, PortConfig::rs485())
        .unwrap();
    s.memory.serial_io.servo_profile = ServoProfile {
        model: ServoModel::LegacyA,
        carrier: ServoCarrier::StandardModbusCandidate,
        ..ServoProfile::default()
    };
    s.memory.set_word(d("D1120"), 0x86);
    s.memory.set_word(d("D1129"), 100);
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
fn reply(s: &mut Simulator, wire: &[u8]) {
    let at = s
        .memory
        .serial_io
        .outgoing_frames()
        .back()
        .unwrap()
        .completed_at_us
        + 2000;
    s.memory
        .serial_io
        .queue_received(SerialPort::Com2, at, wire.to_vec())
        .unwrap();
    let delta = at - s.memory.timing_io.now_us;
    crate::timing_io::advance_by(&mut s.memory, delta);
}
#[test]
fn asdrw_has_genuine_seven_word_layout_and_rejects_native_stub_upload() {
    let p = compile_il("LD X20\nASDRW K1 K0 D20\nMOV K7 D100\nEND").unwrap();
    let layout = crate::es2_encoder::local_execution_layout(&p).unwrap();
    assert_eq!(layout.known_word_at(1), Some(0xba01));
    assert_eq!(layout.action_pc, vec![1, 8]);
    assert!(crate::es2_encoder::encode(&p).is_err());
}
#[test]
fn compiled_status_read_commits_five_direct_words_without_raw_ascii_bank() {
    let p = compile_il("LD X20\nASDRW K1 K0 D20\nMOV K77 D200\nEND").unwrap();
    let mut s = sim();
    s.memory.set_word(d("D1070"), 999);
    s.memory.set_system(1122, true);
    scan(&mut s, &p);
    assert_eq!(s.memory.word(d("D200")), 77);
    assert_eq!(
        s.memory.serial_io.outgoing_frames().back().unwrap().bytes,
        b":010300040005F3\r\n"
    );
    reply(&mut s, b":01030A00010002000300040005E3\r\n");
    for (i, expected) in [1, 2, 3, 4, 5].into_iter().enumerate() {
        assert_eq!(s.memory.word(d(&format!("D{}", 20 + i))), expected);
    }
    assert_eq!(s.memory.word(d("D1070")), 999);
    assert!(s.memory.system_bit(1127));
}
#[test]
fn compiled_servo_on_write_echo_is_ack_and_never_overwrites_data_with_reply() {
    let p = compile_il("LD X20\nASDRW K1 K4 D20\nEND").unwrap();
    let mut s = sim();
    s.memory.set_word(d("D20"), 1);
    s.memory.set_word(d("D21"), -222);
    s.memory.set_system(1122, true);
    scan(&mut s, &p);
    assert_eq!(
        s.memory.serial_io.outgoing_frames().back().unwrap().bytes,
        b":0106021E0001D8\r\n"
    );
    reply(&mut s, b":0106021E0001D8\r\n");
    assert_eq!(s.memory.word(d("D20")), 1);
    assert_eq!(s.memory.word(d("D21")), -222);
    assert!(s.memory.system_bit(1127));
}
#[test]
fn compiled_mc_false_cancels_the_same_servo_transaction_without_done() {
    let p = compile_il("LD X21\nMC N0\nLD X20\nASDRW K1 K0 D20\nMCR N0\nEND").unwrap();
    let mut s = sim();
    s.memory.set_input(d("X21"), true);
    s.memory.set_system(1122, true);
    scan(&mut s, &p);
    assert!(s.memory.serial_io.transaction(SerialPort::Com2).is_some());
    s.memory.set_input(d("X21"), false);
    scan(&mut s, &p);
    assert!(s.memory.serial_io.transaction(SerialPort::Com2).is_none());
    assert!(!s.memory.system_bit(1127));
    assert!(!s.memory.system_bit(1122));
    crate::timing_io::advance_by(&mut s.memory, 1000000);
    assert!(!s.memory.system_bit(1127));
    assert_eq!(s.memory.serial_io.outgoing_frames().len(), 1);
}
#[test]
fn compiled_full_read_span_preflight_holds_request_and_data_on_failure() {
    let p = compile_il("LD X20\nASDRW K1 K0 D4997\nEND").unwrap();
    let mut s = sim();
    s.memory.data_register_limit = Some(4999);
    s.memory.set_word(d("D4997"), 321);
    s.memory.set_system(1122, true);
    s.scan(&p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(!s.memory.execution_limitations.is_empty());
    assert!(s.memory.serial_io.outgoing_frames().is_empty());
    assert!(s.memory.system_bit(1122));
    assert!(!s.memory.system_bit(1127));
    assert_eq!(s.memory.word(d("D4997")), 321);
}
