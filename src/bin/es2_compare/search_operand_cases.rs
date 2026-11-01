//! Direct E/F search values, with unchanged source/result/count roles.
//! Every program uses the same OFF / ON twice / OFF / ON pulse lifecycle.
use super::{Case, case};
use std::collections::BTreeSet;

const FORMS: [&str; 4] = ["SER", "SERP", "DSER", "DSERP"];

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (vec![("X1", true), ("X0", false)], 1),
        (vec![("X1", false), ("X0", true)], 2),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}

fn index_base(index: u16) -> u16 {
    if index == 0 { 1028 } else { 1180 + index * 2 }
}

fn index_observations(index: u16) -> String {
    let base = index_base(index);
    format!(
        "MOV E{index} D60\nMOV F{index} D61\nMOV D{base} D62\nMOV D{} D63",
        base + 1
    )
}

fn seed_words(values: &[i32], wide: bool, base: u16) -> String {
    let mov = if wide { "DMOV" } else { "MOV" };
    let stride = if wide { 2 } else { 1 };
    values
        .iter()
        .enumerate()
        .map(|(i, value)| format!("{mov} K{value} D{}", base + i as u16 * stride))
        .collect::<Vec<_>>()
        .join("\n")
}

fn seed_results(wide: bool, base: u16) -> String {
    let after = base + if wide { 10 } else { 5 };
    format!(
        "MOV H5AA5 D{}\n{}\nMOV H3CC3 D{after}",
        base - 1,
        seed_words(&[-12345; 5], wide, base)
    )
}

fn result_observations(wide: bool, base: u16) -> String {
    let mov = if wide { "DMOV" } else { "MOV" };
    let stride = if wide { 2 } else { 1 };
    let mut lines = (0..5)
        .map(|i| format!("{mov} D{} D{}", base + i * stride, 20 + i * stride))
        .collect::<Vec<_>>();
    lines.push(format!("MOV D{} D50", base - 1));
    lines.push(format!("MOV D{} D51", base + 5 * stride));
    lines.join("\n")
}

fn address(area: &str, index: u16) -> String {
    if area == "Y" {
        format!("Y{index:o}")
    } else {
        format!("{area}{index}")
    }
}

