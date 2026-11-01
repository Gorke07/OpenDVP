//! Seeded native regressions for the shared actual-read clock qualification.
//! Native Session supplies the one-time PC0 input clock levels independently;
//! unseeded reads are tested locally, not invented as native hardware errors.
use super::{Case, case};
use std::collections::BTreeSet;

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (vec![("X1", true), ("X0", false), ("X7", false)], 1),
        (vec![("X1", false), ("X0", true)], 2),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}

fn fixture(name: &str, body: &str) -> Case {
    let source = format!(
        "LD X1\nMOV K0 E1\nMOV H6A5A F1\nMOV K0 F2\nMOV H5AA5 E2\n\
         MOV K0 E3\nFMOV H1357 D19 K18\nZRST M80 M82\n\
         SET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\n\
         MOV H1234 D1067\nMOV K77 D1068\n{body}\n\
         LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\n\
         LD M1067\nOUT Y23\nLD M1068\nOUT Y24\n\
         LDI X7\nMOV K1M1011E1 D300\nDMOV D20 D302\nDMOV D22 D304\n\
         MOV D1067 D306\nMOV D1068 D307\nMOV E1 D308\nMOV F1 D309\n\
         MOV F2 D310\nMOV E2 D311\nMOV K1234 D312\nEND"
    );
    case(&format!("indexed_clock_read_{name}"), &source, stages())
}

pub(super) fn cases() -> Vec<Case> {
    let clock = "K1M1011E1";
    let mut cases = Vec::new();
    for wide in [false, true] {
        for pulse in [false, true] {
            for family in ["MOV", "CMP", "MUL", "MUL16", "BCD", "GRY", "MVM", "RAND"] {
                let stem = if family == "MUL16" {
                    if wide { "MUL32".into() } else { "MUL16".into() }
                } else {
                    format!("{}{family}", if wide { "D" } else { "" })
                };
                let op = format!("{stem}{}", if pulse { "P" } else { "" });
                let args = match family {
                    "MOV" | "BCD" | "GRY" => format!("{clock} D20F2"),
                    "CMP" => format!("{clock} K3 M80F2"),
                    "MUL" | "MUL16" => format!("{clock} K3 D20F2"),
                    "MVM" => format!("{clock} {} D20F2", if wide { "HFFFFFFFF" } else { "HFFFF" }),
                    // Equal bounds give an independently determined result even
                    // though the installed and local RNG states are unrelated.
                    "RAND" => format!("{clock} {clock} D20F2"),
                    _ => unreachable!(),
                };
                cases.push(fixture(
                    &format!("first_or_both_sources_{op}"),
                    &format!("LD X0\n{op} {args}"),
                ));
            }
            for family in ["ADD", "CMP", "DIV", "MVM"] {
                let op = format!(
                    "{}{family}{}",
                    if wide { "D" } else { "" },
                    if pulse { "P" } else { "" }
                );
                let args = match family {
                    "ADD" => format!("K7 {clock} D20F2"),
                    "CMP" => format!("K7 {clock} M80F2"),
                    "DIV" => format!("K7 {clock} D20F2"),
                    "MVM" => format!("D32E3 {clock} D20F2"),
                    _ => unreachable!(),
                };
                cases.push(fixture(
                    &format!("second_source_{op}"),
                    &format!("LD X0\n{op} {args}"),
                ));
            }
        }
        for prefix in ["LD", "AND", "OR"] {
            for suffix in ["=", "&", "Z<="] {
                let op = format!("{}{prefix}{suffix}", if wide { "D" } else { "" });
                let args = match suffix {
                    "=" => format!("{clock} K0"),
                    "&" => format!("{clock} K1"),
                    "Z<=" => format!("{clock} K0 K15"),
                    _ => unreachable!(),
                };
                let lead = if prefix == "LD" { "" } else { "LD X0\n" };
                cases.push(fixture(
                    &format!("contact_{op}_invert"),
                    &format!("{lead}{op} {args}\nINV\nOUT Y0"),
                ));
            }
        }
    }
    assert_eq!(cases.len(), 66);
    assert_eq!(
        cases.iter().map(|c| &c.name).collect::<BTreeSet<_>>().len(),
        66
    );
    cases
}
