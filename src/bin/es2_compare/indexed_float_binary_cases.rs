//! Indexed two-source binary32 operations, preserving integer/raw source kinds.
//! Large finite overflow probes are separate from NaN/Inf probes: their older
//! unindexed counterparts were withheld after nonresponse on other images.
//! This corpus records comparisons; it does not assert current native parity.
use super::{Case, Target, case};
use std::collections::BTreeSet;

const FORMS: [&str; 8] = [
    "DEADD", "DEADDP", "DESUB", "DESUBP", "DEMUL", "DEMULP", "DEDIV", "DEDIVP",
];

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (
            vec![("X1", true), ("X0", false), ("X2", false), ("X7", false)],
            1,
        ),
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

fn result_site(op: &str, left: &str, right: &str, slot: u16) -> String {
    format!(
        "LD X0\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\n{}\n{op} {left} {right} D{}E3\n{}\nLDI X7\nMOV D1067 D{}\nMOV D1068 D{}",
        set_flags(slot % 8),
        159 + slot * 3,
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
    ].into_iter().filter(|section| !section.is_empty()).collect::<Vec<_>>().join("\n");
    assert!(
        source.lines().count() <= 512,
        "oversized indexed float binary case {name}"
    );
    // Thirteen is an upper bound for every instruction emitted here.
    assert!(
        source.lines().count() * 13 <= 0x1ef0,
        "installed program budget: {name}"
    );
    assert!(
        source.lines().any(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            fields.len() == 4
                && FORMS.contains(&fields[0])
                && fields[1..].iter().any(|operand| {
                    operand.starts_with('D')
                        && operand.len() > 3
                        && matches!(operand.as_bytes()[operand.len() - 2], b'E' | b'F')
                        && operand.as_bytes()[operand.len() - 1].is_ascii_digit()
                })
        }),
        "missing indexed float binary action in {name}"
    );
    case(&format!("indexed_float_binary_{name}"), &source, stages())
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
        assert!(actual.contains(op), "missing float binary spelling {op}");
    }
}

