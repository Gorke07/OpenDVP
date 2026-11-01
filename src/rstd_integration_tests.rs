//! Real typed-VM RSTD execution with the genuine Delta editor BE02 codec.
//! ACKs are virtual CAN events; genuine encoding is not native or hardware proof.
use crate::canopen::{CanCpu, CanNmtState, CanOperation, CanProfile, CanReply, CanServoModel};
use crate::execution_plan::PlanOp;
use crate::plc::{Device, Program, ScanStatus, Simulator, compile_il};
fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn sim(cpu: CanCpu, major: u16, minor: u8) -> Simulator {
    let mut s = Simulator::default();
    s.memory.set_input(d("X0"), true);
    s.memory
        .canopen
        .set_profile(CanProfile {
            cpu,
            firmware_major: major,
            firmware_minor: minor,
            servo: CanServoModel::A2M,
        })
        .unwrap();
    s.memory.canopen.configure_node(2, true).unwrap();
    s.memory.set_word(d("D100"), 0x80);
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
fn reply(s: &mut Simulator, r: CanReply) {
    let q = s.memory.canopen.pending().unwrap().clone();
    let at = s.memory.timing_io.now_us + 1;
    s.memory.canopen.queue_reply(at, q.id, q.epoch, r).unwrap();
    crate::timing_io::advance_by(&mut s.memory, 1);
}
fn finish(s: &mut Simulator) {
    for _ in 0..512 {
        if s.memory.canopen.pending().is_none() {
            return;
        }
        reply(s, CanReply::Ack);
    }
    panic!("bounded512CANreplybudget exceeded");
}
fn p() -> Program {
    compile_il("LD X0\nRSTD K2 D100 M0 M1\nMOV K7 D200\nEND").unwrap()
}
#[test]
fn nine_word_genuine_interval_has_one_action_and_real_later_pc() {
    let q = p();
    let l = crate::es2_encoder::local_execution_layout(&q).unwrap();
    assert_eq!(l.word_count, 16);
    assert_eq!(l.action_pc, vec![1, 10]);
    assert!(l.unencoded_ranges.is_empty());
    assert_eq!(l.known_word_at(0), Some(0x0024));
    let expected = [
        0xbe02, 0x0280, 0x0080, 0xc886, 0x0086, 0x0084, 0x0884, 0x0184, 0x0884,
    ];
    for (offset, word) in expected.into_iter().enumerate() {
        assert_eq!(l.known_word_at(1 + offset as u16), Some(word));
    }
    assert!(l.known_word_at(10).is_some());
    assert_eq!(l.known_word_at(15), Some(0x0f00));
    let reset_actions = q
        .networks
        .iter()
        .flat_map(|n| &n.actions)
        .filter(|a| a.label().starts_with("RSTD"))
        .count();
    assert_eq!(reset_actions, 1);
    assert!(crate::es2_encoder::encode(&q).is_err());
}
#[test]
fn nmt_mode_runs_from_il_on_qualified_models_and_ok_waits_for_real_ack() {
    for (cpu, major, minor) in [(CanCpu::Es2C, 3, 60), (CanCpu::Se2, 1, 0)] {
        let mut s = sim(cpu, major, minor);
        scan(&mut s, &p());
        assert_eq!(s.memory.word(d("D200")), 7);
        assert!(!s.memory.system_bit(1614));
        assert!(!s.memory.bit(d("M0")));
        let pending = s.memory.canopen.pending().unwrap();
        assert_eq!(pending.pc, 1);
        assert_eq!(pending.node, 2);
        assert_eq!(pending.operation, CanOperation::Nmt(0x80));
        reply(&mut s, CanReply::Ack);
        assert!(s.memory.bit(d("M0")));
        assert!(!s.memory.bit(d("M1")));
        assert_eq!(
            s.memory.canopen.node(2).unwrap().nmt_state,
            Some(CanNmtState::PreOperational)
        );
    }
}
#[test]
fn call_for_revisits_one_actual_rstd_pc_without_duplicate_pending_requests() {
    let q =
        compile_il("LD X0\nFOR K3\nCALL P0\nNEXT\nFEND\nP0\nLD X0\nRSTD K2 D100 M0 M1\nSRET\nEND")
            .unwrap();
    let reset_pc = q
        .execution_plan
        .nodes
        .iter()
        .find_map(|n| match n.op {
            PlanOp::Action {
                network, action, ..
            } if q.networks[network].actions[action]
                .label()
                .starts_with("RSTD") =>
            {
                Some(n.pc)
            }
            _ => None,
        })
        .unwrap();
    let mut s = sim(CanCpu::Es2C, 3, 60);
    scan(&mut s, &q);
    assert_eq!(s.memory.canopen.outgoing_requests().len(), 1);
    assert_eq!(s.memory.canopen.pending().unwrap().pc, reset_pc);
    reply(&mut s, CanReply::Ack);
    scan(&mut s, &q);
    assert_eq!(s.memory.canopen.outgoing_requests().len(), 1);
    assert!(s.memory.bit(d("M0")));
}
#[test]
fn peer_fault_logs_literal_node_code_header_pc_and_error_without_ok() {
    let mut s = sim(CanCpu::Es2C, 3, 60);
    scan(&mut s, &p());
    reply(&mut s, CanReply::Fault { code: Some(0x1234) });
    assert!(s.memory.system_bit(1616));
    assert_eq!(s.memory.word(d("D6000")), 2);
    assert_eq!(s.memory.word(d("D6001")), 0x1234);
    assert_eq!(s.memory.word(d("D6002")), 1);
    assert!(s.memory.bit(d("M1")));
    assert!(!s.memory.bit(d("M0")));
}
#[test]
fn false_before_ack_suppresses_only_ok_and_keeps_actual_nmt_side_effect() {
    let q = p();
    let mut s = sim(CanCpu::Es2C, 3, 60);
    scan(&mut s, &q);
    s.memory.set_input(d("X0"), false);
    scan(&mut s, &q);
    assert!(s.memory.canopen.pending().is_some());
    reply(&mut s, CanReply::Ack);
    assert!(!s.memory.bit(d("M0")));
    assert!(!s.memory.bit(d("M1")));
    assert_eq!(
        s.memory.canopen.node(2).unwrap().nmt_state,
        Some(CanNmtState::PreOperational)
    );
}
#[test]
fn delta_reset_does_not_dereference_unused_para_beyond_current_model_limit() {
    let q = compile_il("LD M1000\nSET M1614\nINITC K2\nLD M1615\nAND X0\nRSTD K2 D9999 M0 M1\nEND")
        .unwrap();
    let mut s = sim(CanCpu::Es2C, 3, 60);
    s.memory.canopen.configure_node(1, true).unwrap();
    s.memory.data_register_limit = Some(6002);
    s.memory.set_input(d("X0"), false);
    scan(&mut s, &q);
    finish(&mut s);
    assert!(s.memory.system_bit(1615));
    assert!(s.memory.canopen.node(2).unwrap().initialized);
    s.memory.set_input(d("X0"), true);
    scan(&mut s, &q);
    assert_eq!(
        s.memory.canopen.pending().unwrap().operation,
        CanOperation::ResetDelta
    );
    assert!(!s.memory.bit(d("M0")));
    reply(&mut s, CanReply::Ack);
    assert!(!s.memory.canopen.node(2).unwrap().initialized);
    assert!(!s.memory.bit(d("M0")));
    finish(&mut s);
    assert!(s.memory.canopen.node(2).unwrap().initialized);
    assert!(s.memory.bit(d("M0")));
    assert!(!s.memory.bit(d("M1")));
}
#[test]
fn rstd_matrix_rejects_unlisted_forms_and_non_d_parameter_non_m_flags() {
    for line in [
        "RSTDP K2 D100 M0 M1",
        "DRSTD K2 D100 M0 M1",
        "RSTD K2 M100 M0 M1",
        "RSTD K2 D100 Y0 M1",
        "RSTD K2 D100 M0 S0",
        "RSTD K4M0 D100 M0 M1",
    ] {
        assert!(
            compile_il(&format!("LD X0\n{line}\nEND")).is_err(),
            "{line}"
        );
    }
}
