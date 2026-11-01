//! Grouped/index-source comparison differentials. Native execution, not this
//! fixture generator, establishes whether the selected simulator agrees.
use super::{Case, case};
use std::collections::BTreeSet;

// Explicit inventory, independent of the loops that generate source text.
const REQUIRED_SPELLINGS: &str = "
CMP CMPP ZCP ZCPP DCMP DCMPP DZCP DZCPP
LD= LD> LD< LD<> LD<= LD>= AND= AND> AND< AND<> AND<= AND>= OR= OR> OR< OR<> OR<= OR>=
DLD= DLD> DLD< DLD<> DLD<= DLD>= DAND= DAND> DAND< DAND<> DAND<= DAND>= DOR= DOR> DOR< DOR<> DOR<= DOR>=
LD& LD| LD^ AND& AND| AND^ OR& OR| OR^
DLD& DLD| DLD^ DAND& DAND| DAND^ DOR& DOR| DOR^
LDZ= LDZ> LDZ< LDZ<> LDZ<= LDZ>= ANDZ= ANDZ> ANDZ< ANDZ<> ANDZ<= ANDZ>= ORZ= ORZ> ORZ< ORZ<> ORZ<= ORZ>=
DLDZ= DLDZ> DLDZ< DLDZ<> DLDZ<= DLDZ>= DANDZ= DANDZ> DANDZ< DANDZ<> DANDZ<= DANDZ>= DORZ= DORZ> DORZ< DORZ<> DORZ<= DORZ>=
";

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (vec![("X1", true), ("X0", false)], 1),
        (vec![("X1", false), ("X0", true)], 2),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}

