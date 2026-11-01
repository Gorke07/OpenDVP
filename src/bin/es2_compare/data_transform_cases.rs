//! Finite DTM differential scenarios. Potentially nonresponsive raw-float
//! probes are retained separately and must be explicitly selected by the CLI.
use super::{Case, case};
use opendvp::es2_bridge::Target;

fn scenario(name: &str, seed: &str, body: &str) -> Case {
    case(
        &format!("search_transform_dtm_{name}"),
        &format!(
            "LD X1\nFMOV K0 D0 K24\nFMOV H5511 D100 K24\n{seed}\nSET M1020\nSET M1021\nSET M1022\nSET M1029\nLD X0\n{body}\nMOV K123 D300\nINC D301\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1029\nOUT Y3\nLD M1067\nOUT Y4\nLD M1068\nOUT Y5\nLDI X7\nMOV D1067 D302\nMOV D1068 D303\nEND"
        ),
        vec![
            (vec![("X1", true)], 1),
            (vec![("X1", false), ("X0", true)], 2),
            (vec![("X0", false)], 1),
            (vec![("X0", true)], 1),
        ],
    )
}

fn chars(value: &str, base: u16) -> String {
    let mut seed = String::new();
    for (offset, value) in value.bytes().chain([0]).enumerate() {
        seed.push_str(&format!(
            "MOV H{:04X} D{}\n",
            0xab00 | u16::from(value),
            usize::from(base) + offset
        ));
    }
    seed
}

fn float_seed(values: &[(u16, f32)]) -> String {
    values
        .iter()
        .map(|(index, value)| format!("DMOV H{:08X} D{index}\n", value.to_bits()))
        .collect()
}

