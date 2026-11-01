//! Independent RTC service scheduling and original source-PC contracts.
use crate::plc::{Action, Device, Memory, Simulator, compile_il};
use crate::rtc_clock::{self, RtcAction, RtcAdvance, RtcProfile, RtcServiceSchedule as Schedule};

fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn installed(seed: [u8; 7], second: u8) -> Memory {
    let mut m = Memory::default();
    m.rtc_clock
        .configure_installed_shadow(seed, second)
        .unwrap();
    m.rtc_service_schedule = Schedule::ExplicitBoundaries;
    m
}
fn public(m: &Memory) -> [i16; 7] {
    std::array::from_fn(|n| m.word(d(&format!("D{}", 1319 - n))))
}

#[test]
fn same_second_settlement_is_a_fixed_point_for_raw_fields_and_both_correction_levels() {
    for seed in [
        [24, 4, 2, 29, 12, 30, 7],
        [99, 7, 12, 31, 23, 59, 59],
        [255; 7],
    ] {
        for correction in [false, true] {
            for century in [false, true] {
                let mut m = installed(seed, 7);
                m.set_system(1016, century);
                m.set_system(1017, correction);
                let event = rtc_clock::settle_installed_service(&mut m, 7).unwrap();
                assert!(!event.native_second_changed);
                let state = (m.rtc_clock.shadow(), public(&m), m.system_bit(1082));
                for _ in 0..5 {
                    rtc_clock::end_scan(&mut m, RtcAdvance::InstalledSampledSecond(7));
                    assert_eq!(
                        state,
                        (m.rtc_clock.shadow(), public(&m), m.system_bit(1082))
                    );
                }
                assert_eq!(
                    public(&m)[0],
                    i16::from(m.rtc_clock.shadow().unwrap()[0]) + if century { 2000 } else { 0 }
                );
                if correction {
                    assert_eq!(m.rtc_clock.shadow().unwrap()[6], 0);
                }
            }
        }
    }
}

#[test]
fn adjacent_external_event_retains_installed_no_date_carry_and_reports_hour_input() {
    let mut m = installed([24, 4, 2, 29, 23, 59, 59], 59);
    let event = rtc_clock::settle_installed_service(&mut m, 0).unwrap();
    assert!(event.native_second_changed);
    assert_eq!(m.rtc_clock.shadow(), Some([24, 4, 2, 29, 0, 0, 0]));
    assert_eq!(public(&m), [24, 4, 2, 29, 0, 0, 0]);
    assert!(
        !rtc_clock::settle_installed_service(&mut m, 0)
            .unwrap()
            .native_second_changed
    );
}

#[test]
fn repeated_correction_events_cannot_be_replaced_with_one_coalesced_delta() {
    let mut repeated = installed([24, 4, 2, 29, 12, 30, 0], 0);
    repeated.set_system(1017, true);
    let mut coalesced = repeated.clone();
    for second in 1..=31 {
        rtc_clock::settle_installed_service(&mut repeated, second).unwrap();
    }
    rtc_clock::settle_installed_service(&mut coalesced, 31).unwrap();
    assert_eq!(repeated.rtc_clock.shadow(), Some([24, 4, 2, 29, 12, 30, 0]));
    assert_eq!(
        coalesced.rtc_clock.shadow(),
        Some([24, 4, 2, 29, 12, 31, 0])
    );
}

#[test]
fn settled_service_rejects_undeclared_schedule_profile_or_phase_without_mutating_clock() {
    for kind in 0..4 {
        let mut m = installed([24, 4, 2, 29, 12, 30, 7], 7);
        let sample = if kind == 3 { 60 } else { 7 };
        match kind {
            0 => m.rtc_service_schedule = Schedule::EndOfScan,
            1 => m.rtc_clock.configure_frozen([24, 4, 2, 29, 12, 30, 7]),
            2 => m.rtc_clock = Default::default(),
            _ => {}
        }
        let before = (m.rtc_clock.profile(), m.rtc_clock.shadow(), public(&m));
        assert!(rtc_clock::settle_installed_service(&mut m, sample).is_err());
        assert_eq!(
            before,
            (m.rtc_clock.profile(), m.rtc_clock.shadow(), public(&m))
        );
    }
}

