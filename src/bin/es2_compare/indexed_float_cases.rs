//! Indexed FLT/INT conversion modes, raw IEEE inputs and actual access widths.
//! Only numeric matrix roles appear: FLT D-only, INT T/C/D. Direct E/F and
//! floating/constant operands are absent. Natural-C word INT is explicitly
//! named as an installed runtime extension, separate from compiler admission.
use super::{Case, Target, case};
use std::collections::BTreeSet;

const FORMS: [(&str, bool); 8] = [
    ("FLT", false),
    ("FLTP", false),
    ("DFLT", true),
    ("DFLTP", true),
    ("INT", false),
    ("INTP", false),
    ("DINT", true),
    ("DINTP", true),
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

fn seed_words(area: &str, base: u16, count: u16) -> String {
    (0..count)
        .map(|i| {
            format!(
                "MOV H{:04X} {area}{}",
                0xa55au16.wrapping_add(i.wrapping_mul(0x1111)),
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

fn flag_snapshot(slot: u16) -> String {
    (0..3)
        .map(|bit| format!("LD M{}\nOUT Y{:o}", 1020 + bit, 64 + slot * 3 + bit))
        .collect::<Vec<_>>()
        .join("\n")
}

fn scenario(name: &str, seed: &str, body: &str, observe: &str) -> Case {
    let gate = if body.starts_with("LD") { "" } else { "LD X0" };
    let source = [
        "LD X1\nMOV H55AA E0\nMOV H1234 F0\nRST M1020\nSET M1021\nSET M1022\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068",
        seed, gate, body,
        "LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24\nLD M1081\nOUT Y25",
        "LDI X7\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305\nMOV K1234 D306",
        observe, "END",
    ].into_iter().filter(|section| !section.is_empty()).collect::<Vec<_>>().join("\n");
    assert!(
        source.lines().count() <= 512,
        "oversized indexed float case {name}"
    );
    assert!(
        name.starts_with("model_")
            || source.lines().any(|line| {
                let fields: Vec<_> = line.split_whitespace().collect();
                fields.len() == 3
                    && FORMS.iter().any(|(op, _)| *op == fields[0])
                    && fields[1..].iter().any(|operand| {
                        operand.len() > 2
                            && matches!(operand.as_bytes()[operand.len() - 2], b'E' | b'F')
                            && operand.as_bytes()[operand.len() - 1].is_ascii_digit()
                    })
            }),
        "missing indexed conversion in {name}"
    );
    case(&format!("indexed_float_{name}"), &source, stages())
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
    for (op, _) in FORMS {
        assert!(actual.contains(op), "missing conversion spelling {op}");
    }
}

pub(super) fn cases() -> Vec<Case> {
    let mut cases = Vec::new();

    // Every neighboring pair remains finite when interpreted as binary32,
    // but also yields distinct signed word/double integers in forward mode.
    for bank in ['E', 'F'] {
        for index in 0..8u16 {
            let modifier = format!("{bank}{index}");
            let pair = index_base(index);
            let alias = pair + u16::from(bank == 'F');
            let companion = pair + u16::from(bank == 'E');
            let delta = if index % 2 == 0 { -2 } else { 1 };
            for family in ["FLT", "DFLT", "INT", "DINT"] {
                let pulse = (index + u16::from(bank == 'F')) % 2 == 1;
                let op = format!("{family}{}", if pulse { "P" } else { "" });
                let inputs = (0..13)
                    .map(|i| {
                        format!(
                            "MOV H{:04X} D{}",
                            [0x0000, 0x3f00, 0x4000, 0x3f80][i % 4],
                            61 + i
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let poison = if bank == 'F' && (1..7).contains(&index) {
                    format!("MOV H6A5A D{}", alias + 1)
                } else {
                    String::new()
                };
                let mode = if family.contains("FLT") || index % 2 == 0 {
                    "RST"
                } else {
                    "SET"
                };
                let seed = format!(
                    "{inputs}\n{}\n{mode} M1081\nMOV H5AA5 D{companion}\n{poison}\nMOV K{delta} D{alias}",
                    seed_words("D", 155, 16)
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
                    &format!("all_indices_{op}_{modifier}"),
                    &seed,
                    &body,
                    &observe,
                ));
            }
        }
    }
    assert_eq!(cases.len(), 64);

    // Two FLT sites see opposite M1081 modes within one rung, with complete
    // flag snapshots after each. P visits keep their own PC edge identity.
    for (op, wide) in [
        ("FLT", false),
        ("FLTP", false),
        ("DFLT", true),
        ("DFLTP", true),
    ] {
        for reverse_first in [false, true] {
            let seed = format!(
                "{}\nDMOV H00000000 D32\n{} M1081\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K-1 F2",
                seed_words("D", 159, 10),
                if reverse_first { "SET" } else { "RST" }
            );
            let mut parts = Vec::new();
            for (slot, reverse) in [reverse_first, !reverse_first].into_iter().enumerate() {
                let raw = if reverse {
                    if wide {
                        if reverse_first {
                            0xcf00_0000u32
                        } else {
                            0x4f00_0000
                        }
                    } else if reverse_first {
                        0xbf40_0000
                    } else {
                        0x3fc0_0000
                    }
                } else if reverse_first {
                    0
                } else if wide {
                    0x7fff_ffff
                } else {
                    0xffff_ffef
                };
                parts.push(format!(
                    "LD X0\n{} M1081\nDMOV H{raw:08X} D32\n{op} D33E1 D{}F2\n{}",
                    if reverse { "SET" } else { "RST" },
                    161 + slot * 4,
                    flag_snapshot(slot as u16)
                ));
            }
            parts.push("LD X0\nMOV K0 F2".to_owned());
            let observe = format!(
                "{}\nDMOV D32 D240\nMOV E1 D250\nMOV F2 D251",
                read_words("D", 159, 10, 220)
            );
            cases.push(scenario(
                &format!(
                    "same_scan_modes_{op}_reverse_first_{}",
                    u8::from(reverse_first)
                ),
                &seed,
                &parts.join("\n"),
                &observe,
            ));
        }
    }
    for op in ["INT", "INTP", "DINT", "DINTP"] {
        for first_mode in [false, true] {
            let seed = format!(
                "DMOV H3FC00000 D32\n{}\n{} M1081\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K-1 F2",
                seed_words("D", 159, 10),
                if first_mode { "SET" } else { "RST" }
            );
            let mut parts = Vec::new();
            for (slot, mode) in [first_mode, !first_mode].into_iter().enumerate() {
                parts.push(format!(
                    "LD X0\n{} M1081\n{op} D33E1 D{}F2\n{}",
                    if mode { "SET" } else { "RST" },
                    161 + slot * 4,
                    flag_snapshot(slot as u16)
                ));
            }
            parts.push("LD X0\nDMOV HBF400000 D32\nMOV K0 F2".to_owned());
            let observe = format!(
                "{}\nDMOV D32 D240\nMOV E1 D250\nMOV F2 D251",
                read_words("D", 159, 10, 220)
            );
            cases.push(scenario(
                &format!("ignores_M1081_{op}_first_{}", u8::from(first_mode)),
                &seed,
                &parts.join("\n"),
                &observe,
            ));
        }
    }
    assert_eq!(cases.len(), 80);

    let datasets: [(&str, &[u32]); 2] = [
        (
            "zeros_and_fractions",
            &[
                0x0000_0000,
                0x8000_0000,
                0x3f40_0000,
                0xbf40_0000,
                0x3fc0_0000,
                0xbfc0_0000,
            ],
        ),
        (
            "limits_and_exceptional",
            &[
                0x46ff_ff00,
                0x4700_0000,
                0xcf00_0000,
                0x4f00_0000,
                0x7f80_0000,
                0xff80_0000,
                0x7fc0_0000,
                0x7fa1_2345,
            ],
        ),
    ];
    // One guard between wide results; two guards after narrow results. Raw
    // bits cover signed zero, fractions to zero, saturation and exact +2^31.
    for (op, _) in FORMS {
        for (name, raw_values) in datasets {
            let inputs = raw_values
                .iter()
                .enumerate()
                .map(|(i, raw)| format!("DMOV H{raw:08X} D{}", 40 + i * 2))
                .collect::<Vec<_>>()
                .join("\n");
            let count = raw_values.len() as u16;
            let seed = format!(
                "{inputs}\n{}\n{}\nSET M1081\nMOV K1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K1 F2",
                seed_words("D", 159, count * 3 + 2),
                seed_words("D", 80, count * 2)
            );
            let body = raw_values
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    format!(
                        "LD X0\n{op} D{}E1 D{}F2\n{}\nLDI X7\nMOV D1067 D{}\nMOV D1068 D{}",
                        39 + i * 2,
                        159 + i * 3,
                        flag_snapshot(i as u16),
                        80 + i * 2,
                        81 + i * 2
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let observe = format!(
                "{}\n{}\nMOV E1 D280\nMOV F2 D281",
                read_words("D", 159, count * 3 + 2, 200),
                read_words("D", 80, count * 2, 240)
            );
            cases.push(scenario(
                &format!("raw_{op}_{name}"),
                &seed,
                &body,
                &observe,
            ));
        }
    }
    assert_eq!(cases.len(), 96);

    // Exceptional reverse FLT/INT returns before resolving the bad target.
    // The held visit receives live finite source data AND a valid index.
    for (ordinal, (op, _)) in FORMS.into_iter().enumerate() {
        let raw = if op.contains("FLT") {
            if op.ends_with('P') {
                0xff80_0000u32
            } else {
                0x7f80_0000
            }
        } else {
            0x7fc1_2345
        };
        let seed = format!(
            "DMOV H{raw:08X} D32\n{}\nSET M1081\nSET M1020\nSET M1021\nSET M1022\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\n{} M1067\n{} M1068\nMOV H4444 D1067\nMOV K321 D1068",
            seed_words("D", 0, 6),
            if ordinal & 1 == 0 { "RST" } else { "SET" },
            if ordinal & 2 == 0 { "RST" } else { "SET" }
        );
        let body = format!("LD X2\nDMOV H3F400000 D32\nMOV K0 E1\nLD X0\n{op} D32F2 D0E1");
        let observe = format!(
            "{}\nDMOV D32 D240\nMOV E1 D250\nMOV F2 D251",
            read_words("D", 0, 6, 220)
        );
        let mut item = scenario(
            &format!("exception_before_unused_bad_target_{op}"),
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
    assert_eq!(cases.len(), 104);

    for (op, wide) in [
        ("INT", false),
        ("INTP", false),
        ("DINT", true),
        ("DINTP", true),
    ] {
        let target = if wide { "C255E1" } else { "T255E1" };
        let seed = format!(
            "MOV H1357 C198\nMOV H3F80 C199\nDMOV H3FC00000 C200\nDMOV HC0300000 C201\nDMOV H13572468 C253\nDMOV H23456789 C254\nDMOV H3456789A C255\n{}\nSET M1081\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",
            seed_words("T", 252, 4)
        );
        let body = format!("{op} C199F2 {target}\nMOV K-1 E1\nMOV K1 F2");
        let observe = format!(
            "MOV C198 D200\nMOV C199 D201\nDMOV C200 D202\nDMOV C201 D204\nDMOV C253 D206\nDMOV C254 D208\nDMOV C255 D210\n{}\nMOV E1 D250\nMOV F2 D251",
            read_words("T", 252, 4, 220)
        );
        cases.push(scenario(
            &format!(
                "counter_transition_{op}_C199_to_C200{}",
                if wide {
                    ""
                } else {
                    "_native_extension_effective_C200"
                }
            ),
            &seed,
            &body,
            &observe,
        ));

        let seed = format!(
            "MOV H1357 C197\nMOV H2468 C198\nMOV H369A C199\nDMOV H7F800000 C200\nDMOV HC0300000 C201\nDMOV H55667788 C202\nMOV H5AA5 T253\nDMOV H40200000 T254\n{}\nRST M1081\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K1 E3\nMOV H1234 F3\nMOV K1 E4\nMOV H5678 F4",
            seed_words("D", 4996, 4)
        );
        let body = format!(
            "{op} C200F2 C199E1\n{op} T253E3 D4997E4\nMOV K{} E1\nMOV K1 F2",
            if wide { 1 } else { -1 }
        );
        let observe = format!(
            "MOV C197 D200\nMOV C198 D201\nMOV C199 D202\nDMOV C200 D204\nDMOV C201 D206\nDMOV C202 D208\n{}\n{}\nMOV E1 D250\nMOV F2 D251",
            read_words("T", 253, 3, 220),
            read_words("D", 4996, 4, 230)
        );
        cases.push(scenario(
            &format!(
                "natural_source_{op}_counter_target_and_last_timer_pair{}",
                if wide {
                    ""
                } else {
                    "_native_extension_base_C200"
                }
            ),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 112);

    // Common ES2/SS2 pair-safe end. Capture the result before restoring a
    // known finite/raw integer source for the next visit to the same storage.
    for (ordinal, (op, wide)) in FORMS.into_iter().enumerate() {
        let reverse = op.contains("FLT") && ordinal % 2 == 1;
        let forward = op.contains("FLT") && !reverse;
        let initial = if forward {
            if wide { 0x7fff_ffffu32 } else { 0xffff_ffef }
        } else {
            0x3fc0_0000
        };
        let second = if forward { 17u32 } else { 0xc030_0000 };
        let alternate = if forward { 0u32 } else { 0xbf40_0000 };
        let seed = format!(
            "MOV H5566 D4996\nMOV H7788 D4997\nDMOV H{initial:08X} D4998\nDMOV H{second:08X} D32\nDMOV H5AA5A55A D50\n{}\n{} M1081\nMOV K1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K-1 F2\nMOV K-1 E3\nMOV H1234 F3",
            seed_words("D", 159, 6),
            if reverse { "SET" } else { "RST" }
        );
        let body = format!(
            "{op} D4997E1 D161F2\n{op} D33E3 D4997E1\nDMOV D4998 D50\nDMOV H{alternate:08X} D4998"
        );
        let observe = format!(
            "{}\n{}\nDMOV D50 D240\nMOV E1 D250\nMOV F2 D251\nMOV E3 D252",
            read_words("D", 4996, 4, 200),
            read_words("D", 159, 6, 220)
        );
        cases.push(scenario(
            &format!("common_pair_end_{op}_reverse_{}", u8::from(reverse)),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 120);

    for (ordinal, (op, wide)) in FORMS.into_iter().enumerate() {
        let reverse = op.contains("FLT") && ordinal % 2 == 1;
        let forward = op.contains("FLT") && !reverse;
        let raw = if forward {
            if wide { 0x8000_0000u32 } else { 17 }
        } else if wide && op.contains("FLT") {
            0x4f00_0000
        } else {
            0xc030_0000
        };
        let (modifier, alias) = if wide { ("F2", 1185) } else { ("E1", 1182) };
        let aliases = if wide {
            "MOV H5566 E2\nMOV K1 F2\nMOV H2468 E3\nMOV H7788 F3"
        } else {
            "MOV K1 E1\nMOV H6A5A F1\nMOV H7788 E2"
        };
        let seed = format!(
            "DMOV H{raw:08X} D32\n{}\nDMOV H{raw:08X} D63\n{aliases}\n{} M1081\nMOV K-1 E4\nMOV H5678 F4\nMOV K0 E5\nMOV H1234 F5",
            seed_words("D", 62, 5),
            if reverse { "SET" } else { "RST" }
        );
        let body = format!(
            "LDI X7\nMOV K1 {modifier}\nLD X0\n{op} D32E5 D{}{modifier}\n{op} D64E4 D65E4",
            alias - 1
        );
        let alias_read = if wide {
            "MOV E2 D240\nMOV F2 D241\nMOV E3 D242\nMOV F3 D243"
        } else {
            "MOV E1 D240\nMOV F1 D241\nMOV E2 D242"
        };
        let observe = format!(
            "{}\n{alias_read}\nDMOV D32 D250\nMOV E4 D252\nMOV E5 D253",
            read_words("D", 62, 5, 220)
        );
        cases.push(scenario(
            &format!("self_index_and_source_overlap_{op}_{modifier}"),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 128);

    cases.push(scenario(
        "control_FLT_mps_mode_roundtrip",
        &format!("DMOV H00000011 D32\n{}\nRST M1081\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 E3\nMOV H1234 F3", seed_words("D", 159, 9)),
        "MPS\nRST M1081\nMRD\nFLT D32E1 D160F2\nMRD\nSET M1081\nMRD\nFLT D160F2 D164E3\nMPP\nOUT Y10",
        &read_words("D", 159, 9, 220),
    ));
    cases.push(scenario(
        "control_DFLTP_for_same_pc_mode_and_address_changes",
        &format!("DMOV H00000011 D32\nMOV K0 D30\n{}\nRST M1081\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2", seed_words("D", 159, 9)),
        "LDI X7\nMOV K0 D30\nFOR K3\nLD= D30 K1\nSET M1081\nDMOV H3FC00000 D32\nLD<> D30 K1\nRST M1081\nDMOV H00000011 D32\nLDI X7\nMOV D30 F2\nLD X0\nDFLTP D32E1 D160F2\nLDI X7\nINC D30\nNEXT",
        &format!("{}\nMOV D30 D240\nDMOV D32 D242\nMOV F2 D250", read_words("D", 159, 9, 220)),
    ));
    cases.push(scenario(
        "control_INT_cj_skips_false_visits",
        &format!("DMOV H3FC00000 D32\n{}\nSET M1081\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 D40", seed_words("D", 159, 5)),
        "LDI X0\nCJ P10\nLD X0\nINT D32E1 D160F2\nP10\nLDI X7\nMOV K123 D40",
        &format!("{}\nMOV D40 D240", read_words("D", 159, 5, 220)),
    ));
    cases.push(scenario(
        "control_DINTP_mps_in_place_source",
        "MOV H5566 D31\nDMOV H3FC00000 D32\nMOV H7788 D34\nRST M1081\nMOV K0 E1\nMOV H6A5A F1",
        "MPS\nDINTP D32E1 D32E1\nMRD\nSET M1081\nMPP\nOUT Y10",
        &read_words("D", 31, 4, 220),
    ));
    check_cases(&cases, 132);
    cases
}

pub(super) fn model_cases(target: Target) -> Vec<Case> {
    let last: u16 = match target {
        Target::Es2 => 9999,
        Target::Ss2 => 4999,
    };
    let pair = last - 1;
    let before = last - 2;
    let mut cases = Vec::new();
    for (op, _) in FORMS {
        let (seed_data, mode, source, dest, advance) = match op {
            "FLT" => (
                format!("MOV HFFEF D{last}"),
                "RST",
                format!("D{last}F1"),
                "D20E1".to_owned(),
                format!("MOV K0 D{last}"),
            ),
            "FLTP" => (
                "DMOV H3FC00000 D20".to_owned(),
                "SET",
                "D20E1".to_owned(),
                format!("D{last}F1"),
                "DMOV HBF400000 D20".to_owned(),
            ),
            "DFLT" => (
                format!("DMOV H7FFFFFFF D{pair}"),
                "RST",
                format!("D{last}E2"),
                "D20F1".to_owned(),
                format!("DMOV HFFFFFFEF D{pair}"),
            ),
            "DFLTP" => (
                "DMOV H3FC00000 D20".to_owned(),
                "SET",
                "D20F1".to_owned(),
                format!("D{last}E2"),
                "DMOV HBF400000 D20".to_owned(),
            ),
            "INT" => (
                format!("DMOV H7F800000 D{pair}"),
                "SET",
                format!("D{last}E2"),
                "D20F1".to_owned(),
                format!("DMOV H7FC00000 D{pair}"),
            ),
            "INTP" => (
                "DMOV H3FC00000 D20".to_owned(),
                "RST",
                "D20E1".to_owned(),
                format!("D{last}F1"),
                "DMOV HBF400000 D20".to_owned(),
            ),
            "DINT" => (
                format!("DMOV H4F000000 D{pair}"),
                "SET",
                format!("D{last}E2"),
                "D20F1".to_owned(),
                format!("DMOV HCF000000 D{pair}"),
            ),
            "DINTP" => (
                "DMOV HFF800000 D20".to_owned(),
                "RST",
                "D20F1".to_owned(),
                format!("D{last}E2"),
                "DMOV H7FC00000 D20".to_owned(),
            ),
            _ => unreachable!(),
        };
        let seed = format!(
            "{}\n{}\n{seed_data}\n{mode} M1081\nMOV K0 E1\nMOV K0 F1\nMOV K-1 E2\nMOV H6A5A F2",
            seed_words("D", before, 3),
            seed_words("D", 19, 5)
        );
        let body = format!("{op} {source} {dest}\n{advance}");
        let observe = format!(
            "{}\n{}\nMOV E1 D250\nMOV F1 D251\nMOV E2 D252\nMOV F2 D253",
            read_words("D", before, 3, 220),
            read_words("D", 19, 5, 230)
        );
        cases.push(scenario(
            &format!("model_{}_last_access_{op}", target.name()),
            &seed,
            &body,
            &observe,
        ));
    }
    for reverse in [false, true] {
        let seed_data = if reverse {
            "DMOV HC0300000 D20".to_owned()
        } else {
            format!("MOV HFFEF D{last}")
        };
        let seed = format!(
            "{}\n{}\n{seed_data}\n{} M1081",
            seed_words("D", before, 3),
            seed_words("D", 19, 5),
            if reverse { "SET" } else { "RST" }
        );
        let body = if reverse {
            format!("FLT D20 D{last}\nDMOV H3F400000 D20")
        } else {
            format!("FLT D{last} D20\nMOV K0 D{last}")
        };
        let observe = format!(
            "{}\n{}",
            read_words("D", before, 3, 220),
            read_words("D", 19, 5, 230)
        );
        cases.push(scenario(
            &format!(
                "model_{}_plain_FLT_{}",
                target.name(),
                if reverse {
                    "reverse_word_target"
                } else {
                    "forward_word_source"
                }
            ),
            &seed,
            &body,
            &observe,
        ));
    }
    for reverse in [false, true] {
        let seed_data = if reverse {
            format!("DMOV H3FC00000 D{pair}")
        } else {
            format!("MOV H0011 D{last}")
        };
        let seed = format!(
            "{}\n{seed_data}\n{} M1081\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",
            seed_words("D", before, 3),
            if reverse { "SET" } else { "RST" }
        );
        let body = if reverse {
            format!("FLT D{last}E1 D{last}F2")
        } else {
            format!("FLT D{last}F2 D{last}E1")
        };
        let observe = format!(
            "{}\nMOV E1 D250\nMOV F2 D251",
            read_words("D", before, 3, 220)
        );
        cases.push(scenario(
            &format!(
                "model_{}_FLT_overlap_last_word_pair_reverse_{}",
                target.name(),
                u8::from(reverse)
            ),
            &seed,
            &body,
            &observe,
        ));
    }
    check_cases(&cases, 12);
    cases
}
