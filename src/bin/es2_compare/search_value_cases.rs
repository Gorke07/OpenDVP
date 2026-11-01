//! Search result stacks and toggle coils: bounded native differential cases.
use super::{Case, case};
use opendvp::es2_bridge::Target;

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (vec![("X1", true), ("X0", false)], 1),
        (vec![("X1", false), ("X0", true)], 2),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}

fn fixture(seed: &str, body: &str) -> String {
    format!(
        "LD X1\nFMOV K555 D300 K10\n{seed}\nSET M1020\nSET M1021\nSET M1022\nSET M1029\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\nLD X0\n{body}\nMOV K123 D900\nINC D901\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLD M1029\nOUT Y5\nLDI X7\nMOV D1067 D902\nMOV D1068 D903\nEND"
    )
}

fn seed_words(values: &[i32], wide: bool, base: u16) -> String {
    values
        .iter()
        .enumerate()
        .map(|(i, value)| {
            format!(
                "{} K{value} D{}",
                if wide { "DMOV" } else { "MOV" },
                base + i as u16 * if wide { 2 } else { 1 }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn seed_group(values: &[i32], bits: u16, base: u16) -> String {
    let mut seed = Vec::new();
    for (element, value) in values.iter().enumerate() {
        for bit in 0..bits {
            if (*value as u32 >> bit) & 1 != 0 {
                seed.push(format!("SET M{}", base + element as u16 * bits + bit));
            }
        }
    }
    seed.join("\n")
}

fn bit_name(area: char, index: u16) -> String {
    if area == 'Y' {
        format!("Y{index:o}")
    } else {
        format!("{area}{index}")
    }
}

fn seed_result_group(area: char, base: u16, bits: u16) -> String {
    (base..base + bits * 5)
        .map(|i| format!("SET {}", bit_name(area, i)))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn cases(target: Target) -> Vec<Case> {
    let mut cases = Vec::new();
    let mut add = |name: &str, seed: &str, body: &str| {
        cases.push(case(
            &format!("search_value_{name}"),
            &fixture(seed, body),
            stages(),
        ));
    };
    let end: u16 = if target == Target::Ss2 { 4999 } else { 9999 };

    for wide in [false, true] {
        let op = if wide { "DSER" } else { "SER" };
        let mov = if wide { "DMOV" } else { "MOV" };
        let fill = if wide { "DFMOV" } else { "FMOV" };
        let inc = if wide { "DINC" } else { "INC" };
        let max = if wide { 128 } else { 256 };
        for pulse in [false, true] {
            let full = format!("{op}{}", if pulse { "P" } else { "" });
            add(
                &format!("lifecycle_{full}"),
                &seed_words(&[5, 5, 3], wide, 0),
                &format!("{full} D0 K5 D300 K3\n{inc} D0"),
            );
            for (index, (values, needle)) in [
                (vec![88, 100, 110, 150, 100, 300, 100, 5, 100, 500], 100),
                (vec![-7, 3, -7, 3, 0, -7], -7),
                (vec![-7, 3, -7, 3, 0, -7], 10),
                (vec![0], 0),
                (vec![-32768, 32767, -32768, 32767], 32767),
            ]
            .into_iter()
            .enumerate()
            {
                let n = values.len();
                add(
                    &format!("signed_ties_{full}_{index}"),
                    &seed_words(&values, wide, 0),
                    &format!("{full} D0 K{needle} D300 K{n}"),
                );
            }
            if wide {
                add(
                    &format!("signed32_extrema_{full}"),
                    &seed_words(&[i32::MIN, i32::MAX, -1, 0, i32::MIN, i32::MAX], true, 0),
                    &format!("{full} D0 K-1 D300 K6"),
                );
            }
            add(
                &format!("maximum_all_matches_{full}"),
                &format!("{fill} K7 D0 K{max}"),
                &format!("{full} D0 K7 D300 K{max}"),
            );
            add(
                &format!("maximum_no_matches_{full}"),
                &format!("{fill} K-7 D0 K{max}"),
                &format!("{full} D0 K8 D300 K{max}"),
            );
            add(
                &format!("maximum_extrema_last_{full}"),
                &format!(
                    "{fill} K0 D0 K{max}\n{mov} K-7 D{}\n{mov} K7 D{}",
                    (max - 2) * if wide { 2 } else { 1 },
                    (max - 1) * if wide { 2 } else { 1 }
                ),
                &format!("{full} D0 K0 D300 K{max}"),
            );
            for n in [0, -1, max + 1, 1, 3, max] {
                add(
                    &format!("constant_count_{full}_{n}"),
                    &format!("{fill} K7 D0 K{max}"),
                    &format!("{full} D0 K7 D300 K{n}"),
                );
                add(
                    &format!("dynamic_count_{full}_{n}"),
                    &format!("{fill} K7 D0 K{max}\n{mov} K{n} D280"),
                    &format!("{full} D0 K7 D300 D280"),
                );
            }
            for destination in [0, if wide { 2 } else { 1 }, if wide { 4 } else { 2 }] {
                add(
                    &format!("overlap_{full}_D{destination}"),
                    &seed_words(&[4, 2, 4, -7, 8], wide, 0),
                    &format!("{full} D0 K4 D{destination} K5"),
                );
            }
            add(
                &format!("needle_alias_{full}"),
                &seed_words(&[4, 2, 4, -7, 8], wide, 0),
                &format!("{full} D0 D{} D0 K5", if wide { 6 } else { 3 }),
            );
            add(
                &format!("count_destination_alias_{full}"),
                &format!("{}\n{mov} K3 D300", seed_words(&[3, 2, 3], wide, 0)),
                &format!("{full} D0 K3 D300 D300"),
            );
            // Both the start and the fixed five results end exactly at the
            // model boundary. Dynamic n1 must not require maximum capacity.
            let last_source = end - u16::from(wide);
            let last_dest = end - if wide { 9 } else { 4 };
            add(
                &format!("model_source_end_dynamic_n1_{full}"),
                &format!("{mov} K7 D{last_source}\n{mov} K1 D280"),
                &format!("{full} D{last_source} K7 D300 D280"),
            );
            add(
                &format!("model_result_end_{full}"),
                &seed_words(&[-1, 7, -1], wide, 0),
                &format!("{full} D0 K-1 D{last_dest} K3"),
            );
            for source in if wide {
                vec!["T254", "C198"]
            } else {
                vec!["T255", "C199"]
            } {
                let setup = if wide {
                    format!("DMOV K7 {source}")
                } else {
                    format!("DIV16 K7 K1 {source}")
                };
                add(
                    &format!("word_area_source_end_{full}_{source}"),
                    &format!("{setup}\n{mov} K1 D280"),
                    &format!("{full} {source} K7 D300 D280"),
                );
            }
            for dest in if wide {
                vec!["T246", "C190"]
            } else {
                vec!["T251", "C195"]
            } {
                add(
                    &format!("word_area_result_end_{full}_{dest}"),
                    &seed_words(&[-1, 7, -1], wide, 0),
                    &format!("{full} D0 K-1 {dest} K3"),
                );
            }
            for digits in 1..=if wide { 8u16 } else { 4u16 } {
                let bits = digits * 4;
                let seed = format!(
                    "{}\n{}",
                    seed_group(&[3, 1, 3], bits, 2000),
                    seed_result_group('M', 3000, bits)
                );
                add(
                    &format!("groups_{full}_K{digits}"),
                    &seed,
                    &format!("{full} K{digits}M2000 K3 K{digits}M3000 K3"),
                );
                // Full result group clears above bit7, and the fifth group's
                // final bit is the last valid bit of each destination area.
                for (area, limit) in [('Y', 255u16), ('S', 1023)] {
                    let base = limit + 1 - bits * 5;
                    let seed = format!(
                        "{}\n{}",
                        seed_group(&[3, 1, 3], bits, 2000),
                        seed_result_group(area, base, bits)
                    );
                    add(
                        &format!("group_result_end_{full}_K{digits}_{area}"),
                        &seed,
                        &format!(
                            "{full} K{digits}M2000 K3 K{digits}{} K3",
                            bit_name(area, base)
                        ),
                    );
                }
            }
            let bits = if wide { 32 } else { 16 };
            let digits = bits / 4;
            add(
                &format!("group_signed_and_alias_{full}"),
                &seed_group(&[-1, 0, -1, 7, 7], bits, 2000),
                &format!("{full} K{digits}M2000 K-1 K{digits}M2000 K5"),
            );
            add(
                &format!("group_needle_{full}"),
                &format!("{}\nSET M2100", seed_words(&[1, 0, 1], wide, 0)),
                &format!("{full} D0 K1M2100 D300 K3"),
            );
        }
    }
    add(
        "natural_counter_needle_DSER",
        "DFMOV K-7 D0 K3\nDMOV K-7 C200",
        "DSER D0 C200 D300 K3",
    );
    add(
        "word_timer_counter_arrays",
        "DIV16 K-7 K1 T250\nDIV16 K3 K1 T251\nDIV16 K-7 K1 T252\nDIV16 K3 K1 T253\nDIV16 K0 K1 T254",
        "SER T250 K-7 C195 K5",
    );
    add(
        "double_timer_counter_arrays",
        "DMOV K-70000 C190\nDMOV K90000 C192\nDMOV K-70000 C194\nDMOV K90000 C196",
        "DSER C190 K-70000 T246 K4",
    );
    add(
        "ser_preserves_m1029_between_toggles",
        "MOV K7 D0",
        "ALT M1029\nSER D0 K7 D300 K1\nALT M1029",
    );
    add(
        "dser_preserves_m1029_between_pulse_toggles",
        "DMOV K7 D0",
        "ALTP M1029\nDSERP D0 K7 D300 K1\nALTP M1029",
    );
    for op in ["ALT", "ALTP"] {
        for dest in [
            "Y7", "Y377", "M2000", "M4095", "S0", "S1023", "M1020", "M1029",
        ] {
            add(
                &format!("toggle_lifecycle_{op}_{dest}"),
                "",
                &format!("{op} {dest}"),
            );
        }
        add(
            &format!("toggle_distinct_occurrences_{op}"),
            "",
            &format!("{op} Y7\n{op} Y7\n{op} M2000"),
        );
    }
    add(
        "toggle_normal_and_pulse_same_bit",
        "",
        "ALT Y7\nALTP Y7\nALTP M2000\nALT M2000",
    );
    add("toggle_three_occurrences", "", "ALTP Y7\nALTP Y7\nALTP Y7");
    // X stack input uses static input names so no synthesized lifetime or
    // external state is needed. X0 remains an independent enabling contact.
    let mut input_stages = stages();
    for (inputs, _) in &mut input_stages {
        inputs.extend([("X10", true), ("X14", false), ("X20", true)]);
    }
    cases.push(case(
        "search_value_group_X_source_and_value",
        &fixture("", "SER K1X10 K1X10 D300 K3"),
        input_stages,
    ));
    cases
}
