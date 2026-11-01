//! Independent local regression draft for actual indexed clock reads.
//! No native parity claim: unknown InstalledFixed inputs must remain unknown.
//! Integrate as a crate test module after the shared checked-read seam lands.
use crate::indexed_abs_gray::IndexedAbsGrayAction;
use crate::indexed_bit_number::{IndexedBitNumberAction, IndexedBitNumberPredicate};
use crate::indexed_comparison::IndexedPredicate;
use crate::indexed_operand::{AddressedValue, read_resolved_value};
use crate::indexed_random::IndexedRandomAction;
use crate::plc::{Action, Device, Memory, Program, ScanStatus, Simulator, compile_il};
use crate::system_relays::{ClockProfile, ClockState};
use std::collections::BTreeSet;

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn args(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(str::to_owned).collect()
}
fn installed() -> Memory {
    let mut m = Memory::default();
    m.system_relays.set_profile(ClockProfile::InstalledFixed);
    m.set_system(1039, true);
    m.set_word(d("D1039"), 0);
    m.set_word(d("D1028"), 0);
    m.set_word(d("D1182"), 0);
    m.set_double_word(d("D20"), 0x1234_5678);
    m.set_double_word(d("D22"), 0x2345_6789);
    for (index, value) in [(1020, true), (1021, false), (1022, true)] {
        m.set_system(index, value);
    }
    for raw in ["M79", "M80", "M82", "M83"] {
        m.set_internal(d(raw), true);
    }
    m
}
fn flags(m: &Memory) -> [bool; 3] {
    [1020, 1021, 1022].map(|n| m.system_bit(n))
}
fn with_memory(memory: Memory) -> Simulator {
    let mut sim = Simulator::default();
    sim.memory = memory;
    sim
}
fn complete(sim: &mut Simulator, program: &Program) {
    sim.scan(program, 0);
    assert_eq!(sim.last_scan, ScanStatus::Complete);
}
fn program(site: &str) -> Program {
    // F7=0 is independent of the E0/E1 source offsets. It forces the source
    // VM even for scalar-only programs, which also have a non-VM executor.
    let p = compile_il(&format!("LD X0F7\n{site}\nLDI X7\nMOV K321 D300\nEND"))
        .unwrap_or_else(|e| panic!("{site}: {e:?}"));
    assert!(p.requires_vm(), "{site}");
    // The test must exercise a dynamic read, not the pre-scan static guard.
    assert!(!(1011..=1014).any(|n| p.devices.contains(&Memory::system_device(n))));
    p
}
fn pulse(site: &str) -> String {
    let (name, operands) = site.split_once(' ').unwrap();
    format!("{name}P {operands}")
}
fn assert_clock_span(observed: &BTreeSet<Device>) {
    for n in 1011..=1014 {
        assert!(observed.contains(&Memory::system_device(n)), "M{n}");
    }
}
fn assert_clock_error(reason: &str) {
    assert!(
        reason.contains("1011") && reason.contains("1014"),
        "{reason}"
    );
}
fn assert_no_plc_error(m: &Memory) {
    assert!(!m.system_bit(1067) && !m.system_bit(1068));
    assert_eq!((m.word(d("D1067")), m.word(d("D1068"))), (0, 0));
}
fn assert_held(m: &Memory) {
    assert_eq!(m.double_word(d("D20")), 0x1234_5678);
    assert_eq!(m.double_word(d("D22")), 0x2345_6789);
    assert_eq!(
        [m.bit(d("M80")), m.bit(d("M81")), m.bit(d("M82"))],
        [true, false, true]
    );
    assert!(m.bit(d("M79")) && m.bit(d("M83")));
    assert_eq!(flags(m), [true, false, true]);
    assert_no_plc_error(m);
}

