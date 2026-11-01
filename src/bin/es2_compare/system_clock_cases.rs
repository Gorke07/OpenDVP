//! Diagnostic clock capability sources. Equality to stored native values does
//! not prove automatic clock progression: report interpretation is explicit in
use super::{Case, case};
use opendvp::es2_bridge::Target;

fn clock_body() -> &'static str {
    "LD M1002\nAND M1011\nSET M210\nLD M1002\nAND M1012\nSET M211\nLD M1002\nAND M1013\nSET M212\nLD M1002\nAND M1014\nSET M213\nLD M1011\nOUT Y0\nLD M1012\nOUT Y1\nLD M1013\nOUT Y2\nLD M1014\nOUT Y3\nLD M1000\nINC D10\nEND"
}
fn enable_stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (vec![("X0", false), ("X1", true), ("X2", false)], 1),
        (vec![("X1", false), ("X0", true)], 4),
        (vec![("X0", false)], 2),
        (vec![("X0", true)], 4),
        (vec![("X0", false)], 1),
    ]
}
fn counter_body(seed: i16) -> String {
    format!(
        "LD X1\nMOV K{seed} D1015\nLD X0\nOUT M1015\nLD M1000\nMOV D1015 D0\nLD M1015\nOUT Y0\nLD M1000\nINC D10\nEND"
    )
}
pub(super) fn cases(_target: Target) -> Vec<Case> {
    let mut out = vec![
        case(
            "system_clock_fixed_100ms_phase_and_first_body",
            clock_body(),
            vec![(vec![], 20)],
        ),
        // Covers a full 60 s nominal virtual period, including the 30 s half
        // boundary; old installed M1014 has a statically proven bad threshold.
        case(
            "system_clock_fixed_100ms_minute_boundary",
            clock_body(),
            vec![(vec![], 610)],
        ),
    ];
    for seed in [0, 123, 32760, 32767] {
        out.push(case(
            &format!("system_clock_M1015_seed_{seed}_held_off_rearm"),
            &counter_body(seed),
            enable_stages(),
        ));
    }
    out.push(case("system_clock_M1015_reseed_while_enabled",
        "LD X1\nMOV K123 D1015\nLD X0\nOUT M1015\nLD X2\nMOV K32767 D1015\nLD M1000\nMOV D1015 D0\nINC D10\nEND",
        vec![(vec![("X0",false),("X1",true),("X2",false)],1),(vec![("X1",false),("X0",true)],3),(vec![("X2",true)],1),(vec![("X2",false)],3),(vec![("X0",false)],2)]));
    out.push(case("system_clock_M1015_enabled_then_disabled_before_END",
        "LD M1002\nMOV K123 D1015\nLD M1000\nSET M1015\nMOV D1015 D0\nRST M1015\nMOV D1015 D1\nINC D10\nEND",vec![(vec![],5)]));
    out.push(case("system_clock_M1015_disabled_then_enabled_before_END",
        "LD M1002\nMOV K123 D1015\nLD M1000\nRST M1015\nMOV D1015 D0\nSET M1015\nMOV D1015 D1\nINC D10\nEND",vec![(vec![],5)]));
    for visits in [1, 16, 64] {
        out.push(case(&format!("system_clock_M1015_instruction_visits_{visits}_END_boundary"),
            &format!("LD M1002\nMOV K123 D1015\nLD M1000\nSET M1015\nMOV D1015 D0\nFOR K{visits}\nLD M1000\nMOV D1015 D1\nINC D20\nNEXT\nLD M1000\nMOV D1015 D2\nINC D10\nEND"),vec![(vec![],3)]));
    }
    out.push(case("system_clock_M1015_explicit_zero_while_disabled_then_reenable",
        "LD X1\nMOV K32760 D1015\nLD X0\nOUT M1015\nLD X2\nMOV K0 D1015\nLD M1000\nMOV D1015 D0\nINC D10\nEND",
        vec![(vec![("X0",false),("X1",true),("X2",false)],1),(vec![("X1",false),("X0",true)],3),(vec![("X0",false),("X2",true)],1),(vec![("X2",false)],2),(vec![("X0",true)],3)]));
    out
}

