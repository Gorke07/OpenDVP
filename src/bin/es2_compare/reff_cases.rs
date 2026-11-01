//! Only functional installed REFF register semantics are compared here.
//! This corpus does not claim physical filtering or REF/MTR stub parity.
use super::{Case, case};
use opendvp::es2_bridge::Target;
type Stages = Vec<(Vec<(&'static str, bool)>, usize)>;
fn stages() -> Stages {
    vec![
        (
            vec![("X0", false), ("X1", true), ("X2", false), ("X6", true)],
            1,
        ),
        (vec![("X1", false), ("X0", true)], 1),
        (vec![], 3),
        (vec![("X2", true)], 1),
        (vec![("X0", false)], 2),
        (vec![("X2", false), ("X0", true)], 1),
        (vec![], 2),
    ]
}
fn fixture(name: &str, body: &str) -> Case {
    case(
        &format!("reff_{name}"),
        &format!(
            "LD X1\nMOV K17 D1020\nMOV K19 D1021\nLD X2\nMOV K13 D1020\nMOV K11 D1021\n{body}\nLD X6\nMOV D1020 D100\nMOV D1021 D101\nINC D110\nLD M1067\nOUT Y20\nLD M1068\nOUT Y21\nEND"
        ),
        stages(),
    )
}
pub(super) fn cases(_target: Target) -> Vec<Case> {
    let mut out = Vec::new();
    for n in [2, 10, 20] {
        for pulse in [false, true] {
            let op = if pulse { "REFFP" } else { "REFF" };
            out.push(fixture(
                &format!("{op}_K{n}_held_rearm"),
                &format!("LD X0\n{op} K{n}"),
            ));
        }
    }
    out.push(fixture("inverted_false_true", "LDI X0\nREFF K7"));
    out.push(fixture(
        "master_false_holds",
        "LD X0\nMC N0\nLD X6\nREFF K9\nMCR N0",
    ));
    for pulse in [false, true] {
        let op = if pulse { "REFFP" } else { "REFF" };
        out.push(fixture(
            &format!("{op}_FOR_visits"),
            &format!("LD X0\nFOR K3\nLD X6\n{op} K6\nMOV D1020 D120\nNEXT"),
        ));
        let source = format!(
            "LD X1\nMOV K17 D1020\nMOV K19 D1021\nLD X2\nMOV K13 D1020\nMOV K11 D1021\nLD X0\nCALL P0\nLD X6\nMOV D1020 D100\nMOV D1021 D101\nINC D110\nFEND\nP0\nLD X6\n{op} K8\nMOV D1021 D121\nSRET\nEND"
        );
        out.push(case(&format!("reff_{op}_CALL_visits"), &source, stages()));
    }
    out.push(fixture("hex_boundary_H14", "LD X0\nREFF H14"));
    out.push(fixture(
        "source_order_reff_then_direct",
        "LD X0\nREFF K3\nMOV D1020 D120\nMOV D1021 D121\nMOV K18 D1020",
    ));
    out.push(fixture(
        "source_order_direct_then_reff",
        "LD X0\nMOV K18 D1020\nMOV K16 D1021\nREFF K4",
    ));
    out.push(fixture(
        "two_sites_opposite_power",
        "LD X0\nREFF K2\nLDI X0\nREFF K20",
    ));
    out.push(fixture(
        "clear_filter_register_keeps_zero",
        "LD X0\nZRST D1020 D1020",
    ));
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_reff_sources_compile_encode_and_run_locally() {
        for case in cases(Target::Es2) {
            let p = opendvp::plc::compile_il(&case.source)
                .unwrap_or_else(|e| panic!("{}: {e:?}", case.name));
            assert!(!opendvp::es2_encoder::encode(&p).unwrap().words.is_empty());
            let mut sim = opendvp::plc::Simulator::default();
            sim.memory.refresh_io.filter_profile =
                opendvp::refresh_io::FilterProfile::InstalledRegisters;
            for (inputs, count) in case.stages {
                for (input, level) in inputs {
                    sim.memory.set_input(input.parse().unwrap(), level);
                }
                for _ in 0..count {
                    sim.scan(&p, 100);
                    assert_eq!(
                        sim.last_scan,
                        opendvp::plc::ScanStatus::Complete,
                        "{}",
                        case.name
                    );
                    assert!(
                        sim.memory.execution_limitations.is_empty(),
                        "{} {:?}",
                        case.name,
                        sim.memory.execution_limitations
                    );
                }
            }
        }
    }
}
