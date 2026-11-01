//! Independent DCNT reset/lifecycle contracts. InstalledLegacy expectations
//! come from the retained interpreter audit; these are local VM tests and make
//! no assertion about physical high-speed counter hardware.
use crate::counter::{CounterAction, CounterPreset, CounterProfile, CounterState};
use crate::plc::{self, Device, Memory, Program, ScanStatus, Simulator};

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn coil(counter: &str, preset: i32) -> CounterAction {
    CounterAction {
        counter: d(counter),
        preset: CounterPreset::Constant(preset),
    }
}
fn installed() -> Simulator {
    let mut s = Simulator::default();
    s.memory.counters.profile = CounterProfile::InstalledLegacy;
    s
}
fn input(s: &mut Simulator, name: &str, value: bool) {
    s.memory.set_input(d(name), value);
}
fn scan(s: &mut Simulator, p: &Program) {
    assert!(
        p.requires_vm(),
        "these contracts must exercise the source VM"
    );
    s.scan(p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(
        s.memory.execution_limitations.is_empty(),
        "{:?}",
        s.memory.execution_limitations
    );
}
fn counter(m: &Memory, name: &str) -> (i32, bool) {
    (m.double_word(d(name)), m.bit(d(name)))
}

#[test]
fn skipped_rst_and_end_preserve_inhibition_until_a_visited_false_rst() {
    let p = plc::compile("LD X2\nCJ P0\nLD X1\nRST C200\nP0\nLD X0\nDCNT C200 K2\nEND").unwrap();
    let mut s = installed();
    input(&mut s, "X1", true);
    scan(&mut s, &p);
    assert_eq!(counter(&s.memory, "C200"), (0, false));

    input(&mut s, "X1", false);
    input(&mut s, "X2", true); // CJ skips the false RST, across several ENDs.
    for value in [true, false, true] {
        input(&mut s, "X0", value);
        scan(&mut s, &p);
        assert_eq!(counter(&s.memory, "C200"), (0, false));
    }
    input(&mut s, "X2", false); // Actually visit false RST; DCNT is held ON.
    scan(&mut s, &p);
    assert_eq!(counter(&s.memory, "C200"), (0, false));
    input(&mut s, "X0", false);
    scan(&mut s, &p);
    input(&mut s, "X0", true);
    scan(&mut s, &p);
    assert_eq!(counter(&s.memory, "C200"), (1, false));
}

#[test]
fn same_scan_false_rst_releases_the_earlier_true_rst_in_source_order() {
    for (profile, name) in [
        (CounterProfile::DocumentedEvents, "C200"),
        (CounterProfile::InstalledLegacy, "C200"),
        (CounterProfile::InstalledLegacy, "C234"),
    ] {
        for down in [false, true] {
            let p = plc::compile(&format!(
                "LD X1\nRST {name}\nLD X2\nRST {name}\nLD X0\nDCNT {name} K10\nEND"
            ))
            .unwrap();
            let mut s = Simulator::default();
            s.memory.counters.profile = profile;
            s.memory.set_double_word(d(name), 99);
            s.memory.set_bit(d(name), true);
            s.memory.set_system(1000 + d(name).index, down);
            input(&mut s, "X1", true);
            input(&mut s, "X0", true);
            scan(&mut s, &p);
            assert_eq!(
                counter(&s.memory, name),
                (if down { -1 } else { 1 }, false),
                "{profile:?} {name}, down={down}"
            );
        }
    }
}

#[test]
fn an_inhibited_fresh_edge_is_consumed_and_rst_does_not_rearm_it() {
    let mut m = Memory::default();
    m.counters.profile = CounterProfile::InstalledLegacy;
    let action = coil("C200", 1);
    CounterState::visit_reset(&mut m, d("C200"), true);
    action.execute(&mut m, true, 20);
    assert_eq!(counter(&m, "C200"), (0, false));
    CounterState::begin_scan(&mut m);
    CounterState::visit_reset(&mut m, d("C200"), false);
    action.execute(&mut m, true, 20);
    assert_eq!(counter(&m, "C200"), (0, false));
    action.execute(&mut m, false, 20);
    action.execute(&mut m, true, 20);
    assert_eq!(counter(&m, "C200"), (1, true));
    CounterState::visit_reset(&mut m, d("C200"), true);
    CounterState::visit_reset(&mut m, d("C200"), false);
    action.execute(&mut m, true, 20);
    assert_eq!(counter(&m, "C200"), (0, false));
}

#[test]
fn zrst_clears_value_and_contact_without_creating_inhibit_or_rearming_history() {
    let p = plc::compile("LD X1\nZRST C200 C200\nLD X0\nDCNT C200 K1\nEND").unwrap();
    let mut s = installed();
    s.memory.set_double_word(d("C200"), 77);
    s.memory.set_bit(d("C200"), true);
    input(&mut s, "X1", true);
    input(&mut s, "X0", true);
    scan(&mut s, &p);
    assert_eq!(counter(&s.memory, "C200"), (1, true));
    scan(&mut s, &p); // ZRST still ON, but no new DCNT edge.
    assert_eq!(counter(&s.memory, "C200"), (0, false));
    input(&mut s, "X1", false);
    input(&mut s, "X0", false);
    scan(&mut s, &p);
    input(&mut s, "X0", true);
    scan(&mut s, &p);
    assert_eq!(counter(&s.memory, "C200"), (1, true));
}

#[test]
fn zrst_and_plain_value_reset_do_not_release_existing_bare_rst_inhibition() {
    let p = plc::compile("LD X1\nZRST C200 C200\nLD X0\nDCNT C200 K1\nEND").unwrap();
    let mut s = installed();
    CounterState::visit_reset(&mut s.memory, d("C200"), true);
    s.memory.set_double_word(d("C200"), 77);
    s.memory.set_bit(d("C200"), true);
    CounterState::note_reset(&mut s.memory, d("C200"));
    assert_eq!(counter(&s.memory, "C200"), (0, false));
    input(&mut s, "X1", true);
    for power in [true, false, true] {
        input(&mut s, "X0", power);
        scan(&mut s, &p);
        assert_eq!(counter(&s.memory, "C200"), (0, false));
    }
    CounterState::visit_reset(&mut s.memory, d("C200"), false);
    input(&mut s, "X1", false);
    input(&mut s, "X0", false);
    scan(&mut s, &p);
    input(&mut s, "X0", true);
    scan(&mut s, &p);
    assert_eq!(counter(&s.memory, "C200"), (1, true));
}

#[test]
fn inactive_stl_false_rst_is_not_a_visit_and_cannot_release_inhibit() {
    let p = plc::compile("LD X0\nDCNT C200 K1\nSTL S0\nLD X1\nRST C200\nRET\nEND").unwrap();
    let mut s = installed();
    s.memory.sfc.profile = crate::sfc::SfcProfile::InstalledLegacy;
    CounterState::visit_reset(&mut s.memory, d("C200"), true);
    for power in [false, true, false, true] {
        input(&mut s, "X0", power);
        scan(&mut s, &p);
        assert!(!s.memory.bit(d("S0")));
        assert_eq!(counter(&s.memory, "C200"), (0, false));
    }
    // A real false visit releases it; the already consumed ON edge still holds.
    CounterState::visit_reset(&mut s.memory, d("C200"), false);
    scan(&mut s, &p);
    assert_eq!(counter(&s.memory, "C200"), (0, false));
    input(&mut s, "X0", false);
    scan(&mut s, &p);
    input(&mut s, "X0", true);
    scan(&mut s, &p);
    assert_eq!(counter(&s.memory, "C200"), (1, true));
}

#[test]
fn repeated_loop_visits_keep_the_six_word_dcnt_occurrence_history_stable() {
    let p = plc::compile("FOR K3\nLD X0\nDCNT C200 K10\nNEXT\nEND").unwrap();
    // FOR occupies 0..2, LD is 3, DCNT is 4..9; its history slot is PC+5=9.
    assert_eq!(p.native_action_pc, [4]);
    let encoded = crate::es2_encoder::encode(&p).unwrap();
    assert_eq!(
        &encoded.words[4..10],
        &[0x0600, 0xc883, 0x0a80, 0x0080, 0x0080, 0x0080]
    );
    let mut s = installed();
    for (power, expected) in [(true, 1), (true, 1), (false, 1), (true, 2)] {
        input(&mut s, "X0", power);
        scan(&mut s, &p);
        assert_eq!(counter(&s.memory, "C200"), (expected, false));
    }
    // The same occurrence can still see a real ON/OFF/ON within one scan.
    let toggled = plc::compile(
        "LD M1000\nMOV K0 D0\nFOR K3\nLD<> D0 K1\nDCNT C200 K10\nLD M1000\nINC D0\nNEXT\nEND",
    )
    .unwrap();
    let mut s = installed();
    scan(&mut s, &toggled);
    assert_eq!(counter(&s.memory, "C200"), (2, false));
    assert_eq!(s.memory.word(d("D0")), 3);
}

#[test]
fn documented_general_contact_remains_ge_while_installed_contact_is_equality_latched() {
    assert_eq!(
        Memory::default().counters.profile,
        CounterProfile::DocumentedEvents
    );
    // Values are independent literal cases: passing a threshold does not
    // refresh the installed contact unless its exact transition value occurs.
    for (before, contact, down, preset, after, documented, installed) in [
        (10, false, false, 2, 11, true, false),
        (1, true, true, 2, 0, false, true),
        (i32::MAX, true, false, 0, i32::MIN, false, true),
        (1, false, false, 2, 2, true, true),
        (2, true, true, 2, 1, false, false),
    ] {
        for (profile, expected_contact) in [
            (CounterProfile::DocumentedEvents, documented),
            (CounterProfile::InstalledLegacy, installed),
        ] {
            let mut m = Memory::default();
            m.counters.profile = profile;
            m.set_double_word(d("C200"), before);
            m.set_bit(d("C200"), contact);
            m.set_system(1200, down);
            coil("C200", preset).execute(&mut m, true, 50);
            assert_eq!(
                counter(&m, "C200"),
                (after, expected_contact),
                "{profile:?}"
            );
        }
    }
}

#[test]
fn installed_c235_plus_only_changes_enable_without_reading_preset_or_edge_history() {
    for name in ["C235", "C243", "C254"] {
        let mut m = Memory::default();
        m.counters.profile = CounterProfile::InstalledLegacy;
        m.data_register_limit = Some(4999);
        let high = CounterAction {
            counter: d(name),
            preset: CounterPreset::Data(d("D4999")),
        };
        assert!(high.validate_preset_span(&m).is_err());
        m.set_double_word(d(name), 77);
        m.set_bit(d(name), true);
        high.execute(&mut m, true, 50);
        high.execute(&mut m, true, 50);
        assert!(m.counters.installed_coil_enabled(d(name).index));
        assert_eq!(counter(&m, name), (77, true));
        assert!(
            m.execution_limitations.is_empty(),
            "preset must not be read"
        );
        assert!(m.counters.trace.is_empty());
        assert_eq!(m.counters.active_counters().count(), 0);
        assert!(m.timing_io.input_owners.is_empty());

        // A general coil at the same synthetic PC proves the enable-only
        // branch did not consume or clear that separate instruction history.
        let general = coil("C200", 10);
        general.execute(&mut m, true, 50);
        assert_eq!(counter(&m, "C200"), (1, false));
        high.execute(&mut m, false, 50);
        assert!(!m.counters.installed_coil_enabled(d(name).index));
        general.execute(&mut m, true, 50);
        assert_eq!(counter(&m, "C200"), (1, false));
        CounterState::input_edge(&mut m, d("X0"), false, true, 1);
        assert_eq!(counter(&m, name), (77, true));
        assert!(m.timing_io.input_owners.is_empty());
    }
}

#[test]
fn invalid_preset_pair_is_rejected_before_consuming_the_general_coil_edge() {
    for profile in [
        CounterProfile::DocumentedEvents,
        CounterProfile::InstalledLegacy,
    ] {
        let mut m = Memory::default();
        m.counters.profile = profile;
        m.data_register_limit = Some(4999);
        let action = CounterAction {
            counter: d("C200"),
            preset: CounterPreset::Data(d("D4999")),
        };
        action.execute(&mut m, true, 65531); // PC+5 wraps; identity is still stable.
        assert_eq!(counter(&m, "C200"), (0, false));
        assert!(!m.execution_limitations.is_empty());
        assert!(!m.counters.installed_coil_enabled(200));
        assert!(!m.system_bit(1067) && !m.system_bit(1068));

        m.execution_limitations.clear();
        m.data_register_limit = Some(9999);
        m.set_double_word(d("D4999"), 2);
        action.execute(&mut m, true, 65531); // No false visit is needed after rejection.
        assert_eq!(counter(&m, "C200"), (1, false));
        m.data_register_limit = Some(4999);
        action.execute(&mut m, false, 65531); // False path does not read the invalid pair.
        assert!(m.execution_limitations.is_empty());
        m.data_register_limit = Some(9999);
        action.execute(&mut m, true, 65531);
        assert_eq!(counter(&m, "C200"), (2, true));
    }
}

#[test]
fn cpu_restart_and_upload_fresh_state_discard_reset_inhibit_enable_and_history() {
    for restart_cpu in [true, false] {
        let mut s = installed();
        let action = coil("C200", 1);
        CounterState::visit_reset(&mut s.memory, d("C200"), true);
        action.execute(&mut s.memory, true, 50);
        coil("C235", 1).execute(&mut s.memory, true, 60);
        assert!(s.memory.counters.installed_coil_enabled(200));
        assert!(s.memory.counters.installed_coil_enabled(235));
        if restart_cpu {
            s.restart_preserving_file_store();
        } else {
            // Exact local fresh-state boundary used by bridge upload. This
            // does not call upload or start an owned native process.
            s = Simulator::default();
        }
        s.memory.counters.profile = CounterProfile::InstalledLegacy;
        assert!(!s.memory.counters.installed_coil_enabled(200));
        assert!(!s.memory.counters.installed_coil_enabled(235));
        assert_eq!(counter(&s.memory, "C200"), (0, false));
        action.execute(&mut s.memory, true, 50);
        assert_eq!(counter(&s.memory, "C200"), (1, true));
    }
}
