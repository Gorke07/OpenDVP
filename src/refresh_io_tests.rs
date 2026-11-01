use super::*;
fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn action(s: &str) -> RefreshAction {
    let mut p = s.split_whitespace();
    let name = p.next().unwrap();
    RefreshAction::parse(name, &p.map(str::to_string).collect::<Vec<_>>()).unwrap()
}
fn raw(s: &[(u16, bool)]) -> BTreeMap<Device, bool> {
    s.iter().map(|&(i, v)| (at(Area::X, i), v)).collect()
}
fn advance(m: &mut Memory, now: u64, raw: &BTreeMap<Device, bool>) {
    RefreshState::advance(m, now, raw, &BTreeSet::new());
}

#[test]
fn operands_and_entire_matrix_span_are_checked() {
    for s in [
        "REFF K1",
        "REFF D0",
        "REF M0 K8",
        "MTR X1 Y0 M0 K2",
        "MTR X0 Y0 M1 K2",
        "MTR X0 Y0 S1020 K2",
        "MTR X0 Y0 M0 K9",
    ] {
        let mut p = s.split_whitespace();
        let name = p.next().unwrap();
        assert!(
            RefreshAction::parse(name, &p.map(str::to_string).collect::<Vec<_>>()).is_err(),
            "{s}"
        );
    }
    assert_eq!(action("REFF H14").label(), "REFF K20");
    assert_eq!(action("REF Y4 K8").header(), 0x7400);
}
#[test]
fn mpu_refresh_masks_retain_the_documented_banks() {
    let mut p = RefreshProfile {
        family: MpuFamily::Es2,
        input_points: 24,
        output_points: 16,
        ..RefreshProfile::default()
    };
    assert_eq!(p.refresh_range(d("X0"), 4).unwrap().1, 0..8);
    assert_eq!(p.refresh_range(d("X0"), 12).unwrap().1, 0..24);
    assert_eq!(p.refresh_range(d("X10"), 8).unwrap().1, 8..24);
    assert_eq!(p.refresh_range(d("Y4"), 8).unwrap().1, 4..16);
    assert!(p.refresh_range(d("X0"), 10).is_err());
    p.family = MpuFamily::Ss2;
    assert!(p.refresh_range(d("Y0"), 4).is_err());
}
#[test]
fn reff_is_end_committed_and_chatter_restarts_stability_interval() {
    let mut m = Memory::default();
    RefreshState::power_on(&mut m);
    m.refresh_io.seed_input(d("X0"), false, 0);
    action("REFF K5").execute(&mut m, true, 0);
    assert_eq!(m.word(d("D1020")), 5);
    assert_eq!(m.refresh_io.active_filter_ms(0), Some(10));
    RefreshState::end_scan(&mut m);
    assert_eq!(m.refresh_io.active_filter_ms(0), Some(5));
    m.refresh_io.raw_input_edge(d("X0"), true, 1000, false);
    advance(&mut m, 5999, &raw(&[(0, true)]));
    assert!(!m.refresh_io.filters[&d("X0")].accepted);
    m.refresh_io.raw_input_edge(d("X0"), false, 5999, false);
    m.refresh_io.raw_input_edge(d("X0"), true, 6000, false);
    advance(&mut m, 10999, &raw(&[(0, true)]));
    assert!(!m.refresh_io.filters[&d("X0")].accepted);
    advance(&mut m, 11000, &raw(&[(0, true)]));
    assert!(m.refresh_io.filters[&d("X0")].accepted);
    assert!(
        !m.bit(d("X0")),
        "filter acceptance does not refresh the process image"
    );
    RefreshState::begin_scan(&mut m, 11000, &raw(&[(0, true)]), &BTreeSet::new());
    assert!(m.bit(d("X0")));
    assert!(m.refresh_io.next_deadline_us().is_none());
}
#[test]
fn bypass_and_explicit_image_seeds_do_not_revert() {
    let mut m = Memory::default();
    m.refresh_io.filter_profile = FilterProfile::DocumentedSoftware;
    m.refresh_io.seed_input_image(d("X0"), true, 0);
    RefreshState::begin_scan(&mut m, 1, &raw(&[(0, true)]), &BTreeSet::new());
    assert!(m.bit(d("X0")));
    m.refresh_io.raw_input_edge(d("X0"), false, 2, true);
    assert_eq!(m.refresh_io.next_deadline_us(), None);
    RefreshState::begin_scan(&mut m, 2, &raw(&[(0, false)]), &BTreeSet::from([d("X0")]));
    assert!(!m.bit(d("X0")));
}
#[test]
fn second_filter_bank_and_direct_zero_are_explicit_capabilities() {
    let mut m = Memory::default();
    m.refresh_io.profile.second_filter_bank = true;
    RefreshState::power_on(&mut m);
    action("REFF K7").execute(&mut m, true, 0);
    assert_eq!(m.word(d("D1021")), 7);
    m.set_word(d("D1020"), 0);
    RefreshState::end_scan(&mut m);
    assert_eq!(m.refresh_io.active_filter_ms(0), Some(0));
    assert_eq!(m.refresh_io.active_filter_ms(1), Some(7));
    m.refresh_io.raw_input_edge(d("X0"), true, 10, false);
    m.refresh_io.raw_input_edge(d("X10"), true, 10, false);
    assert!(m.refresh_io.filters[&d("X0")].accepted);
    assert!(!m.refresh_io.filters[&d("X10")].accepted);
}
#[test]
fn installed_register_profile_writes_both_without_filter_claim() {
    let mut m = Memory::default();
    m.refresh_io.filter_profile = FilterProfile::InstalledRegisters;
    m.set_word(d("D1020"), 10);
    m.set_word(d("D1021"), 0);
    action("REFF K7").execute(&mut m, false, 0);
    assert_eq!(m.word(d("D1021")), 0);
    action("REFF K7").execute(&mut m, true, 0);
    assert_eq!(m.word(d("D1020")), 7);
    assert_eq!(m.word(d("D1021")), 7);
    assert_eq!(
        m.refresh_io.filter_profile,
        FilterProfile::InstalledRegisters
    );
    m.refresh_io.raw_input_edge(d("X0"), true, 100, false);
    assert_eq!(m.refresh_io.next_deadline_us(), None);
    RefreshState::end_scan(&mut m);
    assert_eq!(m.refresh_io.active_filter_ms(0), Some(10));
}
#[test]
fn immediate_output_refresh_does_not_overwrite_pulse_owner() {
    let mut m = Memory::default();
    m.set_bit(d("Y0"), true);
    m.set_bit(d("Y1"), true);
    m.timing_io.output_owners.insert(
        d("Y1"),
        crate::timing_io::ResourceOwner::External("pulse".into()),
    );
    action("REF Y0 K8").execute(&mut m, true, 0);
    assert!(m.physical_output(d("Y0")));
    assert!(!m.physical_output(d("Y1")));
}
#[test]
fn pulse_only_mode_preserves_io_and_copies_configured_position() {
    let mut m = Memory::default();
    m.refresh_io.profile.pulse_refresh = true;
    m.refresh_io.pulse_positions.insert(0, 123456);
    m.set_system(1672, true);
    m.set_bit(d("Y0"), true);
    action("REF Y0 K8").execute(&mut m, true, 0);
    assert_eq!(m.double_word(d("D1030")), 123456);
    assert!(!m.physical_output(d("Y0")));
    assert!(m.system_bit(1672));
}
#[test]
fn pdf_two_row_matrix_uses_decimal_ten_stride_and_live_rows() {
    let mut m = Memory::default();
    let a = action("MTR X40 Y40 M10 K2");
    m.refresh_io.set_matrix_rows(7, vec![0x81, 0x42]).unwrap();
    a.execute(&mut m, true, 7);
    assert!(m.physical_output(d("Y40")));
    assert!(!m.bit(d("M10")));
    assert_eq!(m.refresh_io.next_deadline_us(), Some(25000));
    advance(&mut m, 25000, &BTreeMap::new());
    assert!(m.bit(d("M10")));
    assert!(m.bit(d("M17")));
    assert!(!m.bit(d("M20")));
    assert!(m.physical_output(d("Y41")));
    advance(&mut m, 50000, &BTreeMap::new());
    assert!(m.bit(d("M21")));
    assert!(m.bit(d("M26")));
    assert!(!m.bit(d("M18")));
    assert!(m.system_bit(1029));
    assert_eq!(m.refresh_io.next_deadline_us(), Some(75000));
    RefreshState::begin_scan(&mut m, 50000, &BTreeMap::new(), &BTreeSet::new());
    assert!(m.system_bit(1029));
    RefreshState::begin_scan(&mut m, 50000, &BTreeMap::new(), &BTreeSet::new());
    assert!(!m.system_bit(1029));
    m.refresh_io.set_matrix_rows(7, vec![0, 0]).unwrap();
    advance(&mut m, 75000, &BTreeMap::new());
    assert!(!m.bit(d("M10")));
}
#[test]
fn matrix_owner_false_rearm_and_missing_backend_are_visible() {
    let mut m = Memory::default();
    let a = action("MTR X0 Y0 M0 K2");
    a.execute(&mut m, true, 1);
    assert_eq!(m.refresh_io.matrix_owner(), None);
    assert!(!m.execution_limitations.is_empty());
    m.execution_limitations.clear();
    m.refresh_io.set_matrix_rows(1, vec![255, 255]).unwrap();
    m.refresh_io.set_matrix_rows(2, vec![0, 0]).unwrap();
    a.execute(&mut m, true, 1);
    a.execute(&mut m, true, 2);
    assert_eq!(m.refresh_io.matrix_owner(), Some(1));
    a.execute(&mut m, false, 2);
    assert_eq!(m.refresh_io.matrix_owner(), Some(1));
    a.execute(&mut m, false, 1);
    assert_eq!(m.refresh_io.matrix_owner(), None);
    assert!(!m.physical_output(d("Y0")));
    a.execute(&mut m, true, 2);
    assert_eq!(m.refresh_io.matrix_owner(), Some(2));
}
#[test]
fn unsupported_matrix_span_is_atomic() {
    let mut m = Memory::default();
    m.refresh_io.profile.input_points = 8;
    m.refresh_io.set_matrix_rows(0, vec![255, 255]).unwrap();
    action("MTR X10 Y0 M0 K2").execute(&mut m, true, 0);
    assert!(!m.bit(d("M0")));
    assert!(!m.physical_output(d("Y0")));
    assert_eq!(m.refresh_io.matrix_owner(), None);
    assert!(!m.execution_limitations.is_empty());
}

#[test]
fn matrix_rejects_entire_span_past_selected_relay_limit_before_any_strobe() {
    let mut m = Memory::default();
    m.relay_limit = Some(2010);
    m.refresh_io.set_matrix_rows(0, vec![255, 255]).unwrap();
    action("MTR X0 Y0 M2000 K2").execute(&mut m, true, 0);
    assert_eq!(m.refresh_io.matrix_owner(), None);
    assert!(!m.execution_limitations.is_empty());
    assert!(!m.physical_output(d("Y0")));
    advance(&mut m, 50000, &BTreeMap::new());
    assert!(!m.bit(d("M2000")));
    assert!(!m.bit(d("M2010")));
    assert!(!m.physical_output(d("Y1")));
}
