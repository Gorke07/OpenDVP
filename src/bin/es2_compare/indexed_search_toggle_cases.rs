//! Addressed SER/DSER/ALT candidates; no executed parity claim.
//! counter_termination_cases is LOCAL ONLY, never native-main.
use super::{Case, Target, case};
use std::collections::BTreeSet;

const SEARCH: [&str; 4] = ["SER", "SERP", "DSER", "DSERP"];
const FORMS: [&str; 6] = ["SER", "SERP", "DSER", "DSERP", "ALT", "ALTP"];
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
fn wide(op: &str) -> bool {
    op.starts_with('D')
}
fn mov(w: bool) -> &'static str {
    if w { "DMOV" } else { "MOV" }
}
fn cmp(w: bool) -> &'static str {
    if w { "DAND=" } else { "AND=" }
}
fn addr(a: &str, n: u16) -> String {
    if matches!(a, "X" | "Y") {
        format!("{a}{n:o}")
    } else {
        format!("{a}{n}")
    }
}
fn vals(a: &str, start: u16, w: bool, values: &[i32]) -> String {
    values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            format!(
                "{} K{v} {a}{}",
                mov(w),
                start + i as u16 * if w { 2 } else { 1 }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn groups(a: &str, start: u16, digits: u16, w: bool, values: &[i32]) -> String {
    values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            format!(
                "{} K{v} K{digits}{}",
                mov(w),
                addr(a, start + i as u16 * digits * 4)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn reads(a: &str, start: u16, n: u16, out: u16) -> String {
    (0..n)
        .map(|i| format!("MOV {a}{} D{}", start + i, out + i))
        .collect::<Vec<_>>()
        .join("\n")
}
fn result_reads(a: &str, start: u16, w: bool, out: u16) -> String {
    (0..5)
        .map(|i| {
            format!(
                "{} {a}{} D{}",
                mov(w),
                start + i * if w { 2 } else { 1 },
                out + i * if w { 2 } else { 1 }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn result_eq(a: &str, start: u16, w: bool, expected: &[i32; 5]) -> String {
    expected
        .iter()
        .enumerate()
        .map(|(i, v)| {
            format!(
                "{} {a}{} K{v}",
                cmp(w),
                start + i as u16 * if w { 2 } else { 1 }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn fixture(name: &str, seed: &str, body: &str, observe: &str) -> Case {
    let gate = if body.starts_with("LD") || body.starts_with("STL") {
        ""
    } else {
        "LD X0"
    };
    let source=[
        "LD X1\nMOV K0 E0\nMOV K0 F0\nSET M1020\nRST M1021\nSET M1022\nRST M1029\nRST M1067\nRST M1068\nMOV H1234 D1067\nMOV K77 D1068\nRST M790\nRST M791",
        seed,gate,body,
        "LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1029\nOUT Y23\nLD M1067\nOUT Y24\nLD M1068\nOUT Y25\nLDI X7\nMOV D1067 D900\nMOV D1068 D901\nMOV E0 D902\nMOV F0 D903\nMOV D1028 D904\nMOV D1029 D905",
        observe,"LDI X7\nMOV K1234 D906","END",
    ].into_iter().filter(|s|!s.is_empty()).collect::<Vec<_>>().join("\n");
    assert!(source.lines().count() <= 512, "oversized {name}");
    assert!(
        source.lines().count() * 17 < 0x1ef0,
        "conservative WORD budget {name}"
    );
    assert!(
        source.lines().any(|l| {
            let f: Vec<_> = l.split_whitespace().collect();
            f.first().is_some_and(|x| FORMS.contains(x))
                && f[1..].iter().any(|x| {
                    let b = x.as_bytes();
                    b.len() >= 4
                        && matches!(b[b.len() - 2], b'E' | b'F')
                        && b[b.len() - 1].is_ascii_digit()
                })
        }),
        "no modified role {name}"
    );
    case(&format!("indexed_search_toggle_{name}"), &source, stages())
}
fn unique(cs: &[Case], expected: usize) {
    assert_eq!(cs.len(), expected);
    assert_eq!(
        cs.iter().map(|c| &c.name).collect::<BTreeSet<_>>().len(),
        expected
    );
}

pub(super) fn common_cases() -> Vec<Case> {
    let mut cs = Vec::new();
    banks(&mut cs);
    packed(&mut cs);
    numeric_roles(&mut cs);
    literals(&mut cs);
    controls(&mut cs);
    scratch(&mut cs);
    maxima(&mut cs);
    unique(&cs, 274);
    let forms: BTreeSet<_> = cs
        .iter()
        .flat_map(|c| c.source.lines())
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    for op in FORMS {
        assert!(forms.contains(op));
    }
    cs
}

fn banks(cs: &mut Vec<Case>) {
    for op in SEARCH {
        let w = wide(op);
        let m = mov(w);
        let inc = if w { "DINC" } else { "INC" };
        for bank in 0..8 {
            let seed = format!(
                "MOV K1 E{bank}\nMOV K-1 F{bank}\n{}\n{}\n{m} K4 D61\n{m} K8 D69\n{m} K5 D280\n{}\n{}",
                vals("D", 41, w, &[4, 2, 4, -7, 8]),
                vals("D", 79, w, &[8, 3, 8, -1, 9]),
                vals("D", 100, w, &[77; 5]),
                vals("D", 121, w, &[88; 5])
            );
            let body = format!(
                "{op} D40E{bank} D62F{bank} D101F{bank} D280\n{op} D80F{bank} D68E{bank} D120E{bank} H5\n{inc} D41\n{inc} D79"
            );
            cs.push(fixture(
                &format!("banks_{op}_E{bank}_F{bank}"),
                &seed,
                &body,
                &format!(
                    "{}\n{}\nMOV E{bank} D300\nMOV F{bank} D301\n{m} D280 D302",
                    result_reads("D", 100, w, 200),
                    result_reads("D", 121, w, 220)
                ),
            ));
        }
    }
    for op in ["ALT", "ALTP"] {
        for bank in 0..8 {
            for area in ["Y", "M", "S"] {
                let a = addr(area, 64);
                let b = addr(area, 80);
                let seed =
                    format!("MOV K1 E{bank}\nMOV K-1 F{bank}\nMOV H5AA5 K4{a}\nMOV HA55A K4{b}");
                let body =
                    format!("{op} {a}E{bank}\n{op} {b}F{bank}\nLD X0\nINC E{bank}\nDEC F{bank}");
                cs.push(fixture(
                    &format!("banks_{op}_{area}_E{bank}_F{bank}"),
                    &seed,
                    &body,
                    &format!(
                        "MOV K4{a} D200\nMOV K4{} D201\nMOV E{bank} D202\nMOV F{bank} D203",
                        addr(area, 76)
                    ),
                ));
            }
        }
    }
    assert_eq!(cs.len(), 80);
}

fn packed(cs: &mut Vec<Case>) {
    for op in SEARCH {
        let w = wide(op);
        let m = mov(w);
        let max = if w { 8 } else { 4 };
        for digits in 1..=max {
            for area in ["X", "Y", "M", "S"] {
                let dd = max + 1 - digits;
                let (da, db) = match area {
                    "M" => ("S", 256),
                    "S" => ("Y", 64),
                    _ => ("M", 512),
                };
                let s = addr(area, 64);
                let mut seed = format!(
                    "MOV K1 E0\nMOV K-1 F0\n{}\nSET {}\nSET {}",
                    groups(da, db, dd, w, &[-1; 5]),
                    addr(da, db - 1),
                    addr(da, db + 5 * dd * 4)
                );
                if area != "X" {
                    seed.push_str(&format!("\n{}", groups(area, 64, digits, w, &[3, 1])));
                }
                let body = format!(
                    "{op} K{digits}{}E0 K{digits}{}F0 K{dd}{}F0 K2",
                    addr(area, 63),
                    addr(area, 65),
                    addr(da, db + 1)
                );
                let mut obs = (0..5)
                    .map(|i| format!("{m} K{dd}{} D{}", addr(da, db + i * dd * 4), 200 + i * 2))
                    .collect::<Vec<_>>();
                obs.push(format!(
                    "LD {}\nOUT M810\nLD {}\nOUT M811\nLDI X7\n{m} K{digits}{s} D220",
                    addr(da, db - 1),
                    addr(da, db + 5 * dd * 4)
                ));
                let mut c = fixture(
                    &format!("packed_{op}_S{area}_K{digits}_D{da}_K{dd}"),
                    &seed,
                    &body,
                    &obs.join("\n"),
                );
                if area == "X" {
                    for ((changes, _), flip) in c.stages.iter_mut().zip([false, true, false, true])
                    {
                        let pattern = (if flip { 1u64 } else { 3 }) | (1u64 << (digits * 4));
                        for (i, n) in INPUTS.into_iter().enumerate() {
                            changes.push((n, pattern & (1u64 << i) != 0));
                        }
                    }
                }
                cs.push(c);
            }
        }
    }
    assert_eq!(cs.len(), 176);
}

fn numeric_roles(cs: &mut Vec<Case>) {
    for op in SEARCH {
        let w = wide(op);
        let m = mov(w);
        for (sa, da) in [("T", "C"), ("C", "D"), ("D", "T")] {
            let sb = if sa == "C" { 180 } else { 40 };
            let db = if da == "C" { 180 } else { 120 };
            let seed = format!(
                "MOV K1 E0\nMOV K-1 F0\n{}\n{}\n{m} K4 D80\n{m} K5 D280",
                vals(sa, sb, w, &[4, 2, 4, -7, 8]),
                vals(da, db, w, &[77; 5])
            );
            cs.push(fixture(
                &format!("numeric_roles_{op}_{sa}_{da}"),
                &seed,
                &format!(
                    "{op} {sa}{}E0 {sa}{}F0 {da}{}F0 D280\n{m} K2 {sa}{sb}",
                    sb - 1,
                    sb + 1,
                    db + 1
                ),
                &format!(
                    "{}\n{}",
                    result_reads(da, db, w, 200),
                    reads(sa, sb, if w { 10 } else { 5 }, 220)
                ),
            ));
        }
        let mut body = Vec::new();
        let mut obs = Vec::new();
        for bank in 0..8 {
            for area in if w { &["E"][..] } else { &["E", "F"][..] } {
                let alias =
                    if bank == 0 { 1028 } else { 1180 + 2 * bank } + u16::from(*area == "F");
                let out = 300 + bank * 12 + u16::from(*area == "F") * 5;
                let modifier = if bank == 7 { "E6" } else { "E7" };
                body.push(format!("LDI X7\n{m} K4 D{alias}\nMOV K0 {modifier}\nLD X0\n{op} D40{modifier} {area}{bank} D100{modifier} K5\nLDI X7\n{}",result_reads("D",100,w,out)));
                obs.push(format!(
                    "MOV D{alias} D{}",
                    500 + bank * 2 + u16::from(*area == "F")
                ));
            }
        }
        cs.push(fixture(
            &format!("direct_S2_all_allowed_EF_{op}"),
            &format!(
                "{}\n{}",
                vals("D", 40, w, &[4, 2, 4, -7, 8]),
                vals("D", 100, w, &[77; 5])
            ),
            &body.join("\n"),
            &obs.join("\n"),
        ));
    }
    assert_eq!(cs.len(), 192);
}

fn block(op: &str, args: &str, setup: &str, on: &str, off: &str, marker: u16) -> String {
    format!(
        "LDI X7\nMOV K0 E0\nMOV K0 F0\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV H1234 D1067\nMOV K77 D1068\n{setup}\nLD X0\n{op} {args}\nLD M791\n{on}\nLDI M791\n{off}\nORB\nOUT M{marker}"
    )
}
fn oracle(name: &str, op: &str, blocks: Vec<String>, observe: &str) -> Case {
    fixture(
        &format!("literal_oracle_{name}_{op}"),
        "",
        &format!(
            "LD X0\n{}OUT M791\n{}\nLD X0\nOUT M790",
            if op.ends_with('P') { "ANI M790\n" } else { "" },
            blocks.join("\n")
        ),
        observe,
    )
}
fn literals(cs: &mut Vec<Case>) {
    for op in SEARCH {
        let w = wide(op);
        let m = mov(w);
        let c = cmp(w);
        let stride = if w { 2 } else { 1 };
        let max = if w { 128 } else { 256 };
        let g = if w { 8 } else { 4 };
        cs.push(oracle(
            "manual_and_no_match_tie_last",
            op,
            vec![
                block(
                    op,
                    "D20E0 K100 D100F0 K10",
                    &format!(
                        "{}\n{}",
                        vals("D", 20, w, &[88, 100, 110, 150, 100, 300, 100, 5, 100, 500]),
                        vals("D", 100, w, &[77; 5])
                    ),
                    &format!(
                        "{}\nAND M1020\nANI M1021\nAND M1022",
                        result_eq("D", 100, w, &[4, 1, 8, 7, 9])
                    ),
                    &result_eq("D", 100, w, &[77; 5]),
                    840,
                ),
                block(
                    op,
                    "D20E0 K10 D100F0 K6",
                    &format!(
                        "{}\n{}",
                        vals("D", 20, w, &[-7, 3, -7, 3, 0, -7]),
                        vals("D", 100, w, &[77; 5])
                    ),
                    &result_eq("D", 100, w, &[0, 0, 0, 5, 3]),
                    &result_eq("D", 100, w, &[77; 5]),
                    841,
                ),
            ],
            &result_reads("D", 100, w, 200),
        ));
        cs.push(oracle(
            "all_source_needle_count_snapshots",
            op,
            vec![
                block(
                    op,
                    "D20E0 K4 D20F0 K5",
                    &vals("D", 20, w, &[4, 2, 4, -7, 8]),
                    &result_eq("D", 20, w, &[2, 0, 2, 3, 4]),
                    &result_eq("D", 20, w, &[4, 2, 4, -7, 8]),
                    840,
                ),
                block(
                    op,
                    "D20E0 D100 D100F0 K5",
                    &format!(
                        "{}\n{}",
                        vals("D", 20, w, &[4, 2, 4, -7, 8]),
                        vals("D", 100, w, &[-7, 77, 77, 77, 77])
                    ),
                    &result_eq("D", 100, w, &[1, 3, 3, 3, 4]),
                    &result_eq("D", 100, w, &[-7, 77, 77, 77, 77]),
                    841,
                ),
                block(
                    op,
                    "D20E0 K3 D100F0 D100",
                    &format!(
                        "{}\n{}",
                        vals("D", 20, w, &[3, 2, 3]),
                        vals("D", 100, w, &[3, 77, 77, 77, 77])
                    ),
                    &result_eq("D", 100, w, &[2, 0, 2, 1, 2]),
                    &result_eq("D", 100, w, &[3, 77, 77, 77, 77]),
                    842,
                ),
            ],
            &format!(
                "{}\n{}",
                result_reads("D", 20, w, 200),
                result_reads("D", 100, w, 220)
            ),
        ));
        cs.push(oracle(
            "cached_destination_own_EF_index",
            op,
            vec![
                block(
                    op,
                    "D20E0 K4 D1182E1 K5",
                    &format!(
                        "{}\n{}\nMOV K0 E1\nMOV K91 D1192\nMOV K92 D1193",
                        vals("D", 20, w, &[4, 2, 4, -7, 8]),
                        vals("D", 1182, w, &[77; 5])
                    ),
                    &format!(
                        "{}\nAND= D1192 K91\nAND= D1193 K92",
                        result_eq("D", 1182, w, &[2, 0, 2, 3, 4])
                    ),
                    "AND= E1 K0\nAND= D1192 K91",
                    840,
                ),
                block(
                    op,
                    "D20E0 K4 D1183F1 K5",
                    &format!(
                        "{}\n{}\nMOV K0 F1\nMOV K91 D1193\nMOV K92 D1194",
                        vals("D", 20, w, &[4, 2, 4, -7, 8]),
                        vals("D", 1183, w, &[77; 5])
                    ),
                    &format!(
                        "{}\nAND= D1193 K91\nAND= D1194 K92",
                        result_eq("D", 1183, w, &[2, 0, 2, 3, 4])
                    ),
                    "AND= F1 K0\nAND= D1193 K91",
                    841,
                ),
            ],
            &reads("D", 1182, 13, 200),
        ));
        let raw = if w { i32::MIN } else { -32768 };
        cs.push(oracle("short_zero_extension_signed_full_group",op,vec![
            block(op,"K1M128E0 K15 D100F0 K3",&format!("{}\n{}",groups("M",128,1,w,&[15,1,15]),vals("D",100,w,&[77;5])),&result_eq("D",100,w,&[2,0,2,1,2]),&result_eq("D",100,w,&[77;5]),840),
            block(op,&format!("K{g}M128E0 K{raw} K1S256F0 K3"),&format!("{}\n{}",groups("M",128,g,w,&[raw,0,raw]),groups("S",256,1,w,&[15;5])),"AND= K1S256 K2\nAND= K1S260 K0\nAND= K1S264 K2\nAND= K1S268 K2\nAND= K1S272 K1","AND= K1S256 K15\nAND= K1S272 K15",841),
        ],"MOV K4S256 D200\nMOV K1S272 D201"));
        cs.push(oracle("invalid_count_after_base_and_needle_hold",op,[0,-1,max+1].into_iter().enumerate().map(|(i,n)|block(op,"D20E0 K4 D100F0 D280",&format!("{}\n{}\n{m} K{n} D280",vals("D",20,w,&[4]),vals("D",100,w,&[77;5])),&format!("{}\nAND= D1067 HE1A\nAND M1067\nAND M1068\nAND M1020\nANI M1021\nAND M1022",result_eq("D",100,w,&[77;5])),&format!("{}\nAND= D1067 H1234\nANI M1067",result_eq("D",100,w,&[77;5])),840+i as u16)).collect(),"MOV D1068 D200"));
        cs.push(oracle(
            "signed_extreme_and_reversed_overlap",
            op,
            vec![block(
                op,
                &format!("D20E0 K-1 D{}F0 K6", 20 - stride),
                &format!(
                    "{}\n{m} K77 D{}",
                    vals(
                        "D",
                        20,
                        w,
                        &[
                            if w { i32::MIN } else { -32768 },
                            if w { i32::MAX } else { 32767 },
                            -1,
                            0,
                            if w { i32::MIN } else { -32768 },
                            if w { i32::MAX } else { 32767 }
                        ]
                    ),
                    20 - stride
                ),
                &result_eq("D", 20 - stride, w, &[1, 2, 2, 4, 5]),
                &format!("{c} D{} K77", 20 - stride),
                840,
            )],
            &result_reads("D", 20 - stride, w, 200),
        ));
    }
    for op in ["ALT", "ALTP"] {
        for (area, bit) in [("Y", 64), ("M", 128), ("S", 128), ("M", 1022)] {
            let a = addr(area, bit);
            let before = addr(area, bit - 1);
            let after = if bit == 1022 {
                "M1019".into()
            } else {
                addr(area, bit + 1)
            };
            cs.push(oracle(
                &format!("toggle_{area}{bit}_ordinary_partner"),
                op,
                vec![
                    block(
                        op,
                        &format!("{a}E0"),
                        &format!("RST {a}\nSET {before}\nSET {after}"),
                        &format!("AND {a}\nAND {before}\nAND {after}"),
                        &format!("ANI {a}\nAND {before}\nAND {after}"),
                        840,
                    ),
                    // This all-plain site exercises ordinary ALT's canonical80 emitter.
                    block(
                        op,
                        &a,
                        &format!("SET {a}\nSET {before}\nSET {after}"),
                        &format!("ANI {a}\nAND {before}\nAND {after}"),
                        &format!("AND {a}\nAND {before}\nAND {after}"),
                        841,
                    ),
                ],
                &format!("LD {a}\nOUT M810\nLDI X7\nMOV K1234 D200"),
            ));
        }
    }
    assert_eq!(cs.len(), 224);
}

fn controls(cs: &mut Vec<Case>) {
    for op in FORMS {
        let w = wide(op);
        let m = mov(w);
        let args = if op.starts_with("ALT") {
            "M128E1"
        } else {
            "D40E1 D80 D100F0 D280"
        };
        let seed = format!(
            "MOV K1 E1\nMOV K0 F0\n{}\n{}\n{m} K4 D80\n{m} K5 D280\nMOV H5AA5 K4M128\n{}\nMOV K0 D290",
            vals("D", 41, w, &[4, 2, 4, -7, 8]),
            vals("D", 50, w, &[4, 4, 1, 4, 9]),
            vals("D", 100, w, &[77; 5])
        );
        for kind in [
            "same_pc_loop",
            "distinct_occurrences_live_modifier",
            "master_false",
            "cj_skipped_off",
        ] {
            let body = match kind {
                "same_pc_loop" => {
                    format!("LD X0\nFOR K3\nLD X0\n{op} {args}\nNEXT\nLDI X7\nINC D290")
                }
                "distinct_occurrences_live_modifier" => format!(
                    "LD X0\nMPS\n{op} {args}\nMRD\nMOV K10 E1\nMRD\n{op} {args}\nMPP\nOUT Y10\nLDI X7\nMOV K1 E1\nINC D290"
                ),
                "master_false" => {
                    format!("LD X7\nMC N0\nLDI X7\n{op} {args}\nMCR N0\nLDI X7\nINC D290")
                }
                _ => format!("LD X2\nCJ P10\nLD X0\n{op} {args}\nP10\nLDI X7\nINC D290"),
            };
            let mut c = fixture(
                &format!("control_{kind}_{op}"),
                &seed,
                &body,
                &format!(
                    "{}\nMOV K4M128 D220\nMOV D290 D221\nMOV E1 D222",
                    result_reads("D", 100, w, 200)
                ),
            );
            if kind == "cj_skipped_off" {
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
    }
    assert_eq!(cs.len(), 248);
}

fn scratch(cs: &mut Vec<Case>) {
    for op in SEARCH {
        let w = wide(op);
        for variant in ["zero", "one", "invalid_count"] {
            // Initialize one owned SORT occurrence before changing scratch.
            // WORD starts at threshold0; DWORD at3, hence exactly one row has
            // progressed before DSER's negative original-tag scratch arrives.
            let initial = if w { 3 } else { 0 };
            let needle = if variant == "zero" { 0 } else { 1 };
            let n = if variant == "invalid_count" { 0 } else { 1 };
            let seed = format!(
                "MOV K0 E7\nMOV K0 F3\n{}\n{}\n{}\n{}\nMOV K7 D750\nSER D750 K{initial} D760 K1\nRST M1029",
                vals("D", 400, false, &[30, 10, 20, 3, 1, 2]),
                vals("D", 499, false, &[-777; 10]),
                vals("D", 40, w, &[2]),
                vals("D", 100, w, &[77; 5])
            );
            let mut c = fixture(
                &format!("private_SORT_scratch_{op}_{variant}"),
                &seed,
                &format!(
                    "LD X2\n{op} D40E7 K{needle} D100F3 K{n}\nLD X0\nSORT D400 K3 K2 D500 K2\nOUT Y10"
                ),
                &format!(
                    "{}\n{}\nLD M1029\nOUT Y11",
                    reads("D", 499, 10, 200),
                    result_reads("D", 100, w, 220)
                ),
            );
            c.stages = vec![
                (
                    vec![("X1", true), ("X0", false), ("X2", false), ("X7", false)],
                    1,
                ),
                (vec![("X1", false), ("X0", true)], 1),
                (vec![("X2", true)], 2),
                (vec![("X2", false)], 1),
                (vec![("X0", false)], 1),
                (vec![("X0", true), ("X2", true)], 1),
            ];
            cs.push(c);
        }
    }
    for op in ["ALT", "ALTP"] {
        let seed = format!(
            "{}\n{}\nMOV K7 D750\nSER D750 K1 D760 K1\nRST M1029\nMOV K0 E0\nRST M128",
            vals("D", 400, false, &[30, 10, 20, 3, 1, 2]),
            vals("D", 499, false, &[-777; 10])
        );
        let mut c = fixture(
            &format!("private_SORT_scratch_hold_{op}"),
            &seed,
            &format!("LD X2\n{op} M128E0\nLD X0\nSORT D400 K3 K2 D500 K2\nOUT Y10"),
            &format!(
                "{}\nLD M128\nOUT M810\nLD M1029\nOUT Y11",
                reads("D", 499, 10, 200)
            ),
        );
        c.stages = vec![
            (
                vec![("X1", true), ("X0", false), ("X2", false), ("X7", false)],
                1,
            ),
            (vec![("X1", false), ("X0", true)], 1),
            (vec![("X2", true)], 2),
            (vec![("X2", false)], 1),
            (vec![("X0", false)], 1),
            (vec![("X0", true), ("X2", true)], 1),
        ];
        cs.push(c);
    }
    assert_eq!(cs.len(), 262);
}

fn maxima(cs: &mut Vec<Case>) {
    for op in SEARCH {
        let w = wide(op);
        let m = mov(w);
        let fill = if w { "DFMOV" } else { "FMOV" };
        let max = if w { 128 } else { 256 };
        let stride = if w { 2 } else { 1 };
        for variant in ["all_match_byte_count", "no_match", "last_extrema"] {
            let mut seed = format!(
                "MOV K0 E0\nMOV K0 F0\n{fill} K7 D0 K{max}\n{}\n{m} K{max} D280",
                vals("D", 300, w, &[77; 5])
            );
            let needle = if variant == "no_match" { 8 } else { 7 };
            if variant == "last_extrema" {
                seed.push_str(&format!(
                    "\n{m} K-7 D{}\n{m} K17 D{}",
                    (max - 2) * stride,
                    (max - 1) * stride
                ));
            }
            let expected = match variant {
                "all_match_byte_count" => [max % 256, 0, max - 1, max - 1, max - 1],
                "no_match" => [0, 0, 0, max - 1, max - 1],
                _ => [max - 2, 0, max - 3, max - 2, max - 1],
            };
            cs.push(oracle(
                &format!("maximum_{variant}"),
                op,
                vec![block(
                    op,
                    &format!("D0E0 K{needle} D300F0 D280"),
                    &seed,
                    &result_eq("D", 300, w, &expected),
                    &result_eq("D", 300, w, &[77; 5]),
                    840,
                )],
                &format!(
                    "{}\nMOV D299 D220\nMOV D{} D221",
                    result_reads("D", 300, w, 200),
                    300 + 5 * stride
                ),
            ));
        }
    }
    assert_eq!(cs.len(), 274);
}

pub(super) fn model_cases(target: Target) -> Vec<Case> {
    let last: u16 = match target {
        Target::Es2 => 9999,
        Target::Ss2 => 4999,
    };
    let mut cs = Vec::new();
    for op in SEARCH {
        let w = wide(op);
        let m = mov(w);
        let width = if w { 32 } else { 16 };
        let digits = width / 4;
        let stride = if w { 2 } else { 1 };
        for area in ["D", "T", "C", "groups"] {
            let (seed,body,observe)=match area {
                "D"=>{let start=last+1-5*stride;let s=last+1-stride;
                    (format!("MOV K-{} E0\nMOV K-{} F0\n{m} K7 D{s}\n{}\n{m} K1 D280",stride-1,5*stride-1,vals("D",100,w,&[77;5])),format!("{op} D{last}E0 K7 D100E1 D280\n{op} D40E1 K4 D{last}F0 K5"),format!("{}\n{}",result_reads("D",100,w,200),reads("D",start,5*stride,220)))},
                "T"=>{let s=256-stride;let d=256-5*stride;
                    (format!("MOV K-{} E0\nMOV K-{} F0\n{m} K7 T{s}\n{}",stride-1,5*stride-1,vals("D",100,w,&[77;5])),format!("{op} T255E0 K7 D100E1 K1\n{op} D40E1 K4 T255F0 K5"),format!("{}\n{}",result_reads("D",100,w,200),reads("T",d,5*stride,220)))},
                "C" if w=>{
                    ("MOV K0 E0\nMOV K0 F0\nMOV K-4 F1\nDMOV K7 C255\nDMOV K2 C197\nMOV HFFFF C199\nDMOV K-3 C200\nDMOV K9 C201\nDMOV K7 C202".into(),format!("{op} C197E0 K65535 D100F0 K3\n{op} C198E0 K-3 D120F0 K2\n{op} C199E0 K65535 D140F0 K2\n{op} C200E0 K9 D160F0 K3\n{op} C255E0 K7 C255F1 K1"),format!("{}\n{}\n{}\n{}\n{}",result_reads("D",100,true,300),result_reads("D",120,true,320),result_reads("D",140,true,340),result_reads("D",160,true,360),(251..=255).map(|i|format!("DMOV C{i} D{}",380+(i-251)*2)).collect::<Vec<_>>().join("\n")))},
                "C"=>{
                    ("MOV K0 E0\nMOV K-4 F0\nMOV K7 C199".into(),format!("{op} C199E0 K7 D100E0 K1\n{op} D40E0 K4 C199F0 K5"),format!("{}\n{}",result_reads("D",100,false,200),reads("C",195,5,220)))},
                _=>{
                    let s=4096-2*width;let d=1024-5*width;
                    (format!("MOV K-{} E0\nMOV K-{} F0\n{}\n{}\nMOV K0 E2",2*width-1,5*width-1,groups("M",s,digits,w,&[3,1]),groups("S",d,digits,w,&[-1;5])),format!("{op} K{digits}M4095E0 K3 K{digits}S1023F0 K2\n{op} K1X374 K1X374 D100E2 K1"),format!("{}\n{}",(0..5).map(|i|format!("{m} K{digits}S{} D{}",d+i*width,200+i*2)).collect::<Vec<_>>().join("\n"),result_reads("D",100,w,220)))},
            };
            let mut c = fixture(
                &format!("model_{}_complete_{area}_{op}", target.name()),
                &format!("MOV K0 E1\n{}\n{seed}", vals("D", 40, w, &[4, 2, 4, -7, 8])),
                &body,
                &observe,
            );
            c.stages[0].0.extend([
                ("X374", true),
                ("X375", false),
                ("X376", true),
                ("X377", false),
            ]);
            cs.push(c);
        }
    }
    for op in ["ALT", "ALTP"] {
        for (area, end) in [("Y", 255), ("M", 4095), ("S", 1023)] {
            let d = addr(area, end);
            let prior = addr(area, end - 1);
            let mut seed = format!("MOV K0 E0\nMOV K-1 F0\nRST {d}\nSET {prior}");
            let mut body = format!("{op} {d}E0\n{op} {d}F0");
            if area == "M" {
                seed.push_str("\nRST M1280");
                body.push_str(&format!("\n{op} M1280"));
            }
            cs.push(fixture(&format!("model_{}_complete_{area}_{op}",target.name()),&seed,&body,&format!("LD {d}\nOUT M810\nLD {prior}\nOUT M811\nLD M1280\nOUT M812\nLDI X7\nMOV K1234 D200")));
        }
    }
    unique(&cs, 22);
    cs
}

/// LOCAL ONLY: real halfword-termination contract, not a complete result stack.
/// Native-main qualification currently rejects these destinations deliberately.
#[allow(dead_code)] // parked reference cases, not part of the default run
pub(super) fn counter_termination_cases(target: Target) -> Vec<Case> {
    let mut cs = Vec::new();
    for op in ["DSER", "DSERP"] {
        for start in [197u16, 198, 199] {
            let setup = format!(
                "{}\nMOV K77 C197\nMOV K78 C198\nMOV K79 C199\nDMOV K123456 C200\nDMOV K234567 C201",
                vals("D", 20, true, &[4, 2, 4, -7, 8])
            );
            let on = match start {
                197 => "AND= C197 K2\nAND= C198 K0\nAND= C199 K0",
                198 => "AND= C197 K77\nAND= C198 K2\nAND= C199 K0",
                _ => "AND= C197 K77\nAND= C198 K78\nAND= C199 K2",
            };
            let mut c = oracle(
                &format!(
                    "LOCAL_ONLY_{}_counter_halfword_stop_C{start}",
                    target.name()
                ),
                op,
                vec![block(
                    op,
                    &format!("D20E0 K4 C{start}F0 K5"),
                    &setup,
                    &format!("{on}\nDAND= C200 K123456\nDAND= C201 K234567\nANI M1067"),
                    "AND= C197 K77\nAND= C198 K78\nAND= C199 K79\nDAND= C200 K123456\nDAND= C201 K234567",
                    840,
                )],
                "MOV C197 D200\nMOV C198 D201\nMOV C199 D202\nDMOV C200 D204\nDMOV C201 D206",
            );
            c.name = c.name.replacen(
                "indexed_search_toggle_literal_oracle_",
                "indexed_search_toggle_LOCAL_ONLY_literal_oracle_",
                1,
            );
            cs.push(c);
        }
    }
    unique(&cs, 6);
    cs
}
