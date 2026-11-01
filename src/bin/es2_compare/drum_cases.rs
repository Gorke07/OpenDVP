//! Bounded drum/cam differential sources. PDF counts are always literal1..64.
//! Unsafe implicit spans and legacy low-byte255 loops are never executed.
use super::{Case, case};
use opendvp::es2_bridge::Target;

fn words(base: &str, values: &[i16]) -> String {
    if !base.starts_with('D') {
        return words("D600", values) + &format!("BMOV D600 {base} K{}\n", values.len());
    }
    let index = base[1..].parse::<usize>().unwrap();
    values
        .iter()
        .enumerate()
        .map(|(i, value)| format!("MOV K{value} D{}\n", index + i))
        .collect()
}
fn raw_pairs(values: &[i32]) -> Vec<i16> {
    values
        .iter()
        .flat_map(|value| [*value as i16, (*value >> 16) as i16])
        .collect()
}
fn group(area: char, base: usize, values: &[i16]) -> String {
    let mut source = String::new();
    for (word, value) in values.iter().enumerate() {
        for bit in 0..16 {
            let index = base + word * 16 + bit;
            let address = if area == 'Y' {
                format!("Y{index:o}")
            } else {
                format!("{area}{index}")
            };
            source.push_str(&format!(
                "{} {address}\n",
                if (*value as u16) & (1 << bit) != 0 {
                    "SET"
                } else {
                    "RST"
                }
            ));
        }
    }
    source
}
fn staged(name: &str, init: &str, update: &str, body: &str, active_scans: usize) -> Case {
    let update_rung = if update.is_empty() {
        String::new()
    } else {
        format!("LD X2\n{update}")
    };
    case(
        &format!("drum_{name}"),
        &format!(
            "LD X1\n{init}SET M1020\nSET M1021\nSET M1022\nSET M1029\n{update_rung}LD X0\n{body}\nMOV K123 D900\nEND"
        ),
        vec![
            (vec![("X1", true), ("X0", false), ("X2", false)], 1),
            (vec![("X1", false), ("X0", true)], 1),
            (vec![("X2", true)], 1),
            (vec![("X2", false)], active_scans),
            (vec![("X0", false)], 1),
            (vec![("X0", true)], 2),
        ],
    )
}
fn input_table(name: &str, op: &str, packed: &str, values: &[i16], init: &str, body: &str) -> Case {
    let init_rung = if init.is_empty() {
        String::new()
    } else {
        format!("LD X1\n{init}")
    };
    let mut inputs = vec![("X1", true), ("X0", false)];
    for (word, value) in values.iter().enumerate() {
        for bit in 0..16 {
            // Case stores static input labels; these small generated test labels
            // intentionally live for this comparator process's fixture lifetime.
            let label: &'static str =
                Box::leak(format!("X{:o}", 64 + word * 16 + bit).into_boxed_str());
            inputs.push((label, (*value as u16) & (1 << bit) != 0));
        }
    }
    case(
        &format!("drum_{name}"),
        &format!("{init_rung}LD X0\n{op} {packed} {body}\nMOV K123 D900\nEND"),
        vec![
            (inputs, 1),
            (vec![("X1", false), ("X0", true)], 3),
            (vec![("X0", false)], 1),
            (vec![("X0", true)], 2),
        ],
    )
}