// Every entry has an actually readable clock group and an ordinary valid
// destination. SWAP/ABS on a clock destination are intentionally absent:
// their writable-role admission rejects that destination before this issue.
const ACTION_SITES: [&str; 18] = [
    "MOV K1M1011E0 D20",
    "DMOV K1M1011E1 D20",
    "CMP K1M1011E0 K3 M80",
    "DCMP K1M1011E1 K3 M80",
    "MUL K1M1011E0 K3 D20",
    "DMUL K1M1011E1 K3 D20",
    "MUL16 K1M1011E0 K3 D20",
    "MUL32 K1M1011E1 K3 D20",
    "BCD K1M1011E0 D20",
    "DBCD K1M1011E1 D20",
    "GRY K1M1011E0 D20",
    "DGRY K1M1011E1 D20",
    "GBIN K1M1011E0 D20",
    "DGBIN K1M1011E1 D20",
    "MVM K1M1011E0 HFFFF D20",
    "DMVM K1M1011E1 HFFFFFFFF D20",
    "RAND K1M1011E0 K1M1011E1 D20",
    "DRAND K1M1011E0 K1M1011E1 D20",
];

fn prepare(site: &str, m: &Memory, observed: &mut BTreeSet<Device>) -> Result<(), String> {
    let p = program(site);
    match &p.networks[0].actions[0] {
        Action::IndexedScalar(a) => a.prepare(m, observed).map(|_| ()),
        Action::IndexedComparison(a) => a.prepare(m, observed).map(|_| ()),
        Action::IndexedMultiply(a) => a.prepare(m, observed).map(|_| ()),
        Action::IndexedTruncated(a) => a.prepare(m, observed).map(|_| ()),
        Action::IndexedBcd(a) => a.prepare(m, observed).map(|_| ()),
        Action::IndexedAbsGray(a) => a.prepare(m, observed).map(|_| ()),
        Action::IndexedSwapMask(a) => a.prepare(m, observed).map(|_| ()),
        Action::IndexedRandom(a) => a.prepare(m, observed).map(|_| ()),
        other => panic!("wrong typed action for {site}: {other:?}"),
    }
}

#[test]
fn checked_read_is_pure_records_the_full_span_and_distinguishes_known_zero() {
    for wide in [false, true] {
        let mut m = installed();
        let binding = AddressedValue::parse("K1M1011E0", wide, false).unwrap();
        // Address resolution itself remains pure and usable by native input capture.
        let value = binding.resolve(&m, false).unwrap();
        let mut observed = BTreeSet::new();
        assert_clock_error(&read_resolved_value(&value, &m, &mut observed).unwrap_err());
        assert_clock_span(&observed);
        assert!(m.execution_limitations.is_empty());
        assert_held(&m);
        // An unknown clock domain must not prohibit unrelated data reads.
        let ordinary = AddressedValue::parse("K1M128E0", wide, false)
            .unwrap()
            .resolve(&m, false)
            .unwrap();
        m.set_internal(d("M128"), true);
        m.set_internal(d("M130"), true);
        assert_eq!(
            read_resolved_value(&ordinary, &m, &mut BTreeSet::new()).unwrap(),
            5
        );
        ClockState::seed_installed_levels(&mut m, [false; 4]).unwrap();
        assert_eq!(read_resolved_value(&value, &m, &mut observed).unwrap(), 0);
        let mut nonzero = installed();
        ClockState::seed_installed_levels(&mut nonzero, [true, false, true, false]).unwrap();
        assert_eq!(
            read_resolved_value(&value, &nonzero, &mut observed).unwrap(),
            5
        );
        let mut documented = Memory::default();
        assert_eq!(
            read_resolved_value(&value, &documented, &mut observed).unwrap(),
            0
        );
        ClockState::advance(&mut documented, 5_000);
        assert_eq!(
            read_resolved_value(&value, &documented, &mut observed).unwrap(),
            1
        );
    }
}

