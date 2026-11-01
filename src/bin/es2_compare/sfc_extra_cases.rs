//! Additional SFC fixtures, separate from the unchanged original95 sources.
//! M1532 crosses into physical C contacts, not logical high-M registers.
//! Sixteen-terminal native cases select InstalledLegacy8 capacity; acceptance
//! of the documented graph does not assert Documented16 runtime equivalence.
use super::{Case, case};
use opendvp::es2_bridge::Target;

type Stages = Vec<(Vec<(&'static str, bool)>, usize)>;
const CONTROL_INPUTS: [&str; 8] = ["X10", "X11", "X12", "X13", "X14", "X15", "X16", "X17"];
fn controls(mode: Option<usize>, home: bool, start: bool, stop: bool) -> Vec<(&'static str, bool)> {
    CONTROL_INPUTS
        .into_iter()
        .enumerate()
        .map(|(n, input)| {
            (
                input,
                match n {
                    0..=4 => mode == Some(n),
                    5 => home,
                    6 => start,
                    _ => stop,
                },
            )
        })
        .collect()
}
fn mode_stages(mode: usize, completed: bool, zero: bool, retain: bool) -> Stages {
    let mut seed = vec![
        ("X1", true),
        ("X0", false),
        ("X2", false),
        ("X3", false),
        ("X4", zero),
        ("X5", completed),
        ("X6", retain),
        ("X7", false),
    ];
    seed.extend(controls(None, false, false, false));
    let mut select = vec![("X1", false), ("X3", true)];
    select.extend(controls(Some(mode), false, false, false));
    vec![
        (seed, 1),
        (select, 2),
        (controls(Some(mode), true, false, false), 2),
        (controls(Some(mode), false, false, false), 1),
        (controls(Some(mode), false, true, false), 3),
        (vec![("X0", true)], 2),
        (vec![("X0", false), ("X2", true)], 2),
        (controls(Some(mode), false, true, true), 1),
        (controls(Some(mode), false, false, true), 2),
        (controls(Some(mode), false, false, false), 1),
        (vec![("X2", false), ("X4", true)], 1),
        (controls(Some(mode), false, true, false), 2),
        (controls(Some(mode), false, false, false), 2),
        (vec![("X3", false)], 1),
        (vec![("X3", true)], 1),
    ]
}
fn ordinary_stages() -> Stages {
    vec![
        (
            vec![
                ("X1", true),
                ("X0", false),
                ("X2", false),
                ("X3", true),
                ("X4", false),
                ("X5", false),
                ("X7", false),
            ],
            1,
        ),
        (vec![("X1", false)], 2),
        (vec![("X0", true)], 3),
        (vec![("X0", false), ("X2", true)], 3),
        (
            vec![("X2", false), ("X4", true), ("X5", true), ("X7", true)],
            2,
        ),
        (vec![("X4", false), ("X5", false), ("X7", false)], 2),
        (vec![("X0", true)], 3),
    ]
}
fn clear() -> &'static str {
    "ZRST S0 S1023\nRST M1040\nRST M1045\nRST M1047\nRST M1048"
}
fn monitor() -> &'static str {
    "LD M1048\nOUT Y30\nLD M1046\nOUT Y31\nLD X3\nMOV D1040 D700\nMOV D1041 D701\nMOV D1042 D702\nMOV D1043 D703\nMOV D1044 D704\nMOV D1045 D705\nMOV D1046 D706\nMOV D1047 D707\nINC D900\nEND"
}
fn fixture(name: &str, seed: &str, body: &str, stages: Stages) -> Case {
    case(
        &format!("sfc_extra_{name}"),
        &format!("LD X1\n{seed}\n{body}"),
        stages,
    )
}
fn physical_m1532_drivers() -> String {
    let mut source = String::new();
    for (n, input) in CONTROL_INPUTS.iter().take(4).enumerate() {
        source.push_str(&format!("LD {input}\nOUT M{}\n", 1532 + n));
    }
    // CNT is the only setter of its C contact. RST on the opposite level and
    // exactly one CNT occurrence make each physical source bit a true level,
    // including held/rearmed states, without duplicate-CNT ownership ambiguity.
    for (n, input) in CONTROL_INPUTS.iter().skip(4).enumerate() {
        source.push_str(&format!("LDI {input}\nRST C{n}\nLD {input}\nCNT C{n} K1\n"));
    }
    source
}
fn system_flag_drivers() -> String {
    CONTROL_INPUTS
        .iter()
        .enumerate()
        .map(|(n, input)| format!("LD {input}\nOUT M{}\n", 1040 + n))
        .collect()
}
fn witnesses(physical: bool) -> String {
    let mut source = String::new();
    if physical {
        for n in 0..4 {
            source.push_str(&format!(
                "LD M{}\nOUT Y{:o}\nLD C{n}\nOUT Y{:o}\nLD X3\nMOV C{n} D{}\n",
                1536 + n,
                16 + n,
                20 + n,
                600 + n
            ));
        }
    }
    for n in 0..8 {
        source.push_str(&format!("LD M{}\nOUT M{}\n", 1040 + n, 500 + n));
    }
    source
}
fn ist_body(source: &str) -> String {
    let drivers = if source == "M1532" {
        physical_m1532_drivers()
    } else {
        system_flag_drivers()
    };
    format!(
        "LD X5\nOUT M1043\nLD X4\nOUT M1044\nLD X6\nOUT M1045\n{drivers}\
LD X3\nIST {source} S20 S80\n\
STL S0\nLD X0\nOUT Y0\nRET\n\
STL S1\nLD X15\nSET S10\nSTL S10\nOUT Y1\nLD X2\nSET S11\nSTL S11\nSET M1043\nRST S11\nRET\n\
STL S2\nLD M1041\nAND M1044\nSET S20\nSTL S20\nOUT Y2\nLD X0\nSET S80\nSTL S80\nOUT Y3\nLD X2\nOUT S2\nRET\n{}{}",
        witnesses(source == "M1532"),
        monitor()
    )
}