pub fn cases(target: Target) -> Vec<Case> {
    let mut out = Vec::new();
    let last = if target == Target::Ss2 { 4999 } else { 9999 };
    for (op, wide) in [("ABSD", false), ("DABSD", true)] {
        for (index, (low, high)) in [(0, 10), (7, 7), (10, 0), (-10, -2), (-2, -10)]
            .into_iter()
            .enumerate()
        {
            for (probe, value) in [low - 1, low, (low + high) / 2, high, high + 1]
                .into_iter()
                .enumerate()
            {
                let values = if wide {
                    raw_pairs(&[low, high])
                } else {
                    vec![low as i16, high as i16]
                };
                let init = words("D100", &values)
                    + &words(
                        "D300",
                        &if wide {
                            raw_pairs(&[value])
                        } else {
                            vec![value as i16]
                        },
                    );
                out.push(staged(
                    &format!("{op}_interval_{index}_{probe}_{value}"),
                    &init,
                    "",
                    &format!("{op} D100 D300 M200 K1"),
                    2,
                ));
            }
        }
        for value in if wide {
            vec![i32::MIN, -1, 0, i32::MAX]
        } else {
            vec![i16::MIN as i32, -1, 0, i16::MAX as i32]
        } {
            let range = if wide {
                raw_pairs(&[i32::MIN, i32::MAX])
            } else {
                vec![i16::MIN, i16::MAX]
            };
            let init = words("D100", &range)
                + &words(
                    "D300",
                    &if wide {
                        raw_pairs(&[value])
                    } else {
                        vec![value as i16]
                    },
                );
            out.push(staged(
                &format!("{op}_signed_extreme_{value}"),
                &init,
                "",
                &format!("{op} D100 D300 S20 H1"),
                2,
            ));
        }
        for table in ["T10", "C10", "D100"] {
            for current in ["T100", "C100", "D300"] {
                let table_values = if wide {
                    raw_pairs(&[-10, 10, 10, -10])
                } else {
                    vec![-10, 10, 10, -10]
                };
                let init = words(table, &table_values)
                    + &words(current, &if wide { raw_pairs(&[-5]) } else { vec![-5] });
                out.push(staged(
                    &format!("{op}_areas_{table}_{current}"),
                    &init,
                    "",
                    &format!("{op} {table} {current} Y10 K2"),
                    2,
                ));
            }
        }
        for area in ['M', 'Y', 'S'] {
            let values = if wide {
                raw_pairs(&[0, 10, 6, 6])
            } else {
                vec![0, 10, 6, 6]
            };
            let init = group(area, 64, &values)
                + &words("D300", &if wide { raw_pairs(&[6]) } else { vec![6] });
            let address = if area == 'Y' {
                "Y100".to_owned()
            } else {
                format!("{area}64")
            };
            let packed = format!("K{}{}", if wide { 8 } else { 4 }, address);
            out.push(staged(
                &format!("{op}_group_{area}"),
                &init,
                "",
                &format!("{op} {packed} D300 M200 K2"),
                2,
            ));
        }
        let x_values = if wide {
            raw_pairs(&[0, 10])
        } else {
            vec![0, 10]
        };
        out.push(input_table(
            &format!("{op}_group_X"),
            op,
            if wide { "K8X100" } else { "K4X100" },
            &x_values,
            &words("D300", &if wide { raw_pairs(&[6]) } else { vec![6] }),
            "D300 M200 K1",
        ));

        let bounds = if wide {
            raw_pairs(&[0, 10, 6, 6])
        } else {
            vec![0, 10, 6, 6]
        };
        let alias = if wide { 128 } else { 96 };
        let init =
            group('M', 64, &bounds) + &words("D300", &if wide { raw_pairs(&[6]) } else { vec![6] });
        out.push(staged(
            &format!("{op}_live_output_table_alias"),
            &init,
            "",
            &format!("{op} K{}M64 D300 M{alias} K2", if wide { 8 } else { 4 }),
            2,
        ));
        for overlap in ["D100", "D101"] {
            // Current value is sampled once even when its words are in the table.
            let init = words("D100", &bounds);
            out.push(staged(
                &format!("{op}_current_table_alias_{overlap}"),
                &init,
                "",
                &format!("{op} D100 {overlap} M200 K2"),
                2,
            ));
        }
        let table = if wide {
            raw_pairs(&[0, 10])
        } else {
            vec![0, 10]
        };
        let init = words("D100", &table)
            + &words("D300", &if wide { raw_pairs(&[5]) } else { vec![5] })
            + "SET M200\nSET M201\n";
        let update = words("D300", &if wide { raw_pairs(&[20]) } else { vec![20] });
        out.push(staged(
            &format!("{op}_live_current_and_false_hold"),
            &init,
            &update,
            &format!("{op} D100 D300 M200 K1"),
            2,
        ));

        let mut count64 = Vec::new();
        for i in 0..64 {
            let pair = [i - 32, i - 16];
            count64.extend(if wide {
                raw_pairs(&pair)
            } else {
                pair.map(|v| v as i16).to_vec()
            });
        }
        let base = last - count64.len() as i32 + 1;
        let init = words(&format!("D{base}"), &count64)
            + &words("D300", &if wide { raw_pairs(&[0]) } else { vec![0] });
        out.push(staged(
            &format!("{op}_count64_model_D_end_M_end"),
            &init,
            "",
            &format!("{op} D{base} D300 M4032 H40"),
            2,
        ));
    }
    for value in [-2147483648i32, -1, 0, 2147483647] {
        let init =
            words("D100", &raw_pairs(&[i32::MIN, i32::MAX])) + &format!("DMOV K{value} C200\n");
        out.push(staged(
            &format!("DABSD_signed_C200_{value}"),
            &init,
            "",
            "DABSD D100 C200 M200 K1",
            2,
        ));
    }
    out.push(staged(
        "ABSD_C199_signed_word",
        &(words("D100", &[-1, -1]) + &words("C199", &[-1])),
        "",
        "ABSD D100 C199 M200 K1",
        2,
    ));
    out.push(staged(
        "DABSD_C199_zero_high_independent_C200",
        &(words("D100", &raw_pairs(&[65530, 65540]))
            + &words("C199", &[-1])
            + "DMOV H80000000 C200\n"),
        "",
        "DABSD D100 C199 M200 K1",
        2,
    ));
    for value in [65534, 65535, 70000, 70001] {
        let init = words("C199", &[-1]) + &format!("DMOV K70000 C200\nDMOV K{value} D300\n");
        out.push(staged(
            &format!("DABSD_C199_table_natural_transition_{value}"),
            &init,
            "",
            "DABSD C199 D300 M200 K1",
            2,
        ));
    }

    for (index, (count, section, thresholds)) in [
        (0, 0, vec![5, 8, 2]),
        (4, 0, vec![5, 8, 2]),
        (5, 0, vec![5, 8, 2]),
        (99, 0, vec![5, 8, 2]),
        (-1, 0, vec![5, 8, 2]),
        (0, 0, vec![0, 0, 0]),
        (0, 0, vec![-1, -2, -3]),
        (8, 1, vec![5, 8, 2]),
        (2, 2, vec![5, 8, 2]),
        (8, 0xab01u16 as i16, vec![5, 8, 2]),
        (0, 255, vec![5, 8, 2]),
        (10, 3, vec![5, 8, 2]),
    ]
    .into_iter()
    .enumerate()
    {
        let init = words("D100", &thresholds) + &words("C10", &[77, 88]);
        let update = words("C10", &[count, section]);
        out.push(staged(
            &format!("INCD_continuation_{index}"),
            &init,
            &update,
            "INCD D100 C10 M200 K3",
            4,
        ));
    }
    for table in ["T10", "C100", "D100"] {
        out.push(staged(
            &format!("INCD_area_{table}"),
            &words(table, &[2, 1, 3]),
            &words("C10", &[9, 0]),
            &format!("INCD {table} C10 Y10 K3"),
            3,
        ));
    }
    for area in ['M', 'Y', 'S'] {
        let init = group(area, 64, &[1, 0, 2]);
        let address = if area == 'Y' {
            "Y100".to_owned()
        } else {
            format!("{area}64")
        };
        out.push(staged(
            &format!("INCD_group_{area}"),
            &init,
            &words("C10", &[1, 0]),
            &format!("INCD K4{address} C10 S20 K3"),
            4,
        ));
    }
    out.push(input_table(
        "INCD_group_X",
        "INCD",
        "K4X100",
        &[0, 0],
        "",
        "C10 M200 K2",
    ));
    out.push(staged(
        "INCD_live_threshold_update",
        &words("D100", &[10, 20]),
        &(words("D100", &[-1, 0]) + &words("C10", &[0, 0])),
        "INCD D100 C10 M200 K2",
        4,
    ));
    out.push(staged(
        "INCD_table_counter_alias",
        &words("C10", &[1, 2]),
        &words("C10", &[2, 0]),
        "INCD C10 C10 M200 K2",
        4,
    ));
    out.push(staged(
        "INCD_live_output_table_alias",
        &group('M', 64, &[1, 2]),
        &words("C10", &[1, 0]),
        "INCD K4M64 C10 M80 K2",
        4,
    ));
    out.push(staged(
        "INCD_two_owners_shared_C_output",
        &words("D100", &[0, 0]),
        "",
        "INCD D100 C10 M200 K2\nINCD D100 C10 M200 K2",
        5,
    ));
    out.push(staged(
        "INCD_two_owners_shared_completion",
        &(words("D100", &[0, 0]) + &words("D110", &[1, 1])),
        &words("C20", &[1, 0]),
        "INCD D100 C10 M200 K2\nINCD D110 C20 M210 K2",
        5,
    ));
    for count in [1usize, 64] {
        let base = last - count as i32 + 1;
        let init = words(&format!("D{base}"), &vec![1; count]) + "MOV K0 D500\n";
        out.push(staged(
            &format!("INCD_count{count}_model_D_end_C198_M_end"),
            &init,
            "",
            &format!(
                "INC D500\nBMOV D500 C198 K1\nINCD D{base} C198 M{} K{count}",
                4096 - count
            ),
            count + 3,
        ));
    }
    out.extend(scratch_cases());
    out
}

