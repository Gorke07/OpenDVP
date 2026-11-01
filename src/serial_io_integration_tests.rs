//! Compiled-VM serial checks from the independent PDF oracle. These use an
//! explicitly configured virtual port/peer and virtual time, not UART hardware
//! or Delta editor-native ST execution.
use crate::plc::{Device, Program, ScanStatus, Simulator, compile_il};
use crate::serial_io::{PortConfig, SerialPort};
use crate::serial_protocol::{VirtualModbusPeer, WireMode};

fn d(text: &str) -> Device {
    text.parse().unwrap()
}
fn configured() -> Simulator {
    let mut s = Simulator::default();
    s.memory
        .serial_io
        .configure(SerialPort::Com2, PortConfig::rs485())
        .unwrap();
    s.memory.set_word(d("D1120"), 0x87); // explicit 9600,8E1: 11 bits/character
    s.memory.set_word(d("D1129"), 100);
    s
}
fn scan(s: &mut Simulator, p: &Program, ms: u64) {
    s.scan(p, ms);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(
        s.memory.execution_limitations.is_empty(),
        "{:?}",
        s.memory.execution_limitations
    );
}
fn request(s: &mut Simulator) {
    s.memory.set_system(1122, true);
}
fn advance(s: &mut Simulator, us: u64) {
    crate::timing_io::advance_by(&mut s.memory, us);
    assert!(
        s.memory.execution_limitations.is_empty(),
        "{:?}",
        s.memory.execution_limitations
    );
}

#[test]
fn four_real_word_lengths_and_later_action_offsets_survive_compilation() {
    for (command, header, words) in [
        ("RS D0 K1 D100 K1", 0xb000, 9),
        ("MODRD K1 H2102 K2", 0xc200, 7),
        ("MODWR K1 H2000 H12", 0xc400, 7),
        ("MODRW K1 K3 H2102 D100 K2", 0x4c01, 11),
    ] {
        let p = compile_il(&format!("LD X0\n{command}\nMOV K7 D200\nEND")).unwrap();
        let layout = crate::es2_encoder::local_execution_layout(&p).unwrap();
        assert_eq!(layout.known_word_at(1), Some(header));
        assert_eq!(layout.action_pc, vec![1, 1 + words]);
        // Existing installed handlers are not a functional communication oracle.
        assert!(crate::es2_encoder::encode(&p).is_err());
    }
}

#[test]
fn il_matrix_rejects_grouped_buffers_non_d_sources_and_unlisted_forms() {
    for command in [
        "RS K1 K1 D100 K1",
        "RS D0 K1 M100 K1",
        "RS D0 T0 D100 K1",
        "MODRD T0 H2102 K2",
        "MODWR K1 H2000 C0",
        "MODRW K1 K3 H2102 K4M0 K2",
        "MODRW K1 K3 H2102 T0 K2",
        "RSP D0 K1 D100 K1",
        "DRS D0 K1 D100 K1",
        "MODRDP K1 K0 K2",
    ] {
        assert!(
            compile_il(&format!("LD X0\n{command}\nEND")).is_err(),
            "{command}"
        );
    }
}

#[test]
fn absent_port_reports_a_limitation_without_fabricated_rx_or_success() {
    let p = compile_il("LD X0\nMODRD K1 H2102 K2\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X0"), true);
    s.memory.set_word(d("D1070"), 777);
    request(&mut s);
    s.scan(&p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(!s.memory.execution_limitations.is_empty());
    assert!(s.memory.serial_io.outgoing_frames().is_empty());
    assert_eq!(s.memory.serial_io.active_count(), 0);
    assert_eq!(s.memory.word(d("D1070")), 777);
    assert!(!s.memory.system_bit(1127));
}

#[test]
fn explicit_false_visit_cancels_default_local_transaction() {
    let p = compile_il("LD X0\nRS D0 K1 D100 K1\nEND").unwrap();
    let mut s = configured();
    s.memory.set_input(d("X0"), true);
    request(&mut s);
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.serial_io.active_count(), 1);
    s.memory
        .serial_io
        .queue_received(SerialPort::Com2, 5_000, vec![0x55])
        .unwrap();
    s.memory.set_input(d("X0"), false);
    scan(&mut s, &p, 0);
    advance(&mut s, 10_000);
    assert_eq!(s.memory.serial_io.active_count(), 0);
    assert_eq!(s.memory.word(d("D100")), 0);
    assert!(!s.memory.system_bit(1123));
}

