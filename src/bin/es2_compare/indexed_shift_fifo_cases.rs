//! Draft corpus: addressed SMOV, raw/word shifts, FIFO and ZRST, normal/P.
//! Static literal oracles are independent of the production shift kernels.
//! No execution or native parity is claimed by this draft.
use super::{Case as ComparisonCase, Target, case};
use std::collections::BTreeSet;

const FORMS: [&str; 16] = [
    "SMOV", "SMOVP", "SFTR", "SFTRP", "SFTL", "SFTLP", "WSFR", "WSFRP", "WSFL", "WSFLP", "SFWR",
    "SFWRP", "SFRD", "SFRDP", "ZRST", "ZRSTP",
];
const INPUTS: [&str; 16] = [
    "X100", "X101", "X102", "X103", "X104", "X105", "X106", "X107", "X110", "X111", "X112", "X113",
    "X114", "X115", "X116", "X117",
];
fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (
            vec![("X1", true), ("X0", false), ("X2", false), ("X7", false)],
            1,
        ),
        (vec![("X1", false), ("X0", true)], 2),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}
fn base(op: &str) -> &str {
    op.strip_suffix('P').unwrap_or(op)
}
fn addr(area: &str, n: u16) -> String {
    if matches!(area, "X" | "Y") {
        format!("{area}{n:o}")
    } else {
        format!("{area}{n}")
    }
}
fn words(area: &str, start: u16, values: &[i32]) -> String {
    values
        .iter()
        .enumerate()
        .map(|(i, v)| format!("MOV K{v} {area}{}", start + i as u16))
        .collect::<Vec<_>>()
        .join("\n")
}
fn reads(area: &str, start: u16, count: u16, out: u16) -> String {
    (0..count)
        .map(|i| format!("MOV {area}{} D{}", start + i, out + i))
        .collect::<Vec<_>>()
        .join("\n")
}
fn bits(area: &str, start: u16, count: u16, value: u64) -> String {
    (0..count)
        .map(|i| {
            format!(
                "{} {}",
                if value & (1u64 << (i % 64)) != 0 {
                    "SET"
                } else {
                    "RST"
                },
                addr(area, start + i)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn groups(area: &str, start: u16, digits: u16, values: &[u16]) -> String {
    values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            bits(
                area,
                start + i as u16 * digits * 4,
                digits * 4,
                u64::from(*v),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn group_reads(area: &str, start: u16, digits: u16, count: u16, out: u16) -> String {
    (0..count)
        .map(|i| {
            format!(
                "MOV K{digits}{} D{}",
                addr(area, start + i * digits * 4),
                out + i
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn bit_reads(area: &str, start: u16, count: u16, out: u16) -> String {
    assert_eq!(count % 4, 0);
    (0..count.div_ceil(16))
        .map(|i| {
            format!(
                "MOV K{}{} D{}",
                (count - i * 16).min(16) / 4,
                addr(area, start + i * 16),
                out + i
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn scenario(name: &str, seed: &str, body: &str, observe: &str) -> ComparisonCase {
    let gate = if body.starts_with("LD") || body.starts_with("STL") {
        ""
    } else {
        "LD X0"
    };
    let source = [
        "LD X1\nMOV K0 E0\nMOV K0 F0\nRST M1168\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV H1234 D1067\nMOV K77 D1068\nRST M790\nRST M791",
        seed, gate, body,
        "LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24\nLD M1168\nOUT Y25\nLDI X7\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305\nMOV K1234 D306",
        observe,"END",
    ].into_iter().filter(|s|!s.is_empty()).collect::<Vec<_>>().join("\n");
    assert!(source.lines().count() <= 650, "source line budget {name}");
    // Most instructions are 1/3/5 WORD; this deliberately conservative bound
    // also accommodates the 11-WORD SMOV and setup instructions.
    assert!(
        source.lines().count() * 11 < 0x1ef0,
        "source WORD budget {name}"
    );
    let indexed = source.lines().any(|line| {
        let f: Vec<_> = line.split_whitespace().collect();
        f.first().is_some_and(|op| FORMS.contains(op))
            && f[1..].iter().any(|v| {
                let b = v.as_bytes();
                b.len() >= 4
                    && matches!(b[b.len() - 2], b'E' | b'F')
                    && b[b.len() - 1].is_ascii_digit()
            })
    });
    assert!(indexed, "no addressed family instruction: {name}");
    case(&format!("indexed_shift_fifo_{name}"), &source, stages())
}
fn with_inputs(mut c: ComparisonCase) -> ComparisonCase {
    for ((changes, _), value) in c.stages.iter_mut().zip([0x1234u16, 0x5678, 0x9abc, 0x4321]) {
        for (i, name) in INPUTS.into_iter().enumerate() {
            changes.push((name, value & (1 << i) != 0));
        }
    }
    c
}
fn check(cases: &[ComparisonCase], expected: usize) {
    assert_eq!(cases.len(), expected);
    let names: BTreeSet<_> = cases.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names.len(), cases.len());
}

pub(super) fn common_cases() -> Vec<ComparisonCase> {
    let mut out = Vec::new();
    bank_cases(&mut out);
    packed_cases(&mut out);
    role_cases(&mut out);
    mixed_plain_cases(&mut out);
    literal_cases(&mut out);
    control_cases(&mut out);
    capacity_cases(&mut out);
    scratch_cases(&mut out);
    group_scratch_cases(&mut out);
    let actual: BTreeSet<_> = out
        .iter()
        .flat_map(|c| c.source.lines())
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    for op in FORMS {
        assert!(actual.contains(op), "missing {op}");
    }
    // Final count is fixed below after all independently designed sections.
    check(&out, COMMON_COUNT);
    out
}

// Every spelling independently visits E0..E7 and F0..F7. Each pair uses +1
// and -1 signed WORD offsets; targets/source data evolve under held power.
fn bank_cases(out: &mut Vec<ComparisonCase>) {
    for op in FORMS {
        for bank in 0..8 {
            let mut seed = format!(
                "MOV K1 E{bank}\nMOV K-1 F{bank}\n{}\n{}",
                words("D", 39, &[91, 92, 1234, 23, 34, 45, 46, 47]),
                words("D", 99, &[81, 5678, 2, 4, 8, 3, 82])
            );
            let (body, observe) = match base(op) {
                "SMOV" => (
                    format!("{op} D40E{bank} K4 K2 D101F{bank} K3\nINC D41"),
                    format!("{}\n{}", reads("D", 39, 8, 200), reads("D", 99, 7, 210)),
                ),
                "SFTR" | "SFTL" => {
                    seed.push_str(&format!(
                        "\n{}\n{}",
                        bits("M", 127, 16, 0x5a5b),
                        bits("S", 255, 16, 0xa55b)
                    ));
                    (
                        format!("{op} M128E{bank} S257F{bank} K9 K3\nINC K1M129"),
                        format!(
                            "{}\n{}",
                            bit_reads("M", 127, 16, 200),
                            bit_reads("S", 255, 16, 210)
                        ),
                    )
                }
                "WSFR" | "WSFL" => (
                    format!("{op} D40E{bank} D101F{bank} K5 K2\nINC D41"),
                    format!("{}\n{}", reads("D", 39, 8, 200), reads("D", 99, 7, 210)),
                ),
                "SFWR" => {
                    seed.push_str("\nMOV K0 D100");
                    (
                        format!("{op} D40E{bank} D101F{bank} K4\nINC D41"),
                        format!("{}\n{}", reads("D", 39, 8, 200), reads("D", 99, 7, 210)),
                    )
                }
                "SFRD" => {
                    seed.push_str("\nMOV K3 D41\nMOV K11 D42\nMOV K22 D43\nMOV K33 D44");
                    (
                        format!("{op} D40E{bank} D101F{bank} K4"),
                        format!("{}\n{}", reads("D", 39, 8, 200), reads("D", 99, 7, 210)),
                    )
                }
                "ZRST" => (
                    format!("{op} D40E{bank} D45F{bank}\nINC D42"),
                    reads("D", 39, 8, 200),
                ),
                _ => unreachable!(),
            };
            out.push(scenario(
                &format!("banks_{op}_E{bank}_F{bank}"),
                &seed,
                &body,
                &format!("{observe}\nMOV E{bank} D280\nMOV F{bank} D281"),
            ));
        }
    }
    assert_eq!(out.len(), 128);
}

// Exact K1..K4 strides and all three writable packed areas. Source/target
// widths differ only for SMOV/FIFO, where the manual permits that difference.
fn packed_cases(out: &mut Vec<ComparisonCase>) {
    for op in [
        "SMOV", "SMOVP", "WSFR", "WSFRP", "WSFL", "WSFLP", "SFWR", "SFWRP", "SFRD", "SFRDP",
    ] {
        for digits in 1..=4 {
            let mut seed = String::from("MOV K0 E0\nMOV K0 F0\nSET M1168");
            let mut body = Vec::new();
            let mut observe = Vec::new();
            for (i, (area, src_area)) in
                [("Y", "M"), ("M", "S"), ("S", "Y")].into_iter().enumerate()
            {
                let d = format!("K{digits}{}F0", addr(area, 128));
                let s = format!("K{digits}{}E0", addr(src_area, 64));
                seed.push_str(&format!(
                    "\n{}\n{}\nSET {}\nSET {}",
                    groups(src_area, 64, digits, &[3, 6]),
                    groups(area, 128, digits, &[1, 2, 4, 8]),
                    addr(area, 127),
                    addr(area, 128 + digits * 16)
                ));
                let statement = match base(op) {
                    "SMOV" => format!("{op} {s} K4 K2 {d} K3"),
                    "WSFR" | "WSFL" => format!("{op} {s} {d} K3 K2"),
                    "SFWR" => {
                        seed.push_str(&format!("\nMOV K0 K{digits}{}", addr(area, 128)));
                        format!("{op} {s} {d} K4")
                    }
                    "SFRD" => {
                        seed.push_str(&format!(
                            "\nMOV K3 K{digits}{}\nMOV H55AA K{}{}",
                            addr(area, 128),
                            5 - digits,
                            addr(area, 224)
                        ));
                        format!("{op} {d} K{}{}E0 K4", 5 - digits, addr(area, 224))
                    }
                    _ => unreachable!(),
                };
                body.push(format!("LD X0\n{statement}"));
                observe.push(format!(
                    "LDI X7\n{}\nMOV K{}{} D{}\nLD {}\nOUT M{}\nLD {}\nOUT M{}",
                    group_reads(area, 128, digits, 4, 200 + i as u16 * 12),
                    5 - digits,
                    addr(area, 224),
                    206 + i as u16 * 12,
                    addr(area, 127),
                    810 + i * 2,
                    addr(area, 128 + digits * 16),
                    811 + i * 2
                ));
            }
            out.push(scenario(
                &format!("packed_{op}_K{digits}_all_writable_areas"),
                &seed,
                &body.join("\n"),
                &observe.join("\n"),
            ));
        }
    }
    assert_eq!(out.len(), 168);
}

fn role_cases(out: &mut Vec<ComparisonCase>) {
    for op in ["SFTR", "SFTRP", "SFTL", "SFTLP"] {
        for source_area in ["X", "Y", "M", "S"] {
            let mut seed = String::from("MOV K1 E0\nMOV K-1 F0");
            if source_area != "X" {
                seed.push_str(&format!("\n{}", bits(source_area, 64, 16, 0x1234)));
            }
            let mut body = Vec::new();
            let mut observe = Vec::new();
            for (i, area) in ["Y", "M", "S"].into_iter().enumerate() {
                seed.push_str(&format!("\n{}", bits(area, 127, 16, 0x5a5b)));
                body.push(format!(
                    "LD X0\n{op} {}E0 {}F0 H5 H2",
                    addr(source_area, 64),
                    addr(area, 129)
                ));
                observe.push(bit_reads(area, 127, 16, 200 + i as u16 * 3));
            }
            out.push(with_inputs(scenario(
                &format!("raw_roles_{op}_{source_area}_to_YMS"),
                &seed,
                &body.join("\n"),
                &observe.join("\n"),
            )));
        }
    }
    // Both numeric categories permit T/C/D mixing. Each independent call has
    // fresh arrays so the nine routes cannot contaminate one another's inputs.
    for op in ["WSFR", "WSFRP", "WSFL", "WSFLP"] {
        let mut body = Vec::new();
        for (i, (sa, da)) in ["T", "C", "D"]
            .into_iter()
            .flat_map(|s| ["T", "C", "D"].into_iter().map(move |d| (s, d)))
            .enumerate()
        {
            body.push(format!(
                "LDI X7\n{}\n{}\nLD X0\n{op} {sa}20E0 {da}40F0 H5 H2\nLDI X7\n{}",
                words(sa, 20, &[101, 202]),
                words(da, 40, &[10, 20, 30, 40, 50]),
                reads(da, 40, 5, 200 + i as u16 * 7)
            ));
        }
        out.push(scenario(
            &format!("numeric_roles_{op}_all_TC_D_pairs"),
            "MOV K0 E0\nMOV K0 F0",
            &body.join("\n"),
            "MOV D200 D280\nMOV D260 D281",
        ));
    }
    // Plain E/F remain direct WORD operands, never suffixed. A separate gray
    // operand in every instruction supplies the addressed role under test.
    for op in ["SMOV", "SMOVP", "SFWR", "SFWRP", "SFRD", "SFRDP"] {
        let mut body = Vec::new();
        let mut observe = Vec::new();
        for (i, role) in ["T20", "C20", "D20", "E1", "F1"].into_iter().enumerate() {
            let gray = if matches!(role, "E1" | "F1") {
                role.to_string()
            } else {
                format!("{role}E0")
            };
            let setup = format!(
                "MOV K1234 {role}\n{}",
                words("D", 100, &[0, 11, 22, 33, 44])
            );
            let instruction = match base(op) {
                "SMOV" => format!(
                    "MOV K5678 D100\nLD X0\n{op} {gray} K4 K2 D100F0 K3\nLDI X7\nMOV D100 D{}\nMOV K1234 D20\nMOV K5678 {role}\nLD X0\n{op} D20F0 K4 K2 {gray} K3\nLDI X7\nMOV {role} D{}",
                    200 + i * 4,
                    201 + i * 4
                ),
                "SFWR" => format!(
                    "LD X0\n{op} {gray} D100F0 K4\nLDI X7\nMOV D100 D{}\nMOV D101 D{}",
                    200 + i * 4,
                    201 + i * 4
                ),
                "SFRD" => format!(
                    "MOV K1 D100\nLD X0\n{op} D100F0 {gray} K4\nLDI X7\nMOV D100 D{}\nMOV {role} D{}",
                    200 + i * 4,
                    201 + i * 4
                ),
                _ => unreachable!(),
            };
            body.push(format!("LDI X7\n{setup}\n{instruction}"));
            observe.push(format!("MOV D{} D{}", 200 + i * 4, 250 + i));
        }
        out.push(scenario(
            &format!("word_alias_roles_{op}_TCDEF"),
            "MOV K0 E0\nMOV K0 F0",
            &body.join("\n"),
            &observe.join("\n"),
        ));
    }
    for op in ["SFWR", "SFWRP"] {
        out.push(scenario(
            &format!("immediate_sources_{op}_KH_signed"),
            "MOV K0 E0\nMOV K0 F0\nMOV K0 D100\nMOV K0 D110\nMOV K0 D120\nMOV K0 D130",
            &format!(
                "{op} K-1 D100E0 K4\n{op} H8000 D110F0 H4\n{op} H7FFF D120E0 K4\n{op} K0 D130F0 K4"
            ),
            &format!(
                "{}\n{}\n{}\n{}",
                reads("D", 100, 4, 200),
                reads("D", 110, 4, 210),
                reads("D", 120, 4, 220),
                reads("D", 130, 4, 230)
            ),
        ));
    }
    for op in [
        "SMOV", "SMOVP", "WSFR", "WSFRP", "WSFL", "WSFLP", "SFWR", "SFWRP",
    ] {
        let seed = format!(
            "MOV K0 E0\nMOV K0 F0\nSET M1168\n{}",
            groups("M", 128, 2, &[0, 2, 4, 8])
        );
        let body = match base(op) {
            "SMOV" => format!("{op} K4X100E0 K4 K2 K2M128F0 K2"),
            "WSFR" | "WSFL" => format!("{op} K2X100E0 K2M128F0 K4 K2"),
            "SFWR" => format!("{op} K4X100E0 K2M128F0 K4"),
            _ => unreachable!(),
        };
        out.push(with_inputs(scenario(
            &format!("packed_input_source_{op}"),
            &seed,
            &body,
            &group_reads("M", 128, 2, 4, 200),
        )));
    }
    for op in ["ZRST", "ZRSTP"] {
        let mut seed = String::from("MOV K1 E0\nMOV K-1 F0");
        let mut body = Vec::new();
        let mut observe = Vec::new();
        for (i, area) in ["Y", "M", "S", "T", "C", "D"].into_iter().enumerate() {
            if matches!(area, "Y" | "M" | "S") {
                seed.push_str(&format!("\n{}", bits(area, 127, 12, 0xfff)));
                body.push(format!(
                    "LD X0\n{op} {}E0 {}F0",
                    addr(area, 128),
                    addr(area, 134)
                ));
                observe.push(bit_reads(area, 127, 12, 200 + i as u16 * 3));
            } else {
                seed.push_str(&format!(
                    "\n{}",
                    words(area, 127, &[11, 22, 33, 44, 55, 66, 77, 88])
                ));
                body.push(format!("LD X0\n{op} {area}128E0 {area}134F0"));
                observe.push(reads(area, 127, 8, 220 + (i as u16 - 3) * 9));
            }
        }
        out.push(scenario(
            &format!("reset_roles_{op}_all_six_areas"),
            &seed,
            &body.join("\n"),
            &observe.join("\n"),
        ));
    }
    assert_eq!(out.len(), 206);
}

// Conditions below are literal public observations, never computed by a
// production kernel. Setup runs on every scan, making held-P skips visible.
fn oracle_block(op: &str, args: &str, setup: &str, on: &str, off: &str, marker: u16) -> String {
    format!(
        "LDI X7\nMOV K0 E0\nMOV K0 F0\nRST M1168\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV H1234 D1067\nMOV K77 D1068\n{setup}\nLD X0\n{op} {args}\nLD M791\n{on}\nLDI M791\n{off}\nORB\nOUT M{marker}"
    )
}
// Each gray role is also exercised without a suffix while its partner is
// addressed. This complements (rather than substitutes for) both-indexed rows.
fn mixed_plain_cases(out: &mut Vec<ComparisonCase>) {
    for op in FORMS {
        let mut seed = format!(
            "MOV K0 E0\nMOV K0 F0\nSET M1168\n{}\n{}\n{}\n{}\n{}\n{}",
            words("D", 20, &[1234, 23, 34, 45]),
            words("D", 100, &[5678, 11, 22, 33]),
            words("T", 100, &[3, 11, 22, 33]),
            words("C", 100, &[3, 11, 22, 33]),
            groups("M", 128, 1, &[3, 1, 2, 4]),
            groups("S", 128, 1, &[3, 5, 6, 7])
        );
        let body = match base(op) {
            "SMOV" => format!(
                "LD X0\n{op} D20E0 H4 H4 D100 H4\n{op} T101 K1 K1 C101F0 K1\n{op} K1M128 K1 K1 K1S128F0 K1\n{op} K1M132E0 K1 K1 K1S132 K1"
            ),
            "SFTR" | "SFTL" => format!("LD X0\n{op} M128 M160F0 K8 K2\n{op} M128E0 S160 K8 K2"),
            "WSFR" | "WSFL" => format!(
                "LD X0\n{op} T101 C100F0 K4 K2\n{op} C101E0 T100 K4 K2\n{op} K1M128 K1S128F0 K4 K2\n{op} K1S128E0 K1M128 K4 K2"
            ),
            "SFWR" => {
                seed.push_str("\nMOV K0 T100\nMOV K0 C100\nMOV K0 K1M128\nMOV K0 K1S128");
                format!(
                    "LD X0\n{op} D20E0 T100 K4\n{op} D20E0 C100 K4\n{op} K1M132E0 K1S128 K4\n{op} K1M132 K1M128F0 K4"
                )
            }
            "SFRD" => format!(
                "LD X0\n{op} T100 D100F0 K4\n{op} C100E0 D20 K4\n{op} K1M128 K1S148F0 K4\n{op} K1S128E0 K1M148 K4"
            ),
            "ZRST" => format!(
                "LD X0\n{op} T100 T102F0\n{op} C100E0 C102\n{op} M128 M134F0\n{op} S128E0 S134"
            ),
            _ => unreachable!(),
        };
        out.push(scenario(
            &format!("plain_partner_roles_{op}"),
            &seed,
            &body,
            &format!(
                "{}\n{}\n{}\n{}\n{}\n{}",
                reads("D", 20, 4, 200),
                reads("D", 100, 4, 210),
                reads("T", 100, 4, 220),
                reads("C", 100, 4, 230),
                bit_reads("M", 128, 40, 240),
                bit_reads("S", 128, 40, 250)
            ),
        ));
    }
    assert_eq!(out.len(), 222);
}
fn oracle_case(name: &str, op: &str, blocks: Vec<String>, observe: &str) -> ComparisonCase {
    let body = format!(
        "LD X0\n{}OUT M791\n{}\nLD X0\nOUT M790",
        if op.ends_with('P') { "ANI M790\n" } else { "" },
        blocks.join("\n")
    );
    scenario(&format!("literal_oracle_{name}_{op}"), "", &body, observe)
}
fn eqs(area: &str, start: u16, values: &[i32]) -> String {
    values
        .iter()
        .enumerate()
        .map(|(i, v)| format!("AND= {area}{} K{v}", start + i as u16))
        .collect::<Vec<_>>()
        .join("\n")
}
fn geqs(area: &str, start: u16, digits: u16, values: &[u16]) -> String {
    values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            format!(
                "AND= K{digits}{} K{v}",
                addr(area, start + i as u16 * digits * 4)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn literal_cases(out: &mut Vec<ComparisonCase>) {
    for op in ["SMOV", "SMOVP"] {
        out.push(oracle_case(
            "decimal_hex_and_short_snapshot",
            op,
            vec![
                oracle_block(
                    op,
                    "D10E0 K4 K2 D20F0 K3",
                    "MOV K1234 D10\nMOV K5678 D20",
                    "AND= D20 K5128\nAND M1020\nANI M1021\nAND M1022",
                    "AND= D20 K5678",
                    840,
                ),
                oracle_block(
                    op,
                    "D10E0 H4 H2 D20F0 H3",
                    "SET M1168\nMOV H1234 D10\nMOV H5678 D20",
                    "AND= D20 H5128",
                    "AND= D20 H5678",
                    841,
                ),
                oracle_block(
                    op,
                    "K1M128E0 K4 K2 D20F0 K3",
                    "SET M1168\nMOV K7 K1M128\nMOV H5678 D20",
                    "AND= D20 H5008\nAND= K1M128 K7",
                    "AND= D20 H5678",
                    842,
                ),
                oracle_block(
                    op,
                    "D10E0 K3 K1 K1M128F0 K1",
                    "SET M1168\nMOV H1234 D10\nMOV K7 K1M128",
                    "AND= K1M128 K2",
                    "AND= K1M128 K7",
                    843,
                ),
            ],
            "MOV D20 D220\nMOV K1M128 D221",
        ));
        out.push(oracle_case("full_snapshot_invalid_source_or_old_destination",op,vec![
            oracle_block(op,"D10E0 K1 K1 D20F0 K1","MOV K-1 D10\nMOV K77 D20","AND= D20 K77\nAND= D1067 HE18\nAND M1067\nAND M1068\nAND M1020\nANI M1021\nAND M1022","AND= D20 K77\nAND= D1067 H1234\nANI M1067",840),
            oracle_block(op,"D10E0 K4 K1 D20F0 K1","MOV K1234 D10\nMOV K10000 D20","AND= D20 K10000\nAND= D1067 HE18\nAND M1067\nAND M1068","AND= D20 K10000\nAND= D1067 H1234",841),
        ],"MOV D1068 D220"));
        out.push(oracle_case(
            "own_target_index_and_overlapping_snapshot",
            op,
            vec![
                oracle_block(
                    op,
                    "D10E0 K1 K1 D1182E1 K1",
                    "MOV K0 E1\nMOV K7 D10\nMOV K88 D1189",
                    "AND= E1 K7\nAND= D1189 K88",
                    "AND= E1 K0\nAND= D1189 K88",
                    840,
                ),
                oracle_block(
                    op,
                    "K4M128E0 K2 K2 K4M132F0 K3",
                    &format!("SET M1168\n{}", bits("M", 128, 20, 0xa5678)),
                    "AND= K4M128 H7878\nAND= K4M132 HA787",
                    "AND= K4M128 H5678\nAND= K4M132 HA567",
                    841,
                ),
            ],
            "MOV E1 D220\nMOV K4M132 D221",
        ));
    }
    // A one-bit shift source overlaps its target. Raw SFTL reads descending;
    // packed WSFL below reads ascending and therefore produces another value.
    for op in ["SFTR", "SFTRP", "SFTL", "SFTLP"] {
        let left = base(op) == "SFTL";
        out.push(oracle_case(
            "raw_live_overlap",
            op,
            vec![
                oracle_block(
                    op,
                    "M128E0 M128F0 K5 K2",
                    &bits("M", 128, 8, 0x15),
                    if left {
                        "AND= K2M128 H15"
                    } else {
                        "AND= K2M128 H0D"
                    },
                    "AND= K2M128 H15",
                    840,
                ),
                oracle_block(
                    op,
                    "M128E0 M129F0 K8 K8",
                    &bits("M", 128, 12, 1),
                    if left {
                        "AND= K3M128 H003"
                    } else {
                        "AND= K3M128 H1FF"
                    },
                    "AND= K3M128 H001",
                    841,
                ),
            ],
            "MOV K3M128 D220",
        ));
    }
    for op in ["WSFR", "WSFRP", "WSFL", "WSFLP"] {
        let left = base(op) == "WSFL";
        out.push(oracle_case(
            "numeric_column_and_group_live_order",
            op,
            vec![
                oracle_block(
                    op,
                    "D12E0 D10F0 K5 K2",
                    &words("D", 10, &[10, 20, 30, 40, 50]),
                    &eqs(
                        "D",
                        10,
                        if left {
                            &[10, 20, 10, 20, 30]
                        } else {
                            &[30, 40, 50, 30, 30]
                        },
                    ),
                    &eqs("D", 10, &[10, 20, 30, 40, 50]),
                    840,
                ),
                oracle_block(
                    op,
                    "K1M136E0 K1M128F0 K5 K2",
                    &groups("M", 128, 1, &[1, 2, 4, 8, 3]),
                    &geqs(
                        "M",
                        128,
                        1,
                        if left {
                            &[1, 2, 1, 2, 4]
                        } else {
                            &[4, 8, 3, 3, 3]
                        },
                    ),
                    &geqs("M", 128, 1, &[1, 2, 4, 8, 3]),
                    841,
                ),
                oracle_block(
                    op,
                    "K2M128E0 K2M129F0 K1 K1",
                    &bits("M", 128, 12, 1),
                    "AND= K3M128 H1FF",
                    "AND= K3M128 H001",
                    842,
                ),
            ],
            "MOV K3M128 D220\nMOV D10 D221",
        ));
    }
    for op in ["SFWR", "SFWRP"] {
        out.push(oracle_case(
            "late_header_index_and_source_snapshot",
            op,
            vec![
                oracle_block(
                    op,
                    "K9 D1182E1 K4",
                    "MOV K0 E1\nMOV K77 F1\nMOV K88 D1184\nMOV K99 D1185",
                    "AND= E1 K1\nAND= F1 K77\nAND= D1184 K9\nAND= D1185 K99\nANI M1022",
                    "AND= E1 K0\nAND= F1 K77\nAND= D1184 K88\nAND M1022",
                    840,
                ),
                oracle_block(
                    op,
                    "E1 D1182F0 K4",
                    "MOV K0 E1\nMOV K77 F1\nMOV K88 D1184",
                    "AND= E1 K1\nAND= F1 K0\nAND= D1184 K88",
                    "AND= E1 K0\nAND= F1 K77\nAND= D1184 K88",
                    841,
                ),
            ],
            "MOV E1 D220\nMOV F1 D221\nMOV D1184 D222",
        ));
        out.push(oracle_case(
            "short_head_untruncated_slot_and_capacity_two",
            op,
            vec![
                oracle_block(
                    op,
                    "K9 K1M128E0 K17",
                    &format!("{}\nMOV K15 K1M128\nMOV K6 K1M192", bits("M", 128, 68, 0)),
                    "AND= K1M128 K0\nAND= K1M132 K0\nAND= K1M188 K0\nAND= K1M192 K9\nANI M1022",
                    "AND= K1M128 K15\nAND= K1M192 K6\nAND M1022",
                    840,
                ),
                oracle_block(
                    op,
                    "H8001 D100E0 H2",
                    &words("D", 99, &[91, 0, 77, 92]),
                    "AND= D99 K91\nAND= D100 K1\nAND= D101 H8001\nAND= D102 K92\nANI M1022",
                    "AND= D100 K0\nAND= D101 K77\nAND M1022",
                    841,
                ),
            ],
            "MOV K1M128 D220\nMOV K1M192 D221\nMOV D101 D222",
        ));
        out.push(oracle_case(
            "full_and_negative_header_flag_order",
            op,
            vec![
                oracle_block(
                    op,
                    "K9 D100E0 K4",
                    &words("D", 100, &[3, 11, 22, 33]),
                    "AND= D100 K3\nAND= D101 K11\nAND M1022\nANI M1067",
                    "AND= D100 K3\nAND= D101 K11\nAND M1022\nANI M1067",
                    840,
                ),
                oracle_block(
                    op,
                    "D20F0 D100E0 K4",
                    &format!("MOV K9 D20\n{}", words("D", 100, &[-1, 11, 22, 33])),
                    "AND= D100 K-1\nAND= D101 K11\nAND M1022\nAND= D1067 HE1A\nAND M1067",
                    "AND= D100 K-1\nAND= D1067 H1234\nANI M1067",
                    841,
                ),
            ],
            "MOV D1068 D220",
        ));
        out.push(oracle_case(
            "source_flag_snapshot_before_carry_clear",
            op,
            vec![oracle_block(
                op,
                "K1M1019 D100E0 K4",
                "MOV K11 K1M1019\nMOV K0 D100\nMOV K77 D101",
                "AND= D100 K1\nAND= D101 K11\nAND= K1M1019 K3",
                "AND= D100 K0\nAND= D101 K77\nAND= K1M1019 K11",
                840,
            )],
            "MOV K1M1019 D220\nMOV D101 D221",
        ));
    }
    for op in ["SFRD", "SFRDP"] {
        out.push(oracle_case("late_head_rebase_and_output_live_shift",op,vec![
            oracle_block(op,"D1182F1 D20E0 K4","MOV K77 E1\nMOV K1 F1\nMOV K22 D1184\nMOV K33 D1185\nMOV K99 D20","AND= E1 K77\nAND= D20 K0\nAND= F1 K22\nAND= D1184 K33\nAND= D1185 K33\nAND M1020","AND= F1 K1\nAND= D20 K99",840),
            oracle_block(op,"D100E0 D102F0 K4",&words("D",100,&[2,11,22,33]),"AND= D100 K1\nAND= D101 K11\nAND= D102 K33\nAND= D103 K33\nANI M1020","AND= D100 K2\nAND= D101 K11\nAND= D102 K22\nAND= D103 K33",841),
        ],"MOV D20 D220\nMOV E1 D221\nMOV F1 D222"));
        out.push(oracle_case(
            "empty_negative_and_capacity_two",
            op,
            vec![
                oracle_block(
                    op,
                    "D100E0 D20F0 K4",
                    &format!("MOV K99 D20\n{}", words("D", 100, &[0, 11, 22, 33])),
                    "AND= D20 K99\nAND= D101 K11\nAND M1020\nAND M1022\nANI M1067",
                    "AND= D20 K99\nAND M1020\nAND M1022",
                    840,
                ),
                oracle_block(
                    op,
                    "D100E0 D20F0 K4",
                    &format!("MOV K99 D20\n{}", words("D", 100, &[-1, 11, 22, 33])),
                    "AND= D20 K99\nANI M1020\nAND M1022\nAND= D1067 HE1A\nAND M1067",
                    "AND= D20 K99\nAND M1020\nAND= D1067 H1234",
                    841,
                ),
                oracle_block(
                    op,
                    "D100E0 D20F0 H2",
                    &format!("MOV K99 D20\n{}", words("D", 99, &[91, 1, 17, 92])),
                    "AND= D100 K0\nAND= D101 K17\nAND= D20 K17\nAND= D102 K92\nAND M1020",
                    "AND= D100 K1\nAND= D20 K99",
                    842,
                ),
            ],
            "MOV D20 D220\nMOV D1068 D221",
        ));
        out.push(oracle_case("packed_output_width_and_capacity_live_bits",op,vec![
            oracle_block(op,"K1M128E0 K2S128F0 K4",&format!("{}\nMOV HAA K2S128",groups("M",128,1,&[1,10,11,12])),"AND= K1M128 K0\nAND= K1M132 K11\nAND= K1M136 K12\nAND= K1M140 K12\nAND= K2S128 K10\nAND M1020","AND= K1M128 K1\nAND= K2S128 HAA",840),
            oracle_block(op,"K1M128E0 K1M136F0 K4",&groups("M",128,1,&[2,1,2,4]),"AND= K1M128 K1\nAND= K1M132 K1\nAND= K1M136 K4\nAND= K1M140 K4","AND= K1M128 K2\nAND= K1M132 K1\nAND= K1M136 K2\nAND= K1M140 K4",841),
        ],"MOV K4M128 D220\nMOV K2S128 D221"));
        out.push(oracle_case(
            "output_overwrites_published_zero_flag",
            op,
            vec![oracle_block(
                op,
                "D100E0 K1M1019 K2",
                "MOV K3 K1M1019\nMOV K1 D100\nMOV K8 D101",
                "AND= D100 K0\nAND= D101 K8\nAND= K1M1019 K8\nANI M1020\nAND M1022",
                "AND= D100 K1\nAND= K1M1019 K3\nAND M1020\nANI M1022",
                840,
            )],
            "MOV K1M1019 D220",
        ));
    }
    for op in ["ZRST", "ZRSTP"] {
        out.push(oracle_case("cached_alias_bounds_and_reversed_range",op,vec![
            oracle_block(op,"D1180E1 D1183F1","MOV K2 E1\nMOV K3 F1\nMOV K11 D1184\nMOV K22 D1185\nMOV K33 D1186\nMOV K44 D1187","AND= E1 K0\nAND= F1 K0\nAND= D1184 K0\nAND= D1185 K0\nAND= D1186 K0\nAND= D1187 K44","AND= E1 K2\nAND= F1 K3\nAND= D1184 K11\nAND= D1185 K22\nAND= D1186 K33",840),
            oracle_block(op,"D104E0 D101F0",&words("D",100,&[10,20,30,40,50]),"AND= D100 K10\nAND= D101 K0\nAND= D102 K30\nAND= D104 K50","AND= D100 K10\nAND= D101 K20\nAND= D104 K50",841),
            oracle_block(op,"C199E0 C201F0","MOV K77 C199\nDMOV H12345678 C200\nDMOV H76543210 C201\nDMOV H13572468 C202","AND= C199 K0\nDAND= C200 K0\nDAND= C201 K0\nDAND= C202 H13572468","AND= C199 K77\nDAND= C200 H12345678\nDAND= C201 H76543210",842),
        ],"MOV C199 D220\nDMOV C200 D222\nDMOV C201 D224"));
    }
    assert_eq!(out.len(), 254);
}

fn controlled_operands(op: &str) -> &'static str {
    match base(op) {
        "SMOV" => "D40E1 K4 K2 D100F1 K3",
        "SFTR" | "SFTL" => "M128E1 M256F1 K8 K2",
        "WSFR" | "WSFL" => "D40E1 D100F1 K4 K2",
        "SFWR" => "D40E1 D100F1 K4",
        "SFRD" => "D40E1 D100F1 K4",
        "ZRST" => "D40E1 D44F1",
        _ => unreachable!(),
    }
}
fn control_seed(op: &str) -> String {
    let mut seed = format!(
        "MOV K1 E1\nMOV K0 F1\nMOV K0 D60\n{}\n{}\n{}\n{}",
        words("D", 39, &[90, 100, 1234, 22, 33, 55, 4321, 66, 77, 88]),
        words("D", 99, &[90, 5678, 11, 22, 33, 44, 55]),
        bits("M", 128, 16, 0x5aa5),
        bits("M", 255, 16, 0xa55a)
    );
    if base(op) == "SFWR" {
        seed.push_str("\nMOV K0 D100");
    }
    if base(op) == "SFRD" {
        seed.push_str("\nMOV K3 D41\nMOV K3 D45");
    }
    seed
}
fn control_cases(out: &mut Vec<ComparisonCase>) {
    for (slot, op) in FORMS.into_iter().enumerate() {
        let args = controlled_operands(op);
        let kind = (slot / 2 + slot % 2) % 4;
        let (label, body) = match kind {
            0 => (
                "for_repeated_same_pc",
                format!("LD X0\nFOR K3\nLD X0\n{op} {args}\nNEXT\nLDI X7\nINC D60"),
            ),
            1 => (
                "mps_between_sites_live_modifier",
                format!(
                    "LD X0\nMPS\n{op} {args}\nMRD\nMOV K5 E1\nMRD\n{op} {args}\nMPP\nOUT Y10\nLDI X7\nMOV K1 E1\nINC D60"
                ),
            ),
            2 => (
                "cj_skipped_off_visit",
                format!("LD X2\nCJ P10\nLD X0\n{op} {args}\nP10\nLDI X7\nINC D60"),
            ),
            _ => (
                "master_false_true_accumulator",
                format!("LD X7\nMC N0\nLDI X7\n{op} {args}\nMCR N0\nLDI X7\nINC D60"),
            ),
        };
        let mut c = scenario(
            &format!("control_{label}_{op}"),
            &control_seed(op),
            &body,
            &format!(
                "{}\n{}\n{}\n{}\nMOV D60 D250\nMOV E1 D251\nMOV F1 D252",
                reads("D", 39, 10, 200),
                reads("D", 99, 7, 220),
                bit_reads("M", 128, 16, 230),
                bit_reads("M", 255, 16, 231)
            ),
        );
        if kind == 2 {
            c.stages = vec![
                (
                    vec![("X1", true), ("X0", false), ("X2", false), ("X7", false)],
                    1,
                ),
                (vec![("X1", false), ("X0", true)], 1),
                (vec![("X0", false), ("X2", true)], 1),
                (vec![("X0", true), ("X2", false)], 1),
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 1),
            ];
        }
        out.push(c);
    }
    for (op, active) in [
        ("SMOVP", false),
        ("SFWRP", true),
        ("SFRDP", false),
        ("ZRSTP", true),
    ] {
        let seed = format!(
            "ZRST S0 S1023\n{} S0\nRST M1040\nRST M1045\nRST M1047\n{}",
            if active { "SET" } else { "RST" },
            control_seed(op)
        );
        let body = format!(
            "STL S0\nLDI X7\n{op} {}\nRET\nLDI X7\nINC D60",
            controlled_operands(op)
        );
        out.push(scenario(
            &format!(
                "control_{}_STL_{op}",
                if active { "active" } else { "inactive" }
            ),
            &seed,
            &body,
            &format!(
                "{}\n{}\nMOV D60 D250",
                reads("D", 39, 10, 200),
                reads("D", 99, 7, 220)
            ),
        ));
    }
    assert_eq!(out.len(), 274);
}

fn capacity_cases(out: &mut Vec<ComparisonCase>) {
    for op in ["SFTR", "SFTRP", "SFTL", "SFTLP"] {
        // All addresses reside entirely in the high M bank; no 1535 gap.
        let seed =
            "MOV K0 E0\nMOV K0 F0\nZRST M2048 M4095\nSET M2048\nSET M3071\nSET M3072\nSET M4095";
        let body = format!("{op} M3072E0 M2048F0 K1024 H400");
        out.push(scenario(
            &format!("maximum_raw1024_{op}"),
            seed,
            &body,
            "MOV K4M2048 D220\nMOV K4M3056 D221\nMOV K4M3072 D222\nMOV K4M4080 D223",
        ));
    }
    for op in ["WSFR", "WSFRP", "WSFL", "WSFLP"] {
        let seed = "MOV K0 E0\nMOV K0 F0\nFMOV K7 D1500 K512\nFMOV K9 D2500 K512\nMOV K91 D1499\nMOV K92 D2012\nMOV K93 D2499\nMOV K94 D3012";
        out.push(scenario(&format!("maximum_word512_{op}"),seed,&format!("{op} D2500E0 D1500F0 H200 H200"),"MOV D1499 D220\nMOV D1500 D221\nMOV D1756 D222\nMOV D2011 D223\nMOV D2012 D224\nMOV D2499 D225\nMOV D3011 D226\nMOV D3012 D227"));
    }
    for op in ["SFWR", "SFWRP", "SFRD", "SFRDP"] {
        let seed = "MOV K0 E0\nMOV K0 F0\nFMOV K7 D1500 K512\nMOV K510 D1500\nMOV K91 D1499\nMOV K92 D2012\nMOV K9 D40\nMOV K55 D2011\nMOV K77 D50";
        let args = if base(op) == "SFWR" {
            "D40E0 D1500F0 H200"
        } else {
            "D1500E0 D50F0 H200"
        };
        out.push(scenario(&format!("maximum_fifo512_{op}"),seed,&format!("{op} {args}"),"MOV D1499 D220\nMOV D1500 D221\nMOV D1501 D222\nMOV D1756 D223\nMOV D2010 D224\nMOV D2011 D225\nMOV D2012 D226\nMOV D50 D227"));
    }
    assert_eq!(out.len(), 286);
}

// The published matrix_bulk predecessor/SER/SORT protocol supplies a bounded
// public witness of the private WORD scratch. It is not read via E0/F0.
// No literal SORT-result promise is inferred here; the parent compares native
// outputs and completion/progress against the model across all these phases.
fn scratch_cases(out: &mut Vec<ComparisonCase>) {
    for (op, variant) in [
        ("SMOV", "decimal"),
        ("SMOV", "hex"),
        ("SMOV", "invalid"),
        ("SMOVP", "decimal"),
        ("SMOVP", "hex"),
        ("SMOVP", "invalid"),
        ("WSFR", "source_base"),
        ("WSFRP", "source_base"),
        ("WSFL", "source_base"),
        ("WSFLP", "source_base"),
        ("SFWR", "full"),
        ("SFWRP", "full"),
        ("SFWR", "negative_header"),
        ("SFWRP", "negative_header"),
    ] {
        let mut seed = format!(
            "MOV K0 E0\nMOV K0 F0\n{}\n{}\nMOV K7 D750\nMOV K1234 D20\nMOV K2345 D21\n{}\nRST M1029",
            words("D", 400, &[30, 10, 20, 3, 1, 2]),
            words("D", 500, &[-777; 6]),
            words("D", 100, &[3, 11, 22, 33, 44])
        );
        if variant == "hex" {
            seed.push_str("\nSET M1168\nMOV H1234 D20");
        }
        if variant == "invalid" {
            seed.push_str("\nMOV K-1 D20");
        }
        if variant == "negative_header" {
            seed.push_str("\nMOV K-1 D100");
        }
        let args = match base(op) {
            "SMOV" => "D20E0 K4 K2 D101F0 K3",
            "WSFR" | "WSFL" => "D20E0 D100F0 K4 K2",
            "SFWR" => "D20E0 D100F0 K4",
            _ => unreachable!(),
        };
        let body = format!(
            "LD X4\nSER D750 K3 D760 K1\nLD X2\n{op} {args}\nLD X0\nSORT D400 K3 K2 D500 K2\nOUT Y10\nLDI X7\nMOV D500 D230\nMOV D501 D231\nMOV D502 D232\nMOV D503 D233\nMOV D504 D234\nMOV D505 D235\nLD M1029\nOUT Y11"
        );
        let mut c = scenario(
            &format!("private_scratch_SORT_{op}_{variant}"),
            &seed,
            &body,
            &reads("D", 100, 5, 240),
        );
        c.stages = vec![
            (
                vec![
                    ("X1", true),
                    ("X4", true),
                    ("X2", false),
                    ("X0", false),
                    ("X7", false),
                ],
                1,
            ),
            (vec![("X1", false), ("X4", false), ("X0", true)], 1),
            (vec![("X4", true), ("X2", true)], 3),
            (vec![("X2", false)], 1),
            (vec![("X0", false), ("X4", false)], 1),
            (vec![("X0", true), ("X2", true), ("X4", true)], 3),
            (vec![("X2", false)], 1),
        ];
        out.push(c);
    }
    assert_eq!(out.len(), 300);
}
// Native special-resolver scratch is the bit-coordinate, not canonical M0=0.
// K1M0E0 leaves 0x800 (2048). A small 2x2 word SORT continuation distinguishes
// that positive scratch from an incorrect zero without a large table access.
// SER runs only during initialization: held P must preserve its first visit's
// scratch instead of having a per-scan seed obscure the addressed predecessor.
fn group_scratch_cases(out: &mut Vec<ComparisonCase>) {
    for op in ["WSFR", "WSFRP", "WSFL", "WSFLP"] {
        let seed = format!(
            "MOV K0 E0\nMOV K0 F0\nMOV K3 K1M0\n{}\n{}\n{}\nMOV K7 D750\nRST M1029\nSER D750 K0 D760 K1",
            groups("M", 128, 1, &[1, 2]),
            words("D", 400, &[30, 10, 3, 1]),
            words("D", 499, &[-777; 8]),
        );
        let body =
            format!("LD X0\n{op} K1M0E0 K1M128F0 K2 K1\nLD X0\nSORT D400 K2 K2 D500 K1\nOUT Y10");
        out.push(scenario(
            &format!("private_scratch_group_coordinate_SORT_{op}"),
            &seed,
            &body,
            &format!(
                "{}\nMOV K1M0 D240\nMOV K2M128 D241\nLD M1029\nOUT Y11",
                reads("D", 499, 8, 230),
            ),
        ));
    }
    assert_eq!(out.len(), 304);
}
const COMMON_COUNT: usize = 304;

pub(super) fn model_cases(target: Target) -> Vec<ComparisonCase> {
    let last: u16 = match target {
        Target::Es2 => 9999,
        Target::Ss2 => 4999,
    };
    let mut out = Vec::new();
    for op in FORMS {
        let mut seed = format!(
            "MOV K0 E0\nMOV K0 F0\nMOV K-1 E1\nMOV K-3 F1\nMOV K-15 E2\nMOV K-15 F2\n{}\n{}\n{}",
            words("D", last - 4, &[91, 1, 1234, 23, 5678]),
            words("T", 251, &[91, 1, 1234, 23, 5678]),
            words("C", 195, &[91, 1, 1234, 23, 5678])
        );
        let mut observe = vec![
            reads("D", last - 4, 5, 200),
            reads("T", 251, 5, 210),
            reads("C", 195, 5, 220),
        ];
        let body = match base(op) {
            "SMOV" => {
                seed.push_str("\nMOV K1234 D20\nMOV K5678 D21");
                format!(
                    "LD X0\n{op} D{last}E0 K4 K2 D21F0 K3\n{op} C199E0 K4 K2 D{last}F0 K3\n{op} D20E0 K4 K2 T255F0 K3"
                )
            }
            "SFTR" | "SFTL" => {
                seed.push_str(&format!(
                    "\nMOV K-4 F1\n{}\n{}\n{}",
                    bits("Y", 249, 7, 0x55),
                    bits("S", 1017, 7, 0x55),
                    bits("M", 4088, 8, 0xaa)
                ));
                observe.extend([
                    "MOV K1Y374 D230".into(),
                    "MOV K1S1020 D231".into(),
                    "MOV K2M4088 D232".into(),
                ]);
                format!("LD X0\n{op} X377E1 Y377F1 K5 K2\n{op} M4095E1 S1023F1 K5 K2")
            }
            "WSFR" | "WSFL" => {
                seed.push_str(&format!(
                    "\nMOV K-4 F1\nMOV K-7 E2\nMOV K-11 F2\n{}\n{}",
                    bits("M", 4088, 8, 0x36),
                    bits("S", 1012, 12, 0x421)
                ));
                observe.push("MOV K3S1012 D230".into());
                format!(
                    "LD X0\n{op} T255E1 D{last}F1 K5 K2\n{op} C199E1 T255F1 K5 K2\n{op} K1M4095E2 K1S1023F2 K3 K2"
                )
            }
            "SFWR" => {
                seed.push_str(&format!(
                    "\nMOV K0 T252\nMOV K0 C196\n{}\n{}",
                    bits("M", 4080, 16, 0x1234),
                    groups("S", 1008, 1, &[0, 2, 4, 8])
                ));
                observe.push("MOV K4S1008 D230".into());
                format!(
                    "LD X0\n{op} D{last}E0 T255F1 K4\n{op} D{last}E0 C199F1 K4\n{op} K4M4095E2 K1S1023F2 K4"
                )
            }
            "SFRD" => {
                seed.push_str(&format!(
                    "\nMOV K3 D{}\nMOV K3 C196\n{}\n{}",
                    last - 3,
                    groups("S", 1008, 1, &[3, 2, 4, 8]),
                    bits("M", 4080, 16, 0xabcd)
                ));
                observe.push("MOV K4S1008 D230\nMOV K4M4080 D231".into());
                format!(
                    "LD X0\n{op} D{last}F1 T255E0 K4\n{op} C199F1 D{last}E0 K4\n{op} K1S1023F2 K4M4095E2 K4"
                )
            }
            "ZRST" => {
                seed.push_str(&format!(
                    "\nDMOV H12345678 C200\nDMOV H76543210 C255\n{}\n{}\n{}",
                    bits("Y", 252, 4, 15),
                    bits("S", 1020, 4, 15),
                    bits("M", 4092, 4, 15)
                ));
                observe.push("DMOV C200 D230\nDMOV C255 D232\nMOV K1Y374 D234\nMOV K1S1020 D235\nMOV K1M4092 D236".into());
                format!(
                    "LD X0\n{op} D{last}E1 D{last}F0\n{op} T255E1 T255F0\n{op} C199E0 C255F0\n{op} Y377E1 Y377F0\n{op} S1023E1 S1023F0\n{op} M4095E1 M4095F0"
                )
            }
            _ => unreachable!(),
        };
        let mut c = scenario(
            &format!("model_{}_complete_ends_{op}", target.name()),
            &seed,
            &body,
            &observe.join("\n"),
        );
        c.stages[0].0.extend([("X376", true), ("X377", false)]);
        c.stages[2].0.extend([("X376", false), ("X377", true)]);
        out.push(c);
    }
    check(&out, 16);
    out
}
