//! Indexed forward/inverse trigonometry with explicit mode/domain distinctions.
//! Cutoff, poles and nonfinite response probes are separately named, never filtered.
use super::{Case, Target, case};
use std::collections::BTreeSet;

const FORMS: [&str; 12] = [
    "DSIN", "DSINP", "DCOS", "DCOSP", "DTAN", "DTANP", "DASIN", "DASINP", "DACOS", "DACOSP",
    "DATAN", "DATANP",
];

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

fn result_site(op: &str, operand: &str, slot: u16, degrees: bool) -> String {
    format!(
        "LD X0\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\n{} M1018\n{}\n{op} {operand} D{}F2\nOUT Y{:o}\n{}\nLDI X7\nMOV D1067 D{}\nMOV D1068 D{}",
        if degrees { "SET" } else { "RST" },
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
        "LD X1\nMOV H55AA E0\nMOV H1234 F0\nRST M1018\nSET M1020\nSET M1021\nSET M1022\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068",
        seed, gate, body,
        "LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24",
        "LDI X7\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305\nMOV K1234 D306",
        observe, "END",
    ].into_iter().filter(|part| !part.is_empty()).collect::<Vec<_>>().join("\n");
    assert!(
        source.lines().count() <= 512,
        "oversized indexed trig case {name}"
    );
    assert!(
        source.lines().count() * 9 <= 0x1ef0,
        "installed program budget: {name}"
    );
    let mut indexed = false;
    for line in source.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.first().is_some_and(|op| FORMS.contains(op)) {
            assert_eq!(fields.len(), 3, "trig field count: {name}: {line}");
            assert!(
                (fields[1].starts_with('K')
                    || fields[1].starts_with('H')
                    || fields[1].starts_with('D'))
                    && fields[2].starts_with('D'),
                "trig operand roles: {name}: {line}"
            );
            indexed |= fields[1..].iter().any(|operand| {
                operand.starts_with('D')
                    && operand.len() > 3
                    && matches!(operand.as_bytes()[operand.len() - 2], b'E' | b'F')
                    && operand.as_bytes()[operand.len() - 1].is_ascii_digit()
            });
        }
    }
    assert!(indexed, "missing indexed trig action in {name}");
    case(&format!("indexed_trig_{name}"), &source, stages())
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
        assert!(actual.contains(op), "missing trig spelling {op}");
    }
}

fn forward(op: &str) -> bool {
    matches!(op.trim_end_matches('P'), "DSIN" | "DCOS" | "DTAN")
}

fn raw_case(op: &str, label: &str, values: &[(u32, bool)]) -> Case {
    let count = values.len() as u16;
    let mut seed = format!(
        "{}\n{}\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K1 F2",
        seed_words(159, count * 3 + 1),
        seed_words(80, count * 2)
    );
    let mut body = Vec::new();
    for (slot, (raw, degrees)) in values.iter().enumerate() {
        let slot = slot as u16;
        seed.push_str(&format!("\nDMOV H{raw:08X} D{}", 32 + slot * 2));
        body.push(result_site(
            op,
            &format!("D{}E1", 33 + slot * 2),
            slot,
            *degrees,
        ));
    }
    let observe = format!(
        "{}\n{}\n{}",
        read_words(32, count * 2, 200),
        read_words(159, count * 3 + 1, 230),
        read_words(80, count * 2, 270)
    );
    scenario(&format!("{label}_{op}"), &seed, &body.join("\n"), &observe)
}

