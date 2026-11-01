//! Grouped and direct-index operand fixtures for all 30 ABS/Gray/indexed forms.
//! Fixture construction alone is not an original simulator compatibility result.
use super::{Case, case};

// Exhaustive spelling inventory: 18 actions and 12 indexed contacts.
const ACTIONS: [(&str, &str, bool); 18] = [
    ("ABS", "ABS", false),
    ("ABSP", "ABS", false),
    ("DABS", "ABS", true),
    ("DABSP", "ABS", true),
    ("GRY", "GRY", false),
    ("GRYP", "GRY", false),
    ("DGRY", "GRY", true),
    ("DGRYP", "GRY", true),
    ("GBIN", "GBIN", false),
    ("GBINP", "GBIN", false),
    ("DGBIN", "GBIN", true),
    ("DGBINP", "GBIN", true),
    ("BOUT", "BOUT", false),
    ("DBOUT", "BOUT", true),
    ("BSET", "BSET", false),
    ("DBSET", "BSET", true),
    ("BRST", "BRST", false),
    ("DBRST", "BRST", true),
];
const CONTACTS: [(&str, &str, bool); 12] = [
    ("BLD", "LD", false),
    ("DBLD", "LD", true),
    ("BLDI", "LD", false),
    ("DBLDI", "LD", true),
    ("BAND", "AND", false),
    ("DBAND", "AND", true),
    ("BANI", "AND", false),
    ("DBANI", "AND", true),
    ("BOR", "OR", false),
    ("DBOR", "OR", true),
    ("BORI", "OR", false),
    ("DBORI", "OR", true),
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
                address(area, base + bit),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn contact(mnemonic: &str, combine: &str, source: &str, index: &str, output: u16) -> String {
    // AND is evaluated with true continuity, OR with false continuity. Only
    // the two fixture inputs are used, and each LD has a complete rung.
    let prefix = match combine {
        "AND" => "LD X0\nORI X0\n",
        "OR" => "LD X0\nANI X0\n",
        _ => "",
    };
    format!("{prefix}{mnemonic} {source} {index}\nOUT Y{output:o}")
}

fn scenario(name: &str, seed: &str, body: &str, guards: &[(&str, bool)], observe: &str) -> Case {
    let powered_body = if body.starts_with("LD X0\n")
        || body.starts_with("BLD ")
        || body.starts_with("BLDI ")
        || body.starts_with("DBLD ")
        || body.starts_with("DBLDI ")
    {
        body.to_owned()
    } else {
        format!("LD X0\n{body}")
    };
    let guard_seed = guards
        .iter()
        .map(|(name, on)| format!("{} {name}", if *on { "SET" } else { "RST" }))
        .collect::<Vec<_>>()
        .join("\n");
    let guard_reads = guards
        .iter()
        .enumerate()
        .map(|(index, (name, _))| format!("LD {name}\nOUT Y{:o}", index + 8))
        .collect::<Vec<_>>()
        .join("\n");
    case(
        &format!("bit_operand_{name}"),
        &format!(
            "LD X1\nMOV H55AA D1028\nMOV H1234 D1029\n{seed}\n{guard_seed}\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\n{powered_body}\nLD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24\n{guard_reads}\nLD X0\nORI X0\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305\n{observe}\nEND"
        ),
        stages(),
    )
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();

    // 18 spellings x Y200/M128/S128 = 54 lifecycle cases. Every normal/P
    // pair sees a changed source on its second held scan and a later rearm.
    for (mnemonic, family, wide) in ACTIONS {
        let bits = if wide { 32 } else { 16 };
        let digits = bits / 4;
        for area in ["Y", "M", "S"] {
            let target = address(area, 128);
            let target_bits = if family == "ABS" { bits } else { 4 };
            let guard_names = [
                address(area, 127),
                address(area, 128 + target_bits),
                address(area, 129 + target_bits),
                address(area, 128 + if wide { 31 } else { 15 }),
            ];
            let guards = if family == "ABS" {
                vec![
                    (guard_names[0].as_str(), true),
                    (guard_names[1].as_str(), false),
                    (guard_names[2].as_str(), true),
                ]
            } else {
                vec![
                    (guard_names[0].as_str(), true),
                    (guard_names[1].as_str(), false),
                    (guard_names[2].as_str(), true),
                    (guard_names[3].as_str(), true),
                ]
            };
            let (seed, body) = if family == "ABS" {
                (
                    seed_bits(area, 128, bits, (-5i32) as u32),
                    format!(
                        "{mnemonic} K{digits}{target}\n{} K{digits}{target}",
                        if wide { "DNEG" } else { "NEG" },
                    ),
                )
            } else if matches!(family, "GRY" | "GBIN") {
                (
                    format!(
                        "{}\n{}",
                        seed_bits("M", 64, bits, 3),
                        seed_bits(area, 128, 4, 10),
                    ),
                    format!(
                        "{mnemonic} K{digits}M64 K1{target}\n{} K{digits}M64",
                        if wide { "DINC" } else { "INC" },
                    ),
                )
            } else {
                (
                    format!(
                        "{}\n{}",
                        seed_bits("M", 64, 4, 1),
                        seed_bits(area, 128, 4, if family == "BSET" { 1 } else { 15 }),
                    ),
                    format!("{mnemonic} K1{target} K1M64\nINC K1M64"),
                )
            };
            cases.push(scenario(
                &format!("lifecycle_{mnemonic}_{target}"),
                &seed,
                &body,
                &guards,
                "",
            ));
        }
    }

    // Each contact spelling independently observes short/full source width,
    // an out-of-group bit, a grouped dynamic index and direct E/F indexes.
    for (mnemonic, combine, wide) in CONTACTS {
        let bits = if wide { 32 } else { 16 };
        let high = if wide { 31 } else { 15 };
        let seed = format!(
            "{}\n{}\nMOV K{high} E1\nMOV K0 F1\nMOV H89AB D1184",
            seed_bits("M", 128, bits, if wide { 0x8000_8009 } else { 0x8009 }),
            seed_bits("M", 64, 4, 3),
        );
        let full = format!("K{}M128", bits / 4);
        let body = [
            contact(mnemonic, combine, "K1M128", "K3", 0),
            contact(mnemonic, combine, "K1M128", &format!("K{high}"), 1),
            contact(mnemonic, combine, &full, &format!("K{high}"), 2),
            contact(mnemonic, combine, "K1M128", "K1M64", 3),
            contact(mnemonic, combine, &full, "E1", 4),
            contact(mnemonic, combine, &full, "F1", 5),
            "LD X0\nINC K1M64".to_owned(),
        ]
        .join("\n");
        cases.push(scenario(
            &format!("contact_{mnemonic}_short_full_group_E_F"),
            &seed,
            &body,
            &[
                ("M127", true),
                ("M160", false),
                ("M63", true),
                ("M68", false),
            ],
            "MOV E1 D30\nMOV F1 D31\nMOV D1184 D32",
        ));
    }

    // Dynamic negative indexes wrap to15/31, but short destination writes
    // still truncate. Both physical would-be selected bits are explicit guards.
    cases.push(scenario(
        "outside_short_group_dynamic_negative_index",
        &format!(
            "{}\n{}\n{}\n{}\n{}\n{}\nMOV K-1 D10\nDMOV H89ABFFFF D12",
            seed_bits("M", 128, 4, 5),
            seed_bits("M", 160, 4, 6),
            seed_bits("M", 192, 4, 9),
            seed_bits("M", 224, 4, 10),
            seed_bits("M", 256, 4, 3),
            seed_bits("M", 288, 4, 12),
        ),
        "BOUT K1M128 D10\nBSET K1M160 D10\nBRST K1M192 D10\nDBOUT K1M224 D12\nDBSET K1M256 D12\nDBRST K1M288 D12",
        &[
            ("M143", true),
            ("M175", false),
            ("M207", true),
            ("M255", true),
            ("M287", false),
            ("M319", true),
        ],
        "MOV K1M128 D20\nMOV K1M160 D21\nMOV K1M192 D22\nMOV K1M224 D23\nMOV K1M256 D24\nMOV K1M288 D25",
    ));

    cases.push(scenario(
        "gray_word_full_sign_errors_short_zero_extension",
        &format!(
            "{}\nMOV H1234 D20\nMOV H5678 D21\nMOV K41 D22\nMOV K42 D23",
            seed_bits("M", 64, 16, 0xffff),
        ),
        "GRY K4M64 D20\nGBIN K4M64 D21\nGRY K3M64 D22\nGBIN K3M64 D23",
        &[("M63", true), ("M80", false)],
        "",
    ));
    cases.push(scenario(
        "gray_double_full_sign_errors_short_zero_extension",
        &format!(
            "{}\nDMOV H12345678 D20\nDMOV H56789ABC D22\nDMOV K41 D24\nDMOV K42 D26",
            seed_bits("M", 64, 32, u32::MAX),
        ),
        "DGRY K8M64 D20\nDGBIN K8M64 D22\nDGRY K7M64 D24\nDGBIN K7M64 D26",
        &[("M63", true), ("M96", false)],
        "",
    ));
    cases.push(scenario(
        "gray_shifted_group_alias_snapshot",
        &format!(
            "{}\n{}\n{}\n{}",
            seed_bits("M", 128, 20, 0xa1234),
            seed_bits("M", 192, 20, 0xb1971),
            seed_bits("M", 256, 32, 0x1234_5678),
            seed_bits("M", 320, 32, 0x1971_2345),
        ),
        "GRY K4M128 K4M132\nGBIN K4M192 K4M196\nDGRY K8M256 K8M264\nDGBIN K8M320 K8M328",
        &[
            ("M127", true),
            ("M148", false),
            ("M255", true),
            ("M296", false),
        ],
        "",
    ));
    cases.push(scenario(
        "index_aliased_group_snapshot_before_store",
        &format!(
            "{}\n{}\n{}\n{}",
            seed_bits("M", 128, 16, 3),
            seed_bits("M", 192, 32, 17),
            seed_bits("M", 256, 16, 0x800f),
            seed_bits("M", 320, 32, 0x8000_001f),
        ),
        "BSET K4M128 K1M128\nDBOUT K8M192 K2M192\nBRST K4M256 K1M256\nDBRST K8M320 K2M320",
        &[
            ("M127", true),
            ("M144", false),
            ("M191", true),
            ("M224", false),
        ],
        "",
    ));
    cases.push(scenario(
        "absolute_minima_E_F_and_wide_negative_32768",
        &format!(
            "{}\n{}\n{}\nMOV H8000 E1\nMOV H8000 F1\nDMOV H80000000 E2\nDMOV K-32768 E3",
            seed_bits("M", 128, 16, 0x8000),
            seed_bits("M", 192, 32, 0x8000_0000),
            seed_bits("M", 256, 4, 15),
        ),
        "ABS K4M128\nDABS K8M192\nABS K1M256\nABS E1\nABS F1\nDABS E2\nDABS E3",
        &[("M127", true), ("M144", false), ("M191", true), ("M224", false)],
        "MOV E1 D20\nMOV F1 D21\nDMOV E2 D22\nDMOV E3 D24\nMOV D1182 D30\nMOV D1183 D31\nDMOV D1184 D32\nDMOV D1186 D34",
    ));
    cases.push(scenario(
        "indexed_timer_counter_natural_width_boundaries",
        "MOV H1234 T255\nMOV H1234 C199\nDMOV H12345678 C200\nDMOV HFFFFFFFF C255\nDMOV K41 C201\nMOV K42 C198\nDMOV K43 C254",
        "BOUT T255 K15\nBSET C199 K15\nDBOUT C200 K31\nDBRST C255 K31",
        &[],
        "MOV T255 D20\nMOV C199 D21\nDMOV C200 D22\nDMOV C255 D24\nDMOV C201 D26\nMOV C198 D28\nDMOV C254 D30",
    ));
    cases.push(scenario(
        "double_C199_zero_extension_and_short_destination",
        &format!(
            "MOV HFFFF C199\nDMOV H89ABCDEF C200\n{}\n{}",
            seed_bits("M", 128, 32, u32::MAX),
            seed_bits("M", 192, 4, 10),
        ),
        "DABS C199\nDGRY C199 K8M128\nDGBIN K8M128 C199\nDGRY C199 K1M192\nDBSET C199 K31",
        &[
            ("M127", true),
            ("M160", false),
            ("M191", true),
            ("M196", false),
        ],
        "MOV C199 D20\nDMOV C200 D22\nDMOV K8M128 D24\nMOV K1M192 D26",
    ));

    // F0/F1/F7 double n operands have a full four-word descriptor. A nonzero
    // next-D highword must neither affect the low-five-bit selection nor be
    // overwritten as scratch. Every double output and contact is exercised.
    for (index, initial, high_word) in [
        ("F0", "HFFFF", 1030),
        ("F1", "K32", 1184),
        ("F7", "K47", 1196),
    ] {
        let seed = format!(
            "{}\n{}\n{}\n{}\nMOV {initial} {index}\nMOV H89AB D{high_word}",
            seed_bits("M", 128, 32, 0xa55a_ffff),
            seed_bits("M", 192, 32, 0),
            seed_bits("M", 256, 32, u32::MAX),
            seed_bits("M", 384, 32, 0x8000_8001),
        );
        let mut body =
            format!("DBOUT K8M128 {index}\nDBSET K8M192 {index}\nDBRST K8M256 {index}\n");
        for (output, (mnemonic, combine)) in [
            ("DBLD", "LD"),
            ("DBLDI", "LD"),
            ("DBAND", "AND"),
            ("DBANI", "AND"),
            ("DBOR", "OR"),
            ("DBORI", "OR"),
        ]
        .into_iter()
        .enumerate()
        {
            body.push_str(&contact(mnemonic, combine, "K8M384", index, output as u16));
            body.push('\n');
        }
        body.push_str(&format!("LD X0\nINC {index}"));
        cases.push(scenario(
            &format!("double_F_index_{index}_all_outputs_contacts"),
            &seed,
            &body,
            &[
                ("M127", true),
                ("M160", false),
                ("M191", true),
                ("M224", false),
                ("M255", true),
                ("M288", false),
            ],
            &format!("MOV {index} D30\nMOV D{high_word} D31"),
        ));
    }

    debug_assert_eq!(cases.len(), 77);
    cases
}
