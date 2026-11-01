//! Indexed R arithmetic: finite F literals and raw D pairs retain IEEE bits.
//! Ordered NaN failures, DDIVR divisor precedence and its four-word padding
//! are observed separately. Overflow and signaling-NaN probes remain explicit;
//! inclusion is not a claim that a reference process completes them.
use super::{Case, Target, case};
use std::collections::BTreeSet;

const FORMS: [&str; 8] = [
    "DADDR", "DADDRP", "DSUBR", "DSUBRP", "DMULR", "DMULRP", "DDIVR", "DDIVRP",
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

fn recovery_stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (
            vec![("X1", true), ("X0", false), ("X2", false), ("X7", false)],
            1,
        ),
        (vec![("X1", false), ("X0", true)], 1),
        (vec![("X2", true)], 1),
        (vec![("X0", false), ("X2", false)], 1),
        (vec![("X0", true)], 1),
    ]
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
        "LD X0\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\n{}\n{op} {left} {right} D{}E3\nOUT Y{:o}\nINC D{}\n{}\nLDI X7\nMOV D1067 D{}\nMOV D1068 D{}",
        set_flags(slot % 8),
        159 + slot * 3,
        128 + slot,
        120 + slot,
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
        "oversized indexed real case {name}"
    );
    // DDIVR includes four encoder padding words in addition to its thirteen-word body.
    assert!(
        source.lines().count() * 17 <= 0x1ef0,
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
        "missing indexed real action in {name}"
    );
    for line in source.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.first().is_some_and(|op| FORMS.contains(op)) {
            assert_eq!(fields.len(), 4, "R field count in {name}: {line}");
            assert!(
                fields[1..3]
                    .iter()
                    .all(|operand| operand.starts_with('D') || operand.starts_with('F'))
                    && fields[3].starts_with('D'),
                "R operands must be F literals/D sources and a D target in {name}: {line}"
            );
        }
    }
    case(&format!("indexed_real_{name}"), &source, stages())
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
            for family in ["DADDR", "DSUBR", "DMULR", "DDIVR"] {
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
    let raw_sets: [(&str, &[(u32, u32)]); 5] = [
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
                // Analogues of the DE family's nonresponsive finite vectors.
                // These are new R probes, not a claim of the same response.
                (0x7f7fffff, 0x7f7fffff),
                (0x7f7fffff, 0xff7fffff),
                (0x7f7fffff, 0x40000000),
                (0xff7fffff, 0x40000000),
                (0x7f7fffff, 0x3f000000),
                // The DE hard-div probe used these same raw source values.
                (0x7f7fffff, 0x00800000),
            ],
        ),
        (
            "infinity_invalid_arithmetic_results",
            &[
                (0x7f800000, 0x3f800000),
                (0xff800000, 0x3f800000),
                (0x7f800000, 0xff800000),
                (0x7f800000, 0x7f800000),
                (0x7f800000, 0x00000000),
                (0xff800000, 0x80000000),
                (0x3f800000, 0x7f800000),
                (0xbf800000, 0xff800000),
            ],
        ),
        (
            "quiet_nan_both_signs_each_source",
            &[
                (0x7fc12345, 0x3f800000),
                (0xffc12345, 0x3f800000),
                (0x3f800000, 0x7fc12345),
                (0x3f800000, 0xffc12345),
                (0x7fc00001, 0xffc00002),
                (0xffc00001, 0x7fc00002),
            ],
        ),
        (
            "signaling_nan_both_signs_each_source",
            &[
                (0x7f812345, 0x3f800000),
                (0xff812345, 0x3f800000),
                (0x3f800000, 0x7f812345),
                (0x3f800000, 0xff812345),
                (0xff800002, 0x7fc00001),
                (0x7f800001, 0xff800002),
            ],
        ),
    ];
    for op in FORMS {
        for (label, values) in raw_sets {
            let literal_overflow = label == "large_finite_overflow";
            let count = values.len() as u16 + u16::from(literal_overflow);
            let mut seed = format!(
                "{}\n{}\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K1 F2\nMOV K1 E3\nMOV H1234 F3",
                seed_words(159, count * 3 + 1),
                seed_words(80, count * 2)
            );
            let mut body = Vec::new();
            for slot in 0..count {
                seed.push_str(&format!("\nMOV K0 D{}", 120 + slot));
            }
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
            if literal_overflow {
                let (left, right) = match op.trim_end_matches('P') {
                    "DADDR" => ("F3.4028235E+38", "F3.4028235E+38"),
                    "DSUBR" => ("F3.4028235E+38", "F-3.4028235E+38"),
                    "DMULR" => ("F3.4028235E+38", "F2.0"),
                    "DDIVR" => ("F3.4028235E+38", "F0.5"),
                    _ => unreachable!(),
                };
                body.push(result_site(op, left, right, values.len() as u16));
            }
            let observe = format!(
                "{}\n{}\n{}\n{}",
                read_words(40, values.len() as u16 * 4, 200),
                read_words(159, count * 3 + 1, 240),
                read_words(80, count * 2, 270),
                read_words(120, count, 286)
            );
            cases.push(scenario(
                &format!("{label}_{op}"),
                &seed,
                &body.join("\n"),
                &observe,
            ));
        }
    }
    assert_eq!(cases.len(), 104);

    // F1 is a literal while D36F2 uses an index WORD. Scientific notation's E
    // is part of the literal; signed zero and subnormal encodings stay intact.
    for op in FORMS {
        let operands = [
            ("F1.200E+0", "D32E1"),
            ("D32E1", "F1.200E+0"),
            ("F-0.0", "F0.0"),
            ("F1.5", "F0.5"),
            ("F16777217", "F16777216"),
            ("F-2.5e-1", "D36F2"),
            ("D40E1", "F1E-45"),
            ("F-0.0", "D44F2"),
            ("D48E1", "F-0.0"),
            ("F1", "D52F2"),
        ];
        let count = operands.len() as u16;
        let mut seed = format!(
            "{}\n{}\n{}\nDMOV H400CCCCD D32\nDMOV HBF000000 D36\nDMOV H00000001 D40\nDMOV H3F800000 D44\nDMOV H3F800000 D48\nDMOV H3F800000 D52\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K1 E3\nMOV H1234 F3",
            seed_words(31, 24),
            seed_words(159, count * 3 + 1),
            seed_words(80, count * 2)
        );
        for slot in 0..count {
            seed.push_str(&format!("\nMOV K0 D{}", 120 + slot));
        }
        let body = operands
            .iter()
            .enumerate()
            .map(|(slot, (left, right))| result_site(op, left, right, slot as u16))
            .collect::<Vec<_>>()
            .join("\n");
        let observe = format!(
            "{}\n{}\n{}\n{}",
            read_words(31, 24, 200),
            read_words(159, count * 3 + 1, 230),
            read_words(80, count * 2, 264),
            read_words(120, count, 286)
        );
        cases.push(scenario(
            &format!("literal_scientific_signed_zero_and_raw_{op}"),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 112);

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
    assert_eq!(cases.len(), 120);

    for op in ["DADDR", "DSUBRP", "DMULR", "DDIVRP"] {
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
    assert_eq!(cases.len(), 124);

    // First-three handlers must not resolve an invalid right index after a
    // left NaN, nor the target after a right NaN. Recovery changes the source
    // and every skipped address together before a successful/inactive visit.
    for op in ["DADDR", "DADDRP", "DSUBR", "DSUBRP", "DMULR", "DMULRP"] {
        for left_error in [true, false] {
            let (left, right, right_base, right_offset, label) = if left_error {
                (
                    0x7fc01234u32,
                    0x40000000u32,
                    10,
                    -11,
                    "left_nan_skips_bad_right_and_target_pc1",
                )
            } else {
                (
                    0x3f800000,
                    0xffc05678,
                    48,
                    0,
                    "right_nan_skips_bad_target_pc5",
                )
            };
            let seed = format!(
                "{}\nDMOV H{left:08X} D32\nDMOV H{right:08X} D{right_base}\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K{right_offset} F2\nMOV K-1 E3\nMOV H1234 F3\nMOV K0 D60\nMOV K0 D62\nMOV H55AA D61\n{}",
                seed_words(0, 4),
                set_flags(if left_error { 5 } else { 3 })
            );
            let body = format!(
                "LD X2\nDMOV H40C00000 D32\nDMOV H40000000 D{right_base}\nMOV K0 F2\nMOV K0 E3\nLD X0\n{op} D32E1 D{right_base}F2 D0E3\nOUT Y10\nINC D60\nMOV K4321 D61\nINCP D62"
            );
            let observe = format!(
                "{}\nDMOV D32 D230\nDMOV D{right_base} D232\nMOV D60 D240\nMOV D61 D241\nMOV D62 D242\nMOV E1 D250\nMOV F2 D251\nMOV E3 D252",
                read_words(0, 4, 220)
            );
            let mut entry = scenario(
                &format!("staged_{label}_live_recovery_{op}"),
                &seed,
                &body,
                &observe,
            );
            entry.stages = recovery_stages();
            cases.push(entry);
        }
    }
    assert_eq!(cases.len(), 136);

    // DDIVR reads both sources. Zero/subnormal wins over left NaN at PC+9;
    // a NaN with a normal divisor takes the encoder's PC+17 padding path.
    // OUT/INC/INCP immediately following it are real executable successors.
    let errors = [
        ("nan_positive_zero_e19_next13", 0x7fc01234u32, 0x00000000u32),
        ("nan_negative_zero_e19_next13", 0xffc01234, 0x80000000),
        ("nan_positive_subnormal_e19_next13", 0x7fc01234, 0x00000001),
        ("nan_negative_subnormal_e19_next13", 0xffc01234, 0x80000001),
        ("left_nan_e1a_next17", 0x7fc01234, 0x40000000),
        ("right_nan_e1a_next17", 0x3f800000, 0xffc01234),
    ];
    for op in ["DDIVR", "DDIVRP"] {
        for (slot, (label, left, right)) in errors.into_iter().enumerate() {
            let seed = format!(
                "{}\nDMOV H{left:08X} D32\nDMOV H{right:08X} D48\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K-1 E3\nMOV H1234 F3\nMOV K0 D60\nMOV K0 D62\nMOV H55AA D61\n{}",
                seed_words(0, 4),
                set_flags(slot as u16 + 1)
            );
            let body = format!(
                "LD X2\nDMOV H40C00000 D32\nDMOV H40000000 D48\nMOV K0 E3\nLD X0\n{op} D32E1 D48F2 D0E3\nOUT Y10\nINC D60\nMOV K4321 D61\nINCP D62"
            );
            let observe = format!(
                "{}\nDMOV D32 D230\nDMOV D48 D232\nMOV D60 D240\nMOV D61 D241\nMOV D62 D242\nMOV E1 D250\nMOV F2 D251\nMOV E3 D252",
                read_words(0, 4, 220)
            );
            let mut entry = scenario(
                &format!("divide_precedence_padding_{label}_live_recovery_{op}"),
                &seed,
                &body,
                &observe,
            );
            entry.stages = recovery_stages();
            cases.push(entry);
        }
    }
    assert_eq!(cases.len(), 148);

    cases.push(scenario(
        "control_DADDR_mps_with_ordinary_DDIVR_nan_successor",
        &format!("{}\nDMOV H3FC00000 D32\nDMOV H7FC01234 D48\nMOV K0 D60\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2", seed_words(159, 12)),
        "MPS\nDADDR D32E1 F2 D160F2\nMRD\nDMOV HC0400000 D32\nMRD\nDADDR F3 D32E1 D164F2\nMPP\nOUT Y10\nLD X0\nDDIVR D48 F1 D168\nOUT Y11\nINC D60",
        &format!("{}\nDMOV D32 D240\nMOV D60 D242", read_words(159, 12, 220)),
    ));
    cases.push(scenario(
        "control_DSUBRP_cj_skipped_false_visits",
        &format!(
            "{}\nDMOV H40C00000 D32\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 D50",
            seed_words(159, 5)
        ),
        "LDI X0\nCJ P10\nLD X0\nDSUBRP D32E1 F2 D160F2\nP10\nLDI X7\nMOV K123 D50",
        &format!("{}\nMOV D50 D240", read_words(159, 5, 220)),
    ));
    cases.push(scenario(
        "control_DMULR_for_live_target_each_visit",
        &format!("{}\nDMOV H3FC00000 D32\nMOV K0 D30\nMOV K0 E1\nMOV H6A5A F1\nMOV K0 E3\nMOV H1234 F3", seed_words(159, 8)),
        "LDI X7\nMOV K0 D30\nFOR K3\nLDI X7\nMOV D30 E3\nLD X0\nDMULR D32E1 F2 D160E3\nLDI X7\nINC D30\nNEXT",
        &format!("{}\nMOV D30 D240\nMOV E3 D250", read_words(159, 8, 220)),
    ));
    cases.push(scenario(
        "control_DDIVRP_for_with_ordinary_DDIVRP_nan_successor",
        &format!("{}\n{}\nDMOV H40C00000 D32\nDMOV HFFC01234 D48\nMOV K0 D30\nMOV K0 D61\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 E3\nMOV H1234 F3", seed_words(159, 8), seed_words(80, 8)),
        "LDI X7\nMOV K0 D30\nFOR K3\nLDI X7\nMUL16 D30 K2 E3\nLD= D30 K0\nOR= D30 K2\nAND X0\nDDIVRP D32E1 F2 D160F2\nLDI X7\nDMOV D160 D80E3\nINC D30\nNEXT\nLD X0\nDDIVRP D48 F1 D164\nOUT Y11\nINC D61",
        &format!("{}\n{}\nMOV D30 D250\nMOV D61 D251", read_words(159, 8, 220), read_words(80, 8, 230)),
    ));
    check_cases(&cases, 152);
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
