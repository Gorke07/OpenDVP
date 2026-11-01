//! SFC sources for genuine compact STL/RET and IST encodings.
//! No numeric-result expectations are manufactured here: installed snapshot,
//! monitor gaps and mode-policy differences stay visible to the comparator.
use super::{Case, case};
use opendvp::es2_bridge::Target;

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (
            vec![
                ("X0", false),
                ("X1", true),
                ("X2", false),
                ("X3", true),
                ("X4", false),
                ("X5", false),
                ("X6", false),
                ("X7", false),
            ],
            1,
        ),
        (vec![("X1", false)], 1),
        (vec![("X0", true)], 1),
        (vec![], 2),
        (vec![("X0", false), ("X2", true)], 1),
        (vec![], 2),
        (vec![("X2", false), ("X4", true)], 1),
        (vec![("X4", false)], 1),
        (vec![("X0", true)], 2),
    ]
}
fn fixture(name: &str, init: &str, body: &str) -> Case {
    let seed = if init.is_empty() {
        String::new()
    } else {
        format!("LD X1\n{init}\n")
    };
    case(&format!("sfc_{name}"), &format!("{seed}{body}"), stages())
}
fn clear() -> &'static str {
    "ZRST S0 S1023\nRST M1040\nRST M1045\nRST M1047"
}
fn monitor() -> &'static str {
    "LD M1046\nOUT Y30\nLD M1047\nOUT Y31\nLD X3\nMOV D1040 D700\nMOV D1041 D701\nMOV D1042 D702\nMOV D1043 D703\nMOV D1044 D704\nMOV D1045 D705\nMOV D1046 D706\nMOV D1047 D707\nINC D900\nEND"
}
fn y(n: usize) -> String {
    format!("Y{n:o}")
}
fn static_name(raw: String) -> &'static str {
    // Generated case input labels live for this comparator process, matching
    // the other fixture modules' fixed-lifetime stage input representation.
    Box::leak(raw.into_boxed_str())
}
fn source_bit(source: &str, n: usize) -> String {
    let area = source.as_bytes()[0] as char;
    let base = if matches!(area, 'X' | 'Y') {
        usize::from_str_radix(&source[1..], 8).unwrap()
    } else {
        source[1..].parse().unwrap()
    };
    if matches!(area, 'X' | 'Y') {
        format!("{area}{:o}", base + n)
    } else {
        format!("{area}{}", base + n)
    }
}
fn controls(
    source: &str,
    mode: Option<usize>,
    home: bool,
    start: bool,
    stop: bool,
) -> Vec<(&'static str, bool)> {
    (0..8)
        .map(|n| {
            let raw = if source.starts_with('X') {
                source_bit(source, n)
            } else {
                format!("X{:o}", 8 + n)
            };
            let value = match n {
                0..=4 => mode == Some(n),
                5 => home,
                6 => start,
                _ => stop,
            };
            (static_name(raw), value)
        })
        .collect()
}
fn drivers(source: &str) -> String {
    if source.starts_with('X') {
        return String::new();
    }
    (0..8)
        .map(|n| format!("LD X{:o}\nOUT {}\n", 8 + n, source_bit(source, n)))
        .collect()
}
fn ist_source(source: &str, first: u16, last: u16, skip: bool) -> String {
    let jump = if skip { "LD X7\nCJ P200\n" } else { "" };
    let label = if skip { "P200\n" } else { "" };
    format!(
        "{}LD X4\nOUT M1044\nLD X6\nOUT M1045\n{jump}LD X3\nIST {source} S{first} S{last}\n{label}STL S0\nLD X0\nOUT Y0\nRET\nSTL S1\nLD {}\nSET S10\nSTL S10\nOUT Y1\nLD X2\nSET S11\nSTL S11\nSET M1043\nRST S11\nRET\nSTL S2\nLD M1041\nAND M1044\nSET S{first}\nSTL S{first}\nOUT Y2\nLD X0\nSET S{last}\nSTL S{last}\nOUT Y3\nLD X2\nOUT S2\nRET\n{}",
        drivers(source),
        source_bit(source, 5),
        monitor()
    )
}
fn ist_stages(source: &str, mode: usize) -> Vec<(Vec<(&'static str, bool)>, usize)> {
    let mut seed = vec![
        ("X1", true),
        ("X0", false),
        ("X2", false),
        ("X3", false),
        ("X4", true),
        ("X6", false),
        ("X7", false),
    ];
    seed.extend(controls(source, None, false, false, false));
    let mut select = vec![("X1", false), ("X3", true)];
    select.extend(controls(source, Some(mode), false, false, false));
    vec![
        (seed, 1),
        (select, 2),
        (controls(source, Some(mode), true, false, false), 1),
        (controls(source, Some(mode), false, false, false), 1),
        (controls(source, Some(mode), false, true, false), 3),
        (vec![("X0", true)], 2),
        (vec![("X0", false), ("X2", true)], 2),
        (controls(source, Some(mode), false, true, true), 1),
        (controls(source, Some(mode), false, false, false), 1),
        (vec![("X2", false)], 1),
        (controls(source, Some(mode), false, true, false), 1),
        (controls(source, Some(mode), false, false, false), 2),
    ]
}

pub fn cases(_target: Target) -> Vec<Case> {
    let mut out = Vec::new();
    for initial in 0..10 {
        out.push(fixture(&format!("initial_S{initial}_snapshot_delay"),&format!("{}\nSET S{initial}",clear()),&format!(
            "STL S{initial}\nOUT Y0\nLD X0\nSET S100\nSTL S100\nOUT Y1\nLD X2\nOUT S{initial}\nRET\n{}",monitor())));
    }
    for step in [
        10, 19, 20, 127, 128, 255, 511, 512, 767, 768, 895, 896, 911, 912, 1023,
    ] {
        out.push(fixture(
            &format!("step_bank_S{step}_snapshot_and_monitor"),
            &format!("{}\nSET S0\nSET M1047", clear()),
            &format!(
                "STL S0\nLD X0\nSET S{step}\nSTL S{step}\nOUT Y0\nSET Y1\nLD X2\nOUT S0\nRET\n{}",
                monitor()
            ),
        ));
    }
    for count in [1, 2, 10] {
        let mut init = clear().to_owned();
        let mut body = String::new();
        for n in 0..count {
            init.push_str(&format!("\nSET S{n}"));
            body.push_str(&format!(
                "STL S{n}\nLD X0\nSET S{}\nSTL S{}\nOUT {}\nLD X2\nOUT S{n}\nRET\n",
                20 + n,
                20 + n,
                y(n)
            ));
        }
        body.push_str(monitor());
        out.push(fixture(
            &format!("multiple_initial_sequences_{count}"),
            &init,
            &body,
        ));
    }
    let prefix = "STL S0\nSET S20\nSTL S20\nOUT Y0\n";
    out.push(fixture("alternative_divergence_and_convergence",&format!("{}\nSET S0",clear()),&format!(
        "{prefix}LD X0\nSET S30\nLD X2\nSET S31\nLD X4\nSET S32\nSTL S30\nOUT Y1\nLD X2\nSET S50\nSTL S31\nOUT Y2\nLD X4\nSET S50\nSTL S32\nOUT Y3\nLD X0\nSET S50\nSTL S50\nOUT Y4\nLD X4\nOUT S0\nRET\n{}",monitor())));
    let mut simultaneous_true = fixture(
        "alternative_multiple_true_installed_priority",
        &format!("{}\nSET S0", clear()),
        &format!(
            "{prefix}LD X0\nSET S30\nLD X2\nSET S31\nLD X4\nSET S32\nSTL S30\nOUT Y1\nSTL S31\nOUT Y2\nSTL S32\nOUT Y3\nRET\n{}",
            monitor()
        ),
    );
    simultaneous_true.stages[2]
        .0
        .extend([("X2", true), ("X4", true)]);
    out.push(simultaneous_true);
    out.push(fixture("simultaneous_fork_ANDjoin",&format!("{}\nSET S0",clear()),&format!(
        "{prefix}LD X0\nSET S30\nSET S31\nSET S32\nSTL S30\nOUT Y1\nLD X2\nSET S40\nSTL S40\nOUT Y4\nSTL S31\nOUT Y2\nLD X2\nSET S41\nSTL S41\nOUT Y5\nSTL S32\nOUT Y3\nLD X2\nSET S42\nSTL S42\nOUT Y6\nSTL S40\nSTL S41\nSTL S42\nLD X4\nSET S50\nSTL S50\nOUT Y7\nLD X0\nOUT S0\nRET\n{}",monitor())));
    out.push(fixture("simultaneous_fork_alternative_merge",&format!("{}\nSET S0",clear()),&format!(
        "{prefix}LD X0\nSET S30\nSET S31\nSTL S30\nOUT Y1\nLD X2\nSET S50\nSTL S31\nOUT Y2\nLD X4\nSET S50\nSTL S50\nOUT Y3\nRET\n{}",monitor())));
    out.push(fixture("cross_sequence_OUT_target",&format!("{}\nSET S0\nSET S1",clear()),&format!(
        "STL S0\nOUT Y0\nLD X0\nOUT S42\nRET\nSTL S1\nOUT Y1\nLD X2\nSET S42\nSTL S42\nOUT Y2\nLD X4\nOUT S1\nRET\n{}",monitor())));
    let mut eight = String::from("STL S0\nLD X0\n");
    for n in 40..48 {
        eight.push_str(&format!("SET S{n}\n"));
    }
    for n in 40..48 {
        eight.push_str(&format!("STL S{n}\nOUT {}\n", y(n - 40)));
    }
    for n in 40..48 {
        eight.push_str(&format!("STL S{n}\n"));
    }
    eight.push_str(&format!(
        "LD X2\nSET S80\nSTL S80\nOUT Y20\nLD X4\nOUT S0\nRET\n{}",
        monitor()
    ));
    out.push(fixture(
        "eight_way_fork_ANDjoin",
        &format!("{}\nSET S0\nSET M1047", clear()),
        &eight,
    ));

    for action in ["SET S30", "OUT S30", "RST S20"] {
        let mut p = fixture(
            &format!("M1040_inhibit_{}", action.replace(' ', "_")),
            &format!("{}\nSET S0", clear()),
            &format!(
                "LD X5\nOUT M1040\nSTL S0\nSET S20\nSTL S20\nOUT Y0\nLD X0\n{action}\nSTL S30\nOUT Y1\nRET\n{}",
                monitor()
            ),
        );
        p.stages = vec![
            (
                vec![("X1", true), ("X0", false), ("X3", true), ("X5", false)],
                1,
            ),
            (vec![("X1", false)], 1),
            (vec![("X0", true), ("X5", true)], 2),
            (vec![("X5", false)], 2),
            (vec![("X0", false)], 1),
            (vec![("X0", true)], 2),
        ];
        out.push(p);
    }
    out.push(fixture(
        "M1034_output_inhibit_separate_from_step",
        &format!("{}\nSET S0", clear()),
        &format!(
            "LD X2\nOUT M1034\nSTL S0\nOUT Y0\nLD X0\nSET S20\nSTL S20\nOUT Y1\nRET\n{}",
            monitor()
        ),
    ));
    for (name, outputs) in [
        ("repeated_OUT_Y_ownership", "OUT Y0\nOUT Y1"),
        ("latched_SET_Y_retains", "SET Y0\nOUT Y1"),
        ("multiple_coils_before_transition", "OUT Y0\nOUT Y1\nOUT Y2"),
    ] {
        out.push(fixture(name,&format!("{}\nSET S0",clear()),&format!(
            "STL S0\n{outputs}\nLD X0\nSET S20\nSTL S20\nOUT Y0\nOUT Y3\nLD X2\nSET S30\nSTL S30\nOUT Y4\nLD X4\nOUT S0\nRET\n{}",monitor())));
    }
    for timer in [0, 127, 184, 240, 255] {
        out.push(fixture(&format!("nonadjacent_repeated_TMR_T{timer}"),&format!("{}\nSET S0",clear()),&format!(
            "STL S0\nTMR T{timer} K3\nLD X0\nSET S20\nSTL S20\nOUT Y0\nLD X2\nSET S30\nSTL S30\nTMR T{timer} K1\nLD X4\nOUT S0\nRET\nLD X3\nMOV T{timer} D800\n{}",monitor())));
    }
    out.push(fixture("direct_bus_then_LD_MPS",&format!("{}\nSET S0",clear()),&format!(
        "STL S0\nOUT Y0\nOUT Y1\nLD X0\nMPS\nAND X2\nOUT Y2\nMRD\nAND X4\nOUT Y3\nMPP\nSET S20\nSTL S20\nOUT Y4\nRET\n{}",monitor())));
    out.push(fixture(
        "transition_before_later_current_outputs",
        &format!("{}\nSET S0", clear()),
        &format!(
            "STL S0\nOUT Y0\nSET S20\nOUT Y1\nSTL S20\nOUT Y2\nRET\n{}",
            monitor()
        ),
    ));
    out.push(fixture(
        "RET_restores_ordinary_LD_context",
        &format!("{}\nSET S0", clear()),
        &format!(
            "STL S0\nLD X0\nOUT Y0\nRET\nLD X2\nOUT Y1\nINC D20\n{}",
            monitor()
        ),
    ));
    out.push(fixture(
        "self_RST_step_ends_sequence",
        &format!("{}\nSET S0", clear()),
        &format!(
            "STL S0\nLD X0\nSET S20\nSTL S20\nOUT Y0\nLD X2\nRST S20\nRET\n{}",
            monitor()
        ),
    ));
    for enabled in [false, true] {
        let mut init = format!(
            "{}\n{} M1047\n",
            clear(),
            if enabled { "SET" } else { "RST" }
        );
        for n in 0..8 {
            init.push_str(&format!("MOV K-777 D{}\n", 1040 + n));
        }
        for step in [0, 7, 19, 20, 127, 128, 511, 895, 896, 1023] {
            init.push_str(&format!("SET S{step}\n"));
        }
        out.push(fixture(
            &format!("monitor_first_eight_ascending_enabled{enabled}"),
            &init,
            &format!(
                "LD X2\nRST S7\nRST S19\nRST S20\nRST S127\nRST S128\nRST S511\nRST S895\n{}",
                monitor()
            ),
        ));
    }
    for step in [895, 896, 1023] {
        out.push(fixture(
            &format!("monitor_high_bank_gap_S{step}"),
            &format!("{}\nSET M1047\nSET S{step}", clear()),
            monitor(),
        ));
    }

    for source in ["X20", "Y20", "M300"] {
        for mode in 0..5 {
            let mut p = fixture(
                &format!("IST_{source}_mode{mode}_first_false_start_hold_stop"),
                clear(),
                &ist_source(source, 20, 80, false),
            );
            p.stages = ist_stages(source, mode);
            out.push(p);
        }
    }
    for source in ["X20", "Y20", "M300"] {
        for retain in [false, true] {
            let mut p = fixture(
                &format!("IST_{source}_mode_switch_retainY{retain}"),
                clear(),
                &ist_source(source, 20, 80, false),
            );
            let mut seed = vec![
                ("X1", true),
                ("X0", false),
                ("X2", false),
                ("X3", false),
                ("X4", true),
                ("X6", retain),
            ];
            seed.extend(controls(source, None, false, false, false));
            p.stages = vec![(seed, 1)];
            for mode in [0, 1, 2, 3, 4, 0, 1, 0] {
                let mut select = vec![("X1", false), ("X3", true)];
                select.extend(controls(source, Some(mode), true, true, false));
                p.stages.push((select, 2));
                p.stages
                    .push((controls(source, Some(mode), false, false, false), 1));
            }
            out.push(p);
        }
    }
    for source in ["Y0", "M1532"] {
        let mut p = fixture(
            &format!("IST_{source}_control_clear_alias"),
            clear(),
            &ist_source(source, 20, 80, false),
        );
        p.stages = ist_stages(source, 4);
        out.push(p);
    }
    for (first, last) in [(20, 21), (20, 911), (910, 911)] {
        let mut p = fixture(
            &format!("IST_auto_range_S{first}_S{last}"),
            clear(),
            &ist_source("X20", first, last, false),
        );
        p.stages = ist_stages("X20", 3);
        out.push(p);
    }
    let mut skipped = fixture(
        "IST_CJ_skipped_vs_false",
        clear(),
        &ist_source("X20", 20, 80, true),
    );
    skipped.stages = ist_stages("X20", 4);
    skipped.stages.extend([
        (vec![("X7", true), ("X3", false)], 2),
        (vec![("X7", false)], 2),
        (vec![("X3", true)], 2),
    ]);
    out.push(skipped);
    let mut multiple = fixture(
        "IST_multiple_selectors_installed_priority",
        clear(),
        &ist_source("X20", 20, 80, false),
    );
    multiple.stages = ist_stages("X20", 3);
    multiple.stages[1]
        .0
        .extend([("X20", true), ("X21", true), ("X22", true), ("X24", true)]);
    out.push(multiple);
    for (name, selector, extra_seed) in [
        ("zero_busy_S10", 1, "SET S10"),
        ("zero_busy_S19", 1, "SET S19"),
        ("auto_busy_first", 4, "SET S20"),
        ("auto_busy_last", 4, "SET S80"),
        ("auto_M1043_OFF", 4, "RST M1043"),
        ("auto_M1043_ON", 4, "SET M1043"),
    ] {
        let mut p = fixture(
            &format!("IST_guard_{name}"),
            &format!("{}\n{extra_seed}", clear()),
            &ist_source("X20", 20, 80, false),
        );
        p.stages = ist_stages("X20", selector);
        out.push(p);
    }
    for mode in [2, 3, 4] {
        let mut p = fixture(
            &format!("IST_mode{mode}_zero_condition_false"),
            clear(),
            &ist_source("X20", 20, 80, false),
        );
        p.stages = ist_stages("X20", mode);
        p.stages[0].0.push(("X4", false));
        out.push(p);
    }
    let body = format!("LD X2\nRST M1047\n{}", ist_source("X20", 20, 80, false));
    let mut p = fixture("IST_monitor_forced_each_active_visit", clear(), &body);
    p.stages = ist_stages("X20", 4);
    p.stages.extend([
        (vec![("X2", true), ("X3", true)], 2),
        (vec![("X3", false)], 2),
        (vec![("X3", true)], 2),
    ]);
    out.push(p);
    out.push(fixture(
        "nested_four_terminal_ANDjoin",
        &format!("{}\nSET S0", clear()),
        &format!("STL S0\nLD X0\nSET S20\nSET S21\nSTL S20\nLD X2\nSET S40\nSET S41\nSTL S21\nLD X2\nSET S42\nSET S43\nSTL S40\nOUT Y0\nSTL S41\nOUT Y1\nSTL S42\nOUT Y2\nSTL S43\nOUT Y3\nSTL S40\nSTL S41\nSTL S42\nSTL S43\nLD X4\nSET S80\nSTL S80\nOUT Y4\nRET\n{}", monitor()),
    ));
    out.push(fixture(
        "partial_branch_readiness_ANDjoin",
        &format!("{}\nSET S0", clear()),
        &format!("STL S0\nSET S20\nSTL S20\nLD X0\nSET S30\nSET S31\nSET S32\nSTL S30\nLD X0\nSET S40\nSTL S40\nOUT Y0\nSTL S31\nLD X2\nSET S41\nSTL S41\nOUT Y1\nSTL S32\nLD X4\nSET S42\nSTL S42\nOUT Y2\nSTL S40\nSTL S41\nSTL S42\nLD X3\nSET S80\nSTL S80\nOUT Y3\nRET\n{}", monitor()),
    ));
    out
}

/// Invalid-source requirements are deliberately NOT sent to the native process.
/// Exact diagnostic wording/code is not invented by these input strings.
#[allow(dead_code)] // Separate parser-negative corpus; never native-uploaded.
pub fn invalid_sources() -> Vec<(&'static str, &'static str)> {
    vec![
        ("STL_non_S_operand", "STL M0\nRET\nEND"),
        ("STL_out_of_range", "STL S1024\nRET\nEND"),
        ("STL_missing_RET", "STL S0\nOUT Y0\nEND"),
        (
            "STL_MC_forbidden",
            "STL S0\nLD X0\nMC N0\nOUT Y0\nMCR N0\nRET\nEND",
        ),
        (
            "STL_in_FOR_forbidden",
            "FOR K2\nSTL S0\nOUT Y0\nRET\nNEXT\nEND",
        ),
        (
            "STL_in_CALL_forbidden",
            "LD X0\nCALL P10\nFEND\nP10\nSTL S0\nOUT Y0\nRET\nSRET\nEND",
        ),
        (
            "STL_P_label_inside_forbidden",
            "STL S0\nP10\nOUT Y0\nRET\nEND",
        ),
        (
            "STL_MPS_without_first_LD",
            "STL S0\nMPS\nOUT Y0\nMPP\nOUT Y1\nRET\nEND",
        ),
        ("IST_source_S_forbidden", "LD X0\nIST S0 S20 S80\nEND"),
        (
            "IST_source_group_forbidden",
            "LD X0\nIST K2X20 S20 S80\nEND",
        ),
        ("IST_equal_range", "LD X0\nIST X20 S20 S20\nEND"),
        ("IST_reversed_range", "LD X0\nIST X20 S80 S20\nEND"),
        ("IST_alarm_range", "LD X0\nIST X20 S20 S912\nEND"),
        ("IST_numeric_range_forbidden", "LD X0\nIST X20 D20 D80\nEND"),
        (
            "IST_two_occurrences",
            "LD X0\nIST X20 S20 S80\nIST X30 S100 S200\nEND",
        ),
        ("ISTP_not_documented", "LD X0\nISTP X20 S20 S80\nEND"),
    ]
}

/// Valid PDF breadth retained outside automatic native execution until the
/// documented16-point graph and installed8-entry snapshot profile are reviewed.
#[allow(dead_code)]
pub fn deferred_sources() -> Vec<(&'static str, String, &'static str)> {
    let mut source = format!(
        "LD X1\n{}\nSET S0\nSTL S0\nLD X0\nSET S20\nSET S21\n",
        clear()
    );
    for (parent, base) in [(20, 40), (21, 50)] {
        source.push_str(&format!("STL S{parent}\nLD X2\n"));
        for step in base..base + 8 {
            source.push_str(&format!("SET S{step}\n"));
        }
    }
    for (base, output) in [(40, 0), (50, 8)] {
        for n in 0..8 {
            source.push_str(&format!("STL S{}\nOUT {}\n", base + n, y(output + n)));
        }
    }
    for base in [40, 50] {
        for n in 0..8 {
            source.push_str(&format!("STL S{}\n", base + n));
        }
    }
    source.push_str(&format!(
        "LD X4\nSET S80\nSTL S80\nOUT Y20\nRET\n{}",
        monitor()
    ));
    vec![(
        "sfc_pdf_combined_sixteen_terminal_join",
        source,
        "PDF16-point combined graph; installed snapshot only captures8. Native push is bounded, but structural quota/profile agreement needs review before inclusion in default corpus.",
    )]
}

#[cfg(test)]
mod tests {
    #[test]
    fn sfc_corpus_compiles_and_visits_full_typed_plans_without_faults() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let mut names = std::collections::BTreeSet::new();
            for case in super::cases(target)
                .into_iter()
                .chain(super::super::sfc_extra_cases::cases(target))
            {
                assert!(names.insert(case.name.clone()));
                let p = opendvp::plc::compile(&case.source)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let encoded = opendvp::es2_encoder::encode(&p)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                assert!(encoded.next_pc.len() <= 512, "{}", case.name);
                let mut sim = opendvp::plc::Simulator::default();
                sim.memory.sfc.profile = opendvp::sfc::SfcProfile::InstalledLegacy;
                for (inputs, scans) in case.stages {
                    for (name, value) in inputs {
                        sim.memory.set_input(name.parse().unwrap(), value);
                    }
                    for _ in 0..scans {
                        sim.scan(&p, 100);
                        assert!(
                            matches!(sim.last_scan, opendvp::plc::ScanStatus::Complete),
                            "{}: {:?}",
                            case.name,
                            sim.last_scan
                        );
                        assert!(
                            sim.memory.execution_limitations.is_empty(),
                            "{}: {:?}",
                            case.name,
                            sim.memory.execution_limitations
                        );
                    }
                }
            }
        }
        for (name, source) in super::invalid_sources() {
            assert!(opendvp::plc::compile(source).is_err(), "{name}");
        }
        for (name, source, _) in super::deferred_sources() {
            opendvp::plc::compile(&source).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }
}
