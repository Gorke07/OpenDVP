use super::*;
fn c(n: u16) -> Device {
    dev(Area::C, n)
}
fn action(n: u16, p: i32) -> CounterAction {
    CounterAction {
        counter: c(n),
        preset: CounterPreset::Constant(p),
    }
}
fn arm(m: &mut Memory, n: u16, p: i32) {
    action(n, p).execute(m, true, n);
}
fn edge(m: &mut Memory, p: u16, old: bool, new: bool, t: u64) {
    CounterState::input_edge(m, dev(Area::X, p), old, new, t);
}
fn pulse(m: &mut Memory, p: u16, t: u64) {
    edge(m, p, false, true, t);
    edge(m, p, true, false, t + 1);
}

#[test]
fn parser_accepts_full_signed_preset_and_rejects_nonmatrix() {
    for p in ["K-2147483648", "K2147483647", "D9998"] {
        assert!(CounterAction::parse("DCNT", &["C254".into(), p.into()]).is_ok());
    }
    for (n, p) in [
        ("C199", "K1"),
        ("C255", "K1"),
        ("C200", "H1"),
        ("C200", "C200"),
        ("C200", "D9999"),
        ("C200", "K2147483648"),
    ] {
        assert!(CounterAction::parse("DCNT", &[n.into(), p.into()]).is_err());
    }
    assert!(CounterAction::parse("DCNTP", &["C200".into(), "K1".into()]).is_err());
}
#[test]
fn general_signed_direction_continues_and_wraps() {
    let mut m = Memory::default();
    let a = action(200, 2);
    a.execute(&mut m, true, 10);
    a.execute(&mut m, true, 10);
    assert_eq!(m.double_word(c(200)), 1);
    a.execute(&mut m, false, 10);
    a.execute(&mut m, true, 10);
    assert!(m.bit(c(200)));
    a.execute(&mut m, false, 10);
    a.execute(&mut m, true, 10);
    assert_eq!(m.double_word(c(200)), 3);
    m.set_system(1200, true);
    a.execute(&mut m, false, 10);
    a.execute(&mut m, true, 10);
    assert!(m.bit(c(200)));
    a.execute(&mut m, false, 10);
    a.execute(&mut m, true, 10);
    assert!(!m.bit(c(200)));
    m.set_double_word(c(200), i32::MIN);
    a.execute(&mut m, false, 10);
    a.execute(&mut m, true, 10);
    assert_eq!(m.double_word(c(200)), i32::MAX);
}
#[test]
fn reset_preserves_held_edge_and_negative_preset_contact_off() {
    let mut m = Memory::default();
    let a = action(200, -5);
    a.execute(&mut m, true, 1);
    CounterState::note_reset(&mut m, c(200));
    a.execute(&mut m, true, 1);
    assert_eq!(m.double_word(c(200)), 0);
    assert!(!m.bit(c(200)));
    CounterState::begin_scan(&mut m);
    a.execute(&mut m, true, 1);
    assert!(!m.bit(c(200)));
    a.execute(&mut m, false, 1);
    a.execute(&mut m, true, 1);
    assert!(m.bit(c(200)));
}
#[test]
fn dynamic_preset_pair_and_model_span_guard() {
    let mut m = Memory::default();
    let a = CounterAction {
        counter: c(200),
        preset: CounterPreset::Data(dev(Area::D, 10)),
    };
    m.set_double_word(dev(Area::D, 10), 70000);
    m.set_double_word(c(200), 69999);
    a.execute(&mut m, true, 1);
    assert!(m.bit(c(200)));
    m.data_register_limit = Some(4999);
    let bad = CounterAction {
        counter: c(201),
        preset: CounterPreset::Data(dev(Area::D, 4999)),
    };
    bad.execute(&mut m, true, 2);
    assert_eq!(m.double_word(c(201)), 0);
    assert!(!m.execution_limitations.is_empty());
}
#[test]
fn physical_single_edge_direction_disable_and_input_image_are_independent() {
    let mut m = Memory::default();
    arm(&mut m, 235, 2);
    assert_eq!(m.double_word(c(235)), 0);
    pulse(&mut m, 0, 10);
    pulse(&mut m, 0, 20);
    assert_eq!(m.double_word(c(235)), 2);
    assert!(m.bit(c(235)));
    assert!(!m.bit(dev(Area::X, 0)));
    m.set_system(1235, true);
    pulse(&mut m, 0, 30);
    assert_eq!(m.double_word(c(235)), 1);
    assert!(!m.bit(c(235)));
    action(235, 2).execute(&mut m, false, 235);
    pulse(&mut m, 0, 40);
    assert_eq!(m.double_word(c(235)), 1);
    arm(&mut m, 235, 2);
    pulse(&mut m, 0, 50);
    assert_eq!(m.double_word(c(235)), 0);
}
#[test]
fn falling_edge_and_false_enable_does_not_count() {
    let mut m = Memory::default();
    m.set_system(1270, true);
    arm(&mut m, 235, 1);
    edge(&mut m, 0, false, true, 1);
    assert_eq!(m.double_word(c(235)), 0);
    edge(&mut m, 0, true, false, 2);
    assert_eq!(m.double_word(c(235)), 1);
}
#[test]
fn ss2_c232_is_general_es2_c232_is_quadrature() {
    let mut es = Memory::default();
    let mut ss = Memory::default();
    ss.counters.model = CounterModel::Ss2;
    arm(&mut es, 232, 1);
    arm(&mut ss, 232, 1);
    assert_eq!(es.double_word(c(232)), 0);
    assert_eq!(ss.double_word(c(232)), 1);
    edge(&mut es, 0, false, true, 1);
    assert_eq!(es.double_word(c(232)), 1);
    assert!(!es.system_bit(1232));
}
#[test]
fn quadrature_waveform_modes_and_direction_monitor() {
    for (mode, expected) in [(1, 1), (2, 2), (4, 4), (0, 4)] {
        let mut m = Memory::default();
        m.set_word(dev(Area::D, 1022), mode);
        arm(&mut m, 251, 1);
        for (p, o, n, t) in [
            (0, false, true, 1),
            (1, false, true, 2),
            (0, true, false, 3),
            (1, true, false, 4),
        ] {
            edge(&mut m, p, o, n, t);
        }
        assert_eq!(m.double_word(c(251)), expected);
        assert!(!m.system_bit(1251));
        for (p, o, n, t) in [
            (1, false, true, 5),
            (0, false, true, 6),
            (1, true, false, 7),
            (0, true, false, 8),
        ] {
            edge(&mut m, p, o, n, t);
        }
        assert_eq!(m.double_word(c(251)), 0);
        assert!(m.system_bit(1251));
    }
}
#[test]
fn hardware_direction_dual_and_up_only_wrap() {
    let mut m = Memory::default();
    arm(&mut m, 245, 1);
    pulse(&mut m, 0, 1);
    edge(&mut m, 1, false, true, 3);
    pulse(&mut m, 0, 4);
    assert_eq!(m.double_word(c(245)), 0);
    assert!(m.system_bit(1245));
    let mut d = Memory::default();
    arm(&mut d, 247, 1);
    pulse(&mut d, 0, 1);
    pulse(&mut d, 1, 3);
    assert_eq!(d.double_word(c(247)), 0);
    let mut u = Memory::default();
    arm(&mut u, 243, 1);
    u.set_double_word(c(243), i32::MAX);
    pulse(&mut u, 0, 1);
    assert_eq!(u.double_word(c(243)), 0);
}
#[test]
fn default_reset_level_disable_and_interrupt_capture_order() {
    let mut m = Memory::default();
    arm(&mut m, 243, 1);
    pulse(&mut m, 0, 1);
    edge(&mut m, 1, false, true, 3);
    assert_eq!(m.double_word(c(243)), 0);
    pulse(&mut m, 0, 4);
    assert_eq!(m.double_word(c(243)), 0);
    let mut disabled = Memory::default();
    disabled.set_system(1243, true);
    arm(&mut disabled, 243, 1);
    edge(&mut disabled, 1, false, true, 1);
    pulse(&mut disabled, 0, 2);
    assert_eq!(disabled.double_word(c(243)), 1);
    let mut irq = Memory::default();
    CounterState::set_interrupt_reset(&mut irq, 4, Some(false));
    arm(&mut irq, 248, 1);
    pulse(&mut irq, 0, 1);
    pulse(&mut irq, 0, 3);
    edge(&mut irq, 4, false, true, 5);
    assert_eq!(irq.double_word(dev(Area::D, 1240)), 2);
    assert_eq!(irq.double_word(c(248)), 0);
    assert!(!irq.bit(c(248)));
}
#[test]
fn global_reset_shares_x7_but_count_pin_conflict_is_reported() {
    let mut m = Memory::default();
    m.set_system(1260, true);
    arm(&mut m, 235, 1);
    arm(&mut m, 236, 1);
    pulse(&mut m, 0, 1);
    pulse(&mut m, 1, 3);
    edge(&mut m, 7, false, true, 5);
    assert_eq!(m.double_word(c(235)), 0);
    assert_eq!(m.double_word(c(236)), 0);
    assert!(m.execution_limitations.is_empty());
    arm(&mut m, 242, 1);
    assert!(!m.execution_limitations.is_empty());
    assert!(!m.counters.active_counters().any(|c| c == 242));
}
#[test]
fn ownership_same_counter_and_csfo_conflicts_hold() {
    let mut m = Memory::default();
    arm(&mut m, 235, 1);
    action(235, 1).execute(&mut m, true, 999);
    assert!(!m.execution_limitations.is_empty());
    let mut other = Memory::default();
    other
        .timing_io
        .input_owners
        .insert(dev(Area::X, 0), ResourceOwner::Csfo(3));
    arm(&mut other, 235, 1);
    assert_eq!(other.counters.active_counters().count(), 0);
    action(235, 1).execute(&mut m, false, 235);
    assert!(!m.timing_io.input_owners.contains_key(&dev(Area::X, 0)));
}
#[test]
fn clock_order_stop_and_trace_bound() {
    let mut m = Memory::default();
    arm(&mut m, 235, 1);
    pulse(&mut m, 0, 10);
    edge(&mut m, 0, false, true, 1);
    assert_eq!(m.double_word(c(235)), 1);
    assert!(!m.execution_limitations.is_empty());
    CounterState::set_running(&mut m, false);
    pulse(&mut m, 0, 20);
    assert_eq!(m.double_word(c(235)), 1);
    CounterState::set_running(&mut m, true);
    for i in 0..300 {
        pulse(&mut m, 0, 30 + i * 2);
    }
    assert_eq!(m.counters.trace.len(), 256);
}
#[test]
fn installed_profile_exact_match_latch_and_highspeed_no_fabricated_events() {
    let mut m = Memory::default();
    m.counters.profile = CounterProfile::InstalledLegacy;
    let a = action(232, 3);
    a.execute(&mut m, true, 1);
    assert_eq!(m.double_word(c(232)), 1);
    m.set_double_word(c(232), 10);
    a.execute(&mut m, false, 1);
    a.execute(&mut m, true, 1);
    assert!(!m.bit(c(232)));
    arm(&mut m, 235, 1);
    pulse(&mut m, 0, 1);
    assert_eq!(m.double_word(c(235)), 0);
}
#[test]
fn enable_while_input_high_is_not_a_synthetic_edge() {
    let mut m = Memory::default();
    CounterState::seed_inputs(
        &mut m,
        [true, false, false, false, false, false, false, false],
    );
    arm(&mut m, 235, 1);
    assert_eq!(m.double_word(c(235)), 0);
    edge(&mut m, 0, true, false, 1);
    edge(&mut m, 0, false, true, 2);
    assert_eq!(m.double_word(c(235)), 1);
}
#[test]
fn selected_falling_reset_and_dynamic_global_collision_hold() {
    let mut m = Memory::default();
    m.counters.hardware_reset = HardwareResetPolicy::SelectedEdge;
    m.set_system(1274, true);
    arm(&mut m, 248, 1);
    pulse(&mut m, 0, 1);
    edge(&mut m, 4, false, true, 3);
    assert_eq!(m.double_word(c(248)), 1);
    edge(&mut m, 4, true, false, 4);
    assert_eq!(m.double_word(c(248)), 0);
    let mut conflict = Memory::default();
    arm(&mut conflict, 235, 1);
    arm(&mut conflict, 242, 1);
    conflict.set_system(1260, true);
    pulse(&mut conflict, 0, 1);
    assert_eq!(conflict.double_word(c(235)), 0);
    assert!(!conflict.execution_limitations.is_empty());
}
#[test]
fn highspeed_signed_wrap_has_immediate_signed_contact() {
    let mut m = Memory::default();
    arm(&mut m, 235, -10);
    m.set_double_word(c(235), i32::MAX);
    pulse(&mut m, 0, 1);
    assert_eq!(m.double_word(c(235)), i32::MIN);
    assert!(!m.bit(c(235)));
    m.set_system(1235, true);
    pulse(&mut m, 0, 3);
    assert_eq!(m.double_word(c(235)), i32::MAX);
    assert!(m.bit(c(235)));
}
#[test]
fn false_nonowner_site_does_not_disarm_or_release_active_owner() {
    let mut m = Memory::default();
    arm(&mut m, 235, 1);
    action(235, 1).execute(&mut m, false, 999);
    pulse(&mut m, 0, 1);
    assert_eq!(m.double_word(c(235)), 1);
    assert!(m.timing_io.input_owners.contains_key(&dev(Area::X, 0)));
    action(235, 1).execute(&mut m, true, 999);
    assert!(!m.execution_limitations.is_empty());
    action(235, 1).execute(&mut m, false, 235);
    assert_eq!(m.counters.active_counters().count(), 0);
}
#[test]
fn standalone_dual_reset_edge_mask_captures_both_edges() {
    let mut m = Memory::default();
    CounterState::set_interrupt_reset_edges(&mut m, 4, 3);
    arm(&mut m, 248, 1);
    pulse(&mut m, 0, 1);
    edge(&mut m, 4, false, true, 3);
    assert_eq!(m.double_word(dev(Area::D, 1240)), 1);
    pulse(&mut m, 0, 4);
    pulse(&mut m, 0, 6);
    edge(&mut m, 4, true, false, 8);
    assert_eq!(m.double_word(dev(Area::D, 1240)), 2);
    assert_eq!(m.double_word(c(248)), 0);
}
#[test]
fn configured_empty_interrupt_map_clears_stale_manual_capture_priority() {
    let mut m = Memory::default();
    CounterState::set_interrupt_reset_edges(&mut m, 1, 3);
    m.interrupts.configure(BTreeMap::new(), 0).unwrap();
    arm(&mut m, 243, 1);
    m.set_double_word(dev(Area::D, 1240), 777);
    pulse(&mut m, 0, 1);
    edge(&mut m, 1, false, true, 3);
    assert_eq!(m.double_word(c(243)), 0);
    assert_eq!(m.double_word(dev(Area::D, 1240)), 777);
    pulse(&mut m, 0, 4);
    assert_eq!(m.double_word(c(243)), 0);
}

#[test]
fn quadrature_default_matches_original_d1022_and_explicit_zero_is_retained() {
    let mut m = Memory::default();
    assert_eq!(m.word(dev(Area::D, 1022)), 4);
    m.set_word(dev(Area::D, 1022), 0);
    assert_eq!(m.word(dev(Area::D, 1022)), 0);
    // All four A/B transitions still count under the documented "other" mode.
    arm(&mut m, 251, 10);
    for (pin, old, new, t) in [
        (0, false, true, 1),
        (1, false, true, 2),
        (0, true, false, 3),
        (1, true, false, 4),
    ] {
        edge(&mut m, pin, old, new, t);
    }
    assert_eq!(m.double_word(c(251)), 4);
}
