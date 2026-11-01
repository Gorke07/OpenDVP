use super::*;
fn dev(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn action(name: &str, args: &[&str]) -> ExternalIoAction {
    ExternalIoAction::parse(name, &args.iter().map(|v| (*v).into()).collect::<Vec<_>>()).unwrap()
}
fn visit(m: &mut Memory, a: &ExternalIoAction, power: bool, pc: u16) {
    ExternalIoState::begin_scan(m);
    a.execute(m, power, pc);
}
fn key(m: &mut Memory, a: &ExternalIoAction, pc: u16, base: Device, k: u16) {
    m.set_internal(d(base.area, base.index + k), true);
    visit(m, a, true, pc);
    m.set_internal(d(base.area, base.index + k), false);
    visit(m, a, true, pc);
}
fn clean(m: &Memory) {
    assert!(
        m.execution_limitations.is_empty(),
        "{:?}",
        m.execution_limitations
    );
}
fn ascii(m: &mut Memory, base: u16, text: &[u8]) {
    for (i, pair) in text.chunks(2).enumerate() {
        let v = u16::from(pair[0]) | (u16::from(*pair.get(1).unwrap_or(&0)) << 8);
        m.set_word(d(Area::D, base + i as u16), v as i16);
    }
}

#[test]
fn exact_eight_forms_lengths_and_write_matrix() {
    for (name, args, h, n) in [
        ("TKY", vec!["M0", "D0", "M20"], 0x9C00, 7),
        ("DTKY", vec!["M0", "E1", "M20"], 0x9D00, 13),
        ("HKY", vec!["X20", "Y20", "D0", "M20"], 0x9E00, 9),
        ("DHKY", vec!["X20", "Y20", "D0", "M20"], 0x9F00, 17),
        ("DSW", vec!["X20", "Y20", "D0", "K2"], 0xA000, 9),
        ("SEGL", vec!["K1234", "Y20", "K0"], 0xA400, 7),
        ("ARWS", vec!["M0", "D0", "Y20", "K0"], 0xA600, 9),
        ("PR", vec!["D0", "Y20"], 0xAA00, 5),
    ] {
        let a = action(name, &args);
        assert_eq!(a.header(), h);
        assert_eq!(a.words(), n);
    }
    for (name, args) in [
        ("TKYP", vec!["M0", "D0", "M20"]),
        ("TKY", vec!["M0", "K1", "M20"]),
        ("HKY", vec!["X20", "Y20", "K4M0", "M20"]),
        ("DSW", vec!["X20", "Y20", "D0", "K3"]),
        ("ARWS", vec!["M0", "D0", "Y21", "K0"]),
        ("PR", vec!["M0", "Y20"]),
    ] {
        assert!(
            ExternalIoAction::parse(name, &args.iter().map(|v| (*v).into()).collect::<Vec<_>>())
                .is_err()
        );
    }
}
#[test]
fn tky_5301_sequence_latches_last_key_and_any_key_tracks_physical_level() {
    let mut m = Memory::default();
    let a = action("TKY", &["M0", "D0", "M20"]);
    for k in [5, 3, 0, 1] {
        key(&mut m, &a, 1, dev("M0"), k);
    }
    assert_eq!(m.word(dev("D0")), 5301);
    assert!(m.bit(dev("M21")));
    assert!(!m.bit(dev("M30")));
    m.set_internal(dev("M9"), true);
    visit(&mut m, &a, true, 1);
    assert!(m.bit(dev("M30")));
    visit(&mut m, &a, true, 1);
    assert_eq!(m.word(dev("D0")), 3019);
    visit(&mut m, &a, false, 1);
    assert_eq!(m.word(dev("D0")), 3019);
    assert!((20..31).all(|n| !m.bit(d(Area::M, n))));
    clean(&m);
}
#[test]
fn dtky_e_register_pair_preserves_eight_decimal_digits_and_overflows_highest() {
    let mut m = Memory::default();
    let a = action("DTKY", &["M0", "E1", "M20"]);
    for k in [1, 2, 3, 4, 5, 6, 7, 8] {
        key(&mut m, &a, 1, dev("M0"), k);
    }
    assert_eq!(m.double_word(dev("D1182")), 12_345_678);
    key(&mut m, &a, 1, dev("M0"), 9);
    assert_eq!(m.double_word(dev("D1182")), 23_456_789);
    clean(&m);
}
#[test]
fn tky_grouped_destination_truncates_only_actual_write_width() {
    let mut m = Memory::default();
    let a = action("TKY", &["M0", "K4M40", "M20"]);
    for k in [9, 8, 7, 6] {
        key(&mut m, &a, 1, dev("M0"), k);
    }
    let value = AdditionalValue::parse("K4M40", false, false)
        .unwrap()
        .read(&m);
    assert_eq!(value, 9876);
    clean(&m);
}
#[test]
fn hky_eight_visits_matrix_mapping_function_status_and_first_press_order() {
    let mut m = Memory::default();
    let a = action("HKY", &["X20", "Y20", "D0", "M20"]);
    m.external_io.set_keyboard(1, &[13, 2]).unwrap();
    for _ in 0..7 {
        visit(&mut m, &a, true, 1);
    }
    assert!(!m.system_bit(1029));
    visit(&mut m, &a, true, 1);
    assert!(m.bit(dev("M23")));
    assert!(m.bit(dev("M26")));
    assert_eq!(m.word(dev("D0")), 0);
    assert!(m.system_bit(1029));
    for _ in 0..8 {
        visit(&mut m, &a, true, 1);
    }
    assert_eq!(m.word(dev("D0")), 0);
    clean(&m);
}
#[test]
fn dhky_hex_word_and_dword_capacity_are_explicit_unsigned_nibbles() {
    for (name, n, digits) in [("HKY", 4, 0xABCDu32), ("DHKY", 8, 0xABCDABCDu32)] {
        let mut m = Memory::default();
        m.set_system(1167, true);
        let a = action(name, &["X20", "Y20", "D0", "M20"]);
        for k in [10, 11, 12, 13].into_iter().cycle().take(n) {
            m.external_io.set_keyboard(1, &[k]).unwrap();
            for _ in 0..8 {
                visit(&mut m, &a, true, 1);
            }
            m.external_io.set_keyboard(1, &[]).unwrap();
            for _ in 0..8 {
                visit(&mut m, &a, true, 1);
            }
        }
        if name == "HKY" {
            assert_eq!(m.word(dev("D0")) as u16, u16::try_from(digits).unwrap());
        } else {
            assert_eq!(m.double_word(dev("D0")) as u32, digits);
        }
        clean(&m);
    }
}
#[test]
fn hky_false_clears_status_but_holds_key_value_and_restarts_scan() {
    let mut m = Memory::default();
    let a = action("HKY", &["X20", "Y20", "D0", "M20"]);
    m.external_io.set_keyboard(1, &[5]).unwrap();
    for _ in 0..8 {
        visit(&mut m, &a, true, 1);
    }
    assert_eq!(m.word(dev("D0")), 5);
    visit(&mut m, &a, false, 1);
    assert_eq!(m.word(dev("D0")), 5);
    assert!((20..28).all(|n| !m.bit(d(Area::M, n))));
    assert!((16..20).all(|n| !m.physical_output(d(Area::Y, n))));
}
#[test]
fn dip_two_groups_use_exact_hundred_ms_strobes_and_commit_on_full_cycle() {
    let mut m = Memory::default();
    let a = action("DSW", &["X20", "Y20", "D0", "K2"]);
    m.external_io.set_dip(1, [1234, 5678]).unwrap();
    visit(&mut m, &a, true, 1);
    assert_eq!(m.external_io.next_deadline_us(), Some(100000));
    ExternalIoState::advance(&mut m, 399999, &BTreeMap::new());
    assert_eq!(m.word(dev("D0")), 0);
    ExternalIoState::advance(&mut m, 400000, &BTreeMap::new());
    assert_eq!(m.word(dev("D0")), 1234);
    assert_eq!(m.word(dev("D1")), 5678);
    assert!(m.system_bit(1029));
    clean(&m);
}
#[test]
fn dip_callback_survives_taken_timing_state_and_false_drops_partial_read() {
    let mut m = Memory::default();
    let a = action("DSW", &["X20", "Y20", "D0", "K1"]);
    m.external_io.set_dip(1, [9876, 0]).unwrap();
    visit(&mut m, &a, true, 1);
    let timing = std::mem::take(&mut m.timing_io);
    ExternalIoState::advance(&mut m, 150000, &BTreeMap::new());
    m.timing_io = timing;
    m.timing_io.now_us = 150000;
    visit(&mut m, &a, false, 1);
    ExternalIoState::advance(&mut m, 500000, &BTreeMap::new());
    assert_eq!(m.word(dev("D0")), 0);
    assert!(m.external_io.claimed_outputs().is_empty());
}
#[test]
fn every_segl_polarity_profile_physically_latches_one_or_two_sets_after_twelve_visits() {
    for mode in 0..8 {
        let mut m = Memory::default();
        m.set_word(dev("D0"), 1234);
        m.set_word(dev("D1"), 4321);
        let a = action("SEGL", &["D0", "Y20", &format!("K{mode}")]);
        for _ in 0..12 {
            visit(&mut m, &a, true, 1);
        }
        assert_eq!(
            m.external_io.display_latches[&1],
            [1234, if mode >= 4 { 4321 } else { 0 }]
        );
        assert!(m.system_bit(1029));
        clean(&m);
    }
}
#[test]
fn segl_latches_actual_setup_bus_not_a_changed_source_on_latch_visit() {
    let mut m = Memory::default();
    m.set_word(dev("D0"), 1234);
    let a = action("SEGL", &["D0", "Y20", "K0"]);
    visit(&mut m, &a, true, 1);
    m.set_word(dev("D0"), 1299);
    visit(&mut m, &a, true, 1);
    assert_eq!(m.external_io.display_latches[&1][0], 4);
    assert_eq!(m.external_io.display_commands[&1][0], 1299);
}
#[test]
fn output_inhibit_suppresses_physical_display_latch_and_matrix_key_capture() {
    let mut m = Memory::default();
    m.set_system(1034, true);
    m.set_word(dev("D0"), 1234);
    let a = action("SEGL", &["D0", "Y20", "K0"]);
    for _ in 0..12 {
        visit(&mut m, &a, true, 1);
    }
    assert_eq!(m.external_io.display_commands[&1], [1234, 0]);
    assert!(!m.external_io.display_latches.contains_key(&1));
    let mut keyboard = Memory::default();
    keyboard.set_system(1034, true);
    keyboard.external_io.set_keyboard(1, &[5]).unwrap();
    let a = action("HKY", &["X20", "Y20", "D0", "M20"]);
    for _ in 0..8 {
        visit(&mut keyboard, &a, true, 1);
    }
    assert_eq!(keyboard.word(dev("D0")), 0);
    assert!(!keyboard.system_bit(1029));
}
#[test]
fn arrow_selected_digit_moves_wraps_and_edits_without_carry() {
    let mut m = Memory::default();
    m.set_word(dev("D0"), 9999);
    let a = action("ARWS", &["M0", "D0", "Y20", "K0"]);
    visit(&mut m, &a, true, 1);
    assert_eq!(m.external_io.selected_digit(1), Some(3));
    assert!(m.physical_output(dev("Y27")));
    key(&mut m, &a, 1, dev("M0"), 1);
    assert_eq!(m.word(dev("D0")), 999);
    key(&mut m, &a, 1, dev("M0"), 3);
    assert_eq!(m.external_io.selected_digit(1), Some(0));
    key(&mut m, &a, 1, dev("M0"), 1);
    assert_eq!(m.word(dev("D0")), 990);
    key(&mut m, &a, 1, dev("M0"), 2);
    assert_eq!(m.external_io.selected_digit(1), Some(3));
    key(&mut m, &a, 1, dev("M0"), 0);
    assert_eq!(m.word(dev("D0")), 9990);
    clean(&m);
}
#[test]
fn packed_print_has_three_visit_byte_phases_and_real_strobe_observations() {
    let mut m = Memory::default();
    ascii(&mut m, 0, b"ABCDEFGH");
    let a = action("PR", &["D0", "Y20"]);
    for _ in 0..24 {
        visit(&mut m, &a, true, 1);
    }
    assert_eq!(m.external_io.printer_commands[&1][0], b"ABCDEFGH");
    assert_eq!(m.external_io.printer_frames[&1][0], b"ABCDEFGH");
    assert!(m.system_bit(1029));
    assert!(!m.physical_output(dev("Y31")));
    visit(&mut m, &a, true, 1);
    assert_eq!(m.external_io.printer_frames[&1].len(), 1);
    clean(&m);
}
#[test]
fn long_print_nul_terminates_and_held_complete_waits_for_false_before_completion_flag() {
    let mut m = Memory::default();
    m.set_system(1027, true);
    ascii(&mut m, 0, b"A\0ignored");
    let a = action("PR", &["D0", "Y20"]);
    for _ in 0..3 {
        visit(&mut m, &a, true, 1);
    }
    assert_eq!(m.external_io.printer_frames[&1][0], b"A");
    assert!(!m.system_bit(1029));
    assert!(m.physical_output(dev("Y31")));
    for _ in 0..8 {
        visit(&mut m, &a, true, 1);
    }
    assert_eq!(m.external_io.printer_frames[&1].len(), 1);
    visit(&mut m, &a, false, 1);
    assert!(m.system_bit(1029));
    assert!(!m.physical_output(dev("Y31")));
    clean(&m);
}
#[test]
fn long_print_finishes_a_full_string_after_contact_turns_false() {
    let mut m = Memory::default();
    m.set_system(1027, true);
    ascii(&mut m, 0, b"AB\0");
    let a = action("PR", &["D0", "Y20"]);
    visit(&mut m, &a, true, 1);
    for _ in 0..5 {
        visit(&mut m, &a, false, 1);
    }
    assert_eq!(m.external_io.printer_frames[&1][0], b"AB");
    assert!(m.system_bit(1029));
    assert!(m.external_io.claimed_outputs().is_empty());
    clean(&m);
}
#[test]
fn legacy_print_false_aborts_all_ten_pins_and_restarts_first_byte() {
    let mut m = Memory::default();
    ascii(&mut m, 0, b"ABCDEFGH");
    let a = action("PR", &["D0", "Y20"]);
    visit(&mut m, &a, true, 1);
    visit(&mut m, &a, false, 1);
    assert!((16..26).all(|n| !m.physical_output(d(Area::Y, n))));
    visit(&mut m, &a, true, 1);
    assert_eq!(
        (0..8).fold(0u8, |v, n| v | (u8::from(m.bit(d(Area::Y, 16 + n))) << n)),
        b'A'
    );
    clean(&m);
}
#[test]
fn inhibited_print_records_intended_command_but_no_physically_strobed_bytes() {
    let mut m = Memory::default();
    m.set_system(1034, true);
    ascii(&mut m, 0, b"ABCDEFGH");
    let a = action("PR", &["D0", "Y20"]);
    for _ in 0..24 {
        visit(&mut m, &a, true, 1);
    }
    assert_eq!(m.external_io.printer_commands[&1][0], b"ABCDEFGH");
    assert!(m.external_io.printer_frames[&1][0].is_empty());
}
#[test]
fn shared_completion_aging_does_not_erase_a_newer_other_command_writer() {
    let mut m = Memory::default();
    ascii(&mut m, 0, b"ABCDEFGH");
    let a = action("PR", &["D0", "Y20"]);
    for _ in 0..24 {
        visit(&mut m, &a, true, 1);
    }
    m.set_system(1029, true);
    ExternalIoState::begin_scan(&mut m);
    ExternalIoState::begin_scan(&mut m);
    assert!(m.system_bit(1029));
}
#[test]
fn quotas_and_foreign_ownership_fail_before_partial_pin_writes() {
    let mut m = Memory::default();
    let a = action("HKY", &["X20", "Y20", "D0", "M20"]);
    m.timing_io
        .output_owners
        .insert(dev("Y21"), ResourceOwner::Csfo(7));
    visit(&mut m, &a, true, 1);
    assert_eq!(m.external_io.active_count(), 0);
    assert!(!m.bit(dev("Y20")));
    assert!(!m.execution_limitations.is_empty());
    let mut quota = Memory::default();
    let a = action("TKY", &["M0", "D0", "M20"]);
    visit(&mut quota, &a, true, 1);
    let b = action("DTKY", &["M40", "D2", "M60"]);
    visit(&mut quota, &b, true, 2);
    assert_eq!(quota.external_io.active_count(), 1);
    assert!(!quota.execution_limitations.is_empty());
}
#[test]
fn full_dynamic_source_and_destination_spans_respect_model_boundaries() {
    let mut m = Memory::default();
    m.data_register_limit = Some(4999);
    let a = action("PR", &["D4996", "Y20"]);
    visit(&mut m, &a, true, 1);
    clean(&m);
    visit(&mut m, &a, false, 1);
    m.set_system(1027, true);
    visit(&mut m, &a, true, 1);
    assert!(!m.execution_limitations.is_empty());
    assert_eq!(m.external_io.active_count(), 0);
    let mut status = Memory::default();
    status.relay_limit = Some(1535);
    let a = action("TKY", &["M0", "D0", "M1532"]);
    visit(&mut status, &a, true, 1);
    assert!(!status.execution_limitations.is_empty());
}
#[test]
fn abort_preserves_environment_key_queue_and_never_drives_discarded_owner() {
    let mut m = Memory::default();
    m.external_io.queue_key(1000, 1, 5, true).unwrap();
    let a = action("HKY", &["X20", "Y20", "D0", "M20"]);
    visit(&mut m, &a, true, 1);
    ExternalIoState::abort(&mut m);
    assert_eq!(m.external_io.next_deadline_us(), Some(1000));
    ExternalIoState::advance(&mut m, 1000, &BTreeMap::new());
    assert!(m.external_io.claimed_outputs().is_empty());
    assert_eq!(m.word(dev("D0")), 0);
    assert!(m.external_io.keyboards[&1].contains_key(&5));
}
#[test]
fn once_per_scan_cadence_keeps_call_revisits_from_advancing_display_phase() {
    let mut m = Memory::default();
    m.external_io.cadence = VisitCadence::OncePerScan;
    m.set_word(dev("D0"), 1234);
    let a = action("SEGL", &["D0", "Y20", "K0"]);
    ExternalIoState::begin_scan(&mut m);
    a.execute(&mut m, true, 1);
    a.execute(&mut m, true, 1);
    assert_eq!(m.external_io.runs[&1].phase, 1);
    clean(&m);
}

#[test]
fn repeated_hky_visit_keeps_current_scan_event_until_next_scan_boundary() {
    let mut m = Memory::default();
    let a = action("HKY", &["X20", "Y20", "D0", "M20"]);
    m.external_io.set_keyboard(1, &[5]).unwrap();
    for _ in 0..8 {
        visit(&mut m, &a, true, 1);
    }
    assert!(m.bit(dev("M27")));
    a.execute(&mut m, true, 1);
    assert!(m.bit(dev("M27")));
    ExternalIoState::begin_scan(&mut m);
    assert!(!m.bit(dev("M27")));
}

#[test]
fn arrow_optional_multiplex_profile_does_not_invent_a_completion_pulse() {
    let mut m = Memory::default();
    m.external_io.arrow_bus = ArrowBus::MultiplexDisplay;
    m.set_word(dev("D0"), 1234);
    let a = action("ARWS", &["M0", "D0", "Y20", "K0"]);
    for _ in 0..24 {
        visit(&mut m, &a, true, 1);
        assert!(!m.system_bit(1029));
    }
    clean(&m);
}

#[test]
fn long_print_restarts_after_false_completed_transfer_without_an_extra_false_visit() {
    let mut m = Memory::default();
    m.set_system(1027, true);
    ascii(&mut m, 0, b"AB\0");
    let a = action("PR", &["D0", "Y20"]);
    visit(&mut m, &a, true, 1);
    for _ in 0..5 {
        visit(&mut m, &a, false, 1);
    }
    assert!(!m.external_io.runs[&1].active);
    ascii(&mut m, 0, b"CD\0");
    for _ in 0..6 {
        visit(&mut m, &a, true, 1);
    }
    assert_eq!(m.external_io.printer_frames[&1].len(), 2);
    assert_eq!(m.external_io.printer_frames[&1][1], b"CD");
    clean(&m);
}

#[test]
fn inhibited_dip_strobe_does_not_claim_a_valid_injected_switch_sample() {
    let mut m = Memory::default();
    m.set_system(1034, true);
    m.set_word(dev("D0"), 9876);
    m.external_io.set_dip(1, [1234, 0]).unwrap();
    let a = action("DSW", &["X20", "Y20", "D0", "K1"]);
    visit(&mut m, &a, true, 1);
    ExternalIoState::advance(&mut m, 400_000, &BTreeMap::new());
    assert_eq!(m.word(dev("D0")), 9876);
    m.set_system(1034, false);
    ExternalIoState::advance(&mut m, 800_000, &BTreeMap::new());
    // The already inhibited units strobe remains inactive until the first
    // new boundary, so one complete subsequent driven round is required.
    ExternalIoState::advance(&mut m, 1_200_000, &BTreeMap::new());
    assert_eq!(m.word(dev("D0")), 1234);
    clean(&m);
}

#[test]
fn buffered_short_pr_keeps_its_source_footprint_after_mode_change() {
    let mut m = Memory::default();
    m.data_register_limit = Some(4999);
    ascii(&mut m, 4996, b"ABCDEFGH");
    let a = action("PR", &["D4996", "Y20"]);
    visit(&mut m, &a, true, 1);
    m.set_system(1027, true);
    for _ in 0..23 {
        visit(&mut m, &a, true, 1);
    }
    assert_eq!(m.external_io.printer_frames[&1][0], b"ABCDEFGH");
    clean(&m);
}

#[test]
fn non_y_status_span_is_checked_even_on_false_visit() {
    for name in ["TKY", "HKY"] {
        let mut m = Memory::default();
        m.relay_limit = Some(31);
        let a = if name == "TKY" {
            action(name, &["M0", "D0", "M30"])
        } else {
            action(name, &["X20", "Y20", "D0", "M30"])
        };
        m.set_internal(dev("M30"), true);
        visit(&mut m, &a, false, 1);
        assert!(m.bit(dev("M30")));
        assert!(!m.execution_limitations.is_empty());
    }
}

#[test]
fn raw_hky_inputs_cannot_bypass_an_inhibited_row_strobe() {
    let mut m = Memory::default();
    m.set_input(dev("X20"), true);
    m.set_system(1034, true);
    let a = action("HKY", &["X20", "Y20", "D0", "M20"]);
    for _ in 0..8 {
        visit(&mut m, &a, true, 1);
    }
    assert!(!m.bit(dev("M27")));
    assert!(!m.system_bit(1029));
    clean(&m);
}

#[test]
fn completion_body_and_idle_emissions_have_distinct_scan_lifetimes() {
    let mut m = Memory::default();
    ExternalIoState::begin_scan(&mut m);
    let mut s = std::mem::take(&mut m.external_io);
    s.emit(&mut m);
    m.external_io = s;
    ExternalIoState::end_scan(&mut m);
    ExternalIoState::begin_scan(&mut m);
    assert!(!m.system_bit(1029));
    ExternalIoState::end_scan(&mut m);
    let mut s = std::mem::take(&mut m.external_io);
    s.emit(&mut m);
    m.external_io = s;
    ExternalIoState::begin_scan(&mut m);
    assert!(m.system_bit(1029));
    ExternalIoState::end_scan(&mut m);
    ExternalIoState::begin_scan(&mut m);
    assert!(!m.system_bit(1029));
}