// Appended sources deliberately observe shared numeric scratch indirectly via
// the real word SORT continuation. The original130 fixtures above are unchanged.
fn scratch_source(init: &str, predecessor: &str, padding: bool) -> String {
    let mut source = String::from("LD X1\n");
    source.push_str(&words("D0", &[30, 10, 20, 3, 1, 2]));
    source.push_str(&words("D100", &[-777; 6]));
    source.push_str(init);
    source.push_str("SET M1020\nSET M1021\nSET M1022\nRST M1029\n");
    if padding {
        // Inactive45MOVs contribute225 real program words to function74's
        // readback cursor, independently giving the retained seed a high byte.
        source.push_str("LD X7\n");
        for _ in 0..45 {
            source.push_str("MOV K0 D800\n");
        }
    }
    source.push_str(predecessor);
    source.push_str("LD X0\nSORT D0 K3 K2 D100 K2\nOUT Y0\nLD X6\nMOV K1234 D300\nLD M1020\nOUT M120\nLD M1021\nOUT M121\nLD M1022\nOUT M122\nLD M1029\nOUT M123\nEND");
    source
}
fn scratch_stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (
            vec![
                ("X1", true),
                ("X0", false),
                ("X2", false),
                ("X6", true),
                ("X7", false),
            ],
            1,
        ),
        (vec![("X1", false), ("X0", true)], 6),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 4),
    ]
}
fn scratch_cases() -> Vec<Case> {
    let mut out = Vec::new();
    for value in [0, 1, 3, -1] {
        let init = words("D200", &[-10, 10]) + &words("D220", &[value]);
        let source = scratch_source(&init, "LD X0\nABSD D200 D220 M200 K1\n", false);
        out.push(case(
            &format!("drum_scratch_ABSD_value_{value}"),
            &source,
            scratch_stages(),
        ));
    }
    for table in ["T20", "C20", "D200"] {
        let init = words(table, &raw_pairs(&[-10, 10]))
            + &words("D220", &raw_pairs(&[0]))
            + &words("D700", &[7]);
        // SER writesFF00. A raw DABSD table changes only its low tag byte;
        // preserving the negative high byte changes SORT's continuation gate.
        let body = format!("LD X0\nSER D700 K-256 D720 K1\nDABSD {table} D220 M200 K1\n");
        out.push(case(
            &format!("drum_scratch_DABSD_raw_{table}_negative_high"),
            &scratch_source(&init, &body, false),
            scratch_stages(),
        ));
    }
    for prior in [Some(0), Some(3), None] {
        let init = group('M', 256, &raw_pairs(&[-10, 10]))
            + &words("D220", &raw_pairs(&[0]))
            + &words("D700", &[7]);
        let mut body = String::from("LD X0\n");
        if let Some(value) = prior {
            body.push_str(&format!("SER D700 K{value} D720 K1\n"));
        }
        body.push_str("DABSD K8M256 D220 M200 K1\n");
        let label = prior.map_or_else(|| "padded_readback_high".to_owned(), |n| format!("SER{n}"));
        out.push(case(
            &format!("drum_scratch_DABSD_group_holds_{label}"),
            &scratch_source(&init, &body, prior.is_none()),
            scratch_stages(),
        ));
    }
    {
        let init = words("D200", &[5, 8]) + &words("D700", &[7]);
        let body = "LD X0\nSER D700 K3 D720 K1\nLD X2\nINCD D200 C10 M200 K2\n";
        out.push(case(
            "drum_scratch_INCD_first_and_false_hold",
            &scratch_source(&init, body, false),
            vec![
                (
                    vec![("X1", true), ("X0", false), ("X2", false), ("X6", true)],
                    1,
                ),
                (vec![("X1", false), ("X0", true)], 2),
                (vec![("X2", true)], 1),
                (vec![("X2", false)], 2),
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 4),
            ],
        ));
    }
    for (value, section) in [(0, 0), (3, 0), (-1, 255)] {
        let init = words("D200", &[1, 2]) + &words("D700", &[7]);
        let update = words("C10", &[value, section]);
        let body = format!("LD X2\n{update}LD X0\nSER D700 K3 D720 K1\nINCD D200 C10 M200 K2\n");
        out.push(case(
            &format!("drum_scratch_INCD_continued_{value}_section_{section}"),
            &scratch_source(&init, &body, false),
            vec![
                (
                    vec![("X1", true), ("X0", false), ("X2", false), ("X6", true)],
                    1,
                ),
                (vec![("X1", false), ("X0", true)], 1),
                (vec![("X2", true)], 5),
                (vec![("X2", false), ("X0", false)], 1),
                (vec![("X0", true)], 3),
            ],
        ));
    }
    out
}
