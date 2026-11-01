//! D-only indexed SQR/DSQR and legal unmodified K/H constants.
//! Negative sources bypass target/index access with E1B at header+1.
//! Zero/borrow are refreshed; carry is preserved. All end spans are bounded.
use super::{Case, Target, case};
use std::collections::BTreeSet;

const FORMS: [(&str, bool); 4] = [
    ("SQR", false),
    ("SQRP", false),
    ("DSQR", true),
    ("DSQRP", true),
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
        "LD X1\nMOV H55AA E0\nMOV H1234 F0\nSET M1020\nSET M1021\nSET M1022\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068",
        seed, gate, body,
        "LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24",
        "LDI X7\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305\nMOV K1234 D306",
        observe, "END",
    ].into_iter().filter(|section| !section.is_empty()).collect::<Vec<_>>().join("\n");
    assert!(
        source.lines().count() <= 512,
        "oversized indexed root case {name}"
    );
    // Every emitted instruction in this corpus is at most nine PLC words.
    assert!(
        source.lines().count() * 9 <= 0x1ef0,
        "installed program budget: {name}"
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
        "missing indexed root action in {name}"
    );
    case(&format!("indexed_root_{name}"), &source, stages())
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
        assert!(actual.contains(op), "missing root spelling {op}");
    }
}

