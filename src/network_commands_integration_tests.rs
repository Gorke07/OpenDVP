//! VM/peer lifecycle checks for explicit DMV and CANopen virtual backends.
//! These are not installed simulator, UART, CAN-controller or servo parity.
use crate::canopen::{CanCpu, CanProfile, CanReply, CanServoModel};
use crate::dmv_comm::{DmvOperation, DmvReply, VirtualDmvPeer};
use crate::plc::{Device, Program, ScanStatus, Simulator, compile_il};
use crate::serial_io::{PortConfig, SerialCpu, SerialPort, SerialProfile};
fn d(s: &str) -> Device {
    s.parse().unwrap()
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
fn seed_dmv(s: &mut Simulator) {
    for (i, v) in [
        (0, 2),
        (1, 1),
        (4, 3),
        (5, 0),
        (6, 1),
        (7, 2),
        (8, 20),
        (1120, 0x86),
        (1129, 100),
    ] {
        s.memory.set_word(d(&format!("D{i}")), v);
    }
    s.memory.set_input(d("X20"), true);
}
fn dmv_sim() -> Simulator {
    let mut s = Simulator::default();
    s.memory
        .serial_io
        .set_profile(SerialProfile {
            cpu: SerialCpu::Ss2,
            firmware_major: 3,
            firmware_minor: 2,
            has_com3: false,
        })
        .unwrap();
    s.memory
        .serial_io
        .configure(SerialPort::Com2, PortConfig::rs485())
        .unwrap();
    s.memory
        .dmv_comm
        .configure_peer(SerialPort::Com2, 1, VirtualDmvPeer::default())
        .unwrap();
    s.memory
        .dmv_comm
        .peer_mut(SerialPort::Com2, 1)
        .unwrap()
        .output = vec![0x1234, 0xabcd];
    seed_dmv(&mut s);
    s
}
fn can_config(s: &mut Simulator) {
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
}
fn can_sim() -> Simulator {
    let mut s = Simulator::default();
    can_config(&mut s);
    s
}
fn ack(s: &mut Simulator) {
    let r = s.memory.canopen.pending().unwrap().clone();
    let at = s.memory.timing_io.now_us + 1;
    s.memory
        .canopen
        .queue_reply(at, r.id, r.epoch, CanReply::Ack)
        .unwrap();
    advance(s, 1);
}
fn finish_init(s: &mut Simulator) {
    for _ in 0..512 {
        if s.memory.canopen.pending().is_none() {
            return;
        }
        ack(s);
    }
    panic!("bounded initialization exceeded512actual replies");
}
#[test]
fn three_genuine_layouts_keep_nominal_pc_separate_from_native_fallbacks() {
    for (line, head, words) in [
        ("DMVRW D0 D4 D8 Y0", 0x6e02, 9),
        ("INITC K1", 0xaf02, 3),
        ("ASDON K1 K1", 0xb002, 5),
    ] {
        let p = compile_il(&format!("LD M1000\n{line}\nMOV K7 D200\nEND")).unwrap();
        let e = crate::es2_encoder::local_execution_layout(&p).unwrap();
        assert_eq!(e.known_word_at(1), Some(head));
        assert_eq!(e.action_pc, vec![1, 1 + words]);
        assert!(crate::es2_encoder::encode(&p).is_err());
    }
}
#[test]
fn dmv_k3_program20_trigger_read_runs_from_real_il_and_preserves_alias_until_read() {
    let p = compile_il("LD X20\nDMVRW D0 D4 D8 Y0\nEND").unwrap();
    let mut s = dmv_sim();
    scan(&mut s, &p);
    assert!(s.memory.bit(d("Y0")));
    assert!(!s.memory.bit(d("Y1")));
    advance(&mut s, 1000);
    assert_eq!(
        s.memory
            .dmv_comm
            .peer(SerialPort::Com2, 1)
            .unwrap()
            .selected_program,
        20
    );
    assert_eq!(s.memory.word(d("D8")), 20);
    advance(&mut s, 1000);
    assert_eq!(
        s.memory
            .dmv_comm
            .peer(SerialPort::Com2, 1)
            .unwrap()
            .trigger_count,
        1
    );
    assert_eq!(s.memory.word(d("D8")), 20);
    advance(&mut s, 1000);
    assert_eq!(s.memory.word(d("D8")) as u16, 0x1234);
    assert_eq!(s.memory.word(d("D9")) as u16, 0xabcd);
    assert!(s.memory.bit(d("Y1")));
    assert!(!s.memory.bit(d("Y0")));
    let ops = s
        .memory
        .dmv_comm
        .outgoing_requests()
        .iter()
        .map(|r| r.operation.clone())
        .collect::<Vec<_>>();
    assert!(matches!(
        ops.as_slice(),
        [
            DmvOperation::SelectProgram(20),
            DmvOperation::Trigger1,
            DmvOperation::ReadOutput { .. }
        ]
    ));
    scan(&mut s, &p);
    assert_eq!(
        s.memory
            .dmv_comm
            .peer(SerialPort::Com2, 1)
            .unwrap()
            .trigger_count,
        1
    );
}
#[test]
fn dmv_owner_blocks_rs_and_modrd_until_the_full_three_phase_job_releases_port() {
    for other in ["RS D30 K1 D40 K1", "MODRD K1 H1000 K2"] {
        let p = compile_il(&format!("LD X20\nDMVRW D0 D4 D8 Y0\n{other}\nEND")).unwrap();
        let mut s = dmv_sim();
        s.memory.set_word(d("D30"), 0x41);
        s.memory.set_system(1122, true);
        scan(&mut s, &p);
        assert!(
            s.memory
                .serial_io
                .external_owner(SerialPort::Com2)
                .is_some()
        );
        assert!(s.memory.serial_io.outgoing_frames().is_empty());
        advance(&mut s, 3000);
        assert_eq!(s.memory.serial_io.external_owner(SerialPort::Com2), None);
        s.memory.set_system(1122, true);
        scan(&mut s, &p);
        assert_eq!(s.memory.serial_io.outgoing_frames().len(), 1, "{other}");
    }
}
#[test]
fn initc_and_asdon_vm_need_all_actual_peer_acks_before_any_completion() {
    let p = compile_il("LD M1000\nSET M1614\nINITC K1\nLD M1615\nASDON K1 K1\nEND").unwrap();
    let mut s = can_sim();
    scan(&mut s, &p);
    assert!(!s.memory.system_bit(1615));
    assert!(!s.memory.system_bit(1640));
    ack(&mut s);
    assert!(!s.memory.system_bit(1615));
    finish_init(&mut s);
    assert!(s.memory.system_bit(1615));
    assert!(s.memory.canopen.node(1).unwrap().initialized);
    assert!(!s.memory.system_bit(1640));
    scan(&mut s, &p);
    assert!(s.memory.canopen.pending().is_some());
    assert!(!s.memory.system_bit(1640));
    ack(&mut s);
    assert!(s.memory.system_bit(1640));
    assert!(s.memory.canopen.node(1).unwrap().servo_enabled);
}
#[test]
fn cpu_restart_preserves_can_environment_but_old_epoch_reply_cannot_complete_new_job() {
    let p = compile_il("LD M1000\nSET M1614\nINITC K1\nEND").unwrap();
    let mut s = can_sim();
    scan(&mut s, &p);
    let old = s.memory.canopen.pending().unwrap().clone();
    s.restart_preserving_file_store();
    scan(&mut s, &p);
    let current = s.memory.canopen.pending().unwrap().clone();
    assert_ne!(old.epoch, current.epoch);
    s.memory
        .canopen
        .queue_reply(1, old.id, old.epoch, CanReply::Ack)
        .unwrap();
    advance(&mut s, 1);
    assert_eq!(s.memory.canopen.pending().unwrap().id, current.id);
    assert!(!s.memory.system_bit(1615));
    assert!(!s.memory.canopen.node(1).unwrap().initialized);
}
#[test]
fn cpu_restart_discards_dmv_queued_operations_and_releases_serial_owner() {
    let p = compile_il("LD X20\nDMVRW D0 D4 D8 Y0\nEND").unwrap();
    let mut s = dmv_sim();
    scan(&mut s, &p);
    assert!(
        s.memory
            .serial_io
            .external_owner(SerialPort::Com2)
            .is_some()
    );
    s.restart_preserving_file_store();
    assert_eq!(s.memory.serial_io.external_owner(SerialPort::Com2), None);
    assert_eq!(s.memory.dmv_comm.active_count(), 0);
    advance(&mut s, 10000);
    assert_eq!(
        s.memory
            .dmv_comm
            .peer(SerialPort::Com2, 1)
            .unwrap()
            .trigger_count,
        0
    );
    seed_dmv(&mut s);
    scan(&mut s, &p);
    advance(&mut s, 3000);
    assert_eq!(
        s.memory
            .dmv_comm
            .peer(SerialPort::Com2, 1)
            .unwrap()
            .trigger_count,
        1
    );
    assert_eq!(s.memory.word(d("D8")) as u16, 0x1234);
}
#[test]
fn vm_watchdog_abort_releases_dmv_owner_and_invalidates_pending_can_job() {
    let p=compile_il("LD M1000\nSET M1614\nINITC K1\nLD X20\nDMVRW D0 D4 D8 Y0\nDELAY K1000\nDELAY K1000\nDELAY K1\nEND").unwrap();
    let mut s = dmv_sim();
    can_config(&mut s);
    s.memory.set_word(d("D1129"), 32767);
    s.memory
        .dmv_comm
        .peer_mut(SerialPort::Com2, 1)
        .unwrap()
        .replies
        .push_back(DmvReply::Silent);
    s.scan(&p, 0);
    assert_ne!(s.last_scan, ScanStatus::Complete);
    assert!(s.memory.system_bit(1008));
    assert_eq!(s.memory.serial_io.external_owner(SerialPort::Com2), None);
    assert_eq!(s.memory.dmv_comm.active_count(), 0);
    assert!(s.memory.canopen.pending().is_none());
    assert!(!s.memory.system_bit(1615));
    assert!(!s.memory.system_bit(1640));
    advance(&mut s, 1000000);
    assert_eq!(
        s.memory
            .dmv_comm
            .peer(SerialPort::Com2, 1)
            .unwrap()
            .trigger_count,
        0
    );
    assert!(!s.memory.system_bit(1615));
}
