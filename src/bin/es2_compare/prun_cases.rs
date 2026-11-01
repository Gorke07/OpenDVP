//! Bounded differential PRUN corpus. Original prun-native-runtime.json proves
//! four forms and three routes; clipping/alignment/control add new probes.
//! No expected result is copied from the installed simulator into the model.
use super::{Case, case};
use opendvp::es2_bridge::Target;

type Stages = Vec<(Vec<(&'static str, bool)>, usize)>;
const INPUTS: [&str; 32] = [
    "X20", "X21", "X22", "X23", "X24", "X25", "X26", "X27", "X30", "X31", "X32", "X33", "X34",
    "X35", "X36", "X37", "X40", "X41", "X42", "X43", "X44", "X45", "X46", "X47", "X50", "X51",
    "X52", "X53", "X54", "X55", "X56", "X57",
];
fn inputs(bits: u32) -> Vec<(&'static str, bool)> {
    INPUTS
        .iter()
        .enumerate()
        .map(|(i, &name)| (name, bits & (1 << i) != 0))
        .collect()
}
fn stages() -> Stages {
    let mut initial = inputs(0x96a5_c369);
    initial.extend([("X0", false), ("X1", true), ("X2", false), ("X6", true)]);
    let mut changed = inputs(0x3c5a_81e7);
    changed.push(("X2", true));
    vec![
        (initial, 1),
        (vec![("X1", false), ("X0", true)], 1),
        (changed, 1), // live source edit while the P condition stays high
        (vec![("X2", false)], 2),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
        (vec![], 1),
    ]
}
fn seed(pattern: u64) -> String {
    let mut out = String::new();
    // Full-mask MVM seeds contiguous groups with existing native-tested syntax.
    // Keep the final two bits and the explicit decimal-bank holes below.
    for i in [0, 16, 32] {
        let word = (pattern >> i) & 0xffff;
        out.push_str(&format!(
            "MOV H{word:04X} D804\nMVM D804 HFFFF K4M{}\n",
            10 + i
        ));
    }
    for i in 48..50 {
        let op = if pattern & (1u64 << i) != 0 {
            "SET"
        } else {
            "RST"
        };
        out.push_str(&format!("{op} M{}\n", 10 + i));
    }
    for y in 16..48 {
        let op = if y % 3 == 0 { "SET" } else { "RST" };
        out.push_str(&format!("{op} Y{y:o}\n"));
    }
    // Decimal bank holes are explicit witnesses, not copied source payload.
    out.push_str("SET M18\nSET M19\nSET M28\nRST M29\nSET M38\nRST M39\nSET M48\nRST M49\n");
    out
}
fn init() -> String {
    format!(
        "LD X1\n{}LD X2\n{}",
        seed(0x0024_9264_a931),
        seed(0x0019_c3a6_5ac7)
    )
}
fn witnesses() -> &'static str {
    // Fixed marker/data words and error relays remain disjoint from all routes.
    "LD X6\nMOV H1357 D800\nINC D801\nLD M1067\nOUT Y0\nLD M1068\nOUT Y1\nLD X6\nMOV D1067 D802\nMOV D1137 D803\nEND"
}
fn fixture(name: &str, body: &str) -> Case {
    case(
        &format!("prun_{name}"),
        &format!("{}NOP\n{body}\n{}", init(), witnesses()),
        stages(),
    )
}

pub(super) fn cases(_target: Target) -> Vec<Case> {
    let mut out = Vec::new();
    for (op, digits) in [("PRUN", 4), ("PRUNP", 4), ("DPRUN", 8), ("DPRUNP", 8)] {
        for (route, source, dest) in [
            ("X_to_M_gaps", "X20", "M10"),
            ("M_to_Y_sparse_source", "M10", "Y20"),
            ("M_to_M_live_forward_overlap", "M10", "M20"),
        ] {
            out.push(fixture(
                &format!("{op}_{route}"),
                &format!("LD X0\n{op} K{digits}{source} K{digits}{dest}"),
            ));
        }
    }
    out.push(fixture(
        "PRUN_source_alignment_error",
        "LD X0\nPRUN K4X21 K4M10",
    ));
    out.push(fixture(
        "DPRUN_dest_alignment_error",
        "LD X0\nDPRUN K8X20 K8M11",
    ));
    out.push(fixture(
        "PRUN_destination_clips_four_bits",
        "LD X0\nPRUN K4X20 K1M10",
    ));
    out.push(fixture(
        "DPRUN_destination_clips_twelve_bits",
        "LD X0\nDPRUN K8M10 K3Y20",
    ));
    out.push(fixture(
        "DPRUNP_MC_false_true_rearm",
        "LD X0\nMC N0\nLD X6\nDPRUNP K8X20 K8M10\nMCR N0",
    ));
    let source = format!(
        "{}NOP\nFOR K3\nLD X0\nCALL P0\nNEXT\nLD X6\nMOV H1357 D800\nINC D801\nFEND\nP0\nLD X0\nDPRUN K8M10 K8M20\nSRET\nEND",
        init()
    );
    out.push(case(
        "prun_DPRUN_CALL_FOR_live_overlap_same_PC",
        &source,
        stages(),
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_corpus_compiles_encodes_and_runs_without_limitations() {
        let cases = cases(Target::Es2);
        assert_eq!(cases.len(), 18);
        for c in cases {
            let p =
                opendvp::plc::compile_il(&c.source).unwrap_or_else(|e| panic!("{}: {e:?}", c.name));
            assert!(
                !opendvp::es2_encoder::encode(&p).unwrap().words.is_empty(),
                "{}",
                c.name
            );
            assert!(
                p.requires_vm(),
                "{} must use independent control-PC prediction",
                c.name
            );
            let mut sim = opendvp::plc::Simulator::default();
            for (input, scans) in c.stages {
                for (input, level) in input {
                    sim.memory.set_input(input.parse().unwrap(), level);
                }
                for _ in 0..scans {
                    sim.scan(&p, 100);
                    assert_eq!(
                        sim.last_scan,
                        opendvp::plc::ScanStatus::Complete,
                        "{}",
                        c.name
                    );
                    assert!(
                        sim.memory.execution_limitations.is_empty(),
                        "{}: {:?}",
                        c.name,
                        sim.memory.execution_limitations
                    );
                }
            }
        }
    }
}
