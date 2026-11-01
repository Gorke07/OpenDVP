//! Positive, bounded SEGD address-suffix corpus draft. Ordinary Kn/direct E/F
//! roles are regression coverage. Native execution/qualification is parent-owned.
//! M840..847 are independent literal/guard markers and must remain TRUE at
//! every recorded stage; they never use the production segment lookup seam.
use super::{Case, Target, case};
use std::collections::BTreeSet;

const INPUTS: [&str; 32] = [
    "X100", "X101", "X102", "X103", "X104", "X105", "X106", "X107", "X110", "X111", "X112", "X113",
    "X114", "X115", "X116", "X117", "X120", "X121", "X122", "X123", "X124", "X125", "X126", "X127",
    "X130", "X131", "X132", "X133", "X134", "X135", "X136", "X137",
];
const FORMS: [&str; 2] = ["SEGD", "SEGDP"];
// Literal audit/manual oracle; deliberately independent of segment_result.
const SEGMENTS: [u16; 16] = [
    0x3f, 0x06, 0x5b, 0x4f, 0x66, 0x6d, 0x7d, 0x27, 0x7f, 0x6f, 0x77, 0x7c, 0x39, 0x5e, 0x79, 0x71,
];

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    let mut seed = vec![("X1", true), ("X0", false), ("X2", false), ("X7", false)];
    for (offset, name) in INPUTS.into_iter().enumerate() {
        // Source X101..X120 is an independently declared process input ABCAh.
        let value = if (1..=16).contains(&offset) {
            0xabcau16 & (1 << (offset - 1)) != 0
        } else {
            matches!(offset, 0 | 17)
        };
        seed.push((name, value));
    }
    vec![
        (seed, 1),
        (vec![("X1", false), ("X0", true), ("X2", true)], 2),
        (vec![("X0", false), ("X2", false)], 1),
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
fn alias(bank: char, index: u16) -> u16 {
    (if index == 0 { 1028 } else { 1180 + 2 * index }) + u16::from(bank == 'F')
}
fn expected_gate(op: &str) -> &'static str {
    if op.ends_with('P') {
        "LD= D310 K2\nOR= D310 K5"
    } else {
        "LD X0"
    }
}
fn expected(op: &str, stores: &str) -> String {
    format!("{}\n{stores}", expected_gate(op))
}
fn equal(left: &str, right: &str) -> String {
    format!("LD= {left} {right}")
}
fn group_mask(digits: u16) -> u16 {
    if digits == 4 {
        u16::MAX
    } else {
        (1u16 << (digits * 4)) - 1
    }
}
fn indexed(raw: &str) -> bool {
    !raw.starts_with(['H', 'E', 'F'])
        && raw
            .char_indices()
            .skip(1)
            .any(|(_, c)| matches!(c, 'E' | 'F'))
}

fn scenario(
    name: &str,
    seed: &str,
    body: &str,
    observe: &str,
    checks: Vec<String>,
    flag_check: Option<&str>,
) -> Case {
    assert!(
        checks.len() <= 5,
        "at most five independent value/guard checks"
    );
    let mut oracles = Vec::new();
    for slot in 0..5 {
        oracles.push(format!(
            "{}\nOUT M{}",
            checks.get(slot).map(String::as_str).unwrap_or("LDI X7"),
            840 + slot
        ));
    }
    oracles.push(format!(
        "{}\nOUT M845",
        flag_check.unwrap_or("LD M1020\nANI M1021\nAND M1022")
    ));
    oracles.push("LDI M1067\nANI M1068\nAND= D1067 H1234\nAND= D1068 K77\nOUT M846".into());
    oracles.push("LD= D306 K1234\nOUT M847".into());
    let source = [
        "LD X1\nMOV K0 D310\nMOV K0 E0\nMOV K0 F0\nMOV K0 E1\nMOV K0 F1\nMOV K0 E2\nMOV K0 F7\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV H1234 D1067\nMOV K77 D1068\nMOV K0 D280\nMOV K0 D281\nMOV K0 D282\nMOV K0 D283",
        seed,
        "LDI X7\nINC D310",
        body,
        "LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24\nLDI X7\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305\nMOV K1234 D306\nMOV D310 D307",
        observe,
        &oracles.join("\n"),
        "LDI X7\nMOV K2M840 D308\nEND",
    ].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n");
    assert!(source.lines().count() <= 192, "source budget: {name}");
    // Every instruction in this draft is <=9 original PLC WORDs. This bound
    // is conservative; parent admission measures the actual encoded length.
    assert!(source.lines().count() * 9 <= 1728, "WORD bound: {name}");
    assert!(
        source.lines().count() * 9 < 0x1ef0,
        "installed budget: {name}"
    );
    let mut has_indexed_segment = false;
    for line in source.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.first().is_some_and(|op| FORMS.contains(op)) {
            assert_eq!(fields.len(), 3, "{name}: {line}");
            has_indexed_segment |= indexed(fields[1]) || indexed(fields[2]);
        }
    }
    assert!(has_indexed_segment, "missing indexed SEGD visit: {name}");
    case(&format!("indexed_segment_{name}"), &source, stages())
}
fn checked(cases: Vec<Case>, count: usize) -> Vec<Case> {
    assert_eq!(cases.len(), count);
    assert_eq!(
        cases.iter().map(|c| &c.name).collect::<BTreeSet<_>>().len(),
        count
    );
    for form in FORMS {
        assert!(
            cases.iter().any(|c| c
                .source
                .lines()
                .any(|line| line.split_whitespace().next() == Some(form))),
            "missing {form}"
        );
    }
    cases
}