fn inhibit_witnesses() -> &'static str {
    // Sample real Y reads into M before the next instruction. These captures
    // expose service ordering, rather than merely testing an end-scan image.
    "LD Y0\nOUT M520\nLD Y7\nOUT M521\nLD Y10\nOUT M522\nLD Y377\nOUT M523\nLD Y1\nOUT M524\nLD Y376\nOUT M525\nLD M1034\nOUT M526\nLD M1000\nINC D950\nEND"
}
fn inhibit_seed() -> String {
    format!(
        "{}\nSET S0\nRST M1034\nSET Y0\nSET Y7\nSET Y10\nSET Y377\nRST Y1\nRST Y376",
        clear()
    )
}
fn inhibit_stages() -> Stages {
    vec![
        (
            vec![
                ("X1", true),
                ("X0", false),
                ("X2", false),
                ("X3", true),
                ("X4", false),
                ("X7", true),
            ],
            1,
        ),
        (vec![("X1", false), ("X0", true)], 1),
        (vec![], 3),
        (vec![("X2", true)], 2),
        (vec![("X2", false), ("X4", true)], 2),
        (vec![("X0", false), ("X4", false)], 2),
        (vec![("X0", true)], 2),
        (vec![("X0", false)], 2),
    ]
}
fn inhibit_cases() -> Vec<Case> {
    let definitions = [
        (
            "M1034_level_false_OUT_restores_original_nonzero_image",
            "LD X0\nOUT M1034\n",
        ),
        (
            "M1034_held_writes_cleared_next_instruction_without_resaving",
            "LD X0\nOUT M1034\nLD X2\nSET Y1\nSET Y376\nLD Y1\nOUT M510\nLD Y376\nOUT M511\nLD X4\nRST Y7\nLD Y7\nOUT M512\n",
        ),
        (
            "M1034_conditional_false_OUT_restores_before_following_LDY",
            "LD X0\nAND X7\nOUT M1034\nLD Y0\nOUT M510\nLD Y377\nOUT M511\n",
        ),
        (
            "M1034_SET_RST_source_phases_and_simultaneous_requests",
            "LD X0\nSET M1034\nLD X2\nRST M1034\nLD Y7\nOUT M510\n",
        ),
        (
            "M1034_active_STL_SET_RST_keeps_service_global",
            "STL S0\nLD X0\nSET M1034\nLD X2\nRST M1034\nLD Y10\nOUT M510\nRET\n",
        ),
        (
            "M1034_CJ_skips_STL_clear_and_RET_until_flag_cleared",
            "STL S0\nLD X0\nSET M1034\nLD X4\nCJ P200\nLD X2\nRST M1034\nRET\nP200\nLD Y377\nOUT M510\n",
        ),
        (
            "M1034_inactive_STL_RST_cannot_clear_then_external_RST_restores",
            "LD X0\nSET M1034\nSTL S20\nRST M1034\nRET\nLD M1034\nOUT M510\nLD X2\nRST M1034\nLD Y0\nOUT M511\n",
        ),
        (
            "M1034_new_ON_edge_replaces_snapshot_after_OFF_image_change",
            "LD X0\nOUT M1034\nLD Y1\nOUT M513\nLD Y7\nOUT M514\nLD X2\nOUT Y1\nLD X4\nRST Y7\nLD Y1\nOUT M510\n",
        ),
    ];
    definitions
        .into_iter()
        .map(|(name, body)| {
            let mut stages = inhibit_stages();
            if name.contains("conditional_false") {
                stages.extend([
                    (vec![("X0", true), ("X7", true)], 2),
                    (vec![("X7", false)], 2),
                    (vec![("X7", true)], 2),
                ]);
            }
            if name.contains("CJ_skips") {
                stages.extend([
                    (vec![("X0", true), ("X2", true), ("X4", true)], 2),
                    (vec![("X0", false), ("X4", false)], 2),
                ]);
            }
            if name.contains("new_ON_edge") {
                stages.extend([
                    (vec![("X0", false), ("X2", true), ("X4", true)], 2),
                    (vec![("X0", true), ("X2", false)], 3),
                    (vec![("X0", false), ("X4", false)], 2),
                ]);
            }
            fixture(
                name,
                &inhibit_seed(),
                &format!("{body}{}", inhibit_witnesses()),
                stages,
            )
        })
        .collect()
}