#[test]
fn every_earlier_family_pure_prepare_rejects_unknown_sources_before_math() {
    let m = installed();
    let extra = [
        "ADD K1 K1M1011E1 D20",
        "CMP K0 K1M1011E1 M80",
        "ZCP K0 K10 K1M1011E1 M80",
        "DIV K10 K1M1011E1 D20",
        "DDIV K10 K1M1011E1 D20",
        "DIV16 K10 K1M1011E1 D20",
        "DIV32 K10 K1M1011E1 D20",
        "BIN K1M1011E1 D20",
        "DBIN K1M1011E1 D20",
        "MVM D10 K1M1011E1 D20",
        "DMVM D10 K1M1011E1 D20",
        "RAND K-1 K1M1011E1 D20",
        "DRAND K-1 K1M1011E1 D20",
    ];
    for site in ACTION_SITES.into_iter().chain(extra) {
        let mut observed = BTreeSet::new();
        assert_clock_error(&prepare(site, &m, &mut observed).unwrap_err());
        assert_clock_span(&observed);
        assert!(m.execution_limitations.is_empty(), "pure: {site}");
        assert_eq!(m.random_state, None);
        assert_held(&m);
    }
}

#[test]
fn enabled_normal_and_pulse_actions_hold_outputs_flags_and_rng_on_unknown_read() {
    for site in ACTION_SITES {
        for site in [site.to_owned(), pulse(site)] {
            let p = program(&site);
            let mut sim = with_memory(installed());
            sim.memory.set_input(d("X0"), true);
            complete(&mut sim, &p);
            assert_held(&sim.memory);
            assert_eq!(sim.memory.random_state, None, "{site}");
            assert_eq!(sim.memory.word(d("D300")), 321, "following PC: {site}");
            assert_clock_span(&sim.observed_devices);
            assert!(
                sim.memory
                    .execution_limitations
                    .iter()
                    .any(|s| s.contains("1011")),
                "{site}"
            );
        }
    }
}

#[test]
fn disabled_normal_and_pulse_actions_do_not_observe_dynamic_clock_sources() {
    for site in ACTION_SITES {
        for site in [site.to_owned(), pulse(site)] {
            let p = program(&site);
            let mut sim = with_memory(installed());
            complete(&mut sim, &p);
            assert_held(&sim.memory);
            assert!(sim.memory.execution_limitations.is_empty(), "{site}");
            assert!(!sim.observed_devices.contains(&d("M1011")), "{site}");
            assert!(!sim.observed_devices.contains(&d("D1028")), "{site}");
            assert!(!sim.observed_devices.contains(&d("D1182")), "{site}");
            assert_eq!(sim.memory.random_state, None);
            assert_eq!(sim.memory.word(d("D300")), 321);
        }
    }
}

#[test]
fn scalar_only_scan_path_also_checks_only_actually_enabled_clock_reads() {
    for name in ["MOV", "MOVP", "DMOV", "DMOVP"] {
        let p = compile_il(&format!("LD X0\n{name} K1M1011E0 D20\nEND")).unwrap();
        assert!(
            !p.requires_vm(),
            "this fixture exercises the scalar-only path"
        );
        for enabled in [false, true] {
            let mut sim = with_memory(installed());
            sim.memory.set_input(d("X0"), enabled);
            complete(&mut sim, &p);
            assert_held(&sim.memory);
            assert_eq!(
                sim.memory.execution_limitations.is_empty(),
                !enabled,
                "{name}"
            );
            assert_eq!(
                sim.observed_devices.contains(&d("M1011")),
                enabled,
                "{name}"
            );
        }
    }
}