pub(super) fn common_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    append_banks(&mut cases); // 16
    append_group_sources(&mut cases); // 16
    append_group_targets(&mut cases); // 12
    append_white_roles(&mut cases); // 4
    append_numeric_ends(&mut cases); // 4
    append_aliases_flags_pulse(&mut cases); // 8
    append_control(&mut cases); // 4
    checked(cases, 64)
}

fn append_banks(cases: &mut Vec<Case>) {
    for bank in ['E', 'F'] {
        for index in 0..8u16 {
            let modifier = format!("{bank}{index}");
            let ordinal = index + if bank == 'F' { 8 } else { 0 };
            let area = ["D", "T", "C"][usize::from(ordinal % 3)];
            let offset: i16 = if index % 2 == 0 { -1 } else { 1 };
            let source = (100 + offset) as u16;
            let target = (140 + offset) as u16;
            let op = FORMS[usize::from(ordinal % 2)];
            let source_value = 0xfff0 | ordinal;
            let other_digit = 15 - ordinal;
            let source_expected = SEGMENTS[usize::from(ordinal)];
            let target_expected = SEGMENTS[usize::from(other_digit)];
            let seed = format!(
                "MOV K{offset} {modifier}\nMOV H6A5A D{}\nMOV H{source_value:04X} {area}{source}\nMOV H55AA {area}{}\nMOV HAA55 {area}{target}\nMOV H1234 {area}{}\nMOV K0 D200\nMOV HAA55 D281",
                alias(bank, index) + 1,
                target - 1,
                target + 1
            );
            let body = format!(
                "LD X0\n{op} {area}100{modifier} D200\n{op} K{other_digit} {area}140{modifier}\n{}",
                expected(
                    op,
                    &format!("MOV H{source_expected:02X} D280\nMOV H{target_expected:02X} D281")
                )
            );
            let observe = format!(
                "MOV {area}{target} D201\nMOV {area}{} D202\nMOV {area}{} D203\nMOV {modifier} D204\nMOV D{} D205",
                target - 1,
                target + 1,
                alias(bank, index) + 1
            );
            cases.push(scenario(
                &format!("bank_{modifier}_{area}_{op}"),
                &seed,
                &body,
                &observe,
                vec![
                    equal("D200", "D280"),
                    equal("D201", "D281"),
                    equal("D202", "H55AA"),
                    equal("D203", "H1234"),
                    format!("LD= D204 K{offset}\nAND= D205 H6A5A"),
                ],
                None,
            ));
        }
    }
}

fn append_group_sources(cases: &mut Vec<Case>) {
    for (area_slot, area) in ["X", "Y", "M", "S"].into_iter().enumerate() {
        for digits in 1..=4u16 {
            let base = if area == "X" { 64 } else { 128 };
            let original = address(area, base);
            let effective = address(area, base + 1);
            let op = FORMS[(area_slot + usize::from(digits)) % 2];
            let bits = 0xabca & group_mask(digits);
            let mut seed = "MOV K1 E1\nMOV K0 D200".to_owned();
            if area != "X" {
                seed.push_str(&format!("\nMOV HABCA K{digits}{effective}"));
            }
            let body = format!(
                "LD X0\n{op} K{digits}{original}E1 D200\n{}",
                expected(op, "MOV H77 D280")
            );
            cases.push(scenario(
                &format!("source_{area}_k{digits}_{op}"),
                &seed,
                &body,
                &format!("MOV K{digits}{effective} D201"),
                vec![
                    equal("D200", "D280"),
                    equal("D201", &format!("H{bits:04X}")),
                ],
                None,
            ));
        }
    }
}

