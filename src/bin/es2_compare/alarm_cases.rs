//! Bounded independent ANS/ANR/ANRP sources. No manufactured result values.
//! Installed timer-class units and visited-handler monitor semantics remain
//! distinct from the newer PDF's universal100ms wording and hardware retention.
use super::{Case, case};
use opendvp::es2_bridge::Target;
type Stages = Vec<(Vec<(&'static str, bool)>, usize)>;

fn stages() -> Stages {
    vec![
        (
            vec![
                ("X1", true),
                ("X0", false),
                ("X2", false),
                ("X3", true),
                ("X4", false),
                ("X5", false),
                ("X6", true),
            ],
            1,
        ),
        (vec![("X1", false), ("X0", true)], 1),
        (vec![], 4),
        (vec![("X0", false)], 1),
        (vec![], 2),
        (vec![("X0", true), ("X2", true)], 1),
        (vec![], 2),
        (vec![("X0", false), ("X2", false), ("X4", true)], 1),
        (vec![], 1),
    ]
}
fn monitor() -> &'static str {
    "LD M1048\nOUT Y30\nLD M1049\nOUT Y31\nLD X6\nMOV D1049 D700\nINC D900\nLD M1067\nOUT M510\nLD M1068\nOUT M511\nEND"
}
fn init() -> &'static str {
    "ZRST S912 S1023\nRST M1048\nSET M1049\nMOV K777 D1049"
}
fn fixture(name: &str, seed: &str, body: &str, stages: Stages) -> Case {
    case(
        &format!("alarm_{name}"),
        &format!("LD X1\n{seed}\n{body}"),
        stages,
    )
}
fn timer_read(timer: u16) -> String {
    format!("LD T{timer}\nOUT Y20\nLD X6\nMOV T{timer} D800\n")
}
fn words(base: u16, values: &[i16]) -> String {
    values
        .iter()
        .enumerate()
        .map(|(n, v)| format!("MOV K{v} D{}\n", base + n as u16))
        .collect()
}

