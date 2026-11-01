//! DRAFT: six addressed text forms. Independent literal output/guard oracles.
//! Existing ordinary group support is not counted as a new algorithm.
use super::{Case, Target, case};
use std::collections::BTreeSet;

const FORMS: [&str; 6] = ["ASCI", "ASCIP", "HEX", "HEXP", "CCD", "CCDP"];
const CHECK_BYTES: &[u8] = &[100, 111, 120, 202, 123, 211];
fn op(f: &str, pulse: bool) -> &'static str {
    match (f, pulse) {
        ("ASCI", false) => "ASCI",
        ("ASCI", true) => "ASCIP",
        ("HEX", false) => "HEX",
        ("HEX", true) => "HEXP",
        ("CCD", false) => "CCD",
        ("CCD", true) => "CCDP",
        _ => unreachable!(),
    }
}
fn raw(area: &str, index: u16) -> String {
    if matches!(area, "X" | "Y") {
        format!("{area}{index:o}")
    } else {
        format!("{area}{index}")
    }
}
fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (
            vec![("X1", true), ("X0", false), ("X2", false), ("X7", false)],
            1,
        ),
        (vec![("X1", false), ("X0", true), ("X2", true)], 2),
        (vec![("X0", false), ("X2", false)], 1),
        (vec![("X0", true)], 1),
    ]
}
fn words(input: &[u8], low: bool) -> Vec<u16> {
    if low {
        input.iter().map(|b| 0xa500 | u16::from(*b)).collect()
    } else {
        input
            .chunks(2)
            .map(|p| u16::from(p[0]) | (u16::from(*p.get(1).unwrap_or(&0x5a)) << 8))
            .collect()
    }
}
fn input(f: &str, low: bool) -> Vec<u16> {
    match f {
        "ASCI" => vec![0x0123],
        "HEX" => words(b"123A", low),
        "CCD" => words(CHECK_BYTES, low),
        _ => unreachable!(),
    }
}
fn output(f: &str, low: bool) -> Vec<u16> {
    match (f, low) {
        ("ASCI", false) => vec![0x3130, 0x3332],
        ("ASCI", true) => vec![0x30, 0x31, 0x32, 0x33],
        ("HEX", _) => vec![0x123a],
        ("CCD", _) => vec![867, 0x11],
        _ => unreachable!(),
    }
}
fn n(f: &str) -> u16 {
    if f == "CCD" { 6 } else { 4 }
}
fn seed(area: &str, base: u16, group: bool, values: &[u16]) -> String {
    if area == "X" {
        return String::new();
    }
    values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            format!(
                "MOV H{v:04X} {}{}",
                if group { "K4" } else { "" },
                raw(area, base + i as u16 * if group { 16 } else { 1 })
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn observed(area: &str, base: u16, group: bool, values: &[u16]) -> Vec<(String, u16, u16)> {
    values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            (
                format!(
                    "{}{}",
                    if group { "K4" } else { "" },
                    raw(area, base + i as u16 * if group { 16 } else { 1 })
                ),
                0xffff,
                *v,
            )
        })
        .collect()
}
fn guarded_seed(area: &str, base: u16, group: bool, count: usize) -> (String, String) {
    let end = base + count as u16 * if group { 16 } else { 1 };
    if group {
        (
            format!(
                "SET {}\nSET {}\n{}",
                raw(area, base - 1),
                raw(area, end),
                seed(area, base, true, &vec![0xffff; count])
            ),
            format!("AND {}\nAND {}", raw(area, base - 1), raw(area, end)),
        )
    } else {
        (
            format!(
                "MOV H1357 {}\nMOV H2468 {}\n{}",
                raw(area, base - 1),
                raw(area, end),
                seed(area, base, false, &vec![0xffff; count])
            ),
            format!(
                "AND= {} H1357\nAND= {} H2468",
                raw(area, base - 1),
                raw(area, end)
            ),
        )
    }
}
fn prefix_words(s: &str) -> u16 {
    s.lines()
        .filter(|l| !l.is_empty())
        .map(|l| match l.split_whitespace().next().unwrap() {
            "LD" | "LDI" | "SET" => 1,
            "RST" | "INC" => 3,
            "MOV" | "LD=" | "OR=" | "AND=" => 5,
            "FMOV" => 7,
            "DMOV" => 9,
            other => panic!("unaccounted fixture prefix {other}"),
        })
        .sum()
}
struct Fixture<'a> {
    name: &'a str,
    op: &'a str,
    low: bool,
    setup: String,
    args: String,
    values: Vec<(String, u16, u16)>,
    observe_prefix: String,
    guard: String,
    error: Option<u16>,
    active_override: Option<String>,
    body_prefix: String,
    body_suffix: String,
}
fn normal<'a>(
    name: &'a str,
    op: &'a str,
    low: bool,
    setup: String,
    args: String,
    values: Vec<(String, u16, u16)>,
    guard: String,
) -> Fixture<'a> {
    Fixture {
        name,
        op,
        low,
        setup,
        args,
        values,
        observe_prefix: String::new(),
        guard,
        error: None,
        active_override: None,
        body_prefix: String::new(),
        body_suffix: String::new(),
    }
}
fn fixture(f: Fixture<'_>) -> Case {
    assert!(!f.values.is_empty() && f.values.len() <= 12);
    let mut prefix = "LD X1\nMOV K0 D310\nLDI X7\nINC D310".to_owned();
    for b in ['E', 'F'] {
        for i in 0..8 {
            prefix.push_str(&format!("\nMOV K0 {b}{i}"));
        }
    }
    prefix.push_str(&format!("\n{} M1161\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nRST M832\nRST M833\nMOV H1234 D1067\nMOV K77 D1068\n{}\nLDI X7",if f.low{"SET"}else{"RST"},f.setup));
    for (i, (_, before, _)) in f.values.iter().enumerate() {
        prefix.push_str(&format!("\nMOV H{before:04X} D{}", 320 + i));
    }
    prefix.push_str("\nMOV H1234 D340\nMOV K77 D341\nMOV K0 D342");
    let header_prefix = if f.body_prefix.is_empty() {
        "LD X0"
    } else {
        &f.body_prefix
    };
    // MC fixtures provide an explicit offset below; main prefix never uses
    // runtime encoder words or derived simulator output to calculate an oracle.
    let header = prefix_words(&prefix)
        + if f.body_prefix.is_empty() {
            1
        } else {
            f.body_prefix
                .lines()
                .map(|l| match l.split_whitespace().next().unwrap() {
                    "LD" | "LDI" => 1,
                    "MC" => 3,
                    _ => panic!("fixture gate"),
                })
                .sum::<u16>()
        };
    let gate = f.active_override.unwrap_or_else(|| {
        if f.op.ends_with('P') {
            "LD= D310 K2\nOR= D310 K5".into()
        } else {
            "LD X0".into()
        }
    });
    let mut expected = gate;
    for (i, (_, _, after)) in f.values.iter().enumerate() {
        expected.push_str(&format!("\nMOV H{after:04X} D{}", 320 + i));
    }
    if let Some(offset) = f.error {
        assert!([1, 3, 5, 7].contains(&offset));
        expected.push_str(&format!(
            "\nMOV HE1A D340\nMOV K{} D341\nMOV K3 D342",
            header + offset
        ));
    }
    let mut observations = format!("LDI X7\nMOV K1234 D306\n{}", f.observe_prefix);
    for (i, (value, _, _)) in f.values.iter().enumerate() {
        observations.push_str(&format!("\nMOV {value} D{}", 240 + i));
    }
    let mut checks = Vec::new();
    for slot in 0..6usize {
        let members = (slot * 2..(slot * 2 + 2).min(f.values.len())).collect::<Vec<_>>();
        let test = if members.is_empty() {
            "LDI X7".into()
        } else {
            members
                .iter()
                .enumerate()
                .map(|(j, i)| {
                    format!(
                        "{}= D{} D{}",
                        if j == 0 { "LD" } else { "AND" },
                        240 + i,
                        320 + i
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        checks.push(format!("{test}\nOUT M{}", 840 + slot));
    }
    checks.push("LD= D1067 D340\nAND= D1068 D341\nAND= K1M830 D342\nAND M1020\nANI M1021\nAND M1022\nOUT M846".into());
    checks.push(format!("LD= D306 K1234\n{}\nOUT M847", f.guard));
    let s = format!(
        "{prefix}\n{header_prefix}\n{} {}\n{}\n{expected}\n{observations}\nLD M1067\nOUT M830\nLD M1068\nOUT M831\n{}\nLDI X7\nMOV K2M840 D308\nEND",
        f.op,
        f.args,
        f.body_suffix,
        checks.join("\n")
    );
    let s = s
        .lines()
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(s.lines().count() < 220, "{} lines", f.name);
    case(
        &format!("indexed_text_literal_oracle_{}", f.name),
        &s,
        stages(),
    )
}
fn add_inputs(mut c: Case, base: u16, values: &[u16]) -> Case {
    for (i, v) in values.iter().enumerate() {
        for bit in 0..16 {
            let d: &'static str = Box::leak(raw("X", base + i as u16 * 16 + bit).into_boxed_str());
            c.stages[0].0.push((d, v & (1 << bit) != 0));
        }
    }
    c
}
fn checked(cases: Vec<Case>, size: usize) -> Vec<Case> {
    assert_eq!(cases.len(), size);
    assert_eq!(
        cases.iter().map(|c| &c.name).collect::<BTreeSet<_>>().len(),
        size
    );
    for form in FORMS {
        assert!(cases.iter().any(|c| {
            c.source
                .lines()
                .any(|l| l.split_whitespace().next() == Some(form))
        }));
    }
    for c in &cases {
        for line in c.source.lines() {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields
                .first()
                .is_some_and(|name| FORMS.contains(name) || ["ASCII", "ASCIIP"].contains(name))
            {
                assert_eq!(fields.len(), 4, "{}: text field count", c.name);
                let count = fields[3];
                if ["CCD", "CCDP"].contains(&fields[0]) {
                    assert!(
                        count.starts_with(['K', 'H'])
                            || count.strip_prefix('D').is_some_and(
                                |s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
                            ),
                        "{}: CCD count is white, never addressed",
                        c.name
                    );
                } else {
                    assert!(
                        count.starts_with(['K', 'H']),
                        "{}: literal count required",
                        c.name
                    );
                }
            }
        }
    }
    cases
}
pub(super) fn common_cases() -> Vec<Case> {
    let mut out = Vec::new();
    banks(&mut out); //96
    groups(&mut out); //48
    x_sources(&mut out); //6
    numeric_areas(&mut out); //12
    constants(&mut out); //8
    text_details(&mut out); //9
    overlaps(&mut out); //3
    counters(&mut out); //10
    cached_offsets(&mut out); //6
    checked_source_ends(&mut out); //3
    master_gates(&mut out); //3
    scratch_witnesses(&mut out); //6 parity-only public SORT witnesses
    out.extend(numeric_end_cases("T", 255)); //6 complete/read and checked partial-write endpoints
    invalid_ccd_counts(&mut out); //2 plain-D domain failures skip late target
    checked(out, 218)
}
fn banks(out: &mut Vec<Case>) {
    for f in ["ASCI", "HEX", "CCD"] {
        for b in ['E', 'F'] {
            for i in 0..8u16 {
                for low in [false, true] {
                    let op = op(f, (i + u16::from(low)) % 2 != 0);
                    let offset = if i % 2 == 0 { -1i16 } else { 1 };
                    let expected = output(f, low);
                    let (ds, guard) = guarded_seed("D", 200, false, expected.len());
                    let name = format!("banks_{op}_{b}{i}_{}", if low { "low" } else { "packed" });
                    let alias = (if i == 0 { 1028 } else { 1180 + 2 * i }) + u16::from(b == 'F');
                    let setup = format!(
                        "MOV K{offset} {b}{i}\nMOV H1234 D{}\n{}\n{}\nMOV K{} D40",
                        alias + 1,
                        seed("D", 100, false, &input(f, low)),
                        ds,
                        n(f)
                    );
                    let args = format!(
                        "D{}{b}{i} D{}{b}{i} {}",
                        100i16 - offset,
                        200i16 - offset,
                        if f == "CCD" {
                            "D40".into()
                        } else {
                            format!("K{}", n(f))
                        }
                    );
                    out.push(fixture(normal(
                        &name,
                        op,
                        low,
                        setup,
                        args,
                        observed("D", 200, false, &expected),
                        guard,
                    )));
                }
            }
        }
    }
}
fn groups(out: &mut Vec<Case>) {
    for f in ["ASCI", "HEX", "CCD"] {
        for source_group in [true, false] {
            for digits in 1..=4usize {
                for low in [false, true] {
                    let op = op(
                        f,
                        (digits + usize::from(low) + usize::from(source_group)) % 2 != 0,
                    );
                    let mut expected = output(f, low);
                    let mut error = None;
                    if source_group {
                        match f {
                            "ASCI" => {
                                expected = match (digits, low) {
                                    (1, false) => vec![0x3030, 0x3330],
                                    (1, true) => vec![0x30, 0x30, 0x30, 0x33],
                                    (2, false) => vec![0x3030, 0x3332],
                                    (2, true) => vec![0x30, 0x30, 0x32, 0x33],
                                    _ => expected,
                                }
                            }
                            "HEX" => {
                                if digits == 1 {
                                    expected = vec![0xffff];
                                    error = Some(7);
                                } else if !low && digits < 4 {
                                    expected = vec![0x10ff];
                                    error = Some(7);
                                }
                            }
                            "CCD" => {
                                expected = if low {
                                    match digits {
                                        1 => vec![51, 1],
                                        _ => vec![867, 17],
                                    }
                                } else {
                                    match digits {
                                        1 => vec![23, 7],
                                        2 => vec![343, 103],
                                        3 => vec![371, 97],
                                        _ => vec![867, 17],
                                    }
                                }
                            }
                            _ => unreachable!(),
                        }
                    } else {
                        match f {
                            "ASCI" => {
                                expected = if low {
                                    if digits == 1 {
                                        vec![0, 1, 2, 3]
                                    } else {
                                        vec![0x30, 0x31, 0x32, 0x33]
                                    }
                                } else {
                                    match digits {
                                        1 => vec![0, 2],
                                        2 => vec![0x30, 0x32],
                                        3 => vec![0x130, 0x332],
                                        _ => vec![0x3130, 0x3332],
                                    }
                                }
                            }
                            "HEX" => expected = vec![[0xa, 0x3a, 0x23a, 0x123a][digits - 1]],
                            "CCD" => {
                                expected = vec![
                                    [3, 0x63, 0x363, 0x363][digits - 1],
                                    [1, 0x11, 0x11, 0x11][digits - 1],
                                ]
                            }
                            _ => unreachable!(),
                        }
                    }
                    let sa = if source_group {
                        ["M", "S", "Y", "M"][digits - 1]
                    } else {
                        "D"
                    };
                    let da = if source_group {
                        "D"
                    } else if f == "CCD" {
                        ["M", "S", "M", "S"][digits - 1]
                    } else {
                        ["Y", "M", "S", "Y"][digits - 1]
                    };
                    let sb = if source_group { 32 } else { 100 };
                    let db = if source_group { 200 } else { 128 };
                    let (src, dst) = (
                        if source_group {
                            format!("K{digits}{}E1", raw(sa, sb - 1))
                        } else {
                            "D99E1".into()
                        },
                        if source_group {
                            "D199F1".into()
                        } else {
                            format!("K{digits}{}F1", raw(da, db - 1))
                        },
                    );
                    let (ds, guard) = guarded_seed(da, db, !source_group, expected.len());
                    let setup = format!(
                        "MOV K1 E1\nMOV K1 F1\n{}\n{ds}",
                        seed(sa, sb, source_group, &input(f, low))
                    );
                    let name = format!(
                        "group_{op}_{}_K{digits}_{sa}_{da}_{}",
                        if source_group { "source" } else { "target" },
                        if low { "low" } else { "packed" }
                    );
                    let mut c = normal(
                        &name,
                        op,
                        low,
                        setup,
                        format!("{src} {dst} K{}", n(f)),
                        observed(da, db, !source_group, &expected),
                        guard,
                    );
                    c.error = error;
                    out.push(fixture(c));
                }
            }
        }
    }
}
fn x_sources(out: &mut Vec<Case>) {
    for f in ["ASCI", "HEX", "CCD"] {
        for low in [false, true] {
            let op = op(f, low);
            let expected = output(f, low);
            let (ds, guard) = guarded_seed("M", 128, true, expected.len());
            let name = format!("X_source_{op}_{low}");
            let c = fixture(normal(
                &name,
                op,
                low,
                format!("MOV K1 E1\n{ds}"),
                format!("K4X37E1 K4M128F0 K{}", n(f)),
                observed("M", 128, true, &expected),
                guard,
            ));
            out.push(add_inputs(c, 32, &input(f, low)));
        }
    }
}
fn numeric_areas(out: &mut Vec<Case>) {
    for f in ["ASCI", "HEX", "CCD"] {
        for (i, (sa, da)) in [("T", "T"), ("T", "C"), ("C", "T"), ("C", "C")]
            .into_iter()
            .enumerate()
        {
            let low = i % 2 != 0;
            let op = op(f, low);
            let expected = output(f, low);
            let (ds, guard) = guarded_seed(da, 80, false, expected.len());
            let name = format!("numeric_{op}_{sa}_{da}_{low}");
            out.push(fixture(normal(
                &name,
                op,
                low,
                format!(
                    "MOV K1 E2\nMOV K-1 F2\n{}\n{ds}",
                    seed(sa, 20, false, &input(f, low))
                ),
                format!("{sa}19E2 {da}81F2 K{}", n(f)),
                observed(da, 80, false, &expected),
                guard,
            )));
        }
    }
}
fn constants(out: &mut Vec<Case>) {
    for (i, op) in ["ASCI", "ASCIP", "ASCII", "ASCIIP"].into_iter().enumerate() {
        let low = i % 2 != 0;
        let expected = if low {
            vec![0x31, 0x32, 0x33, 0x34, 0x31, 0x32, 0x33, 0x34]
        } else {
            vec![0x3231, 0x3433, 0x3231, 0x3433]
        };
        let (ds, guard) = guarded_seed("D", 200, false, expected.len());
        let name = format!("constant_repeat_{op}_{low}");
        out.push(fixture(normal(
            &name,
            op,
            low,
            ds,
            format!("{} D200F7 K8", if i % 2 == 0 { "H1234" } else { "K4660" }),
            observed("D", 200, false, &expected),
            guard,
        )));
    }
    for op in ["HEX", "HEXP"] {
        for low in [false, true] {
            let (ds, guard) = guarded_seed("D", 200, false, 1);
            let name = format!("constant_repeat_{op}_{low}");
            out.push(fixture(normal(
                &name,
                op,
                low,
                ds,
                format!("{} D200E7 H4", if low { "K12593" } else { "H3131" }),
                observed("D", 200, false, &[0x1111]),
                guard,
            )));
        }
    }
}
fn text_details(out: &mut Vec<Case>) {
    for digits in 1..=3 {
        let op = if digits % 2 == 0 { "ASCIIP" } else { "ASCII" };
        let (ds, guard) = guarded_seed("M", 128, true, 1);
        let name = format!("odd_tail_{op}_K{digits}");
        let expected = [0xff03, 0xff33, 0xf033][digits - 1];
        out.push(fixture(normal(
            &name,
            op,
            false,
            ds,
            format!("H123 K{digits}M128E0 K1"),
            observed("M", 128, true, &[expected]),
            guard,
        )));
    }
    for low in [false, true] {
        for group in [false, true] {
            let op = if low { "HEXP" } else { "HEX" };
            let (sa, sb) = if group { ("M", 32) } else { ("D", 100) };
            let (ds, guard) = guarded_seed("D", 200, false, 1);
            let name = format!("ascii_high_bit_{op}_group{group}");
            let mut c = normal(
                &name,
                op,
                low,
                format!(
                    "{}\n{ds}",
                    seed(sa, sb, group, &words(&[0xc1, 0xc2, 0xc3], low))
                ),
                format!("{} D200F0 K3", if group { "K4M32E0" } else { "D100E0" }),
                observed("D", 200, false, &[if group { 0xabc } else { 0xffff }]),
                guard,
            );
            if !group {
                c.error = Some(7);
            }
            out.push(fixture(c));
        }
    }
    for group in [false, true] {
        let (da, db) = if group { ("S", 128) } else { ("D", 200) };
        let (ds, guard) = guarded_seed(da, db, group, 1);
        let name = format!("HEX_partial_invalid_group{group}");
        let mut c = normal(
            &name,
            "HEX",
            false,
            format!("{}\n{ds}", seed("D", 100, false, &words(b"A!23", false))),
            format!("D100E0 {} K4", if group { "K4S128F0" } else { "D200F0" }),
            observed(da, db, group, &[if group { 0xafff } else { 0xa0ff }]),
            guard,
        );
        c.error = Some(7);
        out.push(fixture(c));
    }
}
fn overlaps(out: &mut Vec<Case>) {
    let mut c = normal(
        "ASCI_live_nibble_overlap",
        "ASCI",
        false,
        "MOV H0123 D100\nMOV H4567 D101\nMOV HFFFF D102\nMOV HFFFF D103\nMOV HFFFF D104".into(),
        "D100E0 D101F0 K8".into(),
        vec![
            ("D101".into(), 0x4567, 0x3533),
            ("D102".into(), 0xffff, 0x3736),
            ("D103".into(), 0xffff, 0x3130),
            ("D104".into(), 0xffff, 0x3332),
        ],
        "AND= D100 H0123".into(),
    );
    out.push(fixture(c));
    c = normal(
        "HEX_live_second_word_invalid",
        "HEX",
        false,
        "MOV H4443 D100\nMOV H4443 D101\nMOV H4443 D102\nMOV H4443 D103".into(),
        "D100E0 D100F0 K8".into(),
        vec![
            ("D100".into(), 0x4443, 0x4443),
            ("D101".into(), 0x4443, 0xcdc0),
        ],
        "AND= D102 H4443\nAND= D103 H4443".into(),
    );
    c.error = Some(7);
    out.push(fixture(c));
    out.push(fixture(normal(
        "CCD_all_payload_before_overlap",
        "CCD",
        false,
        seed("D", 100, false, &input("CCD", false)),
        "D100E0 D101F0 K6".into(),
        vec![("D101".into(), 0xca78, 867), ("D102".into(), 0xd37b, 17)],
        "AND= D100 H6F64".into(),
    )));
}
fn counters(out: &mut Vec<Case>) {
    let (ds, g) = guarded_seed("D", 200, false, 4);
    out.push(fixture(normal(
        "ASCI_counter_source_physical_C199_C200low",
        "ASCI",
        false,
        format!("MOV H0123 C199\nDMOV H89AB4567 C200\n{ds}"),
        "C199E0 D200F0 K8".into(),
        observed("D", 200, false, &[0x3534, 0x3736, 0x3130, 0x3332]),
        g,
    )));
    let (ds, g) = guarded_seed("D", 200, false, 2);
    out.push(fixture(normal(
        "HEX_counter_source_physical_C200high",
        "HEXP",
        false,
        format!("MOV H3231 C199\nDMOV H36353433 C200\n{ds}"),
        "C199E0 D200F0 K6".into(),
        observed("D", 200, false, &[0x3456, 0x12]),
        g,
    )));
    for f in ["ASCI", "HEX"] {
        let op = if f == "ASCI" { "ASCIP" } else { "HEX" };
        let name = format!("{op}_counter_result_physical_C199_C200_C201");
        let source = if f == "ASCI" {
            String::new()
        } else {
            seed("D", 100, false, &words(b"123456789ABC", false))
        };
        let after = if f == "ASCI" {
            [0x3130, 0x3332, 0x3130, 0x3332, 0x5555]
        } else {
            [0x9abc, 0x5678, 0x1234, 0xcccc, 0x5555]
        };
        let mut c = normal(
            &name,
            op,
            false,
            format!("{source}\nMOV HFFFF C199\nDMOV HAAAABBBB C200\nDMOV H5555CCCC C201"),
            if f == "ASCI" {
                "H123 C199F0 K8".into()
            } else {
                "D100E0 C199F0 K12".into()
            },
            ["C199", "D230", "D231", "D232", "D233"]
                .into_iter()
                .zip([0xffff, 0xbbbb, 0xaaaa, 0xcccc, 0x5555])
                .zip(after)
                .map(|((s, b), a)| (s.into(), b, a))
                .collect(),
            String::new(),
        );
        c.observe_prefix = "DMOV C200 D230\nDMOV C201 D232".into();
        out.push(fixture(c));
    }
    for count in [2, 3] {
        let (ds, g) = guarded_seed("D", 200, false, 2);
        let name = format!("CCD_C199_source_count{count}");
        let mut c = normal(
            &name,
            "CCD",
            false,
            format!("MOV H0102 C199\nDMOV H11223344 C200\n{ds}"),
            format!("C199E0 D200F0 K{count}"),
            observed(
                "D",
                200,
                false,
                if count == 2 {
                    &[3, 3]
                } else {
                    &[0xffff, 0xffff]
                },
            ),
            g,
        );
        if count == 3 {
            c.error = Some(5);
        }
        out.push(fixture(c));
    }
    for source_natural in [true, false] {
        let name = format!(
            "CCDP_effective_natural_counter_{}",
            if source_natural { "source" } else { "target" }
        );
        let setup = format!(
            "MOV K1 E1\nMOV K1 F1\nMOV K0 D40\nDMOV H11223344 C200\n{}\n{}",
            seed("D", 100, false, &input("CCD", false)),
            seed("D", 200, false, &[0xffff, 0xffff])
        );
        let mut c = normal(
            &name,
            "CCDP",
            false,
            setup,
            if source_natural {
                "C199E1 D200F0 D40".into()
            } else {
                "D100E0 C199F1 K6".into()
            },
            if source_natural {
                observed("D", 200, false, &[0xffff, 0xffff])
            } else {
                vec![
                    ("D230".into(), 0x3344, 0x3344),
                    ("D231".into(), 0x1122, 0x1122),
                ]
            },
            String::new(),
        );
        if !source_natural {
            c.observe_prefix = "DMOV C200 D230".into();
        }
        c.error = Some(if source_natural { 1 } else { 7 });
        out.push(fixture(c));
    }
    for op in ["CCD", "CCDP"] {
        let name = format!("{op}_C199_sum_only_target");
        let mut c = normal(
            &name,
            op,
            false,
            format!(
                "{}\nMOV HFFFF C199\nDMOV H11223344 C200",
                seed("D", 100, false, &input("CCD", false))
            ),
            "D100E0 C199F0 K6".into(),
            vec![
                ("C199".into(), 0xffff, 867),
                ("D230".into(), 0x3344, 0x3344),
                ("D231".into(), 0x1122, 0x1122),
            ],
            String::new(),
        );
        c.observe_prefix = "DMOV C200 D230".into();
        c.error = Some(3);
        out.push(fixture(c));
    }
}
fn cached_offsets(out: &mut Vec<Case>) {
    for f in ["ASCI", "HEX", "CCD"] {
        for low in [false, true] {
            let op = op(f, low);
            let name = format!("own_target_modifier_{op}_{low}");
            let expected = output(f, low);
            let mut values = observed("D", 1182, false, &expected);
            values[0].1 = 0;
            let setup = format!(
                "{}\n{}\nMOV K0 E1\nMOV H2468 D{}",
                seed("D", 100, false, &input(f, low)),
                seed("D", 1182, false, &vec![0xffff; expected.len()]),
                1182 + expected.len()
            );
            out.push(fixture(normal(
                &name,
                op,
                low,
                setup,
                format!("D100E0 D1182E1 K{}", n(f)),
                values,
                format!("AND= D{} H2468", 1182 + expected.len()),
            )));
        }
    }
}
fn checked_source_ends(out: &mut Vec<Case>) {
    let mut c = normal(
        "ASCI_final_nibble_rollover_before_last_character",
        "ASCI",
        false,
        "MOV K1 E1\nMOV H3 K1S1020\nMOV HFFFF D200".into(),
        "K1S1019E1 D200F0 K1".into(),
        observed("D", 200, false, &[0xffff]),
        String::new(),
    );
    c.error = Some(7);
    out.push(fixture(c));
    for low in [false, true] {
        let sb = if low { 1008 } else { 1016 };
        let width = if low { 4 } else { 2 };
        let name = format!("CCD_final_byte_no_extra_advance_low{low}");
        let setup = format!("MOV K1 E1\nMOV H5A K{width}S{sb}\nMOV HFFFF D200\nMOV HFFFF D201");
        out.push(fixture(normal(
            &name,
            "CCDP",
            low,
            setup,
            format!("K{width}S{}E1 D200F0 K1", sb - 1),
            observed("D", 200, false, &[0x5a, 0x5a]),
            String::new(),
        )));
    }
}
fn master_gates(out: &mut Vec<Case>) {
    for f in ["ASCI", "HEX", "CCD"] {
        let op = op(f, true);
        let name = format!("{op}_master_false_then_true_held_logic");
        let expected = output(f, false);
        let (ds, g) = guarded_seed("D", 200, false, expected.len());
        let mut c = normal(
            &name,
            op,
            false,
            format!("{}\n{ds}", seed("D", 100, false, &input(f, false))),
            format!("D100E0 D200F0 K{}", n(f)),
            observed("D", 200, false, &expected),
            g,
        );
        c.body_prefix = "LD X2\nMC N0\nLD X0".into();
        c.body_suffix = "MCR N0".into();
        c.active_override = Some("LD= D310 K3\nOR= D310 K5".into());
        let mut case = fixture(c);
        case.stages = vec![
            (
                vec![("X1", true), ("X0", false), ("X2", false), ("X7", false)],
                1,
            ),
            (vec![("X1", false), ("X0", true)], 1),
            (vec![("X2", true)], 1),
            (vec![("X2", false)], 1),
            (vec![("X2", true)], 1),
        ];
        out.push(case);
    }
}
pub(super) fn model_cases(target: Target) -> Vec<Case> {
    let last = match target {
        Target::Es2 => 9999u16,
        Target::Ss2 => 4999u16,
    };
    numeric_end_cases("D", last)
}
fn numeric_end_cases(area: &str, last: u16) -> Vec<Case> {
    let mut out = Vec::new();
    for f in ["ASCI", "HEX", "CCD"] {
        let plain = op(f, false);
        let name = format!("{plain}_{area}{last}_complete_source");
        let (src, nn, after) = match f {
            "ASCI" => (0x0123, 4, vec![0x3130, 0x3332]),
            "HEX" => (0x4241, 2, vec![0xab]),
            _ => (0x0102, 2, vec![3, 3]),
        };
        let (ds, g) = guarded_seed("D", 200, false, after.len());
        out.push(fixture(normal(
            &name,
            plain,
            false,
            format!("MOV H{src:04X} {area}{last}\n{ds}"),
            format!("{area}{last}E0 D200F0 K{nn}"),
            observed("D", 200, false, &after),
            g,
        )));
        let op = op(f, true);
        let name = format!("{op}_{area}{last}_destination");
        let (nn, after) = match f {
            "ASCI" => (1, 0x33),
            "HEX" => (4, 0x123a),
            _ => (6, 867),
        };
        let mut c = normal(
            &name,
            op,
            false,
            format!(
                "{}\nMOV H1357 {area}{}\nMOV HFFFF {area}{last}",
                seed("D", 100, false, &input(f, false)),
                last - 1
            ),
            format!("D100E0 {area}{last}F0 K{nn}"),
            observed(area, last, false, &[after]),
            format!("AND= {area}{} H1357", last - 1),
        );
        if f == "CCD" {
            c.error = Some(3);
        }
        out.push(fixture(c));
    }
    checked(out, 6)
}

fn scratch_witnesses(out: &mut Vec<Case>) {
    for (f, variant) in [
        ("ASCI", "addressed"),
        ("ASCI", "literal"),
        ("HEX", "addressed"),
        ("HEX", "literal"),
        ("CCD", "sum_zero"),
        ("CCD", "sum_one"),
    ] {
        let argument = match (f, variant) {
            ("ASCI", "literal") => "H1234",
            ("HEX", "literal") => "H3131",
            _ => "D20E0",
        };
        let value = match f {
            "ASCI" => 0x1234,
            "HEX" => 0x3131,
            _ => {
                if variant == "sum_one" {
                    1
                } else {
                    0
                }
            }
        };
        let count = if f == "CCD" { 1 } else { 4 };
        // Closed SER/SORT protocol, not a private memory alias. Repeated SER
        // publishes0101h; addressed ASCI/HEX should retain low01/clear high,
        // constants retain high01, and CCD replaces the WORD with sum0/1.
        // Exact per-scan SORT contents remain parity-only; no result generated
        // by production code is promoted into an independent expected value.
        let source = format!(
            "LD X1\nMOV K0 E0\nMOV K0 F0\nMOV K7 D750\nMOV H{value:04X} D20\nMOV K30 D400\nMOV K10 D401\nMOV K20 D402\nMOV K3 D403\nMOV K1 D404\nMOV K2 D405\nMOV H1357 D406\nMOV H2468 D407\nFMOV K-777 D500 K6\nMOV H1357 D499\nMOV H2468 D506\nMOV H3579 D507\nRST M1029\nRST M1161\nRST M1067\nRST M1068\nLDI X7\nSER D750 K257 D760 K1\nLD X0\n{f} {argument} D30F0 K{count}\nSORT D400 K3 K2 D500 K2\nLDI X7\nMOV D500 D220\nMOV D501 D221\nMOV D502 D222\nMOV D503 D223\nMOV D504 D224\nMOV D505 D225\nMOV D406 D232\nMOV D407 D233\nMOV D499 D234\nMOV D506 D235\nMOV D507 D236\nMOV D30 D226\nMOV D31 D227\nMOV D1067 D228\nMOV D1068 D229\nMOV K1234 D306\nLD M1029\nOUT Y10\nEND"
        );
        out.push(case(
            &format!("indexed_text_private_SORT_scratch_{f}_{variant}"),
            &source,
            vec![
                (vec![("X1", true), ("X0", false), ("X7", false)], 1),
                (vec![("X1", false), ("X0", true)], 5),
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 2),
            ],
        ));
    }
}

fn invalid_ccd_counts(out: &mut Vec<Case>) {
    for (op, n, low) in [("CCD", 0, false), ("CCDP", 257, true)] {
        let name = format!("{op}_plain_count_{n}_skips_invalid_late_target");
        let (ds, g) = guarded_seed("D", 200, false, 2);
        let mut c = normal(
            &name,
            op,
            low,
            format!(
                "MOV K{n} D40\nMOV K-1 F1\n{}\n{ds}",
                seed("D", 100, false, &input("CCD", low))
            ),
            "D100E0 D0F1 D40".into(),
            observed("D", 200, false, &[0xffff, 0xffff]),
            g,
        );
        c.error = Some(5);
        out.push(fixture(c));
    }
}