fn append_group_targets(cases: &mut Vec<Case>) {
    for (area_slot, area) in ["Y", "M", "S"].into_iter().enumerate() {
        for digits in 1..=4u16 {
            let original = address(area, 128);
            let effective = address(area, 129);
            let after = address(area, 129 + 4 * digits);
            let op = FORMS[(area_slot + usize::from(digits)) % 2];
            let mask = group_mask(digits);
            let result = if digits == 1 { "HF" } else { "H3F" };
            let seed = format!(
                "MOV K1 F1\nMOV HFFFF K{digits}{effective}\nSET {original}\nSET {after}\nMOV H{mask:04X} D280"
            );
            let body = format!(
                "LD X0\n{op} K0 K{digits}{original}F1\n{}",
                expected(op, &format!("MOV {result} D280"))
            );
            cases.push(scenario(
                &format!("target_{area}_k{digits}_{op}"),
                &seed,
                &body,
                &format!("MOV K{digits}{effective} D200"),
                vec![equal("D200", "D280"), format!("LD {original}\nAND {after}")],
                None,
            ));
        }
    }
}

fn append_white_roles(cases: &mut Vec<Case>) {
    for (bank, op) in [(0, "SEGD"), (7, "SEGDP")] {
        let seed = format!(
            "MOV K6 E{bank}\nMOV K9 F{bank}\nMOV K0 E1\nMOV K0 D200\nMOV K0 D201\nMOV K0 D202\nMOV K0 D203"
        );
        let body = format!(
            "LD X0\n{op} E{bank} D200E1\n{op} F{bank} D201E1\n{op} K-1 D202E1\n{op} HFFF7 D203E1\n{}",
            expected(op, "MOV H7D D280\nMOV H6F D281\nMOV H71 D282\nMOV H27 D283")
        );
        cases.push(scenario(
            &format!("white_source_pair_{bank}_{op}"),
            &seed,
            &body,
            "",
            vec![
                equal("D200", "D280"),
                equal("D201", "D281"),
                equal("D202", "D282"),
                equal("D203", "D283"),
            ],
            None,
        ));
    }
    for (bank, op) in [(1, "SEGD"), (7, "SEGDP")] {
        let low = alias('E', bank);
        let seed = format!(
            "MOV K0 E3\nMOV HFFF7 D10\nMOV HFFFA D11\nMOV H1111 E{bank}\nMOV H2222 F{bank}\nMOV H5555 D{}\nMOV H6666 D{}\nMOV H1111 D280\nMOV H2222 D281",
            low - 1,
            low + 2
        );
        let body = format!(
            "LD X0\n{op} D10E3 E{bank}\n{op} D11E3 F{bank}\n{}",
            expected(op, "MOV H27 D280\nMOV H77 D281")
        );
        let observe = format!(
            "MOV E{bank} D200\nMOV F{bank} D201\nMOV D{} D202\nMOV D{} D203",
            low - 1,
            low + 2
        );
        cases.push(scenario(
            &format!("white_target_pair_{bank}_{op}"),
            &seed,
            &body,
            &observe,
            vec![
                equal("D200", "D280"),
                equal("D201", "D281"),
                equal("D202", "H5555"),
                equal("D203", "H6666"),
            ],
            None,
        ));
    }
}

