//! Indexed D-only coefficient/exponent and binary32 conversion fixtures.
//! DEBIN validation precedes zero and destination access; DEBCD scratch
//! observations remain profile-independent IL and are ES2-only below.
use super::{Case, Target, case};
use std::collections::BTreeSet;

const FORMS: [&str; 4] = ["DEBCD", "DEBCDP", "DEBIN", "DEBINP"];

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (vec![("X1", true), ("X0", false), ("X7", false)], 1),
        (vec![("X1", false), ("X0", true)], 2),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}

fn index_base(index: u16) -> u16 {
    if index == 0 { 1028 } else { 1180 + index * 2 }
}

fn seed_words(base: u16, count: u16) -> String {
    (0..count)
        .map(|i| {
            format!(
                "MOV H{:04X} D{}",
                0xa55au16.wrapping_add(i.wrapping_mul(0x1111)),
                base + i
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn read_words(base: u16, count: u16, output: u16) -> String {
    (0..count)
        .map(|i| format!("MOV D{} D{}", base + i, output + i))
        .collect::<Vec<_>>()
        .join("\n")
}

fn flag_snapshot(slot: u16) -> String {
    (0..3)
        .map(|bit| format!("LD M{}\nOUT Y{:o}", 1020 + bit, 64 + slot * 3 + bit))
        .collect::<Vec<_>>()
        .join("\n")
}

fn set_flags(pattern: u16) -> String {
    (0..3)
        .map(|bit| {
            format!(
                "{} M{}",
                if pattern & (1 << bit) == 0 {
                    "RST"
                } else {
                    "SET"
                },
                1020 + bit
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn result_site(op: &str, operand: &str, slot: u16) -> String {
    format!(
        "LD X0\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\n{}\n{op} {operand} D{}F2\nOUT Y{:o}\n{}\nLDI X7\nMOV D1067 D{}\nMOV D1068 D{}",
        set_flags(slot % 8),
        159 + slot * 3,
        128 + slot,
        flag_snapshot(slot),
        80 + slot * 2,
        81 + slot * 2
    )
}

fn scenario(name: &str, seed: &str, body: &str, observe: &str) -> Case {
    let gate = if body.starts_with("LD") { "" } else { "LD X0" };
    let source = [
        "LD X1\nMOV H55AA E0\nMOV H1234 F0\nSET M1020\nSET M1021\nSET M1022\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068",
        seed, gate, body,
        "LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24",
        "LDI X7\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305\nMOV K1234 D306",
        observe, "END",
    ].into_iter().filter(|part| !part.is_empty()).collect::<Vec<_>>().join("\n");
    assert!(
        source.lines().count() <= 512,
        "oversized indexed decimal conversion case {name}"
    );
    assert!(
        source.lines().count() * 9 <= 0x1ef0,
        "installed program budget: {name}"
    );
    let mut indexed = false;
    for line in source.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.first().is_some_and(|op| FORMS.contains(op)) {
            assert_eq!(
                fields.len(),
                3,
                "decimal conversion field count: {name}: {line}"
            );
            assert!(
                fields[1].starts_with('D') && fields[2].starts_with('D'),
                "decimal conversion operand roles: {name}: {line}"
            );
            indexed |= fields[1..].iter().any(|operand| {
                operand.starts_with('D')
                    && operand.len() > 3
                    && matches!(operand.as_bytes()[operand.len() - 2], b'E' | b'F')
                    && operand.as_bytes()[operand.len() - 1].is_ascii_digit()
            });
        }
    }
    assert!(
        indexed,
        "missing indexed decimal conversion action in {name}"
    );
    case(
        &format!("indexed_decimal_conversion_{name}"),
        &source,
        stages(),
    )
}

fn check_cases(cases: &[Case], count: usize) {
    assert_eq!(cases.len(), count);
    assert_eq!(
        cases
            .iter()
            .map(|entry| &entry.name)
            .collect::<BTreeSet<_>>()
            .len(),
        count
    );
    let actual: BTreeSet<_> = cases
        .iter()
        .flat_map(|entry| entry.source.lines())
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    for op in FORMS {
        assert!(
            actual.contains(op),
            "missing decimal conversion spelling {op}"
        );
    }
}

fn decimal_pair(coefficient: i16, exponent: i16) -> u32 {
    u32::from(coefficient as u16) | (u32::from(exponent as u16) << 16)
}

pub(super) fn cases() -> Vec<Case> {
    let mut cases = Vec::new();

    // Every possible adjacent source pair is valid in its own interpretation.
    // A DEBIN pair is signed coefficient/exponent, never a raw float here.
    for bank in ['E', 'F'] {
        for index in 0..8u16 {
            let modifier = format!("{bank}{index}");
            let pair = index_base(index);
            let alias = pair + u16::from(bank == 'F');
            let companion = pair + u16::from(bank == 'E');
            let delta = i32::from(index % 4) - 2;
            for family in ["DEBCD", "DEBIN"] {
                let pulse = (index + u16::from(bank == 'F')) % 2 == 1;
                let op = format!("{family}{}", if pulse { "P" } else { "" });
                let inputs = (0..9)
                    .map(|i| {
                        if family == "DEBCD" {
                            format!(
                                "MOV H{:04X} D{}",
                                [0x3f80, 0x4000, 0x4040, 0x4080, 0x4100, 0x3f00][i % 6],
                                61 + i
                            )
                        } else {
                            format!("MOV K{} D{}", i as i32 - 3, 61 + i)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let poison = if bank == 'F' && (1..7).contains(&index) {
                    format!("MOV H6A5A D{}", alias + 1)
                } else {
                    String::new()
                };
                let seed = format!(
                    "{inputs}\n{}\nMOV H5AA5 D{companion}\n{poison}\nMOV K{delta} D{alias}",
                    seed_words(155, 16)
                );
                let body = format!(
                    "{op} D64{modifier} D160{modifier}\nMOV K{} {modifier}",
                    delta + 1
                );
                let observe = format!(
                    "{}\n{}\nMOV E{index} D260\nMOV F{index} D261\nMOV D{pair} D262\nMOV D{} D263",
                    read_words(61, 9, 200),
                    read_words(155, 16, 220),
                    pair + 1
                );
                cases.push(scenario(
                    &format!("all_indices_{op}_{modifier}"),
                    &seed,
                    &body,
                    &observe,
                ));
            }
        }
    }
    assert_eq!(cases.len(), 32);

    // Nonfinite responsiveness is not inferred from other floating families.
    // In particular the infinity cases are separately named and retained.
    let raw_sets: [(&str, &[u32]); 7] = [
        (
            "ordinary_raw",
            &[
                0x3f800000, 0x41200000, 0x42c80000, 0x447a0000, 0xc1b80000, 0x4048f5c3, 0xc048f5c3,
                0x3dcccccd, 0x3c23d70a, 0x3a83126f,
            ],
        ),
        (
            "decimal_power_neighbors",
            &[
                0x3f7fffff, 0x3f800000, 0x3f800001, 0x411fffff, 0x41200000, 0x41200001, 0x3dcccccc,
                0x3dcccccd, 0x3dccccce, 0xbf7fffff, 0xbf800000, 0xbf800001,
            ],
        ),
        (
            "zero_and_quiet_nan",
            &[
                0, 0x80000000, 0x7fc01234, 0xffc01234, 0x7fffffff, 0xffffffff,
            ],
        ),
        (
            "subnormal_and_min_normal",
            &[
                1, 0x80000001, 0x007fffff, 0x807fffff, 0x00800000, 0x80800000, 0x00800001,
                0x80800001,
            ],
        ),
        (
            "signaling_nan_reference_response",
            &[0x7f800001, 0xff800001, 0x7fbfffff, 0xffbfffff],
        ),
        (
            "extreme_finite",
            &[0x7f7fffff, 0xff7fffff, 0x7f000000, 0xff000000],
        ),
        ("infinity_reference_response", &[0x7f800000, 0xff800000]),
    ];
    for op in ["DEBCD", "DEBCDP"] {
        for (label, values) in raw_sets {
            let count = values.len() as u16;
            let mut seed = format!(
                "{}\n{}\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K1 F2",
                seed_words(159, count * 3 + 1),
                seed_words(80, count * 2)
            );
            let mut body = Vec::new();
            for (slot, raw) in values.iter().enumerate() {
                let slot = slot as u16;
                seed.push_str(&format!("\nDMOV H{raw:08X} D{}", 32 + slot * 2));
                body.push(result_site(op, &format!("D{}E1", 33 + slot * 2), slot));
            }
            let observe = format!(
                "{}\n{}\n{}",
                read_words(32, count * 2, 200),
                read_words(159, count * 3 + 1, 230),
                read_words(80, count * 2, 270)
            );
            cases.push(scenario(
                &format!("{label}_{op}"),
                &seed,
                &body.join("\n"),
                &observe,
            ));
        }
    }
    assert_eq!(cases.len(), 46);

    let decimal_sets: [(&str, &[(i16, i16)]); 4] = [
        (
            "manual_and_fractional_exponents",
            &[
                (314, -2),
                (-314, -2),
                (314, -4),
                (-314, -4),
                (1, -4),
                (-1, -4),
                (1000, -3),
                (9999, -4),
                (42, 1),
                (42, -1),
            ],
        ),
        (
            "valid_coefficient_exponent_limits",
            &[
                (-9999, 0),
                (9999, 0),
                (1176, -41),
                (-1176, -41),
                (3402, 35),
                (-3402, 35),
                (9999, 34),
                (-9999, 34),
                (1, -40),
                (-1, -40),
                (0, 35),
                (0, -40),
            ],
        ),
        (
            "invalid_coefficient_exponent_limits",
            &[
                (-10000, 0),
                (10000, 0),
                (-32768, 0),
                (32767, 0),
                (1175, -41),
                (-1175, -41),
                (3403, 35),
                (-3403, 35),
                (1, -42),
                (1, 36),
                (1, -32768),
                (1, 32767),
            ],
        ),
        (
            "validate_before_zero",
            &[
                (0, -42),
                (0, -41),
                (0, -40),
                (0, 0),
                (0, 34),
                (0, 35),
                (0, 36),
                (0, -32768),
            ],
        ),
    ];
    for op in ["DEBIN", "DEBINP"] {
        for (label, values) in decimal_sets {
            let count = values.len() as u16;
            let mut seed = format!(
                "{}\n{}\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K1 F2",
                seed_words(159, count * 3 + 1),
                seed_words(80, count * 2)
            );
            let mut body = Vec::new();
            for (slot, &(coefficient, exponent)) in values.iter().enumerate() {
                let slot = slot as u16;
                seed.push_str(&format!(
                    "\nMOV K{coefficient} D{}\nMOV K{exponent} D{}",
                    32 + slot * 2,
                    33 + slot * 2
                ));
                body.push(result_site(op, &format!("D{}E1", 33 + slot * 2), slot));
            }
            let observe = format!(
                "{}\n{}\n{}",
                read_words(32, count * 2, 200),
                read_words(159, count * 3 + 1, 230),
                read_words(80, count * 2, 270)
            );
            cases.push(scenario(
                &format!("{label}_{op}"),
                &seed,
                &body.join("\n"),
                &observe,
            ));
        }
    }
    assert_eq!(cases.len(), 54);

    // Current DEBIN proof: clear ALU flags, snapshot/validate signed pair,
    // E1A+1, then skip the unused target/index. X2 repairs source and target
    // together, including the conservative preflight's held/OFF visits.
    for op in ["DEBIN", "DEBINP"] {
        for (slot, (coefficient, exponent)) in [(0, -41), (3403, 35), (10000, 0), (1, 36)]
            .into_iter()
            .enumerate()
        {
            let sticky = if slot >= 2 {
                "SET M1067\nSET M1068\nMOV H0123 D1067\nMOV K77 D1068"
            } else {
                ""
            };
            let seed = format!(
                "{}\nMOV K{coefficient} D32\nMOV K{exponent} D33\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K-1 F2\nMOV K0 D60\n{sticky}",
                seed_words(0, 4)
            );
            let body = format!(
                "LD X2\nMOV K314 D32\nMOV K-2 D33\nMOV K0 F2\nLD X0\n{op} D32E1 D0F2\nOUT Y10\nINC D60"
            );
            let observe = format!(
                "{}\nMOV D32 D230\nMOV D33 D231\nMOV D60 D240\nMOV E1 D250\nMOV F2 D251",
                read_words(0, 4, 220)
            );
            let mut entry = scenario(
                &format!("invalid_source_skips_target_recovery_{slot}_{op}"),
                &seed,
                &body,
                &observe,
            );
            entry.stages = vec![
                (
                    vec![("X1", true), ("X0", false), ("X2", false), ("X7", false)],
                    1,
                ),
                (vec![("X1", false), ("X0", true)], 1),
                (vec![("X2", true)], 1),
                (vec![("X0", false), ("X2", false)], 1),
                (vec![("X0", true)], 1),
            ];
            cases.push(entry);
        }
    }
    assert_eq!(cases.len(), 62);

    for (which, op) in FORMS.into_iter().enumerate() {
        let (modifier, alias, companion) = if which % 2 == 0 {
            ("E1", 1182, 1183)
        } else {
            ("F2", 1185, 1184)
        };
        let lexical = alias - 1;
        let raw = if op.starts_with("DEBCD") {
            0x4048f5c3
        } else {
            decimal_pair(314, -2)
        };
        let seed = format!(
            "{}\n{}\nDMOV H{raw:08X} D32\nMOV K1 E1\nMOV K1 F2\nMOV H5AA5 D{companion}",
            seed_words(alias - 1, 4),
            seed_words(31, 5)
        );
        let body = format!(
            "LDI X7\nDMOV H{raw:08X} D32\nMOV K1 E1\nMOV K1 F2\nLD X0\n{op} D31E1 D{lexical}{modifier}\n{}\nLDI X7\nDMOV D{alias} D80\nMOV K0 E1\nMOV K0 F2\nLD X0\n{op} D32E1 D33F2\n{}\nLDI X7\nDMOV D33 D84",
            flag_snapshot(0),
            flag_snapshot(1)
        );
        let observe = format!(
            "{}\n{}\nDMOV D80 D240\nDMOV D84 D244\nMOV D{companion} D246\nMOV E1 D250\nMOV F1 D251\nMOV E2 D252\nMOV F2 D253",
            read_words(31, 5, 200),
            read_words(alias - 1, 4, 220)
        );
        cases.push(scenario(
            &format!("snapshot_overlap_self_{modifier}_{op}"),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 66);

    // The source pair itself contains the live source index WORD. Only the
    // canonical E/F pairs are written; their adjacent system words are not.
    for (op, index, raw) in [
        ("DEBCD", 0u16, 0x4048f5c3u32),
        ("DEBCDP", 1, 0xc1b80000),
        ("DEBIN", 0, decimal_pair(314, -2)),
        ("DEBINP", 1, decimal_pair(-314, -2)),
    ] {
        let pair = index_base(index);
        let offset = raw as u16 as i16;
        let lexical = i32::from(pair) - i32::from(offset);
        assert!((0..4999).contains(&lexical));
        let seed = format!(
            "{}\nDMOV H{raw:08X} D{pair}\nMOV H7BBB E2\nMOV K0 F2",
            seed_words(159, 5)
        );
        let body = format!(
            "LDI X7\nDMOV H{raw:08X} D{pair}\nMOV K0 F2\nLD X0\n{op} D{lexical}E{index} D160F2"
        );
        let observe = format!(
            "{}\nMOV D{pair} D240\nMOV D{} D241\nMOV E{index} D250\nMOV F{index} D251",
            read_words(159, 5, 220),
            pair + 1
        );
        cases.push(scenario(
            &format!("source_contains_index_E{index}_{op}"),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 70);

    for op in FORMS {
        let values = if op.starts_with("DEBCD") {
            [0x4048f5c3, 0xc1b80000, 0x3dcccccd]
        } else {
            [
                decimal_pair(314, -2),
                decimal_pair(-23, 0),
                decimal_pair(1, -4),
            ]
        };
        let seed = format!(
            "{}\nDMOV H{:08X} D32\nDMOV H{:08X} D34\nDMOV H{:08X} D36\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",
            seed_words(159, 10),
            values[0],
            values[1],
            values[2]
        );
        let body = (0..3u16)
            .map(|slot| {
                format!(
                    "LDI X7\nMOV K{} E1\nMOV K{} F2\nLD X0\n{op} D32E1 D160F2\n{}",
                    slot * 2,
                    slot * 3,
                    flag_snapshot(slot)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let observe = format!(
            "{}\n{}\nMOV E1 D250\nMOV F2 D251",
            read_words(32, 6, 200),
            read_words(159, 10, 220)
        );
        cases.push(scenario(
            &format!("same_scan_source_target_offsets_{op}"),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 74);

    cases.push(scenario(
        "control_DEBCD_mps_manual_roundtrip",
        &format!(
            "{}\nDMOV H4048F5C3 D32\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",
            seed_words(159, 8)
        ),
        "MPS\nDEBCD D32E1 D160F2\nMRD\nDEBIN D160F2 D164E1\nMPP\nOUT Y10",
        &read_words(159, 8, 220),
    ));
    cases.push(scenario(
        "control_DEBCDP_for_true_false_true_same_pc",
        &format!("{}\n{}\nDMOV H4048F5C3 D32\nMOV K0 D30\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 E3\nMOV H1234 F3", seed_words(159, 4), seed_words(80, 8)),
        "LDI X7\nMOV K0 D30\nFOR K3\nLDI X7\nMUL16 D30 K2 E3\nLD= D30 K0\nOR= D30 K2\nAND X0\nDEBCDP D32E1 D160F2\nLDI X7\nDMOV D160 D80E3\nINC D30\nNEXT",
        &format!("{}\n{}\nMOV D30 D250", read_words(159, 4, 220), read_words(80, 8, 230)),
    ));
    cases.push(scenario(
        "control_DEBIN_cj_skipped_false_visits",
        &format!("{}\nMOV K314 D32\nMOV K-2 D33\nMOV K0 D50\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2", seed_words(159, 5)),
        "LDI X0\nCJ P10\nLD X0\nDEBIN D32E1 D160F2\nMOV K-314 D32\nP10\nLDI X7\nMOV K123 D50",
        &format!("{}\nMOV D50 D240", read_words(159, 5, 220)),
    ));
    cases.push(scenario(
        "control_DEBINP_mps_changed_target",
        &format!(
            "{}\nMOV K314 D32\nMOV K-2 D33\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",
            seed_words(159, 6)
        ),
        "MPS\nDEBINP D32E1 D160F2\nMRD\nMOV K1 F2\nMPP\nOUT Y10",
        &format!("{}\nMOV E1 D250\nMOV F2 D251", read_words(159, 6, 220)),
    ));
    check_cases(&cases, 78);
    cases
}

fn scratch_seed() -> &'static str {
    "MOV H1111 D4998\nMOV H2222 D4999\nMOV H1357 D5000\nMOV H2468 D5001\nMOV H5555 D5002\nMOV H6666 D5003\nMOV H7777 D5004"
}

fn scratch_observe() -> String {
    format!(
        "MOV D5000 D200\nMOV D5001 D201\n{}\n{}\nMOV E1 D250\nMOV F2 D251",
        read_words(4998, 7, 210),
        read_words(159, 5, 230)
    )
}

pub(super) fn model_cases(target: Target) -> Vec<Case> {
    let last: u16 = match target {
        Target::Es2 => 9999,
        Target::Ss2 => 4999,
    };
    let pair = last - 1;
    let mut cases = Vec::new();
    for op in FORMS {
        let raw = if op.starts_with("DEBCD") {
            0x4048f5c3
        } else {
            decimal_pair(314, -2)
        };
        let next = if op.starts_with("DEBCD") {
            0x3f800000
        } else {
            decimal_pair(0, 0)
        };
        let seed = format!(
            "{}\n{}\nDMOV H{raw:08X} D{pair}\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",
            seed_words(last - 2, 3),
            seed_words(19, 5)
        );
        let body = format!("{op} D{last}E1 D20F2\nDMOV H{next:08X} D{pair}");
        let observe = format!(
            "{}\n{}\nMOV E1 D250\nMOV F2 D251",
            read_words(last - 2, 3, 200),
            read_words(19, 5, 220)
        );
        cases.push(scenario(
            &format!("model_{}_last_pair_source_{op}", target.name()),
            &seed,
            &body,
            &observe,
        ));

        let seed = format!(
            "{}\nDMOV H{raw:08X} D32\nMOV K0 E1\nMOV H6A5A F1\nMOV K-1 F7\nMOV H1234 E7",
            seed_words(last - 3, 4)
        );
        let body = format!("{op} D32E1 D{last}F7\nMOV K-2 F7");
        let observe = format!(
            "{}\nMOV E1 D248\nMOV F7 D250\nMOV E7 D251",
            read_words(last - 3, 4, 220)
        );
        cases.push(scenario(
            &format!("model_{}_last_pair_target_{op}", target.name()),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 8);

    if target == Target::Es2 {
        // Identical source IL is suitable for CurrentPrivate and LegacyVisible
        // ES2 runs. Explicit low-D copies keep public scratch in observations.
        for op in ["DEBCD", "DEBCDP"] {
            for (dest, raw, next) in [
                (160u16, 0x4048f5c3u32, 0u32),
                (4999, 0xc1b80000, 0x7fc01234),
                (5000, 0x00800000, 0),
                (5001, 0x7f7fffff, 0xffc01234),
            ] {
                let lexical = dest - 1;
                let seed = format!(
                    "{}\n{}\nDMOV H{raw:08X} D32\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K1 F2",
                    scratch_seed(),
                    seed_words(159, 5)
                );
                let body = format!("{op} D32E1 D{lexical}F2\nDMOV H{next:08X} D32");
                cases.push(scenario(
                    &format!("model_ES2_scratch_target_D{dest}_{op}"),
                    &seed,
                    &body,
                    &scratch_observe(),
                ));
            }
            for (source, dest) in [(4999u16, 5000u16), (5000, 4999)] {
                let source_base = source + 1;
                let target_base = dest - 1;
                let seed = format!(
                    "{}\n{}\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K1 F2",
                    scratch_seed(),
                    seed_words(159, 5)
                );
                let body = format!(
                    "LDI X7\nDMOV H4048F5C3 D{source}\nLD X0\n{op} D{source_base}E1 D{target_base}F2"
                );
                cases.push(scenario(
                    &format!("model_ES2_scratch_source_D{source}_target_D{dest}_{op}"),
                    &seed,
                    &body,
                    &scratch_observe(),
                ));
            }
        }
    }
    check_cases(&cases, if target == Target::Es2 { 20 } else { 8 });
    cases
}
