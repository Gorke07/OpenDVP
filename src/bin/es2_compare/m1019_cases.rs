//! M1019 storage and the newly valid four-bit M1019..M1022 group.
//! These fixtures use the installed simulator's storage-only backend. They
//! do not infer physical voltage/LED behavior or RUN/STOP lifecycle behavior.
//! M1023 is never addressed; full MUL/DIV use this group only as a source.
use super::{Case, case};
use std::collections::BTreeSet;

const TRUNCATED: [(&str, bool); 8] = [
    ("MUL16", false),
    ("MUL16P", false),
    ("DIV16", false),
    ("DIV16P", false),
    ("MUL32", true),
    ("MUL32P", true),
    ("DIV32", true),
    ("DIV32P", true),
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

fn changing_stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (
            vec![("X1", true), ("X0", false), ("X2", false), ("X7", false)],
            1,
        ),
        (vec![("X1", false), ("X0", true)], 1),
        (vec![("X2", true)], 1),
        (vec![("X0", false), ("X2", false)], 1),
        (vec![("X0", true)], 1),
    ]
}

fn index_base(index: u16) -> u16 {
    if index == 0 { 1028 } else { 1180 + index * 2 }
}

fn seed_words(base: u16, count: u16) -> String {
    (0..count)
        .map(|i| {
            format!(
                "MOV H{:04X} D{}",
                0xa55au16.wrapping_add(i * 0x1111),
                base + i
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn read_words(base: u16, count: u16, output: u16) -> String {
    (0..count)
        .map(|i| format!("MOV D{} D{}", base + i, output + i))
        .collect::<Vec<_>>()
        .join("\n")
}

fn scenario(name: &str, nibble: u8, guard: bool, seed: &str, body: &str, observe: &str) -> Case {
    assert!(nibble < 16);
    let group_seed = (0..4)
        .map(|bit| {
            format!(
                "{} M{}",
                if nibble & (1 << bit) == 0 {
                    "RST"
                } else {
                    "SET"
                },
                1019 + bit
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let guard_seed = format!("{} M1018", if guard { "SET" } else { "RST" });
    let gate = if body.starts_with("LD") { "" } else { "LD X0" };
    let source = [
        "LD X1\nMOV H55AA E0\nMOV H1234 F0",
        &guard_seed,
        &group_seed,
        "RST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068",
        seed,
        gate,
        body,
        "LD M1018\nOUT Y0\nLD M1019\nOUT Y1\nLD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24",
        "LDI X7\nMOV K1M1019 D200\nMOV K1M1018 D201\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV K1234 D306",
        observe,
        "END",
    ].into_iter().filter(|section| !section.is_empty()).collect::<Vec<_>>().join("\n");
    assert!(source.lines().count() <= 512, "oversized M1019 case {name}");
    assert!(
        !source
            .lines()
            .any(|line| line.split_whitespace().any(|word| word == "M1023")),
        "unsupported M1023 in {name}"
    );
    case(&format!("m1019_{name}"), &source, stages())
}

pub(super) fn cases() -> Vec<Case> {
    let mut cases = Vec::new();

    // Basic reads and writes preserve the three adjacent arithmetic flags.
    // LD samples the old value before a later powered update changes it.
    for op in ["LD", "OUT", "SET", "RST"] {
        for initial_on in [false, true] {
            let nibble = if initial_on { 9 } else { 6 };
            let body = if op == "LD" {
                "LD M1019\nAND X0\nOUT Y10\nLD X0\nAND X2\nSET M1019\nLD X0\nANI X2\nRST M1019"
                    .to_owned()
            } else {
                format!("{op} M1019")
            };
            let mut item = scenario(
                &format!("basic_{op}_initial_{}", u8::from(initial_on)),
                nibble,
                initial_on,
                "",
                &body,
                "",
            );
            if op == "LD" {
                item.stages = changing_stages();
            }
            cases.push(item);
        }
    }
    assert_eq!(cases.len(), 8);

    // Resolve all E/F bank words onto exactly M1019. Positive and negative
    // offsets retain the M1018 guard and exercise the new bit in every role.
    for bank in ['E', 'F'] {
        for index in 0..8u16 {
            let ordinal = index + if bank == 'F' { 8 } else { 0 };
            let op = ["LD", "OUT", "SET", "RST"][ordinal as usize % 4];
            let modifier = format!("{bank}{index}");
            let pair = index_base(index);
            let alias = pair + u16::from(bank == 'F');
            let companion = pair + u16::from(bank == 'E');
            let (base, offset) = if index % 2 == 0 {
                (1018, 1)
            } else {
                (1020, -1)
            };
            let seed = format!("MOV H6A5A D{companion}\nMOV K{offset} D{alias}");
            let body = if op == "LD" {
                format!(
                    "LD M{base}{modifier}\nAND X0\nOUT Y10\nLD X0\nAND X2\nSET M1019\nLD X0\nANI X2\nRST M1019"
                )
            } else {
                format!("{op} M{base}{modifier}")
            };
            let observe = format!(
                "MOV E{index} D250\nMOV F{index} D251\nMOV D{pair} D252\nMOV D{} D253",
                pair + 1
            );
            let mut item = scenario(
                &format!("indexed_bit_{op}_{modifier}_offset_{offset}"),
                if index % 2 == 0 { 9 } else { 6 },
                true,
                &seed,
                &body,
                &observe,
            );
            if op == "LD" {
                item.stages = changing_stages();
            }
            cases.push(item);
        }
    }
    assert_eq!(cases.len(), 24);

    // A fitting product/quotient of eight must leave M1022 set by the store.
    // The next source gives a different nibble during held power, separating
    // normal execution from P and confirming natural writes to M1020/M1021.
    for (op, wide) in TRUNCATED {
        for indexed_target in [false, true] {
            let mov = if wide { "DMOV" } else { "MOV" };
            let division = op.starts_with("DIV");
            let (value, rhs, next) = if division { (24, 3, 15) } else { (2, 4, 1) };
            let target = if indexed_target {
                "K1M1018E1"
            } else {
                "K1M1019"
            };
            let seed =
                format!("{mov} K{value} D32\nMOV K1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2");
            let body = format!("{op} D32F2 K{rhs} {target}\n{mov} K{next} D32");
            let observe = format!("{mov} D32 D220\nMOV E1 D250\nMOV F2 D251");
            cases.push(scenario(
                &format!(
                    "group_result_{op}_{}",
                    if indexed_target { "indexed" } else { "direct" }
                ),
                if indexed_target { 5 } else { 9 },
                true,
                &seed,
                &body,
                &observe,
            ));
        }
    }
    assert_eq!(cases.len(), 40);

    // Low result nibble zero: overflow must set carry AFTER the four-bit
    // write. A latched error does not suppress this group-target carry set.
    for (op, wide) in [
        ("MUL16", false),
        ("MUL16P", false),
        ("MUL32", true),
        ("MUL32P", true),
    ] {
        for error_bit in [1067, 1068] {
            let (mov, value) = if wide { ("DMOV", 65536) } else { ("MOV", 256) };
            let target = if error_bit == 1067 {
                "K1M1019"
            } else {
                "K1M1018E1"
            };
            let seed = format!(
                "{mov} K{value} D32\n{mov} K{value} D48\nMOV K1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 E3\nMOV H1234 F3\nSET M{error_bit}\nMOV H4444 D1067\nMOV K321 D1068"
            );
            let body = format!("{op} D32F2 D48E3 {target}");
            cases.push(scenario(
                &format!("overflow_clear_store_set_{op}_M{error_bit}"),
                7,
                true,
                &seed,
                &body,
                "MOV E1 D250\nMOV F2 D251\nMOV E3 D252",
            ));
        }
    }
    assert_eq!(cases.len(), 48);

    // The source's initial value is eight solely because carry was ON.
    // Read it before clearing carry, then write the captured value elsewhere.
    // A held normal visit sees zero; a held P visit keeps the first result.
    for (op, wide) in [
        ("MUL16", false),
        ("MUL16P", false),
        ("MUL32", true),
        ("MUL32P", true),
    ] {
        for group_target in [false, true] {
            let source = if group_target { "K1M1018E1" } else { "K1M1019" };
            let target = if group_target { "K1M128F2" } else { "D160F2" };
            let seed = format!(
                "{}\nSET M127\nMOV HA5 K2M128\nSET M136\nMOV K1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",
                seed_words(159, 6)
            );
            let body = format!("{op} {source} K1 {target}");
            let observe = format!(
                "{}\nMOV K4M124 D240\nMOV E1 D250\nMOV F2 D251",
                read_words(159, 6, 220)
            );
            cases.push(scenario(
                &format!(
                    "source_before_carry_clear_{op}_{}",
                    if group_target {
                        "group"
                    } else if wide {
                        "double_numeric"
                    } else {
                        "word_numeric"
                    }
                ),
                8,
                true,
                &seed,
                &body,
                &observe,
            ));
        }
    }
    assert_eq!(cases.len(), 56);

    // Full forms read the legal four-bit source and write both results into
    // ordinary D storage. No full-result destination spans into M1023.
    for (ordinal, op) in [
        "MUL", "MULP", "DIV", "DIVP", "DMUL", "DMULP", "DDIV", "DDIVP",
    ]
    .into_iter()
    .enumerate()
    {
        let source = if ordinal % 2 == 0 {
            "K1M1019"
        } else {
            "K1M1018E1"
        };
        let seed = format!(
            "{}\nMOV K0 D40\nMOV K1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",
            seed_words(159, 6)
        );
        let body = format!("{op} {source} K3 D160F2\nMOV K1M1019 D40\nRST M1022");
        let observe = format!(
            "{}\nMOV D40 D240\nMOV E1 D250\nMOV F2 D251",
            read_words(159, 6, 220)
        );
        cases.push(scenario(
            &format!("full_source_{op}_both_results_numeric"),
            10,
            true,
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 64);

    // The source starts one bit before the result. Snapshot the overlapping
    // group rather than propagating a bit just written into the next read.
    for op in ["MOV", "CML", "WAND", "WOR", "WXOR", "DMOV", "DCML", "DAND"] {
        let seed = "MOV K1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2";
        let body = match op {
            "WAND" | "DAND" => format!("{op} K1M1018F2 K10 K1M1018E1"),
            "WOR" => format!("{op} K1M1018F2 K2 K1M1018E1"),
            "WXOR" => format!("{op} K1M1018F2 K11 K1M1018E1"),
            _ => format!("{op} K1M1018F2 K1M1018E1"),
        };
        cases.push(scenario(
            &format!("overlapping_flag_group_{op}"),
            9,
            true,
            seed,
            &body,
            "MOV E1 D250\nMOV F2 D251",
        ));
    }
    assert_eq!(cases.len(), 72);

    for (op, wide) in [
        ("DIV16", false),
        ("DIV16P", false),
        ("DIV32", true),
        ("DIV32P", true),
    ] {
        let mov = if wide { "DMOV" } else { "MOV" };
        let seed = format!(
            "{mov} K-17 D32\n{mov} K3 D48\nMOV K0 D40\nMOV K0 D41\nMOV K0 D42\nMOV K1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 E3\nMOV H1234 F3"
        );
        let body = format!(
            "{mov} K3 D48\n{op} D32E3 D48F2 K1M1018E1\nMOV K1M1019 D40\n{mov} K0 D48\n{op} D32E3 D48F2 K1M1018E1\nMOV K1M1019 D41\n{mov} K-3 D48\n{op} D32E3 D48F2 K1M1018E1\nMOV K1M1019 D42"
        );
        cases.push(scenario(
            &format!("negative_zero_recovery_{op}_four_bits_preserved"),
            10,
            true,
            &seed,
            &body,
            "MOV D40 D220\nMOV D41 D221\nMOV D42 D222\nMOV E1 D250\nMOV F2 D251",
        ));
    }
    assert_eq!(cases.len(), 76);

    cases.push(scenario(
        "control_mps_source_snapshot_before_clear",
        8, true,
        "MOV K1 E1\nMOV H6A5A F1\nMOV H5AA5 D160\nMOV K0 D40",
        "MPS\nSET M1019\nMRD\nMUL16 K1M1018E1 K1 D160\nMRD\nMOV K1M1019 D40\nMRD\nRST M1019\nMPP\nOUT Y10",
        "MOV D160 D220\nMOV D40 D221\nMOV E1 D250",
    ));
    cases.push(scenario(
        "control_for_same_pc_pulse_true_false_true",
        9, true,
        "MOV K1 E1\nMOV H6A5A F1\nMOV K0 E2\nMOV H7BBB F2\nDMOV K0 D30\nMOV K0 D50",
        "LDI X7\nDMOV K0 D30\nFOR K3\nLD= D30 K0\nOR= D30 K2\nAND X0\nMUL32P D30E2 K4 K1M1018E1\nLDI X7\nMOV K1M1019 D50\nINC D30\nNEXT",
        "DMOV D30 D220\nMOV D50 D222\nMOV E1 D250\nMOV E2 D251",
    ));
    let mut jumped = scenario(
        "control_cj_skipped_out_preserves_storage",
        9,
        false,
        "MOV K0 D50",
        "LDI X0\nCJ P10\nLD X2\nOUT M1019\nP10\nLDI X7\nMOV K1M1019 D50",
        "MOV D50 D220",
    );
    jumped.stages = changing_stages();
    cases.push(jumped);
    cases.push(scenario(
        "control_mps_indexed_set_reset_live_snapshots",
        9, true,
        "MOV H6A5A E1\nMOV K-1 F1\nMOV K0 D40\nMOV K0 D41",
        "MPS\nSET M1020F1\nMRD\nMOV K1M1019 D40\nMRD\nRST M1020F1\nMRD\nMOV K1M1019 D41\nMPP\nOUT Y10",
        "MOV D40 D220\nMOV D41 D221\nMOV E1 D250\nMOV F1 D251",
    ));

    assert_eq!(cases.len(), 80);
    assert_eq!(
        cases
            .iter()
            .map(|item| &item.name)
            .collect::<BTreeSet<_>>()
            .len(),
        cases.len()
    );
    let actual: BTreeSet<_> = cases
        .iter()
        .flat_map(|item| item.source.lines())
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    for (op, _) in TRUNCATED {
        assert!(actual.contains(op), "missing M1019 truncated form {op}");
    }
    cases
}