// These sources are deliberately NOT included in cases(): the native bridge
// still requires D1039=100. They are local phase tests and future diagnostics,
// not authorization to generalize fixed100ms timers or native timing fidelity.
#[allow(dead_code)]
pub(super) fn variable_quantum_proposals(_target: Target) -> Vec<Case> {
    const QUANTA: [i16; 9] = [1, 5, 7, 13, 499, 500, 999, 1000, 32767];
    fn program(quanta: &[i16]) -> String {
        let mut source = "LD M1000\nMOV K1 D1039\n".to_owned();
        for (selector, quantum) in quanta.iter().enumerate().skip(1) {
            for bit in 0..4 {
                let on = selector & (1 << bit) != 0;
                let instruction = match (bit == 0, on) {
                    (true, true) => "LD",
                    (true, false) => "LDI",
                    (false, true) => "AND",
                    (false, false) => "ANI",
                };
                source.push_str(&format!("{instruction} X{bit}\n"));
            }
            source.push_str(&format!("MOV K{quantum} D1039\n"));
        }
        source.push_str("LD M1000\nMOV D1039 D100\n");
        source.push_str(clock_body());
        source
    }
    fn stages(sequence: &[(usize, usize)]) -> Vec<(Vec<(&'static str, bool)>, usize)> {
        const INPUTS: [&str; 4] = ["X0", "X1", "X2", "X3"];
        sequence
            .iter()
            .map(|(selector, scans)| {
                (
                    INPUTS
                        .iter()
                        .enumerate()
                        .map(|(bit, name)| (*name, selector & (1 << bit) != 0))
                        .collect(),
                    *scans,
                )
            })
            .collect()
    }
    let source = program(&QUANTA);
    vec![
        case(
            "system_clock_proposal_variable_positive_forward",
            &source,
            stages(&[
                (0, 12),
                (1, 12),
                (2, 23),
                (3, 13),
                (4, 5),
                (5, 6),
                (6, 5),
                (7, 5),
                (8, 3),
            ]),
        ),
        case(
            "system_clock_proposal_variable_positive_reverse",
            &source,
            stages(&[
                (8, 5),
                (7, 6),
                (6, 7),
                (5, 6),
                (4, 7),
                (3, 23),
                (2, 23),
                (1, 12),
                (0, 12),
            ]),
        ),
        case(
            "system_clock_proposal_variable_positive_boundary_zigzag",
            &source,
            stages(&[
                (0, 4),
                (1, 1),
                (0, 1),
                (2, 1),
                (3, 1),
                (4, 1),
                (0, 1),
                (5, 1),
                (6, 1),
                (0, 1),
                (7, 1),
                (8, 1),
                (0, 1),
                (8, 1),
                (0, 5),
                (1, 1),
                (2, 1),
                (3, 1),
                (4, 1),
                (5, 1),
                (6, 1),
                (7, 1),
                (8, 1),
            ]),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendvp::{
        plc::{self, Area, Device, Simulator},
        program_control::ControlProfile,
        system_relays::{ClockProfile, ClockState},
    };
    fn device(area: Area, index: u16) -> Device {
        Device { area, index }
    }

    #[test]
    fn variable_positive_quantum_proposals_predict_body_and_end_phases_locally() {
        for target in [Target::Es2, Target::Ss2] {
            for seed in [[false; 4], [true, false, true, true]] {
                for case in variable_quantum_proposals(target) {
                    let p = plc::compile_il(&case.source).unwrap();
                    let mut sim = Simulator::default();
                    sim.control.profile = ControlProfile::InstalledLegacy;
                    sim.memory.data_register_limit =
                        Some(if target == Target::Ss2 { 4999 } else { 9999 });
                    sim.memory.set_system(1039, true);
                    sim.memory.set_word(device(Area::D, 1039), 100);
                    sim.memory
                        .system_relays
                        .set_profile(ClockProfile::InstalledFixed);
                    ClockState::seed_installed_levels(&mut sim.memory, seed).unwrap();
                    let mut total_ms = 0u64;
                    let mut before = seed;
                    for (changes, scans) in &case.stages {
                        for (name, value) in changes {
                            sim.memory.set_input(name.parse().unwrap(), *value);
                        }
                        for _ in 0..*scans {
                            sim.scan(&p, 100);
                            assert!(
                                sim.memory.execution_limitations.is_empty(),
                                "{}: {:?}",
                                case.name,
                                sim.memory.execution_limitations
                            );
                            let quantum = sim.memory.word(device(Area::D, 1039));
                            assert!(quantum > 0);
                            assert_eq!(sim.memory.word(device(Area::D, 100)), quantum);
                            total_ms += quantum as u64;
                            let end = [
                                total_ms % 10 >= 5,
                                total_ms % 100 >= 50,
                                total_ms % 1000 >= 500,
                                false,
                            ];
                            for slot in 0..4 {
                                assert_eq!(
                                    sim.memory.bit(device(Area::Y, slot as u16)),
                                    before[slot],
                                    "{} scan{} body slot{slot}",
                                    case.name,
                                    sim.scans
                                );
                                assert_eq!(
                                    sim.memory.system_bit(1011 + slot as u16),
                                    end[slot],
                                    "{} scan{} END slot{slot}",
                                    case.name,
                                    sim.scans
                                );
                                assert_eq!(
                                    sim.memory.bit(device(Area::M, 210 + slot as u16)),
                                    seed[slot],
                                    "{} first-body input capture slot{slot}",
                                    case.name
                                );
                            }
                            assert_eq!(sim.memory.word(device(Area::D, 10)), sim.scans as i16);
                            before = end;
                        }
                    }
                }
            }
        }
    }
}