fn append_numeric_ends(cases: &mut Vec<Case>) {
    for (area, last) in [("T", 255), ("C", 199)] {
        for op in FORMS {
            let mut seed = format!(
                "MOV K0 E1\nMOV K0 F7\nMOV HFFF7 {area}{last}\nMOV H55AA {area}{}\nMOV K0 D200\nMOV HFFF7 D281",
                last - 1
            );
            if area == "C" {
                seed.push_str("\nDMOV H12345678 C200");
            }
            let body = format!(
                "LD X0\n{op} {area}{last}E1 D200\n{op} K10 {area}{last}F7\n{}",
                expected(op, "MOV H27 D280\nMOV H77 D281")
            );
            let mut observe = format!("MOV {area}{last} D201\nMOV {area}{} D202", last - 1);
            let mut checks = vec![
                equal("D200", "D280"),
                equal("D201", "D281"),
                equal("D202", "H55AA"),
            ];
            if area == "C" {
                observe.push_str("\nDMOV C200 D204");
                checks.push("DLD= D204 H12345678".into());
            }
            cases.push(scenario(
                &format!("word_endpoint_{area}{last}_{op}"),
                &seed,
                &body,
                &observe,
                checks,
                None,
            ));
        }
    }
}

fn append_aliases_flags_pulse(cases: &mut Vec<Case>) {
    for (modifier, base, source, result) in
        [("E1", 1182, "HFFF7", "H27"), ("F7", 1195, "HFFFA", "H77")]
    {
        let seed = format!("MOV H6543 D{}", base + 1);
        // Refresh outside the X0 gate: all conservative OFF/held candidates
        // remain valid after a destination overwrites its own offset WORD.
        let body = format!(
            "LDI X7\nMOV K0 {modifier}\nMOV K0 D280\nLD X0\nSEGD {source} D{base}{modifier}\nMOV {result} D280"
        );
        cases.push(scenario(
            &format!("own_index_target_{modifier}"),
            &seed,
            &body,
            &format!("MOV D{base} D200\nMOV D{} D201", base + 1),
            vec![equal("D200", "D280"), equal("D201", "H6543")],
            None,
        ));
    }
    cases.push(scenario("numeric_source_target_overlap",
        "MOV K1 E1\nMOV K0 F1",
        "LDI X7\nMOV HAAAA D10\nMOV HFFF7 D11\nMOV HAAAA D280\nLD X0\nSEGD D10E1 D10F1\nMOV H27 D280",
        "MOV D10 D200\nMOV D11 D201",
        vec![equal("D200","D280"),equal("D201","HFFF7")],None));
    cases.push(scenario(
        "unaligned_group_snapshot",
        "MOV K0 E1\nMOV K1 F1\nSET M127\nSET M133",
        "LDI X7\nMOV HBA K2M128\nMOV HD D280\nLD X0\nSEGD K2M128E1 K1M128F1\nMOV K7 D280",
        "MOV K1M129 D200",
        vec![equal("D200", "D280"), "LD M127\nAND M133\nANI M128".into()],
        None,
    ));
    cases.push(scenario(
        "flag_group_target_pulse_reseed",
        "MOV K1 E1\nSET M1018",
        &format!(
            "LDI X7\nMOV K8 K1M1019\nMOV K8 D280\nLD X0\nSEGDP K7 K1M1018E1\n{}",
            expected("SEGDP", "MOV K7 D280")
        ),
        "MOV K1M1019 D200",
        vec![equal("D200", "D280"), "LD M1018".into()],
        Some("LD= D200 D280"),
    ));
    cases.push(scenario(
        "flag_group_source_snapshot",
        "MOV K0 E0\nMOV K0 F7\nMOV K0 D200",
        "LDI X7\nMOV K10 K1M1019\nLD X0\nSEGD K1M1019E0 D200F7\nMOV H77 D280",
        "MOV K1M1019 D201",
        vec![equal("D200", "D280"), equal("D201", "K10")],
        None,
    ));
    for op in FORMS {
        let held_update = if op.ends_with('P') {
            "LD= D310 K5"
        } else {
            "LD= D310 K3\nOR= D310 K5"
        };
        let body = format!(
            "LD= D310 K3\nMOV K7 D10\nMOV K1 F1\nLD X0\n{op} D10E1 D20F1\nLD= D310 K2\nMOV H6 D280\n{held_update}\nMOV H27 D281"
        );
        cases.push(scenario(&format!("live_source_target_held_rearm_{op}"),
            "MOV K0 E1\nMOV K0 F1\nMOV K1 D10\nMOV HAAAA D20\nMOV HBBBB D21\nMOV HAAAA D280\nMOV HBBBB D281",
            &body,"MOV D20 D200\nMOV D21 D201",
            vec![equal("D200","D280"),equal("D201","D281"),"LD= D10 K1\nOR= D10 K7".into()],None));
    }
}

