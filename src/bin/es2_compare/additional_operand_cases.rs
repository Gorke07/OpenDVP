//! E/F operands and retained grouped behavior of SWAP, MVM and RAND.
//! Random instructions deliberately use only equal bounds or domain errors:
//! independent native/local random samples are never compared for equality.
use super::{Case, case};
use std::collections::BTreeSet;

const FORMS: [&str; 12] = [
    "SWAP", "SWAPP", "DSWAP", "DSWAPP", "MVM", "MVMP", "DMVM", "DMVMP", "RAND", "RANDP", "DRAND",
    "DRANDP",
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
    if area == "Y" {
        format!("Y{index:o}")
    } else {
        format!("{area}{index}")
    }
}

fn seed_bits(area: &str, base: u16, bits: u16, value: u32) -> String {
    (0..bits)
        .map(|bit| {
            format!(
                "{} {}",
                if value & (1u32 << bit) != 0 {
                    "SET"
                } else {
                    "RST"
                },
                address(area, base + bit)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn scenario(name: &str, seed: &str, body: &str, guards: &[(&str, bool)], observe: &str) -> Case {
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
        &format!("additional_operand_{name}"),
        &format!(
            "LD X1\nMOV H55AA D1028\nMOV H1234 D1029\n{seed}\n{guard_seed}\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\nLD X0\n{body}\nLD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24\n{guard_reads}\nLD X0\nORI X0\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305\n{observe}\nEND"
        ),
        stages(),
    )
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for op in ["SWAP", "SWAPP", "DSWAP", "DSWAPP"] {
        let wide = op.starts_with('D');
        let bits = if wide { 32 } else { 16 };
        for area in ["Y", "M", "S"] {
            let dest = address(area, 128);
            let before = address(area, 127);
            let after = address(area, 128 + bits);
            cases.push(scenario(
                &format!("lifecycle_{op}_{dest}"),
                &seed_bits(area, 128, bits, if wide { 0x1234abcd } else { 0x1234 }),
                &format!("{op} K{}{dest}", bits / 4),
                &[(before.as_str(), true), (after.as_str(), false)],
                &format!(
                    "{} K{}{dest} D20",
                    if wide { "DMOV" } else { "MOV" },
                    bits / 4
                ),
            ));
        }
    }
    for op in ["MVM", "MVMP", "DMVM", "DMVMP"] {
        let wide = op.starts_with('D');
        let bits = if wide { 32 } else { 16 };
        for (area, source_area) in [("Y", "M"), ("M", "S"), ("S", "Y")] {
            let source = address(source_area, 64);
            let dest = address(area, 128);
            let before = address(area, 127);
            let after = address(area, 136);
            cases.push(scenario(
                &format!("lifecycle_{op}_{dest}_mask_E1"),
                &format!(
                    "DMOV {} E1\n{}\n{}",
                    if wide { "H0F0FF0F0" } else { "H12340F0F" },
                    seed_bits(
                        source_area,
                        64,
                        bits,
                        if wide { 0xaa5533cc } else { 0xaa55 }
                    ),
                    seed_bits(area, 128, 16, 0xa55a)
                ),
                &format!(
                    "{op} K{}{source} E1 K2{dest}\n{} {} K{}{source}",
                    bits / 4,
                    if wide { "DMOV" } else { "MOV" },
                    if wide { "H55AACC33" } else { "H55AA" },
                    bits / 4
                ),
                &[(before.as_str(), true), (after.as_str(), false)],
                &format!("MOV K4{dest} D20\nMOV E1 D21\nMOV F1 D22"),
            ));
        }
    }
    for op in ["RAND", "RANDP", "DRAND", "DRANDP"] {
        let wide = op.starts_with('D');
        let mov = if wide { "DMOV" } else { "MOV" };
        let initial = if wide { 70000 } else { 300 };
        for area in ["Y", "M", "S"] {
            let dest = address(area, 128);
            let before = address(area, 127);
            let after = address(area, 136);
            cases.push(scenario(
                &format!("lifecycle_{op}_{dest}_equal_E1_bounds"),
                &format!("{mov} K{initial} E1\n{}", seed_bits(area, 128, 16, 0xa55a)),
                &format!("{op} E1 E1 K2{dest}\n{mov} K{} E1", initial + 1),
                &[(before.as_str(), true), (after.as_str(), false)],
                &format!("MOV K4{dest} D20\nMOV E1 D21\nMOV F1 D22"),
            ));
        }
    }
    assert_eq!(cases.len(), 36);

    // SWAP's explicit F restriction is retained: only word SWAP uses F.
    for op in ["SWAP", "SWAPP", "DSWAP", "DSWAPP"] {
        let wide = op.starts_with('D');
        let indices: &[(&str, u16)] = if wide {
            &[("E0", 1028), ("E1", 1182), ("E7", 1194)]
        } else {
            &[("E0", 1028), ("F0", 1029), ("E1", 1182), ("F7", 1195)]
        };
        for (index, alias) in indices {
            let seed = if wide {
                format!("DMOV H1234ABCD D{alias}\nMOV H5AA5 D{}", alias + 2)
            } else {
                format!("MOV H1234 D{alias}\nMOV H5678 D{}", alias + 1)
            };
            cases.push(scenario(
                &format!("lifecycle_{op}_index_{index}"),
                &seed,
                &format!("{op} {index}"),
                &[],
                &format!(
                    "MOV D{alias} D20\nMOV D{} D21{}",
                    alias + 1,
                    if wide {
                        format!("\nMOV D{} D22", alias + 2)
                    } else {
                        String::new()
                    }
                ),
            ));
        }
    }
    assert_eq!(cases.len(), 50);

    for op in ["MVM", "MVMP", "DMVM", "DMVMP"] {
        let wide = op.starts_with('D');
        for dest in ["E1", "F1"] {
            let (seed, body) = if wide {
                (
                    "DMOV H12345678 E1\nMOV HABCD D1184\nDMOV H89ABCDEF D1187\nDMOV H0F0FF0F0 D1191".to_owned(),
                    format!("{op} F3 F5 {dest}\nDMOV H76543210 D1187"),
                )
            } else {
                (
                    "DMOV H12345678 E1\nMOV HABCD D1184\nMOV HAA55 E3\nMOV H0FF0 E5".to_owned(),
                    format!("{op} E3 E5 {dest}\nMOV H55AA E3"),
                )
            };
            cases.push(scenario(
                &format!("lifecycle_{op}_index_target_{dest}"), &seed, &body, &[],
                "MOV E1 D20\nMOV F1 D21\nMOV D1184 D22\nMOV E3 D23\nMOV F3 D24\nMOV D1188 D25\nMOV E5 D26\nMOV F5 D27\nMOV D1192 D28",
            ));
        }
    }
    for op in ["RAND", "RANDP", "DRAND", "DRANDP"] {
        let wide = op.starts_with('D');
        let mov = if wide { "DMOV" } else { "MOV" };
        let initial = if wide { 70000 } else { 300 };
        for dest in ["E1", "F1"] {
            cases.push(scenario(
                &format!("lifecycle_{op}_F_bounds_target_{dest}"),
                &format!("DMOV H12345678 E1\nMOV HABCD D1184\n{mov} K{initial} D1187"),
                &format!("{op} F3 F3 {dest}\n{mov} K{} D1187", initial + 1),
                &[],
                "MOV E1 D20\nMOV F1 D21\nMOV D1184 D22\nMOV F3 D23\nMOV D1188 D24",
            ));
        }
    }
    assert_eq!(cases.len(), 66);

    cases.push(scenario(
        "swap_word_short_groups_truncate_after_byte_exchange",
        &format!(
            "{}\n{}\n{}",
            seed_bits("M", 128, 16, 0xa55f),
            seed_bits("S", 128, 16, 0xa5ef),
            seed_bits("Y", 128, 16, 0xabcd)
        ),
        "SWAP K1M128\nSWAP K2S128\nSWAP K3Y200",
        &[
            ("M127", true),
            ("M144", false),
            ("S127", true),
            ("S144", false),
            ("Y177", true),
            ("Y220", false),
        ],
        "MOV K4M128 D20\nMOV K4S128 D21\nMOV K4Y200 D22",
    ));
    cases.push(scenario(
        "swap_double_short_groups_preserve_unselected_neighbor_bits",
        &format!(
            "{}\n{}\n{}",
            seed_bits("M", 128, 32, 0xa55aa55f),
            seed_bits("S", 128, 32, 0x12340abc),
            seed_bits("Y", 128, 32, 0x1234abcd)
        ),
        "DSWAP K1M128\nDSWAP K3S128\nDSWAP K5Y200",
        &[
            ("M127", true),
            ("M160", false),
            ("S127", true),
            ("S160", false),
            ("Y177", true),
            ("Y240", false),
        ],
        "DMOV K8M128 D20\nDMOV K8S128 D22\nDMOV K8Y200 D24",
    ));
    cases.push(scenario(
        "mask_zero_all_bits_and_short_group_mask",
        &format!("MOV HAA55 E1\nMOV H1234 F1\nMOV H1234 F2\nMOV H1234 F3\nDMOV H1234ABCD D1189\nDMOV H87654321 D1191\nDMOV H23456789 D1193\n{}", seed_bits("M", 64, 4, 5)),
        "MVM E1 K0 F1\nMVM E1 HFFFF F2\nMVM E1 K1M64 F3\nDMVM F4 K0 F5\nDMVM F4 HFFFFFFFF F6",
        &[("M63", true), ("M68", false)],
        "MOV F1 D20\nMOV F2 D21\nMOV F3 D22\nDMOV D1189 D24\nDMOV D1191 D26\nDMOV D1193 D28",
    ));
    cases.push(scenario(
        "mask_direct_index_source_mask_and_destination_overlap_snapshot",
        "MOV H55AA D1182\nMOV H0FF0 D1183\nMOV HAA55 D1184\nMOV HAA55 E3\nMOV H0F0F F3",
        "DMVM E1 F1 F1\nMVM E3 F3 F3",
        &[],
        "MOV D1182 D20\nMOV D1183 D21\nMOV D1184 D22\nMOV E3 D23\nMOV F3 D24",
    ));
    cases.push(scenario(
        "mask_shifted_packed_source_mask_target_snapshot",
        &format!(
            "{}\n{}",
            seed_bits("M", 128, 32, 0x1234abcd),
            seed_bits("S", 128, 32, 0x89abcdef)
        ),
        "MVM K4M128 K3M132 K2M136\nDMVM K8S128 K5S132 K3S136",
        &[
            ("M127", true),
            ("M160", false),
            ("S127", true),
            ("S160", false),
        ],
        "DMOV K8M128 D20\nDMOV K8S128 D22",
    ));
    cases.push(scenario(
        "wide_F0_mask_source_and_destination_use_D1030",
        "MOV H5AA5 E0\nDMOV H12345678 D1029\nDMOV HFF00FF00 D1183\nDMOV H87654321 D1185",
        "DMVM F0 F1 F2\nDMVM F2 HFFFFFFFF F0",
        &[],
        "MOV E0 D20\nMOV F0 D21\nMOV D1030 D22\nDMOV D1183 D24\nDMOV D1185 D26",
    ));
    cases.push(scenario(
        "wide_F7_mask_source_and_destination_use_D1196",
        "DMOV H12345678 D1195\nDMOV HFF00FF00 D1193\nDMOV H87654321 D1191\nMOV H5AA5 D1197",
        "DMVM F7 F6 F5\nDMVMP F5 HFFFFFFFF F7",
        &[],
        "DMOV D1191 D20\nDMOV D1193 D22\nDMOV D1195 D24\nMOV D1197 D26",
    ));
    cases.push(scenario(
        "random_equal_F0_F7_aliases_and_word_E0_target",
        "DMOV K70000 D1029\nDMOV H12345678 D1195\nMOV H5AA5 D1197",
        "DRAND F0 F0 F7\nDRAND F7 F7 F0\nRAND F0 F0 E0\nDMOV K70001 D1029",
        &[],
        "MOV E0 D20\nMOV F0 D21\nMOV D1030 D22\nDMOV D1195 D24\nMOV D1197 D26",
    ));
    cases.push(scenario(
        "random_equal_bounds_and_overlapping_index_destination",
        "MOV K300 E1\nMOV K300 F1\nDMOV K70000 D1185\nMOV H5AA5 D1187",
        "RAND E1 F1 E1\nDRAND F2 F2 E2\nMOV K301 E1",
        &[],
        "MOV E1 D20\nMOV F1 D21\nMOV E2 D22\nMOV F2 D23\nMOV D1186 D24\nMOV D1187 D25",
    ));
    cases.push(scenario(
        "random_word_negative_lower_holds_then_equal_recovery",
        "MOV K-1 E1\nMOV K7 F1\nMOV K77 F2\nMOV K78 F3",
        "RAND E1 F1 F2\nRANDP E1 F1 F3\nMOV K7 E1",
        &[],
        "MOV E1 D20\nMOV F1 D21\nMOV F2 D22\nMOV F3 D23",
    ));
    cases.push(scenario(
        "random_word_negative_upper_holds_then_equal_recovery",
        "MOV K7 E1\nMOV K-1 F1\nMOV K77 F2\nMOV K78 F3",
        "RAND E1 F1 F2\nRANDP E1 F1 F3\nMOV K7 F1",
        &[],
        "MOV E1 D20\nMOV F1 D21\nMOV F2 D22\nMOV F3 D23",
    ));
    cases.push(scenario(
        "random_double_negative_F_lower_holds_then_equal_recovery",
        "DMOV K-1 D1183\nDMOV K7 D1187\nDMOV K77 D1191\nDMOV K78 D1193",
        "DRAND F1 F3 F5\nDRANDP F1 F3 F6\nDMOV K7 D1183",
        &[],
        "DMOV D1183 D20\nDMOV D1187 D22\nDMOV D1191 D24\nDMOV D1193 D26",
    ));
    cases.push(scenario(
        "random_double_negative_F_high_half_holds_then_equal_recovery",
        "DMOV K7 D1183\nDMOV H80000001 D1187\nDMOV K77 D1191\nDMOV K78 D1193",
        "DRAND F1 F3 F5\nDRANDP F1 F3 F6\nDMOV K7 D1187",
        &[],
        "DMOV D1183 D20\nDMOV D1187 D22\nDMOV D1191 D24\nDMOV D1193 D26",
    ));
    cases.push(scenario(
        "random_reversed_bounds_hold_then_become_equal",
        "MOV K9 E1\nMOV K3 F1\nMOV K77 F2\nDMOV K90000 D1187\nDMOV K30000 D1191\nDMOV K78 D1195",
        "RAND E1 F1 F2\nDRAND F3 F5 F7\nMOV K9 F1\nDMOV K90000 D1191",
        &[],
        "MOV E1 D20\nMOV F1 D21\nMOV F2 D22\nDMOV D1187 D24\nDMOV D1191 D26\nDMOV D1195 D28",
    ));
    cases.push(scenario(
        "random_short_bounds_zero_extend_before_signed_domain_check",
        &format!(
            "MOV K77 F1\nMOV K78 F2\nDMOV K79 D1187\nDMOV K80 D1191\n{}\n{}",
            seed_bits("M", 64, 16, 0xffff),
            seed_bits("S", 64, 32, u32::MAX)
        ),
        "RAND K1M64 K1M64 F1\nRAND K4M64 K4M64 F2\nDRAND K7S64 K7S64 F3\nDRAND K8S64 K8S64 F5",
        &[("M63", true), ("M80", false), ("S63", true), ("S96", false)],
        "MOV F1 D20\nMOV F2 D21\nDMOV D1187 D22\nDMOV D1191 D24",
    ));
    let mut inputs = scenario(
        "KnX_source_mask_and_equal_random_bounds_to_F_targets",
        "MOV H5AA5 F1\nDMOV H12345678 D1187\nMOV K77 F5\nDMOV K78 D1195",
        "MVM K4X20 K4X20 F1\nDMVM K4X20 K4X20 F3\nRAND K4X20 K4X20 F5\nDRAND K4X20 K4X20 F7",
        &[],
        "MOV F1 D20\nDMOV D1187 D22\nMOV F5 D24\nDMOV D1195 D26",
    );
    for (i, name) in [
        "X20", "X21", "X22", "X23", "X24", "X25", "X26", "X27", "X30", "X31", "X32", "X33", "X34",
        "X35", "X36", "X37",
    ]
    .into_iter()
    .enumerate()
    {
        inputs.stages[0].0.push((name, 0x1234u16 & (1 << i) != 0));
    }
    inputs.stages[2].0.push(("X20", true));
    cases.push(inputs);

    assert_eq!(cases.len(), 82);
    let names: BTreeSet<_> = cases.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names.len(), cases.len());
    let actual: BTreeSet<_> = cases
        .iter()
        .flat_map(|c| c.source.lines())
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    for name in FORMS {
        assert!(
            actual.contains(name),
            "missing additional-processing form {name}"
        );
    }
    cases
}
