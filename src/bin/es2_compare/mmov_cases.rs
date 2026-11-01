//! Genuine MMOV/ MMOVP sign-extension cases, separate from former stub probes.
use super::{Case, case};
use opendvp::es2_bridge::Target;

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (vec![("X1", true), ("X0", false)], 1),
        (vec![("X1", false), ("X0", true)], 2),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}

fn source(seed: &str, body: &str) -> String {
    format!(
        "LD X1\nDMOV H12345678 D20\n{seed}\nSET M1020\nSET M1021\nSET M1022\nSET M1029\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\nLD X0\n{body}\nMOV K123 D900\nINC D901\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLD M1029\nOUT Y5\nLDI X7\nMOV D1067 D902\nMOV D1068 D903\nEND"
    )
}

pub(super) fn cases(target: Target) -> Vec<Case> {
    let mut cases = Vec::new();
    let mut add = |name: &str, seed: &str, body: &str| {
        cases.push(case(
            &format!("mmov_native_{name}"),
            &source(seed, body),
            stages(),
        ));
    };
    let last = if target == Target::Ss2 { 4998 } else { 9998 };
    for op in ["MMOV", "MMOVP"] {
        for value in [-32768, -1, 0, 1, 32767] {
            add(
                &format!("literal_{op}_{value}"),
                "",
                &format!("{op} K{value} D20"),
            );
        }
        for value in ["H8000", "HFFFF"] {
            add(
                &format!("hex_{op}_{value}"),
                "",
                &format!("{op} {value} D20"),
            );
        }
        for value in [-32768, -1, 32767] {
            for (src, dest) in [("D0", "D0"), ("D0", "D1"), ("D1", "D0")] {
                add(
                    &format!("alias_{op}_{src}_{dest}_{value}"),
                    &format!("DMOV H12345678 D0\nMOV K{value} {src}"),
                    &format!("{op} {src} {dest}"),
                );
            }
        }
        for src in ["T0", "C0", "T255", "C199"] {
            add(
                &format!("word_source_{op}_{src}"),
                &format!("DIV16 K-32768 K1 {src}"),
                &format!("{op} {src} D20"),
            );
        }
        for dest in [
            "T0".to_string(),
            "T254".into(),
            "C0".into(),
            "C198".into(),
            "C199".into(),
            "C200".into(),
            "C255".into(),
            format!("D{last}"),
        ] {
            add(
                &format!("destination_{op}_{dest}"),
                &format!("DMOV H12345678 C200\nDMOV H76543210 {dest}"),
                &format!("{op} K-7 {dest}"),
            );
        }
        for digits in 1..=4u16 {
            let bits = digits * 4;
            let mut seed = String::new();
            // Smaller groups are zero-extended; a full four-digit word
            // preserves b15 and must be sign-extended by MMOV.
            for bit in [0, bits - 1] {
                seed.push_str(&format!("SET M{}\n", 2000 + bit));
            }
            add(
                &format!("packed_M_source_{op}_K{digits}"),
                &seed,
                &format!("{op} K{digits}M2000 D20"),
            );
        }
        add(
            &format!("packed_Y_source_{op}"),
            "SET Y100\nSET Y117",
            &format!("{op} K4Y100 D20"),
        );
        add(
            &format!("packed_S_source_{op}"),
            "SET S0\nSET S15",
            &format!("{op} K4S0 D20"),
        );
    }
    // Change the source while held to distinguish ordinary versus pulse
    // execution, then exercise a false scan followed by a new rising edge.
    for op in ["MMOV", "MMOVP"] {
        add(
            &format!("mutable_source_lifecycle_{op}"),
            "MOV K-2 D0",
            &format!("{op} D0 D20\nINC D0"),
        );
    }
    for op in ["MMOV", "MMOVP"] {
        for digits in [1, 4] {
            let mut input_stages = stages();
            for (inputs, _) in &mut input_stages {
                inputs.extend([("X100", true), ("X103", true), ("X117", true)]);
            }
            cases.push(case(
                &format!("mmov_native_packed_X_source_{op}_K{digits}"),
                &source("", &format!("{op} K{digits}X100 D20")),
                input_stages,
            ));
        }
    }
    cases
}
