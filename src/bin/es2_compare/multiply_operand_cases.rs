//! Operand differentials for full and truncated multiply/divide forms.
//! The explicit E-target diagnostics are not part of the passing main corpus.
use super::{Case, case};
use std::collections::BTreeSet;

const FORMS: [(&str, bool, bool); 16] = [
    ("MUL", false, false),
    ("MULP", false, false),
    ("DIV", false, false),
    ("DIVP", false, false),
    ("DMUL", true, false),
    ("DMULP", true, false),
    ("DDIV", true, false),
    ("DDIVP", true, false),
    ("MUL16", false, true),
    ("MUL16P", false, true),
    ("DIV16", false, true),
    ("DIV16P", false, true),
    ("MUL32", true, true),
    ("MUL32P", true, true),
    ("DIV32", true, true),
    ("DIV32P", true, true),
];

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (vec![("X1", true), ("X0", false)], 1),
        (vec![("X1", false), ("X0", true)], 2),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}

fn address(area: &str, index: u16) -> String {
    if matches!(area, "X" | "Y") {
        format!("{area}{index:o}")
    } else {
        format!("{area}{index}")
    }
}

fn seed_bits(area: &str, base: u16, bits: u16, value: u64) -> String {
    (0..bits)
        .map(|bit| {
            format!(
                "{} {}",
                if value & (1u64 << bit) != 0 {
                    "SET"
                } else {
                    "RST"
                },
                address(area, base + bit),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn scenario_with_prefix(
    prefix: &str,
    name: &str,
    seed: &str,
    body: &str,
    guards: &[(&str, bool)],
    observe: &str,
) -> Case {
    let guard_seed = guards
        .iter()
        .map(|(name, on)| format!("{} {name}", if *on { "SET" } else { "RST" }))
        .collect::<Vec<_>>()
        .join("\n");
    let guard_reads = guards
        .iter()
        .enumerate()
        .map(|(i, (name, _))| format!("LD {name}\nOUT Y{:o}", i + 8))
        .collect::<Vec<_>>()
        .join("\n");
    case(
        &format!("{prefix}{name}"),
        &format!(
            "LD X1\nMOV H55AA D1028\nMOV H1234 D1029\n{seed}\n{guard_seed}\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\nLD X0\n{body}\nLD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24\n{guard_reads}\nLD X0\nORI X0\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305\n{observe}\nEND"
        ),
        stages(),
    )
}

fn scenario(name: &str, seed: &str, body: &str, guards: &[(&str, bool)], observe: &str) -> Case {
    scenario_with_prefix("multiply_operand_", name, seed, body, guards, observe)
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    // All sixteen forms exercise source changes during held power, a false
    // scan and rearming. A full result has two eight-bit chunks; a truncated
    // result has one. Observe the second chunk even for truncated commands.
    for (mnemonic, wide, truncated) in FORMS {
        let bits = if wide { 32 } else { 16 };
        for (area, source_area) in [("Y", "M"), ("M", "S"), ("S", "Y")] {
            let target = address(area, 128);
            let source = address(source_area, 64);
            let before = address(area, 127);
            let second = address(area, 136);
            let after = address(area, if truncated { 136 } else { 144 });
            let outer = address(area, 145);
            let seed = format!(
                "{}\n{}",
                seed_bits(source_area, 64, bits, (-17i64) as u64),
                seed_bits(area, 128, 16, 0x5aa5),
            );
            let body = format!(
                "{mnemonic} K{}{} K3 K2{target}\n{} K{}{}",
                bits / 4,
                source,
                if wide { "DINC" } else { "INC" },
                bits / 4,
                source,
            );
            cases.push(scenario(
                &format!("lifecycle_{mnemonic}_{target}"),
                &seed,
                &body,
                &[
                    (before.as_str(), true),
                    (after.as_str(), false),
                    (outer.as_str(), true),
                ],
                &format!("MOV K2{target} D20\nMOV K2{second} D21"),
            ));
        }
    }

    cases.push(scenario(
        "full_word_two_nibbles_negative_product_and_remainder",
        &format!(
            "{}\n{}",
            seed_bits("M", 128, 8, 0xa5),
            seed_bits("M", 160, 8, 0x5a)
        ),
        "MUL K-17 K3 K1M128\nDIV K-17 K3 K1M160",
        &[
            ("M127", true),
            ("M136", false),
            ("M159", true),
            ("M168", false),
        ],
        "MOV K1M128 D20\nMOV K1M132 D21\nMOV K1M160 D22\nMOV K1M164 D23",
    ));
    cases.push(scenario(
        "full_double_two_nibbles_negative_product_and_remainder",
        &format!(
            "{}\n{}",
            seed_bits("M", 192, 8, 0xa5),
            seed_bits("M", 224, 8, 0x5a)
        ),
        "DMUL K-17 K3 K1M192\nDDIV K-17 K3 K1M224",
        &[
            ("M191", true),
            ("M200", false),
            ("M223", true),
            ("M232", false),
        ],
        "MOV K1M192 D20\nMOV K1M196 D21\nMOV K1M224 D22\nMOV K1M228 D23",
    ));
    cases.push(scenario(
        "full_word_two_twelve_bit_chunks",
        &format!(
            "{}\n{}",
            seed_bits("M", 128, 24, 0x5aa55a),
            seed_bits("M", 192, 24, 0xa55aa5)
        ),
        "MUL K-30000 K3 K3M128\nDIV K-30000 K7 K3M192",
        &[
            ("M127", true),
            ("M152", false),
            ("M191", true),
            ("M216", false),
        ],
        "MOV K3M128 D20\nMOV K3M140 D21\nMOV K3M192 D22\nMOV K3M204 D23",
    ));
    cases.push(scenario(
        "full_double_two_twenty_eight_bit_chunks",
        &format!(
            "{}\n{}",
            seed_bits("M", 128, 56, 0x5aa55aa55aa55a),
            seed_bits("M", 224, 56, 0xa55aa55aa55aa5)
        ),
        "DMUL K-123456789 K19 K7M128\nDDIV K-123456789 K19 K7M224",
        &[
            ("M127", true),
            ("M184", false),
            ("M223", true),
            ("M280", false),
        ],
        "DMOV K7M128 D20\nDMOV K7M156 D22\nDMOV K7M224 D24\nDMOV K7M252 D26",
    ));
    cases.push(scenario(
        "full_word_shifted_source_target_snapshot",
        &format!(
            "{}\n{}",
            seed_bits("M", 128, 32, 0xa55affef),
            seed_bits("M", 192, 32, 0x5aa5ffef)
        ),
        "MUL K4M128 K3 K3M132\nDIV K4M192 K3 K3M196",
        &[
            ("M127", true),
            ("M160", false),
            ("M191", true),
            ("M224", false),
        ],
        "MOV K3M132 D20\nMOV K3M144 D21\nMOV K3M196 D22\nMOV K3M208 D23",
    ));
    cases.push(scenario(
        "full_double_shifted_source_target_snapshot",
        &format!(
            "{}\n{}",
            seed_bits("M", 128, 48, 0xa55afffffef1),
            seed_bits("M", 224, 48, 0x5aaafffffef1)
        ),
        "DMUL K8M128 K3 K4M136\nDDIV K8M224 K3 K4M232",
        &[
            ("M127", true),
            ("M176", false),
            ("M223", true),
            ("M272", false),
        ],
        "MOV K4M136 D20\nMOV K4M152 D21\nMOV K4M232 D22\nMOV K4M248 D23",
    ));
    cases.push(scenario(
        "zero_divisors_preserve_full_and_truncated_groups",
        &format!(
            "{}\n{}\n{}\n{}",
            seed_bits("M", 128, 16, 0x1234),
            seed_bits("M", 192, 16, 0x5678),
            seed_bits("M", 256, 8, 0x9a),
            seed_bits("M", 320, 8, 0xbc)
        ),
        "DIV K-17 K0 K2M128\nDDIV K-17 K0 K2M192\nDIV16 K-17 K0 K2M256\nDIV32 K-17 K0 K2M320",
        &[
            ("M144", true),
            ("M208", false),
            ("M264", true),
            ("M328", false),
        ],
        "MOV K4M128 D20\nMOV K4M192 D21\nMOV K2M256 D22\nMOV K2M320 D23",
    ));
    cases.push(scenario(
        "zero_divisor_recovery_keeps_latched_error_and_pc",
        &format!("MOV K0 D0\nDMOV K0 D2\n{}\n{}\n{}\n{}", seed_bits("M", 128, 8, 0xa5), seed_bits("M", 160, 8, 0x5a), seed_bits("M", 192, 4, 0xb), seed_bits("M", 224, 4, 0xc)),
        "DIV K-17 D0 K1M128\nDDIV K-17 D2 K1M160\nDIV16 K-17 D0 K1M192\nDIV32 K-17 D2 K1M224\nINC D0\nDINC D2",
        &[("M136", true), ("M168", false), ("M196", true), ("M228", false)],
        "MOV K2M128 D20\nMOV K2M160 D21\nMOV K1M192 D22\nMOV K1M224 D23",
    ));
    cases.push(scenario(
        "full_E_sources_and_documented_alias_coherence",
        "MOV K-17 E1\nMOV K3 E2\nDMOV K-700000001 E3\nDMOV K19 E4",
        "MUL E1 E2 K2M128\nDIV E1 E2 K2M160\nDMUL E3 E4 K4M192\nDDIV E3 E4 K4M256",
        &[("M144", true), ("M176", false), ("M224", true), ("M288", false)],
        "MOV E1 D20\nMOV D1182 D21\nDMOV E3 D22\nDMOV D1186 D24\nMOV K4M128 D26\nMOV K4M160 D27\nDMOV K8M192 D28\nDMOV K8M256 D30",
    ));
    cases.push(scenario(
        "truncated_E_sources_and_short_target_values",
        "MOV K-17 E1\nMOV K3 E2\nDMOV K-700000001 E3\nDMOV K19 E4",
        "MUL16 E1 E2 K2M128\nDIV16 E1 E2 K2M160\nMUL32 E3 E4 K4M192\nDIV32 E3 E4 K4M256",
        &[("M136", true), ("M168", false), ("M208", true), ("M272", false)],
        "MOV E1 D20\nMOV D1182 D21\nDMOV E3 D22\nDMOV D1186 D24\nMOV K2M128 D26\nMOV K2M160 D27\nMOV K4M192 D28\nMOV K4M256 D29",
    ));
    cases.push(scenario(
        "full_word_E_targets_include_F_result_half",
        "DMOV H12345678 E1\nDMOV H23456789 E2\nDMOV H3456789A D182\nDMOV H456789AB D184",
        "MUL K-17 K3 E1\nDIV K-17 K3 E2",
        &[],
        "MOV E1 D20\nMOV F1 D21\nMOV E2 D22\nMOV F2 D23\nDMOV D1182 D24\nDMOV D1184 D26\nDMOV D182 D28\nDMOV D184 D30",
    ));
    cases.push(scenario(
        "truncated_word_E_targets_preserve_F_neighbor",
        "DMOV H12345678 E1\nDMOV H23456789 E2\nMOV K41 D182\nMOV K42 D184",
        "MUL16 K300 K300 E1\nDIV16 K-17 K3 E2",
        &[],
        "MOV E1 D20\nMOV F1 D21\nMOV E2 D22\nMOV F2 D23\nDMOV D1182 D24\nDMOV D1184 D26\nMOV D182 D28\nMOV D184 D29",
    ));
    cases.push(scenario(
        "truncated_word_group_overflow_with_latched_errors",
        &format!("{}\n{}\n{}\nMOV K55 D10", seed_bits("M", 128, 8, 0xa5), seed_bits("M", 160, 8, 0x5a), seed_bits("M", 192, 8, 0xbc)),
        "MUL16 K300 K300 K2M128\nLD M1022\nOUT Y0\nLD X0\nMUL16 K127 K2 K2M160\nLD M1022\nOUT Y1\nLD X0\nSET M1067\nSET M1068\nMUL16 K300 K300 K2M192\nLD M1022\nOUT Y2\nLD X0\nMUL16 K300 K300 D10\nLD M1022\nOUT Y3",
        &[("M136", true), ("M168", false), ("M200", true)],
        "MOV K2M128 D20\nMOV K2M160 D21\nMOV K2M192 D22\nMOV D10 D23",
    ));
    cases.push(scenario(
        "truncated_double_group_overflow_with_latched_errors",
        &format!("{}\n{}\n{}\nDMOV K55 D10", seed_bits("M", 128, 16, 0xa55a), seed_bits("M", 192, 16, 0x5aa5), seed_bits("M", 256, 16, 0xbcde)),
        "MUL32 K2147483647 K2 K4M128\nLD M1022\nOUT Y0\nLD X0\nMUL32 K-17 K3 K4M192\nLD M1022\nOUT Y1\nLD X0\nSET M1067\nSET M1068\nMUL32 K2147483647 K2 K4M256\nLD M1022\nOUT Y2\nLD X0\nMUL32 K2147483647 K2 D10\nLD M1022\nOUT Y3",
        &[("M144", true), ("M208", false), ("M272", true)],
        "MOV K4M128 D20\nMOV K4M192 D21\nMOV K4M256 D22\nDMOV D10 D24",
    ));
    cases.push(scenario(
        "full_natural_counter_result_storage_and_neighbors",
        "DMOV H12345678 C200\nDMOV H23456789 C201\nDMOV H3456789A C202\nDMOV H456789AB C203\nDMOV K123456789 C210\nDMOV K41 C212\nDMOV K42 C213\nDMOV K43 C214\nDMOV K-123456789 C220\nDMOV K44 C222\nDMOV K45 C223\nDMOV K46 C224",
        "MUL K-300 K300 C200\nDIV K-17 K3 C202\nDMUL C210 K-3 C212\nDDIV C220 K7 C222",
        &[],
        "DMOV C200 D20\nDMOV C201 D22\nDMOV C202 D24\nDMOV C203 D26\nDMOV C212 D28\nDMOV C213 D30\nDMOV C214 D32\nDMOV C222 D34\nDMOV C223 D36\nDMOV C224 D38",
    ));
    cases.push(scenario(
        "truncated_numeric_endpoints_and_natural_counters",
        "MOV K41 T254\nMOV K42 T255\nMOV K43 C198\nMOV K44 C199\nDMOV K123456789 C200\nDMOV K45 C201\nDMOV K46 C202\nDMOV K47 C203\nDMOV K-123456789 C210\nDMOV K48 C212\nDMOV K49 C213",
        "MUL16 K300 K3 T255\nDIV16 K-17 K3 C199\nMUL32 C200 K3 C202\nDIV32 C210 K7 C212",
        &[],
        "MOV T254 D20\nMOV T255 D21\nMOV C198 D22\nMOV C199 D23\nDMOV C200 D24\nDMOV C201 D26\nDMOV C202 D28\nDMOV C203 D30\nDMOV C212 D32\nDMOV C213 D34",
    ));

    let mut input_case = scenario(
        "KnX_sources_for_full_and_truncated_widths",
        "MOV K41 D20\nMOV K42 D21",
        "MUL K1X10 K3 K1M128\nDIV K1X10 K3 K1M160\nDMUL K1X10 K3 K1M192\nDDIV K1X10 K3 K1M224\nMUL16 K1X10 K3 K1M256\nDIV16 K1X10 K3 K1M288\nMUL32 K1X10 K3 K1M320\nDIV32 K1X10 K3 K1M352",
        &[
            ("M136", true),
            ("M168", false),
            ("M260", true),
            ("M324", false),
        ],
        "MOV K2M128 D20\nMOV K2M160 D21\nMOV K2M192 D22\nMOV K2M224 D23\nMOV K1M256 D24\nMOV K1M288 D25\nMOV K1M320 D26\nMOV K1M352 D27",
    );
    input_case.stages[0]
        .0
        .extend([("X10", true), ("X11", true), ("X12", false), ("X13", true)]);
    input_case.stages[2]
        .0
        .extend([("X10", false), ("X11", false), ("X12", true)]);
    cases.push(input_case);

    cases.push(scenario(
        "C199_full_results_preserve_or_cross_into_C200_by_width",
        "MOV K111 C199\nDMOV H12345678 C200\nDMOV H23456789 C201",
        "MUL K-17 K3 C199\nMOV C199 D20\nDMOV C200 D22\nDIV K-17 K3 C199\nMOV C199 D24\nDMOV C200 D26\nDMUL K123456789 K-3 C199\nMOV C199 D30\nDMOV C200 D32\nDDIV K-123456789 K7 C199\nMOV C199 D34\nDMOV C200 D36",
        &[],
        "MOV C199 D40\nDMOV C200 D42\nDMOV C201 D44",
    ));

    cases.push(scenario(
        "word_F_sources_follow_explicit_full_instruction_footnote",
        "MOV K-17 F1\nMOV K3 F2\nDMOV H12345678 E3\nDMOV H23456789 E4",
        "MUL F1 F2 E3\nDIV F1 F2 E4\nMULP F1 F2 K2M128\nDIVP F1 F2 K2M160\nINC F1",
        &[("M144", true), ("M176", false)],
        "MOV F1 D20\nMOV F2 D21\nDMOV E3 D22\nDMOV E4 D24\nMOV K4M128 D26\nMOV K4M160 D27",
    ));
    cases.push(scenario(
        "wide_C197_C198_truncated_and_natural_result_transition",
        "MOV K77 C197\nMOV K78 C198\nMOV K79 C199\nDMOV H12345678 C200\nDMOV H23456789 C201",
        "DMUL K100000 K100000 C197\nMOV C197 D20\nMOV C198 D21\nMOV C199 D22\nDMOV C200 D24\nDDIV K-17 K3 C197\nMOV C197 D30\nMOV C198 D31\nMOV C199 D32\nDMOV C200 D34\nDMUL K100000 K100000 C198\nMOV C198 D40\nMOV C199 D41\nDMOV C200 D42\nDDIV K-17 K3 C198\nMOV C198 D50\nMOV C199 D51\nDMOV C200 D52",
        &[],
        "DMOV C201 D60",
    ));
    cases.push(scenario(
        "full_word_and_double_max_group_widths",
        "MOV K77 D0",
        "MUL K32767 K32767 K4M128\nDIV K-32768 K-1 K4M192\nDMUL K2147483647 K2147483647 K8M256\nDDIV K-2147483648 K2 K8M384",
        &[("M127", true), ("M160", false), ("M191", true), ("M224", false), ("M255", true), ("M320", false), ("M383", true), ("M448", false)],
        "DMOV K8M128 D20\nDMOV K8M192 D22\nDMOV K8M256 D24\nDMOV K8M288 D26\nDMOV K8M384 D28\nDMOV K8M416 D30",
    ));
    assert_eq!(cases.len(), 69);
    let names: BTreeSet<_> = cases.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names.len(), cases.len());
    let actual: BTreeSet<_> = cases
        .iter()
        .flat_map(|c| c.source.lines())
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    for (name, _, _) in FORMS {
        assert!(actual.contains(name), "missing multiply/divide form {name}");
    }
    cases
}

pub fn diagnostic_cases() -> Vec<Case> {
    // These explicit probes distinguish documented E1 storage from the
    // installed direct-pointer resolver's D182/D183 path. They must never be
    // silently counted as passing main cases or use E1 as their observation.
    ["MUL32", "MUL32P", "DIV32", "DIV32P"]
        .into_iter()
        .map(|op| {
            scenario_with_prefix(
                "multiply_index_probe_",
                &format!("{op}_E1_actual_and_resolver_aliases"),
                "DMOV H12345678 E1\nDMOV H23456789 D182\nDMOV K-123456789 D0",
                &format!("{op} D0 K3 E1\nDINC D0"),
                &[],
                "DMOV D182 D20\nDMOV D1182 D22\nMOV E1 D24\nMOV F1 D25\nDMOV D0 D26",
            )
        })
        .collect()
}
