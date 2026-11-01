//! Eight addressed calendar/time forms. Draft only: no executed native claim.
//! Arithmetic requires the exact CurrentCommgr success-tail profile.
use super::{Case, Target, case};
use std::collections::BTreeSet;

const FORMS: [&str; 8] = [
    "TCMP", "TCMPP", "TZCP", "TZCPP", "TADD", "TADDP", "TSUB", "TSUBP",
];
const ARITH: [&str; 4] = ["TADD", "TADDP", "TSUB", "TSUBP"];

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (
            vec![
                ("X1", true),
                ("X0", false),
                ("X2", false),
                ("X3", true),
                ("X7", false),
            ],
            1,
        ),
        (vec![("X1", false), ("X0", true)], 2),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}
fn plain(op: &str) -> &str {
    match op {
        "TCMPP" => "TCMP",
        "TZCPP" => "TZCP",
        "TADDP" => "TADD",
        "TSUBP" => "TSUB",
        _ => op,
    }
}
fn pulse(op: &str) -> bool {
    matches!(op, "TCMPP" | "TZCPP" | "TADDP" | "TSUBP")
}
fn addr(area: &str, n: u16) -> String {
    if matches!(area, "X" | "Y") {
        format!("{area}{n:o}")
    } else {
        format!("{area}{n}")
    }
}
fn triple(area: &str, start: u16, values: [i16; 3]) -> String {
    values
        .into_iter()
        .enumerate()
        .map(|(i, v)| format!("MOV K{v} {area}{}", start + i as u16))
        .collect::<Vec<_>>()
        .join("\n")
}
fn read_triple(area: &str, start: u16, out: u16) -> String {
    (0..3)
        .map(|i| format!("MOV {area}{} D{}", start + i, out + i))
        .collect::<Vec<_>>()
        .join("\n")
}
fn check_triple(area: &str, start: u16, values: [i16; 3]) -> String {
    values
        .into_iter()
        .enumerate()
        .map(|(i, v)| format!("AND= {area}{} K{v}", start + i as u16))
        .collect::<Vec<_>>()
        .join("\n")
}
fn result_seed(area: &str, base: u16) -> String {
    (base - 1..=base + 3)
        .map(|n| format!("SET {}", addr(area, n)))
        .collect::<Vec<_>>()
        .join("\n")
}
fn result_read(area: &str, base: u16) -> String {
    format!(
        "MOV K1{} D780\nLD {}\nOUT M830\nLD {}\nOUT M831",
        addr(area, base),
        addr(area, base - 1),
        addr(area, base + 3)
    )
}
// All literal-oracle markers are explicitly declared; callers must inspect only
// OUT M840..M847 present in a case, not assume eight markers for every case.
fn oracle(initial: &str, later: &str, marker: u16) -> String {
    format!("LD X1\n{initial}\nLDI X1\n{later}\nORB\nOUT M{marker}")
}
fn sequence_oracle(operand: &str, values: &[i16], marker: u16) -> String {
    let mut lines = Vec::new();
    for (i, v) in values.iter().enumerate() {
        lines.push(format!("LD= D870 K{}\nAND= {operand} K{v}", i + 1));
        if i != 0 {
            lines.push("ORB".into());
        }
    }
    lines.push(format!("OUT M{marker}"));
    lines.join("\n")
}
fn fixture(name: &str, seed: &str, body: &str, observe: &str) -> Case {
    let gate = if body.starts_with("LD") || body.starts_with("STL") {
        ""
    } else {
        "LD X0"
    };
    let source=[
        "LD X1\nMOV K0 E0\nMOV K0 F0\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV H1234 D1067\nMOV K77 D1068\nMOV K0 D870",
        seed,"LDI X7\nINC D870",gate,body,
        "LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y24\nLD M1068\nOUT Y25\nLDI X7\nMOV D1067 D900\nMOV D1068 D901\nMOV E0 D902\nMOV F0 D903\nMOV D1028 D904\nMOV D1029 D905",
        observe,"LDI X7\nMOV K1234 D906","END",
    ].into_iter().filter(|s|!s.is_empty()).collect::<Vec<_>>().join("\n");
    assert!(source.lines().count() <= 512, "oversized {name}");
    assert!(
        source.lines().count() * 17 < 0x1ef0,
        "conservative WORD budget {name}"
    );
    assert!(
        source.lines().any(|l| {
            let fields = l.split_whitespace().collect::<Vec<_>>();
            fields.first().is_some_and(|x| FORMS.contains(x))
                && fields[1..].iter().any(|x| {
                    let b = x.as_bytes();
                    b.len() >= 4
                        && matches!(b[b.len() - 2], b'E' | b'F')
                        && b[b.len() - 1].is_ascii_digit()
                })
        }),
        "missing addressed site {name}"
    );
    case(&format!("indexed_calendar_time_{name}"), &source, stages())
}
fn unique(cases: &[Case], count: usize) {
    assert_eq!(cases.len(), count);
    assert_eq!(
        cases.iter().map(|c| &c.name).collect::<BTreeSet<_>>().len(),
        count
    );
}
pub(super) fn common_cases() -> Vec<Case> {
    let mut cs = Vec::new();
    banks(&mut cs);
    numeric_roles(&mut cs);
    packed_setpoints(&mut cs);
    white_setpoints(&mut cs);
    semantic_errors(&mut cs);
    arithmetic_oracles(&mut cs);
    tails(&mut cs);
    controls(&mut cs);
    unique(&cs, 378);
    let present = cs
        .iter()
        .flat_map(|c| c.source.lines())
        .filter_map(|l| l.split_whitespace().next())
        .collect::<BTreeSet<_>>();
    for name in FORMS {
        assert!(present.contains(name));
    }
    cs
}

