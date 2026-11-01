//! Addressed block/fill/exchange operand scenarios. Results live in run reports.
//! Forward live BMOV, one fill snapshot/cursor, and late XCH writers are
//! deliberately distinct. Known low-M mappings and unsafe tails are excluded.
use super::{Case as ComparisonCase, Target, case};
use std::collections::BTreeSet;

const FORMS: [&str; 10] = [
    "BMOV", "BMOVP", "FMOV", "FMOVP", "DFMOV", "DFMOVP", "XCH", "XCHP", "DXCH", "DXCHP",
];
const INPUTS: [&str; 48] = [
    "X100", "X101", "X102", "X103", "X104", "X105", "X106", "X107", "X110", "X111", "X112", "X113",
    "X114", "X115", "X116", "X117", "X120", "X121", "X122", "X123", "X124", "X125", "X126", "X127",
    "X130", "X131", "X132", "X133", "X134", "X135", "X136", "X137", "X140", "X141", "X142", "X143",
    "X144", "X145", "X146", "X147", "X150", "X151", "X152", "X153", "X154", "X155", "X156", "X157",
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
fn addr(area: &str, index: u16) -> String {
    if matches!(area, "X" | "Y") {
        format!("{area}{index:o}")
    } else {
        format!("{area}{index}")
    }
}
fn mov(wide: bool) -> &'static str {
    if wide { "DMOV" } else { "MOV" }
}
fn seed_words(area: &str, start: u16, count: u16) -> String {
    (0..count)
        .map(|n| {
            format!(
                "MOV H{:04X} {area}{}",
                0xa55au16.wrapping_add(n * 0x111),
                start + n
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn read_words(area: &str, start: u16, count: u16, out: u16) -> String {
    (0..count)
        .map(|n| format!("MOV {area}{} D{}", start + n, out + n))
        .collect::<Vec<_>>()
        .join("\n")
}
fn seed_bits(area: &str, start: u16, count: u16, value: u64) -> String {
    (0..count)
        .map(|n| {
            format!(
                "{} {}",
                if value & (1u64 << (n % 64)) != 0 {
                    "SET"
                } else {
                    "RST"
                },
                addr(area, start + n)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn read_bits(area: &str, start: u16, count: u16, out: u16) -> String {
    assert_eq!(count % 4, 0);
    (0..count.div_ceil(16))
        .map(|n| {
            format!(
                "MOV K{}{} D{}",
                (count - n * 16).min(16) / 4,
                addr(area, start + n * 16),
                out + n
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
    let source=[
        "LD X1\nMOV K0 E0\nMOV K0 F0\nRST M1303\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV H1234 D1067\nMOV K77 D1068\nRST M790\nRST M791",
        seed,gate,body,
        "LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24\nLD M1303\nOUT Y25\nLDI X7\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305\nMOV K1234 D306",
        observe,"END"].into_iter().filter(|s|!s.is_empty()).collect::<Vec<_>>().join("\n");
    assert!(source.lines().count() <= 512, "source budget {name}");
    //13 WORD is the largest application instruction emitted in these sources.
    assert!(
        source.lines().count() * 13 <= 0x1ef0,
        "conservative WORD budget {name}"
    );
    assert!(
        source.lines().any(|line| {
            let f: Vec<_> = line.split_whitespace().collect();
            f.first().is_some_and(|op| FORMS.contains(op))
                && f[1..].iter().any(|v| {
                    let b = v.as_bytes();
                    !v.starts_with(['H', 'E', 'F'])
                        && b.len() >= 4
                        && matches!(b[b.len() - 2], b'E' | b'F')
                        && b[b.len() - 1].is_ascii_digit()
                })
        }),
        "missing indexed block/exchange: {name}"
    );
    case(&format!("indexed_block_exchange_{name}"), &source, stages())
}
fn inputs(entry: &mut ComparisonCase) {
    for ((values, _), pattern) in entry.stages.iter_mut().zip([
        0x1234_5678_9abcu64,
        0x3141_5926_5358,
        0x2718_2818_2845,
        0x9876_5432_10fe,
    ]) {
        values.extend(
            INPUTS
                .into_iter()
                .enumerate()
                .map(|(n, input)| (input, pattern & (1u64 << n) != 0)),
        );
    }
}
fn check(cases: &[ComparisonCase], count: usize) {
    assert_eq!(cases.len(), count);
    assert_eq!(
        cases.iter().map(|c| &c.name).collect::<BTreeSet<_>>().len(),
        count
    );
    let ops: BTreeSet<_> = cases
        .iter()
        .flat_map(|c| c.source.lines())
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    for name in FORMS {
        assert!(ops.contains(name), "missing {name}");
    }
}
pub(super) fn common_cases() -> Vec<ComparisonCase> {
    let mut cases = Vec::new();
    append_banks(&mut cases);
    append_packed_blocks(&mut cases);
    append_packed_exchange(&mut cases);
    append_oracles(&mut cases);
    append_counts(&mut cases);
    append_controls(&mut cases);
    append_flag_aliases(&mut cases);
    check(&cases, 116);
    cases
}

fn append_banks(cases: &mut Vec<ComparisonCase>) {
    for wide in [false, true] {
        for bank in ['E', 'F'] {
            for n in 0..8u16 {
                let modifier = format!("{bank}{n}");
                let offset = if n % 2 == 0 { -1 } else { 1 };
                let p = if n % 2 == 0 { "" } else { "P" };
                let source = ["D", "T", "C"][usize::from(n) % 3];
                let target = ["T", "C", "D"][usize::from(n) % 3];
                let pairs = if wide {
                    vec![(40, 120), (80, 160)]
                } else {
                    vec![(40, 120), (60, 140), (80, 160)]
                };
                let mut seed = vec![format!("MOV K{offset} {modifier}")];
                let mut observe = vec![format!("MOV {modifier} D280")];
                // F's adjacent word is poison, never a high offset half.
                if bank == 'F' && (1..7).contains(&n) {
                    seed.push(format!("MOV H6A5A D{}", 1182 + n * 2));
                }
                for (slot, (s, d)) in pairs.iter().copied().enumerate() {
                    seed.push(seed_words(source, s - 2, 10));
                    seed.push(seed_words(target, d - 2, 10));
                    observe.push(read_words(source, s - 2, 10, 200 + slot as u16 * 20));
                    observe.push(read_words(target, d - 2, 10, 210 + slot as u16 * 20));
                }
                let body = if wide {
                    format!(
                        "DFMOV{p} {source}40{modifier} {target}120{modifier} H3\nDXCH{p} {source}80{modifier} {target}160{modifier}\nDMOV H12345678 {source}40{modifier}\nMOV K{} {modifier}",
                        -offset
                    )
                } else {
                    format!(
                        "BMOV{p} {source}40{modifier} {target}120{modifier} K3\nFMOV{p} {source}60{modifier} {target}140{modifier} H3\nXCH{p} {source}80{modifier} {target}160{modifier}\nMOV H1234 {source}60{modifier}\nMOV K{} {modifier}",
                        -offset
                    )
                };
                cases.push(scenario(
                    &format!(
                        "banks_{}_{}_{source}_to_{target}",
                        if wide { "double" } else { "word" },
                        modifier
                    ),
                    &seed.join("\n"),
                    &body,
                    &observe.join("\n"),
                ));
            }
        }
    }
    assert_eq!(cases.len(), 32);
}

fn append_packed_blocks(cases: &mut Vec<ComparisonCase>) {
    for op in ["BMOV", "BMOVP", "FMOV", "FMOVP", "DFMOV", "DFMOVP"] {
        let block = op.starts_with("BMOV");
        let wide = op.starts_with('D');
        let max = if wide { 8 } else { 4 };
        for digits in 1..=max {
            let target_area = ["Y", "M", "S"][usize::from(digits - 1) % 3];
            let source_area = if block {
                ["X", "Y", "M", "S"][usize::from(digits - 1)]
            } else if digits == 1 {
                "X"
            } else {
                "S"
            };
            let source_digits = if block {
                digits
            } else if wide {
                8
            } else {
                4
            };
            let source_count = if block { 3 } else { 1 };
            let source_bits = source_digits * 4 * source_count;
            let target_bits = digits * 4 * 3;
            let mut seed = vec![
                format!(
                    "MOV K1 E1\nMOV K1 E2\nMOV H5678 F1\nSET {}\nRST {}",
                    addr(target_area, 127),
                    addr(target_area, 128 + target_bits)
                ),
                seed_bits(target_area, 128, target_bits, 0xa55a_a55a_a55a_a55a),
            ];
            if source_area != "X" {
                seed.push(seed_bits(
                    source_area,
                    64,
                    source_bits,
                    0x1234_5678_9abc_def0,
                ));
                seed.push(format!(
                    "SET {}\nRST {}",
                    addr(source_area, 63),
                    addr(source_area, 64 + source_bits)
                ));
            }
            let source = if !block && !wide && digits == 2 {
                "F1".into()
            } else {
                format!("K{source_digits}{}E2", addr(source_area, 63))
            };
            let target = format!("K{digits}{}E1", addr(target_area, 127));
            let advance = if source == "F1" {
                "MOV H4321 F1".into()
            } else if source_area != "X" {
                format!(
                    "{} {} K{source_digits}{}",
                    mov(wide),
                    if wide { "H31415926" } else { "H2345" },
                    addr(source_area, 64)
                )
            } else {
                String::new()
            };
            let body = format!("{op} {source} {target} K3\n{advance}");
            let observe = format!(
                "{}\n{}\nMOV E1 D280\nMOV E2 D281\nMOV F1 D282\nLD {}\nOUT Y10\nLD {}\nOUT Y11",
                read_bits(source_area, 64, source_bits, 200),
                read_bits(target_area, 128, target_bits, 220),
                addr(target_area, 127),
                addr(target_area, 128 + target_bits)
            );
            let mut entry = scenario(
                &format!("packed_{op}_K{digits}_{source_area}_to_{target_area}"),
                &seed.join("\n"),
                &body,
                &observe,
            );
            if source_area == "X" {
                inputs(&mut entry);
            }
            cases.push(entry);
        }
    }
    assert_eq!(cases.len(), 64);
}

fn append_packed_exchange(cases: &mut Vec<ComparisonCase>) {
    for op in ["XCH", "XCHP", "DXCH", "DXCHP"] {
        let wide = op.starts_with('D');
        let max = if wide { 8u16 } else { 4 };
        let m = mov(wide);
        for area in ["Y", "M", "S"] {
            let seed = format!(
                "MOV K1 E1\nMOV K1 F1\nSET {}\nRST {}\nSET M63\nRST M{}",
                addr(area, 127),
                addr(area, 128 + max * 4),
                64 + max * 4
            );
            let mut body = Vec::new();
            for digits in 1..=max {
                body.push(format!("LDI X7\n{m} {} K{max}{}\n{m} {} K{max}M64\n{} M1303\nLD X0\n{op} K{digits}{}E1 K{}M63F1\nLDI X7\n{m} K{max}{} D{}\n{m} K{max}M64 D{}",if wide{"H12345678"}else{"H1234"},addr(area,128),if wide{"H89ABCDEF"}else{"HABCD"},if digits%2==0{"SET"}else{"RST"},addr(area,127),max+1-digits,addr(area,128),80+(digits-1)*4,82+(digits-1)*4));
            }
            cases.push(scenario(
                &format!("packed_width_roles_{op}_{area}"),
                &seed,
                &body.join("\n"),
                &format!(
                    "{}\nLD {}\nOUT Y10\nLD {}\nOUT Y11\nLD M63\nOUT Y12\nLD M{}\nOUT Y13",
                    read_words("D", 80, max * 4, 200),
                    addr(area, 127),
                    addr(area, 128 + max * 4),
                    64 + max * 4
                ),
            ));
        }
    }
    assert_eq!(cases.len(), 76);
}

// Each expectation is a fixed public readback (wide?, operand, ON value,
// OFF/held value). The condition is independent of production block helpers.
type Expected<'a> = (bool, &'a str, u32, u32);
fn oracle_block(
    op: &str,
    operands: &str,
    setup: &str,
    expected: &[Expected<'_>],
    marker: u16,
) -> String {
    let on = expected
        .iter()
        .map(|(wide, operand, value, _)| {
            format!(
                "{} {operand} H{value:X}",
                if *wide { "DAND=" } else { "AND=" }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let off = expected
        .iter()
        .map(|(wide, operand, _, value)| {
            format!(
                "{} {operand} H{value:X}",
                if *wide { "DAND=" } else { "AND=" }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "LDI X7\n{setup}\nLD X0\n{op} {operands}\nLD M791\n{on}\nLDI M791\n{off}\nORB\nAND M1020\nANI M1021\nAND M1022\nANI M1067\nANI M1068\nAND= D1067 H1234\nAND= D1068 K77\nOUT M{marker}"
    )
}
fn oracle_case(name: &str, op: &str, blocks: &[String], observe: &str) -> ComparisonCase {
    let body = format!(
        "LD X0\n{}OUT M791\n{}\nLD X0\nOUT M790",
        if op.ends_with('P') { "ANI M790\n" } else { "" },
        blocks.join("\n")
    );
    scenario(&format!("literal_oracle_{name}_{op}"), "", &body, observe)
}

fn append_oracles(cases: &mut Vec<ComparisonCase>) {
    let word_seed = "MOV K0 E0\nMOV K0 F0\nMOV K1 D10\nMOV K2 D11\nMOV K3 D12\nMOV K4 D13\nMOV H55AA D9\nMOV H5AA5 D14";
    for (name, operands, result) in [
        ("forward_numeric", "D10E0 D11F0 K3", [1, 1, 1, 1]),
        ("reverse_numeric", "D11E0 D10F0 K3", [2, 3, 4, 4]),
    ] {
        let expect = [
            (false, "D10", result[0], 1),
            (false, "D11", result[1], 2),
            (false, "D12", result[2], 3),
            (false, "D13", result[3], 4),
            (false, "D9", 0x55aa, 0x55aa),
            (false, "D14", 0x5aa5, 0x5aa5),
        ];
        cases.push(oracle_case(
            name,
            "BMOV",
            &[oracle_block("BMOV", operands, word_seed, &expect, 840)],
            &read_words("D", 9, 6, 220),
        ));
    }
    for (name, operands, result) in [
        ("forward_nibbles", "K1M128E0 K1M132F0 K3", 0x1111),
        ("reverse_nibbles", "K1M132E0 K1M128F0 K3", 0x4432),
    ] {
        let setup = "MOV K0 E0\nMOV K0 F0\nMOV H4321 K4M128\nMOV H8 K1M124\nMOV HA K1M144";
        let expected = [
            (false, "K4M128", result, 0x4321),
            (false, "K1M124", 8, 8),
            (false, "K1M144", 10, 10),
        ];
        cases.push(oracle_case(
            name,
            "BMOVP",
            &[oracle_block("BMOVP", operands, setup, &expected, 840)],
            "MOV K4M128 D220\nMOV K1M124 D221\nMOV K1M144 D222",
        ));
    }
    for op in ["BMOV", "BMOVP"] {
        let setup = "MOV K4 D1182\nMOV K1 F1\nMOV K0 E2\nMOV H7777 F2\nMOV H5AA5 E3\nMOV K1 D10\nMOV K2 D11\nMOV K3 D12\nMOV K4 D13\nMOV K99 D16";
        let expected = [
            (false, "D1182", 1, 4),
            (false, "D1183", 2, 1),
            (false, "D1184", 3, 0),
            (false, "D1185", 4, 0x7777),
            (false, "D1186", 0x5aa5, 0x5aa5),
            (false, "D16", 99, 99),
        ];
        cases.push(oracle_case(
            "both_bases_and_count_cached",
            op,
            &[oracle_block(
                op,
                "D10E2 D1181F1 D1182",
                setup,
                &expected,
                840,
            )],
            &format!(
                "{}\n{}",
                read_words("D", 1182, 5, 220),
                read_words("D", 10, 4, 230)
            ),
        ));
    }
    for op in ["FMOV", "FMOVP"] {
        let setup = "MOV K1 E1\nMOV K1 E2\nMOV H1234 K4M128\nMOV HA55A K4M144\nMOV H5AA5 K4M160\nMOV HB K1M132\nMOV H8 K1M124\nMOV H5 K1M176";
        let expected = [
            (false, "K4M128", 11, 0x12b4),
            (false, "K4M144", 11, 0xa55a),
            (false, "K4M160", 11, 0x5aa5),
            (false, "K1M124", 8, 8),
            (false, "K1M176", 5, 5),
        ];
        cases.push(oracle_case(
            "short_source_destroyed_by_first_fill",
            op,
            &[oracle_block(
                op,
                "K1M131E2 K4M127E1 K3",
                setup,
                &expected,
                840,
            )],
            &read_bits("M", 124, 56, 220),
        ));
    }
    for op in ["DFMOV", "DFMOVP"] {
        let setup = "MOV H5678 E1\nMOV H1234 F1\nMOV HAAAA E2\nMOV HBBBB F2\nMOV K1 E3\nMOV HCCCC F3\nMOV HDDDD E4\nMOV H5AA5 F4";
        let expected = [
            (false, "D1182", 0x5678, 0x5678),
            (false, "D1183", 0x5678, 0x1234),
            (false, "D1184", 0x1234, 0xaaaa),
            (false, "D1185", 0x5678, 0xbbbb),
            (false, "D1186", 0x1234, 1),
            (false, "D1187", 0x5678, 0xcccc),
            (false, "D1188", 0x1234, 0xdddd),
            (false, "D1189", 0x5aa5, 0x5aa5),
        ];
        cases.push(oracle_case(
            "source_snapshot_and_cached_fill_cursor",
            op,
            &[oracle_block(op, "E1 D1182E3 K3", setup, &expected, 840)],
            &read_words("D", 1182, 8, 220),
        ));
    }
    for op in ["XCH", "XCHP", "DXCH", "DXCHP"] {
        let wide = op.starts_with('D');
        let setup = if wide {
            "RST M1303\nMOV K0 E1\nMOV H1234 F1\nMOV K0 E2\nMOV K1 D10\nMOV K0 D11\nMOV K77 D12\nMOV K88 D13"
        } else {
            "RST M1303\nMOV K0 E1\nMOV K0 E2\nMOV K1 D10\nMOV K77 D11\nMOV K88 D12"
        };
        let expected = if wide {
            vec![
                (false, "D10", 1, 1),
                (false, "D11", 0, 0),
                (false, "D12", 0x1234, 77),
                (false, "D13", 88, 88),
                (false, "E1", 1, 0),
                (false, "F1", 0, 0x1234),
            ]
        } else {
            vec![
                (false, "D10", 1, 1),
                (false, "D11", 0, 77),
                (false, "D12", 88, 88),
                (false, "E1", 1, 0),
            ]
        };
        cases.push(oracle_case(
            "second_write_redirects_first_index",
            op,
            &[oracle_block(op, "D10E1 D1182E2", setup, &expected, 840)],
            "MOV D10 D220\nMOV D11 D221\nMOV D12 D222\nMOV D13 D223\nMOV E1 D224\nMOV F1 D225",
        ));
    }
    for op in ["XCH", "XCHP", "DXCH", "DXCHP"] {
        let wide = op.starts_with('D');
        let m = mov(wide);
        let max = if wide { 8 } else { 4 };
        let first = if wide { 0x12345678 } else { 0x1234 };
        let second = if wide { 0x89abcdef } else { 0xabcd };
        let swapped = if wide { 0xcdef89ab } else { 0xcdab };
        let grouped = if wide { 0x56781234 } else { 0x3412 };
        let reverse = if wide { 0x12345670 } else { 0x1230 };
        let mut blocks = Vec::new();
        blocks.push(oracle_block(
            op,
            "E1 D182E0",
            &format!("SET M1303\nMOV K0 E0\n{m} H{first:X} E1\n{m} H{second:X} D182"),
            &[(wide, "E1", first, first), (wide, "D182", swapped, second)],
            840,
        ));
        blocks.push(oracle_block(
            op,
            "E1 D1182E0",
            &format!("SET M1303\nMOV K0 E0\n{m} H{first:X} E1"),
            &[(wide, "E1", first, first)],
            841,
        ));
        let setup = format!("SET M1303\nMOV K1 E1\nMOV K1 F1\n{m} H{first:X} K{max}M128");
        let full = format!("K{max}M128");
        blocks.push(oracle_block(
            op,
            &format!("K1M127E1 K{max}M127F1"),
            &setup,
            &[(wide, &full, grouped, first)],
            842,
        ));
        blocks.push(oracle_block(
            op,
            &format!("K{max}M127E1 K1M127F1"),
            &setup,
            &[(wide, &full, reverse, first)],
            843,
        ));
        cases.push(oracle_case(
            "M1303_keys_and_second_width",
            op,
            &blocks,
            &format!("{m} K{max}M128 D220\nMOV K1M840 D222"),
        ));
    }
    for (op, operands, result) in [
        ("DXCH", "D10E0 D11F0", [0x2222, 0x3333, 0x2222, 0x4444]),
        ("DXCHP", "D11E0 D10F0", [0x2222, 0x1111, 0x2222, 0x4444]),
    ] {
        let setup = "RST M1303\nMOV K0 E0\nMOV K0 F0\nMOV H1111 D10\nMOV H2222 D11\nMOV H3333 D12\nMOV H4444 D13";
        let expected = [
            (false, "D10", result[0], 0x1111),
            (false, "D11", result[1], 0x2222),
            (false, "D12", result[2], 0x3333),
            (false, "D13", result[3], 0x4444),
        ];
        cases.push(oracle_case(
            "wide_overlap_second_then_first",
            op,
            &[oracle_block(op, operands, setup, &expected, 840)],
            &read_words("D", 10, 4, 220),
        ));
    }
    assert_eq!(cases.len(), 96);
}

fn append_counts(cases: &mut Vec<ComparisonCase>) {
    for op in ["BMOV", "BMOVP"] {
        for counter in ["D50", "T50", "C50"] {
            let mut blocks = Vec::new();
            for (slot, n) in [0, -1, 513, 1, 3].into_iter().enumerate() {
                let result = match n {
                    1 => [1, 9, 9],
                    3 => [1, 2, 3],
                    _ => [9, 9, 9],
                };
                let setup = format!(
                    "MOV K0 E0\nMOV K0 F0\nMOV K{n} {counter}\nMOV K1 D10\nMOV K2 D11\nMOV K3 D12\nMOV K9 D20\nMOV K9 D21\nMOV K9 D22\nMOV H55AA D19\nMOV H5AA5 D23"
                );
                let expected = [
                    (false, "D20", result[0], 9),
                    (false, "D21", result[1], 9),
                    (false, "D22", result[2], 9),
                    (false, "D19", 0x55aa, 0x55aa),
                    (false, "D23", 0x5aa5, 0x5aa5),
                ];
                blocks.push(oracle_block(
                    op,
                    &format!("D10E0 D20F0 {counter}"),
                    &setup,
                    &expected,
                    840 + slot as u16,
                ));
            }
            cases.push(oracle_case(
                &format!("dynamic_count_domain_{counter}"),
                op,
                &blocks,
                &format!("{}\nMOV {counter} D230", read_words("D", 19, 5, 220)),
            ));
        }
        // Two disjoint complete 512-word spans. The repeated value plus three
        // distinct positions catches both short copies and count truncation.
        let setup = "MOV K0 E0\nMOV K0 F0\nMOV K512 D50\nFMOV H1234 D2000 K512\nMOV H1111 D2000\nMOV H2222 D2256\nMOV H3333 D2511\nFMOV H5AA5 D3000 K512\nMOV H55AA D2999\nMOV HA55A D3512";
        let expected = [
            (false, "D3000", 0x1111, 0x5aa5),
            (false, "D3001", 0x1234, 0x5aa5),
            (false, "D3256", 0x2222, 0x5aa5),
            (false, "D3510", 0x1234, 0x5aa5),
            (false, "D3511", 0x3333, 0x5aa5),
            (false, "D2999", 0x55aa, 0x55aa),
            (false, "D3512", 0xa55a, 0xa55a),
        ];
        cases.push(oracle_case("maximum_512_complete_spans",op,&[oracle_block(op,"D2000E0 D3000F0 D50",setup,&expected,840)],"MOV D2999 D220\nMOV D3000 D221\nMOV D3001 D222\nMOV D3256 D223\nMOV D3510 D224\nMOV D3511 D225\nMOV D3512 D226"));
    }
    assert_eq!(cases.len(), 104);
}

fn append_controls(cases: &mut Vec<ComparisonCase>) {
    for (slot, op) in FORMS.into_iter().enumerate() {
        let seed = format!(
            "{}\n{}\nMOV K1 E1\nMOV K1 F1\nMOV K0 D60\nZRST S0 S1023\nRST M1040\nRST M1045\nRST M1047",
            seed_words("D", 38, 14),
            seed_words("D", 158, 18)
        );
        let count = if op.contains("MOV") { " K3" } else { "" };
        let operands = format!("D40E1 D160F1{count}");
        // Each base appears only at safe complete spans, including skipped
        // and held visits. The suffix values are restored outside X0 gating.
        let (label, body) = match slot % 5 {
            0 => (
                "for_same_pc",
                format!("LD X0\nFOR K3\nLD X0\n{op} {operands}\nNEXT\nLDI X7\nINC D60"),
            ),
            1 => (
                "mps_live_offsets",
                format!(
                    "LD X0\nMPS\n{op} {operands}\nMRD\nMOV K4 E1\nMOV K5 F1\nMRD\n{op} {operands}\nMPP\nOUT Y10\nLDI X7\nMOV K1 E1\nMOV K1 F1\nINC D60"
                ),
            ),
            2 => (
                "cj_skips_off_visit",
                format!("LD X2\nCJ P10\nLD X0\n{op} {operands}\nP10\nLDI X7\nINC D60"),
            ),
            3 => (
                "master_false_true_accumulator",
                format!("LD X7\nMC N0\nLDI X7\n{op} {operands}\nMCR N0\nLDI X7\nINC D60"),
            ),
            _ => (
                "inactive_step_master_false",
                format!("STL S0\nLDI X7\n{op} {operands}\nRET\nLDI X7\nINC D60"),
            ),
        };
        let mut entry = scenario(
            &format!("control_{label}_{op}"),
            &seed,
            &body,
            &format!(
                "{}\n{}\nMOV D60 D250\nMOV E1 D251\nMOV F1 D252\nMOV K1S0 D253",
                read_words("D", 38, 14, 200),
                read_words("D", 158, 18, 220)
            ),
        );
        if slot % 5 == 2 {
            entry.stages = vec![
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
        cases.push(entry);
    }
    assert_eq!(cases.len(), 114);
}

fn append_flag_aliases(cases: &mut Vec<ComparisonCase>) {
    for p in ["", "P"] {
        let seed = "MOV K0 E0\nMOV K1 E1\nMOV K1 F4\nSET M1018";
        // K1M1019 is exactly the supported four-bit span 1019..1022.
        // No unsupported M1023 neighbor is read or written. Explicit payload
        // changes to ALU relays are observed rather than called flag holds.
        let body = format!(
            "LDI X7\nMOV H9 K1M1019\nMOV H6 K1M128\nLD X0\nBMOV{p} K1M1018E1 K1M127F4 K1\nLDI X7\nMOV K1M128 D80\nMOV K1M1019 D81\nMOV H5 K1M1019\nMOV H9 D10\nLD X0\nFMOV{p} D10E0 K1M1018E1 K1\nLDI X7\nMOV K1M1019 D82\nMOV H6 K1M1019\nDMOV H8000000B D10\nLD X0\nDFMOV{p} D10E0 K1M1018E1 K1\nLDI X7\nMOV K1M1019 D83\nMOV H5 K1M1019\nMOV HA F1\nLD X0\nXCH{p} K1M1018E1 F1\nLDI X7\nMOV K1M1019 D84\nMOV F1 D85\nMOV H3 K1M1019\nDMOV H1234567C D20\nLD X0\nDXCH{p} K1M1018E1 D20E0\nLDI X7\nMOV K1M1019 D86\nDMOV D20 D87"
        );
        cases.push(scenario(
            &format!("explicit_flag_group_and_direct_F_word_{p}"),
            seed,
            &body,
            &format!(
                "{}\nLD M1018\nOUT Y10\nMOV E1 D250\nMOV F4 D251",
                read_words("D", 80, 9, 220)
            ),
        ));
    }
    assert_eq!(cases.len(), 116);
}

pub(super) fn model_cases(target: Target) -> Vec<ComparisonCase> {
    let last: u16 = match target {
        Target::Es2 => 9999,
        Target::Ss2 => 4999,
    };
    let mut cases = Vec::new();
    for op in FORMS {
        let wide = op.starts_with('D');
        let m = mov(wide);
        let mut seed = vec![
            seed_words("D", last - 5, 6),
            seed_words("D", 38, 10),
            "MOV K0 E2\nMOV K0 F1\nMOV K0 E3\nMOV H6A5A F3".into(),
        ];
        let mut body = Vec::new();
        let mut observe = vec![
            read_words("D", last - 5, 6, 200),
            read_words("D", 38, 10, 210),
        ];
        if op.starts_with("BMOV") {
            seed.push("MOV K-1 E1\nMOV K-1 F1".into());
            seed.push(seed_words("T", 252, 4));
            seed.push(seed_words("C", 196, 4));
            // First source ends at Dlast; the second destination does too.
            body.push(format!(
                "LD X0\n{op} D{last}E1 D40E2 K2\n{op} D44E2 D{last}F1 K2\n{op} T254E2 C198E2 K2\n{op} C198E2 D46E2 K2"
            ));
            observe.push(format!(
                "{}\n{}",
                read_words("T", 252, 4, 220),
                read_words("C", 196, 4, 224)
            ));
        } else if op.starts_with("FMOV") {
            seed.push("MOV K-1 E1".into());
            seed.push(seed_words("T", 252, 4));
            seed.push(seed_words("C", 196, 4));
            body.push(format!(
                "LD X0\n{op} D40E2 D{last}E1 K2\n{op} D40E2 T254E2 K2\n{op} D40E2 C198E2 K2"
            ));
            observe.push(format!(
                "{}\n{}",
                read_words("T", 252, 4, 220),
                read_words("C", 196, 4, 224)
            ));
        } else if op.starts_with("DFMOV") {
            seed.push("MOV K-3 E1".into());
            body.push(format!("LD X0\n{op} D40E2 D{last}E1 K2"));
            // Both indexed and ordinary DFMOV use the new complete cursor.
            // Every pair is complete: C199's documented low-only element
            // advances by one into natural 32-bit C200, then C201.
            for (slot, start) in [197u16, 198, 199].into_iter().enumerate() {
                let count = if start == 197 { 3 } else { 2 };
                body.push(format!("LDI X7\n{}\nDMOV H89ABCDEF C200\nDMOV H76543210 C201\nLD X0\n{op} H12345678 C{start}E3 K{count}\nLDI X7\n{}\nDMOV C200 D{}\nDMOV C201 D{}",seed_words("C",196,4),read_words("C",196,4,80+slot as u16*8),84+slot as u16*8,86+slot as u16*8));
            }
            body.push(format!("LDI X7\nMOV H1111 C198\nMOV H2222 C199\nDMOV H89ABCDEF C200\nLD X0\n{op} H12345678 C199 K2\nLDI X7\nMOV C198 D104\nMOV C199 D105\nDMOV C200 D106"));
            // Single natural C255 is complete, while the adjacent C254 is
            // a sentinel and is never accidentally treated as a high word.
            body.push(format!("LDI X7\nDMOV H89ABCDEF C254\nDMOV H76543210 C255\nLD X0\n{op} H12345678 C255E3 K1\nLDI X7\nDMOV C254 D108\nDMOV C255 D110"));
            observe.push(read_words("D", 80, 32, 220));
        } else {
            seed.push(format!("MOV K{} E1", if wide { -1 } else { 0 }));
            body.push(format!("LD X0\n{op} D{last}E1 D40E2"));
            // Complete timer pair and C199/C200 reclassification fixtures.
            seed.push(seed_words("T", 252, 4));
            seed.push("MOV H1111 C198\nMOV H2222 C199\nDMOV H89ABCDEF C200\nDMOV H76543210 C201\nDMOV H24681357 C254\nDMOV H13572468 C255".into());
            if wide {
                body.push(format!("LDI X7\nMOV K-1 E3\nLD X0\n{op} T255E3 D42E2\n{op} C200E3 D44E2\nLDI X7\nMOV K1 E3\nLD X0\n{op} C199E3 C255E2"));
            } else {
                body.push(format!("LDI X7\nMOV K0 E3\nLD X0\n{op} T255E3 D42E2\n{op} C199E3 D44E2\nLDI X7\nMOV K-1 E3\nLD X0\n{op} C199E3 D46E2"));
            }
            observe.push(format!("{}\nMOV C198 D224\nMOV C199 D225\nDMOV C200 D226\nDMOV C201 D228\nDMOV C254 D230\nDMOV C255 D232",read_words("T",252,4,220)));
        }
        // Full legal packed spans end exactly at each model-common area end.
        // Their lexical base is the final bit; a negative WORD offset rescues
        // the full operation span before either reader/writer is qualified.
        let digits = if wide { 8 } else { 4 };
        let span = if op.contains("MOV") {
            digits * 4 * 2
        } else {
            digits * 4
        };
        seed.push(format!("MOV K-{} E4\nMOV K0 F4", span - 1));
        for (slot, (area, end)) in [("Y", 255u16), ("M", 4095), ("S", 1023)]
            .into_iter()
            .enumerate()
        {
            seed.push(seed_bits(
                area,
                end - span - 3,
                span + 4,
                0x1234_5678_9abc_def0,
            ));
            seed.push(format!(
                "{m} {} D{}",
                if wide { "H31415926" } else { "H1357" },
                60 + slot as u16 * 4
            ));
            let count = if op.contains("MOV") { " K2" } else { "" };
            body.push(format!(
                "LD X0\n{op} D{}F4 K{digits}{}E4{count}",
                60 + slot as u16 * 4,
                addr(area, end)
            ));
            observe.push(read_bits(
                area,
                end - span - 3,
                span + 4,
                260 + slot as u16 * 5,
            ));
        }
        observe.push("MOV E1 D280\nMOV E2 D281\nMOV E3 D282\nMOV E4 D283".into());
        cases.push(scenario(
            &format!("model_{}_complete_ends_{op}", target.name()),
            &seed.join("\n"),
            &body.join("\n"),
            &observe.join("\n"),
        ));
    }
    check(&cases, 10);
    cases
}
