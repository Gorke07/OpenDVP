//! Built-in DTM15 versus the exact COMMGR 2.11 ES2 image only.
use super::{Case, case};

#[derive(Clone, Copy)]
struct Input {
    longitude: f32,
    latitude: f32,
    zone: i16,
    dst: i16,
    date: [i16; 3],
}
const TAOYUAN: Input = Input {
    longitude: 121.301,
    latitude: 24.993,
    zone: 8,
    dst: 0,
    date: [18, 1, 1],
};

fn seed(base: u16, input: Input) -> String {
    let lon = input.longitude.to_bits();
    let lat = input.latitude.to_bits();
    [
        lon as u16,
        (lon >> 16) as u16,
        lat as u16,
        (lat >> 16) as u16,
        input.zone as u16,
        input.dst as u16,
        input.date[0] as u16,
        input.date[1] as u16,
        input.date[2] as u16,
    ]
    .into_iter()
    .enumerate()
    .map(|(n, word)| format!("MOV H{word:04X} D{}", base + n as u16))
    .collect::<Vec<_>>()
    .join("\n")
}

fn scenario(
    name: &str,
    op: &str,
    input: Input,
    source: u16,
    dest: u16,
    mode_count: &str,
    restore: bool,
) -> Case {
    let params = seed(source, input);
    let guard = |base: u16, count: u16| {
        let before = if base > 0 {
            format!("MOV H6AA6 D{}\n", base - 1)
        } else {
            String::new()
        };
        let after = if base + count <= 9999 {
            format!("MOV H3553 D{}", base + count)
        } else {
            String::new()
        };
        format!("{before}{after}")
    };
    // Seed outputs first: source/output overlap must still have valid inputs.
    let output = (0..6)
        .map(|n| format!("MOV K{} D{}", 700 + n, dest + n))
        .collect::<Vec<_>>()
        .join("\n");
    let advance_date = if restore {
        String::new()
    } else {
        // Changing the live input while X0 remains high distinguishes DTM
        // from DTMP; the latter must keep its first result until rearmed.
        format!("INC D{}", source + 8)
    };
    // The bridge validates native accesses at every instruction boundary,
    // including unpowered ones. Restore aliased inputs on the off scan too.
    let activation = if restore {
        format!("LD X0\nORI X0\n{params}\nLD X0")
    } else {
        "LD X0".into()
    };
    let source = format!(
        "LD X1\n{}\n{}\n{output}\n{params}\nMOV K271 D840\nMOV K-1 D841\nSET M1020\nRST M1021\nSET M1022\nSET M1029\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\nMOV K0 D901\n{activation}\n{op} D{source} D{dest} {mode_count}\n{advance_date}\nINC D901\nLD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1029\nOUT Y23\nLD M1067\nOUT Y24\nLD M1068\nOUT Y25\nLD X0\nORI X0\nMOV D1067 D902\nMOV D1068 D903\nEND",
        guard(source, 9),
        guard(dest, 6),
    );
    case(
        &format!("dtm_solar_{op}_{name}"),
        &source,
        vec![
            (vec![("X1", true), ("X0", false)], 1),
            (vec![("X1", false), ("X0", true)], 2),
            (vec![("X0", false)], 1),
            (vec![("X0", true)], 1),
        ],
    )
}

pub fn cases() -> Vec<Case> {
    let mut result = Vec::new();
    for op in ["DTM", "DTMP"] {
        for (name, longitude, latitude, zone, date) in [
            ("manual", 121.301, 24.993, 8, [18, 1, 1]),
            ("istanbul_june", 28.9784, 41.0082, 3, [24, 6, 21]),
            ("istanbul_dec", 28.9784, 41.0082, 3, [24, 12, 21]),
            ("newyork", -74.006, 40.7128, -5, [24, 3, 20]),
            ("sydney_june", 151.2093, -33.8688, 10, [24, 6, 21]),
            ("sydney_dec", 151.2093, -33.8688, 10, [24, 12, 21]),
            ("equator", 0.0, 0.0, 0, [24, 9, 22]),
            ("west_dateline", -180.0, 0.0, -12, [24, 2, 29]),
            ("east_dateline", 180.0, 0.0, 12, [24, 2, 29]),
            ("north_polar_day", 0.0, 90.0, 0, [24, 6, 21]),
            ("north_polar_night", 0.0, 90.0, 0, [24, 12, 21]),
            ("south_polar_day", 0.0, -90.0, 0, [24, 12, 21]),
            ("south_polar_night", 0.0, -90.0, 0, [24, 6, 21]),
            ("arctic_june", 18.9553, 69.6492, 1, [24, 6, 21]),
            ("arctic_winter", 18.9553, 69.6492, 1, [24, 12, 21]),
            // Independent f64 formula used only to select boundary inputs;
            // final triplet/hold claims come from native differential replay.
            ("rise_only_hold_boundary", 0.0, 65.7289, 0, [24, 6, 21]),
            ("set_only_hold_boundary", 0.0, 67.3902, 0, [24, 12, 21]),
        ] {
            result.push(scenario(
                name,
                op,
                Input {
                    longitude,
                    latitude,
                    zone,
                    dst: 0,
                    date,
                },
                100,
                200,
                "K15 K1",
                false,
            ));
        }
        for (name, date) in [
            ("year0_feb29", [0, 2, 29]),
            ("year0_march1", [0, 3, 1]),
            ("year2000_march1", [2000, 3, 1]),
            ("year4_march1", [4, 3, 1]),
            ("year24_leap", [24, 2, 29]),
            ("year2024_leap", [2024, 2, 29]),
            ("year18_end", [18, 12, 31]),
        ] {
            result.push(scenario(
                name,
                op,
                Input { date, ..TAOYUAN },
                100,
                200,
                "K15 K1",
                false,
            ));
        }
        for dst in [1i16, -1, 2] {
            result.push(scenario(
                &format!("dst_{dst}"),
                op,
                Input { dst, ..TAOYUAN },
                100,
                200,
                "K15 K1",
                false,
            ));
        }
        for count in [-1, 0, 256, 32767] {
            result.push(scenario(
                &format!("reserved_count_{count}"),
                op,
                TAOYUAN,
                100,
                200,
                &format!("K15 K{count}"),
                false,
            ));
        }
        result.push(scenario(
            "dynamic_lowbyte_selector",
            op,
            TAOYUAN,
            100,
            200,
            "D840 D841",
            false,
        ));
        for (name, source, dest) in [
            ("source_start", 0, 200),
            ("source_end", 9991, 200),
            ("dest_end", 100, 9994),
            ("alias_same", 100, 100),
            ("alias_geo_zone", 100, 102),
            ("alias_zone_year", 100, 104),
            ("alias_day", 100, 108),
            ("numeric_scratch_output", 100, 1028),
        ] {
            result.push(scenario(
                name,
                op,
                TAOYUAN,
                source,
                dest,
                "K15 K1",
                (100..=108).contains(&dest),
            ));
        }
    }
    result
}
