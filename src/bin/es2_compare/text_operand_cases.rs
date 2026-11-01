//! Grouped text operands use fixed physical word strides, unlike scalar SEGD.
use super::{Case, case};
use std::collections::BTreeSet;

const FORMS: [&str; 10] = [
    "SEGD", "SEGDP", "ASCI", "ASCIP", "ASCII", "ASCIIP", "HEX", "HEXP", "CCD", "CCDP",
];

fn address(area: &str, index: u16) -> String {
    if matches!(area, "X" | "Y") {
        format!("{area}{index:o}")
    } else {
        format!("{area}{index}")
    }
}

fn seed_words(area: &str, base: u16, values: &[u16]) -> String {
    values
        .iter()
        .enumerate()
        .flat_map(|(i, value)| {
            (0..16).map(move |bit| {
                format!(
                    "{} {}",
                    if value & (1 << bit) != 0 {
                        "SET"
                    } else {
                        "RST"
                    },
                    address(area, base + i as u16 * 16 + bit)
                )
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn observe_words(area: &str, base: u16, count: u16) -> String {
    (0..count)
        .map(|i| format!("MOV K4{} D{}", address(area, base + i * 16), 20 + i))
        .collect::<Vec<_>>()
        .join("\n")
}

fn ascii_words(text: &[u8], low_only: bool) -> Vec<u16> {
    if low_only {
        text.iter()
            .enumerate()
            .map(|(i, b)| (0xa0 + i as u16) << 8 | u16::from(*b))
            .collect()
    } else {
        text.chunks(2)
            .map(|pair| u16::from(pair[0]) | u16::from(*pair.get(1).unwrap_or(&0xa5)) << 8)
            .collect()
    }
}

fn scenario(
    name: &str,
    low_only: bool,
    seed: &str,
    body: &str,
    guards: &[(&str, bool)],
    observe: &str,
) -> Case {
    let guard_seed = guards
        .iter()
        .map(|(d, on)| format!("{} {d}", if *on { "SET" } else { "RST" }))
        .collect::<Vec<_>>()
        .join("\n");
    let guard_reads = guards
        .iter()
        .enumerate()
        .map(|(i, (d, _))| format!("LD {d}\nOUT Y{:o}", i + 8))
        .collect::<Vec<_>>()
        .join("\n");
    let mode = if low_only { "SET M1161" } else { "RST M1161" };
    let source = [
        "LD X1", mode, seed, &guard_seed,
        "SET M1020\nRST M1021\nSET M1022\nSET M1029\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068",
        "LD X0", body,
        "LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1029\nOUT Y23\nLD M1067\nOUT Y24\nLD M1068\nOUT Y25",
        &guard_reads,
        "LD X0\nORI X0\nMOV D1067 D300\nMOV D1068 D301",
        observe, "END",
    ].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n");
    assert!(source.lines().count() <= 512, "oversized text case {name}");
    case(
        &format!("text_operand_{name}"),
        &source,
        vec![
            (vec![("X1", true), ("X0", false)], 1),
            (vec![("X1", false), ("X0", true)], 2),
            (vec![("X0", false)], 1),
            (vec![("X0", true)], 1),
        ],
    )
}

fn inputs(mut item: Case, base: u16, values: &[u16]) -> Case {
    for (i, value) in values.iter().enumerate() {
        for bit in 0..16 {
            let label: &'static str =
                Box::leak(address("X", base + i as u16 * 16 + bit).into_boxed_str());
            item.stages[0].0.push((label, value & (1 << bit) != 0));
        }
    }
    item
}

fn index_base(index: u16) -> u16 {
    if index == 0 { 1028 } else { 1180 + index * 2 }
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for op in ["SEGD", "SEGDP"] {
        for digits in 1..=4 {
            for (dest_area, source_area) in [("Y", "M"), ("M", "S"), ("S", "Y")] {
                let source = address(source_area, 32);
                let dest = address(dest_area, 128);
                let before = address(dest_area, 127);
                let after = address(dest_area, 144);
                cases.push(scenario(
                    &format!("segment_{op}_K{digits}_{dest_area}"),
                    false,
                    &format!(
                        "{}\n{}",
                        seed_words(source_area, 32, &[0xabc7]),
                        seed_words(dest_area, 128, &[0xffff])
                    ),
                    &format!("{op} K{digits}{source} K{digits}{dest}\nMOV HABC9 K4{source}"),
                    &[(before.as_str(), true), (after.as_str(), false)],
                    &observe_words(dest_area, 128, 1),
                ));
            }
        }
        for (index, area) in [(0, 'E'), (0, 'F'), (7, 'E'), (7, 'F')] {
            let base = index_base(index);
            let alias = base + u16::from(area == 'F');
            cases.push(scenario(
                &format!("segment_{op}_index_source_{area}{index}"), false,
                &format!("MOV H55AA D{base}\nMOV H1234 D{}\nMOV HABC7 D{alias}\n{}", base + 1, seed_words("S", 128, &[0xffff])),
                &format!("{op} {area}{index} K2S128\nMOV HABC9 D{alias}"),
                &[("S127", true), ("S144", false)],
                &format!("MOV K4S128 D20\nMOV E{index} D21\nMOV F{index} D22\nMOV D{base} D23\nMOV D{} D24", base + 1),
            ));
        }
        for (index, area) in [(0, 'E'), (0, 'F'), (1, 'E'), (7, 'F')] {
            let base = index_base(index);
            cases.push(scenario(
                &format!("segment_{op}_index_target_{area}{index}"),
                false,
                &format!(
                    "MOV H55AA D{base}\nMOV H1234 D{}\n{}",
                    base + 1,
                    seed_words("M", 32, &[0xabc7])
                ),
                &format!("{op} K1M32 {area}{index}\nMOV HABC9 K4M32"),
                &[("M31", true), ("M48", false)],
                &format!(
                    "MOV E{index} D20\nMOV F{index} D21\nMOV D{base} D22\nMOV D{} D23",
                    base + 1
                ),
            ));
        }
    }
    assert_eq!(cases.len(), 40);

    // All group widths and destination areas, with both M1161 settings. The
    // form alternation gives every width/area one normal and one pulse case.
    for family in ["ASCI", "HEX", "CCD"] {
        let areas: &[&str] = if family == "CCD" {
            &["M", "S"]
        } else {
            &["Y", "M", "S"]
        };
        for digits in 1..=4u16 {
            for (area_index, dest_area) in areas.iter().enumerate() {
                for low_only in [false, true] {
                    let source_area = ["M", "S", "Y"][(area_index + usize::from(digits)) % 3];
                    let source = address(source_area, 32);
                    let dest = address(dest_area, 128);
                    let op = format!(
                        "{family}{}",
                        if (usize::from(digits) + area_index + usize::from(low_only)) % 2 == 0 {
                            "P"
                        } else {
                            ""
                        }
                    );
                    let (source_values, dest_words, source_digits, update) = match family {
                        "ASCI" => (
                            vec![0x0123, 0x4567],
                            if low_only { 5 } else { 3 },
                            digits,
                            0x89ab,
                        ),
                        "HEX" => (
                            ascii_words(b"123AF", low_only),
                            2,
                            4,
                            if low_only { 0xa032 } else { 0x3232 },
                        ),
                        _ => (
                            ascii_words(&[100, 111, 120, 202, 123], low_only),
                            2,
                            digits,
                            0x0102,
                        ),
                    };
                    let source_before = address(source_area, 31);
                    let source_after = address(source_area, 32 + source_values.len() as u16 * 16);
                    let dest_before = address(dest_area, 127);
                    let dest_after = address(dest_area, 128 + dest_words * 16);
                    cases.push(scenario(
                        &format!("{family}_K{digits}_{dest_area}_low_{low_only}_{op}"), low_only,
                        &format!("{}\n{}", seed_words(source_area, 32, &source_values), seed_words(dest_area, 128, &vec![0xffff; dest_words as usize])),
                        &format!("{op} K{source_digits}{source} K{digits}{dest} K5\nMOV H{update:04X} K4{source}"),
                        &[(source_before.as_str(), true), (source_after.as_str(), false), (dest_before.as_str(), false), (dest_after.as_str(), true)],
                        &observe_words(dest_area, 128, dest_words),
                    ));
                }
            }
        }
    }
    assert_eq!(cases.len(), 104);

    for (family, source_digits, values, count, dest_words) in [
        ("SEGD", 1, vec![0xabc7], 1, 1),
        ("ASCI", 3, vec![0x0123, 0x4567], 5, 3),
        ("HEX", 4, ascii_words(b"123AF", false), 5, 2),
        (
            "CCD",
            2,
            ascii_words(&[100, 111, 120, 202, 123], false),
            5,
            2,
        ),
    ] {
        let body = if family == "SEGD" {
            format!("SEGD K{source_digits}X40 K2M128")
        } else {
            format!("{family} K{source_digits}X40 K2M128 K{count}")
        };
        let after = address("M", 128 + dest_words * 16);
        cases.push(inputs(
            scenario(
                &format!("{family}_group_X_source"),
                false,
                &seed_words("M", 128, &vec![0xffff; dest_words as usize]),
                &body,
                &[("M127", true), (after.as_str(), false)],
                &observe_words("M", 128, dest_words),
            ),
            32,
            &values,
        ));
    }
    assert_eq!(cases.len(), 108);

    for family in ["ASCI", "HEX", "CCD"] {
        for low_only in [false, true] {
            let source_values = match family {
                "ASCI" => vec![0x0123, 0x4567],
                "HEX" => ascii_words(b"123AF", low_only),
                _ => ascii_words(&[100, 111, 120, 202, 123], low_only),
            };
            let output_words = if family == "ASCI" {
                if low_only { 5 } else { 3 }
            } else {
                2
            };
            let total_words = output_words.max(source_values.len());
            let after = address("M", 128 + total_words as u16 * 16);
            let op = format!("{family}{}", if low_only { "P" } else { "" });
            cases.push(scenario(
                &format!("{family}_source_result_overlap_low_{low_only}"),
                low_only,
                &format!(
                    "{}\n{}",
                    seed_words("M", 128, &vec![0xffff; total_words]),
                    seed_words("M", 128, &source_values)
                ),
                &format!("{op} K4M128 K3M128 K5"),
                &[("M127", true), (after.as_str(), false)],
                &observe_words("M", 128, total_words as u16),
            ));
        }
    }
    assert_eq!(cases.len(), 114);

    for low_only in [false, true] {
        let source_values = ascii_words(&[0xc1, 0xc2, 0xc3], low_only);
        cases.push(scenario(
            &format!("HEX_group_strips_each_ascii_bit7_low_{low_only}"),
            low_only,
            &format!(
                "{}\n{}",
                seed_words("M", 32, &source_values),
                seed_words("S", 128, &[0xffff])
            ),
            "HEX K4M32 K4S128 K3",
            &[("S127", true), ("S144", false)],
            "MOV K4S128 D20",
        ));
        for digits in 1..=3 {
            cases.push(scenario(
                &format!("HEX_short_source_K{digits}_low_{low_only}"),
                low_only,
                &format!(
                    "{}\n{}",
                    seed_words("M", 32, &source_values),
                    seed_words("S", 128, &[0xffff])
                ),
                &format!("HEXP K{digits}M32 K4S128 K3"),
                &[("S127", true), ("S144", false)],
                "MOV K4S128 D20",
            ));
        }
    }
    assert_eq!(cases.len(), 122);

    for (digits, op) in [(1, "ASCII"), (2, "ASCIIP"), (3, "ASCI")] {
        cases.push(scenario(
            &format!("ASCI_short_odd_tail_preserves_outside_K{digits}_{op}"),
            false,
            &seed_words("M", 128, &[0xffff]),
            &format!("{op} H123 K{digits}M128 K1"),
            &[("M127", true), ("M144", false)],
            "MOV K4M128 D20",
        ));
    }
    for low_only in [false, true] {
        cases.push(scenario(
            &format!("HEX_group_partial_invalid_preserves_following_nibble_low_{low_only}"),
            low_only,
            &format!(
                "{}\n{}",
                seed_words("M", 32, &ascii_words(b"A!23", low_only)),
                seed_words("S", 128, &[0xffff])
            ),
            "HEX K4M32 K4S128 K4",
            &[("S127", true), ("S144", false)],
            "MOV K4S128 D20",
        ));
    }
    for count in [-1, 0, 257] {
        cases.push(scenario(
            &format!("CCD_dynamic_count_{count}_then_pulse_rearm"),
            false,
            &format!(
                "MOV K{count} D250\n{}\n{}",
                seed_words("M", 32, &[0x6f64, 0xca78, 0xd37b]),
                seed_words("S", 128, &[0xffff; 2])
            ),
            "CCDP K2M32 K1S128 D250\nMOV K5 D250",
            &[("S127", true), ("S160", false)],
            "MOV K4S128 D20\nMOV K4S144 D21\nMOV D250 D22",
        ));
    }
    assert_eq!(cases.len(), 130);
    for family in ["ASCI", "HEX", "CCD"] {
        for group_source in [true, false] {
            let low_only = !group_source;
            let values = match family {
                "ASCI" => vec![0x0123, 0x4567],
                "HEX" => ascii_words(b"123AF", low_only),
                _ => ascii_words(&[100, 111, 120, 202, 123], low_only),
            };
            let result_words = if family == "ASCI" {
                if low_only { 5 } else { 3 }
            } else {
                2
            };
            let source_digits = if family == "HEX" { 4 } else { 2 };
            let (seed, body, observe) = if group_source {
                let dest_seed = (99..=100 + result_words)
                    .map(|d| format!("MOV HFFFF D{d}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                (
                    format!("{}\n{dest_seed}", seed_words("M", 32, &values)),
                    format!("{family}P K{source_digits}M32 D100 K5"),
                    format!("BMOV D99 D20 K{}", result_words + 2),
                )
            } else {
                let source_seed = values
                    .iter()
                    .enumerate()
                    .map(|(i, value)| format!("MOV H{value:04X} D{i}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                (
                    format!(
                        "{source_seed}\n{}",
                        seed_words("S", 128, &vec![0xffff; result_words as usize])
                    ),
                    format!("{family} D0 K2S128 K5"),
                    observe_words("S", 128, result_words),
                )
            };
            let after = address("S", 128 + result_words * 16);
            cases.push(scenario(
                &format!("{family}_mixed_group_source_{group_source}"),
                low_only,
                &seed,
                &body,
                &[("S127", true), (after.as_str(), false)],
                &observe,
            ));
        }
    }
    assert_eq!(cases.len(), 136);
    let names = cases.iter().map(|case| &case.name).collect::<BTreeSet<_>>();
    assert_eq!(names.len(), cases.len());
    let forms = cases
        .iter()
        .flat_map(|case| case.source.lines())
        .filter_map(|line| line.split_whitespace().next())
        .filter(|op| FORMS.contains(op))
        .collect::<BTreeSet<_>>();
    assert_eq!(forms, BTreeSet::from(FORMS));
    cases
}