#[test]
fn seeded_and_documented_vm_results_follow_independent_integer_literals() {
    // Readbacks are literal results for input5 and input0, not another wrapper.
    for (site, five, zero) in [
        ("MOV K1M1011E0 D20", 5, 0),
        ("DMOV K1M1011E1 D20", 5, 0),
        ("MUL K1M1011E0 K3 D20", 15, 0),
        ("DMUL K1M1011E1 K3 D20", 15, 0),
        ("MUL16 K1M1011E0 K3 D20", 15, 0),
        ("MUL32 K1M1011E1 K3 D20", 15, 0),
        ("BCD K1M1011E0 D20", 5, 0),
        ("DBCD K1M1011E1 D20", 5, 0),
        ("GRY K1M1011E0 D20", 7, 0),
        ("DGRY K1M1011E1 D20", 7, 0),
        ("GBIN K1M1011E0 D20", 6, 0),
        ("DGBIN K1M1011E1 D20", 6, 0),
        ("MVM K1M1011E0 HFFFF D20", 5, 0),
        ("DMVM K1M1011E1 HFFFFFFFF D20", 5, 0),
        ("RAND K1M1011E0 K1M1011E1 D20", 5, 0),
        ("DRAND K1M1011E0 K1M1011E1 D20", 5, 0),
    ] {
        for installed_profile in [false, true] {
            let p = program(site);
            let mut sim = Simulator::default();
            if installed_profile {
                sim.memory = installed();
                ClockState::seed_installed_levels(&mut sim.memory, [true, false, true, false])
                    .unwrap();
            }
            sim.memory.set_input(d("X0"), true);
            complete(&mut sim, &p);
            assert_eq!(
                sim.memory.word(d("D20")),
                if installed_profile { five } else { zero },
                "{site}"
            );
            if site.starts_with('D') || site.starts_with("MUL ") {
                assert_eq!(sim.memory.word(d("D21")), 0, "paired result: {site}");
            }
            assert_no_plc_error(&sim.memory);
            assert!(sim.memory.execution_limitations.is_empty(), "{site}");
            assert_clock_span(&sim.observed_devices);
        }
    }
    for name in ["CMP", "DCMP"] {
        for installed_profile in [false, true] {
            let p = program(&format!("{name} K1M1011E0 K3 M80"));
            let mut sim = Simulator::default();
            if installed_profile {
                sim.memory = installed();
                ClockState::seed_installed_levels(&mut sim.memory, [true, false, true, false])
                    .unwrap();
            }
            sim.memory.set_input(d("X0"), true);
            complete(&mut sim, &p);
            assert_eq!(
                [
                    sim.memory.bit(d("M80")),
                    sim.memory.bit(d("M81")),
                    sim.memory.bit(d("M82"))
                ],
                [installed_profile, false, !installed_profile]
            );
            assert!(sim.memory.execution_limitations.is_empty());
        }
    }
}

#[test]
fn held_pulse_and_off_skip_unknown_read_but_rearmed_pulse_requires_it() {
    for site in ACTION_SITES {
        let p = program(&pulse(site));
        let mut sim = with_memory(installed());
        ClockState::seed_installed_levels(&mut sim.memory, [true, false, true, false]).unwrap();
        sim.memory.set_input(d("X0"), true);
        complete(&mut sim, &p);
        let saved = sim.memory.double_word(d("D20"));
        let saved_rng = sim.memory.random_state;
        for enabled in [true, false] {
            sim.memory.system_relays.restart_installed_run();
            sim.observed_devices.clear();
            sim.memory.set_input(d("X0"), enabled);
            complete(&mut sim, &p);
            assert_eq!(sim.memory.double_word(d("D20")), saved, "{site}");
            assert_eq!(sim.memory.random_state, saved_rng, "{site}");
            assert!(!sim.observed_devices.contains(&d("M1011")), "{site}");
            assert!(sim.memory.execution_limitations.is_empty(), "{site}");
        }
        sim.memory.system_relays.restart_installed_run();
        sim.observed_devices.clear();
        sim.memory.set_input(d("X0"), true);
        complete(&mut sim, &p);
        assert_eq!(sim.memory.double_word(d("D20")), saved, "{site}");
        assert_eq!(sim.memory.random_state, saved_rng, "{site}");
        assert_clock_span(&sim.observed_devices);
        assert!(!sim.memory.execution_limitations.is_empty(), "{site}");
        assert_no_plc_error(&sim.memory);
    }
}

#[test]
fn unknown_divisors_do_not_invent_zero_divisor_errors_or_clear_carry() {
    for name in ["DIV", "DDIV", "DIV16", "DIV32"] {
        let p = program(&format!("{name} K10 K1M1011E1 D20"));
        let mut sim = with_memory(installed());
        sim.memory.set_input(d("X0"), true);
        complete(&mut sim, &p);
        assert_held(&sim.memory);
        assert!(!sim.memory.execution_limitations.is_empty());
        assert_eq!(sim.memory.word(d("D300")), 321);
    }
}