fn seed_groups(area: &str, base: u16, bits: u16, values: &[i32]) -> String {
    values
        .iter()
        .enumerate()
        .flat_map(|(i, value)| {
            (0..bits).map(move |bit| {
                format!(
                    "{} {}",
                    if (*value as u32) & (1u32 << bit) != 0 {
                        "SET"
                    } else {
                        "RST"
                    },
                    address(area, base + i as u16 * bits + bit)
                )
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn scenario(name: &str, seed: &str, body: &str, guards: &[(&str, bool)], observe: &str) -> Case {
    let guard_seed = guards
        .iter()
        .map(|(name, on)| format!("{} {name}", if *on { "SET" } else { "RST" }))
        .collect::<Vec<_>>()
        .join("\n");
    let guard_reads = guards
        .iter()
        .enumerate()
        .map(|(i, (name, _))| format!("LD {name}\nOUT Y{:o}", i + 8))
        .collect::<Vec<_>>()
        .join("\n");
    let source = [
        "LD X1\nMOV H55AA D1028\nMOV H1234 D1029",
        seed,
        &guard_seed,
        "SET M1020\nRST M1021\nSET M1022\nSET M1029\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\nLD X0",
        body,
        "LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1029\nOUT Y23\nLD M1067\nOUT Y24\nLD M1068\nOUT Y25",
        &guard_reads,
        "LD X0\nORI X0\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305",
        observe,
        "END",
    ]
    .into_iter()
    .filter(|section| !section.is_empty())
    .collect::<Vec<_>>()
    .join("\n");
    assert!(
        source.lines().count() <= 512,
        "oversized search case {name}"
    );
    case(&format!("search_operand_{name}"), &source, stages())
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for op in FORMS {
        let wide = op.starts_with('D');
        let values = if wide {
            [70005, 70005, 70003, -70007, 70008]
        } else {
            [5, 5, 3, -7, 8]
        };
        for area in if wide { &['E'][..] } else { &['E', 'F'][..] } {
            for index in 0..=7 {
                let base = index_base(index);
                let alias = base + u16::from(*area == 'F');
                let index_seed = if wide {
                    format!("DMOV K70005 D{base}")
                } else if *area == 'E' {
                    format!("MOV K5 D{base}\nMOV H1234 D{}", base + 1)
                } else {
                    format!("MOV H55AA D{base}\nMOV K5 D{alias}")
                };
                cases.push(scenario(
                    &format!("lifecycle_{op}_{area}{index}"),
                    &format!(
                        "{}\n{}\n{index_seed}",
                        seed_words(&values, wide, 0),
                        seed_results(wide, 100)
                    ),
                    &format!(
                        "{op} D0 {area}{index} D100 K5\n{} K{} D{alias}",
                        if wide { "DMOV" } else { "MOV" },
                        values[2]
                    ),
                    &[],
                    &format!(
                        "{}\n{}",
                        result_observations(wide, 100),
                        index_observations(index)
                    ),
                ));
            }
        }
    }
    assert_eq!(cases.len(), 48);

    for op in ["SER", "SERP"] {
        cases.push(scenario(
            &format!("signed_F3_ignores_next_word_{op}"),
            &format!(
                "{}\n{}\nMOV H5AA5 D1186\nMOV K-32768 D1187\nMOV K42 D1188",
                seed_words(&[-32768, 32767, -32768, -1, 32767], false, 0),
                seed_results(false, 100)
            ),
            &format!("{op} D0 F3 D100 K5\nMOV K-1 D1187"),
            &[],
            &format!(
                "{}\n{}\nMOV D1188 D64",
                result_observations(false, 100),
                index_observations(3)
            ),
        ));
    }
    for op in ["DSER", "DSERP"] {
        cases.push(scenario(
            &format!("E3_pair_high_half_and_signed_extrema_{op}"),
            &format!(
                "{}\n{}\nDMOV K98305 D1186",
                seed_words(&[-32767, 98305, -98305, 98305, -32767], true, 0),
                seed_results(true, 100)
            ),
            &format!("{op} D0 E3 D100 K5\nDMOV K-98305 D1186"),
            &[],
            &format!(
                "{}\n{}",
                result_observations(true, 100),
                index_observations(3)
            ),
        ));
    }
    for op in ["SER", "DSER"] {
        let wide = op.starts_with('D');
        cases.push(scenario(
            &format!("E7_no_match_and_last_equal_extrema_{op}"),
            &format!(
                "{}\n{}\n{} K10 D1194",
                seed_words(&[-7, 3, -7, 3, 0, -7], wide, 0),
                seed_results(wide, 100),
                if wide { "DMOV" } else { "MOV" }
            ),
            &format!("{op} D0 E7 D100 K6"),
            &[],
            &format!(
                "{}\n{}",
                result_observations(wide, 100),
                index_observations(7)
            ),
        ));
    }
    for op in ["SERP", "DSERP"] {
        let wide = op.starts_with('D');
        let after = if wide { 1194 } else { 1189 };
        // All source/result words and both sentinels stay inside canonical
        // E1..F7 aliases. The needle is the fourth source/result element.
        cases.push(scenario(
            &format!("source_value_and_target_snapshot_{op}"),
            &format!(
                "MOV H5AA5 D1183\n{}\nMOV H3CC3 D{after}",
                seed_words(&[4, 2, 4, -7, 8], wide, 1184)
            ),
            &format!("{op} D1184 {} D1184 K5", if wide { "E5" } else { "F3" }),
            &[],
            &format!(
                "{}\n{}",
                result_observations(wide, 1184),
                index_observations(if wide { 5 } else { 3 })
            ),
        ));
    }
    for (op, source_digits, dest_area, dest_digits) in [
        ("SER", 1, "Y", 1),
        ("SERP", 4, "S", 4),
        ("DSER", 3, "Y", 2),
        ("DSERP", 8, "S", 8),
    ] {
        let wide = op.starts_with('D');
        let mov = if wide { "DMOV" } else { "MOV" };
        let source_bits = source_digits * 4;
        let dest_bits = dest_digits * 4;
        let source_after = address("M", 128 + source_bits * 5);
        let dest_before = address(dest_area, 127);
        let dest_after = address(dest_area, 128 + dest_bits * 5);
        let observe = (0..5)
            .map(|i| {
                format!(
                    "{mov} K{dest_digits}{} D{}",
                    address(dest_area, 128 + i * dest_bits),
                    20 + i * if wide { 2 } else { 1 }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        cases.push(scenario(
            &format!("packed_source_and_result_direct_E3_{op}"),
            &format!(
                "{}\n{}\n{mov} K3 D1186",
                seed_groups("M", 128, source_bits, &[3, 1, 3, 0, 15]),
                seed_groups(dest_area, 128, dest_bits, &[-1; 5])
            ),
            &format!(
                "{op} K{source_digits}M128 E3 K{dest_digits}{} K5",
                address(dest_area, 128)
            ),
            &[
                ("M127", true),
                (source_after.as_str(), false),
                (dest_before.as_str(), false),
                (dest_after.as_str(), true),
            ],
            &format!("{observe}\n{}", index_observations(3)),
        ));
    }
    cases.push(scenario(
        "invalid_zero_count_holds_results_with_F0",
        &format!(
            "{}\n{}\nMOV K5 D1029",
            seed_words(&[5, 5, 3, -7, 8], false, 0),
            seed_results(false, 100)
        ),
        "SER D0 F0 D100 K0",
        &[],
        &format!(
            "{}\n{}",
            result_observations(false, 100),
            index_observations(0)
        ),
    ));
    cases.push(scenario(
        "invalid_wide_count_then_pulse_rearm_E0",
        &format!(
            "{}\n{}\nDMOV K70005 D1028\nDMOV K129 D250",
            seed_words(&[70005, 70005, 70003, -70007, 70008], true, 0),
            seed_results(true, 100)
        ),
        "DSERP D0 E0 D100 D250\nDMOV K5 D250",
        &[],
        &format!(
            "{}\n{}\nDMOV D250 D64",
            result_observations(true, 100),
            index_observations(0)
        ),
    ));

    assert_eq!(cases.len(), 62);
    let names = cases.iter().map(|case| &case.name).collect::<BTreeSet<_>>();
    assert_eq!(names.len(), cases.len());
    let forms = cases
        .iter()
        .flat_map(|case| case.source.lines())
        .filter_map(|line| line.split_whitespace().next())
        .filter(|op| FORMS.contains(op))
        .collect::<BTreeSet<_>>();
    assert_eq!(forms, BTreeSet::from(FORMS));
    // The largest fixture explicitly seeds 160 source and 160 result bits.
    assert_eq!(
        cases.iter().map(|case| case.source.lines().count()).max(),
        Some(376)
    );
    cases
}