#[test]
fn cj_skipped_owner_is_not_a_false_visit_and_can_finish_during_scan_time() {
    let p = compile_il("LD X1\nCJ P0\nLD X0\nRS D0 K1 D100 K1\nP0\nEND").unwrap();
    let mut s = configured();
    s.memory.set_input(d("X0"), true);
    request(&mut s);
    scan(&mut s, &p, 0);
    s.memory
        .serial_io
        .queue_received(SerialPort::Com2, 5_000, vec![0x55])
        .unwrap();
    s.memory.set_input(d("X1"), true);
    scan(&mut s, &p, 10);
    assert_eq!(s.memory.word(d("D100")), 0x55);
    assert!(s.memory.system_bit(1123));
    assert_eq!(s.memory.serial_io.outgoing_frames().len(), 1);
}

#[test]
fn mc_false_holds_request_without_starting_serial_io() {
    let p = compile_il("LD X0\nMC N0\nLD M1000\nMODRD K1 H2102 K2\nMCR N0\nEND").unwrap();
    let mut s = configured();
    request(&mut s);
    scan(&mut s, &p, 0);
    assert!(s.memory.system_bit(1122));
    assert!(s.memory.serial_io.outgoing_frames().is_empty());
    assert_eq!(s.memory.serial_io.active_count(), 0);
}

#[test]
fn delay_services_rx_and_tx_uses_snapshot_even_when_source_aliases_destination() {
    // PDF393 low/high packing; the post-start MOV changes memory, not the frame.
    let p =
        compile_il("LD X0\nRS D0 K2 D0 K2\nMOV H5A5A D0\nDELAY K100\nMOV D0 D200\nEND").unwrap();
    let mut s = configured();
    s.memory.set_word(d("D0"), 0x4241);
    s.memory.set_input(d("X0"), true);
    s.memory
        .serial_io
        .queue_received(SerialPort::Com2, 5_000, b"CD".to_vec())
        .unwrap();
    request(&mut s);
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.serial_io.outgoing_frames()[0].bytes, b"AB");
    assert_eq!(s.memory.word(d("D0")), 0x4443);
    assert_eq!(s.memory.word(d("D200")), 0x4443);
    assert!(s.memory.system_bit(1123));
}

#[test]
fn published_modrd_ascii_reply_reaches_fixed_and_converted_buffers_during_delay() {
    // PDF463: exact published body/reply; checksum3B independently recomputed.
    let p = compile_il("LD X0\nMODRD K1 H2101 K6\nDELAY K500\nMOV D1051 D200\nEND").unwrap();
    let mut s = configured();
    s.memory.set_input(d("X0"), true);
    s.memory
        .serial_io
        .queue_received(
            SerialPort::Com2,
            30_000,
            b":01030C0100176600000000013600003B\r\n".to_vec(),
        )
        .unwrap();
    request(&mut s);
    scan(&mut s, &p, 0);
    assert_eq!(
        s.memory.serial_io.outgoing_frames()[0].bytes,
        b":010321010006D4\r\n"
    );
    assert_eq!(s.memory.word(d("D1070")), 0x3130);
    assert_eq!(s.memory.word(d("D1071")), 0x3330);
    assert_eq!(s.memory.word(d("D1050")), 0x0100);
    assert_eq!(s.memory.word(d("D200")), 0x1766);
    assert!(s.memory.system_bit(1127));
    assert!(!s.memory.system_bit(1123));
}

#[test]
fn published_modwr_rtu_echo_uses_raw_low_byte_words() {
    // PDF469 exactrequest/echo: CRC0207, little-endian checksum bytes.
    let p = compile_il("LD X0\nMODWR K1 H2000 H12\nDELAY K200\nMOV D1076 D200\nEND").unwrap();
    let mut s = configured();
    s.memory.set_system(1143, true);
    s.memory.set_input(d("X0"), true);
    let bytes = vec![1, 6, 0x20, 0, 0, 0x12, 2, 7];
    s.memory
        .serial_io
        .queue_received(SerialPort::Com2, 15_000, bytes.clone())
        .unwrap();
    request(&mut s);
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.serial_io.outgoing_frames()[0].bytes, bytes);
    assert_eq!(s.memory.word(d("D1070")), 1);
    assert_eq!(s.memory.word(d("D1075")), 0x12);
    assert_eq!(s.memory.word(d("D200")), 2);
    assert!(s.memory.system_bit(1127));
}

#[test]
fn no_peer_modbus_times_out_without_mutating_payload_or_retrying_itself() {
    let p = compile_il("LD X0\nMODRD K1 H2102 K2\nDELAY K500\nEND").unwrap();
    let mut s = configured();
    s.memory.set_word(d("D1129"), 5);
    s.memory.set_word(d("D1050"), 777);
    s.memory.set_input(d("X0"), true);
    request(&mut s);
    scan(&mut s, &p, 0);
    assert!(s.memory.system_bit(1129));
    assert_eq!(s.memory.word(d("D1130")), 1);
    assert_eq!(s.memory.word(d("D1050")), 777);
    assert!(!s.memory.system_bit(1127));
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.serial_io.outgoing_frames().len(), 1);
}

