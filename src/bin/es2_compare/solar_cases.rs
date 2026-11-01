//! Explicit-civil-time DSPA probes; no host RTC or fabricated solar oracle.
use super::{Case, case};
use opendvp::es2_bridge::Target;

fn seed(date: [i16; 6], params: [f32; 10], source: u16, dest: u16) -> String {
    let mut lines = vec![
        format!("FMOV H5A5A D{source} K208"),
        format!("FMOV H6B6B D{dest} K8"),
    ];
    for (i, value) in date.into_iter().enumerate() {
        lines.push(format!("MOV K{value} D{}", source + i as u16));
    }
    // Delta-t, timezone, longitude, latitude, elevation, pressure,
    // temperature, slope, azimuth and refraction are raw IEEE binary32 pairs.
    for (i, value) in params.into_iter().enumerate() {
        lines.push(format!(
            "DMOV H{:08X} D{}",
            value.to_bits(),
            source + 6 + 2 * i as u16
        ));
    }
    lines.join("\n")
}

fn source(date: [i16; 6], params: [f32; 10], base: u16, dest: u16, suffix: &str) -> String {
    let seed = seed(date, params, base, dest);
    format!(
        "LD X1\n{seed}\nSET M1020\nSET M1021\nSET M1022\nSET M1029\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\nLD X0\nDSPA D{base} D{dest}\n{suffix}\nMOV K123 D900\nINC D901\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLD M1029\nOUT Y5\nLDI X7\nMOV D1067 D902\nMOV D1068 D903\nEND"
    )
}

fn stages(alias_sensitive: bool) -> Vec<(Vec<(&'static str, bool)>, usize)> {
    let mut result = vec![
        (vec![("X1", true), ("X0", false)], 1),
        (
            vec![("X1", false), ("X0", true)],
            if alias_sensitive { 1 } else { 2 },
        ),
    ];
    if !alias_sensitive {
        result.push((vec![("X0", false)], 1));
        result.push((vec![("X0", true)], 1));
    }
    result
}

pub(super) fn cases(target: Target) -> Vec<Case> {
    let mut result = Vec::new();
    let date = [2009, 3, 23, 10, 10, 30];
    let params = [
        0.0, 8.0, 119.192345, 24.593456, 132.2, 820.0, 15.0, 30.0, -10.0, 0.5667,
    ];
    let mut add = |name: &str, dt, p, base, dest, suffix: &str, alias| {
        result.push(case(
            &format!("solar_native_{name}"),
            &source(dt, p, base, dest, suffix),
            stages(alias),
        ));
    };
    add("manual_civil_example", date, params, 2000, 3000, "", false);
    for (name, dt) in [
        ("midnight", [2024, 6, 21, 0, 0, 0]),
        ("noon", [2024, 6, 21, 12, 0, 0]),
        ("leap_day", [2024, 2, 29, 11, 59, 59]),
        ("century_nonleap", [2100, 3, 1, 8, 30, 15]),
        ("century_leap", [2400, 2, 29, 9, 30, 30]),
        ("new_year", [2000, 1, 1, 0, 0, 0]),
        ("winter", [2025, 12, 21, 12, 30, 45]),
        ("equinox", [2025, 9, 22, 18, 0, 0]),
    ] {
        add(name, dt, params, 2000, 3000, "", false);
    }
    for (field, values) in [
        (0, vec![-100.0, 67.0, 1000.0]),
        (2, vec![-179.0, -75.0, 0.0, 179.0]),
        (3, vec![-80.0, -33.0, 0.0, 51.5, 80.0]),
        (4, vec![0.0, 1500.0, 5000.0]),
        (5, vec![0.0, 1013.25]),
        (6, vec![-40.0, 0.0, 45.0]),
        (7, vec![0.0, 90.0, 180.0]),
        (8, vec![-180.0, 0.0, 180.0]),
    ] {
        for (i, value) in values.into_iter().enumerate() {
            let mut p = params;
            p[field] = value;
            add(
                &format!("parameter_{field}_{i}"),
                date,
                p,
                2000,
                3000,
                "",
                false,
            );
        }
    }
    for (i, ignored) in [0.0f32, -12.0, 14.0, f32::NAN, f32::INFINITY]
        .into_iter()
        .enumerate()
    {
        let mut p = params;
        p[1] = ignored;
        p[9] = ignored;
        add(
            &format!("ignored_timezone_refraction_{i}"),
            date,
            p,
            2000,
            3000,
            "",
            false,
        );
    }
    for offset in [0, 1, 6, 10, 18, 24, 26, 100, 200] {
        add(
            &format!("source_alias_{offset}"),
            date,
            params,
            2000,
            2000 + offset,
            "",
            true,
        );
    }
    let last = if target == Target::Ss2 { 4999 } else { 9999 };
    add(
        "last_source_block",
        date,
        params,
        last - 207,
        3000,
        "",
        false,
    );
    add(
        "last_destination_block",
        date,
        params,
        2000,
        last - 7,
        "",
        false,
    );
    // A real wrapper scratch writer must also agree with legacy SORT, rather
    // than being inferred merely from matching DSPA's eight outputs.
    add(
        "sort_scratch_successor",
        date,
        params,
        2000,
        3000,
        "SORT D3500 K3 K2 D3600 K1",
        false,
    );
    // Reproducible correlated civil/geographical inputs exercise the complete
    // helper chain together, beyond varying one example parameter at a time.
    let mut state = 0x51a7_2026u32;
    let mut next = || {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        state
    };
    for i in 0..64 {
        let dt = [
            2000 + (next() % 100) as i16,
            1 + (next() % 12) as i16,
            1 + (next() % 28) as i16,
            (next() % 24) as i16,
            (next() % 60) as i16,
            (next() % 60) as i16,
        ];
        let p = [
            (next() % 1000) as f32 / 10.0,
            0.0,
            (next() % 34000) as f32 / 100.0 - 170.0,
            (next() % 16000) as f32 / 100.0 - 80.0,
            (next() % 4000) as f32,
            (next() % 11000) as f32 / 10.0,
            (next() % 1000) as f32 / 10.0 - 50.0,
            (next() % 900) as f32 / 10.0,
            (next() % 3600) as f32 / 10.0 - 180.0,
            0.5667,
        ];
        result.push(case(
            &format!("solar_native_correlated_{i}"),
            &source(dt, p, 2000, 3000, ""),
            stages(false),
        ));
    }
    result
}