pub(super) fn cases() -> Vec<Case> {
    let mut cases = Vec::new();

    // All sixteen suffixes serve both source and destination at both widths.
    // Normal/P spellings are balanced; adjacent canonical aliases poison an
    // accidental wide modifier read. Moving indices remain fully in range.
    for bank in ['E', 'F'] {
        for index in 0..8u16 {
            let modifier = format!("{bank}{index}");
            let pair = index_base(index);
            let alias = pair + u16::from(bank == 'F');
            let companion = pair + u16::from(bank == 'E');
            let delta = i32::from(index % 4) - 2;
            for (family, wide) in [("SQR", false), ("DSQR", true)] {
                let pulse = (index + u16::from(bank == 'F')) % 2 == 1;
                let op = format!("{family}{}", if pulse { "P" } else { "" });
                let inputs = (0..13)
                    .map(|i| {
                        format!(
                            "MOV H{:04X} D{}",
                            [0, 1, 4, 9, 16, 25, 0x7fff, 0][i % 8],
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
                let seed = format!(
                    "{inputs}\n{}\n{} M1022\nMOV H5AA5 D{companion}\n{poison}\nMOV K{delta} D{alias}",
                    seed_words("D", 155, 16),
                    if bank == 'E' { "RST" } else { "SET" }
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
    assert_eq!(cases.len(), 32);

    // Constants remain unmodified. Decimal and hex sets independently cover
    // signed interpretation, perfect roots, near-square rounding and E1B.
    for (op, wide) in FORMS {
        for hex in [false, true] {
            let values: &[&str] = match (wide, hex) {
                (false, false) => &["K0", "K1", "K2", "K144", "K32767", "K-1"],
                (false, true) => &["H0000", "H0001", "H7FF8", "H7FF9", "H7FFA", "H8000"],
                (true, false) => &["K0", "K1", "K2", "K2147395600", "K2147483647", "K-1"],
                (true, true) => &[
                    "H00000000",
                    "H00000001",
                    "H7FFEA80F",
                    "H7FFEA810",
                    "H7FFEA811",
                    "H80000000",
                ],
            };
            let count = values.len() as u16;
            let seed = format!(
                "{}\n{}\n{} M1022\nMOV K1 E1\nMOV H6A5A F1",
                seed_words("D", 159, count * 3 + 2),
                seed_words("D", 80, count * 2),
                if hex { "RST" } else { "SET" }
            );
            let body = values
                .iter()
                .enumerate()
                .map(|(i, value)| {
                    format!(
                        "LD X0\n{op} {value} D{}E1\n{}\nLDI X7\nMOV D1067 D{}\nMOV D1068 D{}",
                        159 + i * 3,
                        flag_snapshot(i as u16),
                        80 + i * 2,
                        81 + i * 2
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let observe = format!(
                "{}\n{}\nMOV E1 D270\nMOV F1 D271",
                read_words("D", 159, count * 3 + 2, 200),
                read_words("D", 80, count * 2, 240)
            );
            cases.push(scenario(
                &format!("literal_dataset_{op}_{}", if hex { "H" } else { "K" }),
                &seed,
                &body,
                &observe,
            ));
        }
    }
    assert_eq!(cases.len(), 40);

    // D-source results/flags are captured per site with carry initially OFF
    // and ON. The value just below46340^2 distinguishes double precision
    // from rounding the operand/root prematurely to binary32.
    for (op, wide) in FORMS {
        for carry in [false, true] {
            let values: &[i32] = if wide {
                &[0, 1, 2147395599, 2147395600, 2147395601, i32::MIN]
            } else {
                &[0, 1, 32760, 32761, 32762, i16::MIN as i32]
            };
            let stride = if wide { 2 } else { 1 };
            let mov = if wide { "DMOV" } else { "MOV" };
            let inputs = values
                .iter()
                .enumerate()
                .map(|(i, value)| format!("{mov} K{value} D{}", 40 + i * stride))
                .collect::<Vec<_>>()
                .join("\n");
            let count = values.len() as u16;
            let seed = format!(
                "MOV H5566 D39\n{inputs}\nMOV H7788 D{}\n{}\n{}\n{} M1022\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K1 F2",
                40 + values.len() * stride,
                seed_words("D", 159, count * 3 + 2),
                seed_words("D", 80, count * 2),
                if carry { "SET" } else { "RST" }
            );
            let body = values
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    format!(
                        "LD X0\n{op} D{}E1 D{}F2\n{}\nLDI X7\nMOV D1067 D{}\nMOV D1068 D{}",
                        41 + i * stride,
                        159 + i * 3,
                        flag_snapshot(i as u16),
                        80 + i * 2,
                        81 + i * 2
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let observe = format!(
                "{}\n{}\nMOV D39 D270\nMOV D{} D271\nMOV E1 D272\nMOV F2 D273",
                read_words("D", 159, count * 3 + 2, 200),
                read_words("D", 80, count * 2, 240),
                40 + values.len() * stride
            );
            cases.push(scenario(
                &format!("D_dataset_{op}_carry_{}", u8::from(carry)),
                &seed,
                &body,
                &observe,
            ));
        }
    }
    assert_eq!(cases.len(), 48);

    for (ordinal, (op, wide)) in FORMS.into_iter().enumerate() {
        let mov = if wide { "DMOV" } else { "MOV" };
        let negative = if wide { i32::MIN } else { i16::MIN as i32 };
        let seed = format!(
            "{mov} K{negative} D32\n{}\nSET M1020\nSET M1021\n{} M1022\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K-1 F2\n{} M1067\n{} M1068\nMOV H4444 D1067\nMOV K321 D1068",
            seed_words("D", 0, 6),
            if ordinal % 2 == 0 { "RST" } else { "SET" },
            if ordinal & 1 == 0 { "RST" } else { "SET" },
            if ordinal & 2 == 0 { "RST" } else { "SET" }
        );
        let body = format!("LD X2\n{mov} K145 D32\nMOV K0 E1\nLD X0\n{op} D33F2 D0E1");
        let observe = format!(
            "{}\n{mov} D32 D240\nMOV E1 D250\nMOV F2 D251",
            read_words("D", 0, 6, 220)
        );
        let mut item = scenario(
            &format!("negative_D_before_unused_bad_target_{op}"),
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

        let negative_literal = if op.ends_with('P') {
            if wide { "H80000000" } else { "H8000" }
        } else {
            "K-1"
        };
        let seed = format!(
            "{}\nMOV K-1 E1\nMOV H6A5A F1\nMOV K0 D50\nMOV K0 D51\n{} M1022",
            seed_words("D", 0, 6),
            if wide { "RST" } else { "SET" }
        );
        let body = format!(
            "LD X0\nMOV K-1 E1\n{op} {negative_literal} D0E1\n{}\nLDI X7\nMOV D1067 D50\nMOV D1068 D51\nMOV K0 E1\nLD X0\n{op} H90 D0E1\n{}",
            flag_snapshot(0),
            flag_snapshot(1)
        );
        let observe = format!(
            "{}\nMOV D50 D240\nMOV D51 D241\nMOV E1 D250",
            read_words("D", 0, 6, 220)
        );
        cases.push(scenario(
            &format!("negative_constant_then_same_scan_valid_{op}"),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 56);

    for (op, wide) in FORMS {
        let mov = if wide { "DMOV" } else { "MOV" };
        let value = if wide {
            if op.ends_with('P') {
                i32::MAX
            } else {
                2147395600
            }
        } else if op.ends_with('P') {
            32767
        } else {
            32761
        };
        let (modifier, alias) = if wide { ("F2", 1185) } else { ("E1", 1182) };
        let aliases = if wide {
            "MOV H5566 E2\nMOV K1 F2\nMOV H2468 E3\nMOV H7788 F3"
        } else {
            "MOV K1 E1\nMOV H6A5A F1\nMOV H7788 E2"
        };
        let seed = format!("{mov} K{value} D32\n{aliases}\nMOV K0 E5\nMOV H1234 F5");
        let body = format!(
            "LDI X7\nMOV K1 {modifier}\nLD X0\n{op} D32E5 D{}{modifier}\n{mov} K145 D32",
            alias - 1
        );
        let alias_read = if wide {
            "MOV E2 D220\nMOV F2 D221\nMOV E3 D222\nMOV F3 D223"
        } else {
            "MOV E1 D220\nMOV F1 D221\nMOV E2 D222"
        };
        let observe = format!("{alias_read}\n{mov} D32 D240\nMOV E5 D250\nMOV F5 D251");
        cases.push(scenario(
            &format!("self_index_fixed_result_address_{op}_{modifier}"),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 60);

    for (op, wide) in FORMS {
        let mov = if wide { "DMOV" } else { "MOV" };
        let value = if wide {
            if op.ends_with('P') { 65535 } else { 65536 }
        } else if op.ends_with('P') {
            32767
        } else {
            32761
        };
        let seed = format!(
            "{}\n{mov} K{value} D63\nMOV K-1 E1\nMOV H6A5A F1",
            seed_words("D", 62, 5)
        );
        let body = format!("{op} D64E1 D{}E1", if wide { 65 } else { 64 });
        let observe = format!("{}\nMOV E1 D250\nMOV F1 D251", read_words("D", 62, 5, 220));
        cases.push(scenario(
            &format!(
                "source_result_{}_{op}",
                if wide {
                    "shifted_pair_overlap"
                } else {
                    "in_place"
                }
            ),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 64);

    for (op, wide) in FORMS {
        let mov = if wide { "DMOV" } else { "MOV" };
        let stride = if wide { 2 } else { 1 };
        let inputs = [0, 2, 144]
            .into_iter()
            .enumerate()
            .map(|(i, value)| format!("{mov} K{value} D{}", 40 + i * stride))
            .collect::<Vec<_>>()
            .join("\n");
        let seed = format!(
            "{inputs}\n{}\nMOV K0 E1\nMOV H6A5A F1",
            seed_words("D", 159, 8)
        );
        let body = (0..3)
            .map(|i| {
                format!(
                    "LD X0\nMOV K{} E1\n{op} D40E1 D160E1\n{}",
                    i * stride,
                    flag_snapshot(i as u16)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let observe = format!("{}\nMOV E1 D250\nMOV F1 D251", read_words("D", 159, 8, 220));
        cases.push(scenario(
            &format!("same_scan_three_offsets_{op}"),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 68);

    cases.push(scenario(
        "control_SQR_mps_live_source_updates",
        &format!("MOV K0 D32\n{}\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2", seed_words("D", 159, 7)),
        "MPS\nMOV K0 D32\nMRD\nSQR D32E1 D160F2\nMRD\nMOV K145 D32\nMRD\nSQR D32E1 D164F2\nMPP\nOUT Y10",
        &format!("{}\nMOV D32 D240", read_words("D", 159, 7, 220)),
    ));
    cases.push(scenario(
        "control_SQRP_for_same_pc_true_false_true",
        "MOV K0 D30\nMOV H5AA5 D160\nMOV H1111 D40\nMOV H2222 D41\nMOV H3333 D42\nMOV K0 E2\nMOV K0 F2\nMOV K0 E3\nMOV H6A5A F3",
        "LDI X7\nMOV K0 D30\nFOR K3\nLDI X7\nMOV D30 E3\nLD= D30 K0\nOR= D30 K2\nAND X0\nSQRP D30E2 D160F2\nLDI X7\nMOV D160 D40E3\nINC D30\nNEXT",
        "MOV D160 D220\nMOV D40 D221\nMOV D41 D222\nMOV D42 D223\nMOV D30 D240\nMOV E3 D250",
    ));
    cases.push(scenario(
        "control_DSQR_cj_preserves_skipped_flags",
        &format!("DMOV K2147395600 D32\n{}\nMOV K0 D50\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2", seed_words("D", 159, 5)),
        "LDI X0\nCJ P10\nLD X0\nDSQR D32E1 D160F2\nDMOV K2147483647 D32\nP10\nLDI X7\nMOV K123 D50",
        &format!("{}\nDMOV D32 D240\nMOV D50 D242", read_words("D", 159, 5, 220)),
    ));
    cases.push(scenario(
        "control_DSQRP_mps_changed_target",
        &format!(
            "{}\nRST M1022\nMOV H7BBB E2\nMOV K0 F2",
            seed_words("D", 159, 6)
        ),
        "MPS\nDSQRP H7FFEA810 D160F2\nMRD\nMOV K1 F2\nMPP\nOUT Y10",
        &format!("{}\nMOV E2 D250\nMOV F2 D251", read_words("D", 159, 6, 220)),
    ));
    check_cases(&cases, 72);
    cases
}

pub(super) fn model_cases(target: Target) -> Vec<Case> {
    let last: u16 = match target {
        Target::Es2 => 9999,
        Target::Ss2 => 4999,
    };
    let mut cases = Vec::new();
    for (op, wide) in FORMS {
        let offset = if wide { -1 } else { 0 };
        let start = if wide { last - 1 } else { last };
        let mov = if wide { "DMOV" } else { "MOV" };
        let value = if wide { 2147395600 } else { 32761 };
        let seed = format!(
            "{}\n{}\n{mov} K{value} D{start}\nMOV K{offset} E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",
            seed_words("D", last - 3, 4),
            seed_words("D", 19, 5)
        );
        let body = format!("{op} D{last}E1 D20F2\n{mov} K2 D{start}");
        let observe = format!(
            "{}\n{}\nMOV E1 D250\nMOV F2 D251",
            read_words("D", last - 3, 4, 220),
            read_words("D", 19, 5, 230)
        );
        cases.push(scenario(
            &format!("model_{}_last_source_{op}", target.name()),
            &seed,
            &body,
            &observe,
        ));

        let constant = if wide {
            if op.ends_with('P') {
                "H7FFEA810"
            } else {
                "K2147395600"
            }
        } else if op.ends_with('P') {
            "H90"
        } else {
            "K144"
        };
        let seed = format!(
            "{}\nRST M1022\nMOV K{offset} E1\nMOV H6A5A F1",
            seed_words("D", last - 3, 4)
        );
        let body = format!("{op} {constant} D{last}E1\nMOV K{} E1", offset - 1);
        let observe = format!(
            "{}\nMOV E1 D250\nMOV F1 D251",
            read_words("D", last - 3, 4, 220)
        );
        cases.push(scenario(
            &format!("model_{}_literal_to_last_target_{op}", target.name()),
            &seed,
            &body,
            &observe,
        ));
    }
    check_cases(&cases, 8);
    cases
}