pub(super) fn cases(target: Target) -> Vec<Case> {
    let mut result = Vec::new();
    let gps = "MOV K1 D0\nMOV K23 D1\nMOV K45 D2\nMOV K67 D3\nMOV K78 D4\nMOV K1 D5\nMOV K21 D6\nMOV K30 D7\nMOV K14 D8\nMOV K5 D9\nMOV K69 D10";
    let positioning = "DMOV K10000 D0\nMOV K200 D2\nMOV K50 D3\nMOV K50 D4";
    let calendar = format!(
        "{}MOV K8 D2\nMOV K2011 D3\nMOV K1 D4\nMOV K6 D5\nMOV K8 D6\nMOV K0 D7\nMOV K0 D8",
        float_seed(&[(0, 121.55)])
    );
    let interp16 = "MOV K500 D0\nMOV K200 D1\nMOV K3000 D2\nMOV K30 D101\nMOV K500 D102";
    let interp32 = "DMOV K500 D0\nDMOV K200 D2\nDMOV K3000 D4\nDMOV K30 D102\nDMOV K500 D104";
    let interp_float = float_seed(&[
        (0, 500.0),
        (2, 200.0),
        (4, 3000.0),
        (102, 30.0),
        (104, 500.0),
    ]);
    for mode in (0..=9).chain(11..=14).chain(16..=19) {
        let (seed, count) = match mode {
            6 | 7 => (chars("1aZ9", 0), 4),
            8 => (gps.into(), 1),
            9 => (positioning.into(), 200),
            11 => (calendar.clone(), 1),
            12 => (interp16.into(), 2),
            13 => (interp32.into(), 2),
            14 => (interp_float.clone(), 2),
            16 => (format!("{}{}", chars("ABC", 0), chars("xy", 100)), 8),
            17 => (chars("ABC", 0), 2),
            18 => (chars("123.456", 0), 6),
            19 => (float_seed(&[(0, 123.456)]), 6),
            _ => ("MOV H1234 D0\nMOV HABCD D1\nMOV H9876 D2".into(), 3),
        };
        for pulse in [false, true] {
            let op = if pulse { "DTMP" } else { "DTM" };
            result.push(scenario(
                &format!("lifecycle_{mode}_{op}"),
                &seed,
                &format!("{op} D0 D100 K{mode} K{count}\nINC D0"),
            ));
        }
    }
    for mode in 0..=7 {
        for count in [1, 2, 3, 5] {
            result.push(scenario(
                &format!("bytes_mode_{mode}_count_{count}"),
                "MOV HAA12 D0\nMOV HBB34 D1\nMOV HCCAB D2\nMOV HDDCD D3\nMOV HEEFE D4",
                &format!("DTM D0 D100 K{mode} K{count}"),
            ));
        }
        for dest in ["D0", "D1", "D2", "D99"] {
            result.push(scenario(
                &format!("bytes_mode_{mode}_alias_{dest}"),
                "MOV HAA12 D0\nMOV HBB34 D1\nMOV HCCAB D2\nMOV HDDCD D3\nMOV HEEFE D4",
                &format!("DTM D0 {dest} K{mode} K5"),
            ));
        }
        result.push(scenario(
            &format!("dynamic_mode_{mode}"),
            &format!("{}MOV K{mode} D250\nMOV K5 D251", chars("aFZ19", 0)),
            "DTM D0 D100 D250 D251",
        ));
    }
    for mode in [0, 1, 2, 4, 6, 7] {
        for count in [-1, 0, 255, 256, 257] {
            let mut value = scenario(
                &format!("bytes_mode_{mode}_clamp_{count}"),
                &format!("FMOV HAA31 D0 K256\nFMOV H5511 D2000 K512\nMOV K{count} D251"),
                &format!("DTM D0 D2000 K{mode} D251"),
            );
            value.stages = vec![
                (vec![("X1", true)], 1),
                (vec![("X1", false), ("X0", true)], 1),
            ];
            result.push(value);
        }
    }
    for mode in [6, 7] {
        for text in ["09aAfF", "gG!?zZ", "12345", "aBcDeF"] {
            result.push(scenario(
                &format!("ascii_mode_{mode}_{text}"),
                &chars(text, 0),
                &format!("DTM D0 D100 K{mode} K{}", text.len()),
            ));
        }
    }
    for (index, (north, east, first, second)) in [
        (78, 69, 0, 0),
        (83, 87, 0, 0),
        (78, 87, 12, 255),
        (0, 0, 255, 255),
    ]
    .into_iter()
    .enumerate()
    {
        result.push(scenario(
            &format!("gps_chunks_{index}"),
            &format!("{gps}\nMOV K{north} D4\nMOV K{east} D10\nMOV K{first} D0\nMOV K{second} D6"),
            "DTM D0 D100 K8 K256",
        ));
    }
    for dest in ["D0", "D5", "D7"] {
        result.push(scenario(
            &format!("gps_alias_{dest}"),
            gps,
            &format!("DTM D0 {dest} K8 K1"),
        ));
    }
    for (index, (pulses, total, up, down, n)) in [
        (10000, 200, 50, 50, 0),
        (10000, 200, 50, 50, 200),
        (10000, 300, 50, 100, 200),
        (1000, 10, 20, 30, 1),
        (1, 100, 0, 0, 100),
        (0, 100, 10, 10, 100),
        (12345, 700, 100, 100, 256),
        (32767, 100, 10, 30, 200),
        (2147483, 100, 10, 10, 200),
        (100, 99, -1, 2, 100),
    ]
    .into_iter()
    .enumerate()
    {
        result.push(scenario(
            &format!("positioning_{index}"),
            &format!(
                "DMOV K{pulses} D0\nMOV K{total} D2\nMOV K{up} D3\nMOV K{down} D4\nMOV K{n} D251"
            ),
            "DTM D0 D100 K9 D251",
        ));
    }
    for dest in ["D0", "D1", "D2", "D3"] {
        result.push(scenario(
            &format!("positioning_alias_{dest}"),
            positioning,
            &format!("DTM D0 {dest} K9 K200"),
        ));
    }
    for (index, (longitude, timezone, date, time)) in [
        (121.55f32, 8, [2011, 1, 6], [8, 0, 0]),
        (0.0, 0, [2024, 2, 28], [23, 59, 59]),
        (180.0, 0, [2024, 2, 28], [23, 30, 30]),
        (-180.0, 0, [2024, 3, 1], [0, 30, 30]),
        (200.0, 0, [2025, 12, 31], [18, 30, 0]),
        (-200.0, 0, [2025, 1, 1], [1, 0, 0]),
        (0.0, -1, [2025, 11, 30], [23, 0, 0]),
        (0.0, -1, [2025, 12, 31], [23, 0, 0]),
        (0.0, 1, [2025, 1, 1], [0, 0, 0]),
        (0.0, 8, [2025, 3, 1], [0, 0, 0]),
        (0.1, 0, [2025, 1, 1], [0, 0, 0]),
        (-0.1, 0, [2025, 1, 1], [0, 0, 0]),
    ]
    .into_iter()
    .enumerate()
    {
        let mut seed = format!("{}MOV K{timezone} D2\n", float_seed(&[(0, longitude)]));
        for (offset, value) in date.into_iter().chain(time).enumerate() {
            seed.push_str(&format!("MOV K{value} D{}\n", 3 + offset));
        }
        result.push(scenario(
            &format!("calendar_{index}"),
            &seed,
            "DTM D0 D100 K11 K1",
        ));
    }
    for dest in ["D0", "D2", "D4"] {
        result.push(scenario(
            &format!("calendar_alias_{dest}"),
            &calendar,
            &format!("DTM D0 {dest} K11 K1"),
        ));
    }
    for (mode, seed) in [
        (12, interp16.to_string()),
        (13, interp32.to_string()),
        (14, interp_float.clone()),
    ] {
        for count in [1, 2, 3, 50, 51] {
            result.push(scenario(
                &format!("interpolation_mode_{mode}_count_{count}"),
                &seed,
                &format!("DTM D0 D100 K{mode} K{count}"),
            ));
        }
        for dest in ["D0", "D1", "D2", "D101", "D102"] {
            result.push(scenario(
                &format!("interpolation_mode_{mode}_alias_{dest}"),
                &seed,
                &format!("DTM D0 {dest} K{mode} K2"),
            ));
        }
    }
    for (index, [input, low, high, y1, y2]) in [
        [1, 0, 2, 0, 3],
        [-1, -2, 0, -3, 0],
        [-1, 0, 2, 0, 3],
        [3, 0, 2, 0, 3],
        [0, 0, 0, 10, 20],
        [0, 2, 0, 10, 20],
        [32767, -32768, 32767, -32768, 32767],
        [0, -32768, 32767, 32767, -32768],
    ]
    .into_iter()
    .enumerate()
    {
        result.push(scenario(
            &format!("interpolation_word_values_{index}"),
            &format!(
                "MOV K{input} D0\nMOV K{low} D1\nMOV K{high} D2\nMOV K{y1} D101\nMOV K{y2} D102"
            ),
            "DTM D0 D100 K12 K2",
        ));
    }
    for (index, [input, low, high, y1, y2]) in [
        [1, 0, 2, 0, 3],
        [-1, -2, 0, -3, 0],
        [0, 0, 0, 10, 20],
        [16777217, 0, 33554432, 0, 33554431],
        [2147483647, -2147483648, 2147483647, -2147483648, 2147483647],
        [0, -2147483648, 2147483647, 2147483647, -2147483648],
    ]
    .into_iter()
    .enumerate()
    {
        result.push(scenario(&format!("interpolation_double_values_{index}"),
            &format!("DMOV K{input} D0\nDMOV K{low} D2\nDMOV K{high} D4\nDMOV K{y1} D102\nDMOV K{y2} D104"),"DTM D0 D100 K13 K2"));
    }
    for (index, [input, low, high, y1, y2]) in [
        [1.0f32, 0.0, 2.0, 0.0, 3.0],
        [-1.0, -2.0, 0.0, -3.0, 0.0],
        [-1.0, 0.0, 2.0, 0.0, 3.0],
        [3.0, 0.0, 2.0, 0.0, 3.0],
        [0.1, -0.7, 0.3, -1.3, 2.2],
        [1e20, 0.0, 3e20, -2e20, 7e20],
        [0.0, 0.0, 0.0, 10.0, 20.0],
    ]
    .into_iter()
    .enumerate()
    {
        result.push(scenario(
            &format!("interpolation_float_values_{index}"),
            &float_seed(&[(0, input), (2, low), (4, high), (102, y1), (104, y2)]),
            "DTM D0 D100 K14 K2",
        ));
    }
    for mode in [16, 17] {
        for (index, (source, dest, n)) in [
            ("", "", 1),
            ("ABC", "xy", 8),
            ("ABCDE", "x", 3),
            ("A", "abcdef", 3),
            ("ABC", "", 1),
        ]
        .into_iter()
        .enumerate()
        {
            result.push(scenario(
                &format!("string_{mode}_{index}"),
                &format!("{}{}", chars(source, 0), chars(dest, 100)),
                &format!("DTM D0 D100 K{mode} K{n}"),
            ));
        }
        for dest in ["D0", "D1", "D2"] {
            result.push(scenario(
                &format!("string_{mode}_alias_{dest}"),
                &chars("abcDEF", 0),
                &format!("DTM D0 {dest} K{mode} K5"),
            ));
        }
    }
    for (index, text) in [
        "123.45678",
        "12x.5",
        "12.3x4",
        "-.5",
        "1e2",
        ".12345678",
        "12345",
        "",
    ]
    .into_iter()
    .enumerate()
    {
        for count in [1, 4, 8] {
            result.push(scenario(
                &format!("parse_decimal_{index}_{count}"),
                &chars(text, 0),
                &format!("DTM D0 D100 K18 K{count}"),
            ));
        }
    }
    for value in [
        0.0f32,
        -0.0,
        0.1234,
        1.0,
        9.999999,
        123.45678,
        123456.78,
        99999992.0,
        100000000.0,
        -0.1,
        -1.2,
        -1234.567,
    ] {
        for count in [1, 4, 8] {
            result.push(scenario(
                &format!("format_decimal_{:08X}_{count}", value.to_bits()),
                &float_seed(&[(0, value)]),
                &format!("DTM D0 D100 K19 K{count}"),
            ));
        }
    }
    for mode in [18, 19] {
        let seed = if mode == 18 {
            chars("12.345", 0)
        } else {
            float_seed(&[(0, 12.345)])
        };
        for dest in ["D0", "D1", "D2"] {
            result.push(scenario(
                &format!("decimal_{mode}_alias_{dest}"),
                &seed,
                &format!("DTM D0 {dest} K{mode} K5"),
            ));
        }
    }
    for mode in [20, 21, 29, 37, 38, 40, 51, 53, 255] {
        result.push(scenario(
            &format!("unsupported_noop_{mode}"),
            "MOV K-1 D0",
            &format!("DTM D0 D100 K{mode} K5"),
        ));
    }
    result.push(scenario(
        "mode_low_byte_alias",
        &chars("ABC", 0),
        "DTM D0 D100 H0104 K3",
    ));
    result.push(scenario(
        "dynamic_mode_changed_inside_scan",
        &chars("aB9", 0),
        "MOV K4 D250\nDTM D0 D100 D250 K3\nMOV K5 D250\nDTM D0 D110 D250 K3",
    ));
    let last = if target == Target::Ss2 { 4999 } else { 9999 };
    for mode in [0, 1, 6, 7] {
        result.push(scenario(
            &format!("last_word_mode_{mode}"),
            &format!("MOV HAA31 D{last}\nMOV H5500 D{}", last - 1),
            &format!("DTM D{last} D{} K{mode} K1", last - 1),
        ));
    }
    if target == Target::Es2 {
        for mode in [0, 1, 2, 3, 4, 5, 6, 7] {
            result.push(scenario(
                &format!("partial_physical_end_{mode}"),
                "MOV HAA31 D9999\nFMOV H5511 D2000 K8",
                &format!("DTM D9999 D2000 K{mode} K3"),
            ));
        }
    }
    for mode in [16, 17] {
        result.push(scenario(
            &format!("terminated_near_end_{mode}"),
            &format!("MOV HAA00 D0\nMOV HBB00 D{last}"),
            &format!("DTM D0 D{last} K{mode} K256"),
        ));
        result.push(scenario(
            &format!("source_terminated_near_end_{mode}"),
            &format!("MOV HAA00 D{last}\nMOV HBB00 D100"),
            &format!("DTM D{last} D100 K{mode} K256"),
        ));
    }
    result.push(scenario(
        "format_near_end",
        &float_seed(&[(0, 123.456)]),
        &format!("DTM D0 D{} K19 K8", last - 9),
    ));
    append_sort_scratch_cases(&mut result);
    result
}

