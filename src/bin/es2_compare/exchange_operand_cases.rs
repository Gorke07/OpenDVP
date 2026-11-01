//! Writable XCH operand and native write-order differentials. These fixtures
//! become compatibility evidence only after independent simulator execution.
use super::{Case, case};
use std::collections::BTreeSet;

const REQUIRED_SPELLINGS: &str = "XCH XCHP DXCH DXCHP";

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (vec![("X1", true), ("X0", false)], 1),
        (vec![("X1", false), ("X0", true)], 2),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}

fn address(area: &str, index: u16) -> String {
    if area == "Y" {
        format!("Y{index:o}")
    } else {
        format!("{area}{index}")
    }
}

fn seed_bits(area: &str, base: u16, bits: u16, value: u32) -> String {
    (0..bits)
        .map(|bit| {
            format!(
                "{} {}",
                if value & (1u32 << bit) != 0 {
                    "SET"
                } else {
                    "RST"
                },
                address(area, base + bit)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn scenario(
    name: &str,
    seed: &str,
    body: &str,
    swap_halves: bool,
    guards: &[(&str, bool)],
    observe: &str,
) -> Case {
    let guard_seed = guards
        .iter()
        .map(|(device, on)| format!("{} {device}", if *on { "SET" } else { "RST" }))
        .collect::<Vec<_>>()
        .join("\n");
    let guard_reads = guards
        .iter()
        .enumerate()
        .map(|(index, (device, _))| format!("LD {device}\nOUT Y{index:o}"))
        .collect::<Vec<_>>()
        .join("\n");
    let observe = if observe.is_empty() {
        String::new()
    } else {
        format!("LD X0\nORI X0\n{observe}\n")
    };
    case(
        &format!("exchange_operand_{name}"),
        &format!(
            "LD X1\n{seed}\n{guard_seed}\n{} M1303\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nLD X0\n{body}\n{guard_reads}\nLD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24\nLD M1303\nOUT Y25\n{observe}END",
            if swap_halves { "SET" } else { "RST" },
        ),
        stages(),
    )
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    // Four spellings x Y/M/S. Each side is writable, but the second side is
    // only one nibble. The first group's complete value remains observed.
    for wide in [false, true] {
        let width = if wide { "D" } else { "" };
        let bits = if wide { 32 } else { 16 };
        for pulse in [false, true] {
            let op = format!("{width}XCH{}", if pulse { "P" } else { "" });
            for (area, base) in [("Y", 128), ("M", 192), ("S", 192)] {
                let target = address(area, base);
                let names = [
                    address(area, base - 1),
                    address(area, base + 4),
                    format!("M{}", 64 + bits),
                ];
                cases.push(scenario(
                    &format!("lifecycle_{op}_{target}"),
                    &format!(
                        "{}\n{}",
                        seed_bits("M", 64, bits, if wide { 0x1234_5675 } else { 0x1235 }),
                        seed_bits(area, base, 4, 10)
                    ),
                    &format!(
                        "{op} K{}M64 K1{target}\n{width}INC K{}M64",
                        bits / 4,
                        bits / 4
                    ),
                    false,
                    &[
                        ("M63", true),
                        (names[2].as_str(), false),
                        (names[0].as_str(), true),
                        (names[1].as_str(), false),
                    ],
                    "",
                ));
            }
        }
    }

    // Native same-address detection ignores Kn width. With M1303 ON it
    // rotates the SECOND snapshot and writes only the SECOND descriptor.
    // With M1303 OFF, both snapshots are exchanged: second write, then first.
    for wide in [false, true] {
        let width = if wide { "D" } else { "" };
        let bits = if wide { 32 } else { 16 };
        let short = if wide { "K4M64" } else { "K1M64" };
        let full = if wide { "K8M64" } else { "K4M64" };
        let tail = format!("M{}", 64 + bits);
        for reverse in [false, true] {
            for swap_halves in [false, true] {
                let (left, right) = if reverse {
                    (full, short)
                } else {
                    (short, full)
                };
                cases.push(scenario(
                    &format!("same_base_different_widths_{width}_{reverse}_{swap_halves}"),
                    &seed_bits("M", 64, bits, if wide { 0x1234_5678 } else { 0x1234 }),
                    &format!("{width}XCH {left} {right}"),
                    swap_halves,
                    &[("M63", true), (tail.as_str(), false)],
                    "",
                ));
            }
        }
    }

    // E/F and canonical D aliases share storage but use distinct M1303
    // resolver keys in these binaries. Probe both operand orders.
    for swap_halves in [false, true] {
        cases.push(scenario(
            &format!("word_index_D_alias_both_orders_{swap_halves}"),
            "MOV H1234 D1182\nMOV H5678 D1184\nMOV H9ABC D1187\nMOV HDEF0 D1189\nMOV K41 D1181\nMOV K42 D1190",
            "XCH E1 D1182\nXCH D1184 E2\nXCH F3 D1187\nXCH D1189 F4",
            swap_halves,
            &[],
            "MOV D1182 D20\nMOV D1184 D21\nMOV D1187 D22\nMOV D1189 D23\nMOV D1181 D24\nMOV D1190 D25",
        ));
        cases.push(scenario(
            &format!("double_index_D_alias_both_orders_{swap_halves}"),
            "DMOV H12345678 D1182\nDMOV H9ABCDEF0 D1184\nMOV K41 D1181\nMOV K42 D1186",
            "DXCH E1 D1182\nDXCH D1184 E2",
            swap_halves,
            &[],
            "DMOV D1182 D20\nDMOV D1184 D22\nMOV D1181 D24\nMOV D1186 D25",
        ));
    }

    // Shifted groups overlap without sharing a base. M1303 is ON, but the
    // normal second-then-first write order applies, in both operand orders.
    for wide in [false, true] {
        let width = if wide { "D" } else { "" };
        let (left, right, seed, tail) = if wide {
            (
                "K8M64",
                "K8M72",
                format!(
                    "{}\n{}",
                    seed_bits("M", 64, 32, 0x1234_5678),
                    seed_bits("M", 96, 8, 0x9a)
                ),
                "M104",
            )
        } else {
            ("K4M64", "K4M68", seed_bits("M", 64, 20, 0xabcde), "M84")
        };
        for reverse in [false, true] {
            let (left, right) = if reverse {
                (right, left)
            } else {
                (left, right)
            };
            cases.push(scenario(
                &format!("shifted_overlap_{width}_{reverse}"),
                &seed,
                &format!("{width}XCH {left} {right}"),
                true,
                &[("M63", true), (tail, false)],
                "",
            ));
        }
    }

    cases.push(scenario(
        "word_distinct_E_F_timer_counter_storage",
        "MOV H9ABC D1182\nMOV HDEF0 D1183\nMOV H1234 T255\nMOV H5678 C199\nMOV K41 T254\nMOV K42 C198",
        "XCH E1 F1\nXCH T255 C199",
        true,
        &[],
        "MOV D1182 D20\nMOV D1183 D21\nMOV T255 D22\nMOV C199 D23\nMOV T254 D24\nMOV C198 D25",
    ));
    cases.push(scenario(
        "double_natural_counter_independent_neighbors",
        "DMOV H12345678 C200\nDMOV HABCDEF01 C255\nDMOV K41 C201\nDMOV K42 C254",
        "DXCH C200 C255",
        true,
        &[],
        "DMOV C200 D20\nDMOV C255 D22\nDMOV C201 D24\nDMOV C254 D26",
    ));
    for wide in [false, true] {
        let width = if wide { "D" } else { "" };
        let bits = if wide { 32 } else { 16 };
        let tail = format!("M{}", 64 + bits);
        cases.push(scenario(
            &format!("same_group_{width}XCHP_half_swap"),
            &seed_bits("M", 64, bits, if wide { 0x1234_5678 } else { 0x1234 }),
            &format!("{width}XCHP K{}M64 K{}M64", bits / 4, bits / 4),
            true,
            &[("M63", true), (tail.as_str(), false)],
            "",
        ));
    }

    for pulse in [false, true] {
        let suffix = if pulse { "P" } else { "" };
        cases.push(scenario(
            &format!("word_index_resolver_collision_and_same_index_{pulse}"),
            "MOV H1234 E1\nMOV H5678 F1\nMOV H9ABC D182\nMOV HDEF0 D183\nMOV H1357 E2\nMOV H2468 F2",
            &format!("XCH{suffix} E1 D182\nXCH{suffix} F1 D183\nXCH{suffix} E2 E2\nXCH{suffix} F2 F2"),
            true, &[], "MOV E1 D20\nMOV F1 D21\nMOV D182 D22\nMOV D183 D23\nMOV E2 D24\nMOV F2 D25",
        ));
        cases.push(scenario(
            &format!("double_index_resolver_collision_and_same_index_{pulse}"),
            "DMOV H12345678 E1\nDMOV H9ABCDEF0 D182\nDMOV H13572468 E2",
            &format!("DXCH{suffix} E1 D182\nDXCH{suffix} E2 E2"),
            true,
            &[],
            "DMOV E1 D20\nDMOV D182 D22\nDMOV E2 D24",
        ));
    }
    let required: BTreeSet<_> = REQUIRED_SPELLINGS.split_whitespace().collect();
    assert_eq!(required.len(), 4, "exchange fixture inventory changed");
    let actual: BTreeSet<_> = cases
        .iter()
        .flat_map(|entry| {
            entry
                .source
                .lines()
                .filter_map(|line| line.split_whitespace().next())
        })
        .collect();
    let missing: Vec<_> = required.difference(&actual).copied().collect();
    assert!(
        missing.is_empty(),
        "exchange fixture spellings missing: {missing:?}"
    );
    assert_eq!(cases.len(), 36, "exchange fixture case count changed");
    assert_eq!(
        cases
            .iter()
            .map(|entry| &entry.name)
            .collect::<BTreeSet<_>>()
            .len(),
        36
    );
    cases
}