// Every single gray role is addressed by each of all16 banks. E+1 and F-1
// also demonstrate units: numeric WORDs, raw result bits, not byte offsets.
fn banks(cs: &mut Vec<Case>) {
    for op in FORMS {
        for bank in 0..8 {
            for prefix in ["E", "F"] {
                let delta = if prefix == "E" { 1i16 } else { -1 };
                let suffix = format!("{prefix}{bank}");
                let n = |base: u16| (base as i16 + delta) as u16;
                let mut seed = format!("MOV K{delta} {suffix}\n");
                let body;
                let observe;
                match plain(op) {
                    "TCMP" => {
                        seed += &format!(
                            "MOV K12 D{}\nMOV K20 D{}\nMOV K45 D{}\n{}\n{}",
                            n(20),
                            n(30),
                            n(40),
                            triple("D", n(60), [12, 20, 44]),
                            result_seed("M", n(300))
                        );
                        seed += &format!(
                            "\nMOV K1 K4M{}\nMOV K2 K4M{}\nMOV K3 K4M{}\n{}\n{}",
                            n(500),
                            n(540),
                            n(580),
                            triple("D", n(80), [1, 2, 3]),
                            result_seed("M", n(700))
                        );
                        body = format!(
                            "{op} D20{suffix} D30{suffix} D40{suffix} D60{suffix} M300{suffix}\nINC D{}\nLD X0\n{op} K4M500{suffix} K4M540{suffix} K4M580{suffix} D80{suffix} M700{suffix}",
                            n(60) + 2
                        );
                        observe = format!(
                            "LDI X7\n{}\n{}\nLDI X7\nMOV K1M{} D781\nMOV {suffix} D790",
                            read_triple("D", n(60), 770),
                            result_read("M", n(300)),
                            n(700)
                        );
                    }
                    "TZCP" => {
                        seed += &format!(
                            "{}\n{}\n{}\n{}",
                            triple("D", n(20), [6, 0, 0]),
                            triple("D", n(40), [18, 0, 0]),
                            triple("D", n(60), [5, 0, 0]),
                            result_seed("M", n(300))
                        );
                        body = format!(
                            "{op} D20{suffix} D40{suffix} D60{suffix} M300{suffix}\nINC D{}",
                            n(60)
                        );
                        observe = format!(
                            "LDI X7\n{}\n{}\nMOV {suffix} D790",
                            read_triple("D", n(60), 770),
                            result_read("M", n(300))
                        );
                    }
                    _ => {
                        seed += &format!(
                            "{}\n{}\n{}",
                            triple("D", n(20), [10, 20, 30]),
                            triple("D", n(60), [1, 2, 3]),
                            triple("D", n(100), [7, 8, 9])
                        );
                        body = format!(
                            "{op} D20{suffix} D60{suffix} D100{suffix}\nLD X0\nINC D{}",
                            n(20) + 2
                        );
                        observe = format!(
                            "LDI X7\n{}\n{}\nMOV {suffix} D790",
                            read_triple("D", n(20), 770),
                            read_triple("D", n(100), 780)
                        );
                    }
                }
                cs.push(fixture(
                    &format!("banks_{op}_{suffix}"),
                    &seed,
                    &body,
                    &observe,
                ));
            }
        }
    }
    assert_eq!(cs.len(), 128);
}
fn numeric_roles(cs: &mut Vec<Case>) {
    for op in FORMS {
        for (j, area) in ["T", "C", "D"].into_iter().enumerate() {
            for prefix in ["E", "F"] {
                let suffix = format!("{prefix}3");
                let delta = if prefix == "E" { 1i16 } else { -1 };
                let n = |v: u16| (v as i16 + delta) as u16;
                let out_area = ["Y", "M", "S"][j];
                let out_base = if out_area == "Y" { 128 } else { 300 };
                let mut seed = format!(
                    "MOV K{delta} {suffix}\n{}\n{}\n{}",
                    triple(area, n(20), [6, 0, 0]),
                    triple(area, n(40), [18, 0, 0]),
                    triple(area, n(60), [12, 20, 45])
                );
                let body;
                let observe;
                match plain(op) {
                    "TCMP" => {
                        seed += &format!(
                            "\nMOV K12 {area}{}\nMOV K20 {area}{}\nMOV K45 {area}{}\n{}",
                            n(2),
                            n(4),
                            n(6),
                            result_seed(out_area, n(out_base))
                        );
                        body = format!(
                            "{op} {area}2{suffix} {area}4{suffix} {area}6{suffix} {area}60{suffix} {}{suffix}",
                            addr(out_area, out_base)
                        );
                        observe = format!("LDI X7\n{}", result_read(out_area, n(out_base)));
                    }
                    "TZCP" => {
                        seed += &format!("\n{}", result_seed(out_area, n(out_base)));
                        body = format!(
                            "{op} {area}20{suffix} {area}40{suffix} {area}60{suffix} {}{suffix}",
                            addr(out_area, out_base)
                        );
                        observe = format!("LDI X7\n{}", result_read(out_area, n(out_base)));
                    }
                    _ => {
                        seed += &format!("\n{}", triple(area, n(80), [7, 8, 9]));
                        body = format!("{op} {area}60{suffix} {area}20{suffix} {area}80{suffix}");
                        observe = format!("LDI X7\n{}", read_triple(area, n(80), 780));
                    }
                }
                cs.push(fixture(
                    &format!("numeric_roles_{op}_{area}_{prefix}"),
                    &seed,
                    &body,
                    &observe,
                ));
            }
        }
    }
    assert_eq!(cs.len(), 176);
}
fn packed_setpoints(cs: &mut Vec<Case>) {
    for op in ["TCMP", "TCMPP"] {
        for area in ["X", "Y", "M", "S"] {
            for digits in 1..=4 {
                let bases = if area == "X" {
                    [32, 64, 96]
                } else {
                    [128, 160, 192]
                };
                let mut seed = format!(
                    "MOV K1 E1\nMOV K-1 F2\n{}\n{}",
                    triple("D", 21, [1, 2, 3]),
                    result_seed("M", 299)
                );
                if area != "X" {
                    for (i, b) in bases.into_iter().enumerate() {
                        seed += &format!("\nMOV K{} K{digits}{}", i + 1, addr(area, b + 1));
                    }
                }
                let body = format!(
                    "{op} K{digits}{}E1 K{digits}{}E1 K{digits}{}E1 D20E1 M300F2",
                    addr(area, bases[0]),
                    addr(area, bases[1]),
                    addr(area, bases[2])
                );
                let observe = format!(
                    "LDI X7\n{}\n{}",
                    result_read("M", 299),
                    oracle(
                        "AND= K1M299 K15\nAND M298",
                        "AND= K1M299 K10\nAND M298\nAND M1020\nANI M1021\nAND M1022",
                        840
                    )
                );
                let mut c = fixture(
                    &format!("packed_{op}_{area}_K{digits}"),
                    &seed,
                    &body,
                    &observe,
                );
                if area == "X" {
                    c.stages[0].0.extend([
                        ("X41", true),
                        ("X102", true),
                        ("X141", true),
                        ("X142", true),
                    ]);
                }
                cs.push(c);
            }
        }
    }
    assert_eq!(cs.len(), 208);
}
fn white_setpoints(cs: &mut Vec<Case>) {
    for op in ["TCMP", "TCMPP"] {
        for prefix in ["E", "F"] {
            for bank in 0..=8 {
                let direct = if bank == 8 {
                    prefix.to_string()
                } else {
                    format!("{prefix}{bank}")
                };
                let other = if bank == 7 { 6 } else { 7 };
                let seed = format!(
                    "MOV K0 E{other}\nMOV K0 F{other}\nMOV K12 {direct}\n{}\n{}",
                    triple("D", 20, [12, 20, 45]),
                    result_seed("S", 300)
                );
                let body = format!("{op} {direct} H14 K45 D20E{other} S300F{other}");
                let observe = format!(
                    "LDI X7\nMOV {direct} D790\n{}\n{}",
                    result_read("S", 300),
                    oracle("AND= K1S300 K15", "AND= K1S300 K10\nAND S299", 840)
                );
                cs.push(fixture(
                    &format!("white_{op}_{prefix}_{bank}"),
                    &seed,
                    &body,
                    &observe,
                ));
            }
        }
    }
    assert_eq!(cs.len(), 244);
}