/// Append independent interaction fixtures without changing any retained
/// first-probe source. Word SORT reads the shared numeric scratch left by DTM;
/// exercising every DTM writer is stronger than isolated conversion equality.
fn append_sort_scratch_cases(result: &mut Vec<Case>) {
    let selected: Vec<_> = result
        .iter()
        .filter_map(|item| {
            let suffix = item.name.strip_prefix("search_transform_dtm_")?;
            let lifecycle = suffix.starts_with("lifecycle_") && suffix.ends_with("_DTM");
            let pulse = matches!(
                suffix,
                "lifecycle_0_DTMP"
                    | "lifecycle_11_DTMP"
                    | "lifecycle_12_DTMP"
                    | "lifecycle_14_DTMP"
            );
            let calendar = matches!(
                suffix,
                "calendar_2"
                    | "calendar_3"
                    | "calendar_6"
                    | "calendar_8"
                    | "calendar_10"
                    | "calendar_11"
            );
            let invalid_count = matches!(
                suffix,
                "interpolation_mode_12_count_1"
                    | "interpolation_mode_12_count_51"
                    | "interpolation_mode_13_count_1"
                    | "interpolation_mode_13_count_51"
                    | "interpolation_mode_14_count_1"
                    | "interpolation_mode_14_count_51"
            );
            let segment = matches!(
                suffix,
                "interpolation_word_values_2"
                    | "interpolation_word_values_3"
                    | "interpolation_word_values_4"
                    | "interpolation_double_values_0"
                    | "interpolation_double_values_2"
                    | "interpolation_float_values_2"
                    | "interpolation_float_values_3"
                    | "interpolation_float_values_6"
            );
            (lifecycle || pulse || calendar || invalid_count || segment)
                .then(|| (suffix.to_owned(), item.source.clone()))
        })
        .collect();
    for (mut suffix, mut source) in selected {
        if suffix == "interpolation_double_values_0" {
            source = source.replacen("DMOV K1 D0", "DMOV K3 D0", 1);
            suffix = "interpolation_double_above_final".into();
        }
        source=source.replacen("SET M1020\n","MOV K30 D400\nMOV K10 D401\nMOV K20 D402\nMOV K3 D403\nMOV K1 D404\nMOV K2 D405\nSET M1020\n",1);
        source = source.replacen("MOV K123 D300", "SORT D400 K3 K2 D500 K1\nMOV K123 D300", 1);
        result.push(case(
            &format!("search_transform_dtm_sort_scratch_{suffix}"),
            &source,
            vec![
                (vec![("X1", true)], 1),
                (vec![("X1", false), ("X0", true)], 5),
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 4),
            ],
        ));
    }
}

