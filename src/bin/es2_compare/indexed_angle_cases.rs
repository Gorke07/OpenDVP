//! Indexed DRAD/DDEG: K/H are signed integers, while D pairs carry raw f32.
//! Extreme finite, quiet/signaling NaN and infinity probes remain separate.
//! No semantic rejection or invalid-target bypass is assumed for this family.
use super::{Case, Target, case};
use std::collections::BTreeSet;

const FORMS: [&str; 4] = ["DRAD", "DRADP", "DDEG", "DDEGP"];

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
        "LD X0\n{}\n{op} {operand} D{}F2\nOUT Y{:o}\n{}\nLDI X7\nMOV D1067 D{}\nMOV D1068 D{}",
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
        "oversized indexed angle case {name}"
    );
    assert!(
        source.lines().count() * 9 <= 0x1ef0,
        "installed program budget: {name}"
    );
    let mut indexed = false;
    for line in source.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.first().is_some_and(|op| FORMS.contains(op)) {
            assert_eq!(fields.len(), 3, "angle field count: {name}: {line}");
            assert!(
                (fields[1].starts_with('K')
                    || fields[1].starts_with('H')
                    || fields[1].starts_with('D'))
                    && fields[2].starts_with('D'),
                "angle operand roles: {name}: {line}"
            );
            indexed |= fields[1..].iter().any(|operand| {
                operand.starts_with('D')
                    && operand.len() > 3
                    && matches!(operand.as_bytes()[operand.len() - 2], b'E' | b'F')
                    && operand.as_bytes()[operand.len() - 1].is_ascii_digit()
            });
        }
    }
    assert!(indexed, "missing indexed angle action in {name}");
    case(&format!("indexed_angle_{name}"), &source, stages())
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
        assert!(actual.contains(op), "missing angle spelling {op}");
    }
}

