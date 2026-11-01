//! Bounded valid control-flow sources. No unmatched NEXT, raw IRET or endless CJ.
//! Indexed labels and RET are separately named installed-profile candidates.
use super::{Case, case};
use opendvp::es2_bridge::Target;

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (
            vec![("X0", false), ("X1", true), ("X2", false), ("X3", true)],
            1,
        ),
        (vec![("X0", true), ("X1", false)], 2),
        (vec![("X2", true)], 1),
        (vec![("X0", false), ("X2", false)], 1),
        (vec![("X0", true)], 2),
    ]
}
fn fixture(name: &str, source: &str) -> Case {
    case(&format!("program_control_{name}"), source, stages())
}
fn prelude(init: &str, update: &str, body: &str) -> String {
    let mut source = String::new();
    if !init.is_empty() {
        source.push_str(&format!("LD X1\n{init}"));
    }
    if !update.is_empty() {
        source.push_str(&format!("LD X2\n{update}"));
    }
    source.push_str(body);
    source
}
fn index_alias(name: &str) -> usize {
    let n = name[1..].parse::<usize>().unwrap();
    (if n == 0 { 1028 } else { 1180 + 2 * n }) + usize::from(name.starts_with('F'))
}
fn packed_seed(name: &str, value: u16) -> String {
    let width = name[1..2].parse::<usize>().unwrap() * 4;
    let area = name.as_bytes()[2] as char;
    let base = if area == 'Y' {
        usize::from_str_radix(&name[3..], 8).unwrap()
    } else {
        name[3..].parse().unwrap()
    };
    (0..width)
        .map(|bit| {
            let index = base + bit;
            let address = if area == 'Y' {
                format!("Y{index:o}")
            } else {
                format!("{area}{index}")
            };
            format!(
                "{} {address}\n",
                if value & (1 << bit) == 0 {
                    "RST"
                } else {
                    "SET"
                }
            )
        })
        .collect()
}

