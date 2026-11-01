use crate::calendar_tail::{CalendarTailProfile as Profile, decode_time_tail};
use crate::execution_plan::PlanOp;
use crate::plc::{Device, Simulator, compile_il};
use crate::random_tail::{OperandTailControl as Control, OperandTailOwner as Owner};

fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn sim() -> Simulator {
    let mut sim = Simulator::default();
    sim.memory.calendar_tail_profile = Profile::CurrentCommgr;
    for (base, values) in [(0, [1, 2, 3]), (10, [0, 0, 1]), (123, [0, 0, 1])] {
        for (i, value) in values.into_iter().enumerate() {
            sim.memory.set_word(d(&format!("D{}", base + i)), value);
        }
    }
    sim.memory.set_input(d("X0"), true);
    sim
}
#[test]
fn original_four_words_and_typed_owner_are_preserved() {
    let p = compile_il("LD X0\nTADD D0 D123F0 D125F0\nEND").unwrap();
    let e = crate::es2_encoder::encode(&p).unwrap();
    assert_eq!(
        &e.words[1..8],
        &[0x6401, 0x0086, 0x0086, 0xf6f6, 0x0086, 0xfaf6, 0x0086]
    );
    assert_eq!(e.action_pc, [1]);
    assert_eq!(e.next_pc, [1, 4, 5, 6, 7, 8, 0]);
    assert_eq!(
        e.execution_plan.operand_tail_owners.get(&1),
        Some(&Owner::CalendarTime)
    );
    assert_eq!(
        decode_time_tail(&e.words[1..8]).unwrap(),
        [Some(Control::Np), None, Some(Control::Mps), None]
    );
    assert!(crate::random_tail::decode_drand_tail(&e.words[1..8]).is_err());
    for word in [0xf7f6, 0xf9f6, 0xfbf6, 0xfdf6] {
        let mut words = e.words[1..8].to_vec();
        words[3] = word;
        assert!(decode_time_tail(&words).is_err());
    }
}
#[test]
fn source_owner_validator_rejects_tampering_and_orphan_controls() {
    let p = compile_il("LD X0\nTADD D0 D10 D20\nEND").unwrap();
    let e = crate::es2_encoder::encode(&p).unwrap();
    let mut bad = e.clone();
    bad.execution_plan
        .operand_tail_owners
        .insert(1, Owner::Drand);
    assert!(crate::random_tail::validate_layout(&p, &bad).is_err());
    let mut bad = e.clone();
    bad.words[4] ^= 0x100;
    assert!(crate::random_tail::validate_layout(&p, &bad).is_err());
    let mut bad = e.clone();
    bad.execution_plan.nodes[2].op = PlanOp::OperandTailControl {
        header: 1,
        op: Control::Np,
    };
    assert!(crate::random_tail::validate_layout(&p, &bad).is_err());
    let mut bad = e;
    bad.execution_plan.nodes[1].fallthrough = 8;
    assert!(crate::random_tail::validate_layout(&p, &bad).is_err());
}
#[test]
fn success_visits_four_tails_but_off_error_and_documented_skip_them() {
    let p = compile_il("LD X0\nTADD D0 D10 D20\nEND").unwrap();
    assert!(p.requires_vm());
    for (on, bad, profile, next) in [
        (true, false, Profile::CurrentCommgr, 4),
        (false, false, Profile::CurrentCommgr, 8),
        (true, true, Profile::CurrentCommgr, 8),
        (true, false, Profile::Documented, 8),
    ] {
        let mut s = sim();
        s.memory.set_input(d("X0"), on);
        s.memory.calendar_tail_profile = profile;
        if bad {
            s.memory.set_word(d("D12"), 60);
        }
        s.begin_plan_scan(&p, 0, true).unwrap();
        s.execute_plan_step(&p).unwrap();
        assert_eq!(s.execute_plan_step(&p).unwrap().next_pc, next);
        if next == 4 {
            for pc in 4..8 {
                assert_eq!(s.execute_plan_step(&p).unwrap().pc, pc);
            }
        }
    }
}
#[test]
fn np_tail_history_is_not_reset_by_unvisited_off_or_held_p_visits() {
    for op in ["TADD", "TADDP"] {
        let p = compile_il(&format!("LD X0\n{op} D0 D123F0 D20\nOUT Y0\nEND")).unwrap();
        let mut s = sim();
        for (on, expected) in [
            (true, true),
            (true, if op == "TADD" { false } else { true }),
            (false, false),
            (true, false),
        ] {
            s.memory.set_input(d("X0"), on);
            s.scan(&p, 0);
            assert!(
                s.memory.execution_limitations.is_empty(),
                "{:?}",
                s.memory.execution_limitations
            );
            assert_eq!(s.memory.bit(d("Y0")), expected, "{op} on={on}");
        }
    }
}
#[test]
fn time_profile_is_a_scan_epoch_and_survives_restart() {
    let p = compile_il("LD X0\nTADD D0 D10 D20\nEND").unwrap();
    let mut s = sim();
    s.begin_plan_scan(&p, 0, true).unwrap();
    s.execute_plan_step(&p).unwrap();
    s.memory.calendar_tail_profile = Profile::Documented;
    assert!(
        s.execute_plan_step(&p)
            .unwrap_err()
            .contains("profile changed")
    );
    s.memory.calendar_tail_profile = Profile::CurrentCommgr;
    s.restart_preserving_file_store();
    assert_eq!(s.memory.calendar_tail_profile, Profile::CurrentCommgr);
    assert!(
        crate::calendar_tail::validate_program_profile(&p, Profile::UnqualifiedNative).is_err()
    );
    let safe = compile_il("LD X0\nMOV K1 D0\nEND").unwrap();
    assert!(
        crate::calendar_tail::validate_program_profile(&safe, Profile::UnqualifiedNative).is_ok()
    );
    let indexed_compare = compile_il("LD X0\nTCMP D0E0 K0 K0 D20 M10\nEND").unwrap();
    assert!(
        crate::calendar_tail::validate_program_profile(
            &indexed_compare,
            Profile::UnqualifiedNative
        )
        .is_err()
    );
    let ordinary_compare = compile_il("LD X0\nTCMP K0 K0 K0 D20 M10\nEND").unwrap();
    assert!(
        crate::calendar_tail::validate_program_profile(
            &ordinary_compare,
            Profile::UnqualifiedNative
        )
        .is_ok()
    );
}
#[test]
fn native_outcome_qualification_rejects_disposition_or_scope_divergence() {
    use crate::time_ops::CalendarTimeOutcome as O;
    for same in [O::Nominal, O::ArithmeticSuccess, O::PlcError { offset: 3 }] {
        assert!(crate::calendar_tail::qualify_outcomes(&same, &same).is_ok());
    }
    for (local, native) in [
        (O::ArithmeticSuccess, O::PlcError { offset: 3 }),
        (O::PlcError { offset: 3 }, O::ArithmeticSuccess),
        (O::PlcError { offset: 1 }, O::PlcError { offset: 3 }),
        (O::Limitation("local".into()), O::Nominal),
        (O::Nominal, O::Limitation("native".into())),
    ] {
        assert!(crate::calendar_tail::qualify_outcomes(&local, &native).is_err());
    }
}