#[test]
fn rand_reads_both_bounds_before_domain_or_seed_and_mvm_reads_mask_before_store() {
    for name in ["RAND", "DRAND"] {
        for lower in ["K-1", "K7"] {
            let words = args(&format!("{name} {lower} K1M1011E1 D20"));
            let a = IndexedRandomAction::parse(&words[0], &words[1..]).unwrap();
            for initial in [None, Some(12345)] {
                let mut m = installed();
                m.random_state = initial;
                assert_clock_error(&a.semantic_error(&m).unwrap_err());
                let mut observed = BTreeSet::new();
                a.execute(&mut m, true, &mut observed);
                assert_clock_span(&observed);
                assert_eq!(m.random_state, initial);
                assert_held(&m);
                assert!(!m.execution_limitations.is_empty());
            }
        }
    }
    for name in ["MVM", "DMVM"] {
        let p = program(&format!("{name} D10 K1M1011E1 D20"));
        let mut sim = with_memory(installed());
        sim.memory.set_double_word(d("D10"), -1);
        sim.memory.set_input(d("X0"), true);
        complete(&mut sim, &p);
        assert_held(&sim.memory);
        assert!(sim.observed_devices.contains(&d("D10")));
        assert_clock_span(&sim.observed_devices);
        assert!(!sim.memory.execution_limitations.is_empty());
    }
}

#[test]
fn checked_predicate_and_expression_inversion_do_not_turn_unknown_into_true() {
    let m = installed();
    for site in [
        "LD= K1M1011E0 K0",
        "DLD<> K1M1011E1 K0",
        "LD& K1M1011E0 K1",
        "DLD| K1M1011E1 K0",
        "LDZ= K1M1011E0 K0 K0",
        "DLDZ<> K0 K0 K1M1011E1",
    ] {
        let words = args(site);
        let predicate = IndexedPredicate::parse(&words[0], &words[1..]).unwrap();
        let mut observed = BTreeSet::new();
        assert_clock_error(&predicate.visit(&m, &mut observed).unwrap_err());
        assert_clock_span(&observed);
        assert!(predicate.evaluate(&m).is_err());
        for inv in ["", "INV\n"] {
            let p = compile_il(&format!("{site}\n{inv}OUT Y0\nEND")).unwrap();
            assert!(!p.networks[0].condition.evaluate(&m), "{site}, INV={inv}");
            let mut sim = with_memory(installed());
            sim.scan(&p, 0);
            assert!(matches!(sim.last_scan, ScanStatus::Fault(_)), "{site}");
            assert!(!sim.memory.bit(d("Y0")));
            assert_clock_span(&sim.observed_devices);
            assert_no_plc_error(&sim.memory);
        }
        assert!(m.execution_limitations.is_empty());
    }
}

#[test]
fn signed_and_logic_contacts_are_eager_on_deciding_and_or_inputs() {
    for (lead, site) in [
        ("LD X0", "AND= K1M1011E0 K0"),
        ("LDI X7", "OR<> K1M1011E1 K0"),
        ("LD X0", "DAND& K1M1011E0 K1"),
        ("LDI X7", "DOR| K1M1011E1 K0"),
    ] {
        let p = compile_il(&format!("{lead}\n{site}\nINV\nOUT Y0\nEND")).unwrap();
        let mut sim = with_memory(installed());
        sim.scan(&p, 0);
        assert!(matches!(sim.last_scan, ScanStatus::Fault(_)), "{site}");
        assert_clock_span(&sim.observed_devices);
        assert!(!sim.memory.bit(d("Y0")));
        assert_no_plc_error(&sim.memory);
    }
}