pub(super) fn cases() -> Vec<Case> {
    let mut cases = Vec::new();

    // Every suffix serves all three D roles. Both initial and moved offsets are
    // bounded; every possible adjacent source pair is a finite normal number.
    for bank in ['E', 'F'] {
        for index in 0..8u16 {
            let modifier = format!("{bank}{index}");
            let pair = index_base(index);
            let alias = pair + u16::from(bank == 'F');
            let companion = pair + u16::from(bank == 'E');
            let delta = i32::from(index % 4) - 2;
            for family in ["DEADD", "DESUB", "DEMUL", "DEDIV"] {
                let pulse = (index + u16::from(bank == 'F')) % 2 == 1;
                let op = format!("{family}{}", if pulse { "P" } else { "" });
                let mut inputs = Vec::new();
                for start in [61, 93] {
                    for i in 0..9 {
                        let word = [0x3f80, 0x4000, 0x4040, 0x4080, 0x4100, 0x3f00][i % 6];
                        inputs.push(format!("MOV H{word:04X} D{}", start + i));
                    }
                }
                let poison = if bank == 'F' && (1..7).contains(&index) {
                    format!("MOV H6A5A D{}", alias + 1)
                } else {
                    String::new()
                };
                let seed = format!(
                    "{}\n{}\nMOV H5AA5 D{companion}\n{poison}\nMOV K{delta} D{alias}",
                    inputs.join("\n"),
                    seed_words(155, 16)
                );
                let body = format!(
                    "{op} D64{modifier} D96{modifier} D160{modifier}\nMOV K{} {modifier}",
                    delta + 1
                );
                let observe = format!(
                    "{}\n{}\n{}\nMOV E{index} D260\nMOV F{index} D261\nMOV D{pair} D262\nMOV D{} D263",
                    read_words(61, 9, 200),
                    read_words(93, 9, 210),
                    read_words(155, 16, 230),
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
    assert_eq!(cases.len(), 64);

    // Raw IEEE data are only introduced via DMOV into D storage. In particular,
    // these H encodings must never become arithmetic H operands.
    let raw_sets: [(&str, &[(u32, u32)]); 4] = [
        (
            "finite_zero_subnormal",
            &[
                (0x00000000, 0x00000000),
                (0x80000000, 0x80000000),
                (0x00000000, 0x80000000),
                (0x00000001, 0x3f800000),
                (0x80000001, 0x3f000000),
                (0x00800000, 0x40000000),
                (0x3f800000, 0xbf800000),
                (0x3fc00000, 0x40200000),
            ],
        ),
        (
            "large_finite_overflow",
            &[
                // Slots0/1/2 reproduce the older filtered ADD/SUB/MUL vectors.
                (0x7f7fffff, 0x7f7fffff),
                (0x7f7fffff, 0xff7fffff),
                (0x7f7fffff, 0x40000000),
                (0xff7fffff, 0x40000000),
                (0x7f7fffff, 0x3f000000),
                // Slot5 is the exact older hard-div overflow probe.
                (0x7f7fffff, 0x00800000),
            ],
        ),
        (
            "infinity_and_nan",
            &[
                (0x7f800000, 0x3f800000),
                (0xff800000, 0x3f800000),
                (0x7f800000, 0xff800000),
                (0x7f800000, 0x00000000),
                (0x7f800000, 0x80000000),
                (0x7fc12345, 0x3f800000),
                (0x3f800000, 0xffc12345),
                (0xff812345, 0x3f800000),
            ],
        ),
        (
            "nan_payload_selection",
            &[
                (0x7fc00001, 0xffc00002),
                (0xffc00001, 0x7fc00002),
                (0xffc00001, 0x7fc00001),
                (0xff800002, 0x7fc00001),
                (0x7fc00001, 0xff800002),
                (0xffc00001, 0x7f800002),
                (0x7f800001, 0xff800002),
                (0xffc12345, 0xffc12345),
            ],
        ),
    ];
    for op in FORMS {
        for (label, values) in raw_sets {
            let count = values.len() as u16;
            let mut seed = format!(
                "{}\n{}\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K1 F2\nMOV K1 E3\nMOV H1234 F3",
                seed_words(159, count * 3 + 1),
                seed_words(80, count * 2)
            );
            let mut body = Vec::new();
            for (slot, (left, right)) in values.iter().enumerate() {
                let slot = slot as u16;
                seed.push_str(&format!(
                    "\nDMOV H{left:08X} D{}\nDMOV H{right:08X} D{}",
                    40 + slot * 4,
                    42 + slot * 4
                ));
                body.push(result_site(
                    op,
                    &format!("D{}E1", 41 + slot * 4),
                    &format!("D{}F2", 41 + slot * 4),
                    slot,
                ));
            }
            let observe = format!(
                "{}\n{}\n{}",
                read_words(40, count * 4, 200),
                read_words(159, count * 3 + 1, 240),
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
    assert_eq!(cases.len(), 96);

    // Literal values are signed integers, not raw binary32. MIN32 right-hand
    // literals retain the DIV-only branch driven by the ORIGINAL left sign.
    for op in FORMS {
        let operands = [
            ("H3F800000", "D32E1"),
            ("D32E1", "H3F800000"),
            ("K16777217", "K16777216"),
            ("K-16777217", "K3"),
            ("D36E1", "K-1"),
            ("K0", "H80000000"),
            ("K-1", "H80000000"),
            ("D40E1", "H80000000"),
            ("H80000000", "D44E1"),
            ("H80000000", "H80000000"),
            ("K1", "H80000000"),
            ("D32E1", "H80000000"),
        ];
        let count = operands.len() as u16;
        let seed = format!(
            "{}\n{}\n{}\nDMOV H3F800000 D32\nDMOV HBF800000 D36\nDMOV H80000000 D40\nDMOV H40000000 D44\nMOV K0 E1\nMOV H6A5A F1\nMOV K1 E3\nMOV H1234 F3",
            seed_words(31, 16),
            seed_words(159, count * 3 + 1),
            seed_words(80, count * 2)
        );
        let body = operands
            .iter()
            .enumerate()
            .map(|(slot, (left, right))| result_site(op, left, right, slot as u16))
            .collect::<Vec<_>>()
            .join("\n");
        let observe = format!(
            "{}\n{}\n{}",
            read_words(31, 16, 200),
            read_words(159, count * 3 + 1, 220),
            read_words(80, count * 2, 270)
        );
        cases.push(scenario(
            &format!("integer_kind_and_original_sign_{op}"),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 104);

    // The first destination overwrites its own E/F word. The second half must
    // use the same cached pair. Refreshing at every visit also makes OFF/held
    // preflight safe. A separate second site overlaps the high source half.
    for (which, op) in FORMS.into_iter().enumerate() {
        let use_f = which % 2 == 1;
        let (modifier, alias, companion) = if use_f {
            ("F2", 1185, 1184)
        } else {
            ("E1", 1182, 1183)
        };
        let lexical = alias - 1;
        let seed = format!(
            "{}\n{}\nDMOV H3FC00000 D32\nDMOV H40000000 D48\nMOV K1 E1\nMOV K1 F2\nMOV H5AA5 D{companion}",
            seed_words(alias - 1, 4),
            seed_words(31, 20)
        );
        let body = format!(
            "LDI X7\nMOV K1 E1\nMOV K1 F2\nLD X0\n{op} D31E1 D47F2 D{lexical}{modifier}\n{}\nLDI X7\nDMOV D{alias} D80\nMOV K0 E1\nMOV K0 F2\nLD X0\n{op} D32E1 D48F2 D33E1\n{}",
            flag_snapshot(0),
            flag_snapshot(1)
        );
        let observe = format!(
            "{}\n{}\nDMOV D80 D240\nMOV D{companion} D242\nMOV E1 D250\nMOV F1 D251\nMOV E2 D252\nMOV F2 D253",
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
    assert_eq!(cases.len(), 112);

    for op in ["DEADD", "DESUBP", "DEMUL", "DEDIVP"] {
        let seed = format!(
            "{}\nDMOV H3F800000 D32\nDMOV H40000000 D34\nDMOV H40400000 D36\nDMOV H40800000 D48\nDMOV H40000000 D50\nDMOV H3F000000 D52\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 E3\nMOV H1234 F3",
            seed_words(159, 10)
        );
        let mut body = Vec::new();
        for slot in 0..3u16 {
            body.push(format!(
                "LDI X7\nMOV K{} E1\nMOV K{} F2\nMOV K{} E3\nLD X0\n{op} D32E1 D48F2 D160E3\n{}",
                slot * 2,
                (2 - slot) * 2,
                slot * 3,
                flag_snapshot(slot)
            ));
        }
        let observe = format!(
            "{}\n{}\n{}\nMOV E1 D260\nMOV F2 D261\nMOV E3 D262",
            read_words(32, 6, 200),
            read_words(48, 6, 210),
            read_words(159, 10, 230)
        );
        cases.push(scenario(
            &format!("same_scan_live_three_offsets_{op}"),
            &seed,
            &body.join("\n"),
            &observe,
        ));
    }
    assert_eq!(cases.len(), 116);

    // The bad D0E1 destination is unused until X2 repairs both source values
    // and the index. Initial/OFF preflight can inspect the actual exceptional
    // source; no successful site ever retains an invalid target.
    let errors = [
        ("nan_positive_zero_e19", 0x7fc01234u32, 0x00000000u32),
        ("nan_negative_zero_e1a", 0x7fc01234, 0x80000000),
        ("nan_subnormal_e1a", 0x7fc01234, 0x00000001),
        ("finite_negative_zero_e19", 0x3f800000, 0x80000000),
        ("finite_subnormal_e19", 0x3f800000, 0x80000001),
    ];
    for op in ["DEDIV", "DEDIVP"] {
        for (slot, (label, left, right)) in errors.into_iter().enumerate() {
            let sticky = if slot % 2 == 1 {
                "SET M1067\nSET M1068\nMOV H0123 D1067\nMOV K77 D1068"
            } else {
                ""
            };
            let seed = format!(
                "{}\nDMOV H{left:08X} D32\nDMOV H{right:08X} D48\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K-1 F2\nMOV K-1 E3\nMOV H1234 F3\n{}\n{sticky}",
                seed_words(0, 4),
                set_flags(slot as u16 + 2)
            );
            let body = format!(
                "LD X2\nDMOV H40C00000 D32\nDMOV H40000000 D48\nMOV K0 E1\nLD X0\n{op} D33F2 D49E3 D0E1\nOUT Y10"
            );
            let observe = format!(
                "{}\nDMOV D32 D230\nDMOV D48 D232\nMOV E1 D250\nMOV F2 D251\nMOV E3 D252",
                read_words(0, 4, 220)
            );
            let mut entry = scenario(
                &format!("error_before_bad_target_then_live_recovery_{label}_{op}"),
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
    assert_eq!(cases.len(), 126);

    cases.push(scenario(
        "control_DEADD_mps_live_source",
        &format!("{}\nDMOV H3FC00000 D32\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2", seed_words(159, 8)),
        "MPS\nDEADD D32E1 K2 D160F2\nMRD\nDMOV HC0400000 D32\nMRD\nDEADD K3 D32E1 D164F2\nMPP\nOUT Y10",
        &format!("{}\nDMOV D32 D240", read_words(159, 8, 220)),
    ));
    cases.push(scenario(
        "control_DESUBP_cj_skipped_false_visits",
        &format!(
            "{}\nDMOV H40C00000 D32\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 D50",
            seed_words(159, 5)
        ),
        "LDI X0\nCJ P10\nLD X0\nDESUBP D32E1 K2 D160F2\nP10\nLDI X7\nMOV K123 D50",
        &format!("{}\nMOV D50 D240", read_words(159, 5, 220)),
    ));
    cases.push(scenario(
        "control_DEMUL_for_live_target_each_visit",
        &format!("{}\nDMOV H3FC00000 D32\nMOV K0 D30\nMOV K0 E1\nMOV H6A5A F1\nMOV K0 E3\nMOV H1234 F3", seed_words(159, 8)),
        "LDI X7\nMOV K0 D30\nFOR K3\nLDI X7\nMOV D30 E3\nLD X0\nDEMUL D32E1 K2 D160E3\nLDI X7\nINC D30\nNEXT",
        &format!("{}\nMOV D30 D240\nMOV E3 D250", read_words(159, 8, 220)),
    ));
    cases.push(scenario(
        "control_DEDIVP_for_same_pc_true_false_true",
        &format!("{}\n{}\nDMOV H40C00000 D32\nMOV K0 D30\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 E3\nMOV H1234 F3", seed_words(159, 4), seed_words(80, 8)),
        "LDI X7\nMOV K0 D30\nFOR K3\nLDI X7\nMUL16 D30 K2 E3\nLD= D30 K0\nOR= D30 K2\nAND X0\nDEDIVP D32E1 K2 D160F2\nLDI X7\nDMOV D160 D80E3\nINC D30\nNEXT",
        &format!("{}\n{}\nMOV D30 D250", read_words(159, 4, 220), read_words(80, 8, 230)),
    ));
    check_cases(&cases, 130);
    cases
}

pub(super) fn model_cases(target: Target) -> Vec<Case> {
    let last: u16 = match target {
        Target::Es2 => 9999,
        Target::Ss2 => 4999,
    };
    let pair = last - 1;
    let mut cases = Vec::new();
    for op in FORMS {
        let seed = format!(
            "{}\n{}\nDMOV H3FC00000 D{pair}\nDMOV H40000000 D32\nDMOV H3F000000 D48\nDMOV H5555AAAA D80\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K-1 F7\nMOV H1234 E7",
            seed_words(last - 2, 3),
            seed_words(159, 8)
        );
        // The lexical last register is legal only because -1 produces a pair.
        // Restore its finite source after observing the third result, so P and
        // normal modes visit the same bounded sources on following scans.
        let body = format!(
            "{op} D{last}E1 D32F2 D160F2\n{op} D32F2 D{last}F7 D164F2\n{op} D32F2 D48F2 D{last}E1\nLDI X7\nDMOV D{pair} D80\nDMOV H3FC00000 D{pair}"
        );
        let observe = format!(
            "{}\n{}\nDMOV D80 D240\nMOV E1 D250\nMOV F7 D251\nMOV E7 D252",
            read_words(last - 2, 3, 200),
            read_words(159, 8, 220)
        );
        cases.push(scenario(
            &format!("model_{}_last_pair_three_roles_{op}", target.name()),
            &seed,
            &body,
            &observe,
        ));
    }
    check_cases(&cases, 8);
    cases
}
