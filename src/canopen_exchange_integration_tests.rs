//! Compiled IL and the real scan/VM scheduler; no native CAN parity claim.
use crate::canopen::{CanCpu, CanProfile, CanServoModel};
use crate::canopen_exchange::{CanFrame, RawCanCpu, RawCanPort, SdoResult};
use crate::plc::{Device, Program, ScanStatus, Simulator, compile_il};
fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn sim() -> Simulator {
    let mut s = Simulator::default();
    s.memory.set_input(d("X20"), true);
    s.memory
        .canopen
        .set_profile(CanProfile {
            cpu: CanCpu::Es2C,
            firmware_major: 3,
            firmware_minor: 60,
            servo: CanServoModel::A2M,
        })
        .unwrap();
    s.memory.canopen.configure_node(1, true).unwrap();
    s.memory
        .can_exchange
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
    s.memory.set_word(d("D1177"), 200);
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
fn advance(s: &mut Simulator, delta: u64) {
    crate::timing_io::advance_by(&mut s.memory, delta);
}
fn ack(s: &mut Simulator, result: SdoResult) {
    let request = s.memory.can_exchange.pending_sdo().unwrap().clone();
    let at = s.memory.timing_io.now_us + 1;
    s.memory
        .can_exchange
        .queue_sdo_reply(at, request, result)
        .unwrap();
    advance(s, 1);
}

#[test]
fn compiled_layout_has_literal_known_headers_and_precise_instruction_intervals() {
    for (body, header, width) in [
        ("COPRW K1 H40 H220A K0 D0 M20", Some(0xc802), 13),
        ("CANRS K0 D0 D10 D20 M20", Some(0xc602), 11),
        ("COPWL K1 D0 K1 M20", Some(0xbc02), 9),
        ("DCOPWL K1 D0 K1 M20", Some(0xbd02), 17),
    ] {
        let p = compile_il(&format!("LD X20\n{body}\nMOV K7 D200\nEND")).unwrap();
        let layout = crate::es2_encoder::local_execution_layout(&p).unwrap();
        assert_eq!(layout.action_pc, vec![1, 1 + width]);
        assert_eq!(layout.known_word_at(1), header);
        assert!(layout.unencoded_ranges.is_empty());
        for pc in 1..1 + width {
            assert!(layout.known_word_at(pc).is_some(), "{body} word{pc}");
        }
        assert!(
            crate::es2_encoder::encode(&p).is_err(),
            "unverified native CAN backend must never be uploaded: {body}"
        );
    }
}
#[test]
fn compiled_coprw_read_waits_actual_reply_then_publishes_on_next_visit() {
    let p = compile_il("LD X20\nCOPRW K1 H40 H220A K0 D100 M200\nMOV K77 D201\nEND").unwrap();
    let mut s = sim();
    scan(&mut s, &p);
    assert_eq!(s.memory.word(d("D201")), 77);
    assert!(!s.memory.bit(d("M200")));
    assert_eq!(s.memory.can_exchange.pending_sdo().unwrap().index, 0x220a);
    ack(&mut s, SdoResult::Data(vec![0x34, 0x12]));
    assert_eq!(s.memory.word(d("D100")), 0x1234);
    assert!(!s.memory.bit(d("M200")));
    scan(&mut s, &p);
    assert!(s.memory.bit(d("M200")));
    assert!(s.memory.canopen.exchange_owner().is_none());
}
#[test]
fn compiled_wide_list_ack_does_not_advance_without_an_actual_instruction_visit() {
    let p = compile_il("LD X20\nDCOPWL K1 D10 K2 M200\nEND").unwrap();
    let mut s = sim();
    for (i, v) in [0x212c, 0, 0x1234, 0x5678, 0x212d, 0, 0x0102, 0x0304]
        .into_iter()
        .enumerate()
    {
        s.memory.set_word(d(&format!("D{}", 10 + i)), v as i16);
    }
    scan(&mut s, &p);
    assert_eq!(s.memory.can_exchange.outgoing_sdo().len(), 1);
    ack(&mut s, SdoResult::WriteAck);
    advance(&mut s, 100);
    assert_eq!(s.memory.can_exchange.outgoing_sdo().len(), 1);
    scan(&mut s, &p);
    assert_eq!(s.memory.can_exchange.outgoing_sdo().len(), 2);
    assert_eq!(
        s.memory.can_exchange.pending_sdo().unwrap().write,
        Some(vec![2, 1, 4, 3])
    );
    ack(&mut s, SdoResult::WriteAck);
    scan(&mut s, &p);
    assert!(s.memory.bit(d("M200")));
    assert!(s.memory.canopen.exchange_owner().is_some());
    s.memory.set_input(d("X20"), false);
    scan(&mut s, &p);
    assert!(!s.memory.bit(d("M200")));
    assert!(s.memory.canopen.exchange_owner().is_none());
}
#[test]
fn compiled_initc_and_exchange_cannot_steal_the_same_can_resource() {
    let p =
        compile_il("LD X20\nSET M1614\nINITC K1\nRST M1614\nCOPRW K1 H40 H220A K0 D100 M200\nEND")
            .unwrap();
    let mut s = sim();
    scan(&mut s, &p);
    assert!(s.memory.canopen.pending().is_some());
    assert!(s.memory.can_exchange.outgoing_sdo().is_empty());
    let p =
        compile_il("LD X20\nCOPRW K1 H40 H220A K0 D100 M200\nSET M1614\nINITC K1\nEND").unwrap();
    let mut s = sim();
    scan(&mut s, &p);
    assert!(s.memory.can_exchange.pending_sdo().is_some());
    assert!(s.memory.canopen.pending().is_none());
}
#[test]
fn cpu_restart_retains_can_environment_but_rejects_the_old_reply_epoch() {
    let p = compile_il("LD X20\nCOPRW K1 H40 H220A K0 D100 M200\nEND").unwrap();
    let mut s = sim();
    scan(&mut s, &p);
    let old = s.memory.can_exchange.pending_sdo().unwrap().clone();
    s.restart_preserving_file_store();
    assert!(s.memory.canopen.node(1).is_some());
    assert!(s.memory.can_exchange.pending_sdo().is_none());
    s.memory.set_input(d("X20"), true);
    scan(&mut s, &p);
    let current = s.memory.can_exchange.pending_sdo().unwrap().clone();
    assert_ne!(old.epoch, current.epoch);
    let at = s.memory.timing_io.now_us + 1;
    s.memory
        .can_exchange
        .queue_sdo_reply(at, old, SdoResult::Data(vec![99, 0]))
        .unwrap();
    advance(&mut s, 1);
    assert_eq!(s.memory.word(d("D100")), 0);
    assert_eq!(s.memory.can_exchange.pending_sdo().unwrap(), &current);
}
#[test]
fn compiled_canrs_latches_after_first_run_rung_and_requires_tx_then_real_rx() {
    let p = compile_il("LD M1002\nSET M1620\nLD X20\nCANRS K0 D0 D10 D20 M200\nMOV K55 D201\nEND")
        .unwrap();
    let mut s = sim();
    s.memory.set_word(d("D0"), 0x700);
    s.memory.set_word(d("D1"), 2);
    s.memory.set_word(d("D10"), 0xab11u16 as i16);
    s.memory.set_word(d("D11"), 0xcd22u16 as i16);
    s.memory.set_word(d("D20"), 0x709);
    scan(&mut s, &p);
    let tx = s
        .memory
        .can_exchange
        .outgoing_frames()
        .back()
        .unwrap()
        .clone();
    assert!(tx.frame.standard);
    assert_eq!(tx.frame.data, vec![0x11, 0x22]);
    assert!(!s.memory.bit(d("M200")));
    let at = s.memory.timing_io.now_us + 1;
    s.memory
        .can_exchange
        .queue_transmitted(at, tx.id, tx.epoch)
        .unwrap();
    advance(&mut s, 1);
    let at = s.memory.timing_io.now_us + 1;
    s.memory
        .can_exchange
        .queue_frame(
            at,
            0,
            CanFrame {
                id: 0x709,
                standard: true,
                data: vec![3, 4],
            },
        )
        .unwrap();
    advance(&mut s, 1);
    assert_eq!(s.memory.word(d("D22")), 3);
    scan(&mut s, &p);
    assert!(s.memory.bit(d("M200")));
    assert_eq!(s.memory.word(d("D201")), 55);
}
#[test]
fn compiled_coprw_in_called_subroutine_keeps_real_header_pc_for_fault_log() {
    let p =
        compile_il("LD X20\nCALL P1\nFEND\nP1\nLD X20\nCOPRW K1 H40 H220A K0 D100 M200\nSRET\nEND")
            .unwrap();
    let layout = crate::es2_encoder::local_execution_layout(&p).unwrap();
    let expected = layout
        .action_pc
        .iter()
        .copied()
        .find(|pc| layout.known_word_at(*pc) == Some(0xc802))
        .unwrap();
    let mut s = sim();
    scan(&mut s, &p);
    assert_eq!(s.memory.can_exchange.pending_sdo().unwrap().pc, expected);
    ack(
        &mut s,
        SdoResult::Abort {
            sdo_code: 0x06020000,
            plc_code: Some(88),
        },
    );
    assert_eq!(s.memory.word(d("D6002")) as u16, expected);
    assert_eq!(s.memory.word(d("D6001")), 88);
    assert!(!s.memory.bit(d("M200")));
}
