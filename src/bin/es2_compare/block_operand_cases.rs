//! Grouped block/fill differentials. Boundary probes have a separate prefix
//! because the known native BMOV destination-boundary defect is not parity.
use super::{Case, case};
use std::collections::BTreeSet;

const REQUIRED_SPELLINGS: &str = "BMOV BMOVP FMOV FMOVP DFMOV DFMOVP";

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (vec![("X1", true), ("X0", false)], 1),
        (vec![("X1", false), ("X0", true)], 2),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}

fn address(area: &str, index: u16) -> String {
    if matches!(area, "X" | "Y") {
        format!("{area}{index:o}")
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
                address(area, base + bit),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn seed_elements(area: &str, base: u16, digits: u16, values: &[u32]) -> String {
    values
        .iter()
        .enumerate()
        .map(|(offset, value)| {
            seed_bits(area, base + offset as u16 * digits * 4, digits * 4, *value)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn scenario(name: &str, seed: &str, body: &str, guards: &[(&str, bool)], observe: &str) -> Case {
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
    case(
        &format!("block_operand_{name}"),
        &format!(
            "LD X1\n{seed}\n{guard_seed}\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\nLD X0\n{body}\n{guard_reads}\nLD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24\nLD X0\nORI X0\nMOV D1067 D300\nMOV D1068 D301\n{observe}\nEND"
        ),
        stages(),
    )
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();

    // Six spellings x three writable bit areas x two widths = 36 cases.
    // Each block contains three elements, so wrong group strides affect both
    // interior values and the final guard. Sources change after each visit.
    for operation in ["BMOV", "FMOV", "DFMOV"] {
        let widths = match operation {
            "BMOV" => [1, 3],
            "FMOV" => [2, 4],
            _ => [1, 8],
        };
        for pulse in [false, true] {
            let name = format!("{operation}{}", if pulse { "P" } else { "" });
            for (area, base) in [("Y", 128u16), ("M", 128), ("S", 128)] {
                for digits in widths {
                    let target = address(area, base);
                    let before = address(area, base - 1);
                    let after = address(area, base + digits * 4 * 3);
                    let source_after = format!("M{}", 32 + digits * 4 * 3);
                    let (source_seed, operands, advance) = match operation {
                        "BMOV" => (
                            seed_elements("M", 32, digits, &[0x1234, 0x5678, 0x9abc]),
                            format!("K{digits}M32 K{digits}{target} K3"),
                            format!("INC K{digits}M32"),
                        ),
                        "FMOV" => (
                            "MOV H1234 D1182\nMOV H5678 D1183".to_owned(),
                            format!(
                                "{} K{digits}{target} K3",
                                if digits == 2 { "E1" } else { "F1" }
                            ),
                            format!("INC {}", if digits == 2 { "E1" } else { "F1" }),
                        ),
                        _ => (
                            "DMOV H12345678 D1184".to_owned(),
                            format!("E2 K{digits}{target} K3"),
                            "DINC E2".to_owned(),
                        ),
                    };
                    let mut guards = vec![(before.as_str(), true), (after.as_str(), false)];
                    if operation == "BMOV" {
                        guards.extend([("M31", true), (source_after.as_str(), false)]);
                    }
                    cases.push(scenario(
                        &format!("lifecycle_{name}_K{digits}{target}"),
                        &format!(
                            "{source_seed}\n{}",
                            seed_elements(area, base, digits, &[0x5a5a_5a5a; 3])
                        ),
                        &format!("{name} {operands}\n{advance}"),
                        &guards,
                        "",
                    ));
                }
            }
        }
    }

    // Each readable group area is exercised independently. External X bits
    // are supplied by stages, never written by a PLC instruction.
    const INPUTS: [&str; 24] = [
        "X20", "X21", "X22", "X23", "X24", "X25", "X26", "X27", "X30", "X31", "X32", "X33", "X34",
        "X35", "X36", "X37", "X40", "X41", "X42", "X43", "X44", "X45", "X46", "X47",
    ];
    for area in ["X", "Y", "M", "S"] {
        let source_base = if area == "X" { 16 } else { 64 };
        let source = address(area, source_base);
        let before = address(area, source_base - 1);
        let after = address(area, source_base + 24);
        let source_seed = if area == "X" {
            String::new()
        } else {
            seed_elements(area, source_base, 2, &[0x12, 0x34, 0x56])
        };
        let advance = if area == "X" {
            String::new()
        } else {
            format!("\nINC K2{source}")
        };
        let mut guards = vec![("M127", true), ("M152", false)];
        if area != "X" {
            guards.extend([(before.as_str(), true), (after.as_str(), false)]);
        }
        let mut entry = scenario(
            &format!("BMOV_readable_K2{source}"),
            &format!("{source_seed}\n{}", seed_elements("M", 128, 2, &[0xa5; 3])),
            &format!("BMOV K2{source} K2M128 K3{advance}"),
            &guards,
            "",
        );
        if area == "X" {
            for ((inputs, _), pattern) in
                entry
                    .stages
                    .iter_mut()
                    .zip([0x56_34_12u32, 0xa6_5b_3c, 0xfe_dc_ba, 0x12_34_56])
            {
                inputs.extend(
                    INPUTS
                        .iter()
                        .enumerate()
                        .map(|(bit, name)| (*name, pattern & (1 << bit) != 0)),
                );
            }
        }
        cases.push(entry);
    }

    cases.push(scenario(
        "BMOV_full_groups_to_words",
        &format!(
            "{}\nMOV K41 D3",
            seed_elements("M", 32, 4, &[0x1234, 0x5678, 0x9abc])
        ),
        "BMOV K4M32 D0 K3\nINC K4M32",
        &[("M31", true), ("M80", false)],
        "MOV D3 D30",
    ));
    cases.push(scenario(
        "BMOV_words_to_full_groups",
        &format!(
            "MOV H1234 D0\nMOV H5678 D1\nMOV H9ABC D2\nMOV K41 D3\n{}",
            seed_elements("M", 128, 4, &[0x5a5a; 3])
        ),
        "BMOV D0 K4M128 K3\nINC D0",
        &[("M127", true), ("M176", false)],
        "MOV D3 D30",
    ));

    // BMOV reads each element immediately before its forward write; these
    // shifted overlapping groups expose whole-block snapshot/memmove errors.
    for reverse in [false, true] {
        let (left, right) = if reverse {
            ("K3M36", "K3M32")
        } else {
            ("K3M32", "K3M36")
        };
        cases.push(scenario(
            &format!("BMOV_shifted_overlap_{reverse}"),
            &format!(
                "{}\n{}",
                seed_bits("M", 32, 32, 0x1234_5678),
                seed_bits("M", 64, 8, 0x9a)
            ),
            &format!("BMOV {left} {right} K3"),
            &[("M31", true), ("M72", false)],
            "",
        ));
    }

    // Operand n is sampled once. The first destination write replaces D0
    // (initially 3) with 1; the remaining two original elements still copy.
    for op in ["BMOV", "BMOVP"] {
        cases.push(scenario(
            &format!("{op}_dynamic_count_overwritten_by_first_element"),
            "MOV K3 D0\nMOV K71 D1\nMOV K72 D2\nMOV K41 D3\nMOV K1 D10\nMOV H1234 D11\nMOV H5678 D12",
            &format!("{op} D10 D0 D0\nINC D10"),
            &[],
            "MOV D0 D20\nMOV D1 D21\nMOV D2 D22\nMOV D3 D23",
        ));
    }

    // The source lies inside the first destination element. Its first write
    // destroys that source; the remaining fill elements use its old snapshot.
    cases.push(scenario(
        "FMOV_short_source_inside_first_target_snapshot",
        &format!(
            "{}\n{}",
            seed_elements("M", 128, 4, &[0xa5a5; 3]),
            seed_bits("M", 132, 4, 11)
        ),
        "FMOV K1M132 K4M128 K3",
        &[("M127", true), ("M176", false)],
        "",
    ));
    cases.push(scenario(
        "DFMOV_short_source_inside_first_target_snapshot",
        &format!(
            "{}\n{}",
            seed_elements("M", 128, 8, &[0xa5a5_5a5a; 3]),
            seed_bits("M", 144, 16, 0x5678)
        ),
        "DFMOV K4M144 K8M128 K3",
        &[("M127", true), ("M224", false)],
        "",
    ));
    cases.push(scenario(
        "DFMOVP_natural_counter_stride_and_neighbors",
        "DMOV H12345678 D1182\nMOV K41 C199\nDMOV K42 C203",
        "DFMOVP E1 C200 K3\nDINC E1",
        &[],
        "MOV C199 D20\nDMOV C200 D22\nDMOV C201 D24\nDMOV C202 D26\nDMOV C203 D28",
    ));

    // These three cases are expected parity. Only BMOV DESTINATION crossing
    // the M1535 mapping discontinuity is excluded into separate probes below.
    cases.push(scenario(
        "FMOV_group_crosses_M1535",
        &seed_elements("M", 1532, 3, &[0xabc, 0xdef]),
        "FMOV H1234 K3M1532 K2",
        &[("M1531", true), ("M1556", false)],
        "",
    ));
    cases.push(scenario(
        "DFMOV_group_crosses_M1535",
        &format!(
            "DMOV H12345678 D1182\n{}",
            seed_elements("M", 1532, 8, &[0xa5a5_5a5a; 2])
        ),
        "DFMOV E1 K8M1532 K2\nDINC E1",
        &[("M1531", true), ("M1596", false)],
        "",
    ));
    cases.push(scenario(
        "BMOVP_source_group_crosses_M1535",
        &format!(
            "{}\nMOV K41 D2\nMOV H34 D10",
            seed_elements("M", 1532, 2, &[0x34, 0x12])
        ),
        "BMOVP K2M1532 D0 K2\nINC D10\nFMOV D10 K2M1532 K1",
        &[("M1531", true), ("M1548", false)],
        "MOV D2 D30",
    ));

    // A dynamic count lets the initial source/destination parse remain valid.
    // Source-boundary failure holds the first word, while destination-boundary
    // failure follows its first partial-group write. D0 visits 1,2,1 across
    // active/held/rearmed scans; the later successful operation keeps latches.
    for (digits, base) in [(1, 1020), (2, 1016)] {
        let group = format!("K{digits}S{base}");
        let before = format!("S{}", base - 1);
        cases.push(scenario(
            &format!("BMOV_dynamic_last_group_source_{group}"),
            &format!(
                "{}\nMOV K1 D0\nMOV K41 D19\nMOV K77 D20\nMOV K42 D21",
                seed_bits("S", base, digits * 4, 5)
            ),
            &format!("BMOV {group} D20 D0\nINC {group}\nMOV K2 D0\nLDI X0\nANI X1\nMOV K1 D0"),
            &[(before.as_str(), true)],
            "MOV D19 D30\nMOV D20 D31\nMOV D21 D32",
        ));
        cases.push(scenario(
            &format!("BMOV_dynamic_last_group_destination_{group}"),
            &format!(
                "MOV K1 D0\nMOV K41 D19\nMOV K5 D20\nMOV K42 D21\n{}",
                seed_bits("S", base, digits * 4, 10)
            ),
            &format!("BMOV D20 {group} D0\nINC D20\nMOV K2 D0\nLDI X0\nANI X1\nMOV K1 D0"),
            &[(before.as_str(), true)],
            "MOV D19 D30\nMOV D20 D31\nMOV D21 D32",
        ));
    }

    assert_eq!(cases.len(), 56, "block operand main corpus count changed");
    // Diagnostic-only probes. The main block_operand_ filter excludes both.
    // All destination and adjacent bits remain explicitly tracked so any
    // native boundary corruption appears in the separate probe report.
    for source_group in [false, true] {
        let source = if source_group { "K2M0" } else { "D0" };
        let seed = if source_group {
            seed_elements("M", 0, 2, &[0x34, 0x56])
        } else {
            "MOV H1234 D0\nMOV H5678 D1".to_owned()
        };
        let mut entry = scenario(
            "boundary_probe",
            &format!("{seed}\n{}", seed_elements("M", 1532, 2, &[0xa5; 2])),
            &format!("BMOV {source} K2M1532 K2"),
            &[("M1531", true), ("M1548", false)],
            "",
        );
        entry.name = format!("block_boundary_probe_BMOV_{source}_K2M1532");
        cases.push(entry);
    }

    let required: BTreeSet<_> = REQUIRED_SPELLINGS.split_whitespace().collect();
    assert_eq!(required.len(), 6, "block operand inventory changed");
    let actual: BTreeSet<_> = cases
        .iter()
        .filter(|entry| entry.name.starts_with("block_operand_"))
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
        "block fixture spellings missing: {missing:?}"
    );
    assert_eq!(cases.len(), 58, "block operand/probe count changed");
    assert_eq!(
        cases
            .iter()
            .map(|entry| &entry.name)
            .collect::<BTreeSet<_>>()
            .len(),
        58
    );
    cases
}
