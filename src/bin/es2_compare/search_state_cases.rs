//! Stateful instruction probes; expected values come from the two actual engines.
use super::{Case, case};
use opendvp::es2_bridge::Target;

fn stages(active: usize) -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (
            vec![("X0", false), ("X1", true), ("X2", false), ("X3", true)],
            1,
        ),
        (vec![("X0", true), ("X1", false)], active),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 2),
    ]
}
fn changing_stages(
    active_before: usize,
    active_after: usize,
) -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (
            vec![("X0", false), ("X1", true), ("X2", false), ("X3", true)],
            1,
        ),
        (vec![("X0", true), ("X1", false)], active_before),
        (vec![("X2", true)], active_after),
        (vec![("X0", false), ("X2", false)], 1),
        (vec![("X0", true)], 2),
    ]
}
fn seed_flags(completed: bool) -> String {
    format!(
        "SET M1020\nSET M1021\nSET M1022\n{} M1029\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\n",
        if completed { "SET" } else { "RST" }
    )
}
fn finish(source: &mut String) {
    source.push_str("OUT Y0\nLD X3\nMOV K1234 D300\nLD M1020\nOUT M120\nLD M1021\nOUT M121\nLD M1022\nOUT M122\nLD M1029\nOUT M123\nLD M1026\nOUT M124\nLD M1067\nOUT M125\nLD M1068\nOUT M126\nEND");
}
fn store(wide: bool, value: i32, address: u16) -> String {
    format!(
        "{} K{value} D{address}\n",
        if wide { "DMOV" } else { "MOV" }
    )
}
fn name(wide: bool, base: &str) -> String {
    format!("{}{base}", if wide { "D" } else { "" })
}
fn ramp_source(wide: bool, first: i32, last: i32, n: i32, held: bool) -> String {
    let mut source = String::from("LD X1\n");
    source.push_str(&store(wide, first, 0));
    source.push_str(&store(wide, last, if wide { 2 } else { 1 }));
    source.push_str(&store(wide, n, 4));
    source.push_str(&store(wide, 777, 10));
    source.push_str(&store(wide, 55, if wide { 12 } else { 11 }));
    source.push_str(&seed_flags(true));
    source.push_str(if held { "SET M1026\n" } else { "RST M1026\n" });
    source
}
fn ramp_line(wide: bool, count: &str) -> String {
    format!(
        "{} D0 D{} D10 {count}\n",
        name(wide, "RAMP"),
        if wide { 2 } else { 1 }
    )
}
fn seed_values(wide: bool, values: &[i32], base: u16, source: &mut String) {
    for (index, value) in values.iter().enumerate() {
        source.push_str(&store(
            wide,
            *value,
            base + index as u16 * if wide { 2 } else { 1 },
        ));
    }
}
fn sort_source(wide: bool, values: &[i32], destination_elements: usize) -> String {
    let mut source = String::from("LD X1\n");
    seed_values(wide, values, 0, &mut source);
    for index in 0..destination_elements {
        source.push_str(&store(
            wide,
            -777,
            100 + index as u16 * if wide { 2 } else { 1 },
        ));
    }
    source.push_str(&seed_flags(false));
    source
}

