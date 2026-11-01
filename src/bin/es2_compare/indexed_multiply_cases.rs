//! Bounded indexed full MUL/DIV operands and their two-result destinations.
//! Word base-C200 results are explicitly named installed native extensions:
//! the installed handler accepts them, but the vendor compiler rejects them.
//! No truncated forms, invalid-source probes, M1280..1535, or MIN32 / -1.
use super::{Case, case};
use std::collections::BTreeSet;

const FORMS: [(&str, bool); 8] = [
    ("MUL", false),
    ("MULP", false),
    ("DIV", false),
    ("DIVP", false),
    ("DMUL", true),
    ("DMULP", true),
    ("DDIV", true),
    ("DDIVP", true),
];

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (vec![("X1", true), ("X0", false), ("X7", false)], 1),
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

fn index_base(index: u16) -> u16 {
    if index == 0 { 1028 } else { 1180 + index * 2 }
}

fn seed_words(area: &str, base: u16, count: u16, salt: u16) -> String {
    (0..count)
        .map(|i| {
            format!(
                "MOV H{:04X} {area}{}",
                salt.wrapping_add(i.wrapping_mul(0x1111)),
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

fn seed_bits(area: &str, base: u16, count: u16, pattern: u64) -> String {
    assert!(matches!(area, "Y" | "M" | "S"));
    (0..count)
        .map(|i| {
            format!(
                "{} {}",
                if pattern & (1 << (i % 64)) == 0 {
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
        .map(|word| {
            let bits = (count - word * 16).min(16);
            format!(
                "MOV K{}{} D{}",
                bits / 4,
                address(area, base + word * 16),
                output + word
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn scenario(name: &str, seed: &str, body: &str, observe: &str) -> Case {
    let gate = if body.starts_with("LD") { "" } else { "LD X0" };
    let source = [
        "LD X1\nMOV H55AA E0\nMOV H1234 F0",
        seed,
        "SET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068",
        gate,
        body,
        "LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24",
        "LDI X7\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305\nMOV K1234 D306",
        observe,
        "END",
    ]
    .into_iter()
    .filter(|section| !section.is_empty())
    .collect::<Vec<_>>()
    .join("\n");
    assert!(
        source.lines().count() <= 512,
        "oversized indexed multiply {name}"
    );
    assert!(
        source.lines().any(|line| {
            let words: Vec<_> = line.split_whitespace().collect();
            words.len() == 4
                && FORMS.iter().any(|(op, _)| *op == words[0])
                && words[1..].iter().any(|operand| {
                    operand.len() > 2
                        && matches!(operand.as_bytes()[operand.len() - 2], b'E' | b'F')
                        && operand.as_bytes()[operand.len() - 1].is_ascii_digit()
                })
        }),
        "missing indexed full multiply/divide in {name}"
    );
    case(&format!("indexed_multiply_{name}"), &source, stages())
}

pub(super) fn cases() -> Vec<Case> {
    let mut cases = Vec::new();

    // All sixteen signed-word suffixes are used in every operand role at
    // both arithmetic widths. Companion E/F words are nonzero poison, and
    // normal/P variants share the same moving-address lifecycle.
    for bank in ['E', 'F'] {
        for index in 0..8u16 {
            let modifier = format!("{bank}{index}");
            let pair = index_base(index);
            let alias = pair + u16::from(bank == 'F');
            let companion = pair + u16::from(bank == 'E');
            let delta = if index % 2 == 0 { -2 } else { 1 };
            for (family, wide) in [
                ("MUL", false),
                ("DIV", false),
                ("DMUL", true),
                ("DDIV", true),
            ] {
                let pulse = (index + u16::from(bank == 'F')) % 2 == 1;
                let op = format!("{family}{}", if pulse { "P" } else { "" });
                let next_poison = if bank == 'F' && (1..7).contains(&index) {
                    // F1..F6's following word is another canonical E alias.
                    format!("MOV H6A5A D{}", alias + 1)
                } else {
                    String::new()
                };
                let seed = format!(
                    "{}\n{}\n{}\nMOV H5AA5 D{companion}\n{next_poison}\nMOV K{delta} D{alias}",
                    seed_words("D", 61, 13, 0x8123),
                    seed_words("D", 93, 13, 0x0305),
                    seed_words("D", 155, 16, 0xa55a),
                );
                let body = format!(
                    "{op} D64{modifier} D96{modifier} D160{modifier}\nMOV K{} {modifier}",
                    delta + 1
                );
                let observe = format!(
                    "{}\n{}\n{}\nMOV E{index} D260\nMOV F{index} D261\nMOV D{pair} D262\nMOV D{} D263",
                    read_words("D", 61, 13, 200),
                    read_words("D", 93, 13, 216),
                    read_words("D", 155, 16, 232),
                    pair + 1,
                );
                cases.push(scenario(
                    &format!(
                        "all_indices_{op}_{modifier}_{}",
                        if wide { "double" } else { "word" }
                    ),
                    &seed,
                    &body,
                    &observe,
                ));
            }
        }
    }
    assert_eq!(cases.len(), 64);

    // Every legal declared result width writes two adjacent chunks. Whole
    // windows capture the second chunk and guards as their live base moves.
    // Source roles alternate, including short groups whose value zero-extends.
    for (ordinal, (op, wide)) in FORMS.into_iter().enumerate() {
        let maximum = if wide { 8 } else { 4 };
        for digits in 1..=maximum {
            let source_area = ["X", "Y", "M", "S"][(ordinal + digits as usize) % 4];
            let target_area = ["Y", "M", "S"][(ordinal + digits as usize) % 3];
            let source_digits = (digits + ordinal as u16) % maximum + 1;
            let source = format!("K{source_digits}{}E1", address(source_area, 64));
            let target = format!("K{digits}{}F2", address(target_area, 160));
            let group_seed = if source_area == "X" {
                String::new()
            } else {
                seed_bits(source_area, 62, 40, 0x5a_a55a_ffff_ffef)
            };
            let seed = format!(
                "{group_seed}\n{}\n{}\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K-2 F2\nMOV K-1 E3\nMOV H2468 F3",
                seed_bits(target_area, 154, 80, 0xa55a_3cc3_9696_5aa5),
                seed_words("D", 45, 8, 0x3141),
            );
            let (lhs, rhs) = if digits % 2 == 0 {
                ("D48E3".to_owned(), source.clone())
            } else {
                (source.clone(), "D48E3".to_owned())
            };
            let body = format!("{op} {lhs} {rhs} {target}\nMOV K1 E1\nMOV K-1 F2\nMOV K0 E3");
            let observe = format!(
                "{}\n{}\n{}\nMOV E1 D260\nMOV F1 D261\nMOV E2 D262\nMOV F2 D263\nMOV E3 D264\nMOV F3 D265",
                read_bits(source_area, 62, 48, 200),
                read_bits(target_area, 154, 80, 210),
                read_words("D", 45, 8, 230),
            );
            let mut item = scenario(
                &format!(
                    "groups_{op}_K{source_digits}{source_area}_K{digits}{target_area}_two_chunks"
                ),
                &seed,
                &body,
                &observe,
            );
            if source_area == "X" {
                item.stages[0].0.extend([
                    ("X100", true),
                    ("X102", true),
                    ("X107", true),
                    ("X110", true),
                    ("X117", true),
                    ("X120", true),
                    ("X127", true),
                    ("X130", true),
                    ("X137", true),
                    ("X140", true),
                    ("X143", true),
                ]);
                item.stages[2]
                    .0
                    .extend([("X107", false), ("X110", false), ("X111", true)]);
            }
            cases.push(item);
        }
    }
    assert_eq!(cases.len(), 112);

    // Timer and low-counter addresses advance in registers, not bytes or
    // arithmetic-width pairs; both sources and the result carry modifiers.
    for (op, _) in FORMS {
        for (source_area, target_area) in [("T", "C"), ("C", "T")] {
            let seed = format!(
                "{}\n{}\n{}\nMOV K-1 E4\nMOV H5AA5 F4\nMOV H6A5A E5\nMOV K1 F5\nMOV K-2 E6\nMOV H7BBB F6",
                seed_words(source_area, 17, 12, 0x8135),
                seed_words(source_area, 31, 12, 0x1327),
                seed_words(target_area, 47, 14, 0xa55a),
            );
            let body = format!(
                "{op} {source_area}20E4 {source_area}32F5 {target_area}52E6\nMOV K1 E4\nMOV K2 F5\nMOV K-1 E6"
            );
            let observe = format!(
                "{}\n{}\n{}\nMOV E4 D260\nMOV F5 D261\nMOV E6 D262",
                read_words(source_area, 17, 12, 200),
                read_words(source_area, 31, 12, 216),
                read_words(target_area, 47, 14, 232),
            );
            cases.push(scenario(
                &format!("numeric_{op}_{source_area}_to_{target_area}"),
                &seed,
                &body,
                &observe,
            ));
        }
    }
    assert_eq!(cases.len(), 128);

    // Effective C197/198/199/200 selects the physical layout after applying
    // the suffix. Wide source C199 -> C200 also changes natural read width.
    for (op, wide) in FORMS {
        for (base, modifier) in [(198, "E1"), (200, "F2")] {
            let natural_seed = (200..206)
                .map(|c| format!("DMOV H{:08X} C{c}", 0x8123_4567u32.wrapping_add(c * 0x1111)))
                .collect::<Vec<_>>()
                .join("\n");
            let natural_read = (200..206)
                .map(|c| format!("DMOV C{c} D{}", 210 + (c - 200) * 2))
                .collect::<Vec<_>>()
                .join("\n");
            let seed = format!(
                "{}\n{natural_seed}\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K-1 F2\nMOV K0 E3\nMOV H1234 F3",
                seed_words("C", 196, 4, 0x1357),
            );
            let source_counter = if wide { 199 } else { 197 };
            let body = format!(
                "{op} C{source_counter}E3 K3 C{base}{modifier}\nMOV K0 {modifier}\nMOV K1 E3"
            );
            let observe = format!(
                "{}\n{natural_read}\nMOV E1 D260\nMOV F2 D261\nMOV E3 D262",
                read_words("C", 196, 4, 200)
            );
            let extension = if !wide && base == 200 {
                "_native_extension_base_C200"
            } else {
                ""
            };
            cases.push(scenario(
                &format!("counter_layout_{op}_C{base}_{modifier}{extension}"),
                &seed,
                &body,
                &observe,
            ));
        }
    }
    assert_eq!(cases.len(), 144);

    // The first result overwrites the target's own modifier. All written
    // system D words here are canonical E/F aliases; the second result must
    // continue from the original pointer, including in the wide F2 cases.
    for (ordinal, (op, wide)) in FORMS.into_iter().enumerate() {
        let (modifier, alias) = if wide {
            ("F2", 1185)
        } else if ordinal % 2 == 0 {
            ("E0", 1028)
        } else {
            ("E1", 1182)
        };
        let count = if wide { 4 } else { 2 };
        let mov = if wide { "DMOV" } else { "MOV" };
        let value = if wide { -700000001 } else { -30001 };
        let seed = format!(
            "{mov} K{value} D32\n{mov} K19 D48\n{}\nMOV K0 E5\nMOV H1234 F5\nMOV H5678 E6\nMOV K0 F6\nMOV K1 {modifier}",
            seed_words("D", alias, count, 0xa55a)
        );
        let body = format!(
            "LDI X7\nMOV K1 {modifier}\nLD X0\n{op} D32E5 D48F6 D{}{modifier}",
            alias - 1
        );
        let observe = format!(
            "{}\nMOV {modifier} D250\n{mov} D32 D252\n{mov} D48 D254",
            read_words("D", alias, count, 220)
        );
        cases.push(scenario(
            &format!("self_index_overwrite_{op}_{modifier}_cursor_retained"),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 152);

    // Destination begins at S2. Both complete inputs must have been read
    // before either result is written; later visits may legitimately see a
    // zero divisor produced by the previous result and must preserve data.
    for (op, _) in FORMS {
        let seed = format!(
            "{}\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K-1 F2\nMOV K1 E3\nMOV H1234 F3",
            seed_words("D", 60, 14, 0x1357)
        );
        let body = format!("{op} D64E1 D66F2 D64E3");
        let observe = format!(
            "{}\nMOV E1 D250\nMOV F2 D251\nMOV E3 D252",
            read_words("D", 60, 14, 220)
        );
        cases.push(scenario(
            &format!("source_result_overlap_{op}_snapshot_both_inputs"),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 160);

    // An otherwise end-spanning original label is legal after its negative
    // modifier moves the entire two-/four-word result inside the model.
    for (op, wide) in [
        ("MUL", false),
        ("DIVP", false),
        ("DMUL", true),
        ("DDIVP", true),
    ] {
        for (area, base, first) in [("D", 4999, 4993), ("T", 255, 249)] {
            let offset = if wide { -3 } else { -1 };
            let seed = format!(
                "{}\n{}\nMOV K{offset} E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K-1 F2",
                seed_words(area, first, 7, 0xa55a),
                seed_words("D", 61, 8, 0x8123)
            );
            let body = format!("{op} D64F2 K3 {area}{base}E1\nMOV K{} E1", offset - 1);
            let observe = format!(
                "{}\n{}\nMOV E1 D250\nMOV F2 D251",
                read_words(area, first, 7, 220),
                read_words("D", 61, 8, 240)
            );
            cases.push(scenario(
                &format!("negative_offset_rescues_end_span_{op}_{area}{base}"),
                &seed,
                &body,
                &observe,
            ));
        }
    }
    assert_eq!(cases.len(), 168);

    // Within one rung, read current native divisor data at each instruction:
    // zero + invalid target, nonzero + valid target, then zero + invalid target.
    // Only the zero-divisor instructions receive an invalid effective label.
    // Error/PC snapshots distinguish +5 word and +9 wide operand positions;
    // four independently latched initial error combinations remain visible.
    for (ordinal, (op, wide)) in [
        ("DIV", false),
        ("DIVP", false),
        ("DDIV", true),
        ("DDIVP", true),
    ]
    .into_iter()
    .enumerate()
    {
        let mov = if wide { "DMOV" } else { "MOV" };
        let value = if wide { -700000001 } else { -30001 };
        let seed = format!(
            "{mov} K{value} D32\n{mov} K0 D48\n{}\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 E3\nMOV H1234 F3\nMOV K0 D320\nMOV K0 D321\nMOV K0 D322\nMOV K0 D323\nMOV K0 D324\nMOV K0 D325",
            seed_words("D", 155, 14, 0xa55a)
        );
        let initial_errors = format!(
            "LD X1\n{} M1067\n{} M1068\nMOV H4444 D1067\nMOV K321 D1068\nLD X0",
            if ordinal & 1 == 0 { "RST" } else { "SET" },
            if ordinal & 2 == 0 { "RST" } else { "SET" }
        );
        let body = format!(
            "{initial_errors}\nMOV K30000 E1\n{mov} K0 D48\n{op} D32F2 D48E3 D160E1\nMOV D1067 D320\nMOV D1068 D321\nMOV K-1 E1\n{mov} K3 D48\n{op} D32F2 D48E3 D160E1\nMOV D1067 D322\nMOV D1068 D323\nMOV K30000 E1\n{mov} K0 D48\n{op} D32F2 D48E3 D160E1\nMOV D1067 D324\nMOV D1068 D325"
        );
        let observe = format!(
            "{}\n{}\nMOV E1 D250\n{mov} D48 D252",
            read_words("D", 155, 14, 220),
            read_words("D", 320, 6, 240)
        );
        cases.push(scenario(
            &format!("zero_then_nonzero_live_source_{op}_sticky_{ordinal}"),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 172);

    // Branch-stack reloads separate each preceding output from its next
    // condition. The result is a direct E pair while its source is indexed.
    cases.push(scenario(
        "control_MUL_mps_live_index_and_direct_E_result",
        &format!("{}\nMOV K0 E1\nMOV H1234 F1\nDMOV H5AA5A55A E2", seed_words("D", 31, 8, 0x8123)),
        "MPS\nMOV K1 E1\nMRD\nMUL D32E1 K3 E2\nMRD\nMOV K2 E1\nMRD\nMUL D32E1 K3 D160E1\nMPP\nOUT Y0",
        "DMOV E2 D220\nDMOV D162 D222\nMOV E1 D224\nMOV F1 D225",
    ));
    cases.push(scenario(
        "control_DIVP_same_pc_for_live_indices_direct_F_source",
        &format!("{}\n{}\nMOV K3 F4\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 D30", seed_words("D", 31, 10, 0x8123), seed_words("D", 159, 10, 0xa55a)),
        "LDI X7\nMOV K0 E1\nMOV K0 F2\nMOV K0 D30\nFOR K3\nLD X0\nDIVP D32E1 F4 D160F2\nLDI X7\nINC E1\nINC F2\nINC D30\nNEXT",
        &format!("{}\nMOV E1 D250\nMOV F2 D251\nMOV F4 D252\nMOV D30 D253", read_words("D", 159, 10, 220)),
    ));
    cases.push(scenario(
        "control_DMUL_cj_direct_wide_E_source",
        &format!("DMOV K-700000001 E3\nDMOV K19 D48\n{}\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 D30", seed_words("D", 157, 12, 0xa55a)),
        "LDI X0\nCJ P10\nLD X0\nDMUL E3 D48F2 D160E1\nMOV K0 E1\nP10\nLDI X7\nMOV K123 D30",
        &format!("{}\nDMOV E3 D250\nMOV E1 D252\nMOV F2 D253\nMOV D30 D254", read_words("D", 157, 12, 220)),
    ));
    cases.push(scenario(
        "control_DDIVP_mps_direct_wide_E_source",
        &format!(
            "DMOV K-700000001 E3\n{}\nMOV H7BBB E2\nMOV K0 F2",
            seed_words("D", 158, 12, 0xa55a)
        ),
        "MPS\nDDIVP E3 K19 D160F2\nMRD\nMOV K1 F2\nMPP\nOUT Y1",
        &format!(
            "{}\nDMOV E3 D250\nMOV F2 D252",
            read_words("D", 158, 12, 220)
        ),
    ));

    assert_eq!(cases.len(), 176);

    // Real X input samples choose both the divisor and its corresponding
    // safe/unused destination. Held P must skip even when X changes from
    // zero to one; the final rearmed visit reads the new native input value.
    for (op, wide) in [("DIV", false), ("DDIVP", true)] {
        let mov = if wide { "DMOV" } else { "MOV" };
        let value = if wide { -700000001 } else { -30001 };
        let seed = format!(
            "{mov} K{value} D32\n{}\nMOV K0 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K0 F2\nMOV K0 E3\nMOV H1234 F3",
            seed_words("D", 155, 14, 0xa55a),
        );
        let body = format!(
            "LD X100\nMOV K-1 E1\nLDI X100\nMOV K30000 E1\nLD X0\n{op} D32F2 K1X100E3 D160E1"
        );
        let observe = format!(
            "{}\nMOV K1X100 D250\nMOV E1 D251",
            read_words("D", 155, 14, 220)
        );
        let mut item = scenario(
            &format!("native_X_divisor_zero_to_one_{op}"),
            &seed,
            &body,
            &observe,
        );
        item.stages = vec![
            (
                vec![("X1", true), ("X0", false), ("X7", false), ("X100", false)],
                1,
            ),
            (vec![("X1", false), ("X0", true)], 1),
            (vec![("X100", true)], 1),
            (vec![("X0", false), ("X100", false)], 1),
            (vec![("X0", true), ("X100", true)], 1),
        ];
        cases.push(item);
    }

    assert_eq!(cases.len(), 178);
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
    for (op, _) in FORMS {
        assert!(
            actual.contains(op),
            "missing indexed multiply spelling {op}"
        );
    }
    cases
}
