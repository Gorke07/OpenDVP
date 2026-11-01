use super::*;
fn d(name: &str) -> Device {
    name.parse().unwrap()
}
fn action(name: &str, channel: &str, dest: &str) -> KnobAction {
    KnobAction::parse(name, &[channel.into(), dest.into()]).unwrap()
}
fn memory() -> Memory {
    let mut m = Memory::default();
    m.analog_knob.set_profile(KnobProfile::VirtualSa2);
    m
}
#[test]
fn exact_forms_constant_channels_and_destination_matrix() {
    for name in ["VRRD", "VRSC"] {
        for source in ["K0", "K1", "H0", "H1"] {
            for dest in ["D0", "T255", "C199", "K1Y0", "K4M100", "K4S20"] {
                let a = action(name, source, dest);
                assert_eq!(a.words(), 5);
                assert!(!a.wide());
                assert_eq!(a.header(), if name == "VRRD" { 0xba00 } else { 0xbc00 });
            }
        }
    }
    for (name, source, dest) in [
        ("DVRRD", "K0", "D0"),
        ("DVRSC", "K0", "D0"),
        ("VRRDP", "K0", "D0"), // outer Action pulse owns this spelling
        ("VRRD", "D0", "D1"),
        ("VRRD", "K2", "D1"),
        ("VRRD", "HFFFF", "D1"),
        ("VRSC", "K-1", "D0"),
        ("VRSC", "K0", "K4X0"),
        ("VRSC", "K0", "M0"),
        ("VRRD", "K0", "C200"),
        ("VRRD", "K0", "K8M0"),
    ] {
        assert!(KnobAction::parse(name, &[source.into(), dest.into()]).is_err());
    }
}
#[test]
fn no_configured_input_means_hold_and_explicit_limitation_not_zero() {
    let mut m = Memory::default();
    m.set_word(d("D0"), 777);
    action("VRRD", "K0", "D0").execute(&mut m, true, 1);
    assert_eq!(m.word(d("D0")), 777);
    assert!(!m.execution_limitations.is_empty());
    m.execution_limitations.clear();
    m.analog_knob.set_profile(KnobProfile::VirtualSx2);
    action("VRRD", "K0", "D0").execute(&mut m, true, 1);
    assert_eq!(m.word(d("D0")), 777);
    assert!(!m.execution_limitations.is_empty());
}
#[test]
fn false_power_holds_without_demanding_sample_or_changing_flags() {
    let mut m = Memory::default();
    m.set_word(d("D0"), 777);
    m.set_system(1029, true);
    action("VRRD", "K0", "D0").execute(&mut m, false, 1);
    assert_eq!(m.word(d("D0")), 777);
    assert!(m.system_bit(1029));
    assert!(m.execution_limitations.is_empty());
}
#[test]
fn raw_endpoints_and_two_independent_channels_without_enable_flag() {
    let mut m = memory();
    m.analog_knob.set_raw(0, 0).unwrap();
    m.analog_knob.set_raw(1, 255).unwrap();
    action("VRRD", "K0", "D0").execute(&mut m, true, 1);
    action("VRRD", "H1", "D1").execute(&mut m, true, 6);
    assert_eq!(m.word(d("D0")), 0);
    assert_eq!(m.word(d("D1")), 255);
    assert!(!m.system_bit(1178));
    assert!(!m.system_bit(1179));
    assert!(m.execution_limitations.is_empty());
}
#[test]
fn scaled_input_is_explicit_and_nearest_rounding_away_from_ties() {
    let mut m = memory();
    m.analog_knob.set_raw(0, 255).unwrap();
    action("VRSC", "K0", "D0").execute(&mut m, true, 1);
    assert!(!m.execution_limitations.is_empty()); // no guessed raw*10/255
    m.execution_limitations.clear();
    for (sample, expected) in [
        (0, 0),
        (2499, 2),
        (2501, 3),
        (9499, 9),
        (9501, 10),
        (10000, 10),
    ] {
        m.analog_knob.set_scaled_milli(0, sample).unwrap();
        action("VRSC", "K0", "D0").execute(&mut m, true, 1);
        assert_eq!(m.word(d("D0")), expected);
    }
    assert!(m.execution_limitations.is_empty());
}
#[test]
fn exact_half_tie_holds_by_default_and_named_policies_are_observable() {
    let mut m = memory();
    m.set_word(d("D0"), 77);
    m.analog_knob.set_scaled_milli(0, 2500).unwrap();
    let a = action("VRSC", "K0", "D0");
    a.execute(&mut m, true, 1);
    assert_eq!(m.word(d("D0")), 77);
    assert!(!m.execution_limitations.is_empty());
    m.execution_limitations.clear();
    for (policy, expected) in [
        (HalfwayPolicy::HalfUp, 3),
        (HalfwayPolicy::HalfDown, 2),
        (HalfwayPolicy::HalfToEven, 2),
    ] {
        m.analog_knob.halfway = policy;
        a.execute(&mut m, true, 1);
        assert_eq!(m.word(d("D0")), expected);
    }
    m.analog_knob.set_scaled_milli(0, 3500).unwrap();
    a.execute(&mut m, true, 1);
    assert_eq!(m.word(d("D0")), 4);
}
#[test]
fn invalid_sample_setters_do_not_destroy_previous_inputs() {
    let mut s = KnobState::default();
    s.set_raw(1, 123).unwrap();
    s.set_graduation(1, 7).unwrap();
    let old = s.input(1).unwrap();
    assert!(s.set_graduation(1, 11).is_err());
    assert!(s.set_scaled_milli(1, 10001).is_err());
    assert!(s.set_raw(2, 1).is_err());
    assert_eq!(s.input(1).unwrap(), old);
}
#[test]
fn group_width_truncates_to_available_bits_and_does_not_touch_neighbors() {
    let mut m = memory();
    m.analog_knob.set_raw(0, 0xa5).unwrap();
    m.set_internal(d("M99"), true);
    m.set_internal(d("M104"), true);
    action("VRRD", "K0", "K1M100").execute(&mut m, true, 1);
    assert_eq!(
        (0..4)
            .map(|i| m.bit(at(Area::M, 100 + i)))
            .collect::<Vec<_>>(),
        [true, false, true, false]
    );
    assert!(m.bit(d("M99")));
    assert!(m.bit(d("M104")));
}
#[test]
fn full_runtime_group_and_data_extents_are_atomic() {
    let mut m = memory();
    m.analog_knob.set_raw(0, 255).unwrap();
    m.relay_limit = Some(110);
    m.set_internal(d("M100"), true);
    action("VRRD", "K0", "K4M100").execute(&mut m, true, 1);
    assert!(m.bit(d("M100")));
    assert!(!m.bit(d("M101")));
    assert!(!m.execution_limitations.is_empty());
    m.execution_limitations.clear();
    m.data_register_limit = Some(4999);
    m.set_word(d("D5000"), 77);
    action("VRRD", "K0", "D5000").execute(&mut m, true, 1);
    assert_eq!(m.word(d("D5000")), 77);
    assert!(!m.execution_limitations.is_empty());
}
#[test]
fn documented_word_destinations_and_last_y_group_are_functional() {
    let mut m = memory();
    m.analog_knob.set_raw(0, 255).unwrap();
    for dest in ["T255", "C199", "D9999"] {
        action("VRRD", "K0", dest).execute(&mut m, true, 1);
        assert_eq!(m.word(d(dest)), 255);
    }
    action("VRRD", "K0", "K1Y374").execute(&mut m, true, 1);
    for i in 252..256 {
        assert!(m.bit(at(Area::Y, i)));
    }
    assert!(m.execution_limitations.is_empty());
}
#[test]
fn enabled_public_monitor_updates_raw_word_and_disable_holds_last_sample() {
    let mut m = memory();
    m.analog_knob.set_raw(0, 40).unwrap();
    m.analog_knob.set_raw(1, 90).unwrap();
    m.publish_analog_knob(0, 7);
    KnobState::service(&mut m);
    assert_eq!(m.word(d("D1178")), 7);
    m.set_system(1178, true);
    KnobState::service(&mut m);
    assert_eq!(m.word(d("D1178")), 40);
    assert_eq!(m.word(d("D1179")), 0);
    m.set_system(1178, false);
    m.set_system(1179, true);
    m.analog_knob.set_raw(0, 80).unwrap();
    KnobState::service(&mut m);
    assert_eq!(m.word(d("D1178")), 40);
    assert_eq!(m.word(d("D1179")), 90);
}
#[test]
fn missing_channel_does_not_refresh_automatic_monitor_with_zero() {
    let mut m = memory();
    m.set_system(1178, true);
    m.publish_analog_knob(0, 88);
    KnobState::service(&mut m);
    assert_eq!(m.word(d("D1178")), 88);
    assert!(m.execution_limitations.is_empty());
}
#[test]
fn device_observation_includes_complete_group_and_both_monitor_pairs() {
    let a = action("VRSC", "K1", "K4M100");
    let mut set = BTreeSet::new();
    a.devices(&mut set);
    for i in 100..116 {
        assert!(set.contains(&at(Area::M, i)));
    }
    for i in [1178, 1179] {
        assert!(set.contains(&at(Area::M, i)));
        assert!(set.contains(&at(Area::D, i)));
    }
}
#[test]
fn compiled_outer_pulse_holds_and_rearms_without_replacing_virtual_inputs() {
    let p = crate::plc::compile_il("LD X0\nVRRDP K0 D0\nEND").unwrap();
    let mut s = crate::plc::Simulator::default();
    s.memory
        .analog_knob
        .set_profile(KnobProfile::VirtualExtension);
    s.memory.analog_knob.set_raw(0, 12).unwrap();
    s.memory.set_input(d("X0"), true);
    s.scan(&p, 0);
    assert_eq!(s.memory.word(d("D0")), 12);
    s.memory.analog_knob.set_raw(0, 34).unwrap();
    s.scan(&p, 0);
    assert_eq!(s.memory.word(d("D0")), 12);
    s.memory.set_input(d("X0"), false);
    s.scan(&p, 0);
    s.memory.set_input(d("X0"), true);
    s.scan(&p, 0);
    assert_eq!(s.memory.word(d("D0")), 34);
    assert_eq!(s.last_scan, crate::plc::ScanStatus::Complete);
    assert!(s.memory.execution_limitations.is_empty());
}
#[test]
fn read_does_not_claim_completion_or_modify_arithmetic_status() {
    let mut m = memory();
    m.analog_knob.set_graduation(0, 7).unwrap();
    for index in [1020, 1022, 1029] {
        m.set_system(index, true);
    }
    m.set_system(1021, false);
    action("VRSC", "K0", "D0").execute(&mut m, true, 1);
    assert_eq!(m.word(d("D0")), 7);
    for index in [1020, 1022, 1029] {
        assert!(m.system_bit(index));
    }
    assert!(!m.system_bit(1021));
}
#[test]
fn configured_monitor_words_are_not_ordinary_action_destinations() {
    let mut m = memory();
    m.analog_knob.set_raw(0, 255).unwrap();
    m.publish_analog_knob(0, 40);
    m.publish_analog_knob(1, 90);
    for dest in ["D1178", "D1179"] {
        action("VRRD", "K0", dest).execute(&mut m, true, 1);
    }
    assert_eq!(m.word(d("D1178")), 40);
    assert_eq!(m.word(d("D1179")), 90);
    assert!(!m.execution_limitations.is_empty());
    // Default unconfigured ES2 memory keeps its pre-existing generic behavior.
    let mut unconfigured = Memory::default();
    unconfigured.set_word(d("D1178"), 77);
    assert_eq!(unconfigured.word(d("D1178")), 77);
}