pub fn cases(target: Target) -> Vec<Case> {
    let mut out = Vec::new();
    for mode in 0..5 {
        for (completed, zero, retain) in [
            (false, true, false),
            (true, true, false),
            (false, false, false),
            (false, true, true),
        ] {
            out.push(fixture(
                &format!(
                    "IST_M1532_actual_Cvector_mode{mode}_done{completed}_zero{zero}_retain{retain}"
                ),
                &format!("{}\nSET M1536\nRST M1537\nSET M1538\nRST M1539", clear()),
                &ist_body("M1532"),
                mode_stages(mode, completed, zero, retain),
            ));
        }
    }
    for retain in [false, true] {
        let mut stages = mode_stages(4, false, true, retain);
        for mode in [0, 1, 3, 0, 4, 1, 0] {
            stages.push((controls(Some(mode), true, true, false), 2));
            stages.push((controls(Some(mode), false, false, true), 2));
            stages.push((controls(Some(mode), false, false, false), 1));
        }
        out.push(fixture(
            &format!("IST_M1532_actual_Cvector_mode_switch_retain{retain}"),
            &format!("{}\nSET M1536\nRST M1537\nSET M1538\nRST M1539", clear()),
            &ist_body("M1532"),
            stages,
        ));
    }
    // Here the vector is M1040..1047 itself. IST overwrites several of these
    // flags after decoding: a lazy sequential re-read would change its inputs.
    // The driver is immediately before IST; independent flag setters above it
    // are deliberately superseded, making this source alias explicit.
    for mode in 0..5 {
        out.push(fixture(
            &format!("IST_system_M1040_vector_frozen_before_flag_mutation_mode{mode}"),
            clear(),
            &ist_body("M1040"),
            mode_stages(mode, false, true, false),
        ));
    }
    let mut seed = format!("{}\nSET M1047\n", clear());
    let mut nine = String::new();
    for n in 0..9 {
        seed.push_str(&format!("SET S{n}\n"));
        nine.push_str(&format!("STL S{n}\n"));
    }
    nine.push_str(&format!(
        "LD X0\nSET S20\nSTL S20\nOUT Y0\nRET\n{}",
        monitor()
    ));
    out.push(fixture(
        "nine_adjacent_STL_headers_Installed8_capacity_M1048",
        &seed,
        &nine,
        ordinary_stages(),
    ));

    for all_on in [false, true] {
        out.push(fixture(
            &format!("NOP_between_STL_headers_breaks_join_all_on{all_on}"),
            &format!(
                "{}\nSET S0\n{} S1",
                clear(),
                if all_on { "SET" } else { "RST" }
            ),
            &format!(
                "STL S0\nNOP\nSTL S1\nLD X0\nSET S20\nSTL S20\nOUT Y0\nRET\n{}",
                monitor()
            ),
            ordinary_stages(),
        ));
    }
    // This valid documented16-terminal graph is an installed8-capacity
    // diagnostic, not a promise that the two predecessor profiles coincide.
    for (_, source, _) in super::sfc_cases::deferred_sources() {
        out.push(case(
            "sfc_extra_documented16_graph_Installed8_capacity_diagnostic",
            &source,
            ordinary_stages(),
        ));
    }
    for timer in [0, 184, 240] {
        for reset_before in [false, true] {
            let reset = format!("LD X5\nRST T{timer}\n");
            let body = format!(
                "{}STL S0\nTMR T{timer} K3\nLD X0\nSET S20\nSTL S20\nOUT Y0\nLD X2\nSET S30\nSTL S30\nTMR T{timer} K1\nLD X4\nOUT S0\nRET\n{}LD X3\nMOV T{timer} D800\n{}",
                if reset_before { reset.as_str() } else { "" },
                if reset_before { "" } else { reset.as_str() },
                monitor()
            );
            out.push(fixture(
                &format!("nonadjacent_shared_TMR_T{timer}_RST_before{reset_before}_false_cleanup"),
                &format!("{}\nSET S0\nRST T{timer}", clear()),
                &body,
                ordinary_stages(),
            ));
        }
    }
    for initial_active in [false, true] {
        out.push(fixture(&format!("CNT_false_exit_mask_and_RST_initial_active{initial_active}"),
            &format!("{}\n{} S0\nRST C0",clear(),if initial_active{"SET"}else{"RST"}),
            &format!("STL S0\nLD X0\nCNT C0 K2\nLD X2\nOUT S20\nSTL S20\nOUT Y0\nLD X4\nRST C0\nOUT S0\nRET\nLD C0\nOUT Y20\nLD X3\nMOV C0 D800\n{}",monitor()),ordinary_stages()));
    }
    // Both targets share these finite device ranges; target remains explicit
    // so this module fits the suite's per-model fixture interface.
    out.extend(inhibit_cases());
    let _ = target;
    out
}