pub fn cases(_target: Target) -> Vec<Case> {
    let mut out = Vec::new();
    for op in ["CJ", "CJP"] {
        for label in [0, 10, 255] {
            out.push(fixture(&format!("{op}_forward_P{label}"), &format!(
                "LD X0\n{op} P{label}\nMOV K111 D0\nLD X3\nMOV K222 D1\nP{label}\nLD X3\nINC D2\nEND")));
        }
        let source = format!(
            "LD X1\nSET Y0\nSET M10\nSET M11\nLD X0\n{op} P10\nLD X2\nOUT Y0\nSET M10\nRST M11\nP10\nLD X3\nINC D100\nEND"
        );
        out.push(case(
            &format!("program_control_{op}_skipped_hold_vs_false_visit"),
            &source,
            vec![
                (
                    vec![("X1", true), ("X0", true), ("X2", false), ("X3", true)],
                    1,
                ),
                (vec![("X1", false)], 2),
                (vec![("X0", false)], 1),
                (vec![("X2", true)], 1),
                (vec![("X0", true), ("X2", false)], 2),
                (vec![("X0", false)], 1),
            ],
        ));
        out.push(fixture(
            &format!("{op}_multiple_sites_one_label"),
            &format!("LD X0\n{op} P20\nLD X2\n{op} P20\nLD X3\nINC D0\nP20\nLD X3\nINC D1\nEND"),
        ));
        out.push(fixture(
            &format!("{op}_unused_and_nonsequential_labels"),
            &format!("LD X0\n{op} P255\nP7\nLD X3\nINC D7\nP255\nLD X3\nINC D255\nEND"),
        ));
    }
    out.push(case("program_control_CJ_skipped_timer_counter_drum", &prelude(
        "MOV K2 D100\nMOV K2 D101\nMOV K0 D500\n", "",
        "LD X0\nCJ P20\nLD X2\nTMR T0 K3\nCNT C0 K5\nINC D500\nBMOV D500 C10 K1\nINCD D100 C10 M200 K2\nP20\nLD X3\nINC D900\nEND"), vec![
        (vec![("X1", true), ("X0", false), ("X2", true), ("X3", true)], 1),
        (vec![("X1", false)], 2),
        (vec![("X0", true)], 3),
        (vec![("X0", false), ("X2", false)], 1),
        (vec![("X2", true)], 2),
    ]));
    for op in ["CALL", "CALLP"] {
        for label in [0, 10, 255] {
            out.push(fixture(&format!("{op}_mid_network_return_P{label}"), &format!(
                "LD X0\nMOV K11 D0\n{op} P{label}\nMOV K22 D1\nINC D2\nLD X3\nINC D3\nFEND\nP{label}\nLD X3\nINC D100\nMOV K33 D4\nSRET\nEND")));
        }
        out.push(fixture(
            &format!("{op}_same_subroutine_twice"),
            &format!(
                "LD X0\n{op} P10\nINC D0\n{op} P10\nINC D1\nFEND\nP10\nLD X3\nINC D100\nSRET\nEND"
            ),
        ));
        for depth in [5usize, 6] {
            let mut source = format!("LD X0\n{op} P10\nMOV K123 D900\nFEND\n");
            for i in 0..depth {
                source.push_str(&format!("P{}\nLD X3\nINC D{}\n", 10 + i, 100 + i));
                if i + 1 < depth {
                    source.push_str(&format!("CALL P{}\n", 11 + i));
                }
                source.push_str(&format!("INC D{}\nSRET\n", 200 + i));
            }
            source.push_str("END");
            out.push(fixture(
                &format!("{op}_nested_{depth}_sixth_falls_through"),
                &source,
            ));
        }
    }
    out.push(fixture(
        "CALL_repeated_within_finite_loop",
        "FOR K3\nLD X0\nCALL P10\nINC D0\nNEXT\nFEND\nP10\nLD X3\nINC D100\nSRET\nEND",
    ));
    out.push(fixture(
        "CALLP_revisited_within_finite_loop",
        "FOR K3\nLD X0\nCALLP P10\nINC D0\nNEXT\nFEND\nP10\nLD X3\nINC D100\nSRET\nEND",
    ));
    out.push(fixture("multiple_FEND_main_paths", "LD X0\nCJ P20\nLD X3\nMOV K11 D0\nFEND\nP20\nLD X3\nMOV K22 D0\nCALL P100\nMOV K123 D900\nFEND\nP100\nLD X3\nINC D100\nSRET\nEND"));
    for count in ["K-32768", "K-1", "K0", "K1", "K3", "HFFFF", "H0003"] {
        out.push(fixture(
            &format!("FOR_scalar_{count}"),
            &format!("FOR {count}\nLD X0\nINC D0\nLD X3\nINC D1\nNEXT\nLD X3\nINC D900\nEND"),
        ));
    }
    for source in ["D20", "T20", "C20", "E0", "F0", "E1", "F1", "E7", "F7"] {
        let init = if source.starts_with('E') || source.starts_with('F') {
            format!("MOV K3 D{}\n", index_alias(source))
        } else if source.starts_with('D') {
            "MOV K3 D20\n".to_owned()
        } else {
            format!("MOV K3 D600\nBMOV D600 {source} K1\n")
        };
        out.push(fixture(
            &format!("FOR_count_area_{source}"),
            &prelude(
                &init,
                "",
                &format!("FOR {source}\nLD X3\nINC D0\nNEXT\nLD X3\nINC D900\nEND"),
            ),
        ));
    }
    for source in ["K1M64", "K2Y100", "K3S64", "K4M128"] {
        out.push(fixture(
            &format!("FOR_packed_count_{source}"),
            &prelude(
                &packed_seed(source, 3),
                "",
                &format!("FOR {source}\nLD X3\nINC D0\nNEXT\nEND"),
            ),
        ));
    }
    let mut packed_x = fixture("FOR_packed_count_X", "FOR K1X100\nLD X3\nINC D0\nNEXT\nEND");
    packed_x.stages[0].0.extend([
        ("X100", true),
        ("X101", true),
        ("X102", false),
        ("X103", false),
    ]);
    out.push(packed_x);
    out.push(fixture(
        "FOR_dynamic_count_captured_before_body_write",
        &prelude(
            "MOV K3 D0\n",
            "MOV K4 D0\n",
            "FOR D0\nLD X3\nMOV K1 D0\nINC D10\nNEXT\nLD X3\nINC D900\nEND",
        ),
    ));
    for depth in 1..=5 {
        let mut source = "FOR K2\n".repeat(depth);
        source.push_str("LD X3\nINC D0\n");
        source.push_str(&"NEXT\n".repeat(depth));
        source.push_str("LD X3\nINC D900\nEND");
        out.push(fixture(&format!("FOR_nested_{depth}_bounded"), &source));
    }
    out.push(fixture(
        "FOR_empty_body",
        "FOR K3\nNEXT\nLD X3\nINC D900\nEND",
    ));
    out.push(fixture(
        "CJ_bypasses_whole_FOR",
        "LD X0\nCJ P10\nFOR K3\nLD X3\nINC D0\nNEXT\nP10\nLD X3\nINC D900\nEND",
    ));
    out.push(fixture("CJ_bypasses_inner_FOR_preserves_outer", "FOR K2\nLD X0\nCJ P10\nFOR K3\nLD X3\nINC D0\nNEXT\nP10\nLD X3\nINC D1\nNEXT\nLD X3\nINC D900\nEND"));
    out.push(fixture(
        "FOR_same_counter_action_edges",
        "FOR K3\nLD X0\nCNT C0 K5\nNEXT\nLD X3\nINC D900\nEND",
    ));
    out.push(fixture("MC_false_OUT_SET_RST_timer_counter", &prelude("SET M10\nSET M11\nSET Y0\n", "", "LD X0\nMC N0\nLD X3\nOUT Y0\nSET M10\nRST M11\nTMR T0 K3\nCNT C0 K5\nMCR N0\nLD X3\nOUT Y1\nINC D900\nEND")));
    out.push(fixture(
        "MC_inactive_body_still_runs_FOR_control",
        "LD X0\nMC N0\nFOR K3\nLD X3\nINC D0\nNEXT\nMCR N0\nLD X3\nINC D900\nEND",
    ));
    out.push(fixture("MC_sequential_reuse_N0", "LD X0\nMC N0\nLD X3\nINC D0\nMCR N0\nLD X2\nMC N0\nLD X3\nINC D1\nMCR N0\nLD X3\nINC D900\nEND"));
    for depth in [2usize, 8] {
        let mut source = String::from("LD X0\nMC N0\n");
        for n in 1..depth {
            source.push_str(&format!("LD X3\nMC N{n}\n"));
        }
        source.push_str("LD X3\nOUT Y0\nINC D0\n");
        for n in (0..depth).rev() {
            source.push_str(&format!("MCR N{n}\n"));
        }
        source.push_str("LD X3\nOUT Y1\nINC D900\nEND");
        out.push(fixture(&format!("MC_nested_{depth}_balanced"), &source));
    }
    out.push(fixture(
        "CJ_bypasses_balanced_MC_region",
        "LD X0\nCJ P10\nLD X2\nMC N0\nLD X3\nOUT Y0\nINC D0\nMCR N0\nP10\nLD X3\nINC D900\nEND",
    ));
    for (name, body) in [
        (
            "RET_installed_reset_after_balanced_MPS",
            "LD X0\nMPS\nOUT Y0\nMRD\nSET M10\nMPP\nOUT Y1\nRET\nLD X3\nOUT Y2\nINC D900\nEND",
        ),
        (
            "RET_installed_reset_repeated",
            "LD X0\nMPS\nOUT Y0\nMPP\nOUT Y1\nRET\nRET\nLD X3\nINC D900\nEND",
        ),
    ] {
        out.push(fixture(name, body));
    }
    for index in ["E0", "F1", "E7"] {
        let alias = index_alias(index);
        let init = format!("MOV K0 D{alias}\n");
        let update = format!("MOV K1 D{alias}\n");
        for op in ["CJ", "CJP"] {
            let body = format!(
                "LD X0\n{op} P10{index}\nLD X3\nMOV K-1 D0\nCJ P20\nP10\nLD X3\nMOV K10 D0\nCJ P20\nP11\nLD X3\nMOV K11 D0\nP20\nLD X3\nINC D900\nEND"
            );
            out.push(fixture(
                &format!("indexed_{op}_{index}_bounded_targets"),
                &prelude(&init, &update, &body),
            ));
        }
        for op in ["CALL", "CALLP"] {
            let body = format!(
                "LD X0\n{op} P10{index}\nINC D0\nFEND\nP10\nLD X3\nINC D10\nSRET\nP11\nLD X3\nINC D11\nSRET\nEND"
            );
            out.push(fixture(
                &format!("indexed_{op}_{index}_bounded_targets"),
                &prelude(&init, &update, &body),
            ));
        }
    }
    out
}