pub(super) fn cases() -> Vec<Case> {
    let mut cases = Vec::new();

    // All sixteen signed WORD modifiers serve both D roles. Moving offsets
    // remain bounded, and every adjacent source pair is finite and ordinary.
    for bank in ['E', 'F'] {
        for index in 0..8u16 {
            let modifier = format!("{bank}{index}");
            let pair = index_base(index);
            let alias = pair + u16::from(bank == 'F');
            let companion = pair + u16::from(bank == 'E');
            let delta = i32::from(index % 4) - 2;
            for family in ["DRAD", "DDEG"] {
                let pulse = (index + u16::from(bank == 'F')) % 2 == 1;
                let op = format!("{family}{}", if pulse { "P" } else { "" });
                let inputs = (0..9)
                    .map(|i| {
                        format!(
                            "MOV H{:04X} D{}",
                            [0x3f80, 0x4000, 0x4040, 0x4080, 0x4100, 0x3f00][i % 6],
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

    let raw_sets: [(&str, &[u32]); 6] = [
        (
            "finite_zero_subnormal",
            &[
                0x00000000, 0x80000000, 0x00000001, 0x80000001, 0x007fffff, 0x807fffff, 0x00800000,
                0x80800000,
            ],
        ),
        (
            "rounding_180_pi_neighbors",
            &[
                0x4333ffff, 0x43340000, 0x43340001, 0xc333ffff, 0xc3340000, 0xc3340001, 0x40490fda,
                0x40490fdb, 0x40490fdc, 0xc0490fda, 0xc0490fdb, 0xc0490fdc,
            ],
        ),
        (
            "quiet_nan_sign_and_payload",
            &[0x7fc00001, 0xffc00001, 0x7fffffff, 0xffffffff],
        ),
        (
            "signaling_nan_sign_and_payload",
            &[0x7f800001, 0xff800001, 0x7fbfffff, 0xffbfffff],
        ),
        ("infinity_both_signs", &[0x7f800000, 0xff800000]),
        // These four explicitly named cases are retained even if a reference
        // times out. R/DE exception outcomes do not establish angle outcomes.
        (
            "extreme_finite",
            &[
                0x7f7fffff, 0xff7fffff, 0x7f000000, 0xff000000, 0x7cf00000, 0xfcf00000,
            ],
        ),
    ];
    for op in FORMS {
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
    assert_eq!(cases.len(), 56);

    // The hexadecimal spelling of float pi is still an INTEGER when passed
    // directly. Its D-backed counterpart is raw pi. K/H 2^24+1 also round at
    // the integer-to-binary32 boundary before multiplying by the angle factor.
    for op in FORMS {
        let operands = [
            "K180",
            "H000000B4",
            "D32E1",
            "K-180",
            "HFFFFFF4C",
            "D35F2",
            "H40490FDB",
            "D40E1",
            "K16777217",
            "H01000001",
            "H80000000",
            "H7FFFFFFF",
        ];
        let count = operands.len() as u16;
        let seed = format!(
            "{}\n{}\n{}\nDMOV H43340000 D32\nDMOV HC3340000 D36\nDMOV H40490FDB D40\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K1 F2",
            seed_words(31, 13),
            seed_words(159, count * 3 + 1),
            seed_words(80, count * 2)
        );
        // D35F2 resolves to the raw negative-180 pair; the same F2 also
        // addresses each output pair one word beyond its lexical D base.
        let body = operands
            .iter()
            .enumerate()
            .map(|(slot, operand)| result_site(op, operand, slot as u16))
            .collect::<Vec<_>>()
            .join("\n");
        let observe = format!(
            "{}\n{}\n{}",
            read_words(31, 13, 200),
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
    assert_eq!(cases.len(), 60);

    // A cached target pair must survive overwriting its own index WORD. The
    // second site overlaps the source's high half. Refresh its finite source
    // every scan so that unrelated accidental extreme floats cannot accumulate.
    for (which, op) in FORMS.into_iter().enumerate() {
        let (modifier, alias, companion) = if which % 2 == 0 {
            ("E1", 1182, 1183)
        } else {
            ("F2", 1185, 1184)
        };
        let lexical = alias - 1;
        let seed = format!(
            "{}\n{}\nDMOV H43340000 D32\nMOV K1 E1\nMOV K1 F2\nMOV H5AA5 D{companion}",
            seed_words(alias - 1, 4),
            seed_words(31, 5)
        );
        let body = format!(
            "LDI X7\nDMOV H43340000 D32\nMOV K1 E1\nMOV K1 F2\nLD X0\n{op} D31E1 D{lexical}{modifier}\n{}\nLDI X7\nDMOV D{alias} D80\nMOV K0 E1\nMOV K0 F2\nLD X0\n{op} D32E1 D33F2\n{}\nLDI X7\nDMOV D33 D84",
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
    assert_eq!(cases.len(), 64);

    for op in FORMS {
        let seed = format!(
            "{}\nDMOV H43340000 D32\nDMOV H40490FDB D34\nDMOV HC0490FDB D36\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",
            seed_words(159, 10)
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
    assert_eq!(cases.len(), 68);

    cases.push(scenario(
        "control_DRAD_mps_180_pi_roundtrip",
        &format!(
            "{}\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",
            seed_words(159, 8)
        ),
        "MPS\nDRAD K180 D160E1\nMRD\nDDEG D160E1 D164F2\nMPP\nOUT Y10",
        &read_words(159, 8, 220),
    ));
    cases.push(scenario(
        "control_DRADP_for_true_false_true_same_pc",
        &format!("{}\n{}\nDMOV H43340000 D32\nMOV K0 D30\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 E3\nMOV H1234 F3", seed_words(159, 4), seed_words(80, 8)),
        "LDI X7\nMOV K0 D30\nFOR K3\nLDI X7\nMUL16 D30 K2 E3\nLD= D30 K0\nOR= D30 K2\nAND X0\nDRADP D32E1 D160F2\nLDI X7\nDMOV D160 D80E3\nINC D30\nNEXT",
        &format!("{}\n{}\nMOV D30 D250", read_words(159, 4, 220), read_words(80, 8, 230)),
    ));
    cases.push(scenario(
        "control_DDEG_cj_skipped_false_visits",
        &format!(
            "{}\nDMOV H40490FDB D32\nMOV K0 D50\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",
            seed_words(159, 5)
        ),
        "LDI X0\nCJ P10\nLD X0\nDDEG D32E1 D160F2\nP10\nLDI X7\nMOV K123 D50",
        &format!("{}\nMOV D50 D240", read_words(159, 5, 220)),
    ));
    cases.push(scenario(
        "control_DDEGP_mps_changed_target",
        &format!(
            "{}\nDMOV H40490FDB D32\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",
            seed_words(159, 6)
        ),
        "MPS\nDDEGP D32E1 D160F2\nMRD\nMOV K1 F2\nMPP\nOUT Y10",
        &format!("{}\nMOV E1 D250\nMOV F2 D251", read_words(159, 6, 220)),
    ));
    check_cases(&cases, 72);
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
        let raw = if op.starts_with("DRAD") {
            0x43340000u32
        } else {
            0x40490fdb
        };
        let seed = format!(
            "{}\n{}\nDMOV H{raw:08X} D{pair}\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",
            seed_words(last - 2, 3),
            seed_words(19, 5)
        );
        let body = format!("{op} D{last}E1 D20F2\nDMOV H3F800000 D{pair}");
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

        let literal = if op.ends_with('P') {
            "H000000B4"
        } else {
            "K180"
        };
        let seed = format!("{}\nMOV K-1 F7\nMOV H1234 E7", seed_words(last - 3, 4));
        let body = format!("{op} {literal} D{last}F7\nMOV K-2 F7");
        let observe = format!("{}\nMOV F7 D250\nMOV E7 D251", read_words(last - 3, 4, 220));
        cases.push(scenario(
            &format!("model_{}_literal_last_pair_target_{op}", target.name()),
            &seed,
            &body,
            &observe,
        ));
    }
    check_cases(&cases, 8);
    cases
}
