//! Indexed log(base, argument) and pow(base, exponent), preserving input kinds.
//! Staged domain errors must not read unvisited right/target modifiers.
//! Nonfinite/overflow responsiveness is recorded separately from ordinary math.
use super::{Case, Target, case};
use std::collections::BTreeSet;

const FORMS: [&str; 4] = ["DLOG", "DLOGP", "DPOW", "DPOWP"];

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
        "oversized indexed log/pow case {name}"
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
        "missing indexed log/pow action in {name}"
    );
    for line in source.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.first().is_some_and(|op| FORMS.contains(op)) {
            assert_eq!(fields.len(), 4, "log/pow fields: {name}: {line}");
            assert!(
                fields[1..3].iter().all(|value| value.starts_with('K')
                    || value.starts_with('H')
                    || value.starts_with('D'))
                    && fields[3].starts_with('D'),
                "log/pow operand roles: {name}: {line}"
            );
        }
    }
    case(&format!("indexed_log_pow_{name}"), &source, stages())
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
            for family in ["DLOG", "DPOW"] {
                let pulse = (index + u16::from(bank == 'F')) % 2 == 1;
                let op = format!("{family}{}", if pulse { "P" } else { "" });
                let mut inputs = Vec::new();
                for start in [61, 93] {
                    for i in 0..9 {
                        let word = if start == 61 {
                            [0x3f80, 0x4000, 0x4040, 0x4080, 0x4100, 0x3f00][i % 6]
                        } else {
                            [0x4000, 0x3fc0, 0x3f00, 0x4040, 0x3f80, 0x4080][i % 6]
                        };
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
    assert_eq!(cases.len(), 32);

    let log_sets: [(&str, &[(u32, u32)]); 6] = [
        (
            "manual_positive_pairs",
            &[
                (0x40a00000, 0x42fa0000),
                (0x41200000, 0x3f800000),
                (0x40000000, 0x41000000),
                (0x3f000000, 0x41000000),
                (0x40000000, 0x3f000000),
                (0x41200000, 0x447a0000),
                (0x42fa0000, 0x40a00000),
                (0x40a00000, 0x40a00000),
            ],
        ),
        (
            "rounded_log10_boundaries",
            &[
                (0x3f7fffff, 1),
                (0x3f800001, 1),
                (0x3f7fffff, 0x7f7fffff),
                (0x3f800001, 0x7f7fffff),
                (0x40000000, 0x3f7fffff),
                (0x40000000, 0x3f800001),
                (0x3f000000, 0x3f7fffff),
                (0x3f000000, 0x3f800001),
            ],
        ),
        (
            "base_argument_domain",
            &[
                (0, 0x40000000),
                (0x80000000, 0x40000000),
                (0xbf800000, 0x40000000),
                (0x3f800000, 0x40000000),
                (0x40000000, 0),
                (0x40000000, 0x80000000),
                (0x40000000, 0xbf800000),
                (0x40000000, 0x80000001),
            ],
        ),
        (
            "quiet_nan_response",
            &[
                (0x7fc01234, 0x40000000),
                (0xffc01234, 0x40000000),
                (0x40000000, 0x7fc01234),
                (0x40000000, 0xffc01234),
            ],
        ),
        (
            "signaling_nan_response",
            &[
                (0x7f800001, 0x40000000),
                (0xff800001, 0x40000000),
                (0x40000000, 0x7f800001),
                (0x40000000, 0xff800001),
            ],
        ),
        (
            "infinity_response",
            &[
                (0x7f800000, 0x40000000),
                (0x40000000, 0x7f800000),
                (0x7f800000, 0x7f800000),
                (0xff800000, 0x40000000),
                (0x40000000, 0xff800000),
                (0x7f800000, 0x3f800000),
            ],
        ),
    ];
    let pow_sets: [(&str, &[(u32, u32)]); 8] = [
        (
            "integer_zero_negative_exponents",
            &[
                (0x40a00000, 0x40400000),
                (0x40a00000, 0),
                (0x40a00000, 0x80000000),
                (0x40000000, 0xbf800000),
                (0x40000000, 0xc0000000),
                (0x40000000, 0xc0400000),
                (0x3f000000, 0x40800000),
                (0x3f000000, 0xc0800000),
            ],
        ),
        (
            "general_fractional_exponents",
            &[
                (0x40000000, 0x3f000000),
                (0x40000000, 0xbf000000),
                (0x41100000, 0x3f000000),
                (0x41100000, 0xbf000000),
                (0x40a00000, 0x3fc00000),
                (0x3f000000, 0x3fc00000),
                (0x3f800001, 0x3f800001),
                (0x3f7fffff, 0xbf800001),
            ],
        ),
        (
            "base_domain",
            &[
                (0, 0),
                (0x80000000, 0x3f800000),
                (0xbf800000, 0x40000000),
                (0x80000001, 0x40000000),
                (0xff7fffff, 0),
                (0xbf000000, 0xc0000000),
            ],
        ),
        (
            "quiet_nan_response",
            &[
                (0x7fc01234, 0x40000000),
                (0xffc01234, 0x40000000),
                (0x40000000, 0x7fc01234),
                (0x40000000, 0xffc01234),
            ],
        ),
        (
            "signaling_nan_response",
            &[
                (0x7f800001, 0x40000000),
                (0xff800001, 0x40000000),
                (0x40000000, 0x7f800001),
                (0x40000000, 0xff800001),
            ],
        ),
        (
            "infinity_response",
            &[
                (0x7f800000, 0xbf800000),
                (0x7f800000, 0x80000000),
                (0x7f800000, 0),
                (0x7f800000, 0x3f800000),
                (0x40000000, 0x7f800000),
                (0x3f000000, 0xff800000),
            ],
        ),
        (
            "finite_overflow_response",
            &[
                (0x7f7fffff, 0x40000000),
                (0x41200000, 0x42c80000),
                (0x40000000, 0x43000000),
                (0x3f000000, 0xc3160000),
            ],
        ),
        (
            "finite_underflow",
            &[
                (0x40000000, 0xc3150000),
                (0x40000000, 0xc3160000),
                (0x3f000000, 0x43150000),
                (0x3f000000, 0x43160000),
                (0x00800000, 0x40000000),
                (0x3dcccccd, 0x42000000),
                (0x40000000, 0xc2c80000),
                (0x40000000, 0xc2fc0000),
            ],
        ),
    ];
    for op in FORMS {
        let sets: &[(&str, &[(u32, u32)])] = if op.starts_with("DLOG") {
            &log_sets
        } else {
            &pow_sets
        };
        for &(label, values) in sets {
            let extra = if label == "finite_overflow_response" {
                2
            } else {
                0
            };
            let count = values.len() as u16 + extra;
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
            if extra != 0 {
                body.push(result_site(op, "K2", "K128", values.len() as u16));
                body.push(result_site(op, "H01000001", "K10", values.len() as u16 + 1));
            }
            let observe = format!(
                "{}\n{}\n{}",
                read_words(40, values.len() as u16 * 4, 200),
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
    assert_eq!(cases.len(), 60);

    for op in FORMS {
        let is_log = op.starts_with("DLOG");
        let operands: &[(&str, &str)] = if is_log {
            &[
                ("K5", "K125"),
                ("H00000005", "H0000007D"),
                ("D32E1", "D48F2"),
                ("K5", "D48F2"),
                ("D32E1", "K125"),
                ("H3F800000", "K125"),
                ("D36E1", "K125"),
                ("K16777217", "H01000001"),
                ("K2", "D52F2"),
                ("D40E1", "K2"),
                ("K0", "K2"),
                ("HFFFFFFFF", "K2"),
            ]
        } else {
            &[
                ("K5", "K3"),
                ("H00000005", "H00000003"),
                ("D32E1", "D48F2"),
                ("K5", "D48F2"),
                ("D32E1", "K3"),
                ("H3F800000", "K1"),
                ("D36E1", "K3"),
                ("K16777217", "K1"),
                ("K2", "D52F2"),
                ("D40E1", "K-2"),
                ("K0", "K2"),
                ("HFFFFFFFF", "K2"),
            ]
        };
        let count = operands.len() as u16;
        let right = if is_log { 0x42fa0000u32 } else { 0x40400000 };
        let other = if is_log { 0x40400000u32 } else { 0xc0000000 };
        let seed = format!(
            "{}\n{}\n{}\nDMOV H40A00000 D32\nDMOV H3F800000 D36\nDMOV H3F000000 D40\nDMOV H{right:08X} D48\nDMOV H{other:08X} D52\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K1 E3\nMOV H1234 F3",
            seed_words(31, 24),
            seed_words(159, count * 3 + 1),
            seed_words(80, count * 2)
        );
        let body = operands
            .iter()
            .enumerate()
            .map(|(slot, (a, b))| result_site(op, a, b, slot as u16))
            .collect::<Vec<_>>()
            .join("\n");
        let observe = format!(
            "{}\n{}\n{}",
            read_words(31, 24, 200),
            read_words(159, count * 3 + 1, 230),
            read_words(80, count * 2, 270)
        );
        cases.push(scenario(
            &format!("integer_K_H_and_raw_D_sources_{op}"),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 64);

    // A bad base bypasses RHS and target; a bad DLOG argument bypasses target.
    // In particular +Inf base / zero argument must fail before either log10
    // helper runs. No DPOW exponent is misrepresented as a PLC domain error.
    for op in FORMS {
        let is_log = op.starts_with("DLOG");
        let mut errors = vec![
            ("zero_base", 0u32, 0u32, true),
            ("nan_base", 0x7fc01234, 0, true),
        ];
        if is_log {
            errors.extend([
                ("unit_base", 0x3f800000, 0, true),
                ("zero_argument", 0x40a00000, 0, false),
                ("nan_argument", 0x40a00000, 0xffc01234, false),
                (
                    "infinite_base_zero_argument_before_math",
                    0x7f800000,
                    0,
                    false,
                ),
            ]);
        } else {
            errors.push(("negative_base", 0xbf800000, 0, true));
        }
        for (slot, (label, left, right, bad_base)) in errors.into_iter().enumerate() {
            let base = if bad_base { 10 } else { 48 };
            let offset = if bad_base { -11 } else { 0 };
            let repaired = if is_log { 0x42fa0000u32 } else { 0x40400000 };
            let sticky = if label == "negative_base" || label == "unit_base" {
                "SET M1067\nSET M1068\nMOV H0123 D1067\nMOV K77 D1068"
            } else {
                ""
            };
            let seed = format!(
                "{}\nDMOV H{left:08X} D32\nDMOV H{right:08X} D{base}\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K{offset} F2\nMOV K-1 E3\nMOV H1234 F3\nMOV K0 D60\n{}\n{sticky}",
                seed_words(0, 4),
                set_flags(slot as u16 % 8)
            );
            let body = format!(
                "LD X2\nDMOV H40A00000 D32\nDMOV H{repaired:08X} D{base}\nMOV K0 F2\nMOV K0 E3\nLD X0\n{op} D32E1 D{base}F2 D0E3\nOUT Y10\nINC D60"
            );
            let observe = format!(
                "{}\nDMOV D32 D230\nDMOV D{base} D232\nMOV D60 D240\nMOV E1 D250\nMOV F2 D251\nMOV E3 D252",
                read_words(0, 4, 220)
            );
            let mut entry = scenario(
                &format!("staged_{label}_bad_unused_indices_live_recovery_{op}"),
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
    assert_eq!(cases.len(), 82);

    for (which, op) in FORMS.into_iter().enumerate() {
        let (modifier, alias, companion) = if which % 2 == 0 {
            ("E1", 1182, 1183)
        } else {
            ("F2", 1185, 1184)
        };
        let lexical = alias - 1;
        let right = if op.starts_with("DLOG") {
            0x42fa0000u32
        } else {
            0x40400000
        };
        let seed = format!(
            "{}\n{}\nDMOV H40A00000 D32\nDMOV H{right:08X} D48\nMOV K1 E1\nMOV K1 F2\nMOV H5AA5 D{companion}",
            seed_words(alias - 1, 4),
            seed_words(31, 20)
        );
        let body = format!(
            "LDI X7\nDMOV H40A00000 D32\nDMOV H{right:08X} D48\nMOV K1 E1\nMOV K1 F2\nLD X0\n{op} D31E1 D47F2 D{lexical}{modifier}\n{}\nLDI X7\nDMOV D{alias} D80\nMOV K0 E1\nMOV K0 F2\nLD X0\n{op} D32E1 D48F2 D33E1\n{}\nLDI X7\nDMOV D33 D84\nDMOV H40A00000 D32\nDMOV H{right:08X} D48\nLD X0\n{op} D32E1 D48F2 D49F2\n{}\nLDI X7\nDMOV D49 D88",
            flag_snapshot(0),
            flag_snapshot(1),
            flag_snapshot(2)
        );
        let observe = format!(
            "{}\n{}\n{}\nDMOV D80 D240\nDMOV D84 D244\nDMOV D88 D248\nMOV D{companion} D250",
            read_words(31, 5, 200),
            read_words(47, 5, 210),
            read_words(alias - 1, 4, 220)
        );
        cases.push(scenario(
            &format!("both_source_overlaps_self_index_{modifier}_{op}"),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 86);

    for (which, op) in FORMS.into_iter().enumerate() {
        let index = if which % 2 == 0 { 0u16 } else { 1 };
        let pair = index_base(index);
        let right = if op.starts_with("DLOG") {
            0x42fa0001u32
        } else {
            0x40400001
        };
        let seed = format!(
            "{}\nDMOV H40A00000 D32\nDMOV H{right:08X} D{pair}\nMOV K0 E3\nMOV H1234 F3\nMOV H7BBB E2\nMOV K0 F2",
            seed_words(159, 5)
        );
        let body = format!(
            "LDI X7\nDMOV H{right:08X} D{pair}\nMOV K0 E3\nLD X0\n{op} D31E{index} D{}E{index} D160E3",
            pair - 1
        );
        // Both source-pair low words are +1, selecting the base at D32 and
        // the right source at the canonical E/F pair in the same visit.
        let observe = format!(
            "{}\nMOV D{pair} D240\nMOV D{} D241\nMOV E{index} D250\nMOV F{index} D251",
            read_words(159, 5, 220),
            pair + 1
        );
        cases.push(scenario(
            &format!("right_source_contains_index_E{index}_{op}"),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 90);

    for op in FORMS {
        let rights = if op.starts_with("DLOG") {
            [0x42fa0000u32, 0x41000000, 0x3f000000]
        } else {
            [0x40400000, 0xc0000000, 0x3f000000]
        };
        let seed = format!(
            "{}\nDMOV H40A00000 D32\nDMOV H40000000 D34\nDMOV H3F000000 D36\nDMOV H{:08X} D48\nDMOV H{:08X} D50\nDMOV H{:08X} D52\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 E3\nMOV H1234 F3",
            seed_words(159, 10),
            rights[0],
            rights[1],
            rights[2]
        );
        let body=(0..3u16).map(|slot|format!("LDI X7\nMOV K{} E1\nMOV K{} F2\nMOV K{} E3\nLD X0\n{op} D32E1 D48F2 D160E3\n{}",slot*2,slot*2,slot*3,flag_snapshot(slot))).collect::<Vec<_>>().join("\n");
        let observe = format!(
            "{}\n{}\n{}\nMOV E1 D260\nMOV F2 D261\nMOV E3 D262",
            read_words(32, 6, 200),
            read_words(48, 6, 210),
            read_words(159, 10, 230)
        );
        cases.push(scenario(
            &format!("same_scan_three_live_offsets_{op}"),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 94);

    cases.push(scenario(
        "control_DLOG_mps_manual_roundtrip",
        &format!(
            "{}\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",
            seed_words(159, 8)
        ),
        "MPS\nDLOG K5 K125 D160E1\nMRD\nDPOW K5 D160E1 D164F2\nMPP\nOUT Y10",
        &read_words(159, 8, 220),
    ));
    cases.push(scenario("control_DLOGP_cj_skipped_false_visits",
        &format!("{}\nDMOV H40A00000 D32\nDMOV H42FA0000 D48\nMOV K0 D60\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",seed_words(159,5)),
        "LDI X0\nCJ P10\nLD X0\nDLOGP D32E1 D48F2 D160F2\nP10\nLDI X7\nINC D60",
        &format!("{}\nMOV D60 D240",read_words(159,5,220))));
    cases.push(scenario("control_DPOW_mps_changed_target",
        &format!("{}\nDMOV H40A00000 D32\nDMOV HC0000000 D48\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",seed_words(159,6)),
        "MPS\nDPOW D32E1 D48F2 D160F2\nMRD\nMOV K1 F2\nMPP\nOUT Y10",
        &format!("{}\nMOV F2 D250",read_words(159,6,220))));
    cases.push(scenario("control_DPOWP_for_true_false_true_exponents",
        &format!("{}\n{}\nDMOV H40A00000 D32\nDMOV H00000000 D48\nMOV K0 D30\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 E3\nMOV H1234 F3",seed_words(159,4),seed_words(80,8)),
        "LDI X7\nMOV K0 D30\nFOR K3\nLD= D30 K0\nDMOV H00000000 D48\nLD= D30 K1\nDMOV H3F800000 D48\nLD= D30 K2\nDMOV H40000000 D48\nLDI X7\nMUL16 D30 K2 E3\nLD= D30 K0\nOR= D30 K2\nAND X0\nDPOWP D32E1 D48F2 D160F2\nLDI X7\nDMOV D160 D80E3\nINC D30\nNEXT",
        &format!("{}\n{}\nMOV D30 D250",read_words(159,4,220),read_words(80,8,230))));
    check_cases(&cases, 98);
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
        let right = if op.starts_with("DLOG") {
            0x42fa0000u32
        } else {
            0x40400000
        };
        let seed = format!(
            "{}\n{}\nDMOV H40A00000 D{pair}\nDMOV H{right:08X} D32\nDMOV H40A00000 D48\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K-1 F7\nMOV H1234 E7",
            seed_words(last - 2, 3),
            seed_words(159, 8)
        );
        let body = format!(
            "LDI X7\nDMOV H40A00000 D{pair}\nLD X0\n{op} D{last}E1 D32F2 D160F2\nLDI X7\nDMOV H{right:08X} D{pair}\nLD X0\n{op} D48F2 D{last}F7 D164F2"
        );
        let observe = format!(
            "{}\n{}\nMOV E1 D250\nMOV F7 D251",
            read_words(last - 2, 3, 200),
            read_words(159, 8, 220)
        );
        cases.push(scenario(
            &format!("model_{}_both_last_pair_sources_{op}", target.name()),
            &seed,
            &body,
            &observe,
        ));

        let literal = if op.starts_with("DLOG") {
            "H0000007D"
        } else {
            "K3"
        };
        let seed = format!("{}\nMOV K-1 E1\nMOV H6A5A F1", seed_words(last - 3, 4));
        let body = format!("{op} K5 {literal} D{last}E1\nMOV K-2 E1");
        let observe = format!("{}\nMOV E1 D250\nMOV F1 D251", read_words(last - 3, 4, 220));
        cases.push(scenario(
            &format!("model_{}_last_pair_target_{op}", target.name()),
            &seed,
            &body,
            &observe,
        ));
    }
    check_cases(&cases, 8);
    cases
}
