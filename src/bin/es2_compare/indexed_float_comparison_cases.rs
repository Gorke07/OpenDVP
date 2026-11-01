//! Addressed floating comparisons: action result width and contact short circuits.
//! Exceptional raw inputs remain explicitly named native responsiveness probes.
use super::{Case, Target, case};
use std::collections::BTreeSet;

const ACTIONS: [&str; 4] = ["DECMP", "DECMPP", "DEZCP", "DEZCPP"];
const PREFIXES: [&str; 3] = ["FLD", "FAND", "FOR"];
const RELATIONS: [&str; 6] = ["=", ">", "<", "<>", "<=", ">="];

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

fn address(area: &str, index: u16) -> String {
    if area == "Y" {
        format!("Y{index:o}")
    } else {
        format!("{area}{index}")
    }
}

fn seed_bits(area: &str, base: u16, count: u16) -> String {
    (0..count)
        .map(|bit| {
            format!(
                "{} {}",
                if bit % 2 == 0 { "SET" } else { "RST" },
                address(area, base + bit)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn read_bits(area: &str, base: u16, output: u16) -> String {
    format!(
        "MOV K4{} D{output}\nMOV K4{} D{}",
        address(area, base),
        address(area, base + 16),
        output + 1
    )
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

fn flag_snapshot(slot: u16) -> String {
    (0..3)
        .map(|bit| format!("LD M{}\nOUT Y{:o}", 1020 + bit, 64 + slot * 3 + bit))
        .collect::<Vec<_>>()
        .join("\n")
}

fn has_modifier(operand: &str) -> bool {
    let bytes = operand.as_bytes();
    bytes.len() >= 4
        && matches!(bytes[bytes.len() - 2], b'E' | b'F')
        && bytes[bytes.len() - 1].is_ascii_digit()
}

fn contact(prefix: &str, relation: &str, left: &str, right: &str, output: u16) -> String {
    let gate = match prefix {
        "FLD" => "",
        "FAND" => "LD X0\n",
        "FOR" => "LDI X0\n",
        _ => unreachable!(),
    };
    format!("{gate}{prefix}{relation} {left} {right}\nOUT Y{output:o}")
}

fn scenario(name: &str, seed: &str, body: &str, observe: &str) -> Case {
    let gate = if body.starts_with("LD") || body.starts_with("FLD") {
        ""
    } else {
        "LD X0"
    };
    let source = [
        "LD X1\nMOV H55AA E0\nMOV H1234 F0\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068",
        seed, gate, body,
        "LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24",
        "LDI X7\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305\nMOV K1234 D306",
        observe, "END",
    ].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n");
    assert!(
        source.lines().count() <= 512,
        "oversized float comparison: {name}"
    );
    assert!(
        source.lines().count() * 9 <= 0x1ef0,
        "installed budget: {name}"
    );
    let mut indexed = false;
    for line in source.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        let Some(op) = fields.first() else { continue };
        if ACTIONS.contains(op) {
            let zone = op.starts_with("DEZCP");
            assert_eq!(
                fields.len(),
                if zone { 5 } else { 4 },
                "action fields: {name}: {line}"
            );
            for operand in &fields[1..fields.len() - 1] {
                assert!(
                    operand.starts_with(['K', 'H', 'D']),
                    "action source: {name}: {line}"
                );
            }
            assert!(
                fields[fields.len() - 1].starts_with(['Y', 'M', 'S']),
                "action target: {name}: {line}"
            );
            indexed |= fields[1..].iter().any(|operand| has_modifier(operand));
        } else if PREFIXES.iter().any(|prefix| {
            op.strip_prefix(*prefix)
                .is_some_and(|relation| RELATIONS.contains(&relation))
        }) {
            assert_eq!(fields.len(), 3, "contact fields: {name}: {line}");
            assert!(
                fields[1..]
                    .iter()
                    .all(|operand| operand.starts_with(['F', 'T', 'C', 'D'])),
                "contact source: {name}: {line}"
            );
            indexed |= fields[1..]
                .iter()
                .any(|operand| !operand.starts_with('F') && has_modifier(operand));
        }
    }
    assert!(indexed, "missing indexed floating comparison: {name}");
    case(
        &format!("indexed_float_comparison_{name}"),
        &source,
        stages(),
    )
}

fn check_cases(cases: &[Case], count: usize, all_forms: bool) {
    assert_eq!(cases.len(), count);
    assert_eq!(
        cases
            .iter()
            .map(|entry| &entry.name)
            .collect::<BTreeSet<_>>()
            .len(),
        count
    );
    if all_forms {
        let actual: BTreeSet<_> = cases
            .iter()
            .flat_map(|entry| entry.source.lines())
            .filter_map(|line| line.split_whitespace().next())
            .collect();
        for op in ACTIONS {
            assert!(actual.contains(op), "missing action {op}");
        }
        for prefix in PREFIXES {
            for relation in RELATIONS {
                assert!(
                    actual.contains(format!("{prefix}{relation}").as_str()),
                    "missing contact {prefix}{relation}"
                );
            }
        }
    }
}

fn action_vectors(op: &str, label: &str, values: &[[u32; 3]]) -> Case {
    let count = values.len() as u16;
    let mut seed = format!(
        "{}\n{}\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 E3\nMOV K0 F3",
        seed_bits("M", 159, 32),
        seed_words(80, count * 2)
    );
    let mut body = Vec::new();
    for (slot, values) in values.iter().enumerate() {
        let slot = slot as u16;
        for (operand, raw) in values.iter().enumerate() {
            seed.push_str(&format!(
                "\nDMOV H{raw:08X} D{}",
                32 + slot * 6 + operand as u16 * 2
            ));
        }
        let args = if op.starts_with("DEZCP") {
            format!(
                "D{}E1 D{}F2 D{}E3",
                32 + slot * 6,
                34 + slot * 6,
                36 + slot * 6
            )
        } else {
            format!("D{}E1 D{}F2", 32 + slot * 6, 34 + slot * 6)
        };
        body.push(format!("LD X0\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\n{}\n{op} {args} M{}F3\nOUT Y{:o}\n{}\nLDI X7\nMOV D1067 D{}\nMOV D1068 D{}",set_flags(slot%8),160+slot*4,128+slot,flag_snapshot(slot),80+slot*2,81+slot*2));
    }
    let observe = format!(
        "{}\n{}\n{}",
        read_words(32, count * 6, 200),
        read_bits("M", 159, 240),
        read_words(80, count * 2, 260)
    );
    scenario(&format!("{label}_{op}"), &seed, &body.join("\n"), &observe)
}

fn contact_vectors(relation: &str, label: &str, values: &[(u32, u32)]) -> Case {
    let mut seed = "MOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K1 F2".to_owned();
    let mut body = Vec::new();
    for (slot, (left, right)) in values.iter().enumerate() {
        let base = 32 + slot as u16 * 4;
        seed.push_str(&format!(
            "\nDMOV H{left:08X} D{base}\nDMOV H{right:08X} D{}",
            base + 2
        ));
        for (p, prefix) in PREFIXES.into_iter().enumerate() {
            body.push(contact(
                prefix,
                relation,
                &format!("D{}E1", base + 1),
                &format!("D{}F2", base + 1),
                64 + slot as u16 * 3 + p as u16,
            ));
        }
    }
    scenario(
        &format!("{label}_relation{relation}"),
        &seed,
        &body.join("\n"),
        &read_words(32, values.len() as u16 * 4, 200),
    )
}

pub(super) fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    // Each modifier word visits every action source role and its bit-result role.
    // A live offset changes even during a held P scan; its next rising edge must
    // use the new sources and preserve the old three-bit result window.
    for (form, op) in ACTIONS.into_iter().enumerate() {
        for bank in ['E', 'F'] {
            for index in 0..8u16 {
                let modifier = format!("{bank}{index}");
                let pair = index_base(index);
                let alias = pair + u16::from(bank == 'F');
                let companion = pair + u16::from(bank == 'E');
                let offset: i16 = if index % 2 == 0 { -2 } else { 1 };
                let area = ["Y", "M", "S"][(form + index as usize) % 3];
                let mut seed = format!(
                    "{}\nMOV H6A5A D{companion}\nMOV K{offset} {modifier}",
                    seed_bits(area, 152, 32)
                );
                let left = if op.starts_with("DEZCP") {
                    [0xbf800000u32; 5]
                } else {
                    [0xbf800000, 0, 0x3f800000, 0x40000000, 0xbf800000]
                };
                let right = if op.starts_with("DEZCP") {
                    0x3f800000u32
                } else {
                    0
                };
                let value = [0xc0000000u32, 0, 0x40000000, 0x3f800000, 0xbf800000];
                for slot in 0..5u16 {
                    for (base, raw) in [
                        (64, left[slot as usize]),
                        (96, right),
                        (128, value[slot as usize]),
                    ] {
                        seed.push_str(&format!(
                            "\nDMOV H{raw:08X} D{}",
                            (base as i16 + offset) as u16 + slot * 2
                        ));
                    }
                }
                let args = if op.starts_with("DEZCP") {
                    format!("D64{modifier} D96{modifier} D128{modifier}")
                } else {
                    format!("D64{modifier} D96{modifier}")
                };
                let body = format!(
                    "{op} {args} {}{modifier}\nOUT Y10\nINC {modifier}\nINC {modifier}",
                    address(area, 160)
                );
                let observe = format!(
                    "{}\n{}\n{}\nMOV {modifier} D250\nMOV D{alias} D251\nMOV D{companion} D252",
                    read_bits(area, 152, 220),
                    read_words((64i16 + offset) as u16, 10, 230),
                    read_words((96i16 + offset) as u16, 10, 260)
                );
                cases.push(scenario(
                    &format!("all_banks_{op}_{modifier}_{area}"),
                    &seed,
                    &body,
                    &observe,
                ));
            }
        }
    }
    assert_eq!(cases.len(), 64);

    // All contact prefixes and both operand positions use all sixteen modifiers.
    // OFF scans are real FLD visits; FAND/FOR visit opposite shortcut branches.
    for prefix in PREFIXES {
        for bank in ['E', 'F'] {
            for index in 0..8u16 {
                let modifier = format!("{bank}{index}");
                let pair = index_base(index);
                let companion = pair + u16::from(bank == 'E');
                let offset: i16 = if index % 2 == 0 { -2 } else { 1 };
                let mut seed = format!("MOV H6A5A D{companion}\nMOV K{offset} {modifier}");
                for (slot, raw) in [0xbf800000u32, 0, 0x3f800000, 0x40000000, 0xbf800000]
                    .into_iter()
                    .enumerate()
                {
                    seed.push_str(&format!(
                        "\nDMOV H{raw:08X} D{}\nDMOV H00000000 D{}",
                        (64i16 + offset) as u16 + slot as u16 * 2,
                        (96i16 + offset) as u16 + slot as u16 * 2
                    ));
                }
                let mut body = RELATIONS
                    .into_iter()
                    .enumerate()
                    .map(|(slot, relation)| {
                        contact(
                            prefix,
                            relation,
                            &format!("D64{modifier}"),
                            &format!("D96{modifier}"),
                            slot as u16,
                        )
                    })
                    .collect::<Vec<_>>();
                body.push(format!("LD X0\nINC {modifier}\nINC {modifier}"));
                let observe = format!(
                    "{}\n{}\nMOV {modifier} D250\nMOV D{companion} D251",
                    read_words((64i16 + offset) as u16, 10, 200),
                    read_words((96i16 + offset) as u16, 10, 220)
                );
                cases.push(scenario(
                    &format!("all_banks_{prefix}_{modifier}"),
                    &seed,
                    &body.join("\n"),
                    &observe,
                ));
            }
        }
    }
    assert_eq!(cases.len(), 112);

    for op in ACTIONS {
        let vectors = if op.starts_with("DEZCP") {
            vec![
                [0xbf800000, 0x3f800000, 0xbf800000],
                [0xbf800000, 0x3f800000, 0x3f800000],
                [0x40000000, 0x3f800000, 0x3fc00000],
                [0x40000000, 0x3f800000, 0x40000000],
                [0x80000000, 0, 0],
                [1, 0x007fffff, 1],
            ]
        } else {
            vec![
                [0, 0x80000000, 0],
                [1, 0, 0],
                [0x80000001, 0, 0],
                [0x3f800001, 0x3f800000, 0],
                [0xbf800000, 0x3f800000, 0],
                [0x7f7fffff, 0x7f7fffff, 0],
            ]
        };
        cases.push(action_vectors(
            op,
            "ordered_endpoints_signed_zero_subnormal",
            &vectors,
        ));
        for (label, positive, negative) in [
            ("quiet_nan_response", 0x7fc01234, 0xffc05678),
            ("signaling_nan_response", 0x7f801234, 0xff805678),
            ("infinity_response", 0x7f800000, 0xff800000),
        ] {
            let vectors = if op.starts_with("DEZCP") {
                vec![
                    [positive, 0x40000000, 0x3f800000],
                    [0, negative, 0x3f800000],
                    [0, 0x40000000, positive],
                    [negative, positive, 0],
                ]
            } else {
                vec![
                    [positive, 0x3f800000, 0],
                    [0x3f800000, negative, 0],
                    [positive, negative, 0],
                    [positive, positive, 0],
                ]
            };
            cases.push(action_vectors(op, label, &vectors));
        }
    }
    assert_eq!(cases.len(), 128);

    for relation in RELATIONS {
        cases.push(contact_vectors(
            relation,
            "finite_raw_signed_zero_subnormal",
            &[
                (0, 0x80000000),
                (1, 0),
                (0x80000001, 0),
                (0x3f99999b, 0x3f99999a),
                (0xbf800000, 0x3f800000),
                (0x7f7fffff, 0x7f7fffff),
            ],
        ));
        for (label, positive, negative) in [
            ("contact_quiet_nan_response", 0x7fc01234, 0xffc05678),
            ("contact_signaling_nan_response", 0x7f801234, 0xff805678),
            ("contact_infinity_response", 0x7f800000, 0xff800000),
        ] {
            cases.push(contact_vectors(
                relation,
                label,
                &[
                    (positive, 0),
                    (0, negative),
                    (positive, negative),
                    (positive, positive),
                ],
            ));
        }
    }
    assert_eq!(cases.len(), 152);

    // Remaining sections cover natural T/C storage, semantic errors and flow.
    append_storage_cases(&mut cases);
    append_error_and_literal_cases(&mut cases);
    append_alias_and_control_cases(&mut cases);
    check_cases(&cases, 191, true);
    cases
}

fn append_storage_cases(cases: &mut Vec<Case>) {
    // T and C0..198 use two adjacent WORDs. C199 zero-extends its one
    // remaining WORD; C200..255 select one natural DWORD after indexing.
    for (label, operand, offset, seed, observe) in [
        (
            "T20",
            "T21",
            -1,
            "MOV H55AA T19\nDMOV H3F800000 T20\nMOV H6A5A T22",
            "MOV T19 D230\nMOV T20 D231\nMOV T21 D232\nMOV T22 D233",
        ),
        (
            "T254",
            "T255",
            -1,
            "MOV H55AA T253\nDMOV H3F800000 T254",
            "MOV T253 D230\nMOV T254 D231\nMOV T255 D232",
        ),
        (
            "C198",
            "C199",
            -1,
            "MOV H55AA C197\nDMOV H3F800000 C198\nDMOV H40000001 C200",
            "MOV C197 D230\nMOV C198 D231\nMOV C199 D232\nDMOV C200 D234",
        ),
        (
            "C199",
            "C200",
            -1,
            "MOV H55AA C198\nMOV H8001 C199\nDMOV H40000001 C200",
            "MOV C198 D230\nMOV C199 D231\nDMOV C200 D234",
        ),
        (
            "C200",
            "C199",
            1,
            "MOV H55AA C199\nDMOV H3F800000 C200\nDMOV H40000001 C201",
            "MOV C199 D230\nDMOV C200 D232\nDMOV C201 D234",
        ),
        (
            "C255",
            "C254",
            1,
            "DMOV H40000001 C254\nDMOV H3F800000 C255",
            "DMOV C254 D230\nDMOV C255 D232",
        ),
    ] {
        let seed = format!(
            "{seed}\nDMOV H3F800000 D32\nMOV K{offset} E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2"
        );
        let mut body = Vec::new();
        for (r, relation) in RELATIONS.into_iter().enumerate() {
            for (p, prefix) in PREFIXES.into_iter().enumerate() {
                body.push(contact(
                    prefix,
                    relation,
                    &format!("{operand}E1"),
                    "D32F2",
                    64 + r as u16 * 6 + p as u16,
                ));
                body.push(contact(
                    prefix,
                    relation,
                    "D32F2",
                    &format!("{operand}E1"),
                    67 + r as u16 * 6 + p as u16,
                ));
            }
        }
        cases.push(scenario(
            &format!("natural_storage_both_roles_{label}"),
            &seed,
            &body.join("\n"),
            &format!("{observe}\nDMOV D32 D240\nMOV E1 D250\nMOV F2 D251"),
        ));
    }
    for prefix in PREFIXES {
        let seed = "MOV H55AA C198\nMOV H8001 C199\nDMOV H3F800000 C200\nDMOV H40000001 C201\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K1 F2";
        let mut body = Vec::new();
        for (r, relation) in RELATIONS.into_iter().enumerate() {
            body.push(contact(
                prefix,
                relation,
                "C200E1",
                "C199F2",
                64 + r as u16 * 2,
            ));
            body.push(contact(
                prefix,
                relation,
                "C199F2",
                "C200E1",
                65 + r as u16 * 2,
            ));
        }
        cases.push(scenario(&format!("effective_C199_C200_both_directions_{prefix}"),seed,&body.join("\n"),"MOV C198 D230\nMOV C199 D231\nDMOV C200 D232\nDMOV C201 D234\nMOV E1 D250\nMOV F2 D251"));
    }
    assert_eq!(cases.len(), 161);
}

fn append_error_and_literal_cases(cases: &mut Vec<Case>) {
    // DECMP reads BOTH sources before rejecting NaN at PC+5. Only its target
    // is unreachable. Recovery repairs the actual source and target together.
    for op in ["DECMP", "DECMPP"] {
        for right_nan in [false, true] {
            let left = if right_nan { 0x3f800000u32 } else { 0x7fc01234 };
            let right = if right_nan { 0xffc05678u32 } else { 0x40000000 };
            let sticky = if right_nan {
                "SET M1067\nSET M1068\nMOV H0123 D1067\nMOV K77 D1068"
            } else {
                ""
            };
            let seed = format!(
                "{}\nDMOV H{left:08X} D32\nDMOV H{right:08X} D36\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K-1 F2\nMOV K0 D60\n{sticky}",
                seed_bits("Y", 0, 16)
            );
            let body = format!(
                "LD X2\nDMOV H3F800000 D32\nDMOV H40000000 D36\nMOV K0 F2\nLD X0\n{op} D32E1 D36E1 Y0F2\nOUT Y10\nINC D60"
            );
            let mut entry = scenario(
                &format!(
                    "quiet_nan_skips_bad_target_{}_live_recovery_{op}",
                    if right_nan { "right_sticky" } else { "left" }
                ),
                &seed,
                &body,
                "MOV K4Y0 D220\nDMOV D32 D230\nDMOV D36 D234\nMOV D60 D240\nMOV E1 D250\nMOV F2 D251",
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
    assert_eq!(cases.len(), 165);

    for op in ACTIONS {
        let seed = format!(
            "{}\nDMOV H4B800000 D32\nDMOV H3F800000 D36\nDMOV HCF000000 D40\nDMOV H00000000 D44\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",
            seed_bits("M", 159, 32)
        );
        let args = if op.starts_with("DEZCP") {
            [
                "K16777217 H01000001 D32F2",
                "H3F800000 K1065353216 D36F2",
                "H80000000 K-2147483648 D40F2",
                "K1 K-1 D44F2",
            ]
        } else {
            [
                "K16777217 D32F2",
                "H01000001 D32F2",
                "H3F800000 D36F2",
                "H80000000 D40F2",
            ]
        };
        let body = args
            .into_iter()
            .enumerate()
            .map(|(slot, args)| {
                format!(
                    "LD X0\n{op} {args} M{}E1\nOUT Y{:o}",
                    160 + slot * 4,
                    128 + slot
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        cases.push(scenario(
            &format!("integer_constants_round_before_compare_{op}"),
            &seed,
            &body,
            &format!("{}\n{}", read_bits("M", 159, 220), read_words(32, 14, 240)),
        ));
    }
    for prefix in PREFIXES {
        let values = [
            ("F0", 0x80000000u32),
            ("F-0.0", 0),
            ("F1.2", 0x3f99999b),
            ("F1.200E+0", 0x3f99999a),
            ("F3.4028235E38", 0x7f7fffff),
            ("F1E-45", 1),
        ];
        let mut seed = "MOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K1 F2".to_owned();
        let mut body = Vec::new();
        for (slot, (literal, raw)) in values.into_iter().enumerate() {
            let base = 32 + slot as u16 * 2;
            seed.push_str(&format!("\nDMOV H{raw:08X} D{base}"));
            body.push(contact(
                prefix,
                RELATIONS[slot],
                &format!("D{}E1", base + 1),
                literal,
                64 + slot as u16 * 2,
            ));
            body.push(contact(
                prefix,
                RELATIONS[slot],
                literal,
                &format!("D{}F2", base - 1),
                65 + slot as u16 * 2,
            ));
        }
        cases.push(scenario(
            &format!("finite_literal_both_roles_{prefix}"),
            &seed,
            &body.join("\n"),
            &read_words(32, 12, 220),
        ));
    }
    assert_eq!(cases.len(), 172);
}

fn append_alias_and_control_cases(cases: &mut Vec<Case>) {
    for op in ACTIONS {
        let seed = "SET M1019\nDMOV HBF800000 D32\nDMOV H00000000 D36\nDMOV H3F800000 D40\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2";
        let args = if op.starts_with("DEZCP") {
            [
                "D32E1 D40E1 D32E1",
                "D32E1 D40E1 D36E1",
                "D32E1 D36E1 D40E1",
            ]
        } else {
            ["D32E1 D36E1", "D36E1 D36E1", "D40E1 D36E1"]
        };
        let body = args
            .into_iter()
            .enumerate()
            .map(|(slot, args)| {
                format!(
                    "LD X0\n{op} {args} M1020F2\nLDI X7\nMOV K1M1019 D{}",
                    80 + slot
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        cases.push(scenario(&format!("result_overlaps_all_ALU_flags_three_bits_{op}"),seed,&body,"MOV D80 D220\nMOV D81 D221\nMOV D82 D222\nMOV K1M1019 D223\nDMOV D32 D230\nDMOV D36 D234\nDMOV D40 D238"));
    }
    assert_eq!(cases.len(), 176);

    for op in ACTIONS {
        let seed = format!(
            "{}\nDMOV H3FC00000 D32\nDMOV H3F800001 D1182\nMOV H4000 E2\nMOV K-1 F2",
            seed_bits("M", 159, 32)
        );
        let args = if op.starts_with("DEZCP") {
            "D1181E1 D1184F2 D31E1"
        } else {
            "D1181E1 D1184F2"
        };
        let body = format!(
            "LDI X7\nDMOV H3F800001 D1182\nMOV H4000 E2\nMOV K-1 F2\nLD X0\n{op} {args} M161F2\nOUT Y10"
        );
        cases.push(scenario(
            &format!("source_contains_own_index_shifted_alias_{op}"),
            &seed,
            &body,
            &format!(
                "{}\nMOV E1 D230\nMOV F1 D231\nMOV E2 D232\nMOV F2 D233\nDMOV D32 D240",
                read_bits("M", 159, 220)
            ),
        ));
    }
    for prefix in PREFIXES {
        let seed = "DMOV H3F800000 D1182\nMOV H4000 E2\nMOV K1 F2";
        let mut body = Vec::new();
        for (slot, relation) in RELATIONS.into_iter().enumerate() {
            body.push("LDI X7\nDMOV H3F800000 D1182\nMOV H4000 E2\nMOV K1 F2".to_owned());
            body.push(contact(
                prefix,
                relation,
                "D1182E1",
                "D1183F2",
                64 + slot as u16 * 2,
            ));
            body.push("LDI X7\nMOV K2 E1\nMOV K0 F2".to_owned());
            body.push(contact(
                prefix,
                relation,
                "D1182E1",
                "D1183F2",
                65 + slot as u16 * 2,
            ));
        }
        cases.push(scenario(&format!("live_modifiers_inside_source_pair_{prefix}"),seed,&body.join("\n"),"MOV E1 D230\nMOV F1 D231\nMOV E2 D232\nMOV F2 D233\nMOV D1182 D240\nMOV D1183 D241\nMOV D1184 D242\nMOV D1185 D243"));
    }
    assert_eq!(cases.len(), 183);

    for op in ["DECMPP", "DEZCPP"] {
        let seed = format!(
            "{}\n{}\nDMOV HBF800000 D32\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 E3\nMOV H1234 F3\nMOV K0 D30",
            seed_bits("M", 159, 16),
            seed_words(80, 4)
        );
        let args = if op.starts_with("DEZCP") {
            "K-1 K1 D32E1"
        } else {
            "D32E1 K0"
        };
        let body = format!(
            "LDI X7\nMOV K0 D30\nDMOV HBF800000 D32\nFOR K3\nLDI X7\nMOV D30 E3\nLD= D30 K2\nDMOV H40000000 D32\nLD= D30 K0\nOR= D30 K2\nAND X0\n{op} {args} M160F2\nLDI X7\nMOV K4M159 D80E3\nINC D30\nNEXT"
        );
        cases.push(scenario(
            &format!("control_for_same_pc_true_false_true_{op}"),
            &seed,
            &body,
            &format!("MOV K4M159 D220\n{}\nMOV D30 D250", read_words(80, 4, 230)),
        ));
    }
    cases.push(scenario("control_DECMP_mps_changed_target",
        &format!("{}\nDMOV HBF800000 D32\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",seed_bits("M",159,16)),
        "MPS\nDECMP D32E1 K0 M160F2\nMRD\nMOV K4 F2\nMRD\nDECMP K0 D32E1 M160F2\nMPP\nOUT Y10\nLDI X7\nMOV K0 F2",
        "MOV K4M159 D220\nMOV F2 D250"));
    cases.push(scenario(
        "control_DEZCP_cj_skip_successor",
        &format!(
            "{}\nDMOV H3F800000 D32\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 D60",
            seed_bits("S", 159, 16)
        ),
        "LDI X0\nCJ P10\nLD X0\nDEZCP K-1 K1 D32E1 S160F2\nP10\nLDI X7\nINC D60",
        "MOV K4S159 D220\nMOV D60 D240",
    ));
    for prefix in ["FLD", "FAND"] {
        let predicate = if prefix == "FLD" {
            "FLD< D32E1 D36F2"
        } else {
            "LDI X7\nFAND< D32E1 D36F2"
        };
        let seed = "DMOV HBF800000 D32\nDMOV H3F800000 D36\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nSET Y0\nMOV K0 D60";
        let body = format!("LD X0\nMC N0\n{predicate}\nOUT Y0\nMCR N0\nLDI X7\nINC D60");
        cases.push(scenario(
            &format!("control_master_false_source_visit_{prefix}"),
            seed,
            &body,
            "DMOV D32 D230\nDMOV D36 D234\nMOV D60 D240",
        ));
    }
    cases.push(scenario(
        "control_FOR_mps_shortcut_and_evaluation",
        "DMOV HBF800000 D32\nDMOV H3F800000 D36\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",
        "MPS\nFOR> D32E1 D36F2\nOUT Y0\nMRD\nFOR< D32E1 D36F2\nOUT Y1\nMPP\nOUT Y2",
        "DMOV D32 D230\nDMOV D36 D234",
    ));
    cases.push(scenario("control_contact_for_live_index_and_branch_stack",
        "DMOV HBF800000 D32\nDMOV H00000000 D34\nDMOV H3F800000 D36\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 D30\nMOV K0 D60",
        "LDI X7\nMOV K0 D30\nMOV K0 E1\nFOR K3\nFLD<= D32E1 F0\nAND X0\nMPS\nOUT Y0\nMRD\nFAND<> D32E1 F0\nOUT Y1\nINC D60\nMPP\nFOR= D32E1 F1\nOUT Y2\nLDI X7\nINC E1\nINC E1\nINC D30\nNEXT",
        "MOV E1 D230\nMOV D30 D231\nMOV D60 D232\nDMOV D32 D240\nDMOV D34 D244\nDMOV D36 D248"));
    assert_eq!(cases.len(), 191);
}

pub(super) fn model_cases(target: Target) -> Vec<Case> {
    let last: u16 = match target {
        Target::Es2 => 9999,
        Target::Ss2 => 4999,
    };
    let pair = last - 1;
    let mut cases = Vec::new();
    for (slot, op) in ACTIONS.into_iter().enumerate() {
        let (area, result) = if slot % 2 == 0 {
            ("Y", 253)
        } else {
            ("S", 1021)
        };
        let seed = format!(
            "{}\n{}\nDMOV H3F800000 D{pair}\nDMOV H3F800000 D80\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",
            seed_words(last - 2, 3),
            seed_bits(area, result - 1, 4)
        );
        let args = if op.starts_with("DEZCP") {
            format!("K-1 D80F2 D{last}E1")
        } else {
            format!("D{last}E1 D80F2")
        };
        let body = format!(
            "INC D{pair}\n{op} {args} {}F2\nOUT Y10",
            address(area, result)
        );
        let observe = format!(
            "{}\nMOV K1{} D230\nDMOV D80 D240\nMOV E1 D250\nMOV F2 D251",
            read_words(last - 2, 3, 220),
            address(area, result - 1)
        );
        cases.push(scenario(
            &format!(
                "{}_last_pair_and_last_three_bits_{op}_{area}",
                target.name()
            ),
            &seed,
            &body,
            &observe,
        ));
    }
    for prefix in PREFIXES {
        for relation in RELATIONS {
            let seed = format!(
                "{}\nDMOV H3F99999A D{pair}\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K-1 F2",
                seed_words(last - 2, 3)
            );
            let body = format!(
                "LD X0\nINC D{pair}\n{}\n{}\n{}",
                contact(prefix, relation, &format!("D{last}E1"), "F1.2", 0),
                contact(prefix, relation, "F1.2", &format!("D{last}F2"), 1),
                contact(
                    prefix,
                    relation,
                    &format!("D{last}E1"),
                    &format!("D{last}F2"),
                    2
                )
            );
            let observe = format!("{}\nMOV E1 D250\nMOV F2 D251", read_words(last - 2, 3, 220));
            cases.push(scenario(
                &format!("{}_last_pair_both_roles_{prefix}{relation}", target.name()),
                &seed,
                &body,
                &observe,
            ));
        }
    }
    check_cases(&cases, 22, true);
    cases
}