pub(super) fn cases(target: Target) -> Vec<Case> {
    let mut result = Vec::new();
    for wide in [false, true] {
        let ramp = name(wide, "RAMP");
        let sort = name(wide, "SORT");
        let endpoints: &[(i32, i32)] = if wide {
            &[
                (10, 100),
                (-2_147_483_600, 2_147_483_600),
                (16_777_217, 16_777_299),
            ]
        } else {
            &[(10, 100), (-100, 100), (100, -100)]
        };
        for held in [false, true] {
            for (pair, (first, last)) in endpoints.iter().enumerate() {
                for n in [1, 3] {
                    let mut source = ramp_source(wide, *first, *last, n, held);
                    source.push_str("LD X0\n");
                    source.push_str(&ramp_line(wide, &format!("K{n}")));
                    finish(&mut source);
                    result.push(case(
                        &format!("search_state_ramp_{ramp}_mode_{held}_pair_{pair}_n_{n}"),
                        &source,
                        stages(n as usize + 3),
                    ));
                }
            }
        }
        for n in [0, -1] {
            let mut source = ramp_source(wide, 10, 90, n, false);
            source.push_str("LD X0\n");
            source.push_str(&ramp_line(wide, "D4"));
            finish(&mut source);
            result.push(case(
                &format!("search_state_ramp_{ramp}_dynamic_invalid_{n}"),
                &source,
                stages(5),
            ));
        }
        for variation in ["start", "end", "count"] {
            let mut source = ramp_source(wide, 10, 100, 4, false);
            source.push_str("LD X2\n");
            source.push_str(&match variation {
                "start" => store(wide, -30, 0),
                "end" => store(wide, -100, if wide { 2 } else { 1 }),
                _ => store(wide, 2, 4),
            });
            source.push_str("LD X0\n");
            source.push_str(&ramp_line(wide, "D4"));
            finish(&mut source);
            result.push(case(
                &format!("search_state_ramp_{ramp}_live_{variation}"),
                &source,
                changing_stages(2, 5),
            ));
        }
        for on in [false, true] {
            let mut source = ramp_source(wide, 17, 117, 1, !on);
            source.push_str(&format!(
                "LD X2\n{} M1026\nLD X0\n",
                if on { "SET" } else { "RST" }
            ));
            source.push_str(&ramp_line(wide, "K1"));
            finish(&mut source);
            result.push(case(
                &format!("search_state_ramp_{ramp}_mode_flip_{on}"),
                &source,
                changing_stages(3, 5),
            ));
        }
        for alias in ["start", "end", "count"] {
            let mut source = ramp_source(wide, 10, 100, 3, false);
            let counter = if wide { 12 } else { 11 };
            if alias == "start" {
                source.push_str(&store(wide, 10, 10));
            }
            if alias == "end" {
                source.push_str(&store(wide, 100, counter));
            }
            if alias == "count" {
                source.push_str(&store(wide, 3, counter));
            }
            let first = if alias == "start" { 10 } else { 0 };
            let last = if alias == "end" {
                counter
            } else if wide {
                2
            } else {
                1
            };
            let count = if alias == "count" {
                format!("D{counter}")
            } else {
                "K3".into()
            };
            source.push_str(&format!("LD X0\n{ramp} D{first} D{last} D10 {count}\n"));
            finish(&mut source);
            result.push(case(
                &format!("search_state_ramp_{ramp}_alias_{alias}"),
                &source,
                stages(7),
            ));
        }
        {
            let mut source = ramp_source(wide, 10, 110, 3, false);
            source.push_str("LD X0\n");
            source.push_str(&ramp_line(wide, "K3"));
            source.push_str(&format!(
                "OUT Y1\nLD X2\n{ramp} D0 D{} D20 K1\n",
                if wide { 2 } else { 1 }
            ));
            finish(&mut source);
            result.push(case(
                &format!("search_state_ramp_{ramp}_occurrence_global_completion"),
                &source,
                vec![
                    (
                        vec![("X0", false), ("X1", true), ("X2", false), ("X3", true)],
                        1,
                    ),
                    (vec![("X0", true), ("X1", false)], 2),
                    (vec![("X2", true)], 3),
                    (vec![("X2", false)], 2),
                    (vec![("X0", false)], 1),
                    (vec![("X0", true), ("X2", true)], 2),
                ],
            ));
        }

        let data: Vec<Vec<i32>> = vec![
            vec![],
            vec![7],
            vec![5, -2],
            vec![3, 3, -1, 0, 3],
            if wide {
                vec![i32::MAX, i32::MIN, 0, -1, 16_777_217]
            } else {
                vec![32767, -32768, 0, -1, 123]
            },
            (0..32).map(|i| 31 - i).collect(),
        ];
        for (index, values) in data.iter().enumerate() {
            let mut source = sort_source(wide, values, values.len().max(1));
            source.push_str(&format!("LD X0\n{sort} D0 K1 K1 D100 K{}\n", values.len()));
            finish(&mut source);
            result.push(case(
                &format!("search_state_sort_{sort}_one_dimensional_{index}"),
                &source,
                stages(3),
            ));
        }
        for (rows, columns) in [(2, 2), (3, 2), (4, 3)] {
            for key in [1, columns] {
                let mut values = Vec::new();
                for column in 0..columns {
                    for row in 0..rows {
                        values.push(if column + 1 == key {
                            [3, -1, 3, 0][row as usize]
                        } else {
                            column * 100 + row * 10
                        });
                    }
                }
                let mut source = sort_source(wide, &values, values.len());
                source.push_str(&format!(
                    "LD X0\n{sort} D0 K{rows} K{columns} D100 K{key}\n"
                ));
                finish(&mut source);
                result.push(case(
                    &format!("search_state_sort_{sort}_table_{rows}_{columns}_key_{key}"),
                    &source,
                    stages(rows as usize + 2),
                ));
            }
        }
        for shift in [0, 1, 2] {
            let mut source = sort_source(wide, &[4, 1, 3, 2], 0);
            source.push_str(&format!("LD X0\n{sort} D0 K1 K1 D{shift} K4\n"));
            finish(&mut source);
            result.push(case(
                &format!("search_state_sort_{sort}_overlap_shift_{shift}"),
                &source,
                stages(3),
            ));
        }
        for n in [0, 33, -1] {
            let mut source = sort_source(wide, &[5, 1, 3], 3);
            source.push_str(&store(wide, n, 40));
            source.push_str(&format!("LD X0\n{sort} D0 K1 K1 D100 D40\n"));
            finish(&mut source);
            result.push(case(
                &format!("search_state_sort_{sort}_dynamic_length_{n}"),
                &source,
                stages(3),
            ));
        }
        {
            let mut source = sort_source(wide, &[4, 2, 3, 1], 4);
            source.push_str(&store(wide, 2, 40));
            source.push_str("LD X2\n");
            source.push_str(&store(wide, 4, 40));
            source.push_str(&format!("LD X0\n{sort} D0 K1 K1 D100 D40\n"));
            finish(&mut source);
            result.push(case(
                &format!("search_state_sort_{sort}_captured_dynamic_length"),
                &source,
                changing_stages(2, 3),
            ));
        }
        for n in [0, 3] {
            let mut source = sort_source(wide, &[30, 20, 10, 3, 2, 1], 6);
            source.push_str(&store(wide, n, 40));
            source.push_str(&format!("LD X0\n{sort} D0 K3 K2 D100 D40\n"));
            finish(&mut source);
            result.push(case(
                &format!("search_state_sort_{sort}_invalid_key_{n}"),
                &source,
                stages(3),
            ));
        }
        {
            let limit = if target == Target::Ss2 { 4999 } else { 9999 };
            let start = limit - if wide { 1 } else { 0 };
            let mut source = String::from("LD X1\n");
            source.push_str(&store(wide, -7, start));
            source.push_str(&store(wide, 1, 40));
            source.push_str(&seed_flags(false));
            source.push_str(&format!(
                "LD X0\n{sort} D{start} K1 K1 D{} D40\n",
                limit - 4
            ));
            finish(&mut source);
            result.push(case(
                &format!("search_state_sort_{sort}_model_endpoint_single"),
                &source,
                stages(3),
            ));
        }
        {
            let mut source = sort_source(wide, &[30, 10, 20, 3, 1, 2], 12);
            source.push_str(&format!(
                "LD X0\n{sort} D0 K3 K2 D100 K2\nOUT Y1\nLD X2\n{sort} D0 K1 K1 D140 K3\n"
            ));
            finish(&mut source);
            result.push(case(
                &format!("search_state_sort_{sort}_shared_owner"),
                &source,
                vec![
                    (
                        vec![("X0", false), ("X1", true), ("X2", false), ("X3", true)],
                        1,
                    ),
                    (vec![("X0", true), ("X1", false), ("X2", true)], 5),
                    (vec![("X0", false)], 1),
                    (vec![("X2", true)], 2),
                    (vec![("X0", true)], 2),
                ],
            ));
        }
    }
    // The legacy word SORT body compares progress with shared scratch53A42E.
    // These preceding families deliberately write different scratch values,
    // exposing an implementation bug rather than hiding it behind a clean case.
    for predecessor in ["SER0", "SER1", "SER3", "DSER", "RAMP", "DRAMP", "DTM0"] {
        let mut source = sort_source(false, &[30, 10, 20, 3, 1, 2], 6);
        source.push_str("MOV K2 D200\nMOV K9 D201\nMOV K65 D202\n");
        if matches!(predecessor, "DSER" | "DRAMP") {
            source.push_str("DMOV K2 D200\nDMOV K9 D202\n");
        }
        source.push_str("LD X0\n");
        source.push_str(match predecessor {
            "SER0" => "SER D200 K0 D220 K1\n",
            "SER1" => "SER D200 K1 D220 K1\n",
            "SER3" => "SER D200 K3 D220 K1\n",
            "DSER" => "DSER D200 K0 D220 K1\n",
            "RAMP" => "RAMP D200 D201 D220 K2\n",
            "DRAMP" => "DRAMP D200 D202 D220 K2\n",
            _ => "DTM D202 D220 K0 K1\n",
        });
        source.push_str("OUT Y1\nLD X0\nSORT D0 K3 K2 D100 K2\n");
        finish(&mut source);
        result.push(case(
            &format!("search_state_sort_SORT_scratch_predecessor_{predecessor}"),
            &source,
            stages(6),
        ));
    }
    for wide in [false, true] {
        // Isolated maximum-row table exercises all32 row steps, two held
        // completion calls and a rearm. Its program-readback scratch seed is
        // independently larger than32, without a preceding SER/RAMP/DTM.
        let mut source = String::from("LD X1\n");
        let mut values: Vec<i32> = (1..=32).rev().collect();
        values.extend(0..32);
        seed_values(wide, &values, 0, &mut source);
        for index in 0..64 {
            source.push_str(&store(wide, -777, 2000 + index * if wide { 2 } else { 1 }));
        }
        source.push_str(&seed_flags(false));
        let sort = name(wide, "SORT");
        source.push_str(&format!("LD X0\n{sort} D0 K32 K2 D2000 K1\n"));
        finish(&mut source);
        result.push(case(
            &format!("search_state_sort_{sort}_isolated_32_rows"),
            &source,
            stages(34),
        ));
    }
    for wide in [false, true] {
        let mut source = sort_source(wide, &[30, 10, 20, 3, 1, 2], 6);
        source.push_str(&format!(
            "LD X2\nRST M1029\nLD X0\n{} D0 K3 K2 D100 K2\n",
            name(wide, "SORT")
        ));
        finish(&mut source);
        result.push(case(
            &format!(
                "search_state_sort_{}_completion_relay_reset_while_owned",
                name(wide, "SORT")
            ),
            &source,
            vec![
                (
                    vec![("X0", false), ("X1", true), ("X2", false), ("X3", true)],
                    1,
                ),
                (vec![("X0", true), ("X1", false)], 4),
                (vec![("X2", true)], 2),
                (vec![("X2", false)], 1),
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 2),
            ],
        ));
    }
    {
        let mut source = sort_source(false, &[30, 10, 20, 3, 1, 2], 6);
        // The disabled MOVs still occupy225 codewords. Together with this
        // fixture they make the real0x74-readback scratch seed exceed255.
        source.push_str("LD X3\n");
        for _ in 0..45 {
            source.push_str("MOV K0 D800\n");
        }
        source.push_str("LD X2\nMOV K30 D100\nMOV K10 D101\nMOV K20 D102\nMOV K3 D103\nMOV K1 D104\nMOV K2 D105\nRST M1029\nLD X0\nSORT D0 K3 K2 D100 K2\n");
        finish(&mut source);
        // X3 remainsOFF to keep padding inactive; X6 drives the post marker.
        source = source.replace("LD X3\nMOV K1234 D300", "LD X6\nMOV K1234 D300");
        result.push(case(
            "search_state_sort_SORT_byte_progress_wrap",
            &source,
            vec![
                (
                    vec![
                        ("X0", false),
                        ("X1", true),
                        ("X2", false),
                        ("X3", false),
                        ("X6", true),
                    ],
                    1,
                ),
                (vec![("X0", true), ("X1", false)], 255),
                (vec![("X2", true)], 1),
                (vec![("X2", false)], 5),
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 2),
            ],
        ));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_first_probe_sources_and_scan_counts_are_unchanged() {
        for (model, target) in [("es2", Target::Es2), ("ss2", Target::Ss2)] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("analysis/{model}-search-first-probe.json"));
            // The retained probe output is large local evidence and is not part of the repository.
            let Ok(text) = std::fs::read_to_string(&path) else {
                eprintln!("skipping {model}: retained probe file not present");
                continue;
            };
            let previous: serde_json::Value = serde_json::from_str(&text).unwrap();
            let mut generated = cases(target);
            generated.extend(super::super::search_value_cases::cases(target));
            let current: std::collections::BTreeMap<_, _> =
                generated.iter().map(|c| (c.name.as_str(), c)).collect();
            let retained = previous["cases"].as_array().unwrap();
            let wrap = current
                .get("search_state_sort_SORT_byte_progress_wrap")
                .unwrap();
            let program = opendvp::plc::compile(&wrap.source).unwrap();
            let encoded = opendvp::es2_encoder::encode(&program).unwrap();
            assert!(encoded.words.len() >= 256);
            assert!(encoded.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS);
            assert_eq!(wrap.stages.iter().map(|(_, n)| *n).sum::<usize>(), 265);
            assert_eq!(retained.len(), 333);
            for original in retained {
                let name = original["name"].as_str().unwrap();
                let current = current
                    .get(name)
                    .unwrap_or_else(|| panic!("removed retained scenario{name}"));
                assert_eq!(
                    current.source,
                    original["source"].as_str().unwrap(),
                    "{model}/{name}"
                );
                assert_eq!(
                    current.stages.iter().map(|(_, n)| *n).sum::<usize>(),
                    original["scans"].as_u64().unwrap() as usize,
                    "{model}/{name}"
                );
            }
        }
    }
}
