use super::*;
fn device(s: &str) -> Device {
    s.parse().unwrap()
}
fn action(name: &str, args: &[&str]) -> ExtendedMotionAction {
    ExtendedMotionAction::parse(
        name,
        &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn memory() -> Memory {
    let mut m = Memory::default();
    for (index, value) in [
        (1131, 100),
        (1132, 100),
        (1340, 100),
        (1352, 100),
        (1343, 100),
        (1353, 100),
    ] {
        m.set_word(d(Area::D, index), value);
    }
    m
}
fn table(m: &mut Memory, base: u16, control: u16, parts: &[(i32, i32)]) {
    m.set_word(d(Area::D, control), parts.len() as i16);
    for (i, (hz, n)) in parts.iter().enumerate() {
        m.set_double_word(d(Area::D, base + 4 * i as u16), *hz);
        m.set_double_word(d(Area::D, base + 4 * i as u16 + 2), *n);
    }
}
fn advance(m: &mut Memory, target: u64) {
    // Same ordering required of the shared scheduler: split at both deadline sets.
    let mut guard = 0;
    while m.pulse_output.now_us < target {
        guard += 1;
        assert!(guard < 10000);
        let now = m.pulse_output.now_us;
        let next = m
            .pulse_output
            .next_deadline_us()
            .into_iter()
            .chain(m.motion_extended.next_deadline_us())
            .filter(|t| *t > now)
            .min()
            .unwrap_or(target)
            .min(target);
        PulseState::advance(m, next);
        MotionExtendedState::advance(m, next);
    }
}
#[test]
fn exact_two_segments_and_single_pin_ownership() {
    let mut m = memory();
    table(&mut m, 0, 300, &[(1000, 2), (2000, 2)]);
    let a = action("DPTPO", &["D0", "D300", "Y0"]);
    a.execute(&mut m, true, 7);
    assert!(m.pulse_output.claimed_outputs().contains_key(&device("Y0")));
    assert!(!m.pulse_output.claimed_outputs().contains_key(&device("Y1")));
    advance(&mut m, 3000);
    assert_eq!(m.pulse_output.axis_snapshot(7).unwrap().position, 4);
    assert!(m.system_bit(1029));
    assert_eq!(m.word(device("D301")), 1);
    a.execute(&mut m, true, 7);
    assert_eq!(m.word(device("D301")), 2);
}
#[test]
fn table_partial_bounds_atomic_no_owner() {
    let mut m = memory();
    m.data_register_limit = Some(4999);
    m.set_word(device("D100"), 2);
    m.set_double_word(device("D4996"), 1000);
    m.set_double_word(device("D4998"), 2);
    let a = action("DPTPO", &["D4996", "D100", "Y0"]);
    a.execute(&mut m, true, 9);
    assert!(m.pulse_output.axis_snapshot(9).is_none());
    assert_eq!(m.word(device("D101")), 0);
    assert!(!m.execution_limitations.is_empty());
}
#[test]
fn table_false_releases_and_held_does_not_restart() {
    let mut m = memory();
    table(&mut m, 0, 100, &[(1000, 1)]);
    let a = action("DPTPO", &["D0", "D100", "Y0"]);
    a.execute(&mut m, true, 1);
    advance(&mut m, 1000);
    a.execute(&mut m, true, 1);
    assert_eq!(m.pulse_output.axis_snapshot(1).unwrap().position, 1);
    a.execute(&mut m, false, 1);
    assert!(m.pulse_output.axis_owner(0).is_none());
    a.execute(&mut m, true, 1);
    advance(&mut m, 2000);
    assert_eq!(m.pulse_output.axis_snapshot(1).unwrap().position, 2);
}
#[test]
fn table_monitor_updates_only_on_visit() {
    let mut m = memory();
    table(&mut m, 0, 100, &[(1000, 1), (1000, 10)]);
    let a = action("DPTPO", &["D0", "D100", "Y0"]);
    a.execute(&mut m, true, 3);
    advance(&mut m, 1000);
    assert_eq!(m.word(device("D101")), 1);
    a.execute(&mut m, true, 3);
    assert_eq!(m.word(device("D101")), 2);
}
#[test]
fn table_cyclic_latched_parameters_by_default() {
    let mut m = memory();
    m.set_system(1262, true);
    table(&mut m, 0, 100, &[(1000, 2)]);
    let a = action("DPTPO", &["D0", "D100", "Y0"]);
    a.execute(&mut m, true, 2);
    m.set_double_word(device("D0"), 2000);
    advance(&mut m, 4000);
    assert_eq!(m.pulse_output.axis_snapshot(2).unwrap().position, 4);
    assert!(!m.system_bit(1029));
}
#[test]
fn table_pause_preserves_remaining_count() {
    let mut m = memory();
    table(&mut m, 0, 100, &[(1000, 10)]);
    let a = action("DPTPO", &["D0", "D100", "Y0"]);
    a.execute(&mut m, true, 2);
    advance(&mut m, 2000);
    m.set_system(1078, true);
    MotionExtendedState::service_controls(&mut m);
    advance(&mut m, 12000);
    assert_eq!(m.pulse_output.axis_snapshot(2).unwrap().position, 2);
    m.set_system(1078, false);
    MotionExtendedState::service_controls(&mut m);
    advance(&mut m, 20000);
    assert_eq!(m.pulse_output.axis_snapshot(2).unwrap().position, 10);
}
#[test]
fn variable_startup_published_staircase() {
    let mut m = memory();
    m.set_double_word(device("D0"), 6000);
    m.set_word(device("D20"), 1000);
    m.set_word(device("D21"), 10);
    let a = action("DVSPO", &["D0", "K0", "D20", "Y0"]);
    a.execute(&mut m, true, 6);
    assert_eq!(
        m.pulse_output.axis_snapshot(6).unwrap().frequency_millihz,
        1_000_000
    );
    advance(&mut m, 50000);
    assert_eq!(
        m.pulse_output.axis_snapshot(6).unwrap().frequency_millihz,
        6_000_000
    );
}
#[test]
fn variable_gap_change_alone_is_ignored() {
    let mut m = memory();
    m.set_double_word(device("D0"), 6000);
    m.set_word(device("D20"), 1000);
    m.set_word(device("D21"), 10);
    let a = action("DVSPO", &["D0", "K0", "D20", "Y0"]);
    a.execute(&mut m, true, 6);
    m.set_word(device("D20"), 5000);
    a.execute(&mut m, true, 6);
    advance(&mut m, 10000);
    assert_eq!(
        m.pulse_output.axis_snapshot(6).unwrap().frequency_millihz,
        2_000_000
    );
}
#[test]
fn variable_finite_target_latched() {
    let mut m = memory();
    m.set_double_word(device("D0"), 1000);
    m.set_double_word(device("D10"), 3);
    m.set_word(device("D20"), 1000);
    m.set_word(device("D21"), 1);
    let a = action("DVSPO", &["D0", "D10", "D20", "Y0"]);
    a.execute(&mut m, true, 6);
    m.set_double_word(device("D10"), 100);
    a.execute(&mut m, true, 6);
    advance(&mut m, 10000);
    assert_eq!(m.pulse_output.axis_snapshot(6).unwrap().position, 3);
    assert!(m.system_bit(1029));
}
#[test]
fn dicf_immediate_first_step_vs_visit_wait() {
    let mut m = memory();
    m.set_double_word(device("D0"), 1000);
    m.set_word(device("D20"), 1000);
    m.set_word(device("D21"), 10);
    let a = action("DVSPO", &["D0", "K0", "D20", "Y0"]);
    a.execute(&mut m, true, 6);
    m.set_double_word(device("D0"), 6000);
    a.execute(&mut m, true, 6);
    assert_eq!(
        m.pulse_output.axis_snapshot(6).unwrap().frequency_millihz,
        1_000_000
    );
    m.set_double_word(device("D0"), 7000);
    assert!(MotionExtendedState::change_frequency(
        &mut m,
        device("D0"),
        &CalendarValue::parse("D20", true).unwrap(),
        device("Y0"),
        0
    ));
    assert_eq!(
        m.pulse_output.axis_snapshot(6).unwrap().frequency_millihz,
        2_000_000
    );
    assert!(!MotionExtendedState::change_frequency(
        &mut m,
        device("D2"),
        &CalendarValue::parse("D20", true).unwrap(),
        device("Y0"),
        0
    ));
}
#[test]
fn closed_waits_for_real_dispatch_not_raw_edge() {
    let mut m = memory();
    let a = action("DCLLM", &["X4", "K2", "K100", "Y0"]);
    a.execute(&mut m, true, 4);
    advance(&mut m, 100000);
    assert!(!m.system_bit(1029));
    MotionExtendedState::input_edge(&mut m, device("X4"), false, true, 100000);
    assert!(!m.system_bit(1029));
    MotionExtendedState::feedback_interrupt(&mut m, "I401", 100000);
    assert!(m.system_bit(1029));
}
#[test]
fn closed_counter_irq_checks_selected_counter_target() {
    let mut m = memory();
    let a = action("DCLLM", &["C243", "K50", "K1000", "Y0"]);
    a.execute(&mut m, true, 4);
    m.set_double_word(device("C244"), 100);
    MotionExtendedState::feedback_interrupt(&mut m, "I010", 0);
    assert!(!m.system_bit(1029));
    m.set_double_word(device("C243"), 50);
    MotionExtendedState::feedback_interrupt(&mut m, "I010", 0);
    assert!(m.system_bit(1029));
}
#[test]
fn closed_latched_negative_direction_survives_parameter_edit() {
    let mut m = memory();
    m.set_double_word(device("D20"), -2);
    let a = action("DCLLM", &["X4", "D20", "K100", "Y0"]);
    a.execute(&mut m, true, 4);
    m.set_double_word(device("D20"), 2);
    advance(&mut m, 100000);
    assert!(m.bit(device("Y1")));
    MotionExtendedState::feedback_interrupt(&mut m, "I401", 100000);
    assert!(m.bit(device("Y1")));
    a.execute(&mut m, false, 4);
    assert!(!m.bit(device("Y1")));
}
#[test]
fn reset_clock_keeps_explicit_profile_but_drops_running_owners() {
    let mut m = memory();
    m.motion_extended.cyclic_table_edits = true;
    m.motion_extended.now_us = 200;
    m.motion_extended.reset_instruction_state();
    assert_eq!(m.motion_extended.now_us, 0);
    assert!(m.motion_extended.cyclic_table_edits);
}
#[test]
fn parse_rejects_undocumented_forms_and_columns() {
    for (name, args) in [
        ("DPTPOP", vec!["D0", "D100", "Y0"]),
        ("PTPO", vec!["D0", "D100", "Y0"]),
        ("DPTPO", vec!["K0", "D100", "Y0"]),
        ("DCLLM", vec!["X0", "K1", "K1000", "Y0"]),
        ("DVSPO", vec!["E0", "K1", "D20", "Y0"]),
    ] {
        assert!(
            ExtendedMotionAction::parse(
                name,
                &args.iter().map(|s| s.to_string()).collect::<Vec<_>>()
            )
            .is_err()
        );
    }
}
#[test]
fn delayed_dispatch_keeps_the_capture_time_mask_decision() {
    let mut m = memory();
    let id: crate::interrupt::InterruptId = "I401".parse().unwrap();
    m.interrupts
        .configure(BTreeMap::from([(id, 100)]), 0)
        .unwrap();
    m.interrupts.enabled = true;
    m.set_system(1156, true);
    m.set_double_word(device("D1026"), 10);
    let a = action("DCLLM", &["X4", "K10000", "K100", "Y0"]);
    a.execute(&mut m, true, 8);
    advance(&mut m, 10000);
    m.interrupts
        .input_edge(device("X4"), false, true, 10000)
        .unwrap();
    MotionExtendedState::input_edge(&mut m, device("X4"), false, true, 10000);
    // Interrupt stays pending while pulse count passes the front mask.
    advance(&mut m, 200000);
    m.interrupts.advance_to(200000).unwrap();
    assert!(m.interrupts.dispatch([false; 10]).is_some());
    MotionExtendedState::feedback_interrupt(&mut m, "I401", 200000);
    assert!(!m.system_bit(1029));
    m.interrupts.finish_handler().unwrap();
    // A genuinely later unmasked event can finish at its dispatch time.
    m.interrupts
        .input_edge(device("X4"), false, true, 200000)
        .unwrap();
    MotionExtendedState::input_edge(&mut m, device("X4"), false, true, 200000);
    assert!(m.interrupts.dispatch([false; 10]).is_some());
    MotionExtendedState::feedback_interrupt(&mut m, "I401", 200000);
    assert!(m.system_bit(1029));
}