#[test]
fn explicit_schedule_has_no_implicit_end_service_and_default_retains_it() {
    let p = compile_il("LDI X7\nTRD D100\nSET M1017\nEND").unwrap();
    for schedule in [Schedule::EndOfScan, Schedule::ExplicitBoundaries] {
        let mut s = Simulator::default();
        s.memory = installed([24, 4, 2, 29, 12, 30, 31], 7);
        s.memory.rtc_service_schedule = schedule;
        s.scan(&p, 0);
        assert_eq!(s.last_scan, crate::plc::ScanStatus::Complete);
        if schedule == Schedule::ExplicitBoundaries {
            assert_eq!(
                s.memory.rtc_clock.shadow(),
                Some([24, 4, 2, 29, 12, 30, 31])
            );
            assert!(!s.memory.system_bit(1082));
            rtc_clock::settle_installed_service(&mut s.memory, 7).unwrap();
        }
        assert_eq!(s.memory.rtc_clock.shadow(), Some([24, 4, 2, 29, 12, 31, 0]));
        assert!(s.memory.system_bit(1082));
    }
}

#[test]
fn active_cursor_locks_service_schedule_and_cpu_restart_preserves_selection() {
    let p = compile_il("LDI X7\nTRD D100\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory = installed([24, 4, 2, 29, 12, 30, 7], 7);
    s.begin_plan_scan(&p, 0, true).unwrap();
    s.memory.rtc_service_schedule = Schedule::EndOfScan;
    assert!(
        s.execute_plan_step(&p)
            .unwrap_err()
            .contains("RTC service schedule")
    );
    s.memory.rtc_service_schedule = Schedule::ExplicitBoundaries;
    s.restart_preserving_file_store();
    assert_eq!(s.memory.rtc_service_schedule, Schedule::ExplicitBoundaries);
}

#[test]
fn public_normal_pulse_and_manual_legacy_actions_all_require_original_three_word_vm_nodes() {
    for name in ["TRD", "TRDP", "TWR", "TWRP"] {
        for operand in ["D100", "D100E0", "T100F7", "C100E1"] {
            let p = compile_il(&format!("LD X0\n{name} {operand}\nEND")).unwrap();
            assert!(p.requires_vm());
            let e = crate::es2_encoder::encode(&p).unwrap();
            assert_eq!(e.action_pc, [1]);
            assert_eq!(e.next_pc, [1, 4, 0]);
            assert_eq!(
                e.words[1],
                (if name.starts_with("TRD") {
                    0x6a01
                } else {
                    0x6c01
                }) | if name.ends_with('P') { 0x10 } else { 0 }
            );
            assert!(e.execution_plan.operand_tail_owners.is_empty());
            assert_eq!(e.execution_plan.nodes[1].fallthrough, 4);
            assert!(matches!(
                e.preflight.get(&1),
                Some(crate::es2_encoder::PreflightCheck::IndexedRtc(0))
            ));
        }
    }
    for pulse in [false, true] {
        let mut p = compile_il("LD X0\nTRD D100\nEND").unwrap();
        let action = Action::Rtc(RtcAction::Read { dest: d("D100") });
        p.networks[0].actions[0] = if pulse {
            Action::Pulse(Box::new(action))
        } else {
            action
        };
        assert!(p.requires_vm());
        let mut s = Simulator::default();
        s.memory
            .rtc_clock
            .configure_frozen([24, 4, 2, 29, 12, 30, 7]);
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 0);
        assert_eq!(s.memory.word(d("D100")), 24);
        assert_eq!(s.memory.word(d("D106")), 7);
    }
}

#[test]
fn settled_endpoint_preserves_own_twr_commit_while_republishing_transient_public_aliases() {
    let mut m = installed([24, 4, 2, 29, 12, 30, 7], 7);
    for (i, v) in [4, 2, 12, 15, 15, 27, 30].into_iter().enumerate() {
        m.set_word(d(&format!("D{}", 10 + i)), v);
    }
    let action = crate::indexed_rtc::IndexedRtcAction::parse("TWR", &["D10E0".into()]).unwrap();
    assert_eq!(
        action.execute(&mut m, true, &mut Default::default()),
        rtc_clock::RtcOutcome::Complete
    );
    assert_eq!(m.rtc_clock.profile(), Some(RtcProfile::InstalledShadow));
    rtc_clock::settle_installed_service(&mut m, 7).unwrap();
    assert_eq!(m.rtc_clock.shadow(), Some([4, 2, 12, 15, 15, 27, 30]));
    m.set_word(d("D1319"), 999);
    rtc_clock::settle_installed_service(&mut m, 7).unwrap();
    assert_eq!(public(&m), [4, 2, 12, 15, 15, 27, 30]);
    assert_eq!(m.rtc_clock.shadow(), Some([4, 2, 12, 15, 15, 27, 30]));
}
