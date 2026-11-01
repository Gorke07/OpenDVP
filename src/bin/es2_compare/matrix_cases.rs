//! Matrix differential sources: all counts valid1..256, full spans bounded.
//! No grouped invalid-count/overflow probes and no physical-input-write claim.
use super::{Case, case};
use opendvp::es2_bridge::Target;

fn packed(raw: &str) -> Option<(usize, char, usize)> {
    let tail = raw.strip_prefix('K')?;
    let width = tail[..1].parse::<usize>().unwrap() * 4;
    let area = tail.as_bytes()[1] as char;
    let index = if matches!(area, 'X' | 'Y') {
        usize::from_str_radix(&tail[2..], 8).unwrap()
    } else {
        tail[2..].parse().unwrap()
    };
    Some((width, area, index))
}
fn bit_name(area: char, index: usize) -> String {
    if matches!(area, 'X' | 'Y') {
        format!("{area}{index:o}")
    } else {
        format!("{area}{index}")
    }
}
fn seed(base: &str, values: &[i16]) -> String {
    if let Some((width, area, index)) = packed(base) {
        assert_ne!(
            area, 'X',
            "X source is seeded through explicit stage inputs"
        );
        let limit = match area {
            'Y' => 256,
            'S' => 1024,
            _ => 4096,
        };
        assert!(index + width * values.len() <= limit);
        return values
            .iter()
            .enumerate()
            .flat_map(|(row, value)| {
                (0..width).map(move |bit| {
                    format!(
                        "{} {}\n",
                        if (*value as u16) & (1 << bit) == 0 {
                            "RST"
                        } else {
                            "SET"
                        },
                        bit_name(area, index + row * width + bit)
                    )
                })
            })
            .collect();
    }
    let index = base[1..].parse::<usize>().unwrap();
    if !base.starts_with('D') {
        return seed("D600", values) + &format!("BMOV D600 {base} K{}\n", values.len());
    }
    values
        .iter()
        .enumerate()
        .map(|(i, value)| format!("MOV K{value} D{}\n", index + i))
        .collect()
}
fn flags(settings: &[(u16, bool)]) -> String {
    let mut source = String::from("SET M1020\nSET M1021\nSET M1022\nSET M1029\n");
    for index in 1088..=1099 {
        source.push_str(&format!(
            "{} M{index}\n",
            if settings.iter().any(|(i, value)| *i == index && *value) {
                "SET"
            } else {
                "RST"
            }
        ));
    }
    source
}
fn staged(name: &str, init: &str, update: &str, body: &str, settings: &[(u16, bool)]) -> Case {
    let update = if update.is_empty() {
        String::new()
    } else {
        format!("LD X2\n{update}")
    };
    case(
        &format!("matrix_{name}"),
        &format!(
            "LD X1\n{init}{}{update}LD X0\n{body}\nMOV K123 D900\nEND",
            flags(settings)
        ),
        vec![
            (vec![("X1", true), ("X0", false), ("X2", false)], 1),
            (vec![("X1", false), ("X0", true)], 3),
            (vec![("X2", true)], 1),
            (vec![("X2", false), ("X0", false)], 1),
            (vec![("X0", true)], 3),
        ],
    )
}
fn q_seed(pointer: &str, value: i16) -> String {
    if pointer.starts_with('E') || pointer.starts_with('F') {
        let n = pointer[1..].parse::<usize>().unwrap();
        let alias =
            if n == 0 { 1028 } else { 1180 + 2 * n } + usize::from(pointer.starts_with('F'));
        seed(&format!("D{alias}"), &[value])
    } else {
        seed(pointer, &[value])
    }
}
fn source_x(mut fixture: Case, operand: &str, values: &[i16]) -> Case {
    let (width, area, base) = packed(operand).unwrap();
    assert_eq!(area, 'X');
    assert!(base + width * values.len() <= 256);
    for (row, value) in values.iter().enumerate() {
        for bit in 0..width {
            let label: &'static str =
                Box::leak(bit_name('X', base + row * width + bit).into_boxed_str());
            fixture.stages[0]
                .0
                .push((label, (*value as u16) & (1 << bit) != 0));
        }
    }
    fixture
}

