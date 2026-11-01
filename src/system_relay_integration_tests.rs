use crate::{
    plc::{self, Device, Simulator},
    system_relays::{ClockProfile, ClockState},
};
fn d(s: &str) -> Device {
    s.parse().unwrap()
}

#[test]
fn documented_clock_is_observed_at_instruction_time_and_continues_between_scans() {
    let p = plc::compile("LD M1011\nOUT Y0\nLD M1000\nDELAY K50\nLD M1011\nOUT Y1\nEND").unwrap();
    let mut s = Simulator::default();
    s.scan(&p, 0);
    assert!(!s.memory.bit(d("Y0")));
    assert!(s.memory.bit(d("Y1")));
    assert_eq!(s.memory.system_relays.power_elapsed_us(), 5000);
    ClockState::set_running(&mut s.memory, false);
    crate::timing_io::advance_by(&mut s.memory, 5000);
    assert!(!s.memory.system_bit(1011));
    assert_eq!(s.scans, 1);
}

#[test]
fn high_speed_enable_requires_end_and_off_on_in_one_body_rearms_at_next_end() {
    let p = plc::compile("LD M1000\nSET M1015\nMOV D1015 D20\nEND").unwrap();
    let mut s = Simulator::default();
    s.scan(&p, 1);
    assert_eq!(s.memory.word(d("D1015")), 0);
    s.scan(&p, 1);
    assert_eq!(s.memory.word(d("D1015")), 10);
    assert_eq!(s.memory.word(d("D20")), 0);
    let toggle =
        plc::compile("LD M1000\nRST M1015\nSET M1015\nDELAY K100\nMOV D1015 D20\nEND").unwrap();
    s.scan(&toggle, 0);
    assert_eq!(
        s.memory.word(d("D20")),
        10,
        "disable/re-enable must not count the intervening body"
    );
    assert_eq!(s.memory.word(d("D1015")), 10);
    crate::timing_io::advance_by(&mut s.memory, 100);
    assert_eq!(s.memory.word(d("D1015")), 11);
}

#[test]
fn fixed_scan_wait_is_minimum_duration_and_does_not_double_count_delay() {
    // Normal DELAY units are 100 us, and each operand must be 1..1000.
    for (body, expected) in [
        ("", 100_000),
        ("DELAY K50\n", 100_000),
        ("DELAY K600\nDELAY K600\n", 140_000),
    ] {
        let p = plc::compile(&format!("LD M1000\nMOV K100 D1039\nSET M1039\n{body}END")).unwrap();
        let mut s = Simulator::default();
        s.scan(&p, 20);
        assert!(s.memory.execution_limitations.is_empty());
        assert_eq!(s.memory.timing_io.now_us, expected);
        assert_eq!(
            s.memory.system_relays.power_elapsed_us(),
            u128::from(expected)
        );
    }
}

#[test]
fn installed_first_body_is_an_explicit_input_then_endpoints_are_predicted() {
    let p = plc::compile("LD M1013\nOUT Y0\nLD M1014\nOUT Y1\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory
        .system_relays
        .set_profile(ClockProfile::InstalledFixed);
    s.memory.set_system(1039, true);
    s.memory.set_word(d("D1039"), 100);
    s.scan(&p, 100);
    assert!(matches!(s.last_scan, plc::ScanStatus::Fault(_)));
    assert_eq!(s.scans, 0);
    ClockState::seed_installed_levels(&mut s.memory, [true, true, true, true]).unwrap();
    s.scan(&p, 100);
    assert!(s.memory.bit(d("Y0")) && s.memory.bit(d("Y1")));
    assert!(!s.memory.system_bit(1013) && !s.memory.system_bit(1014));
    for _ in 0..4 {
        s.scan(&p, 100);
    }
    assert!(s.memory.system_bit(1013));
    assert!(!s.memory.system_bit(1014));
    assert!(
        !s.memory.bit(d("Y0")),
        "fifth body's contact precedes the fifth END"
    );
}
