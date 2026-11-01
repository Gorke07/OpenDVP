//! Role-specific operands for decode/encode, count, bit tests, mean and root.
//! SQR remains D/K/H-only. DECO/ENCO do not acquire Kn operands merely because
//! the other families accept them. Endpoint diagnostics are kept by the caller.
use super::{Case, case};
use std::collections::BTreeSet;

const FORMS: [&str; 20] = [
    "DECO", "DECOP", "ENCO", "ENCOP", "SUM", "SUMP", "DSUM", "DSUMP", "BON", "BONP", "DBON",
    "DBONP", "MEAN", "MEANP", "DMEAN", "DMEANP", "SQR", "SQRP", "DSQR", "DSQRP",
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

fn seed_elements(area: &str, base: u16, digits: u16, values: &[u32]) -> String {
    values
        .iter()
        .enumerate()
        .map(|(i, value)| seed_bits(area, base + i as u16 * digits * 4, digits * 4, *value))
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
        &format!("data_operand_{name}"),
        &format!(
            "LD X1\nMOV H55AA D1028\nMOV H1234 D1029\n{seed}\n{guard_seed}\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\nLD X0\n{body}\nLD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24\n{guard_reads}\nLD X0\nORI X0\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305\n{observe}\nEND"
        ),
        stages(),
    )
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();

    // Decode/encode have raw-bit sources, not Kn sources. E/F/D destinations
    // distinguish actual aliases from ordinary word storage and preserve the
    // neighboring E/F half. Each input changes after the first enabled visit.
    for op in ["DECO", "DECOP", "ENCO", "ENCOP"] {
        let decode = op.starts_with("DECO");
        for (area, dest) in [("Y", "E1"), ("M", "F1"), ("S", "D20")] {
            let source = address(area, 64);
            let before = address(area, 63);
            let after = address(area, if decode { 67 } else { 72 });
            let changed = address(area, if decode { 66 } else { 70 });
            cases.push(scenario(
                &format!("lifecycle_{op}_{source}_to_{dest}"),
                &format!(
                    "DMOV H12345678 E1\nMOV H5AA5 D20\n{}",
                    seed_bits(
                        area,
                        64,
                        if decode { 3 } else { 8 },
                        if decode { 3 } else { 0x12 }
                    )
                ),
                &format!("{op} {source} {dest} K3\nSET {changed}"),
                &[(before.as_str(), true), (after.as_str(), false)],
                "MOV E1 D30\nMOV F1 D31\nMOV D20 D32\nDMOV D1182 D34",
            ));
        }
    }

    for op in ["SUM", "SUMP", "DSUM", "DSUMP"] {
        let wide = op.starts_with('D');
        let bits = if wide { 32 } else { 16 };
        for area in ["Y", "M", "S"] {
            let source = address(area, 64);
            let before = address(area, 63);
            let after = address(area, 64 + bits);
            cases.push(scenario(
                &format!("lifecycle_{op}_K{}{source}", bits / 4),
                &format!(
                    "DMOV H12345678 E1\n{}",
                    seed_bits(area, 64, bits, if wide { 0x87654321 } else { 0x8123 })
                ),
                &format!(
                    "{op} K{}{source} E1\n{} K7 K{}{source}",
                    bits / 4,
                    if wide { "DMOV" } else { "MOV" },
                    bits / 4
                ),
                &[(before.as_str(), true), (after.as_str(), false)],
                "MOV E1 D20\nMOV F1 D21\nDMOV D1182 D22",
            ));
        }
    }

    for op in ["BON", "BONP", "DBON", "DBONP"] {
        let wide = op.starts_with('D');
        let bits = if wide { 32 } else { 16 };
        for area in ["Y", "M", "S"] {
            let source = address(area, 64);
            let dest = address(area, 128);
            let before = address(area, 127);
            let after = address(area, 129);
            cases.push(scenario(
                &format!("lifecycle_{op}_{dest}_index_E1"),
                &format!(
                    "DMOV K1 E1\nSET {dest}\n{}",
                    seed_bits(area, 64, bits, if wide { 0x80000005 } else { 0x8005 })
                ),
                &format!(
                    "{op} K{}{source} {dest} E1\n{} K2 E1",
                    bits / 4,
                    if wide { "DMOV" } else { "MOV" }
                ),
                &[(before.as_str(), true), (after.as_str(), false)],
                "MOV E1 D20\nMOV F1 D21\nDMOV D1182 D22",
            ));
        }
    }

    for op in ["MEAN", "MEANP", "DMEAN", "DMEANP"] {
        for area in ["Y", "M", "S"] {
            let source = address(area, 64);
            let dest = address(area, 128);
            let before = address(area, 127);
            let after = address(area, 132);
            let source_after = address(area, 88);
            cases.push(scenario(
                &format!("lifecycle_{op}_K2{source}_K1{dest}"),
                &format!(
                    "DMOV K3 E1\n{}\n{}",
                    seed_elements(area, 64, 2, &[11, 22, 34]),
                    seed_bits(area, 128, 12, 0xa5b)
                ),
                &format!("{op} K2{source} K1{dest} E1\nMOV K20 K2{source}"),
                &[
                    (before.as_str(), true),
                    (after.as_str(), false),
                    (source_after.as_str(), true),
                ],
                &format!("MOV K3{dest} D20\nMOV E1 D21\nMOV F1 D22"),
            ));
        }
    }

    // The PDF deliberately gives SQR a narrower D/K/H matrix. Keep these four
    // regression cases as D operands rather than inventing Kn/E/F support.
    for op in ["SQR", "SQRP", "DSQR", "DSQRP"] {
        let mov = if op.starts_with('D') { "DMOV" } else { "MOV" };
        cases.push(scenario(
            &format!("lifecycle_{op}_documented_D_operands"),
            &format!("{mov} K144 D0\nDMOV H12345678 D20\nMOV K77 D22"),
            &format!("{op} D0 D20\n{mov} K169 D0"),
            &[],
            "MOV D20 D30\nMOV D21 D31\nMOV D22 D32",
        ));
    }
    assert_eq!(cases.len(), 52);

    cases.push(scenario(
        "decode_E_F_in_place_snapshot",
        "MOV K13 E1\nMOV HABCD F1\nDMOV H3456789A D182",
        "DECO E1 E1 K4\nDECO F1 F1 K3",
        &[],
        "MOV E1 D20\nMOV F1 D21\nDMOV D1182 D22\nDMOV D182 D24",
    ));
    cases.push(scenario(
        "encode_E_F_highest_mask_and_zero_recovery",
        "MOV HFF08 E1\nMOV HFF00 F1\nMOV K41 E2\nMOV K42 F2",
        "ENCO E1 E2 K3\nENCOP F1 F2 K3\nMOV H0080 F1",
        &[],
        "MOV E1 D20\nMOV F1 D21\nMOV E2 D22\nMOV F2 D23\nDMOV D1182 D24\nDMOV D1184 D26",
    ));
    cases.push(scenario(
        "decode_raw_source_target_alias_then_encode",
        &seed_bits("M", 128, 8, 0x03),
        "DECO M128 M128 K3\nENCO M128 E1 K3",
        &[("M127", true), ("M136", false)],
        "MOV K2M128 D20\nMOV E1 D21\nMOV F1 D22",
    ));
    cases.push(scenario(
        "sum_short_groups_signed_full_width_and_index_aliases",
        &format!(
            "MOV H8001 E3\nMOV K77 F3\nDMOV H80000001 E4\n{}\n{}",
            seed_bits("M", 128, 16, 0xffff),
            seed_bits("M", 192, 32, u32::MAX)
        ),
        "SUM K1M128 F1\nDSUM K8M192 E2\nSUM E3 F3\nDSUM E4 E4",
        &[
            ("M127", true),
            ("M144", false),
            ("M191", true),
            ("M224", false),
        ],
        "MOV F1 D20\nDMOV E2 D22\nMOV E3 D24\nMOV F3 D25\nDMOV E4 D26\nDMOV D1188 D28",
    ));
    cases.push(scenario(
        "sum_zero_flag_is_sticky_without_clearing_other_flags",
        "DMOV H12345678 E1\nDMOV H23456789 E2\nDMOV H3456789A E3",
        "RST M1020\nSUM K0 E1\nLD M1020\nOUT Y0\nLD X0\nSUM K1 F1\nLD M1020\nOUT Y1\nLD X0\nRST M1020\nDSUM H80000000 E2\nLD M1020\nOUT Y2\nLD X0\nDSUM K0 E3\nLD M1020\nOUT Y3",
        &[],
        "MOV E1 D20\nMOV F1 D21\nDMOV E2 D22\nDMOV E3 D24",
    ));
    cases.push(scenario(
        "bon_word_invalid_E_index_holds_and_recovers",
        &format!("MOV K16 E1\nSET Y0\nSET Y1\n{}", seed_bits("M", 64, 16, 1)),
        "BON K4M64 Y0 E1\nBONP K4M64 Y1 E1\nMOV K15 E1",
        &[("M63", true), ("M80", false)],
        "MOV E1 D20\nMOV F1 D21",
    ));
    cases.push(scenario(
        "bon_double_invalid_E_index_holds_and_recovers",
        &format!("DMOV K32 E1\nSET Y0\nSET Y1\n{}", seed_bits("M", 64, 32, 1)),
        "DBON K8M64 Y0 E1\nDBONP K8M64 Y1 E1\nDMOV K31 E1",
        &[("M63", true), ("M96", false)],
        "DMOV E1 D20\nDMOV D1182 D22",
    ));

    // BON's dynamic n is range-checked, unlike BLD's masked n. A wide F
    // operand reads its actual next D word; setting it after the first visit
    // must cause E1A and retain the output even though the source becomes zero.
    for (index, high) in [("F0", 1030), ("F1", 1184), ("F7", 1196)] {
        cases.push(scenario(
            &format!("bon_double_index_{index}_uses_high_word_for_range_check"),
            &format!(
                "MOV K31 {index}\nMOV K0 D{high}\nRST Y0\nRST Y1\n{}",
                seed_bits("M", 64, 32, 0x80000000)
            ),
            &format!(
                "DBON K8M64 Y0 {index}\nDBONP K8M64 Y1 {index}\nMOV K1 D{high}\nDMOV K0 K8M64"
            ),
            &[("M63", true), ("M96", false)],
            &format!("MOV {index} D20\nMOV D{high} D21\nDMOV K8M64 D22"),
        ));
    }

    cases.push(scenario(
        "bon_E_F_sources_and_packed_destination_alias",
        &format!(
            "DMOV H80018000 E1\nDMOV H000F0000 E2\nDMOV H80000001 E3\nDMOV K31 E4\n{}\n{}",
            seed_bits("M", 128, 16, 2),
            seed_bits("M", 192, 32, 2)
        ),
        "BON E1 Y0 F2\nBON F1 Y1 E2\nDBON E3 Y2 E4\nBON K4M128 M128 K1\nDBON K8M192 M192 K1",
        &[
            ("M127", true),
            ("M144", false),
            ("M191", true),
            ("M224", false),
        ],
        "MOV K4M128 D20\nDMOV K8M192 D22\nDMOV E1 D24\nDMOV E3 D26\nDMOV E4 D28",
    ));
    cases.push(scenario(
        "mean_word_declared_group_stride_and_short_target_truncation",
        &format!("{}\n{}\n{}\n{}", seed_elements("M", 64, 1, &[1, 2, 9, 15]), seed_elements("S", 64, 3, &[4095, 2048, 3]), seed_bits("M", 128, 16, 0xa55a), seed_bits("M", 192, 16, 0x5aa5)),
        "MEAN K1M64 K2M128 K4\nMEAN K3S64 K1M192 K3",
        &[("M63", true), ("M80", false), ("S63", true), ("S100", false)],
        "MOV K4M128 D20\nMOV K4M192 D21\nMOV K4M64 D22\nMOV K3S64 D23\nMOV K3S76 D24\nMOV K3S88 D25",
    ));
    cases.push(scenario(
        "mean_word_count_and_destination_same_group_snapshot",
        &format!(
            "{}\n{}",
            seed_elements("S", 64, 2, &[2, 4, 6, 8, 10, 12]),
            seed_bits("M", 128, 8, 0xa3)
        ),
        "MEAN K2S64 K1M128 K1M128",
        &[
            ("S63", true),
            ("S112", false),
            ("M127", true),
            ("M136", false),
        ],
        "MOV K2M128 D20",
    ));
    cases.push(scenario(
        "mean_double_count_and_destination_same_short_group_snapshot",
        &format!(
            "{}\n{}",
            seed_elements("S", 64, 2, &[2, 4, 6, 8, 10, 12]),
            seed_bits("M", 128, 8, 0xa3)
        ),
        "DMEAN K2S64 K1M128 K1M128",
        &[
            ("S63", true),
            ("S112", false),
            ("M127", true),
            ("M136", false),
        ],
        "MOV K2M128 D20",
    ));
    cases.push(scenario(
        "mean_double_full_signed_groups_and_mixed_remainders",
        &format!(
            "{}\n{}\n{}",
            seed_elements("M", 64, 8, &[(-5i32) as u32, 2, 1]),
            seed_elements("S", 64, 3, &[4095, 2048, 3]),
            seed_bits("M", 256, 32, 0xa55aa55a)
        ),
        "DMEAN K8M64 E1 K3\nDMEAN K3S64 K4M256 K3",
        &[
            ("M63", true),
            ("M160", false),
            ("S63", true),
            ("S100", false),
            ("M288", true),
        ],
        "DMOV E1 D20\nDMOV D1182 D22\nDMOV K8M256 D24",
    ));
    cases.push(scenario(
        "mean_word_F_count_invalid_then_pulse_recovery",
        &format!(
            "MOV K0 F1\n{}\n{}\n{}",
            seed_elements("M", 64, 1, &[3, 6, 9]),
            seed_bits("M", 128, 8, 0xa5),
            seed_bits("M", 160, 8, 0x5a)
        ),
        "MEAN K1M64 K1M128 F1\nMEANP K1M64 K1M160 F1\nMOV K3 F1",
        &[
            ("M63", true),
            ("M76", false),
            ("M136", true),
            ("M168", false),
        ],
        "MOV K2M128 D20\nMOV K2M160 D21\nMOV F1 D22",
    ));
    cases.push(scenario(
        "mean_double_F_count_invalid_high_word_then_recovery",
        &format!(
            "MOV K3 F7\nMOV K1 D1196\n{}\n{}\n{}",
            seed_elements("M", 64, 2, &[10, 20, 30]),
            seed_bits("M", 128, 8, 0xa5),
            seed_bits("M", 160, 8, 0x5a)
        ),
        "DMEAN K2M64 K1M128 F7\nDMEANP K2M64 K1M160 F7\nMOV K0 D1196",
        &[
            ("M63", true),
            ("M88", false),
            ("M136", true),
            ("M168", false),
        ],
        "MOV K2M128 D20\nMOV K2M160 D21\nMOV F7 D22\nMOV D1196 D23",
    ));
    cases.push(scenario(
        "root_documented_operands_zero_fraction_negative_and_maximum",
        "MOV K0 D0\nMOV K-1 D2\nMOV K15 D4\nDMOV K2147483647 D6\nDMOV K2147395600 D8\nMOV K77 D20\nMOV K78 D21\nMOV K79 D22\nDMOV K80 D24\nDMOV K81 D26",
        "SQR D0 D20\nLD M1020\nOUT Y0\nLD X0\nSQR D2 D21\nLD M1020\nOUT Y1\nLD M1021\nOUT Y2\nLD X0\nSQR D4 D22\nLD M1021\nOUT Y3\nLD X0\nDSQR D6 D24\nLD M1021\nOUT Y4\nLD X0\nDSQR D8 D26\nLD M1021\nOUT Y5",
        &[], "",
    ));
    cases.push(scenario(
        "decode_encode_E0_F0_aliases_and_later_zero_source",
        "MOV K3 E0\nMOV H0080 F0\nMOV K41 E1\nMOV K42 F1",
        "DECO E0 E1 K3\nENCO F0 F1 K3\nMOV K5 E0\nMOV H0100 F0",
        &[],
        "MOV E1 D20\nMOV F1 D21\nDMOV D1182 D22",
    ));
    cases.push(scenario(
        "mean_E_F_destinations_preserve_or_write_the_documented_pair",
        &format!(
            "DMOV H12345678 E1\nDMOV H23456789 E2\n{}\n{}",
            seed_elements("M", 64, 1, &[11, 12, 13]),
            seed_elements("M", 96, 2, &[100, 200, 240])
        ),
        "MEAN K1M64 F1 K3\nDMEAN K2M96 E2 K3",
        &[
            ("M63", true),
            ("M76", false),
            ("M95", true),
            ("M120", false),
        ],
        "MOV E1 D20\nMOV F1 D21\nDMOV E2 D22\nDMOV D1182 D24\nDMOV D1184 D26",
    ));
    cases.push(scenario(
        "decode_encode_full_256_bit_span_from_E_source",
        "MOV K255 E1\nMOV K41 F1\nSET S768\nSET S895\nSET S1023",
        "DECO E1 S768 K8\nENCO S768 F1 K8\nMOV K0 E1\nLD S768\nOUT Y0\nLD S1023\nOUT Y1",
        &[("S767", true)],
        "MOV E1 D20\nMOV F1 D21",
    ));

    let mut inputs = scenario(
        "KnX_sources_for_sum_bon_and_group_mean",
        &format!(
            "DMOV K2 E3\n{}\n{}",
            seed_bits("M", 128, 8, 0xa5),
            seed_bits("M", 160, 8, 0x5a)
        ),
        "SUM K4X20 E1\nDSUM K4X20 E2\nBON K4X20 Y0 E3\nDBON K4X20 Y1 E3\nMEAN K1X20 K1M128 K4\nDMEAN K1X20 K1M160 K4",
        &[
            ("M127", true),
            ("M136", false),
            ("M159", true),
            ("M168", false),
        ],
        "MOV E1 D20\nMOV F1 D21\nDMOV E2 D22\nMOV K2M128 D24\nMOV K2M160 D25",
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

    assert_eq!(cases.len(), 74);
    let names: BTreeSet<_> = cases.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names.len(), cases.len());
    let actual: BTreeSet<_> = cases
        .iter()
        .flat_map(|c| c.source.lines())
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    for name in FORMS {
        assert!(actual.contains(name), "missing data-processing form {name}");
    }
    cases
}