fn semantic_errors(cs: &mut Vec<Case>) {
    for op in ["TCMP", "TCMPP"] {
        for bad in 0..4 {
            let values = if bad == 3 { [24, 0, 0] } else { [12, 20, 45] };
            let s = [
                if bad == 0 { 24 } else { 12 },
                if bad == 1 { 60 } else { 20 },
                if bad == 2 { -1 } else { 45 },
            ];
            let seed = format!(
                "MOV K0 E1\nMOV K32767 E7\nMOV K{} D30\nMOV K{} D40\nMOV K{} D50\n{}\n{}",
                s[0],
                s[1],
                s[2],
                triple("D", 60, values),
                result_seed("M", 300)
            );
            let body = format!("{op} D30E1 D40E1 D50E1 D60E1 M300E7");
            let observe = format!(
                "LDI X7\n{}\n{}",
                result_read("M", 300),
                oracle(
                    "AND= D1067 H1234",
                    "AND= D1067 HE1A\nAND= K1M300 K15\nAND M1020\nANI M1021\nAND M1022",
                    840
                )
            );
            cs.push(fixture(
                &format!("semantic_{op}_first_bad_role{bad}_unvisited_invalid_target"),
                &seed,
                &body,
                &observe,
            ));
        }
    }
    for op in ["TZCP", "TZCPP"] {
        for bad in 0..3 {
            let mut v = [[6, 0, 0], [18, 0, 0], [12, 0, 0]];
            v[bad][bad] = if bad == 0 { 24 } else { 60 };
            let seed = format!(
                "MOV K0 E1\nMOV K32767 F7\n{}\n{}\n{}\n{}",
                triple("D", 20, v[0]),
                triple("D", 40, v[1]),
                triple("D", 60, v[2]),
                result_seed("M", 300)
            );
            let body = format!("{op} D20E1 D40E1 D60E1 M300F7");
            let observe = oracle(
                "AND= D1067 H1234",
                "AND= D1067 HE1A\nAND= K1M300 K15\nAND M1020\nANI M1021\nAND M1022",
                840,
            );
            cs.push(fixture(
                &format!("semantic_{op}_first_bad_role{bad}_unvisited_invalid_target"),
                &seed,
                &body,
                &observe,
            ));
        }
    }
    for op in ARITH {
        for side in 0..2 {
            for field in 0..3 {
                let mut left = [1, 2, 3];
                let mut right = [4, 5, 6];
                if side == 0 {
                    left[field] = if field == 0 { 24 } else { -1 };
                } else {
                    right[field] = if field == 0 { -1 } else { 60 };
                }
                let seed = format!(
                    "MOV K0 E1\nMOV K0 F2\n{}\n{}\n{}",
                    triple("D", 20, left),
                    triple("D", 60, right),
                    triple("D", 100, [21, 22, 23])
                );
                let mut expected = [21, 22, 23];
                if field < 2 {
                    expected[2] = if op.starts_with("TADD") { 9 } else { 57 };
                }
                if field == 0 {
                    expected[1] = if op.starts_with("TADD") { 7 } else { 56 };
                }
                let body = format!("{op} D20E1 D60F2 D100F2");
                let observe = format!(
                    "LDI X7\n{}\n{}",
                    read_triple("D", 100, 780),
                    oracle(
                        &check_triple("D", 100, [21, 22, 23]),
                        &format!(
                            "{}\nAND= D1067 HE1A\nAND M1020\nANI M1021\nAND M1022",
                            check_triple("D", 100, expected)
                        ),
                        840
                    )
                );
                cs.push(fixture(
                    &format!("semantic_{op}_side{side}_field{field}_progressive"),
                    &seed,
                    &body,
                    &observe,
                ));
            }
        }
    }
    assert_eq!(cs.len(), 282);
}