#[test]
fn difference_contact_shortcuts_skip_clock_read_but_active_inputs_require_it() {
    for (lead, site, shortcut_input, shortcut_result) in [
        ("LD X0", "ANDZ= K1M1011E0 K0 K0", false, false),
        ("LD X0", "DANDZ<> K0 K1M1011E1 K0", false, false),
        ("LD X0", "ORZ= K0 K0 K1M1011E0", true, true),
        ("LD X0", "DORZ<> K1M1011E1 K0 K0", true, true),
    ] {
        let p = compile_il(&format!(
            "{lead}\n{site}\nOUT Y0\nLDI X7\nMOV K321 D300\nEND"
        ))
        .unwrap();
        let mut skipped = with_memory(installed());
        skipped.memory.set_input(d("X0"), shortcut_input);
        complete(&mut skipped, &p);
        assert_eq!(skipped.memory.bit(d("Y0")), shortcut_result);
        assert_eq!(skipped.memory.word(d("D300")), 321);
        assert!(!skipped.observed_devices.contains(&d("M1011")));
        assert!(skipped.memory.execution_limitations.is_empty());
        let mut active = with_memory(installed());
        active.memory.set_input(d("X0"), !shortcut_input);
        active.scan(&p, 0);
        assert!(matches!(active.last_scan, ScanStatus::Fault(_)), "{site}");
        assert_clock_span(&active.observed_devices);
        assert!(!active.memory.bit(d("Y0")));
        assert_eq!(active.memory.word(d("D300")), 0);
    }
}

#[test]
fn bit_number_nc_and_all_rmw_forms_keep_the_checked_read_boundary() {
    for name in ["BLDI", "DBLDI"] {
        let words = args(&format!("{name} K1M1011E0 K0"));
        let p = IndexedBitNumberPredicate::parse(&words[0], &words[1..]).unwrap();
        assert!(p.evaluate(&installed()).is_err());
        let program = compile_il(&format!("{name} K1M1011E0 K0\nINV\nOUT Y0\nEND")).unwrap();
        let mut sim = with_memory(installed());
        sim.scan(&program, 0);
        assert!(matches!(sim.last_scan, ScanStatus::Fault(_)));
        assert!(!sim.memory.bit(d("Y0")));
    }
    for name in ["BOUT", "DBOUT", "BSET", "DBSET", "BRST", "DBRST"] {
        let words = args(&format!("{name} D20E0 K1M1011E1"));
        let a = IndexedBitNumberAction::parse(&words[0], &words[1..]).unwrap();
        for power in [false, true] {
            let mut m = installed();
            let mut observed = BTreeSet::new();
            a.execute(&mut m, power, &mut observed);
            assert_held(&m);
            if power || name.contains("OUT") {
                assert_clock_span(&observed);
                assert!(observed.contains(&d("D20")));
                assert!(!m.execution_limitations.is_empty());
            } else {
                assert!(observed.is_empty());
                assert!(m.execution_limitations.is_empty());
            }
        }
    }
}

#[test]
fn successful_gray_semantic_probe_is_pure_and_unknown_input_does_not_become_zero() {
    for name in ["GRY", "DGRY", "GBIN", "DGBIN"] {
        let words = args(&format!("{name} K1M1011E1 D20"));
        let a = IndexedAbsGrayAction::parse(&words[0], &words[1..]).unwrap();
        let mut m = installed();
        assert_clock_error(&a.semantic_error(&m).unwrap_err());
        assert!(m.execution_limitations.is_empty());
        assert_held(&m);
        ClockState::seed_installed_levels(&mut m, [true, false, true, false]).unwrap();
        assert_eq!(a.semantic_error(&m).unwrap(), None);
        assert_held(&m);
    }
}

#[test]
fn ordinary_public_bits_api_rejects_all_six_invented_pulse_forms() {
    for name in ["BOUT", "DBOUT", "BSET", "DBSET", "BRST", "DBRST"] {
        assert!(compile_il(&format!("LD X0\n{name}P D20 K0\nEND")).is_err());
        let mut p = compile_il(&format!("LD X0\n{name} D20 K0\nEND")).unwrap();
        let action = p.networks[0].actions[0].clone();
        assert!(matches!(action, Action::Bits(_)), "ordinary route: {name}");
        p.networks[0].actions[0] = Action::Pulse(Box::new(action));
        assert!(
            crate::es2_encoder::encode(&p).is_err(),
            "invented public Pulse: {name}"
        );
    }
}

