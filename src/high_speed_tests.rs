use super::*;
fn dev(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn action(name: &str, args: &[&str]) -> HighSpeedAction {
    HighSpeedAction::parse(name, &args.iter().map(|v| (*v).into()).collect::<Vec<_>>()).unwrap()
}
fn run(m: &mut Memory, pc: u16, name: &str, args: &[&str]) {
    action(name, args).execute(m, true, pc);
}
fn change(
    m: &mut Memory,
    c: &str,
    value: i32,
    at: u64,
    down: bool,
    reason: CounterChange,
    active: &[Device],
) {
    let c = dev(c);
    let before = m.double_word(c);
    m.set_double_word(c, value);
    HighSpeedState::counter_changed(m, c, before, value, at, down, reason, active);
}
fn edge(m: &mut Memory, p: &str, old: bool, new: bool, t: u64) {
    HighSpeedState::advance_to(m, t);
    HighSpeedState::input_edge(m, dev(p), old, new, t);
}
fn pulse(m: &mut Memory, t: u64) {
    edge(m, "X0", false, true, t);
    edge(m, "X0", true, false, t + 1);
}
fn pair(m: &Memory, p: &str) -> i32 {
    m.double_word(dev(p))
}
fn clean(m: &Memory) {
    assert!(
        m.execution_limitations.is_empty(),
        "{:?}",
        m.execution_limitations
    );
}

#[test]
fn exact_forms_matrices_lengths_and_header_metadata() {
    for (name, args, header, words) in [
        ("DHSCS", vec!["K1", "C235", "M0"], 0x7B00, 13),
        ("DHSCR", vec!["HFFFFFFFF", "C235", "C236"], 0x7D00, 13),
        ("DHSZ", vec!["K-1", "K1", "C235", "Y0"], 0x7F00, 17),
        ("SPD", vec!["X1", "E1", "T10"], 0x8000, 7),
    ] {
        let a = action(name, &args);
        assert_eq!(a.header(), header);
        assert_eq!(a.words(), words);
    }
    for (name, args) in [
        ("HSCS", vec!["K1", "C235", "M0"]),
        ("DHSCR", vec!["K1", "C235", "I010"]),
        ("DHSCS", vec!["K1", "C231", "M0"]),
        ("DHSZ", vec!["K1", "K2", "C235", "X0"]),
        ("SPD", vec!["X10", "K10", "D0"]),
        ("SPDP", vec!["X0", "K10", "D0"]),
    ] {
        assert!(
            HighSpeedAction::parse(name, &args.iter().map(|v| (*v).into()).collect::<Vec<_>>())
                .is_err()
        );
    }
}
#[test]
fn equality_sets_and_reset_latches_survive_nonmatching_values_and_false() {
    let mut m = Memory::default();
    run(&mut m, 1, "DHSCS", &["K2", "C235", "M0"]);
    change(&mut m, "C235", 2, 10, false, CounterChange::Count, &[]);
    assert!(m.bit(dev("M0")));
    change(&mut m, "C235", 3, 20, false, CounterChange::Count, &[]);
    assert!(m.bit(dev("M0")));
    action("DHSCS", &["K2", "C235", "M0"]).execute(&mut m, false, 1);
    assert!(m.bit(dev("M0")));
    run(&mut m, 2, "DHSCR", &["K2", "C235", "M0"]);
    change(&mut m, "C235", 2, 30, true, CounterChange::Count, &[]);
    assert!(!m.bit(dev("M0")));
    change(&mut m, "C235", 1, 40, true, CounterChange::Count, &[]);
    assert!(!m.bit(dev("M0")));
    clean(&m);
}
#[test]
fn arm_policy_is_explicit_and_held_scan_does_not_reassert_equal_output() {
    let mut m = Memory::default();
    run(&mut m, 1, "DHSCS", &["K0", "C235", "M0"]);
    assert!(m.bit(dev("M0")));
    m.set_internal(dev("M0"), false);
    run(&mut m, 1, "DHSCS", &["K0", "C235", "M0"]);
    assert!(!m.bit(dev("M0")));
    let mut transition = Memory::default();
    transition.high_speed.arm_comparison = ArmComparison::TransitionOnly;
    run(&mut transition, 1, "DHSCS", &["K0", "C235", "M0"]);
    assert!(!transition.bit(dev("M0")));
}
#[test]
fn signed_fullwidth_threshold_and_e_alias_are_not_truncated() {
    let mut m = Memory::default();
    m.set_double_word(dev("D1182"), 70_000);
    run(&mut m, 1, "DHSCS", &["E1", "C235", "M0"]);
    change(&mut m, "C235", 4464, 1, false, CounterChange::Count, &[]);
    assert!(!m.bit(dev("M0")));
    change(&mut m, "C235", 70_000, 2, false, CounterChange::Count, &[]);
    assert!(m.bit(dev("M0")));
    run(&mut m, 2, "DHSCS", &["K-2147483648", "C236", "M1"]);
    change(
        &mut m,
        "C236",
        i32::MIN,
        3,
        false,
        CounterChange::Count,
        &[],
    );
    assert!(m.bit(dev("M1")));
    clean(&m);
}
#[test]
fn f_threshold_is_not_in_wide_matrix_but_spd_word_period_allows_f() {
    assert!(HighSpeedAction::parse("DHSCS", &["F1".into(), "C235".into(), "M0".into()]).is_err());
    let mut m = Memory::default();
    m.set_word(dev("D1183"), 5);
    run(&mut m, 1, "SPD", &["X0", "F1", "D0"]);
    assert_eq!(m.word(dev("D4")), 5);
    clean(&m);
}
#[test]
fn software_set_reset_six_limit_is_separate_from_six_zone_slots() {
    let mut m = Memory::default();
    for pc in 0..6 {
        run(&mut m, pc, "DHSCS", &["K1", "C235", "M0"]);
        run(&mut m, 100 + pc, "DHSZ", &["K1", "K3", "C235", "M10"]);
    }
    assert_eq!(m.high_speed.comparator_count(), 12);
    clean(&m);
    run(&mut m, 9, "DHSCR", &["K1", "C235", "M1"]);
    assert_eq!(m.high_speed.comparator_count(), 12);
    assert!(!m.execution_limitations.is_empty());
}
#[test]
fn hardware_zone_reserves_slots_three_four_and_only_two_equal_slots_remain() {
    let mut m = Memory::default();
    run(&mut m, 1, "DHSZ", &["K1", "K3", "C243", "M10"]);
    run(&mut m, 2, "DHSCS", &["K1", "C243", "M0"]);
    run(&mut m, 3, "DHSCR", &["K1", "C243", "M1"]);
    assert_eq!(m.high_speed.comparator_count(), 3);
    clean(&m);
    run(&mut m, 4, "DHSCS", &["K1", "C243", "M2"]);
    assert_eq!(m.high_speed.comparator_count(), 3);
    assert!(!m.execution_limitations.is_empty());
}
#[test]
fn counter_irq_requires_correct_mapping_and_linked_handler() {
    let mut m = Memory::default();
    let irq = InterruptId::Counter { slot: 1 };
    m.interrupts
        .configure(BTreeMap::from([(irq, 100)]), 0)
        .unwrap();
    run(&mut m, 1, "DHSCS", &["K2", "C235", "I010"]);
    change(&mut m, "C235", 2, 17, false, CounterChange::Count, &[]);
    assert_eq!(m.interrupts.pending()[&irq].at_us, 17);
    clean(&m);
    let mut wrong = Memory::default();
    wrong
        .interrupts
        .configure(BTreeMap::from([(irq, 100)]), 0)
        .unwrap();
    run(&mut wrong, 1, "DHSCS", &["K2", "C233", "I010"]);
    assert_eq!(wrong.high_speed.comparator_count(), 0);
    assert!(!wrong.execution_limitations.is_empty());
}
#[test]
fn hardware_irq_slot_conflicts_with_zone_but_other_group_remains_free() {
    let mut m = Memory::default();
    m.interrupts
        .configure(
            BTreeMap::from([
                (InterruptId::Counter { slot: 3 }, 100),
                (InterruptId::Counter { slot: 5 }, 200),
            ]),
            0,
        )
        .unwrap();
    run(&mut m, 1, "DHSZ", &["K1", "K3", "C243", "M10"]);
    run(&mut m, 2, "DHSCS", &["K1", "C243", "I030"]);
    assert_eq!(m.high_speed.comparator_count(), 1);
    run(&mut m, 3, "DHSCS", &["K1", "C244", "I050"]);
    assert_eq!(m.high_speed.comparator_count(), 2);
}
#[test]
fn hardware_requested_counter_aliases_the_actual_enabled_group_source() {
    let mut m = Memory::default();
    run(&mut m, 1, "DHSCS", &["K1", "C245", "M0"]);
    change(
        &mut m,
        "C243",
        1,
        20,
        false,
        CounterChange::Count,
        &[dev("C243")],
    );
    assert!(m.bit(dev("M0")));
    assert_eq!(pair(&m, "C245"), 0);
    clean(&m);
}
#[test]
fn legacy_hardware_offset_and_latched_threshold_profiles_are_explicit() {
    let mut m = Memory::default();
    m.high_speed.hardware_timing = HardwareTiming::LegacyDirectionalOffset;
    m.high_speed.live_hardware_thresholds = false;
    run(&mut m, 1, "DHSCS", &["K5", "C243", "M0"]);
    run(&mut m, 1, "DHSCS", &["K9", "C243", "M0"]);
    change(&mut m, "C243", 5, 1, false, CounterChange::Count, &[]);
    assert!(!m.bit(dev("M0")));
    change(&mut m, "C243", 6, 2, false, CounterChange::Count, &[]);
    assert!(m.bit(dev("M0")));
    assert!(!m.high_speed.notices.is_empty());
    clean(&m);
}
#[test]
fn modern_dynamic_threshold_changes_only_on_instruction_visit() {
    let mut m = Memory::default();
    m.set_double_word(dev("D0"), 5);
    run(&mut m, 1, "DHSCS", &["D0", "C235", "M0"]);
    m.set_double_word(dev("D0"), 7);
    change(&mut m, "C235", 7, 1, false, CounterChange::Count, &[]);
    assert!(!m.bit(dev("M0")));
    run(&mut m, 1, "DHSCS", &["D0", "C235", "M0"]);
    assert!(m.bit(dev("M0")));
    clean(&m);
}
#[test]
fn zone_is_one_hot_and_both_boundary_interpretations_are_named() {
    let mut m = Memory::default();
    run(&mut m, 1, "DHSZ", &["K2", "K4", "C235", "M0"]);
    for (value, region) in [(1, 0), (2, 1), (3, 1), (4, 1), (5, 2)] {
        change(
            &mut m,
            "C235",
            value,
            value as u64,
            false,
            CounterChange::Count,
            &[],
        );
        for n in 0..3 {
            assert_eq!(m.bit(d(Area::M, n)), n == region);
        }
    }
    m.high_speed.zone_boundary = ZoneBoundary::LowerSideInclusive;
    change(&mut m, "C235", 2, 10, true, CounterChange::Count, &[]);
    assert!(m.bit(dev("M0")));
    assert!(!m.bit(dev("M1")));
    clean(&m);
}
#[test]
fn reversed_zone_is_blocked_and_equal_bounds_select_middle_at_equality() {
    let mut m = Memory::default();
    run(&mut m, 1, "DHSZ", &["K3", "K2", "C235", "M0"]);
    assert_eq!(m.high_speed.comparator_count(), 0);
    assert!(!m.execution_limitations.is_empty());
    let mut equal = Memory::default();
    run(&mut equal, 1, "DHSZ", &["K2", "K2", "C235", "M0"]);
    change(&mut equal, "C235", 2, 1, false, CounterChange::Count, &[]);
    assert!(equal.bit(dev("M1")));
    assert!(!equal.bit(dev("M0")));
}
#[test]
fn dhscr_counter_destination_clears_value_and_contact_without_recursive_reset() {
    let mut m = Memory::default();
    run(&mut m, 1, "DHSCR", &["K2", "C235", "C235"]);
    change(&mut m, "C235", 2, 1, false, CounterChange::Count, &[]);
    assert_eq!(pair(&m, "C235"), 0);
    assert!(!m.bit(dev("C235")));
    clean(&m);
    let mut cross = Memory::default();
    run(&mut cross, 1, "DHSCR", &["K2", "C235", "C236"]);
    assert_eq!(cross.high_speed.comparator_count(), 0);
    assert!(!cross.execution_limitations.is_empty());
    cross.execution_limitations.clear();
    cross.high_speed.counter_reset_target = CounterResetTarget::AnyHighSpeedCounter;
    cross.set_double_word(dev("C236"), 50000);
    run(&mut cross, 1, "DHSCR", &["K2", "C235", "C236"]);
    change(&mut cross, "C235", 2, 1, false, CounterChange::Count, &[]);
    assert_eq!(pair(&cross, "C236"), 0);
    clean(&cross);
}
#[test]
fn load_trigger_is_explicit_opt_in_while_zone_always_reflects_value_changes() {
    let mut m = Memory::default();
    run(&mut m, 1, "DHSCS", &["K2", "C235", "M0"]);
    run(&mut m, 2, "DHSZ", &["K1", "K3", "C235", "M10"]);
    change(&mut m, "C235", 2, 1, false, CounterChange::Load, &[]);
    assert!(!m.bit(dev("M0")));
    assert!(m.bit(dev("M11")));
    m.high_speed.compare_changes = CompareChanges::AllValueChanges;
    change(&mut m, "C235", 2, 2, false, CounterChange::Load, &[]);
    assert!(m.bit(dev("M0")));
}
#[test]
fn immediate_y_threshold_depends_on_model_and_other_y_stays_logical_until_end() {
    for (model, immediate4) in [(CounterModel::Es2, false), (CounterModel::Ss2, true)] {
        let mut m = Memory::default();
        m.counters.model = model;
        run(&mut m, 1, "DHSCS", &["K1", "C235", "Y4"]);
        change(&mut m, "C235", 1, 1, false, CounterChange::Count, &[]);
        assert!(m.bit(dev("Y4")));
        assert_eq!(m.physical_output(dev("Y4")), immediate4);
    }
}
#[test]
fn ss2_c232_is_not_silently_treated_as_a_highspeed_comparator_source() {
    let mut m = Memory::default();
    m.counters.model = CounterModel::Ss2;
    run(&mut m, 1, "DHSCS", &["K1", "C232", "M0"]);
    assert_eq!(m.high_speed.comparator_count(), 0);
    assert!(!m.execution_limitations.is_empty());
}
#[test]
fn spd_continuous_windows_store_previous_and_current_fullwidth_results() {
    let mut m = Memory::default();
    run(&mut m, 1, "SPD", &["X0", "K5", "D10"]);
    pulse(&mut m, 100);
    pulse(&mut m, 200);
    assert_eq!(pair(&m, "D12"), 2);
    assert_eq!(pair(&m, "D10"), 0);
    HighSpeedState::advance_to(&mut m, 5000);
    assert_eq!(pair(&m, "D10"), 2);
    assert_eq!(pair(&m, "D12"), 0);
    assert_eq!(m.word(dev("D14")), 5);
    pulse(&mut m, 6000);
    HighSpeedState::advance_to(&mut m, 10000);
    assert_eq!(pair(&m, "D10"), 1);
    clean(&m);
}
#[test]
fn spd_deadline_edge_belongs_to_new_window_and_remaining_time_ceil_is_explicit() {
    let mut m = Memory::default();
    run(&mut m, 1, "SPD", &["X0", "K1", "D0"]);
    edge(&mut m, "X0", false, true, 1000);
    assert_eq!(pair(&m, "D0"), 0);
    assert_eq!(pair(&m, "D2"), 1);
    HighSpeedState::advance_to(&mut m, 1001);
    assert_eq!(m.word(dev("D4")), 1);
    HighSpeedState::advance_to(&mut m, 2000);
    assert_eq!(pair(&m, "D0"), 1);
    clean(&m);
}
#[test]
fn spd_quadrature_counts_all_four_edges_and_reverse_is_signed() {
    let mut m = Memory::default();
    run(&mut m, 1, "SPD", &["X1", "K10", "D0"]);
    for (p, o, n, t) in [
        ("X0", false, true, 1),
        ("X1", false, true, 2),
        ("X0", true, false, 3),
        ("X1", true, false, 4),
    ] {
        edge(&mut m, p, o, n, t);
    }
    assert_eq!(pair(&m, "D2"), 4);
    for (p, o, n, t) in [
        ("X1", false, true, 5),
        ("X0", false, true, 6),
        ("X1", true, false, 7),
        ("X0", true, false, 8),
    ] {
        edge(&mut m, p, o, n, t);
    }
    assert_eq!(pair(&m, "D2"), 0);
    clean(&m);
}
#[test]
fn spd_one_shot_requires_false_rearm_and_does_not_silently_repeat() {
    let mut m = Memory::default();
    m.high_speed.speed_windows = SpeedWindows::OneShotUntilRearm;
    let a = action("SPD", &["X0", "K1", "D0"]);
    a.execute(&mut m, true, 1);
    pulse(&mut m, 100);
    HighSpeedState::advance_to(&mut m, 1000);
    pulse(&mut m, 1100);
    assert_eq!(pair(&m, "D0"), 1);
    assert_eq!(pair(&m, "D2"), 0);
    a.execute(&mut m, false, 1);
    m.timing_io.now_us = 1200;
    a.execute(&mut m, true, 1);
    pulse(&mut m, 1300);
    HighSpeedState::advance_to(&mut m, 2200);
    assert_eq!(pair(&m, "D0"), 1);
    clean(&m);
}
#[test]
fn spd_false_holds_previous_results_releases_pins_and_reenable_resets_working_only() {
    let mut m = Memory::default();
    let a = action("SPD", &["X0", "K10", "D0"]);
    m.set_double_word(dev("D0"), 123456);
    a.execute(&mut m, true, 1);
    pulse(&mut m, 100);
    a.execute(&mut m, false, 1);
    pulse(&mut m, 200);
    assert_eq!(pair(&m, "D0"), 123456);
    assert_eq!(pair(&m, "D2"), 1);
    assert!(!m.timing_io.input_owners.contains_key(&dev("X0")));
    m.timing_io.now_us = 300;
    a.execute(&mut m, true, 1);
    assert_eq!(pair(&m, "D2"), 0);
    assert_eq!(pair(&m, "D0"), 123456);
    clean(&m);
}
#[test]
fn spd_period_is_snapshotted_before_alias_writes_and_next_window_change_is_deferred() {
    let mut m = Memory::default();
    m.set_word(dev("D0"), 5);
    run(&mut m, 1, "SPD", &["X0", "D0", "D0"]);
    assert_eq!(m.word(dev("D4")), 5);
    m.set_word(dev("D0"), 2);
    run(&mut m, 1, "SPD", &["X0", "D0", "D0"]);
    HighSpeedState::advance_to(&mut m, 4999);
    assert_eq!(m.word(dev("D4")), 1);
    HighSpeedState::advance_to(&mut m, 5000);
    assert_eq!(m.word(dev("D4")), 2);
    clean(&m);
}
#[test]
fn spd_counter_csfo_and_external_irq_ownership_are_exclusive() {
    let mut m = Memory::default();
    m.timing_io
        .input_owners
        .insert(dev("X0"), ResourceOwner::External("DCNT".into()));
    run(&mut m, 1, "SPD", &["X0", "K1", "D0"]);
    assert_eq!(m.high_speed.speed_count(), 0);
    assert!(!m.execution_limitations.is_empty());
    let mut irq = Memory::default();
    irq.interrupts
        .configure(
            BTreeMap::from([(
                InterruptId::External {
                    input: 0,
                    rising: true,
                },
                100,
            )]),
            0,
        )
        .unwrap();
    run(&mut irq, 1, "SPD", &["X0", "K1", "D0"]);
    assert_eq!(irq.high_speed.speed_count(), 0);
    let mut paired = Memory::default();
    run(&mut paired, 1, "SPD", &["X1", "K1", "D0"]);
    run(&mut paired, 2, "SPD", &["X0", "K1", "D10"]);
    assert_eq!(paired.high_speed.speed_count(), 1);
    assert!(!paired.execution_limitations.is_empty());
}
#[test]
fn spd_model_bounds_invalid_period_and_unknown_counter_layout_hold_without_partial_write() {
    let mut m = Memory::default();
    m.data_register_limit = Some(4999);
    m.set_word(dev("D4996"), 123);
    run(&mut m, 1, "SPD", &["X0", "K1", "D4996"]);
    assert_eq!(m.word(dev("D4996")), 123);
    assert_eq!(m.high_speed.speed_count(), 0);
    let mut bad = Memory::default();
    run(&mut bad, 1, "SPD", &["X0", "K0", "D0"]);
    assert_eq!(bad.high_speed.speed_count(), 0);
    assert!(!bad.execution_limitations.is_empty());
    let mut natural = Memory::default();
    run(&mut natural, 1, "SPD", &["X0", "K1", "C199"]);
    assert_eq!(natural.high_speed.speed_count(), 0);
    assert!(
        natural
            .execution_limitations
            .iter()
            .any(|v| v.contains("eşlemesi"))
    );
}
#[test]
fn spd_large_idle_advance_is_analytical_and_does_not_fabricate_a_last_nonzero_sample() {
    let mut m = Memory::default();
    run(&mut m, 1, "SPD", &["X0", "K1", "D0"]);
    pulse(&mut m, 100);
    HighSpeedState::advance_to(&mut m, 1_000_000_000_000);
    assert_eq!(pair(&m, "D0"), 0);
    assert_eq!(m.word(dev("D4")), 1);
    clean(&m);
}
#[test]
fn spd_t_and_low_c_destinations_store_both_words_without_touching_contacts() {
    for base in ["T10", "C10"] {
        let mut m = Memory::default();
        let b = dev(base);
        m.set_bit(b, true);
        run(&mut m, 1, "SPD", &["X0", "K1", base]);
        pulse(&mut m, 100);
        HighSpeedState::advance_to(&mut m, 1000);
        assert_eq!(m.word(b), 1);
        assert_eq!(m.word(d(b.area, b.index + 1)), 0);
        assert!(m.bit(b));
        clean(&m);
    }
}
#[test]
fn spd_result_and_display_preserve_signed32_above_word_range() {
    let mut m = Memory::default();
    let a = action("SPD", &["X0", "K1000", "D0"]);
    a.execute(&mut m, true, 1);
    for i in 0..32768 {
        pulse(&mut m, 10 + i * 2);
    }
    HighSpeedState::advance_to(&mut m, 1_000_000);
    assert_eq!(pair(&m, "D0"), 32768);
    assert_eq!(m.word(dev("D0")), i16::MIN);
    assert_eq!(m.word(dev("D1")), 0);
    assert_eq!(a.display_value(&m), "32768");
    clean(&m);
}
#[test]
fn complete_grouped_and_zone_output_spans_respect_model_relay_limit() {
    let mut m = Memory::default();
    m.relay_limit = Some(1535);
    run(&mut m, 1, "DHSCS", &["K1M1534", "C235", "M0"]);
    assert_eq!(m.high_speed.comparator_count(), 0);
    assert!(!m.execution_limitations.is_empty());
    let mut output = Memory::default();
    output.relay_limit = Some(1535);
    run(&mut output, 1, "DHSZ", &["K1", "K3", "C235", "M1534"]);
    assert_eq!(output.high_speed.comparator_count(), 0);
    assert!(!output.bit(dev("M1534")));
}