pub(super) fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for bank in ['E', 'F'] {
        for index in 0..8u16 {
            let modifier = format!("{bank}{index}");
            let pair = index_base(index);
            let alias = pair + u16::from(bank == 'F');
            let companion = pair + u16::from(bank == 'E');
            let delta = i32::from(index % 4) - 2;
            for family in ["DSIN", "DCOS", "DTAN", "DASIN", "DACOS", "DATAN"] {
                let pulse = (index + u16::from(bank == 'F')) % 2 == 1;
                let op = format!("{family}{}", if pulse { "P" } else { "" });
                // All adjacent pairs remain inside [-1,1], including inverse
                // inputs when the live index moves by one individual D word.
                let inputs = (0..9)
                    .map(|i| {
                        format!(
                            "MOV H{:04X} D{}",
                            [0x3e80, 0xbe80, 0x3f00, 0xbf00, 0x3e00, 0xbe00][i % 6],
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
                    "{inputs}\n{}\nMOV H5AA5 D{companion}\n{poison}\nMOV K{delta} D{alias}\n{} M1018",
                    seed_words(155, 16),
                    if index % 2 == 0 { "RST" } else { "SET" }
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
    assert_eq!(cases.len(), 96);

    for op in FORMS.into_iter().filter(|op| forward(op)) {
        let rad = [
            0, 0x80000000, 0x3f060a92, 0x3f490fdb, 0x40490fdb, 0x40c90fdb, 0xbf490fdb, 0xc0490fdb,
        ]
        .map(|raw| (raw, false));
        cases.push(raw_case(op, "radian_reference_vectors", &rad));
        let degree_values: &[u32] = if op.starts_with("DTAN") {
            &[
                0, 0x41f00000, 0x42340000, 0x42700000, 0x43340000, 0x43b40000, 0xc1f00000,
                0xc2700000,
            ]
        } else {
            &[
                0, 0x41f00000, 0x42340000, 0x42700000, 0x42b40000, 0x43340000, 0x43870000,
                0x43b40000, 0xc1f00000,
            ]
        };
        let degrees = degree_values
            .iter()
            .map(|raw| (*raw, true))
            .collect::<Vec<_>>();
        cases.push(raw_case(op, "degree_reference_without_clamp", &degrees));
        let rounding = [
            0x41efffff, 0x41f00000, 0x41f00001, 0x4233ffff, 0x42340000, 0x42340001, 0x43b3ffff,
            0x43b40000, 0x43b40001, 0xc1efffff, 0xc1f00000, 0xc1f00001,
        ]
        .map(|raw| (raw, true));
        cases.push(raw_case(op, "degree_two_rounding_boundaries", &rounding));
        let cutoff = [
            0x59ffffff, 0x5a000000, 0x5a000001, 0xd9ffffff, 0xda000000, 0xda000001,
        ]
        .into_iter()
        .flat_map(|raw| [(raw, false), (raw, true)])
        .collect::<Vec<_>>();
        cases.push(raw_case(op, "cutoff_response_both_modes", &cutoff));
        let nonfinite = [
            0x7f800000, 0xff800000, 0x7fc01234, 0xffc01234, 0x7f800001, 0xff800001,
        ]
        .into_iter()
        .flat_map(|raw| [(raw, false), (raw, true)])
        .collect::<Vec<_>>();
        cases.push(raw_case(op, "nonfinite_response_both_modes", &nonfinite));
        if op.starts_with("DTAN") {
            let poles = [
                (0x3fc90fda, false),
                (0x3fc90fdb, false),
                (0x3fc90fdc, false),
                (0x4096cbe3, false),
                (0x4096cbe4, false),
                (0x4096cbe5, false),
                (0x42b3ffff, true),
                (0x42b40000, true),
                (0x42b40001, true),
                (0x4386ffff, true),
                (0x43870000, true),
                (0x43870001, true),
            ];
            cases.push(raw_case(op, "pole_neighborhood_response", &poles));
        }
    }
    assert_eq!(cases.len(), 128);

    for op in FORMS.into_iter().filter(|op| !forward(op)) {
        let endpoints = [
            0xbf800000, 0xbf7fffff, 0xbf000001, 0xbf000000, 0xbeffffff, 0x80000000, 0, 0x3effffff,
            0x3f000000, 0x3f000001, 0x3f7fffff, 0x3f800000,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, raw)| (raw, i % 2 == 1))
        .collect::<Vec<_>>();
        cases.push(raw_case(op, "inverse_endpoints_half_branch", &endpoints));
        let outside = [
            0x3f800001, 0xbf800001, 0x40000000, 0xc0000000, 0x7f7fffff, 0xff7fffff, 0x00800000,
            0x80800000,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, raw)| (raw, i % 2 == 1))
        .collect::<Vec<_>>();
        cases.push(raw_case(
            op,
            if op.starts_with("DATAN") {
                "large_finite_response"
            } else {
                "inverse_outside_endpoints_and_tiny"
            },
            &outside,
        ));
        let nan = [
            (0x7fc01234, false),
            (0xffc01234, true),
            (0x7f800001, false),
            (0xff800001, true),
        ];
        cases.push(raw_case(op, "inverse_quiet_signaling_nan_response", &nan));
        cases.push(raw_case(
            op,
            "inverse_infinity_response",
            &[(0x7f800000, false), (0xff800000, true)],
        ));
    }
    assert_eq!(cases.len(), 152);

    // One unchanged source/site: change M1018 while powered and P is held.
    // Inverse outputs must remain invariant; forward outputs switch only when
    // the instruction actually executes. All indices stay valid while gated.
    for op in FORMS {
        let raw = if forward(op) {
            0x41f00000u32
        } else {
            0x3f000000
        };
        let seed = format!(
            "{}\nDMOV H{raw:08X} D32\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nRST M1018",
            seed_words(159, 5)
        );
        let body = format!("LD X2\nSET M1018\nLD X0\n{op} D32E1 D160F2\nOUT Y10");
        let observe = format!(
            "{}\nDMOV D32 D240\nLD M1018\nOUT Y25",
            read_words(159, 5, 220)
        );
        let mut entry = scenario(
            &format!("same_site_mode_change_held_rearm_{op}"),
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
    assert_eq!(cases.len(), 164);

    for op in FORMS {
        let mut operands = vec![
            "K0",
            "K1",
            "K-1",
            "H00000001",
            "HFFFFFFFF",
            "K30",
            "H0000001E",
            "K90",
            "H0000005A",
            "H3F800000",
            "D32E1",
            "D36E1",
        ];
        if op.starts_with("DTAN") {
            operands[7] = "K60";
            operands[8] = "H0000003C";
        }
        let count = operands.len() as u16;
        let seed = format!(
            "{}\n{}\n{}\nDMOV H3F800000 D32\nDMOV H41F00000 D36\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K1 F2",
            seed_words(31, 9),
            seed_words(159, count * 3 + 1),
            seed_words(80, count * 2)
        );
        let body = operands
            .iter()
            .enumerate()
            .map(|(slot, operand)| result_site(op, operand, slot as u16, true))
            .collect::<Vec<_>>()
            .join("\n");
        let observe = format!(
            "{}\n{}\n{}",
            read_words(31, 9, 200),
            read_words(159, count * 3 + 1, 230),
            read_words(80, count * 2, 270)
        );
        cases.push(scenario(
            &format!("integer_K_H_vs_raw_D_{op}"),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 176);

    for op in FORMS.into_iter().filter(|op| !forward(op)) {
        let mut invalid = vec![(
            "nan_priority_e1a",
            if op.ends_with('P') {
                0xffc01234u32
            } else {
                0x7fc01234
            },
        )];
        if !op.starts_with("DATAN") {
            invalid.push((
                "outside_e19",
                if op.ends_with('P') {
                    0xbf800001
                } else {
                    0x3f800001
                },
            ));
        }
        for (slot, (label, raw)) in invalid.into_iter().enumerate() {
            let sticky = if slot == 1 {
                "SET M1067\nSET M1068\nMOV H0123 D1067\nMOV K77 D1068"
            } else {
                ""
            };
            let seed = format!(
                "{}\nDMOV H{raw:08X} D32\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K-1 F2\nMOV K0 D60\n{}\nSET M1018\n{sticky}",
                seed_words(0, 4),
                set_flags(if slot == 0 { 5 } else { 7 })
            );
            let body = format!(
                "LD X2\nDMOV H3F000000 D32\nMOV K0 F2\nLD X0\n{op} D32E1 D0F2\nOUT Y10\nINC D60"
            );
            let observe = format!(
                "{}\nDMOV D32 D230\nMOV D60 D240\nMOV E1 D250\nMOV F2 D251",
                read_words(0, 4, 220)
            );
            let mut entry = scenario(
                &format!("domain_skips_bad_target_{label}_recovery_{op}"),
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
    assert_eq!(cases.len(), 186);

    for (which, op) in FORMS.into_iter().enumerate() {
        let (modifier, alias, companion) = if which % 2 == 0 {
            ("E1", 1182, 1183)
        } else {
            ("F2", 1185, 1184)
        };
        let lexical = alias - 1;
        let seed = format!(
            "{}\n{}\nDMOV H3F000000 D32\nMOV K1 E1\nMOV K1 F2\nMOV H5AA5 D{companion}",
            seed_words(alias - 1, 4),
            seed_words(31, 5)
        );
        let body = format!(
            "LDI X7\nDMOV H3F000000 D32\nMOV K1 E1\nMOV K1 F2\nLD X0\n{op} D31E1 D{lexical}{modifier}\n{}\nLDI X7\nDMOV D{alias} D80\nMOV K0 E1\nMOV K0 F2\nLD X0\n{op} D32E1 D33F2\n{}\nLDI X7\nDMOV D33 D84",
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
    assert_eq!(cases.len(), 198);

    for op in ["DSINP", "DCOS", "DTANP", "DASIN", "DACOSP", "DATAN"] {
        let seed = format!(
            "{}\nDMOV H00000000 D32\nDMOV H3F000000 D34\nDMOV HBF000000 D36\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",
            seed_words(159, 10)
        );
        let body = (0..3u16)
            .map(|slot| {
                format!(
                    "LDI X7\nMOV K{} E1\nMOV K{} F2\n{} M1018\nLD X0\n{op} D32E1 D160F2\n{}",
                    slot * 2,
                    slot * 3,
                    if slot % 2 == 0 { "RST" } else { "SET" },
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
            &format!("same_scan_live_offsets_and_mode_{op}"),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 204);

    cases.push(scenario(
        "control_DSIN_mps_degree_radian",
        &format!(
            "{}\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",
            seed_words(159, 8)
        ),
        "MPS\nSET M1018\nMRD\nDSIN K30 D160E1\nMRD\nRST M1018\nMRD\nDSIN K30 D164F2\nMPP\nOUT Y10",
        &read_words(159, 8, 220),
    ));
    for op in ["DCOSP", "DACOSP"] {
        let seed = format!(
            "{}\n{}\nDMOV H3F000000 D32\nMOV K0 D30\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 E3\nMOV H1234 F3",
            seed_words(159, 4),
            seed_words(80, 8)
        );
        let body = format!(
            "LDI X7\nMOV K0 D30\nFOR K3\nLDI X7\nMUL16 D30 K2 E3\nLD= D30 K0\nOR= D30 K2\nAND X0\n{op} D32E1 D160F2\nLDI X7\nDMOV D160 D80E3\nINC D30\nNEXT"
        );
        let observe = format!(
            "{}\n{}\nMOV D30 D250",
            read_words(159, 4, 220),
            read_words(80, 8, 230)
        );
        cases.push(scenario(
            &format!("control_{op}_for_true_false_true"),
            &seed,
            &body,
            &observe,
        ));
    }
    cases.push(scenario(
        "control_DTAN_cj_bounded_radian",
        &format!(
            "{}\nDMOV H3F000000 D32\nMOV K0 D50\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",
            seed_words(159, 5)
        ),
        "LDI X0\nCJ P10\nLD X0\nDTAN D32E1 D160F2\nP10\nLDI X7\nMOV K123 D50",
        &format!("{}\nMOV D50 D240", read_words(159, 5, 220)),
    ));
    cases.push(scenario(
        "control_DASIN_mps_integer_endpoints",
        &format!(
            "{}\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nSET M1018",
            seed_words(159, 8)
        ),
        "MPS\nDASIN K-1 D160E1\nMRD\nDASIN K1 D164F2\nMPP\nOUT Y10",
        &read_words(159, 8, 220),
    ));
    cases.push(scenario(
        "control_DATAN_mps_changed_target",
        &format!(
            "{}\nDMOV H3F000000 D32\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nSET M1018",
            seed_words(159, 6)
        ),
        "MPS\nDATAN D32E1 D160F2\nMRD\nMOV K1 F2\nMPP\nOUT Y10",
        &format!("{}\nMOV F2 D250", read_words(159, 6, 220)),
    ));
    check_cases(&cases, 210);
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
            "{}\n{}\nDMOV H3F000000 D{pair}\nDMOV H5555AAAA D80\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\n{} M1018",
            seed_words(last - 2, 3),
            seed_words(19, 5),
            if op.ends_with('P') { "SET" } else { "RST" }
        );
        let body = format!(
            "LDI X7\nDMOV H3F000000 D{pair}\nLD X0\n{op} D{last}E1 D20F2\n{op} K0 D{last}E1\nLDI X7\nDMOV D{pair} D80"
        );
        let observe = format!(
            "{}\n{}\nDMOV D80 D240\nMOV E1 D250\nMOV F2 D251",
            read_words(last - 2, 3, 200),
            read_words(19, 5, 220)
        );
        cases.push(scenario(
            &format!("model_{}_last_pair_source_and_target_{op}", target.name()),
            &seed,
            &body,
            &observe,
        ));
    }
    check_cases(&cases, 12);
    cases
}
