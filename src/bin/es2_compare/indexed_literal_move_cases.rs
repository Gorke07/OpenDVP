//! Finite F-literal bit copies to addressed numeric and packed destinations.
//! Ordinary cases avoid the separately qualified low-M remapping region.
use super::{Case, Target, case};
use std::collections::BTreeSet;

const FORMS: [&str; 2] = ["DMOVR", "DMOVRP"];

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (vec![("X1", true), ("X0", false), ("X7", false)], 1),
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

fn seed_bits(area: &str, base: u16, count: u16) -> String {
    (0..count)
        .map(|i| {
            format!(
                "{} {}",
                if i % 2 == 0 { "SET" } else { "RST" },
                address(area, base + i)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn read_bits(area: &str, base: u16, count: u16, output: u16) -> String {
    assert_eq!(count % 4, 0);
    (0..count.div_ceil(16))
        .map(|i| {
            format!(
                "MOV K{}{} D{}",
                (count - i * 16).min(16) / 4,
                address(area, base + i * 16),
                output + i
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn scenario(name: &str, seed: &str, body: &str, observe: &str) -> Case {
    let gate = if body.starts_with("LD") || body.starts_with("STL") {
        ""
    } else {
        "LD X0"
    };
    let source = [
        "LD X1\nMOV H55AA E0\nMOV H1234 F0\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV H1234 D1067\nMOV K77 D1068",
        seed,gate,body,
        "LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24",
        "LDI X7\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305\nMOV K1234 D306",
        observe,"END",
    ].into_iter().filter(|s|!s.is_empty()).collect::<Vec<_>>().join("\n");
    assert!(
        source.lines().count() <= 512,
        "oversized indexed literal move: {name}"
    );
    assert!(
        source.lines().count() * 9 <= 0x1ef0,
        "installed budget: {name}"
    );
    let mut indexed = false;
    for line in source.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.first().is_some_and(|op| FORMS.contains(op)) {
            assert_eq!(fields.len(), 3, "literal move fields: {name}: {line}");
            assert!(
                fields[1].strip_prefix('F').is_some_and(|literal| literal
                    .parse::<f32>()
                    .is_ok_and(|value| value.is_finite())),
                "finite literal: {name}: {line}"
            );
            assert!(
                fields[2].starts_with(['K', 'T', 'C', 'D']),
                "literal move target: {name}: {line}"
            );
            let bytes = fields[2].as_bytes();
            indexed |= bytes.len() >= 4
                && matches!(bytes[bytes.len() - 2], b'E' | b'F')
                && bytes[bytes.len() - 1].is_ascii_digit();
        }
    }
    assert!(indexed, "missing indexed literal move in {name}");
    case(&format!("indexed_literal_move_{name}"), &source, stages())
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
    for op in FORMS {
        assert!(
            cases.iter().any(|entry| entry
                .source
                .lines()
                .any(|line| line.split_whitespace().next() == Some(op))),
            "missing {op}"
        );
    }
}

pub(super) fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    // All six target areas visit all sixteen signed WORD modifiers. Advancing
    // one unit moves a Kn target one bit and a numeric target one register.
    // It also advances during a held P visit, exposing stale target reuse.
    for op in FORMS {
        for bank in ['E', 'F'] {
            for index in 0..8u16 {
                let modifier = format!("{bank}{index}");
                let companion = index_base(index) + u16::from(bank == 'E');
                let offset = if index % 2 == 0 { -2 } else { 1 };
                let digits = index + 1;
                let literal = ["F1.2", "F1.0", "F-0.0"][index as usize % 3];
                let seed = format!(
                    "{}\n{}\n{}\n{}\n{}\n{}\nMOV H6A5A D{companion}\nMOV K{offset} {modifier}",
                    seed_words("D", 60, 12),
                    seed_words("T", 60, 12),
                    seed_words("C", 60, 12),
                    seed_bits("Y", 60, 48),
                    seed_bits("M", 60, 48),
                    seed_bits("S", 60, 48)
                );
                let body = format!(
                    "{op} {literal} D64{modifier}\n{op} {literal} T64{modifier}\n{op} {literal} C64{modifier}\n{op} {literal} K{digits}Y100{modifier}\n{op} {literal} K{digits}M64{modifier}\n{op} {literal} K{digits}S64{modifier}\nOUT Y10\nINC {modifier}"
                );
                let observe = format!(
                    "{}\n{}\n{}\n{}\n{}\n{}\nMOV {modifier} D280\nMOV D{companion} D281",
                    read_words("D", 60, 12, 200),
                    read_words("T", 60, 12, 220),
                    read_words("C", 60, 12, 240),
                    read_bits("Y", 60, 48, 260),
                    read_bits("M", 60, 48, 264),
                    read_bits("S", 60, 48, 268)
                );
                cases.push(scenario(
                    &format!("all_banks_six_areas_{op}_{modifier}_K{digits}"),
                    &seed,
                    &body,
                    &observe,
                ));
            }
        }
    }
    assert_eq!(cases.len(), 32);

    // Each width/area has three independently observable copies: a fraction
    // with nonzero low bits, integral one, and the negative-zero sign bit.
    // Explicit ON guards on both sides detect wider-than-declared clearing.
    for op in FORMS {
        for area in ["Y", "M", "S"] {
            for digits in 1..=8u16 {
                let offset: i16 = if digits % 2 == 0 { 1 } else { -1 };
                let actual = (128i16 + offset) as u16;
                let seed = format!(
                    "{}\nSET {}\nSET {}\n{}\nMOV K{offset} E1\nMOV H6A5A F1",
                    seed_bits(area, 120, 48),
                    address(area, actual - 1),
                    address(area, actual + digits * 4),
                    seed_words("D", 80, 9)
                );
                let body = ["F1.2", "F1.0", "F-0.0"]
                    .into_iter()
                    .enumerate()
                    .map(|(slot, literal)| {
                        format!(
                            "LD X0\n{op} {literal} K{digits}{}E1\nLDI X7\n{}",
                            address(area, 128),
                            read_bits(area, 120, 48, 80 + slot as u16 * 3)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let observe = format!(
                    "{}\n{}\nMOV E1 D250\nMOV F1 D251",
                    read_bits(area, 120, 48, 220),
                    read_words("D", 80, 9, 230)
                );
                cases.push(scenario(
                    &format!("exact_width_{op}_K{digits}{area}_fraction_one_negative_zero"),
                    &seed,
                    &body,
                    &observe,
                ));
            }
        }
    }
    assert_eq!(cases.len(), 80);

    append_numeric_boundaries(&mut cases);
    append_aliases(&mut cases);
    append_controls(&mut cases);
    check_cases(&cases, 124);
    cases
}

fn append_numeric_boundaries(cases: &mut Vec<Case>) {
    for op in FORMS {
        for (label, target, offset, seed, observe) in [
            (
                "T0",
                "T1",
                -1,
                "MOV H55AA T0\nMOV H6A5A T1\nMOV H3CC3 T2",
                "MOV T0 D220\nMOV T1 D221\nMOV T2 D222",
            ),
            (
                "T254",
                "T255",
                -1,
                "MOV H55AA T253\nMOV H6A5A T254\nMOV H3CC3 T255",
                "MOV T253 D220\nMOV T254 D221\nMOV T255 D222",
            ),
            (
                "C0",
                "C1",
                -1,
                "MOV H55AA C0\nMOV H6A5A C1\nMOV H3CC3 C2",
                "MOV C0 D220\nMOV C1 D221\nMOV C2 D222",
            ),
            (
                "C198",
                "C199",
                -1,
                "MOV H55AA C197\nMOV H6A5A C198\nMOV H3CC3 C199\nDMOV H12345678 C200",
                "MOV C197 D220\nMOV C198 D221\nMOV C199 D222\nDMOV C200 D224",
            ),
            (
                "C199_from_C200",
                "C200",
                -1,
                "MOV H55AA C198\nMOV H6A5A C199\nDMOV H12345678 C200\nDMOV H5AA53CC3 C201",
                "MOV C198 D220\nMOV C199 D221\nDMOV C200 D222\nDMOV C201 D224",
            ),
            (
                "C200_from_C199",
                "C199",
                1,
                "MOV H55AA C199\nDMOV H12345678 C200\nDMOV H5AA53CC3 C201",
                "MOV C199 D220\nDMOV C200 D222\nDMOV C201 D224",
            ),
            (
                "C255",
                "C254",
                1,
                "DMOV H12345678 C254\nDMOV H5AA53CC3 C255",
                "DMOV C254 D220\nDMOV C255 D222",
            ),
        ] {
            let seed =
                format!("{seed}\nMOV H7BBB E2\nMOV K{offset} F2\nMOV K0 D60\nSET M1067\nSET M1068");
            let body = format!("{op} F1.2 {target}F2\nINC D60");
            cases.push(scenario(
                &format!("numeric_boundary_{op}_{label}"),
                &seed,
                &body,
                &format!("{observe}\nMOV E2 D250\nMOV F2 D251\nMOV D60 D252"),
            ));
        }
    }
    assert_eq!(cases.len(), 94);
}

fn append_aliases(cases: &mut Vec<Case>) {
    // Only canonical E/F backing words are seeded. E0's pair and the F1/F6
    // pairs stay within that scope; no clock/system neighbor is initialized.
    for op in FORMS {
        for (modifier, alias) in [("E0", 1028u16), ("E1", 1182), ("F1", 1183), ("F6", 1193)] {
            let mut seed = (0..8u16)
                .map(|i| format!("MOV H55AA E{i}\nMOV H6A5A F{i}"))
                .collect::<Vec<_>>()
                .join("\n");
            seed.push_str(&format!("\nMOV K-1 {modifier}"));
            let body = format!(
                "LDI X7\nMOV K-1 {modifier}\nLD X0\n{op} F1.2 D{}{modifier}\nOUT Y10",
                alias + 1
            );
            let observe = (0..8u16)
                .map(|i| format!("MOV E{i} D{}\nMOV F{i} D{}", 220 + i * 2, 221 + i * 2))
                .collect::<Vec<_>>()
                .join("\n");
            cases.push(scenario(
                &format!("cached_pair_overwrites_own_index_{op}_{modifier}"),
                &seed,
                &body,
                &observe,
            ));
        }
    }
    assert_eq!(cases.len(), 102);

    for op in FORMS {
        for (label, literal) in [
            ("fraction", "F1.2"),
            ("one", "F1.0"),
            ("negative_zero", "F-0.0"),
            ("low_bit_one", "F1.0000001"),
        ] {
            let seed = "SET M1018\nSET M1019\nSET M1020\nSET M1021\nSET M1022\nSET M1067\nSET M1068\nMOV K1 E1\nMOV H6A5A F1";
            let body =
                format!("{op} {literal} K1M1018E1\nLD M1018\nOUT Y0\nLDI X7\nMOV K1M1019 D80");
            cases.push(scenario(
                &format!("raw_flag_overlap_sticky_errors_{op}_{label}"),
                seed,
                &body,
                "MOV D80 D220\nMOV K1M1019 D221\nMOV E1 D250\nMOV F1 D251",
            ));
        }
    }
    assert_eq!(cases.len(), 110);

    for op in FORMS {
        let seed = format!(
            "{}\n{}\nMOV K0 E1\nMOV H6A5A F1",
            seed_words("D", 79, 7),
            seed_words("D", 90, 21)
        );
        let body = ["F1.2", "F-0.0", "F1.0"]
            .into_iter()
            .enumerate()
            .map(|(slot, literal)| {
                format!(
                    "LDI X7\nMOV K{slot} E1\nLD X0\n{op} {literal} D80E1\nLDI X7\n{}",
                    read_words("D", 79, 7, 90 + slot as u16 * 7)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        cases.push(scenario(
            &format!("same_scan_shifted_numeric_pair_overlap_{op}"),
            &seed,
            &body,
            &format!(
                "{}\n{}\nMOV E1 D250",
                read_words("D", 79, 7, 200),
                read_words("D", 90, 21, 220)
            ),
        ));

        let seed = format!(
            "{}\n{}\nMOV H7BBB E2\nMOV K0 F2",
            seed_bits("M", 124, 48),
            seed_words("D", 80, 9)
        );
        let body = [(0, "F1.2"), (1, "F-0.0"), (3, "F1.0")]
            .into_iter()
            .enumerate()
            .map(|(slot, (offset, literal))| {
                format!(
                    "LDI X7\nMOV K{offset} F2\nLD X0\n{op} {literal} K8M128F2\nLDI X7\n{}",
                    read_bits("M", 124, 48, 80 + slot as u16 * 3)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        cases.push(scenario(
            &format!("same_scan_shifted_group_bit_overlap_{op}"),
            &seed,
            &body,
            &format!(
                "{}\n{}\nMOV F2 D250",
                read_bits("M", 124, 48, 220),
                read_words("D", 80, 9, 230)
            ),
        ));
    }
    assert_eq!(cases.len(), 114);
}

fn append_controls(cases: &mut Vec<Case>) {
    for op in FORMS {
        let seed = format!(
            "{}\n{}\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 D30",
            seed_words("D", 159, 5),
            seed_words("D", 80, 6)
        );
        let body = format!(
            "LDI X7\nMOV K0 D30\nFOR K3\nLDI X7\nMUL16 D30 K2 E2\nMOV E2 F2\nLD= D30 K1\nDMOV H55555555 D160\nLD= D30 K0\nOR= D30 K2\nAND X0\n{op} F1.2 D160E1\nLDI X7\nDMOV D160 D80F2\nINC D30\nNEXT"
        );
        cases.push(scenario(
            &format!("control_for_same_pc_true_false_true_{op}"),
            &seed,
            &body,
            &format!(
                "{}\n{}\nMOV D30 D250",
                read_words("D", 159, 5, 220),
                read_words("D", 80, 6, 230)
            ),
        ));

        let seed = format!("{}\nMOV K0 E1\nMOV H6A5A F1", seed_words("D", 159, 7));
        let body = format!(
            "MPS\n{op} F1.2 D160E1\nMRD\nMOV K2 E1\nMRD\n{op} F-0.0 D160E1\nMPP\nOUT Y10\nLDI X7\nMOV K0 E1"
        );
        cases.push(scenario(
            &format!("control_mps_two_offsets_{op}"),
            &seed,
            &body,
            &read_words("D", 159, 7, 220),
        ));

        let seed = format!(
            "{}\nMOV K0 E1\nMOV H6A5A F1\nMOV K0 D60",
            seed_words("D", 159, 5)
        );
        let body = format!(
            "LD X2\nDMOV H55555555 D160\nLD X2\nCJ P10\nLD X0\n{op} F1.2 D160E1\nP10\nLDI X7\nINC D60"
        );
        let mut entry = scenario(
            &format!("control_cj_skipped_false_keeps_pulse_history_{op}"),
            &seed,
            &body,
            &format!("{}\nMOV D60 D250", read_words("D", 159, 5, 220)),
        );
        entry.stages = vec![
            (
                vec![("X1", true), ("X0", false), ("X2", false), ("X7", false)],
                1,
            ),
            (vec![("X1", false), ("X0", true)], 1),
            (vec![("X0", false), ("X2", true)], 1),
            (vec![("X0", true), ("X2", false)], 1),
            (vec![("X0", false)], 1),
            (vec![("X0", true)], 1),
        ];
        cases.push(entry);

        let seed = format!(
            "{}\nMOV K0 E1\nMOV H6A5A F1\nMOV K0 D60",
            seed_words("D", 159, 5)
        );
        let body = format!("LD X0\nMC N0\nLDI X7\n{op} F1.2 D160E1\nMCR N0\nLDI X7\nINC D60");
        cases.push(scenario(
            &format!("control_master_false_true_logic_{op}"),
            &seed,
            &body,
            &format!("{}\nMOV D60 D250", read_words("D", 159, 5, 220)),
        ));

        let seed = format!(
            "ZRST S0 S1023\nRST M1040\nRST M1045\nRST M1047\n{}\nMOV K0 E1\nMOV H6A5A F1\nMOV K0 D60",
            seed_words("D", 159, 5)
        );
        let body = format!("STL S0\nLDI X7\n{op} F1.2 D160E1\nRET\nLDI X7\nINC D60");
        cases.push(scenario(
            &format!("control_inactive_step_true_logic_{op}"),
            &seed,
            &body,
            &format!(
                "{}\nMOV K1S0 D240\nMOV D60 D250",
                read_words("D", 159, 5, 220)
            ),
        ));
    }
    assert_eq!(cases.len(), 124);
}

pub(super) fn model_cases(target: Target) -> Vec<Case> {
    let last: u16 = match target {
        Target::Es2 => 9999,
        Target::Ss2 => 4999,
    };
    let mut cases = Vec::new();
    for op in FORMS {
        let seed = format!("{}\nMOV K-1 E1\nMOV H6A5A F1", seed_words("D", last - 2, 3));
        cases.push(scenario(
            &format!("{}_last_numeric_pair_{op}", target.name()),
            &seed,
            &format!("{op} F1.2 D{last}E1\nOUT Y10"),
            &format!(
                "{}\nMOV E1 D250\nMOV F1 D251",
                read_words("D", last - 2, 3, 220)
            ),
        ));
        for (area, end) in [("Y", 255u16), ("M", 4095), ("S", 1023)] {
            let seed = format!(
                "{}\n{}\nMOV H7BBB E2\nMOV K-3 F2",
                seed_bits(area, end - 35, 36),
                seed_words("D", 80, 9)
            );
            let body = [1, 4, 8u16]
                .into_iter()
                .enumerate()
                .map(|(slot, digits)| {
                    format!(
                        "LDI X7\nMOV K-{} F2\nLD X0\n{op} F-0.0 K{digits}{}F2\nLDI X7\n{}",
                        digits * 4 - 1,
                        address(area, end),
                        read_bits(area, end - 35, 36, 80 + slot as u16 * 3)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let observe = format!(
                "{}\n{}\nMOV F2 D250",
                read_bits(area, end - 35, 36, 220),
                read_words("D", 80, 9, 230)
            );
            cases.push(scenario(
                &format!("{}_last_{area}_K1_K4_K8_negative_zero_{op}", target.name()),
                &seed,
                &body,
                &observe,
            ));
        }
    }
    check_cases(&cases, 8);
    cases
}