#[test]
fn h17_descriptor_pairs_latch_destination_and_explicit_peer_writes_first() {
    // PDF520/523 descriptors, corrected independent LRC26 for1100/1000.
    let p = compile_il("LD X0\nMODRW K1 K23 D0 D10 D20\nEND").unwrap();
    let mut s = configured();
    for (name, value) in [
        ("D0", 0x1100),
        ("D1", 0x1000),
        ("D10", 3000),
        ("D11", 4000),
        ("D20", 2),
        ("D21", 2),
        ("D4000", 0x1770),
        ("D4001", 0x12),
    ] {
        s.memory.set_word(d(name), value);
    }
    s.memory.set_input(d("X0"), true);
    request(&mut s);
    scan(&mut s, &p, 0);
    let frame = s.memory.serial_io.outgoing_frames()[0].bytes.clone();
    assert_eq!(frame, b":01171100000210000002041770001226\r\n");
    let mut peer = VirtualModbusPeer::new(1).unwrap();
    peer.holding
        .extend([(0x1000, 0), (0x1001, 0), (0x1100, 0x0100), (0x1101, 0x1766)]);
    let reply = peer.respond(WireMode::Ascii, &frame).unwrap().unwrap();
    assert_eq!(peer.holding[&0x1000], 0x1770);
    assert_eq!(peer.holding[&0x1001], 0x12);
    assert_eq!(reply, b":0117040100176666\r\n");
    // Even pointers edited while busy cannot redirect this already-started RX.
    s.memory.set_word(d("D10"), 3500);
    s.memory.set_word(d("D4000"), 9);
    s.memory
        .serial_io
        .queue_received(SerialPort::Com2, 50_000, reply)
        .unwrap();
    advance(&mut s, 60_000);
    assert_eq!(s.memory.word(d("D3000")), 0x3130);
    assert_eq!(s.memory.word(d("D3500")), 0);
    assert_eq!(s.memory.word(d("D1296")), 0x0100);
    assert_eq!(s.memory.word(d("D1297")), 0x1766);
    assert!(s.memory.system_bit(1127));
}

#[test]
fn call_and_for_revisits_use_same_word_pc_and_do_not_duplicate_active_owner() {
    let p =
        compile_il("LD X0\nFOR K3\nCALL P0\nNEXT\nFEND\nP0\nLD X0\nRS D0 K1 D100 K1\nSRET\nEND")
            .unwrap();
    let layout = crate::es2_encoder::local_execution_layout(&p).unwrap();
    let mut s = configured();
    s.memory.set_input(d("X0"), true);
    request(&mut s);
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.serial_io.outgoing_frames().len(), 1);
    assert_eq!(
        s.memory.serial_io.outgoing_frames()[0].pc,
        layout.action_pc[0]
    );
    assert_eq!(
        s.memory.serial_io.transaction(SerialPort::Com2).unwrap().pc,
        layout.action_pc[0]
    );
}

#[test]
fn explicit_instruction_cost_services_receive_only_deadline_before_later_mov() {
    let p = compile_il("LD X0\nRS D0 K0 D100 K1\nFOR K8\nMOV K0 D200\nNEXT\nMOV D100 D201\nEND")
        .unwrap();
    let mut s = configured();
    s.memory.interrupts.header_instruction_cost_us = 100;
    s.memory.set_input(d("X0"), true);
    s.memory
        .serial_io
        .queue_received(SerialPort::Com2, 500, vec![0x66])
        .unwrap();
    request(&mut s);
    scan(&mut s, &p, 0);
    assert_eq!(s.memory.word(d("D201")), 0x66);
    assert!(s.memory.system_bit(1123));
}

#[test]
fn cpu_restart_preserves_explicit_port_but_drops_old_pending_bytes_and_frames() {
    let p = compile_il("LD X0\nRS D0 K1 D100 K1\nEND").unwrap();
    let mut s = configured();
    s.memory.set_input(d("X0"), true);
    request(&mut s);
    scan(&mut s, &p, 0);
    s.memory
        .serial_io
        .queue_received(SerialPort::Com2, 5_000, vec![0x77])
        .unwrap();
    s.restart_preserving_file_store();
    assert_eq!(s.memory.serial_io.configured_ports().count(), 1);
    assert_eq!(s.memory.serial_io.active_count(), 0);
    assert!(s.memory.serial_io.outgoing_frames().is_empty());
    advance(&mut s, 10_000);
    assert_eq!(s.memory.word(d("D100")), 0);
    assert!(!s.memory.system_bit(1123));
}