/// Retained experimental raw-input cases. These have not been observed to
/// fail; they are separate because native floating exceptions may not return.
#[allow(dead_code)]
pub(super) fn risky_cases(_target: Target) -> Vec<Case> {
    let mut result = Vec::new();
    for raw in [
        0x7f800000u32,
        0xff800000,
        0x7fc12345,
        0xffc12345,
        0x7f812345,
        0xff812345,
    ] {
        result.push(scenario(
            &format!("risky_format_{raw:08X}"),
            &format!("DMOV H{raw:08X} D0"),
            "DTM D0 D100 K19 K8",
        ));
        for source in [0, 2, 4, 102, 104] {
            let mut seed = float_seed(&[(0, 1.0), (2, 0.0), (4, 2.0), (102, 0.0), (104, 3.0)]);
            seed.push_str(&format!("DMOV H{raw:08X} D{source}"));
            result.push(scenario(
                &format!("risky_interpolation_{raw:08X}_{source}"),
                &seed,
                "DTM D0 D100 K14 K2",
            ));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finite_transform_corpus_fits_native_budget_and_separates_risky_inputs() {
        for target in [Target::Es2, Target::Ss2] {
            let finite = cases(target);
            println!(
                "{target:?}: {} finite DTM scenarios, {} separate risky scenarios",
                finite.len(),
                risky_cases(target).len()
            );
            let mut names = std::collections::BTreeSet::new();
            for item in finite.iter().chain(risky_cases(target).iter()) {
                assert!(names.insert(item.name.clone()), "duplicate {}", item.name);
                let program = opendvp::plc::compile(&item.source)
                    .unwrap_or_else(|error| panic!("{}: {error}", item.name));
                let encoded = opendvp::es2_encoder::encode(&program)
                    .unwrap_or_else(|error| panic!("{}: {error}", item.name));
                assert!(
                    encoded.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                    "{} exceeds native budget",
                    item.name
                );
            }
            assert!(finite.iter().all(|item| !item.name.contains("risky_")));
        }
    }
}