pub fn cases(_target: Target) -> Vec<Case> {
    let mut out = Vec::new();
    for timer in [0, 63, 64, 126, 127, 128, 183] {
        for fine in [false, true] {
            if fine && ![64, 126].contains(&timer) {
                continue;
            }
            for preset in [1, 5] {
                for alarm in [912, 1023] {
                    out.push(fixture(
                        &format!("ANS_T{timer}_K{preset}_S{alarm}_M1028_{fine}"),
                        &format!(
                            "{}\nRST T{timer}\n{} M1028",
                            init(),
                            if fine { "SET" } else { "RST" }
                        ),
                        &format!(
                            "LD X0\nANS T{timer} K{preset} S{alarm}\n{}{}",
                            timer_read(timer),
                            monitor()
                        ),
                        stages(),
                    ));
                }
            }
        }
    }
    // Current/contact are independently seeded; DIV16 is a genuine supported
    // numeric T writer. The initial TMR seed must be skipped on later scans,
    // otherwise a false TMR visit would destroy the very state being tested.
    for timer in [0, 64, 127, 183] {
        for value in [-1, 0, 1, 2, 32767] {
            for contact in [false, true] {
                let prelude = format!(
                    "LDI X1\nCJ P200\nLD X1\n{}\nRST T{timer}\n{}DIV16 K{value} K1 T{timer}\nP200\n",
                    init(),
                    if contact {
                        format!("TMR T{timer} K0\n")
                    } else {
                        String::new()
                    }
                );
                let source = format!(
                    "{prelude}LD X0\nANS T{timer} K1 S999\n{}{}",
                    timer_read(timer),
                    monitor()
                );
                let mut scan = stages();
                scan[0].0.push(("X0", true));
                out.push(case(
                    &format!("alarm_ANS_preseed_T{timer}_{value}_oldcontact{contact}"),
                    &source,
                    scan,
                ));
            }
        }
    }
    for timer in [0, 127, 183] {
        for value in [32766, 32767] {
            let mut scan = stages();
            scan[0].0.push(("X0", true));
            out.push(fixture(
                &format!("ANS_T{timer}_K32767_preseed{value}"),
                &format!("{}\nRST T{timer}\nDIV16 K{value} K1 T{timer}", init()),
                &format!(
                    "LD X0\nANS T{timer} K32767 S1023\n{}{}",
                    timer_read(timer),
                    monitor()
                ),
                scan,
            ));
        }
    }
    for op in ["ANR", "ANRP"] {
        for alarms in [
            &[912, 920, 1023][..],
            &[1023, 913, 912][..],
            &[1023][..],
            &[][..],
        ] {
            let extra = alarms
                .iter()
                .map(|s| format!("SET S{s}\n"))
                .collect::<String>();
            out.push(fixture(
                &format!(
                    "{op}_lowest_priority_{}",
                    alarms
                        .iter()
                        .map(u16::to_string)
                        .collect::<Vec<_>>()
                        .join("_")
                ),
                &format!("{}\n{extra}", init()),
                &format!("LD X0\n{op}\n{}", monitor()),
                stages(),
            ));
        }
        out.push(fixture(
            &format!("{op}_handler_firstfalse_heldtrue_firstfalse_secondfalse_monitor"),
            &format!("{}\nSET S920", init()),
            &format!(
                "LD X0\n{op}\nLD X2\nSET S912\nLD X4\nRST S920\n{}",
                monitor()
            ),
            vec![
                (
                    vec![
                        ("X1", true),
                        ("X0", false),
                        ("X2", false),
                        ("X4", false),
                        ("X6", true),
                    ],
                    1,
                ),
                (vec![("X1", false)], 1),
                (vec![("X0", true), ("X2", true)], 1),
                (vec![("X2", false)], 2),
                (vec![("X0", false)], 1),
                (vec![], 1),
                (vec![("X0", true)], 1),
                (vec![], 2),
                (vec![("X0", false)], 2),
            ],
        ));
    }
    for enabled in [false, true] {
        for empty in [false, true] {
            for before in [false, true] {
                let flags = "LD X3\nOUT M1049\n";
                let body = format!(
                    "{}LD X0\nANR\n{}LD X2\nSET S912\nLD X4\nRST S1023\n{}",
                    if before { flags } else { "" },
                    if before { "" } else { flags },
                    monitor()
                );
                let mut scan = stages();
                scan[0].0.push(("X3", enabled));
                scan.extend([
                    (vec![("X3", !enabled), ("X0", false)], 2),
                    (vec![("X3", enabled)], 2),
                ]);
                out.push(fixture(
                    &format!(
                        "monitor_enabled{enabled}_empty{empty}_enable_before{before}_stale_D1049"
                    ),
                    &format!(
                        "{}\nSET M1048\n{} S1023",
                        init(),
                        if empty { "RST" } else { "SET" }
                    ),
                    &body,
                    scan,
                ));
            }
        }
    }
    for (name, actions) in [
        ("SET_before_false_ANR", "LD X2\nSET S912\nLD X0\nANR\n"),
        ("SET_after_false_ANR", "LD X0\nANR\nLD X2\nSET S912\n"),
        ("RST_before_false_ANR", "LD X2\nRST S1023\nLD X0\nANR\n"),
        ("RST_after_false_ANR", "LD X0\nANR\nLD X2\nRST S1023\n"),
    ] {
        out.push(fixture(
            name,
            &format!("{}\nSET S1023", init()),
            &format!("{actions}{}", monitor()),
            stages(),
        ));
    }
    for before in [false, true] {
        let reset = "LD X2\nANR\n";
        let mut scan = stages();
        scan[0].0.extend([("X0", true), ("X2", true)]);
        out.push(fixture(
            &format!("expired_ANS_ANR_before{before}_reassertion_order"),
            &format!("{}\nSET S999\nDIV16 K5 K1 T0", init()),
            &format!(
                "{}LD X0\nANS T0 K5 S912\n{}{}",
                if before { reset } else { "" },
                if before { "" } else { reset },
                monitor()
            ),
            scan,
        ));
    }
    for op in ["ANR", "ANRP"] {
        for count in [1, 3] {
            out.push(fixture(
                &format!("{op}_same_PC_FOR{count}_visited_monitor_vs_pulse_skip"),
                &format!("{}\nSET S920\nSET S1023", init()),
                &format!(
                    "FOR K{count}\nLD X2\n{op}\nLD X4\nSET S912\nNEXT\n{}",
                    monitor()
                ),
                stages(),
            ));
        }
        let source = format!(
            "LD X1\n{}\nSET S920\nSET S1023\nLD X4\nSET S912\nLD X0\nCALL P10\nLD M1048\nOUT Y30\nLD X6\nMOV D1049 D700\nINC D900\nFEND\nP10\nLD X2\n{op}\nSRET\nEND",
            init()
        );
        out.push(case(
            &format!("alarm_{op}_CALL_body_held_false_and_reentry"),
            &source,
            stages(),
        ));
    }

    for timer in [0, 64, 127] {
        for (name, first, second) in [
            (
                "TMR_then_false_ANS",
                format!("TMR T{timer} K3"),
                format!("ANS T{timer} K5 S912"),
            ),
            (
                "ANS_then_false_TMR",
                format!("ANS T{timer} K3 S912"),
                format!("TMR T{timer} K5"),
            ),
            (
                "ANS_then_false_ANS",
                format!("ANS T{timer} K3 S912"),
                format!("ANS T{timer} K5 S1023"),
            ),
            (
                "ANS_small_then_large",
                format!("ANS T{timer} K1 S912"),
                format!("ANS T{timer} K5 S1023"),
            ),
            (
                "ANS_large_then_small",
                format!("ANS T{timer} K5 S912"),
                format!("ANS T{timer} K1 S1023"),
            ),
        ] {
            out.push(fixture(
                &format!("shared_T{timer}_{name}_enable_and_contact"),
                &format!("{}\nRST T{timer}", init()),
                &format!(
                    "LD X0\n{first}\nLD X6\nMOV T{timer} D801\nLD X2\n{second}\n{}{}",
                    timer_read(timer),
                    monitor()
                ),
                stages(),
            ));
        }
        for before in [false, true] {
            let reset = format!("LD X2\nRST T{timer}\n");
            out.push(fixture(
                &format!("RST_T{timer}_before{before}_ANS_source_order"),
                &format!("{}\nRST T{timer}", init()),
                &format!(
                    "{}LD X0\nANS T{timer} K3 S912\n{}{}{}",
                    if before { reset.as_str() } else { "" },
                    if before { "" } else { reset.as_str() },
                    timer_read(timer),
                    monitor()
                ),
                stages(),
            ));
        }
    }
    for timer in [0, 7] {
        out.push(fixture(
            &format!("STMR_hold_T{timer}_false_ANS_preserves_inhibit"),
            &format!("{}\nRST T{timer}", init()),
            &format!(
                "LD X0\nSTMR T{timer} K1 M200\nLD X2\nANS T{timer} K5 S912\n{}{}",
                timer_read(timer),
                monitor()
            ),
            stages(),
        ));
    }
    for op in ["ANR", "ANRP"] {
        out.push(fixture(
            &format!("CJ_skips_{op}_and_ANS_without_false_visit"),
            &format!("{}\nSET S1023\nRST T0", init()),
            &format!(
                "LD X5\nRST S1023\nLD X4\nCJ P100\nLD X0\nANS T0 K3 S912\nLD X2\n{op}\nP100\n{}{}",
                timer_read(0),
                monitor()
            ),
            vec![
                (
                    vec![
                        ("X1", true),
                        ("X0", false),
                        ("X2", false),
                        ("X4", false),
                        ("X5", false),
                        ("X6", true),
                    ],
                    1,
                ),
                (vec![("X1", false), ("X0", true)], 3),
                (vec![("X4", true), ("X5", true)], 3),
                (vec![("X4", false), ("X5", false), ("X0", false)], 2),
                (vec![("X2", true)], 3),
            ],
        ));
        out.push(fixture(
            &format!("MC_false_{op}_still_visits_alarm_monitor"),
            &format!("{}\nSET S999\nRST T0", init()),
            &format!(
                "LD X4\nMC N0\nLD X0\nANS T0 K3 S912\nLD X2\n{op}\nMCR N0\n{}{}",
                timer_read(0),
                monitor()
            ),
            stages(),
        ));
        out.push(fixture(
            &format!("inactive_STL_outmask_does_not_skip_false_{op}_monitor"),
            &format!("{}\nRST S20\nSET S999\nRST T0", init()),
            &format!(
                "STL S20\nLD X0\nANS T0 K3 S912\nLD X2\n{op}\nRET\n{}{}",
                timer_read(0),
                monitor()
            ),
            stages(),
        ));
    }
    for enabled in [false, true] {
        for op in ["ANR", "ANRP"] {
            for before in [false, true] {
                let mut seed = format!(
                    "{}\n{} M1049\n",
                    init(),
                    if enabled { "SET" } else { "RST" }
                );
                let mut headers = String::new();
                for n in 0..9 {
                    seed.push_str(&format!("SET S{n}\n"));
                    headers.push_str(&format!("STL S{n}\n"));
                }
                let reset = format!("LD X0\n{op}\n");
                out.push(fixture(
                    &format!("SFC_Installed8_overflow_{op}_before{before}_monitor{enabled}"),
                    &seed,
                    &format!(
                        "{}{headers}LD X2\nOUT Y0\nRET\n{}{}",
                        if before { reset.as_str() } else { "" },
                        if before { "" } else { reset.as_str() },
                        monitor()
                    ),
                    stages(),
                ));
            }
        }
    }

    let table = words(400, &[30, 10, 20, 3, 1, 2]) + &words(500, &[-777; 6]);
    for (name, timer, preseed, predecessor) in [
        ("ANS_below_T0", 0, 0, "ANS T0 K32767 S912"),
        ("ANS_below_T3", 3, 0, "ANS T3 K32767 S912"),
        ("ANS_reached_S1023", 0, 1, "ANS T0 K1 S1023"),
        ("ANR_holds_numeric_scratch", 0, 0, "ANR"),
    ] {
        out.push(fixture(&format!("scratch_{name}_false_active_SORT_continuation"),
            &format!("{}\n{table}RST M1029\nDIV16 K{preseed} K1 T{timer}",init()),
            &format!("LD X2\n{predecessor}\nLD X0\nSORT D400 K3 K2 D500 K2\nLD X6\nMOV T{timer} D800\n{}",monitor()),
            vec![(vec![("X1",true),("X0",false),("X2",false),("X6",true)],1),
                (vec![("X1",false),("X0",true),("X2",true)],5),(vec![("X2",false)],3),
                (vec![("X0",false)],1),(vec![("X0",true),("X2",true)],4)]));
    }
    out.push(case(
        "alarm_example",
        include_str!("../../../examples/alarms.il"),
        vec![
            (
                vec![("X0", false), ("X1", false), ("X2", false), ("X3", true)],
                1,
            ),
            (vec![("X0", true), ("X1", true)], 7),
            (vec![("X0", false), ("X1", false)], 1),
            (vec![("X2", true)], 3),
            (vec![("X2", false)], 2),
            (vec![("X2", true)], 1),
        ],
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_alarm_sources_compile_encode_and_finish_bounded_scans() {
        use opendvp::{
            es2_encoder,
            plc::{self, Simulator},
            sfc::SfcProfile,
        };
        use std::collections::BTreeSet;
        for target in [Target::Es2, Target::Ss2] {
            let mut names = BTreeSet::new();
            for case in cases(target) {
                assert!(names.insert(case.name.clone()), "duplicate{}", case.name);
                let program = plc::compile_il(&case.source)
                    .unwrap_or_else(|e| panic!("{}: {e}\n{}", case.name, case.source));
                es2_encoder::encode(&program).unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let mut sim = Simulator::default();
                sim.memory.sfc.profile = SfcProfile::InstalledLegacy;
                let mut scans = 0;
                for (inputs, count) in case.stages {
                    for (input, value) in inputs {
                        sim.memory.set_input(input.parse().unwrap(), value);
                    }
                    for _ in 0..count {
                        sim.scan(&program, 100);
                        scans += 1;
                        assert!(
                            sim.memory.execution_limitations.is_empty(),
                            "{}: {:?}",
                            case.name,
                            sim.memory.execution_limitations
                        );
                    }
                }
                assert_eq!(sim.scans, scans, "{} incomplete/faultedscan", case.name);
            }
        }
    }
}
