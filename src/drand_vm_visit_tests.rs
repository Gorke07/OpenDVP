//! A DRAND header has different native PC lengths on active/fresh-false visits.
//! Exact per-visit traces matter when an addressed action requires the source VM.
use crate::plc::{Device, Program, Simulator, compile_il};

fn d(name: &str) -> Device {
    name.parse().unwrap()
}
fn trace(sim: &mut Simulator, program: &Program) -> Vec<(u16, u16)> {
    sim.memory.random_tail_profile = crate::random_tail::RandomTailProfile::CurrentCommgr;
    sim.begin_plan_scan(program, 0, false).unwrap();
    let mut visits = Vec::new();
    loop {
        let step = sim.execute_plan_step(program).unwrap();
        visits.push((step.pc, step.next_pc));
        if step.scan_boundary {
            break;
        }
    }
    visits
}
fn program(instruction: &str) -> Program {
    let program = compile_il(&format!(
        "LD X0\n{instruction}\nMOV K99 D40\nLD X1\nSWAP D50E0\nEND"
    ))
    .unwrap();
    assert!(program.requires_vm());
    program
}

#[test]
fn drand_vm_skips_real_operand_tail_on_active_success_and_domain_error() {
    for instruction in ["DRAND K7 K7 D20", "DRAND K-1 K7 D20", "DRAND K10 K1 D20"] {
        let program = program(instruction);
        for power in [false, true] {
            let mut sim = Simulator::default();
            sim.memory.set_input(d("X0"), power);
            let visits = trace(&mut sim, &program);
            assert!(
                visits.contains(&(1, if power { 14 } else { 8 })),
                "{instruction} {power}: {visits:?}"
            );
            assert_eq!(
                visits.iter().filter(|(pc, _)| (8..14).contains(pc)).count(),
                if power { 0 } else { 6 }
            );
            assert_eq!(sim.memory.word(d("D40")), if power { 99 } else { 0 });
            if power && instruction == "DRAND K7 K7 D20" {
                assert_eq!(sim.memory.double_word(d("D20")), 7);
            }
            if power && instruction != "DRAND K7 K7 D20" {
                assert_eq!(sim.memory.word(d("D1067")), 0x0e1a);
                assert_eq!(sim.memory.word(d("D1068")), 6); // header PC1 + operand error offset5
            }
        }
    }
}

#[test]
fn drandp_vm_uses_history_before_qualification_for_held_and_first_off_skip() {
    let program = program("DRANDP K7 K7 D20");
    let mut sim = Simulator::default();
    for (power, next) in [
        (false, 8),
        (false, 8),
        (true, 14),
        (true, 14),
        (false, 14),
        (false, 8),
        (true, 14),
    ] {
        let seed = sim.memory.random_state;
        sim.memory.set_input(d("X0"), power);
        let visits = trace(&mut sim, &program);
        assert!(visits.contains(&(1, next)), "{power}: {visits:?}");
        assert_eq!(
            visits.iter().filter(|(pc, _)| (8..14).contains(pc)).count(),
            if next == 8 { 6 } else { 0 }
        );
        if !power {
            assert_eq!(sim.memory.random_state, seed);
        }
    }
    assert_eq!(sim.memory.double_word(d("D20")), 7);
}
