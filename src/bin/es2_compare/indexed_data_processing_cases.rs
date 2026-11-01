//! Draft addressed DECO/ENCO/SUM/BON/MEAN corpus. No runtime claim.
//! common_cases + model_cases are native-candidate lists with complete spans.
//! documented_partial_cases is LOCAL ONLY and must never be sent to native.
use super::{Case as ComparisonCase, Target, case};
use std::collections::BTreeSet;

const FORMS: [&str; 16] = [
    "DECO", "DECOP", "ENCO", "ENCOP", "SUM", "SUMP", "DSUM", "DSUMP", "BON", "BONP", "DBON",
    "DBONP", "MEAN", "MEANP", "DMEAN", "DMEANP",
];
const INPUTS: [&str; 64] = [
    "X100", "X101", "X102", "X103", "X104", "X105", "X106", "X107", "X110", "X111", "X112", "X113",
    "X114", "X115", "X116", "X117", "X120", "X121", "X122", "X123", "X124", "X125", "X126", "X127",
    "X130", "X131", "X132", "X133", "X134", "X135", "X136", "X137", "X140", "X141", "X142", "X143",
    "X144", "X145", "X146", "X147", "X150", "X151", "X152", "X153", "X154", "X155", "X156", "X157",
    "X160", "X161", "X162", "X163", "X164", "X165", "X166", "X167", "X170", "X171", "X172", "X173",
    "X174", "X175", "X176", "X177",
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
fn wide(op: &str) -> bool {
    matches!(base(op), "DSUM" | "DBON" | "DMEAN")
}
fn mov(w: bool) -> &'static str {
    if w { "DMOV" } else { "MOV" }
}
fn addr(area: &str, i: u16) -> String {
    if matches!(area, "X" | "Y") {
        format!("{area}{i:o}")
    } else {
        format!("{area}{i}")
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
fn values(area: &str, start: u16, wide: bool, vs: &[i32]) -> String {
    vs.iter()
        .enumerate()
        .map(|(i, v)| {
            format!(
                "{} K{v} {area}{}",
                mov(wide),
                start + i as u16 * if wide { 2 } else { 1 }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn reads(area: &str, start: u16, n: u16, out: u16) -> String {
    (0..n)
        .map(|i| format!("MOV {area}{} D{}", start + i, out + i))
        .collect::<Vec<_>>()
        .join("\n")
}
fn bits(area: &str, start: u16, n: u16, value: u64) -> String {
    (0..n)
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
fn groups(area: &str, start: u16, digits: u16, vs: &[u32]) -> String {
    vs.iter()
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
fn bit_reads(area: &str, start: u16, n: u16, out: u16) -> String {
    assert_eq!(n % 4, 0);
    (0..n.div_ceil(16))
        .map(|i| {
            format!(
                "MOV K{}{} D{}",
                (n - i * 16).min(16) / 4,
                addr(area, start + i * 16),
                out + i
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn with_inputs(mut c: ComparisonCase) -> ComparisonCase {
    for ((changes, _), pattern) in c.stages.iter_mut().zip([
        0x1234_5678_9abc_def0u64,
        0x8000_0007_0000_0003,
        0x5555_aaaa_1111_3333,
        0x0000_0009_0000_0005,
    ]) {
        for (i, name) in INPUTS.into_iter().enumerate() {
            changes.push((name, pattern & (1 << i) != 0));
        }
    }
    c
}
fn index_alias(bank: char, n: u16) -> u16 {
    (if n == 0 { 1028 } else { 1180 + 2 * n }) + u16::from(bank == 'F')
}
fn scenario(name: &str, seed: &str, body: &str, observe: &str) -> ComparisonCase {
    let gate = if body.starts_with("LD") || body.starts_with("STL") {
        ""
    } else {
        "LD X0"
    };
    let source=[
        "LD X1\nMOV K0 E0\nMOV K0 F0\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV H1234 D1067\nMOV K77 D1068\nRST M790\nRST M791",
        seed,gate,body,
        "LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24\nLDI X7\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305\nMOV K1234 D306",
        observe,"END",
    ].into_iter().filter(|s|!s.is_empty()).collect::<Vec<_>>().join("\n");
    assert!(
        source.lines().count() <= 512,
        "source instruction budget {name}"
    );
    assert!(
        source.lines().count() * 13 < 0x1ef0,
        "source WORD budget {name}"
    );
    assert!(
        source.lines().any(|line| {
            let fs: Vec<_> = line.split_whitespace().collect();
            fs.first().is_some_and(|x| FORMS.contains(x))
                && fs[1..].iter().any(|v| {
                    let b = v.as_bytes();
                    b.len() >= 4
                        && matches!(b[b.len() - 2], b'E' | b'F')
                        && b[b.len() - 1].is_ascii_digit()
                })
        }),
        "no modified family role {name}"
    );
    // Avoid a bare trailing LD/LDI observation before END. Custom observation
    // blocks include real actions after their conditions.
    let lines: Vec<_> = source.lines().collect();
    assert!(
        !lines[lines.len() - 2].starts_with("LD"),
        "dangling final condition {name}"
    );
    case(
        &format!("indexed_data_processing_{name}"),
        &source,
        stages(),
    )
}
fn check(cs: &[ComparisonCase], n: usize) {
    assert_eq!(cs.len(), n);
    let names: BTreeSet<_> = cs.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names.len(), n);
}
pub(super) fn common_cases() -> Vec<ComparisonCase> {
    let mut cs = Vec::new();
    bank_cases(&mut cs);
    packed_cases(&mut cs);
    raw_cases(&mut cs);
    role_cases(&mut cs);
    count_cases(&mut cs);
    literal_cases(&mut cs);
    control_cases(&mut cs);
    maximum_cases(&mut cs);
    let actual: BTreeSet<_> = cs
        .iter()
        .flat_map(|c| c.source.lines())
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    for op in FORMS {
        assert!(actual.contains(op), "missing {op}");
    }
    check(&cs, COMMON_COUNT);
    cs
}

// Two independent sites per pair reverse E/F source/target/count use. Thus
// EACH spelling and each gray scalar role visits every modifier bank.
fn bank_cases(cs: &mut Vec<ComparisonCase>) {
    for op in FORMS {
        for b in 0..8 {
            let w = wide(op);
            let m = mov(w);
            let mut seed = format!(
                "MOV K1 E{b}\nMOV K-1 F{b}\n{}\n{}\n{m} K0 D61\n{m} K0 D65\n{m} K77 D100\n{m} K88 D121\n{}\n{}",
                values("D", 41, w, &[4, -1]),
                values("D", 79, w, &[8, 2]),
                bits("M", 128, 4, 0xb),
                bits("M", 159, 4, 0xd)
            );
            let body = match base(op) {
                "DECO" => {
                    seed.push_str("\nMOV K3 D41\nMOV K5 D79");
                    format!("{op} D40E{b} D101F{b} K4\n{op} D80F{b} D120E{b} H4\nINC D41\nINC D79")
                }
                "ENCO" => {
                    seed.push_str("\nMOV H21 D41\nMOV H14 D79");
                    format!(
                        "{op} D40E{b} D101F{b} K4\n{op} D80F{b} D120E{b} H4\nMOV H8001 D41\nMOV H4001 D79"
                    )
                }
                "SUM" | "DSUM" => {
                    seed.push_str(&format!("\n{m} H8003 D41\n{m} H7005 D79"));
                    format!(
                        "{op} D40E{b} D101F{b}\n{op} D80F{b} D120E{b}\n{m} H8007 D41\n{m} H7007 D79"
                    )
                }
                "BON" | "DBON" => {
                    seed.push_str(&format!("\n{m} H8005 D41\n{m} H800A D79"));
                    format!(
                        "{op} D40E{b} M128E{b} D62F{b}\n{op} D80F{b} M161F{b} D64E{b}\nINC D61\nINC D65"
                    )
                }
                "MEAN" | "DMEAN" => {
                    seed.push_str(&format!("\n{m} K2 D61\n{m} K2 D65"));
                    format!(
                        "{op} D40E{b} D101F{b} D62F{b}\n{op} D80F{b} D120E{b} D64E{b}\nINC D41\nINC D79"
                    )
                }
                _ => unreachable!(),
            };
            cs.push(scenario(&format!("banks_{op}_E{b}_F{b}"),&seed,&body,&format!("{}\n{}\n{m} D100 D220\n{m} D121 D222\n{m} D61 D224\n{m} D65 D226\nMOV K1M128 D228\nMOV K1M159 D229\nMOV E{b} D280\nMOV F{b} D281",reads("D",40,6,200),reads("D",78,6,210))));
        }
    }
    assert_eq!(cs.len(), 128);
}

fn packed_cases(cs: &mut Vec<ComparisonCase>) {
    for op in [
        "SUM", "SUMP", "DSUM", "DSUMP", "BON", "BONP", "DBON", "DBONP", "MEAN", "MEANP", "DMEAN",
        "DMEANP",
    ] {
        let w = wide(op);
        let m = mov(w);
        let max = if w { 8 } else { 4 };
        for digits in 1..=max {
            let mut seed = format!("MOV K0 E0\nMOV K0 F0\n{m} K3 D50");
            let mut body = Vec::new();
            let mut observe = Vec::new();
            for (i, area) in ["Y", "M", "S", "X"].into_iter().enumerate() {
                if area != "X" {
                    seed.push_str(&format!(
                        "\n{}",
                        groups(area, 64, digits, &[0x8000_000f, 0x7fff_fff1])
                    ));
                }
                let target = if i < 3 {
                    addr(area, 160)
                } else {
                    "M200".into()
                };
                if matches!(base(op), "BON" | "DBON") {
                    seed.push_str(&format!("\nSET {target}"));
                }
                if matches!(base(op), "MEAN" | "DMEAN") {
                    seed.push_str(&format!(
                        "\n{m} K2 D50\n{m} H55AA K{}{}",
                        max + 1 - digits,
                        if i < 3 {
                            addr(area, 160)
                        } else {
                            "M224".into()
                        }
                    ));
                }
                let source = format!("K{digits}{}E0", addr(area, 64));
                let statement = match base(op) {
                    "SUM" | "DSUM" => format!("{op} {source} D{}F0", 100 + i * 4),
                    "BON" | "DBON" => format!("{op} {source} {target}F0 D50E0"),
                    "MEAN" | "DMEAN" => format!(
                        "{op} {source} K{}{}F0 D50E0",
                        max + 1 - digits,
                        if i < 3 {
                            addr(area, 160)
                        } else {
                            "M224".into()
                        }
                    ),
                    _ => unreachable!(),
                };
                body.push(format!("LD X0\n{statement}"));
                observe.push(if matches!(base(op), "SUM" | "DSUM") {
                    format!("{m} D{} D{}", 100 + i * 4, 220 + i * 4)
                } else if matches!(base(op), "BON" | "DBON") {
                    format!(
                        "LD {target}\nOUT M{}\nLDI X7\nMOV K1{} D{}",
                        810 + i,
                        if i < 3 {
                            addr(area, 159)
                        } else {
                            "M199".into()
                        },
                        220 + i
                    )
                } else {
                    format!(
                        "{m} K{}{} D{}",
                        max + 1 - digits,
                        if i < 3 {
                            addr(area, 160)
                        } else {
                            "M224".into()
                        },
                        220 + i * 4
                    )
                });
            }
            observe.push("LDI X7\nMOV K1234 D290".into());
            cs.push(with_inputs(scenario(
                &format!("packed_{op}_K{digits}_XYMS"),
                &seed,
                &body.join("\n"),
                &observe.join("\n"),
            )));
        }
    }
    assert_eq!(cs.len(), 200);
}

fn raw_cases(cs: &mut Vec<ComparisonCase>) {
    for op in ["DECO", "DECOP", "ENCO", "ENCOP"] {
        for area in ["X", "Y", "M", "S"] {
            let mut seed = String::from("MOV K1 E0\nMOV K-1 F0\nMOV K77 D100\nMOV K88 D110");
            if area != "X" {
                seed.push_str(&format!("\n{}", bits(area, 64, 16, 0xa53b)));
            }
            let mut body = Vec::new();
            let mut observe = Vec::new();
            if base(op) == "DECO" {
                for (i, da) in ["Y", "M", "S"].into_iter().enumerate() {
                    seed.push_str(&format!("\n{}", bits(da, 127, 12, 0xfff)));
                    body.push(format!(
                        "LD X0\n{op} {}E0 {}F0 K3",
                        addr(area, 64),
                        addr(da, 129)
                    ));
                    observe.push(bit_reads(da, 127, 12, 200 + i as u16 * 2));
                }
                body.push(format!("LD X0\n{op} {}E0 D101F0 K3", addr(area, 64)));
            } else {
                body.push(format!(
                    "LD X0\n{op} {}E0 D101F0 H3\n{op} {} D110E0 K3",
                    addr(area, 64),
                    addr(area, 64)
                ));
            }
            observe.push("LDI X7\nMOV D100 D220\nMOV D111 D221".into());
            cs.push(with_inputs(scenario(
                &format!("raw_{op}_{area}_source"),
                &seed,
                &body.join("\n"),
                &observe.join("\n"),
            )));
        }
    }
    assert_eq!(cs.len(), 216);
}

fn role_cases(cs: &mut Vec<ComparisonCase>) {
    for op in FORMS {
        let w = wide(op);
        let m = mov(w);
        let mut body = Vec::new();
        let mut observe = Vec::new();
        let roles: Vec<&str> = if w {
            vec!["T20", "C198", "D20", "E1"]
        } else {
            vec!["T20", "C20", "D20", "E1", "F1"]
        };
        for (i, role) in roles.into_iter().enumerate() {
            let modified = if role.starts_with(['E', 'F']) {
                role.to_string()
            } else {
                format!("{role}E0")
            };
            let setup = format!("{m} H8005 {role}\n{m} K77 D200\n{m} K2 D180");
            let statement = match base(op) {
                "DECO" | "ENCO" => format!(
                    "{op} {modified} D200F0 K4\nLDI X7\nMOV D200 D{}\nMOV K3 D40\nLD X0\n{op} D40E0 {role} K4",
                    220 + i * 4
                ),
                "SUM" | "DSUM" => format!(
                    "{op} {modified} D200F0\nLDI X7\n{m} D200 D{}\n{m} H8007 D40\nLD X0\n{op} D40F0 {role}",
                    220 + i * 4
                ),
                "BON" | "DBON" => format!(
                    "{op} {modified} {}F0 D180E0",
                    addr(["Y", "M", "S"][i % 3], 128 + i as u16 * 4)
                ),
                "MEAN" | "DMEAN" => format!("{op} D40E0 {role} D180F0"),
                _ => unreachable!(),
            };
            body.push(format!("LDI X7\n{setup}\nLD X0\n{statement}"));
            observe.push(format!("LDI X7\n{m} {role} D{}", 250 + i * 3));
        }
        if matches!(base(op), "DECO" | "SUM" | "DSUM" | "BON" | "DBON") {
            let args = match base(op) {
                "DECO" => "HFFFF D210E0 K4",
                "SUM" | "DSUM" => "K-1 D210E0",
                _ => "K-1 M240E0 H0",
            };
            body.push(format!("LD X0\n{op} {args}"));
            observe.push("LDI X7\nMOV D210 D278\nMOV K1M240 D279".into());
        }
        let seed = format!(
            "MOV K0 E0\nMOV K0 F0\n{}\nMOV K0 D210\nMOV K0 D211",
            values("D", 40, w, &[4, -1, 8, 2])
        );
        observe.push("LDI X7\nMOV K1234 D290".into());
        cs.push(scenario(
            &format!("plain_and_modified_TCDEF_roles_{op}"),
            &seed,
            &body.join("\n"),
            &observe.join("\n"),
        ));
    }
    assert_eq!(cs.len(), 232);
}

fn count_cases(cs: &mut Vec<ComparisonCase>) {
    for op in [
        "BON", "BONP", "DBON", "DBONP", "MEAN", "MEANP", "DMEAN", "DMEANP",
    ] {
        let w = wide(op);
        let m = mov(w);
        let is_mean = base(op).ends_with("MEAN");
        let mut body = Vec::new();
        for (i, role) in ["T180", "C180", "D180", "E3", "F3"].into_iter().enumerate() {
            let store = if w && role == "F3" { "D1187" } else { role };
            let n = if role.starts_with(['E', 'F']) {
                role.to_string()
            } else {
                format!("{role}F0")
            };
            let target = if is_mean {
                format!("D{}E0", 200 + i * 4)
            } else {
                format!("M{}E0", 256 + i * 4)
            };
            body.push(format!(
                "LDI X7\n{m} K2 {store}\nLD X0\n{op} D40E0 {target} {n}"
            ));
        }
        if is_mean {
            for digits in 1..=if w { 8 } else { 4 } {
                for (j, area) in ["Y", "M", "S", "X"].into_iter().enumerate() {
                    let operand = format!("K{digits}{}", addr(area, 64));
                    if area != "X" {
                        body.push(format!("LDI X7\n{m} K2 {operand}"));
                    }
                    body.push(format!("LD X0\n{op} D40E0 D{}F0 {operand}E0", 230 + j * 4));
                }
            }
        }
        let seed = "MOV K0 E0\nMOV K0 F0\nFMOV K6 D40 K128\nFMOV K77 D200 K48";
        let mut c = scenario(
            &format!("all_count_roles_{op}"),
            seed,
            &body.join("\n"),
            &format!(
                "{}\nMOV K4M256 D280\nMOV K1234 D290",
                reads("D", 200, 16, 260)
            ),
        );
        for ((changes, _), pattern) in c.stages.iter_mut().zip([2u64, 3, 2, 4]) {
            for (i, name) in INPUTS.into_iter().enumerate() {
                changes.push((name, pattern & (1 << i) != 0));
            }
        }
        cs.push(c);
    }
    for op in ["DBON", "DBONP", "DMEAN", "DMEANP"] {
        let mut body = Vec::new();
        for (i, n) in [0u16, 1, 7].into_iter().enumerate() {
            let alias = index_alias('F', n);
            let target = if base(op) == "DBON" {
                format!("M{}E6", 400 + i * 4)
            } else {
                format!("D{}E6", 200 + i * 8)
            };
            body.push(format!("LDI X7\nMOV K2 F{n}\nMOV K0 D{}\nLD X0\n{op} D40E6 {target} F{n}\nLDI X7\nMOV D1067 D{}\nMOV D1068 D{}\nMOV K1 D{}\nLD X0\n{op} D40E6 {target} F{n}\nLDI X7\nMOV D1067 D{}\nMOV D1068 D{}",alias+1,240+i*4,241+i*4,alias+1,242+i*4,243+i*4));
        }
        cs.push(scenario(&format!("wide_F_count_F0_F1_F7_full_high_word_{op}"),&format!("MOV K0 E6\n{}\nDMOV K77 D200\nDMOV K88 D208\nDMOV K99 D216\nSET M400\nSET M404\nSET M408",values("D",40,true,&[4,-1,8,2])),&body.join("\n"),"LDI X7\nDMOV D200 D260\nDMOV D208 D262\nDMOV D216 D264\nMOV K3M400 D266\nMOV D1030 D267\nMOV D1184 D268\nMOV D1196 D269"));
    }
    assert_eq!(cs.len(), 244);
}

fn oracle_block(op: &str, args: &str, setup: &str, on: &str, off: &str, marker: u16) -> String {
    format!(
        "LDI X7\nMOV K0 E0\nMOV K0 F0\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV H1234 D1067\nMOV K77 D1068\n{setup}\nLD X0\n{op} {args}\nLD M791\n{on}\nLDI M791\n{off}\nORB\nOUT M{marker}"
    )
}
fn oracle_case(name: &str, op: &str, blocks: Vec<String>, observe: &str) -> ComparisonCase {
    let body = format!(
        "LD X0\n{}OUT M791\n{}\nLD X0\nOUT M790",
        if op.ends_with('P') { "ANI M790\n" } else { "" },
        blocks.join("\n")
    );
    scenario(&format!("literal_oracle_{name}_{op}"), "", &body, observe)
}
fn literal_cases(cs: &mut Vec<ComparisonCase>) {
    for op in ["DECO", "DECOP"] {
        cs.push(oracle_case(
            "mask_snapshot_and_exact_raw_count",
            op,
            vec![
                oracle_block(
                    op,
                    "D10E0 D20F0 K3",
                    "MOV HFFFB D10\nMOV H5AA5 D20",
                    "AND= D20 K8\nAND= D10 HFFFB\nAND M1020\nANI M1021\nAND M1022",
                    "AND= D20 H5AA5",
                    840,
                ),
                oracle_block(
                    op,
                    "M128E0 M128F0 K3",
                    &bits("M", 127, 12, 0xff7),
                    "AND= K3M127 HE11",
                    "AND= K3M127 HFF7",
                    841,
                ),
            ],
            "MOV D20 D220\nMOV K3M127 D221",
        ));
        cs.push(oracle_case(
            "late_index_and_numeric_full_clear",
            op,
            vec![
                oracle_block(
                    op,
                    "K3 D1182E1 K4",
                    "MOV K0 E1\nMOV K77 F1\nMOV K88 D1190",
                    "AND= E1 K8\nAND= F1 K77\nAND= D1190 K88",
                    "AND= E1 K0\nAND= D1190 K88",
                    840,
                ),
                oracle_block(
                    op,
                    "M128E0 D20F0 K1",
                    "SET M128\nMOV HFFFF D20",
                    "AND= D20 K2",
                    "AND= D20 HFFFF",
                    841,
                ),
            ],
            "MOV E1 D220\nMOV D20 D221",
        ));
    }
    for op in ["ENCO", "ENCOP"] {
        cs.push(oracle_case(
            "highest_mask_and_empty_error",
            op,
            vec![
                oracle_block(
                    op,
                    "D10E0 D20F0 K3",
                    "MOV HFF08 D10\nMOV K77 D20",
                    "AND= D20 K3\nANI M1067\nAND M1020\nANI M1021\nAND M1022",
                    "AND= D20 K77",
                    840,
                ),
                oracle_block(
                    op,
                    "D10E0 D20F0 K3",
                    "MOV HFF00 D10\nMOV K77 D20",
                    "AND= D20 K77\nAND= D1067 HE1A\nAND M1067\nAND M1068",
                    "AND= D20 K77\nAND= D1067 H1234\nANI M1067",
                    841,
                ),
            ],
            "MOV D20 D220\nMOV D1068 D221",
        ));
        cs.push(oracle_case(
            "raw_highest_and_source_flag_snapshot",
            op,
            vec![
                oracle_block(
                    op,
                    "M128E0 D20F0 K3",
                    &format!("{}\nMOV K77 D20", bits("M", 127, 12, 0xb93)),
                    "AND= D20 K7\nAND= K3M127 HB93",
                    "AND= D20 K77",
                    840,
                ),
                oracle_block(
                    op,
                    "M1020 D20E0 K1",
                    "SET M1020\nSET M1021\nMOV K77 D20",
                    "AND= D20 K1\nAND M1020\nAND M1021\nAND M1022",
                    "AND= D20 K77",
                    841,
                ),
            ],
            "MOV D20 D220\nMOV K3M127 D221",
        ));
        cs.push(oracle_case(
            "late_target_own_index_and_source_target_alias",
            op,
            vec![
                oracle_block(
                    op,
                    "D10E0 D1182E1 K4",
                    "MOV H8001 D10\nMOV K0 E1\nMOV K77 F1\nMOV K88 D1197",
                    "AND= E1 K15\nAND= F1 K77\nAND= D1197 K88",
                    "AND= E1 K0\nAND= F1 K77",
                    840,
                ),
                oracle_block(
                    op,
                    "D20E0 D20F0 K4",
                    "MOV H4108 D20",
                    "AND= D20 K14",
                    "AND= D20 H4108",
                    841,
                ),
            ],
            "MOV E1 D220\nMOV D20 D221",
        ));
    }
    for op in ["SUM", "SUMP", "DSUM", "DSUMP"] {
        let w = wide(op);
        let m = mov(w);
        let cmp = if w { "DAND=" } else { "AND=" };
        let max = if w { 32 } else { 16 };
        let g = if w { 8 } else { 4 };
        cs.push(oracle_case(
            "zero_extension_full_population_and_sticky_zero",
            op,
            vec![
                oracle_block(
                    op,
                    "K1M128E0 D20F0",
                    "MOV K15 K1M128\nDMOV K77 D20",
                    &format!("{cmp} D20 K4\nAND M1020\nANI M1021\nAND M1022"),
                    &format!("{cmp} D20 K77"),
                    840,
                ),
                oracle_block(
                    op,
                    &format!("K{g}M128E0 D20F0"),
                    &format!("{m} K-1 K{g}M128\n{m} K77 D20\nRST M1020"),
                    &format!("{cmp} D20 K{max}\nANI M1020\nANI M1021\nAND M1022"),
                    &format!("{cmp} D20 K77\nANI M1020"),
                    841,
                ),
                oracle_block(
                    op,
                    "K0 D20E0",
                    &format!("{m} K77 D20\nRST M1020"),
                    &format!("{cmp} D20 K0\nAND M1020"),
                    &format!("{cmp} D20 K77\nANI M1020"),
                    842,
                ),
            ],
            "MOV D20 D220\nMOV D21 D221",
        ));
        cs.push(oracle_case(
            "own_index_pair_cache_and_overlap_snapshot",
            op,
            vec![
                oracle_block(
                    op,
                    "K-1 D1182E1",
                    "MOV K0 E1\nMOV K77 F1\nMOV K88 D1214\nMOV K99 D1215",
                    if w {
                        "AND= E1 K32\nAND= F1 K0\nAND= D1214 K88\nAND= D1215 K99"
                    } else {
                        "AND= E1 K16\nAND= F1 K77\nAND= D1214 K88"
                    },
                    "AND= E1 K0\nAND= F1 K77",
                    840,
                ),
                oracle_block(
                    op,
                    "D20E0 D20F0",
                    &format!("{m} H800F D20"),
                    &format!("{cmp} D20 K5"),
                    &format!("{cmp} D20 H800F"),
                    841,
                ),
            ],
            "MOV E1 D220\nMOV F1 D221\nMOV D20 D222\nMOV D21 D223",
        ));
    }
    for op in ["BON", "BONP", "DBON", "DBONP"] {
        let w = wide(op);
        let m = mov(w);
        let top = if w { 31 } else { 15 };
        let outside = top + 1;
        cs.push(oracle_case(
            "short_high_zero_exact_one_bit_and_flag_alias",
            op,
            vec![
                oracle_block(
                    op,
                    &format!("K1M128E0 M160F0 K{top}"),
                    "MOV K15 K1M128\nSET M159\nSET M160\nSET M161",
                    "AND M159\nANI M160\nAND M161\nAND M1020\nANI M1021\nAND M1022",
                    "AND M159\nAND M160\nAND M161",
                    840,
                ),
                oracle_block(
                    op,
                    "K1M1019 M1022E0 K0",
                    "MOV K10 K1M1019",
                    "ANI M1019\nAND M1020\nANI M1021\nANI M1022",
                    "AND= K1M1019 K10",
                    841,
                ),
            ],
            "MOV K1M159 D220\nMOV K1M1019 D221",
        ));
        cs.push(oracle_case(
            "signed_full_count_domain_error_holds_target",
            op,
            vec![
                oracle_block(
                    op,
                    "K-1 M160E0 D50F0",
                    &format!("{m} K-1 D50\nSET M160"),
                    "AND M160\nAND= D1067 HE1A\nAND M1067\nAND M1068\nAND M1020\nAND M1022",
                    "AND M160\nAND= D1067 H1234",
                    840,
                ),
                oracle_block(
                    op,
                    "K0 M160E0 D50F0",
                    &format!("{m} K{outside} D50\nSET M160"),
                    "AND M160\nAND= D1067 HE1A\nAND M1067",
                    "AND M160\nAND= D1067 H1234",
                    841,
                ),
                oracle_block(
                    op,
                    &format!("K-1 M160E0 H{top:X}"),
                    "RST M160\nSET M159\nSET M161",
                    "AND M160\nAND M159\nAND M161\nANI M1067",
                    "ANI M160\nAND M159\nAND M161",
                    842,
                ),
            ],
            "MOV D1068 D220\nMOV K1M159 D221",
        ));
    }
    for op in ["MEAN", "MEANP", "DMEAN", "DMEANP"] {
        let w = wide(op);
        let m = mov(w);
        let cmp = if w { "DAND=" } else { "AND=" };
        let result = if w { 2 } else { 1 };
        let g = if w { 8 } else { 4 };
        cs.push(oracle_case(
            "mixed_sign_algorithm_and_requested_count_snapshot",
            op,
            vec![
                oracle_block(
                    op,
                    "D20E0 D40F0 K2",
                    &format!("{}\n{m} K77 D40", values("D", 20, w, &[4, -1])),
                    &format!("{cmp} D40 K{result}\nAND M1020\nANI M1021\nAND M1022"),
                    &format!("{cmp} D40 K77"),
                    840,
                ),
                oracle_block(
                    op,
                    "D20E0 D40F0 D40E0",
                    &format!("{}\n{m} K2 D40", values("D", 20, w, &[8, 4])),
                    &format!("{cmp} D40 K6"),
                    &format!("{cmp} D40 K2"),
                    841,
                ),
                oracle_block(
                    op,
                    "D20E0 D20F0 K2",
                    &values("D", 20, w, &[8, 4]),
                    &format!("{cmp} D20 K6"),
                    &format!("{cmp} D20 K8"),
                    842,
                ),
            ],
            "MOV D20 D220\nMOV D21 D221\nMOV D40 D222\nMOV D41 D223",
        ));
        cs.push(oracle_case(
            "packed_stride_sign_and_target_flag_truncation",
            op,
            vec![
                oracle_block(
                    op,
                    "K1M128E0 D40F0 K2",
                    &format!("{}\n{m} K77 D40", groups("M", 128, 1, &[15, 1])),
                    &format!("{cmp} D40 K8"),
                    &format!("{cmp} D40 K77"),
                    840,
                ),
                oracle_block(
                    op,
                    &format!("K{g}M128E0 D40F0 K2"),
                    &format!("{}\n{m} K77 D40", groups("M", 128, g, &[u32::MAX, 3])),
                    &format!("{cmp} D40 K1"),
                    &format!("{cmp} D40 K77"),
                    841,
                ),
                oracle_block(
                    op,
                    "D20E0 K1M1019F0 K2",
                    &format!("{}\nMOV K3 K1M1019", values("D", 20, w, &[20, 28])),
                    "AND= K1M1019 K8\nANI M1020\nANI M1021\nAND M1022",
                    "AND= K1M1019 K3\nAND M1020",
                    842,
                ),
            ],
            "MOV K1M1019 D220\nMOV D40 D221\nMOV D41 D222",
        ));
        cs.push(oracle_case("count_zero_negative_and65_hold_target",op,[0,-1,65].into_iter().enumerate().map(|(i,n)|oracle_block(op,"D20E0 D40F0 D50E0",&format!("{}\n{m} K77 D40\n{m} K{n} D50",values("D",20,w,&[4,-1])),&format!("{cmp} D40 K77\nAND= D1067 HE1A\nAND M1067\nAND M1068\nAND M1020\nANI M1021\nAND M1022"),&format!("{cmp} D40 K77\nAND= D1067 H1234\nANI M1067"),840+i as u16)).collect(),"MOV D1068 D220\nMOV D40 D221\nMOV D41 D222"));
    }
    assert_eq!(cs.len(), 282);
}

fn control_args(op: &str) -> String {
    match base(op) {
        "DECO" | "ENCO" => "D40E1 D100F1 K4".into(),
        "SUM" | "DSUM" => "D40E1 D100F1".into(),
        "BON" | "DBON" => "D40E1 M160F1 D60E1".into(),
        "MEAN" | "DMEAN" => "D40E1 D100F1 D60E1".into(),
        _ => unreachable!(),
    }
}
fn control_seed(op: &str) -> String {
    let w = wide(op);
    let m = mov(w);
    format!(
        "MOV K1 E1\nMOV K0 F1\n{}\n{}\n{m} K2 D61\n{m} K2 D65\n{m} K77 D100\nMOV K0 D70\nSET M160",
        values("D", 41, w, &[4, 8]),
        values("D", 45, w, &[3, 11])
    )
}
fn control_cases(cs: &mut Vec<ComparisonCase>) {
    for (i, op) in FORMS.into_iter().enumerate() {
        let args = control_args(op);
        let kind = (i / 2 + i % 2) % 4;
        let (label, body) = match kind {
            0 => (
                "for_same_pc",
                format!("LD X0\nFOR K3\nLD X0\n{op} {args}\nNEXT\nLDI X7\nINC D70"),
            ),
            1 => (
                "mps_live_modifier_two_sites",
                format!(
                    "LD X0\nMPS\n{op} {args}\nMRD\nMOV K5 E1\nMRD\n{op} {args}\nMPP\nOUT Y10\nLDI X7\nMOV K1 E1\nINC D70"
                ),
            ),
            2 => (
                "cj_skip_off_history",
                format!("LD X2\nCJ P10\nLD X0\n{op} {args}\nP10\nLDI X7\nINC D70"),
            ),
            _ => (
                "master_false_true_accumulator",
                format!("LD X7\nMC N0\nLDI X7\n{op} {args}\nMCR N0\nLDI X7\nINC D70"),
            ),
        };
        let mut c = scenario(
            &format!("control_{label}_{op}"),
            &control_seed(op),
            &body,
            "MOV D100 D220\nMOV D101 D221\nMOV K1M159 D222\nMOV D70 D223\nMOV E1 D224",
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
        cs.push(c);
    }
    for (op, active) in [
        ("DECOP", false),
        ("ENCOP", true),
        ("DBONP", false),
        ("DMEANP", true),
    ] {
        let seed = format!(
            "ZRST S0 S1023\n{} S0\nRST M1040\nRST M1045\nRST M1047\n{}",
            if active { "SET" } else { "RST" },
            control_seed(op)
        );
        let body = format!(
            "STL S0\nLDI X7\n{op} {}\nRET\nLDI X7\nINC D70",
            control_args(op)
        );
        cs.push(scenario(
            &format!(
                "control_{}_STL_{op}",
                if active { "active" } else { "inactive" }
            ),
            &seed,
            &body,
            "MOV D100 D220\nMOV D101 D221\nMOV K1M159 D222\nMOV D70 D223",
        ));
    }
    assert_eq!(cs.len(), 302);
}

fn maximum_cases(cs: &mut Vec<ComparisonCase>) {
    for op in ["DECO", "DECOP", "ENCO", "ENCOP"] {
        let seed = "MOV K0 E0\nMOV K0 F0\nMOV K255 D20\nMOV K77 D100\nZRST M2048 M2304\nSET M2048\nSET M2176\nSET M2303\nSET M2304";
        let body = if base(op) == "DECO" {
            format!("{op} D20E0 M2048F0 H8")
        } else {
            format!("{op} M2048E0 D100F0 H8")
        };
        cs.push(scenario(&format!("maximum_raw256_{op}"),seed,&body,"MOV K4M2048 D220\nMOV K4M2160 D221\nMOV K4M2288 D222\nLD M2304\nOUT M810\nLDI X7\nMOV D100 D223"));
    }
    for op in ["MEAN", "MEANP", "DMEAN", "DMEANP"] {
        let w = wide(op);
        let m = mov(w);
        let seed = format!(
            "MOV K0 E0\nMOV K0 F0\n{}\n{m} K64 D180\n{m} K77 D200",
            values("D", 40, w, &[6; 64])
        );
        cs.push(scenario(
            &format!("maximum_requested64_{op}"),
            &seed,
            &format!("{op} D40E0 D200F0 D180E0"),
            &format!("{m} D200 D220\n{m} D180 D222\nMOV D39 D224\nMOV D168 D225"),
        ));
    }
    assert_eq!(cs.len(), 310);
}
const COMMON_COUNT: usize = 310;

pub(super) fn model_cases(target: Target) -> Vec<ComparisonCase> {
    let last: u16 = match target {
        Target::Es2 => 9999,
        Target::Ss2 => 4999,
    };
    let mut cs = Vec::new();
    for op in FORMS {
        let w = wide(op);
        let m = mov(w);
        let width = if w { 32 } else { 16 };
        let digits = width / 4;
        let mut seed = format!(
            "MOV K0 E0\nMOV K0 F0\nMOV K-1 E1\nMOV K{} F1\nMOV K-{} E2\nMOV K-{} F2\n{}\n{}\n{}\nDMOV H80000001 C200\nDMOV H80000001 C255\n{}\n{}",
            if w { -1 } else { 0 },
            width - 1,
            width - 1,
            words("D", last - 4, &[91, 2, 0, 4, 0]),
            words("T", 251, &[91, 2, 0, 4, 0]),
            words("C", 195, &[91, 2, 0, 4, 0]),
            bits("M", 4096 - width, width, 0xffff_ffff),
            bits("S", 1024 - width, width, 0x55aa)
        );
        let mut observe = vec![
            reads("D", last - 4, 5, 200),
            reads("T", 251, 5, 210),
            reads("C", 195, 5, 220),
            "DMOV C200 D230\nDMOV C255 D232\nMOV K1Y374 D234".into(),
        ];
        let body = match base(op) {
            "DECO" => {
                seed.push_str("\nMOV K-2 E1\nMOV K-7 F1\nMOV K3 D20\nMOV K77 D21\nMOV K88 D22\nMOV K15 C199\nSET S1023");
                observe.push("MOV D100 D236\nMOV D22 D237\nMOV D20 D238\nMOV D21 D239".into());
                // Direct raw source also needs the handler-default width80:
                // explicit84 would read8 bits beyond this valid1-bit endpoint.
                format!(
                    "LD X0\n{op} C199E0 D{last}F0 K4\n{op} D{last}E0 D100F0 K4\n{op} D20E0 T255F0 K4\n{op} X377E1 Y377F1 K3\n{op} S1023 D22 K1\n{op} S1023 D20E0 K1\n{op} S1023E0 D21F0 K1"
                )
            }
            "ENCO" => {
                seed.push_str("\nMOV K-7 F1\nSET M4095\nMOV H8001 T255");
                observe.push("MOV D100 D236".into());
                format!(
                    "LD X0\n{op} X377F1 D{last}E0 K3\n{op} D{last}E0 D100F0 K4\n{op} M4095F1 T255E0 K3\n{op} T255E0 C199F0 K4"
                )
            }
            "SUM" | "DSUM" => {
                seed.push_str(&format!(
                    "\n{m} K-1 D{}\n{m} H8001 T{}",
                    last - u16::from(w),
                    255 - u16::from(w)
                ));
                observe.push(format!("{m} D100 D236"));
                format!(
                    "LD X0\n{op} D{last}F1 D100E0\n{op} T255F1 D{last}F1\n{op} C199E0 T255F1\n{op} K{digits}M4095E2 D{last}F1"
                )
            }
            "BON" | "DBON" => {
                seed.push_str(&format!(
                    "\n{m} K{} D{}\nSET Y376\nRST Y377",
                    width - 1,
                    last - u16::from(w)
                ));
                format!("LD X0\n{op} K{digits}M4095E2 Y377E0 D{last}F1\n{op} C199E0 S1023E0 K15")
            }
            "MEAN" | "DMEAN" => {
                let nwords = if w { 4 } else { 2 };
                seed.push_str(&format!(
                    "\nMOV K-{} E1\n{}\nMOV K-{} E2\n{}",
                    nwords - 1,
                    values("D", last - nwords + 1, w, &[4, 8]),
                    width * 2 - 1,
                    groups("M", 4096 - width * 2, digits, &[4, 8])
                ));
                let mut b = format!(
                    "LD X0\n{op} D{last}E1 T255F1 K2\n{op} K{digits}M4095E2 K{digits}S1023F2 K2"
                );
                if w {
                    b.push_str(&format!("\nLDI X7\nDMOV K2 C197\nMOV K4 C199\nDMOV K6 C200\nLD X0\n{op} C197E0 D100F0 K3\nLDI X7\nDMOV D100 D240\nMOV K4 C198\nMOV K0 C199\nDMOV K8 C200\nLD X0\n{op} C198E0 D100F0 K2\nLDI X7\nDMOV D100 D242\nMOV K6 C199\nDMOV K10 C200\nLD X0\n{op} C199E0 D100F0 K2\nLDI X7\nDMOV D100 D244\nLD X0\n{op} C255E0 D100F0 K1"));
                } else {
                    b.push_str(&format!(
                        "\nLDI X7\nMOV K4 C198\nMOV K8 C199\nLD X0\n{op} C198E0 D100F0 K2"
                    ));
                }
                observe.push(format!(
                    "{}\n{}\nMOV D100 D248\nMOV D101 D249",
                    reads("D", 240, 6, 260),
                    bit_reads("S", 1024 - width, width, 270)
                ));
                b
            }
            _ => unreachable!(),
        };
        observe.push("LDI X7\nMOV K1234 D290".into());
        let mut c = scenario(
            &format!("model_{}_complete_ends_{op}", target.name()),
            &seed,
            &body,
            &observe.join("\n"),
        );
        c.stages[0].0.extend([
            ("X370", true),
            ("X371", false),
            ("X372", false),
            ("X373", true),
            ("X374", false),
            ("X375", true),
            ("X376", false),
            ("X377", true),
        ]);
        cs.push(c);
    }
    check(&cs, 16);
    cs
}

/// LOCAL ONLY. These satisfy the documented valid-address interpretation,
/// but native raw numeric/group loops have no complete endpoint guard.
/// The parent must NOT append this list to the native suite.
#[allow(dead_code)] // parked reference cases, not part of the default run
pub(super) fn documented_partial_cases(target: Target) -> Vec<ComparisonCase> {
    let last: u16 = match target {
        Target::Es2 => 9999,
        Target::Ss2 => 4999,
    };
    let mut cs = Vec::new();
    for op in ["MEAN", "MEANP", "DMEAN", "DMEANP"] {
        let w = wide(op);
        let m = mov(w);
        let cmp = if w { "DAND=" } else { "AND=" };
        for (area, end) in [("D", last), ("T", 255), ("C", if w { 255 } else { 199 })] {
            let init = if w && area == "C" {
                format!("DMOV K6 C255")
            } else {
                format!("MOV K6 {area}{end}")
            };
            let blocks = vec![oracle_block(
                op,
                &format!("{area}{end}E0 D50F0 D50E0"),
                &format!("{init}\n{m} K2 D50"),
                &format!("{cmp} D50 K3\nANI M1067\nAND M1020\nANI M1021\nAND M1022"),
                &format!("{cmp} D50 K2\nANI M1067"),
                840,
            )];
            let mut c = oracle_case(
                &format!(
                    "LOCAL_ONLY_model_{}_partial_numeric_{area}{end}",
                    target.name()
                ),
                op,
                blocks,
                "MOV D50 D220\nMOV D51 D221",
            );
            c.name = c.name.replacen(
                "indexed_data_processing_literal_oracle_",
                "indexed_data_processing_LOCAL_ONLY_literal_oracle_",
                1,
            );
            cs.push(c);
        }
        // First group is complete. Only the SECOND element is partly beyond
        // the area end; missing bits are zero, denominator remains requested2.
        for (area, end) in [("S", 1023u16), ("M", 4095u16)] {
            let width = if w { 32 } else { 16 };
            let digits = width / 4;
            let start = end - width - 3;
            let setup = format!(
                "{}\nMOV K8 K1{}\n{m} K2 D50",
                bits(area, start, width, 4),
                addr(area, end - 3)
            );
            let mut c = oracle_case(
                &format!(
                    "LOCAL_ONLY_model_{}_later_partial_K{digits}_{area}",
                    target.name()
                ),
                op,
                vec![oracle_block(
                    op,
                    &format!("K{digits}{}E0 D50F0 D50E0", addr(area, start)),
                    &setup,
                    &format!("{cmp} D50 K6\nANI M1067"),
                    &format!("{cmp} D50 K2\nANI M1067"),
                    840,
                )],
                &format!(
                    "MOV K1{} D220\nMOV D50 D221\nMOV D51 D222",
                    addr(area, end - 3)
                ),
            );
            c.name = c.name.replacen(
                "indexed_data_processing_literal_oracle_",
                "indexed_data_processing_LOCAL_ONLY_literal_oracle_",
                1,
            );
            cs.push(c);
        }
    }
    check(&cs, 20);
    cs
}