pub fn cases(target: Target) -> Vec<Case> {
    let mut out = Vec::new();
    let end = if target == Target::Ss2 { 4999 } else { 9999 };
    let left = [0xaaaa_u16 as i16, 0x5555, 0x00ff];
    let right = [0x5555, 0x0f0f, 0xf0f0_u16 as i16];
    for family in ["MAND", "MOR", "MXOR", "MXNR", "MINV", "MBS", "MBR"] {
        for pulse in [false, true] {
            let op = format!("{family}{}", if pulse { "P" } else { "" });
            let binary = matches!(family, "MAND" | "MOR" | "MXOR" | "MXNR");
            let line = |a: &str, b: &str, dest: &str, n: &str| {
                if binary {
                    format!("{op} {a} {b} {dest} {n}")
                } else {
                    format!("{op} {a} {dest} {n}")
                }
            };
            for dest in ["D20", "D0", "D1", "D9", "D10", "D11"] {
                let init = seed("D0", &left) + &seed("D10", &right);
                out.push(staged(
                    &format!("{op}_raw_alias_{dest}"),
                    &init,
                    "",
                    &line("D0", "D10", dest, "K3"),
                    &[],
                ));
            }
            for area in ["T10", "C10", "D0"] {
                let init = seed(area, &left) + &seed("D10", &right);
                out.push(staged(
                    &format!("{op}_raw_source_{area}"),
                    &init,
                    "",
                    &line(area, "D10", "D20", "K3"),
                    &[],
                ));
            }
            for dest in ["T20", "C20"] {
                let init = seed("D0", &left) + &seed("D10", &right);
                out.push(staged(
                    &format!("{op}_raw_dest_{dest}"),
                    &init,
                    "",
                    &line("D0", "D10", dest, "K3"),
                    &[],
                ));
            }
            for digits in 1..=4 {
                let group = format!("K{digits}M0");
                let dest = format!("K{digits}M128");
                for (index, (a, b, d)) in [
                    (group.as_str(), "D10", "D20"),
                    ("D0", group.as_str(), "D20"),
                    ("D0", "D10", dest.as_str()),
                    (group.as_str(), "D10", group.as_str()),
                    (group.as_str(), dest.as_str(), "K2M4"),
                ]
                .into_iter()
                .enumerate()
                {
                    let init = seed("D0", &left)
                        + &seed("D10", &right)
                        + &seed(&group, &left)
                        + &seed(&dest, &right);
                    out.push(staged(
                        &format!("{op}_packed{digits}_mix_{index}"),
                        &init,
                        "",
                        &line(a, b, d, "K3"),
                        &[],
                    ));
                }
            }
            for (index, (a, dest)) in [("K2Y100", "K3S64"), ("K3S64", "K1Y200"), ("K4M64", "K4M65")]
                .into_iter()
                .enumerate()
            {
                let init = seed(a, &left) + &seed("D10", &right);
                out.push(staged(
                    &format!("{op}_packed_area_alias_{index}"),
                    &init,
                    "",
                    &line(a, "D10", dest, "K3"),
                    &[],
                ));
            }
            let init = seed("D10", &right);
            out.push(source_x(
                staged(
                    &format!("{op}_packed_X"),
                    &init,
                    "",
                    &line("K2X100", "D10", "D20", "K3"),
                    &[],
                ),
                "K2X100",
                &[0x37, 0x68, 0x45],
            ));
            let init = seed("D0", &left) + &seed("D10", &right) + &seed("D50", &[3]);
            out.push(staged(
                &format!("{op}_dynamic_count"),
                &init,
                &seed("D50", &[1]),
                &line("D0", "D10", "D20", "D50"),
                &[],
            ));
            let (alias_left, alias_right) = match family {
                "MINV" | "MXNR" => ([-4, -3, -2], [0, 0, 0]),
                "MAND" => ([3, 2, 1], [3, 3, 3]),
                "MBS" | "MBR" => ([1, 0, 0], [0, 0, 0]),
                _ => ([3, 2, 1], [0, 0, 0]),
            };
            // D20 is both count and first output. Every later held count is
            // still2/3, so this never turns into an invalid-count native probe.
            let init = seed("D0", &alias_left) + &seed("D10", &alias_right) + &seed("D20", &[3]);
            out.push(staged(
                &format!("{op}_count_alias_first_output"),
                &init,
                "",
                &line("D0", "D10", "D20", "D20"),
                &[],
            ));
            let init = String::from("FMOV HAAAA D2000 K256\nFMOV H0F0F D2400 K256\n");
            out.push(staged(
                &format!("{op}_n256_model_end"),
                &init,
                "",
                &line("D2000", "D2400", &format!("D{}", end - 255), "H100"),
                &[],
            ));
            out.push(staged(
                &format!("{op}_n1"),
                &(seed("D0", &left) + &seed("D10", &right)),
                "",
                &line("D0", "D10", "D20", "H1"),
                &[],
            ));
            if matches!(family, "MBS" | "MBR") {
                for direction in [false, true] {
                    for fill in [false, true] {
                        for dest in ["D0", "D1", "D20", "K2M128"] {
                            let init = seed("D0", &[1, 0, i16::MIN]);
                            out.push(staged(
                                &format!("{op}_direction_{direction}_fill_{fill}_{dest}"),
                                &init,
                                "",
                                &line("D0", "D10", dest, "K3"),
                                &[(1097, direction), (1096, fill), (1095, true)],
                            ));
                        }
                    }
                }
            }
        }
    }
    for family in ["MCMP", "MBRD", "MBWR", "MBC"] {
        for pulse in [false, true] {
            let op = format!("{family}{}", if pulse { "P" } else { "" });
            let line = |src: &str, n: &str, pointer: &str| {
                if family == "MCMP" {
                    format!("{op} {src} D10 {n} {pointer}")
                } else {
                    format!("{op} {src} {n} {pointer}")
                }
            };
            for pointer in [
                "D500", "T40", "C40", "K1M600", "K2Y200", "K3S100", "K4M640", "E0", "F0", "E1",
                "F1", "E7", "F7",
            ] {
                let init = seed("D0", &left) + &seed("D10", &right) + &q_seed(pointer, 0);
                out.push(staged(
                    &format!("{op}_scalar_{pointer}"),
                    &init,
                    "",
                    &line("D0", "K3", pointer),
                    &[(1093, true), (1096, true), (1098, true)],
                ));
            }
            for value in [-1, 0, 1, 15, 16, 46, 47, 48, i16::MAX] {
                for reset in [false, true] {
                    let init = seed("D0", &left) + &seed("D10", &right) + &seed("D500", &[value]);
                    let settings = [
                        (1088, reset),
                        (1089, true),
                        (1090, reset),
                        (1091, true),
                        (1092, true),
                        (1093, true),
                        (1094, reset),
                        (1095, true),
                        (1096, true),
                        (1098, reset),
                        (1099, true),
                    ];
                    out.push(staged(
                        &format!("{op}_pointer_{value}_clear_{reset}"),
                        &init,
                        "",
                        &line("D0", "K3", "D500"),
                        &settings,
                    ));
                }
            }
            for digits in 1..=4 {
                let group = format!("K{digits}M64");
                // MBWR has a physical48-bit write window even when digits<4.
                // M64..111 fits in all cases; no systemcontrol/input forcing.
                let init = seed(&group, &left) + &seed("D10", &right) + &seed("D500", &[15]);
                out.push(staged(
                    &format!("{op}_packed_padding_{digits}"),
                    &init,
                    "",
                    &line(&group, "K3", "D500"),
                    &[(1093, true), (1096, true), (1098, true)],
                ));
            }
            for pointer in ["D0", "D1", "D2"] {
                let init = seed("D0", &[0, 1, 2]) + &seed("D10", &right);
                out.push(staged(
                    &format!("{op}_raw_pointer_alias_{pointer}"),
                    &init,
                    "",
                    &line("D0", "K3", pointer),
                    &[(1093, true), (1096, true), (1098, true)],
                ));
            }
            for table in ["T10", "C10", "D0"] {
                let init = seed(table, &left) + &seed("D10", &right) + &seed("D500", &[0]);
                out.push(staged(
                    &format!("{op}_area_{table}"),
                    &init,
                    "",
                    &line(table, "K3", "D500"),
                    &[(1098, true)],
                ));
            }
            let init =
                seed("D0", &left) + &seed("D10", &right) + &seed("D50", &[3]) + &seed("D500", &[0]);
            out.push(staged(
                &format!("{op}_dynamic_count"),
                &init,
                &seed("D50", &[1]),
                &line("D0", "D50", "D500"),
                &[(1093, true), (1098, true)],
            ));
            let init = "FMOV HAAAA D2000 K256\nFMOV H5555 D2400 K256\nMOV K4095 D500\n";
            let body = if family == "MCMP" {
                format!("{op} D2000 D2400 H100 D500")
            } else {
                line("D2000", "H100", "D500")
            };
            out.push(staged(
                &format!("{op}_n256_last_bit"),
                init,
                "",
                &body,
                &[(1093, true), (1096, true), (1098, true)],
            ));
            let init = seed("D0", &[0]) + &seed("D10", &[0]) + &seed("D500", &[0]);
            out.push(staged(
                &format!("{op}_n1_all_zero"),
                &init,
                "",
                &line("D0", "H1", "D500"),
                &[(1098, true), (1099, true)],
            ));
            if family != "MBWR" {
                out.push(source_x(
                    staged(
                        &format!("{op}_packed_X"),
                        &(seed("D10", &right) + &seed("D500", &[0])),
                        "",
                        &line("K2X100", "K3", "D500"),
                        &[(1093, true), (1098, true)],
                    ),
                    "K2X100",
                    &[0x37, 0x68, 0x45],
                ));
            }
            if family == "MBC" {
                let init = seed("D0", &[0]) + &q_seed("K1M600", 7);
                out.push(staged(
                    &format!("{op}_count16_truncated_scalar_zero"),
                    &init,
                    "",
                    &line("D0", "K1", "K1M600"),
                    &[(1099, true)],
                ));
                for (value, ones) in [(0i16, false), (-1, false), (-1, true)] {
                    let init = seed("D0", &[value]) + &seed("D500", &[77]);
                    out.push(staged(
                        &format!("{op}_homogeneous_{value}_ones_{ones}"),
                        &init,
                        "",
                        &line("D0", "K1", "D500"),
                        &[(1098, ones), (1099, true)],
                    ));
                }
            } else {
                for value in [-1, 48] {
                    let init = seed("D0", &left) + &seed("D10", &right) + &seed("D500", &[value]);
                    out.push(staged(
                        &format!("{op}_new_pointer_error_{value}"),
                        &init,
                        "",
                        &line("D0", "K3", "D500"),
                        &[(1089, true), (1091, true), (1095, true)],
                    ));
                }
                for clear in [false, true] {
                    let init = seed("D0", &left) + &seed("D10", &right) + &seed("D500", &[15]);
                    out.push(staged(
                        &format!("{op}_clear_{clear}_without_increment"),
                        &init,
                        "",
                        &line("D0", "K3", "D500"),
                        &[(1090, clear), (1094, clear), (1096, true)],
                    ));
                }
            }
            if family == "MCMP" {
                for equal in [false, true] {
                    let init = seed("D0", &[0])
                        + &seed("D10", &[if equal { 0x7fff } else { i16::MIN }])
                        + &seed("D500", &[0]);
                    out.push(staged(
                        &format!("{op}_last_match_equal_{equal}"),
                        &init,
                        "",
                        &line("D0", "K1", "D500"),
                        &[(1088, equal)],
                    ));
                }
            }
            if matches!(family, "MBRD" | "MBWR") {
                let init = seed("D0", &left);
                out.push(staged(
                    &format!("{op}_pointer_shared_controls"),
                    &init,
                    "",
                    &line("D0", "K3", "K2M1092"),
                    &[(1093, true), (1095, true), (1096, true)],
                ));
            }
        }
    }
    for (name, body) in [
        ("compare_then_read", "MCMP D0 D10 K3 D500\nMBRD D0 K3 D500"),
        ("write_then_read", "MBWR D0 K3 D500\nMBRD D0 K3 D500"),
        ("read_then_write", "MBRD D0 K3 D500\nMBWR D0 K3 D500"),
        ("shift_then_rotate", "MBS D0 D20 K3\nMBR D20 D30 K3"),
    ] {
        let init = seed("D0", &left) + &seed("D10", &right) + &seed("D500", &[0]);
        out.push(staged(
            &format!("shared_{name}"),
            &init,
            "",
            body,
            &[(1093, true), (1096, true), (1098, true)],
        ));
    }
    out.extend(scratch_cases());
    out
}