fn seed_bits(base: u16, bits: u16, value: u32) -> String {
    (0..bits)
        .map(|bit| {
            format!(
                "{} M{}",
                if value & (1u32 << bit) != 0 {
                    "SET"
                } else {
                    "RST"
                },
                base + bit,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn scenario(name: &str, seed: &str, advance: &str, body: &str) -> Case {
    let advance = if advance.is_empty() {
        String::new()
    } else {
        format!("LD X0\n{advance}\n")
    };
    // Source-changing ADD is permitted here, but its ALU flags are deliberately
    // replaced before the comparison. The observed pattern must remain 101.
    case(
        &format!("comparison_operand_{name}"),
        &format!(
            "LD X1\n{seed}\n{advance}LDI X7\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\n{body}\nLD M1020\nOUT Y30\nLD M1021\nOUT Y31\nLD M1022\nOUT Y32\nLD M1067\nOUT Y33\nLD M1068\nOUT Y34\nEND"
        ),
        stages(),
    )
}

fn contact(prefix: &str, name: &str, args: &str, output: usize) -> String {
    let gate = match prefix {
        "LD" => "",
        "AND" => "LD X0\n",
        // When X0 is ON, OR's left side is false, so all active changing
        // source values reach the actual comparison. OFF tests its shortcut.
        "OR" => "LDI X0\n",
        _ => unreachable!(),
    };
    format!("{gate}{name} {args}\nOUT Y{output:o}")
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let relations = ["=", ">", "<", "<>", "<=", ">="];

    // Eight action spellings: the changing source crosses less/equal/greater
    // (or below/inside/above). Pulse output differs from normal output during
    // the second consecutive ON scan; all outputs must hold while disabled.
    for wide in [false, true] {
        let width = if wide { "D" } else { "" };
        let bits = if wide { 32 } else { 16 };
        let group = if wide { "K8M0" } else { "K4M0" };
        for op in ["CMP", "ZCP"] {
            for pulse in [false, true] {
                let name = format!("{width}{op}{}", if pulse { "P" } else { "" });
                let (initial, indices, operands, advance) = if op == "CMP" {
                    (
                        0xffff_fffe,
                        "MOV K0 D1182\nMOV K0 D1183".to_owned(),
                        format!("{group} {} M80", if wide { "E1" } else { "F1" }),
                        format!("{width}INC {group}"),
                    )
                } else {
                    (
                        0xffff_fffc,
                        if wide {
                            "DMOV K-1 D1182\nDMOV K1 D1184".to_owned()
                        } else {
                            "MOV K-1 D1182\nMOV K1 D1183".to_owned()
                        },
                        format!("E1 {} {group} M80", if wide { "E2" } else { "F1" }),
                        format!("{width}ADD {group} K2 {group}"),
                    )
                };
                let seed = format!(
                    "{}\n{indices}\nSET M79\nSET M80\nSET M81\nRST M82\nSET M83\nSET M84\nRST M85\nSET M86\nSET M87",
                    seed_bits(0, bits, initial),
                );
                let body = format!(
                    "LD X0\n{name} {operands}\nLD M80\nOUT Y0\nLD M81\nOUT Y1\nLD M82\nOUT Y2\nLD M79\nOUT Y3\nLD M83\nOUT Y4\nLD M84\nOUT Y5\nLD M85\nOUT Y6\nLD M86\nOUT Y7\nLD M87\nOUT Y10"
                );
                cases.push(scenario(
                    &format!("{name}_lifecycle"),
                    &seed,
                    &advance,
                    &body,
                ));
            }
        }
    }

    // Thirty-six signed contacts, six independently visible results per case.
    // Every normal/D prefix and LD/AND/OR prefix receives all six relations.
    for wide in [false, true] {
        let width = if wide { "D" } else { "" };
        let bits = if wide { 32 } else { 16 };
        let group = if wide { "K8M0" } else { "K4M0" };
        let seed = format!(
            "{}\nMOV K0 D1182\nMOV K0 D1183",
            seed_bits(0, bits, 0xffff_fffe)
        );
        let advance = format!("{width}INC {group}");
        for prefix in ["LD", "AND", "OR"] {
            let body = relations
                .iter()
                .enumerate()
                .map(|(index, relation)| {
                    let right = if !wide && index % 2 == 1 { "F1" } else { "E1" };
                    contact(
                        prefix,
                        &format!("{width}{prefix}{relation}"),
                        &format!("{group} {right}"),
                        index,
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            cases.push(scenario(
                &format!("signed_{width}{prefix}"),
                &seed,
                &advance,
                &body,
            ));
        }
    }

    // Eighteen bitwise contacts. The full group and a separate short group
    // advance through 0,1,2,3; masks include direct E/F aliases in both roles.
    for wide in [false, true] {
        let width = if wide { "D" } else { "" };
        let bits = if wide { 32 } else { 16 };
        let group = if wide { "K8M0" } else { "K4M0" };
        let seed = format!(
            "{}\n{}\nMOV K1 D1182\nMOV K0 D1183\nMOV K2 D1184\nMOV K0 D1185\nMOV K0 D1186\nMOV K0 D1187",
            seed_bits(0, bits, 0),
            seed_bits(32, 4, 0),
        );
        let advance = format!("{width}INC {group}\n{width}INC K1M32");
        for prefix in ["LD", "AND", "OR"] {
            let body = [
                ("&", format!("{group} E1")),
                ("|", format!("K1M32 {}", if wide { "E3" } else { "F1" })),
                ("^", format!("E2 {group}")),
            ]
            .into_iter()
            .enumerate()
            .map(|(index, (operation, args))| {
                contact(prefix, &format!("{width}{prefix}{operation}"), &args, index)
            })
            .collect::<Vec<_>>()
            .join("\n");
            cases.push(scenario(
                &format!("logic_{width}{prefix}"),
                &seed,
                &advance,
                &body,
            ));
        }
    }

    // Thirty-six absolute-difference contacts. E/F are intentionally absent:
    // that operand matrix allows groups but forbids direct index registers.
    for wide in [false, true] {
        let width = if wide { "D" } else { "" };
        let bits = if wide { 32 } else { 16 };
        let group = if wide { "K8M0" } else { "K4M0" };
        let seed = format!(
            "{}\n{}\n{}",
            seed_bits(0, bits, 0xffff_fffe),
            seed_bits(40, 4, 0),
            seed_bits(48, 4, 1),
        );
        let advance = format!("{width}INC {group}");
        for prefix in ["LD", "AND", "OR"] {
            let body = relations
                .iter()
                .enumerate()
                .map(|(index, relation)| {
                    contact(
                        prefix,
                        &format!("{width}{prefix}Z{relation}"),
                        &format!("{group} K1M40 K1M48"),
                        index,
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            cases.push(scenario(
                &format!("difference_{width}{prefix}"),
                &seed,
                &advance,
                &body,
            ));
        }
    }

    cases.push(scenario(
        "signed_and_short_group_widths",
        &format!("{}\n{}", seed_bits(0, 16, 0xffff), seed_bits(32, 4, 15)),
        "",
        "LD< K4M0 K0\nOUT Y0\nDLD> K4M0 K0\nOUT Y1\nLD> K1M32 K0\nOUT Y2\nDLD> K1M32 K0\nOUT Y3\nDLD& K4M0 HFFFF0000\nOUT Y4\nLD& K4M0 H8000\nOUT Y5",
    ));
    cases.push(scenario(
        "difference_minimum_and_wrapped_subtraction",
        &format!("{}\n{}", seed_bits(0, 16, 0x8000), seed_bits(32, 32, 0x8000_0000)),
        "",
        "LDZ= K4M0 K0 K0\nOUT Y0\nDLDZ= K8M32 K0 K0\nOUT Y1\nLDZ= K0 K0 K4M0\nOUT Y2\nDLDZ= K0 K0 K8M32\nOUT Y3\nLDZ= H7FFF K4M0 K1\nOUT Y4\nDLDZ= H7FFFFFFF K8M32 K1\nOUT Y5",
    ));
    cases.push(scenario(
        "action_results_overlap_source_groups",
        &format!(
            "{}\n{}\n{}\n{}",
            seed_bits(0, 4, 1), seed_bits(32, 32, 1), seed_bits(72, 4, 1), seed_bits(80, 4, 1),
        ),
        "",
        "LD X0\nCMP K1M0 K1 M0\nDCMP K8M32 K1 M32\nZCP K0 K1 K1M72 M72\nDZCP K0 K1 K1M80 M80\nLD M0\nOUT Y0\nLD M1\nOUT Y1\nLD M2\nOUT Y2\nLD M32\nOUT Y3\nLD M33\nOUT Y4\nLD M34\nOUT Y5\nLD M72\nOUT Y6\nLD M73\nOUT Y7\nLD M74\nOUT Y10\nLD M80\nOUT Y11\nLD M81\nOUT Y12\nLD M82\nOUT Y13",
    ));
    cases.push(scenario(
        "natural_counter_and_index_aliases",
        &format!(
            "DMOV H80000000 C200\nDMOV H80000000 D1182\nMOV HFFFF D1184\nMOV K0 D1185\n{}\n{}",
            seed_bits(0, 32, 0x8000_0000), seed_bits(32, 16, 0xffff),
        ),
        "",
        "DLD= C200 E1\nOUT Y0\nDLD< C200 K4M32\nOUT Y1\nLD= K4M32 E2\nOUT Y2\nLD= F2 K0\nOUT Y3\nDLD= K8M0 C200\nOUT Y4\nDLD> K4M32 E2\nOUT Y5",
    ));
    let required: BTreeSet<_> = REQUIRED_SPELLINGS.split_whitespace().collect();
    assert_eq!(required.len(), 98, "comparison fixture inventory changed");
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
        "comparison fixture spellings missing: {missing:?}"
    );
    assert_eq!(cases.len(), 30, "comparison fixture case count changed");
    assert_eq!(
        cases
            .iter()
            .map(|entry| &entry.name)
            .collect::<BTreeSet<_>>()
            .len(),
        30
    );
    cases
}
