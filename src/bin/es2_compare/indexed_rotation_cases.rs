//! Draft only: documented addressed ROR/ROL/RCR/RCL and their D/P forms.
//! No native execution or parity is implied. Literal oracle vectors do not
//! call the production rotation kernel. Packed operands remain K4/K8 only.
//! No currently admitted complete writable K4/K8 span contains M1022, so
//! carry-overwritten-by-target is not claimed by these public-syntax cases.
use super::{Case as ComparisonCase, Target, case};
use std::collections::BTreeSet;

const FORMS: [&str; 16] = [
    "ROR", "RORP", "ROL", "ROLP", "RCR", "RCRP", "RCL", "RCLP", "DROR", "DRORP", "DROL", "DROLP",
    "DRCR", "DRCRP", "DRCL", "DRCLP",
];
const BASES: [&str; 4] = ["ROR", "ROL", "RCR", "RCL"];

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
fn address(area: &str, index: u16) -> String {
    if matches!(area, "X" | "Y") {
        format!("{area}{index:o}")
    } else {
        format!("{area}{index}")
    }
}
fn mov(wide: bool) -> &'static str {
    if wide { "DMOV" } else { "MOV" }
}
fn index_alias(bank: char, index: u16) -> u16 {
    (if index == 0 { 1028 } else { 1180 + index * 2 }) + u16::from(bank == 'F')
}
fn seed_words(area: &str, base: u16, count: u16) -> String {
    (0..count)
        .map(|i| {
            format!(
                "MOV H{:04X} {area}{}",
                0xa55au16.wrapping_add(i * 0x111),
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
                if i == 0 || i + 1 == count || i % 5 == 2 {
                    "SET"
                } else {
                    "RST"
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
    let gate = if body.starts_with("LD") || body.starts_with("STL") {
        ""
    } else {
        "LD X0"
    };
    let source = [
        "LD X1\nMOV H55AA E0\nMOV H1234 F0\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV H1234 D1067\nMOV K77 D1068",
        seed, gate, body,
        "LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24\nLDI X7\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305\nMOV K1234 D306",
        observe, "END",
    ].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n");
    assert!(source.lines().count() <= 512, "source budget: {name}");
    assert!(source.lines().count() * 9 <= 0x1ef0, "WORD budget: {name}");
    let mut indexed = false;
    for line in source.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.first().is_some_and(|op| FORMS.contains(op)) {
            assert_eq!(fields.len(), 3, "{name}: {line}");
            assert!(fields[2].starts_with(['K', 'H']), "constant-only n: {line}");
            let count = if let Some(h) = fields[2].strip_prefix('H') {
                u16::from_str_radix(h, 16).unwrap()
            } else {
                fields[2][1..].parse::<u16>().unwrap()
            };
            assert!((1..=if fields[0].starts_with('D') { 32 } else { 16 }).contains(&count));
            if fields[1].starts_with('K') {
                assert!(fields[1].starts_with(if fields[0].starts_with('D') {
                    "K8"
                } else {
                    "K4"
                }));
            }
            let bytes = fields[1].as_bytes();
            indexed |= bytes.len() >= 4
                && matches!(bytes[bytes.len() - 2], b'E' | b'F')
                && bytes[bytes.len() - 1].is_ascii_digit();
        }
    }
    assert!(indexed, "missing indexed rotation: {name}");
    case(&format!("indexed_rotation_{name}"), &source, stages())
}
fn check_cases(cases: &[ComparisonCase], count: usize) {
    assert_eq!(cases.len(), count);
    assert_eq!(
        cases.iter().map(|c| &c.name).collect::<BTreeSet<_>>().len(),
        count
    );
    let names: BTreeSet<_> = cases
        .iter()
        .flat_map(|c| c.source.lines())
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    for op in FORMS {
        assert!(names.contains(op), "missing {op}");
    }
}

pub(super) fn common_cases() -> Vec<ComparisonCase> {
    let mut cases = Vec::new();
    append_banks(&mut cases);
    append_literal_oracles(&mut cases);
    append_aliases(&mut cases);
    append_numeric_edges(&mut cases);
    append_control(&mut cases);
    append_steps(&mut cases);
    check_cases(&cases, 84);
    cases
}

fn append_banks(cases: &mut Vec<ComparisonCase>) {
    for wide in [false, true] {
        for bank in ['E', 'F'] {
            for index in 0..8u16 {
                let modifier = format!("{bank}{index}");
                let area = ["D", "T", "C", "Y", "M", "S"]
                    [(usize::from(index) + if bank == 'F' { 3 } else { 0 }) % 6];
                let grouped = matches!(area, "Y" | "M" | "S");
                let bits = if wide { 32 } else { 16 };
                let offset: i16 = if index % 2 == 0 { -1 } else { 1 };
                let next = -offset;
                let suffix = if index % 2 == 0 { "" } else { "P" };
                let prefix = if wide { "D" } else { "" };
                let mut seed = vec![format!("MOV K{offset} {modifier}")];
                // F is a signed WORD modifier even for a DWORD rotation.
                // F1..F6 high-word poison stays in canonical E/F backing words.
                if bank == 'F' && (1..7).contains(&index) {
                    seed.push(format!("MOV H6A5A D{}", index_alias(bank, index) + 1));
                }
                let mut body = Vec::new();
                let mut observe = vec![format!(
                    "MOV {modifier} D250\nMOV D{} D251",
                    index_alias(bank, index)
                )];
                for (slot, family) in BASES.into_iter().enumerate() {
                    let base = if area == "Y" {
                        64 + slot as u16 * 48
                    } else if grouped {
                        128 + slot as u16 * 64
                    } else {
                        100 + slot as u16 * 20
                    };
                    let target = if grouped {
                        seed.push(seed_bits(area, base - 4, bits + 8));
                        observe.push(read_bits(area, base - 4, bits + 8, 200 + slot as u16 * 4));
                        format!("K{}{}{modifier}", bits / 4, address(area, base))
                    } else {
                        seed.push(seed_words(area, base - 2, 6));
                        observe.push(read_words(area, base - 2, 6, 200 + slot as u16 * 6));
                        format!("{area}{base}{modifier}")
                    };
                    body.push(format!(
                        "LD X0\n{prefix}{family}{suffix} {target} {}\nLD M1022\nOUT M{}",
                        if slot == 3 { "H1" } else { "K1" },
                        700 + slot
                    ));
                }
                // All candidate offsets remain valid on OFF and held-P visits.
                body.push(format!("LD X0\nMOV K{next} {modifier}"));
                observe.push("MOV K1M700 D248".into());
                cases.push(scenario(
                    &format!(
                        "banks_{}_{}_{}",
                        if wide { "double" } else { "word" },
                        modifier,
                        area
                    ),
                    &seed.join("\n"),
                    &body.join("\n"),
                    &observe.join("\n"),
                ));
            }
        }
    }
    assert_eq!(cases.len(), 32);
}

// For seed8001/80000001, six literal experiments are: count1 carry0,
// count1 carry1, countWIDTH carry0, countWIDTH carry1, zero/count1 carry1,
// all-ones/count3 carry0. Carry rotations are rings of17 or33 bits.
fn literal_results(family: &str, wide: bool) -> [u32; 6] {
    match (family, wide) {
        ("ROR", false) => [0xc000, 0xc000, 0x8001, 0x8001, 0, 0xffff],
        ("ROL", false) => [3, 3, 0x8001, 0x8001, 0, 0xffff],
        ("RCR", false) => [0x4000, 0xc000, 2, 3, 0x8000, 0xdfff],
        ("RCL", false) => [2, 3, 0x4000, 0xc000, 1, 0xfffb],
        ("ROR", true) => [
            0xc0000000, 0xc0000000, 0x80000001, 0x80000001, 0, 0xffffffff,
        ],
        ("ROL", true) => [3, 3, 0x80000001, 0x80000001, 0, 0xffffffff],
        ("RCR", true) => [0x40000000, 0xc0000000, 2, 3, 0x80000000, 0xdfffffff],
        ("RCL", true) => [2, 3, 0x40000000, 0xc0000000, 1, 0xfffffffb],
        _ => unreachable!(),
    }
}
fn append_literal_oracles(cases: &mut Vec<ComparisonCase>) {
    for op in FORMS {
        let wide = op.starts_with('D');
        let family = op.trim_start_matches('D').trim_end_matches('P');
        let width = if wide { 32 } else { 16 };
        let input = if wide { 0x80000001u32 } else { 0x8001 };
        let full = if wide { u32::MAX } else { 0xffff };
        let m = mov(wide);
        let cmp = if wide { "DAND=" } else { "AND=" };
        let values = [input, input, input, input, 0, full];
        let counts = [
            "K1".to_owned(),
            "H1".into(),
            format!("K{width}"),
            format!("H{width:X}"),
            "K1".into(),
            "H3".into(),
        ];
        let carries = [false, true, false, true, true, false];
        let outgoing = [true, true, true, true, false, true];
        let results = literal_results(family, wide);
        let mut body = vec![format!(
            "LD X0\n{}OUT M791",
            if op.ends_with('P') { "ANI M790\n" } else { "" }
        )];
        for n in 0..6 {
            let (input, result, carry, next_carry) =
                (values[n], results[n], carries[n], outgoing[n]);
            let (zero, borrow) = (n % 2 == 0, n % 2 != 0);
            body.push(format!(
                "LDI X7\nMOV K1 E1\n{m} H{input:X} D101\n{} M1020\n{} M1021\n{} M1022\nLD X0\n{op} D100E1 {}\nLDI X7\n{m} D101 D{}\nLD M1022\nOUT M{}\nLD M791\n{cmp} D101 H{result:X}\n{} M1022\nLDI M791\n{cmp} D101 H{input:X}\n{} M1022\nORB\n{} M1020\n{} M1021\nOUT M{}",
                if zero { "SET" } else { "RST" }, if borrow { "SET" } else { "RST" }, if carry { "SET" } else { "RST" }, counts[n], 80+n*2, 880+n,
                if next_carry { "AND" } else { "ANI" }, if carry { "AND" } else { "ANI" }, if zero { "AND" } else { "ANI" }, if borrow { "AND" } else { "ANI" }, 840+n,
            ));
        }
        body.push("LD X0\nOUT M790".into());
        cases.push(scenario(
            &format!("literal_oracle_{op}"),
            &format!(
                "{}\nMOV K1 E1\nMOV H6A5A F1\nRST M790\nRST M791",
                seed_words("D", 99, 6)
            ),
            &body.join("\n"),
            &format!(
                "{}\n{}\nMOV K2M840 D248\nMOV K2M880 D249\nMOV E1 D250\nMOV F1 D251",
                read_words("D", 80, 12, 220),
                read_words("D", 99, 6, 236)
            ),
        ));
    }
    assert_eq!(cases.len(), 48);
}

fn append_aliases(cases: &mut Vec<ComparisonCase>) {
    for wide in [false, true] {
        for family in BASES {
            let op = format!("{}{family}", if wide { "D" } else { "" });
            let (modifier, alias) = if wide { ("F1", 1183u16) } else { ("E1", 1182) };
            let target = format!("D{}{modifier}", alias - 2);
            let seed = "MOV H55AA E1\nMOV H8001 F1\nMOV H8001 E2\nMOV H5AA5 F2\nMOV H2468 E3\nMOV H1357 F3";
            // Refresh the overwritten modifier on EVERY visit, before even an
            // OFF/held native preflight. The high writer must retain its base.
            let body = format!(
                "LDI X7\nMOV K2 {modifier}\nLD X0\n{op} {target} K1\nLDI X7\nMOV E1 D80\nMOV F1 D81\nMOV E2 D82\nMOV F2 D83\nMOV K2 {modifier}\nLD X0\n{op}P {target} K1\nLDI X7\nMOV E1 D84\nMOV F1 D85\nMOV E2 D86\nMOV F2 D87"
            );
            cases.push(scenario(&format!("own_index_cached_pair_{op}_{modifier}"), seed, &body, &format!("{}\nMOV E1 D240\nMOV F1 D241\nMOV E2 D242\nMOV F2 D243\nMOV E3 D244\nMOV F3 D245", read_words("D", 80, 8, 220))));
        }
    }
    assert_eq!(cases.len(), 56);
}

fn append_numeric_edges(cases: &mut Vec<ComparisonCase>) {
    let fixtures = [
        ("ROR", "C199", 0, "word_C199"),
        ("RCLP", "C199", -1, "word_C198_negative"),
        ("DROR", "C197", 1, "wide_C198_pair"),
        ("DRCLP", "C200", -1, "wide_C199_low_only"),
        ("DRCR", "C199", 1, "wide_C200_natural"),
        ("DROLP", "C255", 0, "wide_C255_natural"),
        ("RCRP", "T255", 0, "word_T255"),
        ("DROL", "T255", -1, "wide_T254_pair"),
    ];
    for (op, base, offset, label) in fixtures {
        let seed = format!(
            "{}\n{}\nMOV H8001 C199\nDMOV H80000001 C200\nDMOV H5AA5A55A C201\nDMOV H80000001 C254\nDMOV H5AA5A55A C255\nMOV K{offset} F2\nMOV H7BBB E2\nMOV H6A5A E3\nSET M1067\nSET M1068",
            seed_words("C", 196, 3),
            seed_words("T", 252, 4)
        );
        cases.push(scenario(&format!("numeric_edge_{op}_{label}"), &seed, &format!("{op} {base}F2 H1"), &format!("{}\n{}\nDMOV C200 D230\nDMOV C201 D232\nDMOV C254 D234\nDMOV C255 D236\nMOV F2 D250\nMOV E3 D251", read_words("C", 196, 4, 220), read_words("T", 252, 4, 224))));
    }
    assert_eq!(cases.len(), 64);
}

fn append_control(cases: &mut Vec<ComparisonCase>) {
    for (slot, op) in FORMS.into_iter().enumerate() {
        let wide = op.starts_with('D');
        let m = mov(wide);
        let input = if wide { "H80000001" } else { "H8001" };
        let seed = format!(
            "{}\n{m} {input} D161\n{m} {input} D165\nMOV K1 E1\nMOV H6A5A F1\nMOV K0 D60",
            seed_words("D", 159, 10)
        );
        let kind = (slot / 2 + slot % 2) % 4;
        let (label, body) = match kind {
            0 => (
                "for_same_pc",
                format!("LD X0\nFOR K3\nLD X0\n{op} D160E1 K1\nNEXT\nLDI X7\nINC D60"),
            ),
            1 => (
                "mps_live_offset",
                format!(
                    "LD X0\nMPS\n{op} D160E1 K1\nMRD\nMOV K5 E1\nMRD\n{op} D160E1 K1\nMPP\nOUT Y10\nLDI X7\nMOV K1 E1\nINC D60"
                ),
            ),
            2 => (
                "cj_skipped_visit",
                format!("LD X2\nCJ P10\nLD X0\n{op} D160E1 K1\nP10\nLDI X7\nINC D60"),
            ),
            _ => (
                "master_false_true_body",
                format!("LD X7\nMC N0\nLD X0\n{op} D160E1 K1\nMCR N0\nLDI X7\nINC D60"),
            ),
        };
        let mut entry = scenario(
            &format!("control_{label}_{op}"),
            &seed,
            &body,
            &format!(
                "{}\nMOV D60 D250\nMOV E1 D251",
                read_words("D", 159, 10, 220)
            ),
        );
        if kind == 2 {
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
    assert_eq!(cases.len(), 80);
}

fn append_steps(cases: &mut Vec<ComparisonCase>) {
    for (op, active) in [
        ("ROR", false),
        ("RCLP", true),
        ("DRCR", true),
        ("DROLP", false),
    ] {
        let seed = format!(
            "ZRST S0 S1023\n{} S0\nRST M1040\nRST M1045\nRST M1047\n{}\nMOV K1 E1\nMOV H6A5A F1\nMOV K0 D60",
            if active { "SET" } else { "RST" },
            seed_words("D", 159, 5)
        );
        // Inactive STL clears master and output context; LDI alone cannot
        // restore the master gate. No illegal MCR-inside-STL workaround.
        let body = format!("STL S0\nLDI X7\n{op} D160E1 K1\nRET\nLDI X7\nINC D60");
        cases.push(scenario(
            &format!(
                "control_{}_step_{op}",
                if active { "active" } else { "inactive" }
            ),
            &seed,
            &body,
            &format!(
                "{}\nMOV K1S0 D240\nMOV D60 D250",
                read_words("D", 159, 5, 220)
            ),
        ));
    }
    assert_eq!(cases.len(), 84);
}

pub(super) fn model_cases(target: Target) -> Vec<ComparisonCase> {
    let last: u16 = match target {
        Target::Es2 => 9999,
        Target::Ss2 => 4999,
    };
    let mut cases = Vec::new();
    for op in FORMS {
        let wide = op.starts_with('D');
        let bits = if wide { 32u16 } else { 16 };
        let m = mov(wide);
        let mut seed = vec![format!(
            "{}\n{m} {} D{}\nMOV K{} E1\nMOV H6A5A F1\nMOV K-{} E2\nMOV H1234 F2",
            seed_words("D", last - 2, 3),
            if wide { "H80000001" } else { "H8001" },
            last - u16::from(wide),
            if wide { -1 } else { 0 },
            bits - 1
        )];
        let mut body = vec![format!("LD X0\n{op} D{last}E1 K1\nLD M1022\nOUT M700")];
        let mut observe = vec![read_words("D", last - 2, 3, 200)];
        for (slot, (area, end)) in [("Y", 255u16), ("M", 4095), ("S", 1023)]
            .into_iter()
            .enumerate()
        {
            seed.push(seed_bits(area, end - 35, 36));
            body.push(format!(
                "LD X0\n{op} K{}{}E2 K1\nLD M1022\nOUT M{}",
                bits / 4,
                address(area, end),
                701 + slot
            ));
            observe.push(read_bits(area, end - 35, 36, 220 + slot as u16 * 4));
        }
        observe.push("MOV K1M700 D248\nMOV E1 D250\nMOV E2 D251".into());
        cases.push(scenario(
            &format!(
                "model_{}_negative_offset_rescues_last_pair_and_groups_{op}",
                target.name()
            ),
            &seed.join("\n"),
            &body.join("\n"),
            &observe.join("\n"),
        ));
    }
    check_cases(&cases, 16);
    cases
}
