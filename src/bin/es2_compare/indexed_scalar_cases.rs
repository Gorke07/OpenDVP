//! Bounded scalar E/F address modifiers, distinct from direct E/F values.
//! Each fixture includes OFF / ON twice / OFF / ON visits. Constructing the
//! corpus is not a native result; the parent runner compares every snapshot.
use super::{Case, case};
use std::collections::BTreeSet;

const FAMILIES: [(&str, &str); 10] = [
    ("MOV", "DMOV"),
    ("ADD", "DADD"),
    ("SUB", "DSUB"),
    ("INC", "DINC"),
    ("DEC", "DDEC"),
    ("CML", "DCML"),
    ("NEG", "DNEG"),
    ("WAND", "DAND"),
    ("WOR", "DOR"),
    ("WXOR", "DXOR"),
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

fn index_base(index: u16) -> u16 {
    if index == 0 { 1028 } else { 1180 + index * 2 }
}

fn seed_words(area: &str, base: u16, count: u16, salt: u16) -> String {
    (0..count)
        .map(|i| {
            let value = salt.wrapping_add(i.wrapping_mul(0x1111));
            format!("MOV H{value:04X} {area}{}", base + i)
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

fn seed_bits(area: &str, base: u16, bits: u16, value: u64) -> String {
    (0..bits)
        .map(|bit| {
            format!(
                "{} {}",
                if value & (1u64 << bit) != 0 {
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

fn operation(op: &str, source: &str, rhs: &str, dest: &str) -> String {
    match op.trim_end_matches('P') {
        "INC" | "DINC" | "DEC" | "DDEC" | "NEG" | "DNEG" => {
            format!("{op} {dest}")
        }
        "MOV" | "DMOV" | "CML" | "DCML" => format!("{op} {source} {dest}"),
        _ => format!("{op} {source} {rhs} {dest}"),
    }
}

fn scenario(name: &str, seed: &str, body: &str, observe: &str) -> Case {
    let source = [
        "LD X1\nMOV H55AA D1028\nMOV H1234 D1029",
        seed,
        "SET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\nLD X0",
        body,
        "LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24",
        "LD X0\nORI X0\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305",
        observe,
        "END",
    ]
    .into_iter()
    .filter(|section| !section.is_empty())
    .collect::<Vec<_>>()
    .join("\n");
    assert!(
        source.lines().count() <= 512,
        "oversized indexed case {name}"
    );
    case(&format!("indexed_scalar_{name}"), &source, stages())
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut forms = BTreeSet::new();

    // Every spelling gets ordinary D and grouped operand lifecycles. MOV
    // changes the offsets after the action without overwriting its flags.
    for (family, (word, double)) in FAMILIES.into_iter().enumerate() {
        for wide in [false, true] {
            for pulse in [false, true] {
                let op = format!(
                    "{}{}",
                    if wide { double } else { word },
                    if pulse { "P" } else { "" }
                );
                forms.insert(op.clone());
                let seed = format!(
                    "{}\n{}\n{}\nMOV K1 E1\nMOV H6A5A F1\nMOV H55AA E2\nMOV K-3 F2\nMOV K-1 E3\nMOV H3333 F3",
                    seed_words("D", 31, 11, 0x1357),
                    seed_words("D", 44, 10, 0x2468),
                    seed_words("D", 123, 13, 0xa55a),
                );
                let body = format!(
                    "{}\nMOV K2 E1\nMOV K-2 F2\nMOV K-2 E3",
                    operation(&op, "D32E1", "D48E3", "D128F2")
                );
                let observe = format!(
                    "{}\n{}\n{}\nMOV E1 D260\nMOV F1 D261\nMOV E2 D262\nMOV F2 D263\nMOV E3 D264\nMOV F3 D265",
                    read_words("D", 31, 11, 200),
                    read_words("D", 44, 10, 212),
                    read_words("D", 123, 13, 224),
                );
                cases.push(scenario(
                    &format!("lifecycle_{op}_D"),
                    &seed,
                    &body,
                    &observe,
                ));

                let ordinal = family * 4 + usize::from(wide) * 2 + usize::from(pulse);
                let area = ["Y", "M", "S"][ordinal % 3];
                let source_area = ["Y", "M", "S"][(ordinal + 1) % 3];
                let width_max = if wide { 8 } else { 4 };
                let width_index = family * 2 + usize::from(pulse);
                let source_digits = width_index % width_max + 1;
                let target_digits = (width_index + 2) % width_max + 1;
                let source = format!("K{source_digits}{}E1", address(source_area, 64));
                let target = format!("K{target_digits}{}F2", address(area, 160));
                let seed = format!(
                    "{}\n{}\n{}\nMOV K1 E1\nMOV H6A5A F1\nMOV H55AA E2\nMOV K-3 F2\nMOV K-1 E3\nMOV H3333 F3",
                    seed_bits(source_area, 62, 40, 0x5a_1234_abcd),
                    seed_bits(area, 154, 48, 0xa55a_3cc3_9696),
                    seed_words("D", 44, 10, 0x2468),
                );
                let body = format!(
                    "{}\nMOV K2 E1\nMOV K-2 F2\nMOV K-2 E3",
                    operation(&op, &source, "D48E3", &target)
                );
                let observe = format!(
                    "DMOV K8{} D200\nMOV K2{} D202\nDMOV K8{} D210\nMOV K4{} D212\nMOV E1 D220\nMOV F1 D221\nMOV E2 D222\nMOV F2 D223\nMOV E3 D224\nMOV F3 D225",
                    address(source_area, 62),
                    address(source_area, 94),
                    address(area, 154),
                    address(area, 186),
                );
                cases.push(scenario(
                    &format!(
                        "lifecycle_{op}_K{source_digits}_{source_area}_to_K{target_digits}_{area}"
                    ),
                    &seed,
                    &body,
                    &observe,
                ));
            }
        }
    }
    assert_eq!(forms.len(), 40);
    assert_eq!(cases.len(), 80);

    // All16 index suffixes at both scalar widths and both pulse modes. The
    // unused companion word is deliberately nonzero: wide data still uses a
    // signed16-bit offset, including F0..F7 as an address modifier.
    for bank in ['E', 'F'] {
        for index in 0..8u16 {
            let base = index_base(index);
            let delta = i32::from(index % 5) - 2;
            let modifier = format!("{bank}{index}");
            let alias = base + u16::from(bank == 'F');
            let other = base + u16::from(bank == 'E');
            for op in ["MOV", "MOVP", "DMOV", "DMOVP"] {
                let seed = format!(
                    "{}\n{}\nMOV H6A5A D{other}\nMOV K{delta} D{alias}",
                    seed_words("D", 76, 15, 0x8123),
                    seed_words("D", 155, 17, 0x5aa5),
                );
                let body = format!(
                    "{op} D80{modifier} D160{modifier}\nMOV K{} {modifier}",
                    delta + 1
                );
                let observe = format!(
                    "{}\n{}\nMOV E{index} D250\nMOV F{index} D251\nMOV D{base} D252\nMOV D{} D253",
                    read_words("D", 76, 15, 200),
                    read_words("D", 155, 17, 220),
                    base + 1,
                );
                cases.push(scenario(
                    &format!("all_indices_{op}_{modifier}"),
                    &seed,
                    &body,
                    &observe,
                ));
            }
        }
    }
    assert_eq!(cases.len(), 144);

    // Timer and low-counter offsets count registers, not bytes or wide pairs.
    for op in ["MOV", "MOVP", "DMOV", "DMOVP"] {
        for (source_area, dest_area) in [("T", "C"), ("C", "T")] {
            let seed = format!(
                "{}\n{}\nMOV K-1 E4\nMOV H2345 F4\nMOV H4567 E5\nMOV K1 F5",
                seed_words(source_area, 18, 10, 0x7135),
                seed_words(dest_area, 38, 11, 0x5aa5),
            );
            let body = format!("{op} {source_area}20E4 {dest_area}40F5\nMOV K1 E4\nMOV K2 F5");
            let observe = format!(
                "{}\n{}\nMOV E4 D250\nMOV F4 D251\nMOV E5 D252\nMOV F5 D253",
                read_words(source_area, 18, 10, 200),
                read_words(dest_area, 38, 11, 220),
            );
            cases.push(scenario(
                &format!("timer_counter_{op}_{source_area}_to_{dest_area}"),
                &seed,
                &body,
                &observe,
            ));
        }
    }
    assert_eq!(cases.len(), 152);

    for op in ["MOV", "MOVP", "DMOV", "DMOVP"] {
        let digits = if op.starts_with('D') { 8 } else { 4 };
        let seed = format!(
            "{}\nMOV K-2 E7\nMOV K1 F7",
            seed_bits("S", 154, 48, 0xa55a_3cc3_9696)
        );
        let body = format!("{op} K{digits}X40F7 K{digits}S160E7\nMOV K-1 E7\nMOV K2 F7");
        let mut item = scenario(
            &format!("input_group_{op}_F7_offset_in_bits"),
            &seed,
            &body,
            "DMOV K8X40 D200\nMOV K2X100 D202\nDMOV K8S154 D210\nMOV K4S186 D212\nMOV E7 D220\nMOV F7 D221",
        );
        item.stages[0].0.extend([
            ("X41", true),
            ("X43", true),
            ("X47", true),
            ("X50", true),
            ("X57", true),
            ("X60", true),
            ("X73", true),
            ("X100", true),
        ]);
        cases.push(item);
    }
    assert_eq!(cases.len(), 156);

    // The first indexed write changes the modifier consumed by the next
    // action. Resetting it before the tested P action exposes its held state.
    // Only canonical E/F alias words are written, including the E0/F0 pair.
    for modifier in ["E0", "F0", "E7", "F7"] {
        let index = if modifier.ends_with('0') { 0 } else { 7 };
        let alias = index_base(index) + u16::from(modifier.starts_with('F'));
        for op in ["MOV", "MOVP"] {
            let seed = format!(
                "{}\nMOV H5AA5 D160\nMOV H6A5A E{index}\nMOV H3CC3 F{index}\nMOV K1 {modifier}",
                seed_words("D", 78, 7, 0x1248)
            );
            let body = format!(
                "MOV K1 {modifier}\n{op} K2 D{}{modifier}\nMOV D80{modifier} D160",
                alias - 1,
            );
            let observe = format!(
                "MOV D160 D200\nMOV {modifier} D201\nMOV D{alias} D202\nMOV E{index} D203\nMOV F{index} D204\n{}",
                read_words("D", 78, 7, 210)
            );
            cases.push(scenario(
                &format!("live_modifier_target_alias_{op}_{modifier}"),
                &seed,
                &body,
                &observe,
            ));
        }
    }
    assert_eq!(cases.len(), 164);

    for op in ["DMOV", "DMOVP"] {
        cases.push(scenario(
            &format!("wide_D5F0_uses_word_offset_{op}"),
            &format!(
                "{}\n{}\nMOV K2 E0\nMOV K-1 F0",
                seed_words("D", 3, 8, 0x89ab),
                seed_words("D", 18, 10, 0x5aa5),
            ),
            &format!("{op} D5F0 D20E0\nMOV K0 F0\nMOV K1 E0"),
            &format!(
                "{}\n{}",
                read_words("D", 3, 8, 200),
                read_words("D", 18, 10, 220)
            ),
        ));
    }

    for op in ["MOV", "DMOV"] {
        cases.push(scenario(
            &format!("same_rung_three_offsets_{op}"),
            &format!(
                "{}\n{}\nMOV K0 E1\nMOV H7AAA F1",
                seed_words("D", 76, 12, 0x8a12),
                seed_words("D", 159, 9, 0x5aa5)
            ),
            &format!("MOV K1 E1\n{op} D79E1 D160\nMOV K3 E1\n{op} D79E1 D162\nMOV K-1 E1\n{op} D79E1 D164"),
            &format!(
                "{}\nMOV E1 D240\nMOV F1 D241",
                read_words("D", 159, 9, 220)
            ),
        ));
    }

    for op in ["MOV", "MOVP"] {
        cases.push(scenario(
            &format!("source_is_index_target_is_other_index_{op}"),
            &format!(
                "{}\nMOV K1 E1\nMOV K4 F1\nMOV H5AA5 D160",
                seed_words("D", 79, 8, 0x1357)
            ),
            &format!("MOV K4 F1\n{op} D1181E1 D1182E1\nMOV D80F1 D160"),
            "MOV E1 D200\nMOV F1 D201\nMOV D1182 D202\nMOV D1183 D203\nMOV D160 D204",
        ));
    }
    assert_eq!(cases.len(), 170);

    // Natural32 counters stay wholly above C200; no storage-region crossing.
    for op in ["DMOV", "DMOVP"] {
        let seed_counters = (207..227)
            .map(|i| format!("DMOV H{:08X} C{i}", 0x1234_0000u32 + i * 0x1111))
            .collect::<Vec<_>>()
            .join("\n");
        let read_counters = (207..227)
            .map(|i| format!("DMOV C{i} D{}", 200 + (i - 207) * 2))
            .collect::<Vec<_>>()
            .join("\n");
        cases.push(scenario(
            &format!("natural_counter_{op}_register_stride"),
            &format!("{seed_counters}\nMOV K-1 E4\nMOV H1234 F4\nMOV H5678 E5\nMOV K1 F5"),
            &format!("{op} C210E4 C220F5\nMOV K1 E4\nMOV K2 F5"),
            &format!("{read_counters}\nMOV E4 D250\nMOV F4 D251\nMOV E5 D252\nMOV F5 D253"),
        ));
    }

    // Indexed grouped S2 is an independent binary operand, with a different
    // signed offset and a destination group that has its own live suffix.
    for op in ["ADD", "ADDP", "DADD", "DADDP"] {
        let digits = if op.starts_with('D') { 8 } else { 4 };
        cases.push(scenario(
            &format!("binary_second_group_{op}"),
            &format!(
                "{}\n{}\n{}\nMOV K1 E1\nMOV H4567 F1\nMOV H6789 E2\nMOV K-1 F2\nMOV K-2 E3\nMOV H2345 F3",
                seed_words("D", 31, 8, 0x1357),
                seed_bits("M", 62, 40, 0x5a_1234_abcd),
                seed_bits("S", 154, 48, 0xa55a_3cc3_9696),
            ),
            &format!("{op} D32E1 K{digits}M64F2 K{digits}S160E3\nMOV K2 E1\nMOV K1 F2\nMOV K-1 E3"),
            "DMOV K8M62 D200\nMOV K2M94 D202\nDMOV K8S154 D210\nMOV K4S186 D212\nMOV E1 D220\nMOV F2 D221\nMOV E3 D222",
        ));
    }

    assert_eq!(cases.len(), 176);
    assert_eq!(
        cases
            .iter()
            .map(|item| &item.name)
            .collect::<BTreeSet<_>>()
            .len(),
        cases.len(),
        "duplicate indexed scalar case name"
    );
    cases
}