fn append_control(cases: &mut Vec<Case>) {
    cases.push(scenario("control_master_changes",
        "MOV K0 E1\nMOV K0 F1\nMOV K0 E2\nMOV K1 D10\nMOV K0 D20\nMOV K0 D22",
        "LD X2\nMC N0\nLD X0\nSEGD D10E1 D20F1\nMCR N0\nLD X0\nSEGD HFFF7 D22E2\nMOV H27 D281\nLD X0\nAND X2\nMOV H6 D280",
        "MOV D20 D200\nMOV D22 D201",
        vec![equal("D200","D280"),equal("D201","D281")],None));
    cases.push(scenario("control_cj_skips_pulse_visits",
        "MOV K0 E1\nMOV K0 F1\nMOV K7 D10\nMOV K0 D20",
        "LD X2\nCJ P10\nLD X0\nSEGDP D10E1 D20F1\nP10\nLDI X7\nMOV K321 D205\nLD X0\nANI X2\nMOV H27 D280",
        "MOV D20 D200",
        vec![equal("D200","D280"),equal("D205","K321")],None));
    cases.push(scenario("control_for_live_source_same_pc",
        "MOV K0 E1\nMOV K0 F1\nMOV K0 D10\nMOV K0 D20",
        "LD X0\nFOR K3\nLD X0\nSEGD D10E1 D20F1\nINC D10\nNEXT\nLD= D310 K2\nMOV H5B D280\nLD= D310 K3\nMOV H6D D280\nLD= D310 K5\nMOV H7F D280",
        "MOV D20 D200\nMOV D10 D201",
        vec![equal("D200","D280"),"LD= D201 K0\nOR= D201 K3\nOR= D201 K6\nOR= D201 K9".into()],None));
    cases.push(scenario("control_mps_live_index",
        "MOV K0 E1\nMOV K0 F1\nMOV K1 D10\nMOV K7 D11\nMOV K0 D20\nMOV K0 D22",
        "LD X0\nMPS\nSEGD D10E1 D20F1\nMRD\nMOV K1 E1\nMRD\nSEGD D10E1 D22F1\nMPP\nOUT Y10\nLDI X7\nMOV K0 E1\nLD X0\nMOV H6 D280\nMOV H27 D281",
        "MOV D20 D200\nMOV D22 D201",
        vec![equal("D200","D280"),equal("D201","D281"),equal("E1","K0")],None));
}

pub(super) fn model_cases(target: Target) -> Vec<Case> {
    let last: u16 = match target {
        Target::Es2 => 9999,
        Target::Ss2 => 4999,
    };
    let mut cases = Vec::new();
    for op in FORMS {
        for negative in [false, true] {
            let offset = if negative { -1 } else { 0 };
            let effective = last - u16::from(negative);
            let source = if negative { "HFFFA" } else { "HFFF7" };
            let result = if negative { "H77" } else { "H27" };
            let tag = if negative {
                "negative_rescue"
            } else {
                "last_word"
            };
            let source_seed = format!(
                "MOV K{offset} E1\nMOV {source} D{effective}\nMOV H55AA D{}\nMOV K0 D200",
                effective - 1
            );
            let source_body = format!(
                "LD X0\n{op} D{last}E1 D200\n{}",
                expected(op, &format!("MOV {result} D280"))
            );
            cases.push(scenario(
                &format!("model_d{last}_source_{tag}_{op}"),
                &source_seed,
                &source_body,
                &format!("MOV D{effective} D201\nMOV D{} D202", effective - 1),
                vec![
                    equal("D200", "D280"),
                    equal("D201", source),
                    equal("D202", "H55AA"),
                ],
                None,
            ));
            let target_seed = format!(
                "MOV K{offset} F7\nMOV HAAAA D{effective}\nMOV H55AA D{}\nMOV HAAAA D280",
                effective - 1
            );
            let target_body = format!(
                "LD X0\n{op} {source} D{last}F7\n{}",
                expected(op, &format!("MOV {result} D280"))
            );
            cases.push(scenario(
                &format!("model_d{last}_target_{tag}_{op}"),
                &target_seed,
                &target_body,
                &format!("MOV D{effective} D200\nMOV D{} D201", effective - 1),
                vec![equal("D200", "D280"), equal("D201", "H55AA")],
                None,
            ));
        }
    }
    checked(cases, 8)
}