fn arithmetic_oracles(cs: &mut Vec<Case>) {
    for op in ARITH {
        let sub = op.starts_with("TSUB");
        for mode in 0..3 {
            let (a, b, want, carry, zero) = match (sub, mode) {
                (false, 0) => ([23, 59, 59], [0, 0, 1], [0, 0, 0], true, true),
                (true, 0) => ([0, 0, 0], [0, 0, 1], [23, 59, 59], true, false),
                (false, 1) => ([12, 20, 45], [12, 20, 45], [0, 41, 30], true, false),
                (true, 1) => ([12, 20, 45], [12, 20, 45], [0, 0, 0], false, true),
                (false, _) => ([1, 2, 3], [4, 5, 6], [5, 7, 9], false, false),
                (true, _) => ([10, 20, 30], [1, 2, 3], [9, 18, 27], false, false),
            };
            let seed = format!(
                "MOV K0 E1\nMOV K0 F2\n{}\n{}\n{}",
                triple("D", 20, a),
                triple("D", 60, b),
                triple("D", 100, [7, 8, 9])
            );
            let flags = format!(
                "{} M1020\n{} M{}\n{} M{}",
                if zero { "AND" } else { "ANI" },
                if carry { "AND" } else { "ANI" },
                if sub { 1021 } else { 1022 },
                if sub { "AND" } else { "ANI" },
                if sub { 1022 } else { 1021 }
            );
            let observe = oracle(
                &check_triple("D", 100, [7, 8, 9]),
                &format!(
                    "{}\n{flags}\nAND= D1067 H1234",
                    check_triple("D", 100, want)
                ),
                840,
            );
            cs.push(fixture(
                &format!("literal_{op}_arithmetic{mode}"),
                &seed,
                &format!("{op} D20E1 D60F2 D100F2"),
                &observe,
            ));
        }
        // Restore on every powered visit, including held-P. Literal oracles
        // therefore distinguish native progressive writes from a skipped body.
        for direction in ["minus", "plus"] {
            let a = if sub { [10, 20, 30] } else { [1, 2, 3] };
            let b = if sub { [1, 2, 3] } else { [4, 5, 6] };
            let target = if direction == "minus" { 1 } else { 3 };
            let seed = format!(
                "MOV K0 E1\nMOV K0 F2\nMOV K77 D1\nMOV K88 D5\n{}\n{}",
                triple("D", 2, a),
                triple("D", 60, b)
            );
            let restore = format!("LD X0\n{}", triple("D", 2, a));
            let body = format!("{restore}\n{op} D2E1 D60F2 D{target}E1");
            let initial = if direction == "minus" {
                [77, a[0], a[1]]
            } else {
                [a[1], a[2], 88]
            };
            let result = match (sub, direction) {
                (false, "minus") => [18, 14, 9],
                (false, _) => [5, 7, 9],
                (true, "minus") => [77, 25, 27],
                (true, _) => [9, 18, 27],
            };
            let held = if pulse(op) {
                if direction == "minus" {
                    [result[0], a[0], a[1]]
                } else {
                    [a[1], a[2], result[2]]
                }
            } else {
                result
            };
            let checks = (0..3)
                .map(|i| {
                    sequence_oracle(
                        &format!("D{}", target + i as u16),
                        &[initial[i], result[i], held[i], held[i], result[i]],
                        840 + i as u16,
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let observe = format!(
                "LDI X7\nMOV D1 D779\n{}\nMOV D5 D783\n{checks}",
                read_triple("D", 2, 780)
            );
            cs.push(fixture(
                &format!("overlap_{op}_dest_{direction}_live_fields"),
                &seed,
                &body,
                &observe,
            ));
        }
        // Two pointers refer to D1180. The seconds store changes E1; cached
        // pointers must remain D1180 for minute/hour accesses in this visit.
        let seed = format!(
            "MOV K0 E1\nMOV H1234 F1\n{}\n{}",
            triple("D", 1180, [1, 2, 0]),
            triple("D", 60, [0, 0, 1])
        );
        let body = format!("LDI X7\nMOV K0 E1\nLD X0\n{op} D1180E1 D60E1 D1180E1");
        let own = if !sub {
            [
                [1, 1, 1, 1, 1],
                [2, 2, 2, 2, 2],
                [0, 1, if pulse(op) { 0 } else { 1 }, 0, 1],
            ]
        } else if pulse(op) {
            [[1, 1, 1, 1, 1], [2, 1, 1, 1, 0], [0, 59, 0, 0, 59]]
        } else {
            [[1, 1, 1, 1, 0], [2, 1, 0, 0, 59], [0, 59, 59, 0, 59]]
        };
        let ownchecks = (0..3)
            .map(|i| sequence_oracle(&format!("D{}", 1180 + i), &own[i], 840 + i as u16))
            .collect::<Vec<_>>()
            .join("\n");
        cs.push(fixture(
            &format!("cached_{op}_source_and_target_E1"),
            &seed,
            &body,
            &format!(
                "LDI X7\nMOV D1180 D780\nMOV D1181 D781\nMOV E1 D782\nMOV F1 D783\n{ownchecks}"
            ),
        ));
        // Target seconds is its own F1 index, written before minute/hour.
        let seed = format!(
            "MOV K0 E3\nMOV K0 F1\nMOV H2345 E2\n{}\n{}\n{}",
            triple("D", 20, [1, 2, 0]),
            triple("D", 60, [0, 0, 1]),
            triple("D", 1181, [7, 8, 0])
        );
        let body = format!("LDI X7\nMOV K0 F1\nLD X0\n{op} D20E3 D60E3 D1181F1");
        let second = if sub { 59 } else { 1 };
        let minute = if sub { 1 } else { 2 };
        let targetchecks = format!(
            "{}\n{}\n{}",
            sequence_oracle("D1181", &[7, 1, 1, 1, 1], 840),
            sequence_oracle("D1182", &[8, minute, minute, minute, minute], 841),
            sequence_oracle(
                "D1183",
                &[0, second, if pulse(op) { 0 } else { second }, 0, second],
                842
            )
        );
        cs.push(fixture(
            &format!("cached_{op}_target_seconds_F1"),
            &seed,
            &body,
            &format!(
                "LDI X7\nMOV D1181 D780\nMOV E1 D781\nMOV F1 D782\nMOV E2 D783\n{targetchecks}"
            ),
        ));
        // A valid domain-error is followed by a successful source at the same
        // header; sticky errors must not suppress success+3 or its inert tail.
        let seed = format!(
            "MOV K0 E1\nMOV K0 F2\n{}\n{}\n{}",
            triple("D", 20, [24, 2, 3]),
            triple("D", 60, [1, 1, 1]),
            triple("D", 100, [7, 8, 9])
        );
        let body = format!("LD X2\nMOV K10 D20\nLD X0\n{op} D20E1 D60F2 D100F2\nOUT M820");
        let mut c = fixture(
            &format!("sticky_error_then_success_{op}"),
            &seed,
            &body,
            "LDI X7\nMOV K1M820 D790\nMOV D100 D780\nMOV D101 D781\nMOV D102 D782",
        );
        c.stages = vec![
            (
                vec![("X1", true), ("X0", false), ("X2", false), ("X7", false)],
                1,
            ),
            (vec![("X1", false), ("X0", true)], 1),
            (vec![("X0", false), ("X2", true)], 1),
            (vec![("X0", true)], 2),
        ];
        cs.push(c);
    }
    assert_eq!(cs.len(), 314);
}

// Replace the complete native L WORD using public contacts/outputs. No private
// L address is fabricated as D memory. Original last two bits are 1,x.
fn seed_logic() -> String {
    (0..16u16)
        .map(|slot| {
            let load = if slot == 15 {
                "LD X0"
            } else if (0xa5a2u16 >> (15 - slot)) & 1 == 0 {
                "LD X7"
            } else {
                "LDI X7"
            };
            format!("{load}\nOUT M{}", 800 + slot)
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn tails(cs: &mut Vec<Case>) {
    for op in ARITH {
        for role in ["right", "dest"] {
            for (index, kind) in [(123, "NP"), (124, "ANB"), (125, "MPS"), (126, "MPP")] {
                let (right, dest) = if role == "right" {
                    (format!("D{index}F0"), "D200E1".into())
                } else {
                    ("D60E1".into(), format!("D{index}F0"))
                };
                let seed = format!(
                    "MOV K0 E1\nMOV K0 F0\n{}\n{}\n{}\n{}",
                    triple("D", 20, [10, 20, 30]),
                    triple("D", 60, [1, 2, 3]),
                    triple(
                        "D",
                        index,
                        if role == "right" {
                            [1, 2, 3]
                        } else {
                            [7, 8, 9]
                        }
                    ),
                    triple("D", 200, [7, 8, 9])
                );
                let body = format!(
                    "LD X3\nMC N0\n{}\nMPS\nINV\nMPS\nINV\n{op} D20E1 {right} {dest}\nMCR N0\nOUT M820\nMRD\nOUT M821\nMPP\nOUT M822\nMPP\nOUT M823",
                    seed_logic()
                );
                // x=false/no hidden tail ->0110; x=true/no tail ->1001.
                // Active input has low bits101. Hidden ANB and MPP both reduce to1110;
                // hidden MPS gives0111. NP only changes bit0, keeping separate PC history.
                let active = match kind {
                    "NP" => 9,
                    "ANB" | "MPP" => 14,
                    _ => 7,
                };
                let mut want = [6, active, active, 9, 6, active];
                if kind == "NP" {
                    want[2] = 8;
                    want[5] = 8;
                }
                if pulse(op) {
                    want[2] = 9;
                }
                let observe = format!(
                    "LDI X7\nMOV K1M820 D790\n{}\n{}\n{}",
                    read_triple("D", index, 770),
                    read_triple("D", 200, 780),
                    sequence_oracle("K1M820", &want, 840)
                );
                let mut c = fixture(
                    &format!("tail_{op}_{role}_{kind}_original_D{index}F0"),
                    &seed,
                    &body,
                    &observe,
                );
                c.stages = vec![
                    (
                        vec![("X1", true), ("X0", false), ("X3", true), ("X7", false)],
                        1,
                    ),
                    (vec![("X1", false), ("X0", true)], 2),
                    (vec![("X3", false)], 1),
                    (vec![("X0", false), ("X3", true)], 1),
                    (vec![("X0", true)], 1),
                ];
                cs.push(c);
            }
        }
    }
    assert_eq!(cs.len(), 346);
}

fn controls(cs: &mut Vec<Case>) {
    for op in FORMS {
        for mode in ["master", "step", "jump", "loop"] {
            let seed = format!(
                "MOV K0 E1\nMOV K0 F2\nMOV K0 D860\n{}\n{}\n{}\n{}\n{}\nZRST S0 S20\nSET S0",
                triple("D", 20, [6, 0, 0]),
                triple("D", 40, [18, 0, 0]),
                triple("D", 60, [12, 20, 45]),
                triple("D", 100, [7, 8, 9]),
                result_seed("M", 300)
            );
            let action = match plain(op) {
                "TCMP" => format!("{op} K12 H14 K45 D60E1 M300F2"),
                "TZCP" => format!("{op} D20E1 D40F2 D60E1 M300F2"),
                _ => format!("{op} D60E1 D20F2 D100F2"),
            };
            let body = match mode {
                "master" => format!("LD X2\nMC N0\nLD X0\n{action}\nINC D860\nMCR N0"),
                "step" => format!("LD X2\nSET S1\nSTL S1\nLD X0\n{action}\nINC D860\nRET"),
                "jump" => format!("LD X2\nCJ P10\nLD X0\n{action}\nINC D860\nP10"),
                _ => format!(
                    "LDI X7\nMOV K0 D861\nFOR K3\nLD= D861 K0\nOR= D861 K2\nAND X0\n{action}\nINC D860\nLDI X7\nINC D861\nNEXT"
                ),
            };
            let mut c = fixture(
                &format!("control_{op}_{mode}"),
                &seed,
                &body,
                "LDI X7\nMOV K1M300 D780\nMOV D100 D781\nMOV D101 D782\nMOV D102 D783\nMOV D860 D790",
            );
            if mode != "loop" {
                c.stages = vec![
                    (
                        vec![("X1", true), ("X0", false), ("X2", false), ("X7", false)],
                        1,
                    ),
                    (vec![("X1", false), ("X0", true)], 1),
                    (vec![("X2", true)], 1),
                    (vec![("X0", false), ("X2", false)], 1),
                    (vec![("X0", true), ("X2", true)], 1),
                ];
            }
            cs.push(c);
        }
    }
    assert_eq!(cs.len(), 378);
}

pub(super) fn model_cases(target: Target) -> Vec<Case> {
    let mut cs = Vec::new();
    let last = if target == Target::Ss2 { 4997 } else { 9997 };
    for op in FORMS {
        for (area, end) in [("T", 253), ("C", 197), ("D", last)] {
            let seed = format!(
                "MOV K1 E1\nMOV K-1 F2\n{}\n{}\n{}\n{}\nSET S1020\nSET S1021\nSET S1022\nSET S1023",
                triple(area, end, [12, 20, 45]),
                triple("D", 20, [6, 0, 0]),
                triple("D", 40, [18, 0, 0]),
                triple("D", 100, [7, 8, 9])
            );
            // Both the original and effective start are valid; first complete block
            // ends at the true model boundary. No partial/unchecked pointer fixture.
            let source = format!("{area}{}E1", end - 1);
            let body = match plain(op) {
                "TCMP" => format!(
                    "{op} {area}{}E1 {area}{end}E1 {area}{}E1 {source} S1022F2",
                    end - 1,
                    end + 1
                ),
                "TZCP" => format!(
                    "{op} D20E1 D40F2 {source} S1022F2\nLD X0\n{op} {source} D40F2 D20E1 M300E0\nLD X0\n{op} D20E1 {source} D40F2 M310E0"
                ),
                _ => format!(
                    "{op} {source} D21F2 D101F2\nLD X0\n{op} D20E0 {source} D200E0\nLD X0\n{op} D20E0 D40E0 {source}"
                ),
            };
            // TZCP low/high are effective D21/D39: seed their complete values too.
            let extra = if plain(op) == "TZCP" {
                format!(
                    "\n{}\n{}",
                    triple("D", 21, [6, 0, 0]),
                    triple("D", 39, [18, 0, 0])
                )
            } else {
                String::new()
            };
            let observe = format!(
                "LDI X7\n{}\nMOV K1S1020 D790\n{}\n{}\nMOV K1M300 D791\nMOV K1M310 D792",
                read_triple(area, end, 770),
                read_triple("D", 100, 780),
                read_triple("D", 200, 784)
            );
            cs.push(fixture(
                &format!("model_{target:?}_{op}_{area}_complete_last"),
                &(seed + &extra),
                &body,
                &observe,
            ));
        }
    }
    unique(&cs, 24);
    cs
}