#[test]
fn ordinary_public_bits_api_retains_documented_abs_and_gray_pulse_headers() {
    for (name, operands, header) in [
        ("ABS", "D20", 0xbe10),
        ("DABS", "D20", 0xbf10),
        ("GRY", "D10 D20", 0x7211),
        ("DGRY", "D10 D20", 0x7311),
        ("GBIN", "D10 D20", 0x7411),
        ("DGBIN", "D10 D20", 0x7511),
    ] {
        let mut p = compile_il(&format!("LD X0\n{name} {operands}\nEND")).unwrap();
        let action = p.networks[0].actions[0].clone();
        assert!(matches!(action, Action::Bits(_)));
        p.networks[0].actions[0] = Action::Pulse(Box::new(action));
        let encoded = crate::es2_encoder::encode(&p).unwrap();
        assert_eq!(
            encoded.words[usize::from(encoded.action_pc[0])],
            header,
            "{name}"
        );
        let explicit = compile_il(&format!("LD X0\n{name}P {operands}\nEND")).unwrap();
        assert_eq!(
            crate::es2_encoder::encode(&explicit).unwrap().words,
            encoded.words
        );
    }
}

#[test]
fn clock_failure_precedes_later_bad_sources_and_target_modifiers() {
    let mut m = installed();
    m.set_word(d("D1195"), -30_000); // F7 is unrelated to the E1 clock address.
    for site in [
        "MOV K1M1011E1 D20F7",
        "CMP K1M1011E1 K3 M80F7",
        "MUL K1M1011E1 K3 D20F7",
        "MUL16 K1M1011E1 K3 D20F7",
        "BCD K1M1011E1 D20F7",
        "GRY K1M1011E1 D20F7",
        "MVM K1M1011E1 HFFFF D20F7",
        "RAND K1M1011E1 K1M1011E1 D20F7",
    ] {
        let mut observed = BTreeSet::new();
        assert_clock_error(&prepare(site, &m, &mut observed).unwrap_err());
        assert_clock_span(&observed);
        assert!(!observed.contains(&d("D1195")), "late target: {site}");
    }
    for site in ["ADD K1M1011E1 D0F7 D20", "CMP K1M1011E1 D0F7 M80"] {
        let mut observed = BTreeSet::new();
        assert_clock_error(&prepare(site, &m, &mut observed).unwrap_err());
        assert!(!observed.contains(&d("D1195")), "late source: {site}");
    }
    for operands in ["K1M1011E1 D0F7", "D0F7 K1M1011E1"] {
        let predicate = IndexedPredicate::parse("LD=", &args(operands)).unwrap();
        let mut observed = BTreeSet::new();
        let error = predicate.visit(&m, &mut observed).unwrap_err();
        if operands.starts_with('K') {
            assert_clock_error(&error);
            assert_clock_span(&observed);
            assert!(!observed.contains(&d("D1195")));
        } else {
            assert!(!error.contains("1011"));
            assert!(observed.contains(&d("D1195")));
            assert!(!observed.contains(&d("D1182")) && !observed.contains(&d("M1011")));
        }
    }
    // Address-only resolution is still available without seeding clock data.
    for site in ["MOV K1M1011E1 D20", "CMP K1M1011E1 K0 M80"] {
        let p = program(site);
        match &p.networks[0].actions[0] {
            Action::IndexedScalar(a) => {
                a.resolve(&m).unwrap();
            }
            Action::IndexedComparison(a) => {
                a.resolve(&m).unwrap();
            }
            _ => unreachable!(),
        }
    }
    let predicate = IndexedPredicate::parse("LD=", &args("K1M1011E1 K0")).unwrap();
    predicate.resolve(&m).unwrap();
    assert_held(&m);
}
