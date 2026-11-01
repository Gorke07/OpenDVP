use super::*;
fn profile() -> CanProfile {
    CanProfile {
        cpu: CanCpu::Es2C,
        firmware_major: 3,
        firmware_minor: 60,
        servo: CanServoModel::A2M,
    }
}
fn memory(count: u8) -> Memory {
    let mut memory = Memory::default();
    memory.canopen.set_profile(profile()).unwrap();
    for id in 1..=count {
        memory.canopen.configure_node(id, true).unwrap();
    }
    memory.set_system(1614, true);
    memory
}
fn action(name: &str, args: &[&str]) -> CanopenAction {
    CanopenAction::parse(
        name,
        &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn ack(memory: &mut Memory) {
    let r = memory.canopen.pending().unwrap().clone();
    let at = memory.canopen.now_us + 1;
    memory
        .canopen
        .queue_reply(at, r.id, r.epoch, CanReply::Ack)
        .unwrap();
    memory.timing_io.now_us = at;
    CanopenNetwork::advance(memory, at);
}
fn finish(memory: &mut Memory) {
    for _ in 0..512 {
        if memory.canopen.pending().is_none() {
            return;
        }
        ack(memory);
    }
    panic!("bounded init did not finish");
}

#[test]
fn genuine_constant_and_reset_forms_keep_their_distinct_headers() {
    let init = action("INITC", &["K3"]);
    assert_eq!(init.header(), Some(0xaf02));
    assert_eq!(init.words(), 3);
    let enable = action("ASDON", &["H2", "K-1"]);
    assert_eq!(enable.header(), Some(0xb002));
    assert_eq!(enable.words(), 5);
    assert!(matches!(
        enable,
        CanopenAction::ServoEnable {
            node: 2,
            enabled: true
        }
    ));
    for (name, args) in [
        ("INITC", vec!["D0"]),
        ("ASDON", vec!["K1", "D0"]),
        ("ASDONP", vec!["K1", "K1"]),
        ("RSTDP", vec!["K1", "D0", "M0", "M1"]),
    ] {
        assert!(
            CanopenAction::parse(
                name,
                &args.into_iter().map(String::from).collect::<Vec<_>>()
            )
            .is_err()
        );
    }
    let reset = CanopenAction::reset_unencoded(
        CalendarValue::parse("K1", false).unwrap(),
        d(0),
        m(0),
        m(1),
    )
    .unwrap();
    assert_eq!(reset.header(), Some(0xbe02));
    assert_eq!(reset.words(), 9);
    assert_eq!(reset, action("RSTD", &["K1", "D0", "M0", "M1"]));
}
#[test]
fn init_has_full_parameter_and_mapping_plan_but_no_success_without_all_replies() {
    let mut memory = memory(2);
    action("INITC", &["K2"]).execute(&mut memory, true, 10);
    assert!(!memory.system_bit(1615));
    assert!(!memory.canopen.node(1).unwrap().initialized);
    ack(&mut memory);
    assert!(!memory.system_bit(1615));
    assert_eq!(
        memory.canopen.node(1).unwrap().parameters.get("P2-30"),
        Some(&5)
    );
    finish(&mut memory);
    assert!(memory.system_bit(1615));
    for node in 1..=2 {
        let n = memory.canopen.node(node).unwrap();
        assert!(n.initialized);
        assert_eq!(n.parameters.get("P5-20"), Some(&1));
        assert_eq!(n.parameters.get("P5-35"), Some(&1));
        assert_eq!(n.parameters.get("P5-60"), Some(&1));
        assert_eq!(n.parameters.get("P5-75"), Some(&1));
        assert_eq!(n.parameters.get("P3-06"), Some(&0x100));
        assert_eq!(n.parameters.get("P2-36"), Some(&0x101));
        assert_eq!(n.pdo_maps[&1], vec!["P5-07", "P0-01", "P0-46", "P4-07"]);
        assert_eq!(n.objects[&0x1016], 200);
        assert_eq!(n.objects[&0x1017], 0);
    }
}
#[test]
fn firmware_qualified_legacy_plan_is_not_modern_plan() {
    let old = CanProfile {
        firmware_minor: 48,
        ..profile()
    };
    let ops = CanopenNetwork::initialization_plan(old).unwrap();
    assert!(ops.contains(&CanOperation::ObjectWrite {
        index: 0x1017,
        value: 200
    }));
    assert!(
        !ops.iter()
            .any(|o| matches!(o,CanOperation::ParameterWrite{parameter,..} if parameter=="P2-30"))
    );
    assert!(
        CanopenNetwork::initialization_plan(CanProfile {
            firmware_minor: 50,
            ..profile()
        })
        .is_err()
    );
    assert!(
        CanopenNetwork::initialization_plan(CanProfile {
            firmware_minor: 48,
            servo: CanServoModel::A3M,
            ..profile()
        })
        .is_err()
    );
}
#[test]
fn asdon_waits_for_real_initialized_node_and_status_feedback() {
    let mut memory = memory(1);
    let enable = action("ASDON", &["K1", "K2"]);
    memory.set_system(1615, true);
    enable.execute(&mut memory, true, 20);
    assert!(memory.canopen.pending().is_none());
    assert!(!memory.system_bit(1640));
    action("INITC", &["K1"]).execute(&mut memory, true, 10);
    finish(&mut memory);
    enable.execute(&mut memory, true, 20);
    assert!(!memory.system_bit(1640));
    ack(&mut memory);
    assert!(memory.system_bit(1640));
    action("ASDON", &["K1", "K0"]).execute(&mut memory, true, 30);
    ack(&mut memory);
    assert!(!memory.system_bit(1640));
}
#[test]
fn asdon_outside_nodes_has_no_documented_error_or_request() {
    let mut memory = memory(1);
    for node in ["K0", "K9"] {
        action("ASDON", &[node, "K1"]).execute(&mut memory, true, 20);
    }
    assert!(memory.canopen.outgoing_requests().is_empty());
    assert!(!memory.system_bit(1616));
}
#[test]
fn peer_fault_records_supplied_node_code_and_actual_header_pc() {
    let mut memory = memory(1);
    action("INITC", &["K1"]).execute(&mut memory, true, 41);
    let request = memory.canopen.pending().unwrap().clone();
    memory
        .canopen
        .queue_reply(
            1,
            request.id,
            request.epoch,
            CanReply::Fault { code: Some(0x1234) },
        )
        .unwrap();
    CanopenNetwork::advance(&mut memory, 1);
    assert!(memory.system_bit(1616));
    assert_eq!(memory.word(d(6000)), 1);
    assert_eq!(memory.word(d(6001)), 0x1234);
    assert_eq!(memory.word(d(6002)), 41);
    assert!(!memory.system_bit(1615));
}
#[test]
fn ds301_reset_uses_actual_parameter_code_and_false_suppresses_ok_only() {
    let mut memory = memory(1);
    memory.set_system(1614, false);
    memory.set_word(d(0), 0x80);
    let reset = CanopenAction::reset_unencoded(
        CalendarValue::parse("K1", false).unwrap(),
        d(0),
        m(20),
        m(21),
    )
    .unwrap();
    reset.execute(&mut memory, true, 51);
    assert_eq!(
        memory.canopen.pending().unwrap().operation,
        CanOperation::Nmt(0x80)
    );
    reset.execute(&mut memory, false, 51);
    ack(&mut memory);
    assert!(!memory.bit(m(20)));
    assert_eq!(
        memory.canopen.node(1).unwrap().nmt_state,
        Some(CanNmtState::PreOperational)
    );
}
#[test]
fn invalid_ds301_code_sets_error_without_fabricated_log_code() {
    let mut memory = memory(1);
    memory.set_system(1614, false);
    memory.set_word(d(0), 3);
    memory.set_word(d(6001), 99);
    let reset = CanopenAction::reset_unencoded(
        CalendarValue::parse("K1", false).unwrap(),
        d(0),
        m(20),
        m(21),
    )
    .unwrap();
    reset.execute(&mut memory, true, 51);
    assert!(memory.bit(m(21)));
    assert_eq!(memory.word(d(6001)), 99);
    assert!(memory.canopen.pending().is_none());
}
#[test]
fn first_owner_is_preserved_and_old_epoch_reply_cannot_initialize_new_job() {
    let mut memory = memory(1);
    let init = action("INITC", &["K1"]);
    init.execute(&mut memory, true, 40);
    action("INITC", &["K1"]).execute(&mut memory, true, 20);
    let old = memory.canopen.pending().unwrap().clone();
    assert_eq!(old.pc, 40);
    memory.canopen.reset_instruction_state();
    init.execute(&mut memory, true, 40);
    let current = memory.canopen.pending().unwrap().id;
    memory
        .canopen
        .queue_reply(1, old.id, old.epoch, CanReply::Ack)
        .unwrap();
    CanopenNetwork::advance(&mut memory, 1);
    assert_eq!(memory.canopen.pending().unwrap().id, current);
    assert!(!memory.system_bit(1615));
}
#[test]
fn missing_node_preflight_does_not_start_partial_initialization() {
    let mut memory = memory(1);
    action("INITC", &["K2"]).execute(&mut memory, true, 10);
    assert!(memory.canopen.outgoing_requests().is_empty());
    assert!(memory.canopen.node(1).unwrap().parameters.is_empty());
}
