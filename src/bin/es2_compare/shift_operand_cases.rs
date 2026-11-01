//! Packed operands and direct index aliases for rotations, word shifts,
//! queues and digit moves. Fixtures become evidence only after native runs.
use super::{Case, case};
use std::collections::BTreeSet;

const ROTATIONS: [&str; 16] = [
    "ROR", "RORP", "ROL", "ROLP", "RCR", "RCRP", "RCL", "RCLP", "DROR", "DRORP", "DROL", "DROLP",
    "DRCR", "DRCRP", "DRCL", "DRCLP",
];
const OTHER_FORMS: [&str; 10] = [
    "WSFR", "WSFRP", "WSFL", "WSFLP", "SFWR", "SFWRP", "SFRD", "SFRDP", "SMOV", "SMOVP",
];

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
                address(area, base + bit)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn seed_elements(area: &str, base: u16, digits: u16, values: &[u32]) -> String {
    values
        .iter()
        .enumerate()
        .map(|(i, value)| seed_bits(area, base + i as u16 * digits * 4, digits * 4, *value))
        .collect::<Vec<_>>()
        .join("\n")
}

fn observe_elements(area: &str, base: u16, digits: u16, count: u16, out: u16) -> String {
    (0..count)
        .map(|i| {
            format!(
                "MOV K{digits}{} D{}",
                address(area, base + i * digits * 4),
                out + i
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn scenario(name: &str, seed: &str, body: &str, guards: &[(&str, bool)], observe: &str) -> Case {
    let guard_seed = guards
        .iter()
        .map(|(name, on)| format!("{} {name}", if *on { "SET" } else { "RST" }))
        .collect::<Vec<_>>()
        .join("\n");
    let guard_reads = guards
        .iter()
        .enumerate()
        .map(|(i, (name, _))| format!("LD {name}\nOUT Y{:o}", i + 8))
        .collect::<Vec<_>>()
        .join("\n");
    case(
        &format!("shift_operand_{name}"),
        &format!(
            "LD X1\nMOV H55AA D1028\nMOV H1234 D1029\nRST M1168\n{seed}\n{guard_seed}\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\nLD X0\n{body}\nLD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24\nLD M1168\nOUT Y25\n{guard_reads}\nLD X0\nORI X0\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305\n{observe}\nEND"
        ),
        stages(),
    )
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    // Sixteen rotation spellings x three writable packed areas. Only the
    // documented complete K4/K8 width is used as a rotation destination.
    for op in ROTATIONS {
        let wide = op.starts_with('D');
        let bits = if wide { 32 } else { 16 };
        for area in ["Y", "M", "S"] {
            let target = address(area, 128);
            let before = address(area, 127);
            let after = address(area, 128 + bits);
            cases.push(scenario(
                &format!("lifecycle_{op}_{target}"),
                &seed_bits(area, 128, bits, if wide { 0x87654321 } else { 0x8123 }),
                &format!("{op} K{}{target} K{}", bits / 4, if wide { 17 } else { 3 }),
                &[(before.as_str(), true), (after.as_str(), false)],
                &format!(
                    "{} K{}{target} D20",
                    if wide { "DMOV" } else { "MOV" },
                    bits / 4
                ),
            ));
        }
    }

    for op in ["WSFR", "WSFRP", "WSFL", "WSFLP"] {
        for (area, source_area) in [("Y", "M"), ("M", "S"), ("S", "Y")] {
            let source = address(source_area, 64);
            let target = address(area, 128);
            let before = address(area, 127);
            let after = address(area, 168);
            let source_before = address(source_area, 63);
            let source_after = address(source_area, 80);
            cases.push(scenario(
                &format!("lifecycle_{op}_{target}"),
                &format!(
                    "{}\n{}",
                    seed_elements(source_area, 64, 2, &[11, 22]),
                    seed_elements(area, 128, 2, &[31, 42, 53, 64, 75])
                ),
                &format!("{op} K2{source} K2{target} K5 K2\nMOV K99 K2{source}"),
                &[
                    (before.as_str(), true),
                    (after.as_str(), false),
                    (source_before.as_str(), true),
                    (source_after.as_str(), false),
                ],
                &observe_elements(area, 128, 2, 5, 20),
            ));
        }
    }

    for op in ["SFWR", "SFWRP"] {
        for (area, source_area) in [("Y", "M"), ("M", "S"), ("S", "Y")] {
            let source = address(source_area, 64);
            let target = address(area, 128);
            let before = address(area, 127);
            let after = address(area, 144);
            cases.push(scenario(
                &format!("lifecycle_{op}_{target}"),
                &format!(
                    "{}\n{}",
                    seed_bits(source_area, 64, 8, 11),
                    seed_elements(area, 128, 1, &[0, 4, 5, 6])
                ),
                &format!("{op} K2{source} K1{target} K4\nMOV K12 K2{source}"),
                &[(before.as_str(), true), (after.as_str(), false)],
                &observe_elements(area, 128, 1, 4, 20),
            ));
        }
    }

    for op in ["SFRD", "SFRDP"] {
        for area in ["Y", "M", "S"] {
            let queue = address(area, 128);
            let output = address(area, 192);
            let before = address(area, 127);
            let after = address(area, 144);
            let output_after = address(area, 200);
            cases.push(scenario(
                &format!("lifecycle_{op}_{queue}"),
                &format!(
                    "{}\n{}",
                    seed_elements(area, 128, 1, &[3, 11, 12, 13]),
                    seed_bits(area, 192, 8, 0xa5)
                ),
                &format!("{op} K1{queue} K2{output} K4"),
                &[
                    (before.as_str(), true),
                    (after.as_str(), false),
                    (output_after.as_str(), true),
                ],
                &format!(
                    "{}\nMOV K2{output} D30",
                    observe_elements(area, 128, 1, 4, 20)
                ),
            ));
        }
    }

    for op in ["SMOV", "SMOVP"] {
        for (area, source_area) in [("Y", "M"), ("M", "S"), ("S", "Y")] {
            let source = address(source_area, 64);
            let target = address(area, 128);
            let before = address(area, 127);
            let after = address(area, 144);
            cases.push(scenario(
                &format!("lifecycle_{op}_{target}"),
                &format!(
                    "{}\n{}",
                    seed_bits(source_area, 64, 16, 1234),
                    seed_bits(area, 128, 16, 5678)
                ),
                &format!("{op} K4{source} K4 K2 K4{target} K3\nMOV K1334 K4{source}"),
                &[(before.as_str(), true), (after.as_str(), false)],
                &format!("MOV K4{source} D20\nMOV K4{target} D21"),
            ));
        }
    }

    // Direct E/F destinations and full-count rotations complement the main
    // packed lifecycle cases. E/F remain aliases of the actual special D.
    cases.push(scenario(
        "rotate_word_E_F_destinations_and_hex_counts",
        "DMOV H81238765 E1\nDMOV H12348001 E2\nDMOV H23456789 D182\nDMOV H3456789A D184",
        "ROR E1 H1\nROL F1 H10\nRCR E2 H1\nRCL F2 H10",
        &[],
        "MOV E1 D20\nMOV F1 D21\nMOV E2 D22\nMOV F2 D23\nDMOV D1182 D24\nDMOV D1184 D26\nDMOV D182 D28\nDMOV D184 D30",
    ));
    cases.push(scenario(
        "rotate_double_E_pairs_full_counts",
        "DMOV H87654321 E1\nDMOV H81234567 E2\nDMOV H12345678 E3\nDMOV H80000001 E4",
        "DROR E1 H20\nDROL E2 H20\nDRCR E3 H20\nDRCL E4 H20",
        &[],
        "DMOV E1 D20\nDMOV E2 D22\nDMOV E3 D24\nDMOV E4 D26\nDMOV D1182 D28\nDMOV D1184 D30\nDMOV D1186 D32\nDMOV D1188 D34",
    ));
    cases.push(scenario(
        "rotate_packed_full_count_carry_states",
        &format!("{}\n{}\n{}\n{}", seed_bits("M", 128, 16, 0x8123), seed_bits("M", 160, 16, 0x8123), seed_bits("M", 192, 32, 0x87654321), seed_bits("M", 256, 32, 0x87654321)),
        "RST M1022\nROR K4M128 K16\nLD M1022\nOUT Y0\nLD X0\nRST M1022\nRCR K4M160 K16\nLD M1022\nOUT Y1\nLD X0\nSET M1022\nDROR K8M192 K32\nLD M1022\nOUT Y2\nLD X0\nSET M1022\nDRCL K8M256 K32\nLD M1022\nOUT Y3",
        &[("M127", true), ("M144", false), ("M224", true), ("M288", false)],
        "MOV K4M128 D20\nMOV K4M160 D21\nDMOV K8M192 D22\nDMOV K8M256 D24",
    ));
    cases.push(scenario(
        "word_shift_overlapping_group_column_order",
        &format!(
            "{}\n{}",
            seed_elements("M", 128, 1, &[1, 2, 3, 4, 5, 6]),
            seed_elements("M", 192, 1, &[1, 2, 3, 4, 5, 6])
        ),
        "WSFR K1M136 K1M128 K5 K2\nWSFL K1M200 K1M192 K5 K2",
        &[
            ("M127", true),
            ("M152", false),
            ("M191", true),
            ("M216", false),
        ],
        &format!(
            "{}\n{}",
            observe_elements("M", 128, 1, 6, 20),
            observe_elements("M", 192, 1, 6, 30)
        ),
    ));
    cases.push(scenario(
        "word_shift_complete_words_remainder_columns",
        &format!(
            "{}\n{}\n{}",
            seed_elements("Y", 64, 4, &[101, 202, 303]),
            seed_elements("M", 128, 4, &[10, 20, 30, 40, 50, 60, 70]),
            seed_elements("S", 128, 4, &[10, 20, 30, 40, 50, 60, 70])
        ),
        "WSFR K4Y100 K4M128 K7 K3\nWSFL K4Y100 K4S128 K7 K3",
        &[
            ("M127", true),
            ("M240", false),
            ("S127", true),
            ("S240", false),
        ],
        &format!(
            "{}\n{}",
            observe_elements("M", 128, 4, 7, 20),
            observe_elements("S", 128, 4, 7, 30)
        ),
    ));
    let mut input_shift = scenario(
        "word_shift_and_fifo_KnX_sources",
        &format!(
            "{}\n{}\n{}",
            seed_elements("M", 128, 1, &[1, 2, 3, 4, 5]),
            seed_elements("S", 128, 1, &[1, 2, 3, 4, 5]),
            seed_elements("M", 256, 1, &[0, 10, 11, 12])
        ),
        "WSFR K1X10 K1M128 K5 K2\nWSFL K1X10 K1S128 K5 K2\nSFWR K1X10 K1M256 K4",
        &[
            ("M127", true),
            ("M148", false),
            ("S127", true),
            ("S148", false),
            ("M255", true),
            ("M272", false),
        ],
        &format!(
            "{}\n{}\n{}",
            observe_elements("M", 128, 1, 5, 20),
            observe_elements("S", 128, 1, 5, 30),
            observe_elements("M", 256, 1, 4, 40)
        ),
    );
    for (i, name) in ["X10", "X11", "X12", "X13", "X14", "X15", "X16", "X17"]
        .into_iter()
        .enumerate()
    {
        input_shift.stages[0].0.push((name, 0xbcu8 & (1 << i) != 0));
    }
    input_shift.stages[2].0.push(("X10", true));
    cases.push(input_shift);

    cases.push(scenario(
        "fifo_push_capacity_and_full_pointer_hold",
        &format!(
            "{}\n{}",
            seed_elements("M", 128, 1, &[2, 4, 5, 6]),
            seed_elements("M", 192, 1, &[2, 7, 8, 9])
        ),
        "SFWR K11 K1M128 K4\nLD M1022\nOUT Y0\nLD X0\nSFWRP K12 K1M192 K4\nLD M1022\nOUT Y1",
        &[
            ("M127", true),
            ("M144", false),
            ("M191", true),
            ("M208", false),
        ],
        &format!(
            "{}\n{}",
            observe_elements("M", 128, 1, 4, 20),
            observe_elements("M", 192, 1, 4, 30)
        ),
    ));
    cases.push(scenario(
        "fifo_pop_empty_and_last_slot_retention",
        &format!(
            "{}\n{}\n{}\n{}",
            seed_elements("M", 128, 1, &[0, 7, 8, 9]),
            seed_elements("M", 192, 1, &[1, 10, 11, 12]),
            seed_bits("M", 256, 8, 0xa5),
            seed_bits("M", 288, 8, 0x5a)
        ),
        "SFRD K1M128 K2M256 K4\nLD M1020\nOUT Y0\nLD X0\nSFRD K1M192 K2M288 K4\nLD M1020\nOUT Y1",
        &[
            ("M144", true),
            ("M208", false),
            ("M264", true),
            ("M296", false),
        ],
        &format!(
            "{}\n{}\nMOV K2M256 D40\nMOV K2M288 D41",
            observe_elements("M", 128, 1, 4, 20),
            observe_elements("M", 192, 1, 4, 30)
        ),
    ));
    cases.push(scenario(
        "fifo_push_snapshots_head_payload_alias",
        &format!(
            "{}\n{}",
            seed_elements("M", 128, 1, &[0, 7, 8, 9]),
            seed_elements("M", 192, 2, &[0, 71, 82, 93])
        ),
        "SFWR K1M128 K1M128 K4\nSFWR K2M192 K2M192 K4",
        &[
            ("M127", true),
            ("M144", false),
            ("M191", true),
            ("M224", false),
        ],
        &format!(
            "{}\n{}",
            observe_elements("M", 128, 1, 4, 20),
            observe_elements("M", 192, 2, 4, 30)
        ),
    ));
    cases.push(scenario(
        "fifo_pop_output_aliases_head_before_shift",
        &format!(
            "{}\n{}",
            seed_elements("M", 128, 1, &[3, 11, 12, 13]),
            seed_elements("M", 192, 1, &[3, 11, 12, 13])
        ),
        "SFRD K1M128 K1M128 K4\nSFRD K1M192 K2M192 K4",
        &[
            ("M127", true),
            ("M144", false),
            ("M191", true),
            ("M208", false),
        ],
        &format!(
            "{}\n{}",
            observe_elements("M", 128, 1, 4, 20),
            observe_elements("M", 192, 1, 4, 30)
        ),
    ));
    cases.push(scenario(
        "fifo_pop_output_aliases_unread_payload",
        &format!(
            "{}\n{}",
            seed_elements("M", 128, 1, &[3, 11, 12, 13]),
            seed_elements("M", 192, 1, &[3, 11, 12, 13])
        ),
        "SFRD K1M128 K1M136 K4\nSFRD K1M192 K1M204 K4",
        &[
            ("M127", true),
            ("M144", false),
            ("M191", true),
            ("M208", false),
        ],
        &format!(
            "{}\n{}",
            observe_elements("M", 128, 1, 4, 20),
            observe_elements("M", 192, 1, 4, 30)
        ),
    ));
    let wrap_values: Vec<u32> = (0..18).map(|n| if n == 0 { 15 } else { n % 16 }).collect();
    cases.push(scenario(
        "fifo_four_bit_pointer_wrap_keeps_untruncated_payload_slot",
        &seed_elements("M", 128, 1, &wrap_values),
        "SFWR K9 K1M128 K18",
        &[("M127", true), ("M200", false)],
        "MOV K1M128 D20\nMOV K1M132 D21\nMOV K1M136 D22\nMOV K1M188 D23\nMOV K1M192 D24\nMOV K1M196 D25",
    ));
    cases.push(scenario(
        "fifo_E_F_sources_and_pop_destinations",
        &format!("MOV H1234 E1\nMOV H5678 F1\nDMOV H23456789 E2\n{}\n{}\n{}\n{}", seed_elements("M", 128, 2, &[0, 41, 42, 43]), seed_elements("S", 128, 2, &[0, 51, 52, 53]), seed_elements("M", 192, 4, &[3, 1111, 2222, 3333]), seed_elements("S", 192, 4, &[3, 4444, 5555, 6666])),
        "SFWR E1 K2M128 K4\nSFWR F1 K2S128 K4\nSFRD K4M192 E2 K4\nSFRD K4S192 F2 K4",
        &[("M160", true), ("S160", false), ("M256", true), ("S256", false)],
        &format!("{}\n{}\nMOV E1 D40\nMOV F1 D41\nMOV E2 D42\nMOV F2 D43\nDMOV D1182 D44\nDMOV D1184 D46", observe_elements("M", 128, 2, 4, 20), observe_elements("S", 128, 2, 4, 30)),
    ));
    cases.push(scenario(
        "smov_hex_short_target_and_shifted_alias_snapshot",
        &format!(
            "SET M1168\n{}\n{}\n{}\n{}\n{}",
            seed_bits("M", 64, 16, 0xfa12),
            seed_bits("M", 128, 16, 0x5abc),
            seed_bits("M", 192, 20, 0xa1234),
            seed_bits("M", 256, 4, 7),
            seed_bits("M", 288, 16, 0x5678)
        ),
        "SMOV K4M64 K4 K2 K2M128 K2\nSMOV K4M192 K4 K2 K4M196 K3\nSMOV K1M256 K4 K2 K4M288 K3",
        &[
            ("M127", true),
            ("M144", false),
            ("M191", true),
            ("M212", false),
            ("M260", true),
            ("M304", false),
        ],
        "MOV K4M128 D20\nMOV K4M192 D21\nMOV K4M196 D22\nMOV K4M288 D23",
    ));
    cases.push(scenario(
        "smov_decimal_checks_full_source_and_destination_before_truncation",
        &format!(
            "MOV K1234 E1\nMOV K10000 F1\n{}\n{}\n{}\n{}",
            seed_bits("M", 64, 16, 0xffff),
            seed_bits("M", 128, 16, 77),
            seed_bits("S", 64, 12, 4095),
            seed_bits("S", 128, 16, 0x5a55)
        ),
        "SMOV K4M64 K4 K2 K4M128 K3\nSMOV E1 K4 K2 F1 K3\nSMOV K3S64 K2 K1 K2S128 K1",
        &[
            ("M127", true),
            ("M144", false),
            ("S127", true),
            ("S144", false),
        ],
        "MOV K4M128 D20\nMOV E1 D21\nMOV F1 D22\nMOV K4S128 D23\nDMOV D1182 D24",
    ));
    let mut input_digits = scenario(
        "smov_KnX_E_F_decimal_to_hex_mode",
        &format!(
            "MOV K5678 E1\nMOV K4321 F1\n{}",
            seed_bits("M", 128, 16, 8765)
        ),
        "SMOV K4X20 K4 K2 E1 K3\nSMOV F1 K4 K2 K4M128 K3\nLD X2\nOUT M1168",
        &[("M127", true), ("M144", false)],
        "MOV E1 D20\nMOV F1 D21\nMOV K4M128 D22\nDMOV D1182 D24",
    );
    for (i, name) in [
        "X20", "X21", "X22", "X23", "X24", "X25", "X26", "X27", "X30", "X31", "X32", "X33", "X34",
        "X35", "X36", "X37",
    ]
    .into_iter()
    .enumerate()
    {
        input_digits.stages[0]
            .0
            .push((name, 1234u16 & (1 << i) != 0));
    }
    input_digits.stages[2].0.push(("X2", true));
    cases.push(input_digits);

    // The injected source starts two bits before its target. Sequential
    // ascending bit reads can observe earlier stores from the same injection;
    // a group snapshot would produce a different low nibble.
    cases.push(scenario(
        "word_shift_unaligned_source_observes_earlier_injected_bits",
        &format!(
            "{}\n{}",
            seed_bits("M", 128, 12, 0x0a59),
            seed_bits("S", 128, 12, 0x0a59)
        ),
        "WSFL K1M126 K1M128 K3 K1\nWSFR K1S126 K1S128 K3 K1",
        &[
            ("M126", false),
            ("M127", true),
            ("M140", false),
            ("S126", false),
            ("S127", true),
            ("S140", false),
        ],
        "MOV K3M128 D20\nMOV K1M126 D21\nMOV K3S128 D22\nMOV K1S126 D23",
    ));

    assert_eq!(cases.len(), 95);
    let names: BTreeSet<_> = cases.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names.len(), cases.len());
    let actual: BTreeSet<_> = cases
        .iter()
        .flat_map(|c| c.source.lines())
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    for name in ROTATIONS.into_iter().chain(OTHER_FORMS) {
        assert!(actual.contains(name), "missing shift form {name}");
    }
    cases
}