fn scratch_cases() -> Vec<Case> {
    let mut out = Vec::new();
    for (label, predecessor, extra) in [
        ("MAND_raw", "MAND D200 D210 D220 K1", ""),
        ("MAND_group_dest_offset3", "MAND D200 D210 K2M259 K1", ""),
        ("MINV_raw", "MINV D200 D220 K1", ""),
        ("MINV_group_dest_offset1", "MINV D200 K2M257 K1", ""),
        (
            "MCMP_invalid_pointer",
            "MCMP D200 D210 K1 D500",
            "MOV K-1 D500\n",
        ),
        ("MBRD_pointer0", "MBRD D200 K1 D500", "MOV K0 D500\n"),
        ("MBWR_pointer3", "MBWR D200 K1 D500", "MOV K3 D500\n"),
        ("MBS_left", "MBS D200 D220 K1", "RST M1097\n"),
        (
            "MBS_right_insert0",
            "MBS D200 D220 K1",
            "SET M1097\nRST M1096\n",
        ),
        (
            "MBS_right_insert1",
            "MBS D200 D220 K1",
            "SET M1097\nSET M1096\n",
        ),
        ("MBR_right", "MBR D200 D220 K1", "SET M1097\n"),
        ("MBC_hold", "MBC D200 K1 D500", ""),
    ] {
        let init = seed("D0", &[30, 10, 20, 3, 1, 2])
            + &seed("D100", &[-777; 6])
            + &seed("D200", &[1])
            + &seed("D210", &[3])
            + &seed("D700", &[7]);
        let prior = match label {
            "MINV_raw" | "MINV_group_dest_offset1" | "MBS_left" | "MBC_hold" => 3,
            "MBS_right_insert0" => -253,
            _ => -256,
        };
        let source = format!(
            "LD X1\n{init}{}LD X0\nSER D700 K{prior} D720 K1\n{extra}{predecessor}\nSORT D0 K3 K2 D100 K2\nMOV K123 D900\nEND",
            flags(&[])
        );
        out.push(case(
            &format!("matrix_scratch_{label}"),
            &source,
            vec![
                (vec![("X1", true), ("X0", false)], 1),
                (vec![("X1", false), ("X0", true)], 6),
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 4),
            ],
        ));
    }
    out
}
