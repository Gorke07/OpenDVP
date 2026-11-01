//! Deterministic RAND operand parity and exact DRAND tail fixtures.
//! No independent random streams are compared for numerical equality.
//! Parent must compile/encode and check every actual tail PC before native use.
use super::{Case as ComparisonCase, Target, case};
use opendvp::bit_index_operand::BitIndexValue;
use opendvp::indexed_operand::AddressedValue;
use std::collections::BTreeSet;

const FORMS: [&str; 4] = ["RAND", "RANDP", "DRAND", "DRANDP"];
const INPUTS: [&str; 32] = [
    "X100", "X101", "X102", "X103", "X104", "X105", "X106", "X107", "X110", "X111", "X112", "X113",
    "X114", "X115", "X116", "X117", "X120", "X121", "X122", "X123", "X124", "X125", "X126", "X127",
    "X130", "X131", "X132", "X133", "X134", "X135", "X136", "X137",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TailClass {
    Inert,
    Np,
    Pn,
    Anb,
    Orb,
    Mps,
    Mrd,
    Mpp,
    Inv,
}

#[derive(Clone, Debug)]
pub(super) struct TailAudit {
    pub case_name: String,
    pub source_line: usize,
    /// Header+7..+12, including both real upper-source WORDs.
    pub words: [u16; 6],
    pub classes: [TailClass; 6],
}

fn classify(word: u16) -> TailClass {
    match word & 0x00f0 {
        0x0080 | 0x0090 | 0x00e0 => TailClass::Inert,
        0x00f0 => match word >> 8 {
            0xf6 => TailClass::Np,
            0xf7 => TailClass::Pn,
            0xf8 => TailClass::Anb,
            0xf9 => TailClass::Orb,
            0xfa => TailClass::Mps,
            0xfb => TailClass::Mrd,
            0xfc => TailClass::Mpp,
            0xfd => TailClass::Inv,
            _ => TailClass::Inert,
        },
        _ => panic!("unqualified DRAND operand-tail WORD {word:04x}"),
    }
}

fn wide_words(raw: &str, write: bool) -> [u16; 4] {
    let words = if raw.starts_with('F') {
        BitIndexValue::parse(raw, true)
            .expect("scoped direct F")
            .native_words()
            .expect("F descriptor")
    } else {
        AddressedValue::parse(raw, true, write)
            .expect("documented wide role")
            .native_words()
            .expect("original descriptor")
    };
    words.try_into().expect("four WORD operand")
}

pub(super) fn tail_inventory(cases: &[ComparisonCase]) -> Vec<TailAudit> {
    let mut result = Vec::new();
    for entry in cases {
        for (line, source) in entry.source.lines().enumerate() {
            let fields: Vec<_> = source.split_whitespace().collect();
            if fields
                .first()
                .is_some_and(|op| matches!(*op, "DRAND" | "DRANDP"))
            {
                assert_eq!(fields.len(), 4, "{source}");
                let upper = wide_words(fields[2], false);
                let target = wide_words(fields[3], true);
                let words = [
                    upper[2], upper[3], target[0], target[1], target[2], target[3],
                ];
                result.push(TailAudit {
                    case_name: entry.name.clone(),
                    source_line: line + 1,
                    words,
                    classes: words.map(classify),
                });
            }
        }
    }
    result
}

fn check_closed_descriptor_examples() {
    // Independent literal vectors from the current two-binary static proof;
    // unlike derived inventory, these do not reuse the generator's formulas.
    for (raw, words, control) in [
        ("D123F0", [0xf6f6, 0x0086, 0x0080, 0x0080], TailClass::Np),
        ("D124F0", [0xf8f6, 0x0086, 0x0080, 0x0080], TailClass::Anb),
        ("D125F0", [0xfaf6, 0x0086, 0x0080, 0x0080], TailClass::Mps),
        ("D126F0", [0xfcf6, 0x0086, 0x0080, 0x0080], TailClass::Mpp),
        ("K1M246F0", [0xf6f4, 0x0882, 0x0080, 0x0080], TailClass::Np),
        ("K1M247F0", [0xf7f4, 0x0882, 0x0080, 0x0080], TailClass::Pn),
        ("K1M248F0", [0xf8f4, 0x0882, 0x0080, 0x0080], TailClass::Anb),
        ("K1M249F0", [0xf9f4, 0x0882, 0x0080, 0x0080], TailClass::Orb),
        ("K1M250F0", [0xfaf4, 0x0882, 0x0080, 0x0080], TailClass::Mps),
        ("K1M251F0", [0xfbf4, 0x0882, 0x0080, 0x0080], TailClass::Mrd),
        ("K1M252F0", [0xfcf4, 0x0882, 0x0080, 0x0080], TailClass::Mpp),
        ("K1M253F0", [0xfdf4, 0x0882, 0x0080, 0x0080], TailClass::Inv),
    ] {
        assert_eq!(wide_words(raw, true), words, "{raw}");
        assert_eq!(
            words.map(classify),
            [
                control,
                TailClass::Inert,
                TailClass::Inert,
                TailClass::Inert
            ]
        );
    }
}

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    // Two OFF visits distinguish DRANDP's first release skip from a later
    // fresh-false body which exposes all six genuine operand-tail WORDs.
    vec![
        (vec![("X1", true), ("X0", false), ("X7", false)], 1),
        (vec![("X1", false), ("X0", true)], 2),
        (vec![("X0", false)], 2),
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
fn index_base(index: u16) -> u16 {
    if index == 0 { 1028 } else { 1180 + index * 2 }
}
fn seed_words(area: &str, base: u16, count: u16) -> String {
    (0..count)
        .map(|i| {
            format!(
                "MOV H{:04X} {area}{}",
                0xa55au16.wrapping_add(i.wrapping_mul(0x1111)),
                base + i
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn read_words(area: &str, base: u16, count: u16, output: u16) -> String {
    (0..count)
        .map(|i| format!("MOV {area}{} D{}", base + i, output + i))
        .collect::<Vec<_>>()
        .join("\n")
}
fn seed_bits(area: &str, base: u16, count: u16) -> String {
    (0..count)
        .map(|i| {
            format!(
                "{} {}",
                if (0xa55a33ccu32 >> (i % 32)) & 1 == 0 {
                    "RST"
                } else {
                    "SET"
                },
                address(area, base + i)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn read_bits(area: &str, base: u16, count: u16, output: u16) -> String {
    assert_eq!(count % 4, 0);
    (0..count.div_ceil(16))
        .map(|i| {
            format!(
                "MOV K{}{} D{}",
                (count - i * 16).min(16) / 4,
                address(area, base + i * 16),
                output + i
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn scenario(name: &str, seed: &str, body: &str, observe: &str) -> ComparisonCase {
    scenario_with_tail(name, seed, body, observe, &[])
}
fn scenario_with_tail(
    name: &str,
    seed: &str,
    body: &str,
    observe: &str,
    expected: &[TailClass],
) -> ComparisonCase {
    let gate = if body.starts_with("LD") || body.starts_with("STL") {
        ""
    } else {
        "LD X0"
    };
    let source = ["LD X1\nMOV H55AA E0\nMOV H1234 F0\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068",seed,gate,body,
        "LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24",
        "LDI X7\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305\nMOV K1234 D306",observe,"END"].into_iter().filter(|s|!s.is_empty()).collect::<Vec<_>>().join("\n");
    assert!(
        source.lines().count() <= 512,
        "oversized RAND fixture {name}"
    );
    assert!(
        source.lines().count() * 13 <= 0x1ef0,
        "installed budget {name}"
    );
    assert!(
        source.lines().any(|line| {
            let f: Vec<_> = line.split_whitespace().collect();
            f.first().is_some_and(|op| FORMS.contains(op))
                && f[1..].iter().any(|arg| {
                    let b = arg.as_bytes();
                    !arg.starts_with(['H', 'E', 'F'])
                        && b.len() >= 4
                        && matches!(b[b.len() - 2], b'E' | b'F')
                        && b[b.len() - 1].is_ascii_digit()
                })
        }),
        "missing addressed RAND {name}"
    );
    let entry = case(&format!("indexed_random_{name}"), &source, stages());
    let inventory = tail_inventory(std::slice::from_ref(&entry));
    let mut actual = Vec::new();
    for audit in inventory {
        for (slot, class) in audit.classes.into_iter().enumerate() {
            if class != TailClass::Inert {
                assert_eq!(
                    slot, 2,
                    "unexpected non-target tail at {}:{}: {:04x?}",
                    audit.case_name, audit.source_line, audit.words
                );
                actual.push(class);
            }
        }
    }
    assert_eq!(
        actual.as_slice(),
        expected,
        "unqualified tail controls in {name}"
    );
    entry
}
fn check_cases(cases: &[ComparisonCase], count: usize) {
    assert_eq!(cases.len(), count);
    assert_eq!(
        cases
            .iter()
            .map(|entry| &entry.name)
            .collect::<BTreeSet<_>>()
            .len(),
        count
    );
    for op in FORMS {
        assert!(
            cases.iter().any(|entry| entry
                .source
                .lines()
                .any(|line| line.split_whitespace().next() == Some(op))),
            "missing {op}"
        );
    }
}

pub(super) fn common_cases() -> Vec<ComparisonCase> {
    check_closed_descriptor_examples();
    let mut cases = Vec::new();
    for wide in [false, true] {
        for bank in ['E', 'F'] {
            for index in 0..8u16 {
                let modifier = format!("{bank}{index}");
                let companion = index_base(index) + u16::from(bank == 'E');
                let op = format!(
                    "{}RAND{}",
                    if wide { "D" } else { "" },
                    if index % 2 == 0 { "" } else { "P" }
                );
                let digits = if wide { index + 1 } else { index % 4 + 1 };
                let offset = if index % 2 == 0 { -1 } else { 1 };
                let bounds = (0..12u16)
                    .map(|i| format!("MOV K7 D{}\nMOV K7 D{}", 28 + i, 60 + i))
                    .collect::<Vec<_>>()
                    .join("\n");
                let seed = [
                    bounds,
                    seed_words("D", 156, 12),
                    seed_words("T", 60, 12),
                    seed_words("C", 60, 12),
                    seed_bits("Y", 120, 48),
                    seed_bits("M", 120, 48),
                    seed_bits("S", 120, 48),
                    format!("MOV H6A5A D{companion}\nMOV K{offset} {modifier}\nMOV K0 D90"),
                ]
                .join("\n");
                let mut body = Vec::new();
                for target in [
                    format!("D160{modifier}"),
                    format!("T64{modifier}"),
                    format!("C64{modifier}"),
                    format!("K{digits}Y200{modifier}"),
                    format!("K{digits}M128{modifier}"),
                    format!("K{digits}S128{modifier}"),
                ] {
                    body.push(format!("{op} D32{modifier} D64{modifier} {target}"));
                }
                body.push(format!("INC {modifier}\nINC D90\nOUT Y10"));
                let observe = format!(
                    "{}\n{}\n{}\n{}\n{}\n{}\nMOV {modifier} D280\nMOV D{companion} D281\nMOV D90 D282",
                    read_words("D", 156, 12, 200),
                    read_words("T", 60, 12, 220),
                    read_words("C", 60, 12, 240),
                    read_bits("Y", 120, 48, 260),
                    read_bits("M", 120, 48, 264),
                    read_bits("S", 120, 48, 268)
                );
                cases.push(scenario(
                    &format!("all_banks_equal_six_areas_{op}_{modifier}_K{digits}"),
                    &seed,
                    &body.join("\n"),
                    &observe,
                ));
            }
        }
    }
    assert_eq!(cases.len(), 32);
    append_width_cases(&mut cases);
    append_numeric_and_errors(&mut cases);
    append_direct_and_self_aliases(&mut cases);
    append_safe_flow(&mut cases);
    append_collision_cases(&mut cases);
    check_cases(&cases, 110);
    cases
}

fn append_width_cases(cases: &mut Vec<ComparisonCase>) {
    for op in FORMS {
        for area in ["Y", "M", "S"] {
            let wide = op.starts_with('D');
            let max = if wide { 8u16 } else { 4 };
            let seed = format!(
                "{}\n{}\nDMOV H155533CC K8Y100\nDMOV H155533CC K8M64\nDMOV H155533CC K8S64\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K1 F2",
                seed_bits(area, 120, 48),
                seed_words("D", 80, max * 6)
            );
            let mut body = Vec::new();
            for digits in 1..=max {
                let reset = format!(
                    "LDI X7\nDMOV H89ABCDEF K8{}\nSET {}\nSET {}",
                    address(area, 128),
                    address(area, 127),
                    address(area, 128 + digits * 4)
                );
                body.push(format!(
                    "{reset}\nLD X0\n{op} K{digits}X101E1 K{digits}M65E1 K{max}{}F2\nLDI X7\n{}",
                    address(area, 127),
                    read_bits(area, 120, 48, 80 + (digits - 1) * 6)
                ));
                body.push(format!(
                    "{reset}\nLD X0\n{op} K{max}Y101E1 K{max}S65E1 K{digits}{}F2\nLDI X7\n{}",
                    address(area, 127),
                    read_bits(area, 120, 48, 83 + (digits - 1) * 6)
                ));
            }
            let observe = format!(
                "{}\n{}\nDMOV K8X100 D280\nMOV E1 D282\nMOV F2 D283",
                read_bits(area, 120, 48, 200),
                read_words("D", 80, max * 6, 220)
            );
            let mut entry = scenario(
                &format!("all_packed_widths_equal_{op}_{area}"),
                &seed,
                &body.join("\n"),
                &observe,
            );
            entry.stages[0].0.extend(
                INPUTS
                    .into_iter()
                    .enumerate()
                    .map(|(bit, input)| (input, (0x155533ccu32 >> bit) & 1 != 0)),
            );
            cases.push(entry);
        }
    }
    assert_eq!(cases.len(), 44);
}

fn append_numeric_and_errors(cases: &mut Vec<ComparisonCase>) {
    for op in FORMS {
        let wide = op.starts_with('D');
        let offset = if wide { -1 } else { 0 };
        let seed = format!(
            "{}\nMOV K7 T254\nMOV K7 T255\nMOV K9 C198\nMOV K9 C199\nDMOV H01234567 C200\nDMOV H12345678 C201\nDMOV H11223344 C254\nDMOV H01234567 C255\nMOV K{offset} E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K{offset} F2\nMOV K1 E3\nMOV H1234 F3",
            seed_words("D", 0, 4)
        );
        let body = if wide {
            format!(
                "{op} T255E1 T255E1 D1F2\n{op} C200E1 C200E1 T255F2\n{op} C199E1 C199E1 C200F2\nDMOV C200 D80\n{op} C199E3 C199E3 C254E3"
            )
        } else {
            format!(
                "{op} T255E1 T255E1 D0F2\n{op} C199E1 C199E1 T255F2\n{op} C198E1 C198E1 C199F2\nDMOV C200 D80"
            )
        };
        cases.push(scenario(&format!("numeric_counter_timer_endpoints_equal_{op}"),&seed,&body,"MOV D0 D220\nMOV D1 D221\nMOV T254 D222\nMOV T255 D223\nMOV C198 D224\nMOV C199 D225\nDMOV C200 D226\nDMOV C201 D228\nDMOV C254 D230\nDMOV C255 D232\nDMOV D80 D234"));
        for (label, lower, upper) in [
            ("negative_lower", -1, 7),
            ("negative_upper", 7, -1),
            ("reversed", 9, 7),
        ] {
            let mov = if wide { "DMOV" } else { "MOV" };
            let sticky = if label == "negative_upper" {
                "SET M1067\nSET M1068\nMOV H0123 D1067\nMOV K77 D1068"
            } else {
                ""
            };
            let seed = format!(
                "{}\n{mov} K{lower} D32\n{mov} K{upper} D36\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 E3\nMOV H1234 F3\nMOV K0 D90\n{sticky}",
                seed_words("D", 0, 4)
            );
            let body = format!(
                "LD X0\nANI X2\nMOV K-1 E3\nLD X2\n{mov} K7 D32\n{mov} K7 D36\nMOV K0 E3\nLD X0\n{op} D32E1 D36F2 D0E3\nINC D90\nOUT Y10"
            );
            let mut entry = scenario(
                &format!("domain_{label}_skip_bad_target_then_recover_{op}"),
                &seed,
                &body,
                "MOV D0 D220\nMOV D1 D221\nMOV D2 D222\nMOV D3 D223\nDMOV D32 D230\nDMOV D36 D234\nMOV E3 D250\nMOV D90 D251",
            );
            entry.stages = vec![
                (
                    vec![("X1", true), ("X0", false), ("X2", false), ("X7", false)],
                    1,
                ),
                (vec![("X1", false), ("X0", true)], 1),
                (vec![("X2", true)], 1),
                (vec![("X0", false)], 2),
                (vec![("X0", true)], 1),
            ];
            cases.push(entry);
        }
    }
    assert_eq!(cases.len(), 60);
}

fn append_direct_and_self_aliases(cases: &mut Vec<ComparisonCase>) {
    for op in ["DRAND", "DRANDP"] {
        for role in ["lower", "upper", "target"] {
            let seed = "MOV H55AA E1\nDMOV H00010007 D1183\nMOV H6A5A F2\nMOV K0 E3\nMOV K0 F3\nDMOV H00010007 D32\nDMOV H00010007 D36\nMOV H55AA D159\nDMOV H12345678 D160\nMOV H6A5A D162";
            let args = match role {
                "lower" => "F1 D36E3 D160F3",
                "upper" => "D32E3 F1 D160F3",
                _ => "D32E3 D36F3 F1",
            };
            cases.push(scenario(&format!("direct_wide_F_{role}_equal_{op}"),seed,&format!("{op} {args}"),"MOV E1 D220\nMOV F1 D221\nMOV E2 D222\nMOV F2 D223\nMOV D159 D230\nDMOV D160 D232\nMOV D162 D234"));
        }
    }
    for op in FORMS {
        let (modifier, alias) = if op.ends_with('P') {
            ("F1", 1183)
        } else {
            ("E1", 1182)
        };
        let seed =
            format!("MOV H55AA E1\nMOV H1234 F1\nMOV H5678 E2\nMOV H6A5A F2\nMOV K1 {modifier}");
        let body = format!(
            "LDI X7\nMOV K1 {modifier}\nLD X0\n{op} K7 K7 D{}{modifier}\nOUT Y10",
            alias - 1
        );
        cases.push(scenario(
            &format!("cached_own_index_equal_{op}_{modifier}"),
            &seed,
            &body,
            "MOV E1 D220\nMOV F1 D221\nMOV E2 D222\nMOV F2 D223",
        ));
    }
    assert_eq!(cases.len(), 70);
}

fn append_safe_flow(cases: &mut Vec<ComparisonCase>) {
    for op in FORMS {
        let seed = format!(
            "{}\n{}\nMOV K0 E1\nMOV H6A5A F1\nMOV K0 E3\nMOV H1234 F3\nMOV K0 D30\nMOV K0 D90",
            seed_words("D", 159, 5),
            seed_words("D", 80, 6)
        );
        let body = format!(
            "LDI X7\nMOV K0 D30\nFOR K3\nLDI X7\nMUL16 D30 K2 E3\nLD= D30 K1\nDMOV H55555555 D160\nLD= D30 K0\nOR= D30 K2\nAND X0\n{op} K7 K7 D160E1\nINC D90\nLDI X7\nDMOV D160 D80E3\nINC D30\nNEXT"
        );
        cases.push(scenario(
            &format!("safe_for_same_pc_true_false_true_{op}"),
            &seed,
            &body,
            &format!(
                "{}\n{}\nMOV D90 D250",
                read_words("D", 159, 5, 220),
                read_words("D", 80, 6, 230)
            ),
        ));
        let body = format!(
            "LD X2\nDMOV H55555555 D160\nLD X2\nCJ P10\nLD X0\n{op} K7 K7 D160E1\nINC D90\nP10\nLDI X7\nINC D30"
        );
        let mut entry = scenario(
            &format!("safe_cj_skipped_false_preserves_history_{op}"),
            &seed,
            &body,
            &format!(
                "{}\nMOV D30 D240\nMOV D90 D250",
                read_words("D", 159, 5, 220)
            ),
        );
        entry.stages = vec![
            (
                vec![("X1", true), ("X0", false), ("X2", false), ("X7", false)],
                1,
            ),
            (vec![("X1", false), ("X0", true)], 1),
            (vec![("X0", false), ("X2", true)], 1),
            (vec![("X0", true), ("X2", false)], 1),
            (vec![("X0", false)], 2),
            (vec![("X0", true)], 1),
        ];
        cases.push(entry);
    }
    for op in ["DRAND", "DRANDP"] {
        let seed = format!(
            "{}\n{}\nDMOV H11223344 C254\nDMOV H12345678 C255\nMOV K0 F0\nMOV K0 E7\nMOV K0 F7\nMOV K0 D90",
            seed_words("D", 121, 10),
            seed_bits("M", 240, 32)
        );
        let targets = [
            "D122F0", "D127F0", "D123E7", "D126E7", "C255F7", "K1M245F0", "K1M254F7",
        ];
        let body = targets
            .into_iter()
            .map(|target| format!("LD X0\n{op} K7 K7 {target}\nINC D90"))
            .collect::<Vec<_>>()
            .join("\n");
        cases.push(scenario(
            &format!("closed_inert_tail_neighbors_E_F_natural_C_{op}"),
            &seed,
            &body,
            &format!(
                "{}\n{}\nDMOV C254 D240\nDMOV C255 D242\nMOV D90 D250",
                read_words("D", 121, 10, 220),
                read_bits("M", 240, 32, 234)
            ),
        ));
    }
    assert_eq!(cases.len(), 80);
}

// Sixteen completed LD/OUT networks replace the entire logic WORD, not only
// bit0. No private native global is misrepresented as a public PLC D register.
// The last input varies; after MPS/INV/MPS/INV the low bits are x,!x,x.
fn seed_logic(complement: bool) -> String {
    (0..16u16)
        .map(|slot| {
            let load = if slot == 15 {
                if complement { "LDI X0" } else { "LD X0" }
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
fn collision_body(op: &str, target: &str, complement: bool, output: u16, master: &str) -> String {
    format!(
        "LD {master}\nMC N0\n{}\nMPS\nINV\nMPS\nINV\n{op} K7 K7 {target}\nMCR N0\nOUT Y{output:o}\nINC D90\nMRD\nOUT Y{:o}\nMPP\nOUT Y{:o}\nMPP\nOUT Y{:o}\nLDI X7\nINC D91",
        seed_logic(complement),
        output + 1,
        output + 2,
        output + 3
    )
}
fn collision_stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (
            vec![
                ("X1", true),
                ("X0", false),
                ("X2", false),
                ("X3", false),
                ("X7", false),
            ],
            1,
        ),
        (vec![("X1", false), ("X0", true)], 1),
        (vec![("X3", true), ("X2", true)], 2),
        (vec![("X3", false)], 2),
        (vec![("X0", false)], 1),
        (vec![("X0", true), ("X3", true)], 1),
    ]
}
fn append_collision_cases(cases: &mut Vec<ComparisonCase>) {
    let targets = [
        ("D123F0", TailClass::Np),
        ("D124F0", TailClass::Anb),
        ("D125F0", TailClass::Mps),
        ("D126F0", TailClass::Mpp),
        ("K1M246F0", TailClass::Np),
        ("K1M247F0", TailClass::Pn),
        ("K1M248F0", TailClass::Anb),
        ("K1M249F0", TailClass::Orb),
        ("K1M250F0", TailClass::Mps),
        ("K1M251F0", TailClass::Mrd),
        ("K1M252F0", TailClass::Mpp),
        ("K1M253F0", TailClass::Inv),
    ];
    let seed = format!(
        "{}\n{}\nMOV K0 F0\nMOV K0 D90\nMOV K0 D91",
        seed_words("D", 120, 12),
        seed_bits("M", 240, 32)
    );
    let observe = format!(
        "{}\n{}\nMOV F0 D260\nMOV D90 D261\nMOV D91 D262",
        read_words("D", 120, 12, 220),
        read_bits("M", 240, 32, 240)
    );
    for op in ["DRAND", "DRANDP"] {
        for (target, control) in targets {
            // The original payload controls dispatch. A live +1 modifier must not
            // change NP into PN or replace any other classified opcode.
            let body = format!(
                "LDI X7\nMOV K0 F0\nLD X2\nMOV K1 F0\n{}",
                collision_body(op, target, false, 0, "X3")
            );
            let mut entry = scenario_with_tail(
                &format!("tail_{control:?}_{target}_{op}_full_word_master_and_P"),
                &seed,
                &body,
                &observe,
                &[control],
            );
            entry.stages = collision_stages();
            cases.push(entry);
        }
    }
    assert_eq!(cases.len(), 104);
    for op in ["DRAND", "DRANDP"] {
        for (target, control) in [("K1M246F0", TailClass::Np), ("K1M247F0", TailClass::Pn)] {
            let body = format!(
                "LD X2\nCJ P10\n{}\nP10\nLDI X7\nINC D92",
                collision_body(op, target, false, 0, "X7")
            );
            let mut entry = scenario_with_tail(
                &format!("tail_{control:?}_cj_skipped_sample_history_{op}"),
                &format!("{seed}\nMOV K0 D92"),
                &body,
                &format!("{observe}\nMOV D92 D263"),
                &[control],
            );
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
            cases.push(entry);
        }
    }
    for op in ["DRAND", "DRANDP"] {
        let body = format!(
            "{}\n{}",
            collision_body(op, "D123F0", false, 0, "X7"),
            collision_body(op, "K1M246F0", true, 4, "X7")
        );
        let mut entry = scenario_with_tail(
            &format!("tail_NP_two_distinct_PCs_complement_inputs_{op}"),
            &seed,
            &body,
            &observe,
            &[TailClass::Np, TailClass::Np],
        );
        entry.stages = stages();
        cases.push(entry);
    }
}

pub(super) fn model_cases(target: Target) -> Vec<ComparisonCase> {
    let last: u16 = match target {
        Target::Es2 => 9999,
        Target::Ss2 => 4999,
    };
    let mut cases = Vec::new();
    for op in FORMS {
        let wide = op.starts_with('D');
        let offset = if wide { -1 } else { 0 };
        let digits = if wide { 8u16 } else { 4 };
        let seed = format!(
            "{}\n{}\nMOV K{offset} E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2",
            seed_words("D", last - 2, 3),
            seed_words("D", 159, 4)
        );
        let body = format!("{op} K7 K7 D{last}E1\n{op} D{last}E1 D{last}E1 D160F2\nOUT Y10");
        cases.push(scenario(
            &format!(
                "{}_last_numeric_source_and_target_equal_{op}",
                target.name()
            ),
            &seed,
            &body,
            &format!(
                "{}\n{}",
                read_words("D", last - 2, 3, 220),
                read_words("D", 159, 4, 230)
            ),
        ));
        let seed = format!(
            "{}\n{}\n{}\nMOV H7BBB E2\nMOV K-{} F2",
            seed_bits("Y", 220, 36),
            seed_bits("M", 4060, 36),
            seed_bits("S", 988, 36),
            digits * 4 - 1
        );
        let body = [("Y", 255), ("M", 4095), ("S", 1023)]
            .into_iter()
            .map(|(area, end)| format!("LD X0\n{op} K7 H7 K{digits}{}F2", address(area, end)))
            .collect::<Vec<_>>()
            .join("\n");
        cases.push(scenario(
            &format!("{}_last_packed_three_areas_equal_{op}", target.name()),
            &seed,
            &body,
            &format!(
                "{}\n{}\n{}",
                read_bits("Y", 220, 36, 220),
                read_bits("M", 4060, 36, 224),
                read_bits("S", 988, 36, 228)
            ),
        ));
    }
    check_cases(&cases, 8);
    cases
}

// FUTURE INVARIANTS (not cases in the value-equality corpus): run independent
// local/native streams for [0,1], [3,19], [0,32767], [0,2147483647], and moving
// Kn/D targets. Assert inclusive bounds, exact Kn truncation, guard retention,
// unchanged flags, successful next-PC and P rearm separately on each side.
// Keep results and RNG state independent. Prefer fixed indices first; a later
// moving-target harness must observe actual resolved addresses per visit and
// use the classified tail's power/marker oracle, not assume power equals X0.
