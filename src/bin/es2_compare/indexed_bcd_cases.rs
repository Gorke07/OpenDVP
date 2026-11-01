//! Bounded indexed BCD/BIN conversion operands and source-validation order.
//! Includes canonical direct E-wide writer probes, separately from the
//! truncated-multiply direct E storage divergence. No direct wide F.
use super::{Case, case};
use std::collections::BTreeSet;

const FORMS: [(&str, bool); 8] = [
    ("BCD", false),
    ("BCDP", false),
    ("BIN", false),
    ("BINP", false),
    ("DBCD", true),
    ("DBCDP", true),
    ("DBIN", true),
    ("DBINP", true),
];

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (vec![("X1", true), ("X0", false), ("X7", false)], 1),
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

fn index_base(index: u16) -> u16 {
    if index == 0 { 1028 } else { 1180 + index * 2 }
}

fn seed_words(area: &str, base: u16, count: u16, salt: u16) -> String {
    (0..count)
        .map(|i| {
            format!(
                "MOV H{:04X} {area}{}",
                salt.wrapping_add(i.wrapping_mul(0x1111)),
                base + i
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn read_words(area: &str, base: u16, count: u16, output: u16) -> String {
    (0..count)
        .map(|i| format!("MOV {area}{} D{}", base + i, output + i))
        .collect::<Vec<_>>()
        .join("\n")
}

fn seed_bits(area: &str, base: u16, count: u16, pattern: u64) -> String {
    assert!(matches!(area, "Y" | "M" | "S"));
    (0..count)
        .map(|i| {
            format!(
                "{} {}",
                if pattern & (1 << (i % 64)) == 0 {
                    "RST"
                } else {
                    "SET"
                },
                address(area, base + i)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn read_bits(area: &str, base: u16, count: u16, output: u16) -> String {
    assert_eq!(count % 4, 0);
    (0..count.div_ceil(16))
        .map(|word| {
            let bits = (count - word * 16).min(16);
            format!(
                "MOV K{}{} D{}",
                bits / 4,
                address(area, base + word * 16),
                output + word
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn scenario(name: &str, seed: &str, body: &str, observe: &str) -> Case {
    let gate = if body.starts_with("LD") { "" } else { "LD X0" };
    let source = [
        "LD X1\nMOV H55AA E0\nMOV H1234 F0",
        "SET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068",
        seed,
        gate,
        body,
        "LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24",
        "LDI X7\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305\nMOV K1234 D306",
        observe,
        "END",
    ]
    .into_iter()
    .filter(|section| !section.is_empty())
    .collect::<Vec<_>>()
    .join("\n");
    assert!(
        source.lines().count() <= 512,
        "oversized indexed BCD/BIN {name}"
    );
    assert!(
        source.lines().any(|line| {
            let words: Vec<_> = line.split_whitespace().collect();
            words.len() == 3
                && FORMS.iter().any(|(op, _)| *op == words[0])
                && words[1..].iter().any(|operand| {
                    operand.len() > 2
                        && matches!(operand.as_bytes()[operand.len() - 2], b'E' | b'F')
                        && operand.as_bytes()[operand.len() - 1].is_ascii_digit()
                })
        }),
        "missing indexed BCD/BIN action in {name}"
    );
    case(&format!("indexed_bcd_{name}"), &source, stages())
}

fn seed_conversion_words(area: &str, base: u16, count: u16) -> String {
    // Every adjacent word pair is valid binary input below 99,999,999, and
    // every nibble is also valid packed BCD. Odd wide offsets remain useful.
    (0..count)
        .map(|i| {
            format!(
                "MOV H{:04X} {area}{}",
                [0x0123, 0x0234, 0x0345, 0x0456][i as usize % 4],
                base + i
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn literals(op: &str, wide: bool) -> (&'static str, &'static str, &'static str, &'static str) {
    match (op.contains("BCD"), wide) {
        (true, false) => ("K1234", "K4321", "K9999", "K10000"),
        (true, true) => ("K12345678", "K87654321", "K99999999", "K100000000"),
        (false, false) => ("H1234", "H4321", "H9999", "H9A99"),
        (false, true) => ("H12345678", "H87654321", "H99999999", "H9A999999"),
    }
}

pub(super) fn cases() -> Vec<Case> {
    let mut cases = Vec::new();

    for bank in ['E', 'F'] {
        for index in 0..8u16 {
            let modifier = format!("{bank}{index}");
            let pair = index_base(index);
            let alias = pair + u16::from(bank == 'F');
            let companion = pair + u16::from(bank == 'E');
            let delta = if index % 2 == 0 { -2 } else { 1 };
            for (family, wide) in [
                ("BCD", false),
                ("BIN", false),
                ("DBCD", true),
                ("DBIN", true),
            ] {
                let pulse = (index + u16::from(bank == 'F')) % 2 == 1;
                let op = format!("{family}{}", if pulse { "P" } else { "" });
                let poison = if bank == 'F' && (1..7).contains(&index) {
                    format!("MOV H6A5A D{}", alias + 1)
                } else {
                    String::new()
                };
                let seed = format!(
                    "{}\n{}\nMOV H5AA5 D{companion}\n{poison}\nMOV K{delta} D{alias}",
                    seed_conversion_words("D", 61, 13),
                    seed_words("D", 155, 16, 0xa55a)
                );
                let body = format!(
                    "{op} D64{modifier} D160{modifier}\nMOV K{} {modifier}",
                    delta + 1
                );
                let observe = format!(
                    "{}\n{}\nMOV E{index} D260\nMOV F{index} D261\nMOV D{pair} D262\nMOV D{} D263",
                    read_words("D", 61, 13, 200),
                    read_words("D", 155, 16, 220),
                    pair + 1
                );
                cases.push(scenario(
                    &format!(
                        "all_indices_{op}_{modifier}_{}",
                        if wide { "double" } else { "word" }
                    ),
                    &seed,
                    &body,
                    &observe,
                ));
            }
        }
    }
    assert_eq!(cases.len(), 64);

    const INPUTS: [&str; 40] = [
        "X100", "X101", "X102", "X103", "X104", "X105", "X106", "X107", "X110", "X111", "X112",
        "X113", "X114", "X115", "X116", "X117", "X120", "X121", "X122", "X123", "X124", "X125",
        "X126", "X127", "X130", "X131", "X132", "X133", "X134", "X135", "X136", "X137", "X140",
        "X141", "X142", "X143", "X144", "X145", "X146", "X147",
    ];
    // Source offsets advance by a nibble, while target offsets advance by
    // one bit. Every declared width occurs; full sources remain valid before
    // conversion and short sources zero-extend rather than inheriting sign.
    for (ordinal, (op, wide)) in FORMS.into_iter().enumerate() {
        let maximum = if wide { 8 } else { 4 };
        let pattern = if wide { 0x0123_4567u64 } else { 0x1234 };
        for (area_index, target_area) in ["Y", "M", "S"].into_iter().enumerate() {
            let source_area = ["X", "Y", "M", "S"][(ordinal + area_index) % 4];
            let source_digits = (ordinal * 3 + area_index) as u16 % maximum + 1;
            let target_digits = (ordinal * 3 + area_index + 2) as u16 % maximum + 1;
            let source_seed = if source_area == "X" {
                String::new()
            } else {
                seed_bits(source_area, 62, 44, (pattern << 2) | 1)
            };
            let seed = format!(
                "{source_seed}\n{}\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K-2 F2\nMOV H2468 E3",
                seed_bits(target_area, 154, 48, 0xa55a_3cc3_9696)
            );
            let body = format!(
                "{op} K{source_digits}{}E1 K{target_digits}{}F2\nMOV K4 E1\nMOV K-1 F2",
                address(source_area, 64),
                address(target_area, 160)
            );
            let observe = format!(
                "{}\n{}\nMOV E1 D250\nMOV F1 D251\nMOV E2 D252\nMOV F2 D253\nMOV E3 D254",
                read_bits(source_area, 62, 48, 200),
                read_bits(target_area, 154, 48, 220)
            );
            let mut item = scenario(
                &format!("groups_{op}_K{source_digits}{source_area}_K{target_digits}{target_area}"),
                &seed,
                &body,
                &observe,
            );
            if source_area == "X" {
                item.stages[0].0.extend(
                    INPUTS
                        .iter()
                        .enumerate()
                        .map(|(bit, input)| (*input, pattern & (1 << bit) != 0)),
                );
            }
            cases.push(item);
        }
    }
    assert_eq!(cases.len(), 88);

    // Validate the complete source before truncating into a single nibble.
    // A later valid maximum succeeds while earlier error state stays latched.
    for (ordinal, (op, wide)) in FORMS.into_iter().enumerate() {
        let mov = if wide { "DMOV" } else { "MOV" };
        let (_, _, maximum, invalid) = literals(op, wide);
        let area = ["Y", "M", "S"][ordinal % 3];
        let seed = format!(
            "{mov} {invalid} D32\n{}\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\n{} M1067\n{} M1068\nMOV H4444 D1067\nMOV K321 D1068",
            seed_bits(area, 126, 20, 0x5a5a5),
            if ordinal & 1 == 0 { "RST" } else { "SET" },
            if ordinal & 2 == 0 { "RST" } else { "SET" }
        );
        let body = format!("{op} D32E1 K1{}F2\n{mov} {maximum} D32", address(area, 128));
        let observe = format!(
            "{}\n{mov} D32 D240\nMOV E1 D250\nMOV F2 D251",
            read_bits(area, 126, 20, 220)
        );
        cases.push(scenario(
            &format!(
                "invalid_full_source_before_short_target_{op}_sticky_{}",
                ordinal % 4
            ),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 96);

    for (op, wide) in FORMS {
        let (_, alternate, maximum, _) = literals(op, wide);
        let word_max = if op.contains("BCD") { "K9999" } else { "H9999" };
        if wide {
            let (value, _, _, _) = literals(op, true);
            let seed = format!(
                "MOV H1357 C198\nMOV {word_max} C199\nDMOV {value} C200\nDMOV H55667788 C201\nDMOV H13572468 C253\nDMOV H23456789 C254\nDMOV H3456789A C255\n{}\n{}\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K-1 E3\nMOV H1234 F3\nMOV K-1 E4\nMOV H5678 F4",
                seed_words("T", 252, 4, 0xa55a),
                seed_words("D", 4996, 4, 0x5aa5)
            );
            let body = format!(
                "{op} C199F2 C255E1\n{op} C199F2 T255E3\n{op} C199F2 D4999E4\nMOV K-1 E1\nMOV K1 F2\nMOV K-2 E3\nMOV K-2 E4"
            );
            let observe = format!(
                "MOV C198 D200\nMOV C199 D201\nDMOV C200 D202\nDMOV C201 D204\nDMOV C253 D206\nDMOV C254 D208\nDMOV C255 D210\n{}\n{}\nMOV E1 D250\nMOV F2 D251\nMOV E3 D252\nMOV E4 D253",
                read_words("T", 252, 4, 220),
                read_words("D", 4996, 4, 230)
            );
            cases.push(scenario(
                &format!("natural_counter_and_last_pair_{op}_C199_C200_C255"),
                &seed,
                &body,
                &observe,
            ));
        } else {
            let seed = format!(
                "MOV H1357 C197\nMOV {alternate} C198\nMOV {maximum} C199\nDMOV H55667788 C200\n{}\n{}\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 E3\nMOV H1234 F3",
                seed_words("T", 253, 3, 0xa55a),
                seed_words("D", 4997, 3, 0x5aa5)
            );
            let body = format!(
                "{op} C199F2 T255E1\n{op} C199F2 D4999E3\nMOV K-1 E1\nMOV K-1 F2\nMOV K-1 E3"
            );
            let observe = format!(
                "MOV C197 D200\nMOV C198 D201\nMOV C199 D202\nDMOV C200 D204\n{}\n{}\nMOV E1 D250\nMOV F2 D251\nMOV E3 D252",
                read_words("T", 253, 3, 220),
                read_words("D", 4997, 3, 230)
            );
            cases.push(scenario(
                &format!("last_word_{op}_C199_T255_D4999"),
                &seed,
                &body,
                &observe,
            ));
        }
    }
    assert_eq!(cases.len(), 104);

    // Overwrite the target's own index but refresh it even on OFF/held scans.
    // A second site tests in-place word and shifted overlapping wide values;
    // converted data can become invalid on a later visit, preserving target.
    for (op, wide) in FORMS {
        let (value, alternate, _, _) = literals(op, wide);
        let mov = if wide { "DMOV" } else { "MOV" };
        let (modifier, alias) = if wide { ("F2", 1185) } else { ("E1", 1182) };
        let alias_seed = if wide {
            "MOV H5566 E2\nMOV K1 F2\nMOV H2468 E3\nMOV H7788 F3"
        } else {
            "MOV K1 E1\nMOV H6A5A F1\nMOV H7788 E2"
        };
        let overlap_target = if wide { 65 } else { 64 };
        let seed = format!(
            "{mov} {value} D32\n{}\n{mov} {value} D63\n{alias_seed}\nMOV K-1 E4\nMOV H5678 F4\nMOV K0 E5\nMOV H1234 F5",
            seed_words("D", 62, 5, 0xa55a)
        );
        let body = format!(
            "LDI X7\nMOV K1 {modifier}\nLD X0\n{op} D32E5 D{}{modifier}\n{op} D64E4 D{overlap_target}E4\n{mov} {alternate} D32",
            alias - 1
        );
        let alias_read = if wide {
            "MOV E2 D240\nMOV F2 D241\nMOV E3 D242\nMOV F3 D243"
        } else {
            "MOV E1 D240\nMOV F1 D241\nMOV E2 D242"
        };
        let observe = format!(
            "{}\n{alias_read}\n{mov} D32 D250\nMOV E4 D252\nMOV E5 D253",
            read_words("D", 62, 5, 220)
        );
        cases.push(scenario(
            &format!("self_index_and_source_overlap_{op}_{modifier}"),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 112);

    // Direct E/F remain valid word values/targets when another operand is
    // indexed. Each direct destination is independent of the offset words.
    for op in ["BCD", "BCDP", "BIN", "BINP"] {
        let (value, alternate, _, _) = literals(op, false);
        let seed = format!(
            "MOV {value} E1\nMOV {value} F1\nMOV {value} D32\nDMOV H5AA5A55A E2\nMOV K0 E3\nMOV H6A5A F3\nMOV K0 E4\nMOV H7BBB F4\n{}",
            seed_words("D", 159, 8, 0xa55a)
        );
        let body = format!(
            "{op} F1 D160E3\n{op} E1 D164E3\n{op} D32E4 E2\n{op} D32E4 F2\nMOV {alternate} E1\nMOV {alternate} F1\nMOV {alternate} D32"
        );
        let observe = format!(
            "{}\nMOV E1 D240\nMOV F1 D241\nMOV E2 D242\nMOV F2 D243\nMOV E3 D250\nMOV E4 D251",
            read_words("D", 159, 8, 220)
        );
        cases.push(scenario(
            &format!("direct_E_F_word_roles_{op}"),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 116);

    // Legal M1019..M1022 groups reveal natural flag writes. InstalledStorageOnly
    // is required for M1019, as in its independently owned storage corpus.
    cases.push(scenario(
        "control_BCD_mps_M1019_result",
        "SET M1018\nMOV H5 K1M1019\nMOV K8 D32\nMOV K1 E1\nMOV H6A5A F1\nMOV K0 E4\nMOV H7BBB F4",
        "MPS\nBCD D32E4 K1M1018E1\nMRD\nMOV K9 D32\nMPP\nOUT Y10",
        "MOV K1M1019 D240\nLD M1018\nOUT Y11\nLDI X7\nMOV D32 D250\nMOV E1 D251",
    ));
    cases.push(scenario(
        "control_BINP_for_M1019_source",
        &format!(
            "SET M1018\nMOV H9 K1M1019\nMOV K1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\n{}",
            seed_words("D", 159, 5, 0xa55a)
        ),
        "LDI X7\nMOV K0 D40\nFOR K3\nLD X0\nBINP K1M1018E1 D160F2\nRST M1019\nNEXT",
        &format!(
            "{}\nMOV K1M1019 D240\nLD M1018\nOUT Y11",
            read_words("D", 159, 5, 220)
        ),
    ));
    cases.push(scenario(
        "control_DBCD_cj_direct_E_source_to_M1019",
        "SET M1018\nMOV H5 K1M1019\nDMOV K12345678 E3\nMOV K1 E1\nMOV H6A5A F1\nMOV K0 D40",
        "LDI X0\nCJ P10\nLD X0\nDBCD E3 K1M1018E1\nDMOV K87654321 E3\nP10\nLDI X7\nMOV K123 D40",
        "MOV K1M1019 D240\nDMOV E3 D250\nMOV D40 D252\nLD M1018\nOUT Y11",
    ));
    cases.push(scenario(
        "control_DBINP_mps_M1019_source",
        &format!(
            "SET M1018\nMOV H9 K1M1019\nMOV K1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\n{}",
            seed_words("D", 159, 5, 0xa55a)
        ),
        "MPS\nDBINP K1M1018E1 D160F2\nMRD\nRST M1019\nMPP\nOUT Y10",
        &format!(
            "{}\nMOV K1M1019 D240\nLD M1018\nOUT Y11",
            read_words("D", 159, 5, 220)
        ),
    ));
    assert_eq!(cases.len(), 120);

    // E18 precedes any access to D or its modifier. Source and target become
    // valid together before the second held visit; native source capture must
    // use that live value rather than a prior/local conversion counterpart.
    for (op, wide) in FORMS {
        let mov = if wide { "DMOV" } else { "MOV" };
        let (valid, _, _, invalid) = literals(op, wide);
        let seed = format!(
            "{mov} {invalid} D32\n{}\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",
            seed_words("D", 0, 6, 0xa55a)
        );
        let body = format!("LD X2\n{mov} {valid} D32\nMOV K0 E1\nLD X0\n{op} D32F2 D0E1");
        let observe = format!(
            "{}\n{mov} D32 D240\nMOV E1 D250\nMOV F2 D251",
            read_words("D", 0, 6, 220)
        );
        let mut item = scenario(
            &format!("invalid_source_unused_negative_target_{op}"),
            &seed,
            &body,
            &observe,
        );
        item.stages = vec![
            (
                vec![("X1", true), ("X0", false), ("X2", false), ("X7", false)],
                1,
            ),
            (vec![("X1", false), ("X0", true)], 1),
            (vec![("X2", true)], 1),
            (vec![("X0", false)], 1),
            (vec![("X0", true)], 1),
        ];
        cases.push(item);
    }
    assert_eq!(cases.len(), 128);

    // This family's generic wide writer maps direct E targets canonically.
    // Every normal/P wide form visits E0/E1 together and E7 separately. Their
    // paired F words are actual result halves, not modifiers or extra targets.
    for op in ["DBCD", "DBCDP", "DBIN", "DBINP"] {
        let (value, alternate, _, _) = literals(op, true);
        for last_bank in [false, true] {
            let targets = if last_bank {
                "DMOV H5AA5A55A E7"
            } else {
                "DMOV H5AA5A55A E0\nDMOV H3CC39696 E1\nDMOV H12345678 E2"
            };
            let seed = format!(
                "MOV H5566 D31\nDMOV {value} D32\nDMOV {alternate} D34\nMOV H7788 D36\n{targets}\nMOV K0 E4\nMOV H6A5A F4\nMOV H7BBB E5\nMOV K0 F5\nDMOV H9ABC5678 E6"
            );
            let body = if last_bank {
                format!("{op} D32F5 E7\nMOV K2 E4\nMOV K2 F5")
            } else {
                format!("{op} D32E4 E0\n{op} D32F5 E1\nMOV K2 E4\nMOV K2 F5")
            };
            let target_reads = if last_bank {
                "DMOV E7 D220\nDMOV D1194 D222\nDMOV E6 D224"
            } else {
                "DMOV E0 D220\nDMOV D1028 D222\nDMOV E1 D224\nDMOV D1182 D226\nDMOV E2 D228\nDMOV E6 D230"
            };
            let observe = format!(
                "{target_reads}\n{}\nMOV E4 D250\nMOV F4 D251\nMOV F5 D252",
                read_words("D", 31, 6, 240)
            );
            cases.push(scenario(
                &format!(
                    "canonical_direct_E_wide_{op}_{}",
                    if last_bank { "E7_F7" } else { "E0_F0_E1_F1" }
                ),
                &seed,
                &body,
                &observe,
            ));
        }
    }
    assert_eq!(cases.len(), 136);
    assert_eq!(
        cases
            .iter()
            .map(|item| &item.name)
            .collect::<BTreeSet<_>>()
            .len(),
        cases.len()
    );
    let actual: BTreeSet<_> = cases
        .iter()
        .flat_map(|item| item.source.lines())
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    for (op, _) in FORMS {
        assert!(actual.contains(op), "missing indexed BCD/BIN spelling {op}");
    }
    cases
}
