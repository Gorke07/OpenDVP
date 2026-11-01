//! Addressed byte swaps and masked moves, with bounded deterministic inputs.
//! Low-M remapping and invalid wrapping addresses are outside this corpus.
use super::{Case as ComparisonCase, Target, case};
use std::collections::BTreeSet;

const FORMS: [&str; 8] = [
    "SWAP", "SWAPP", "DSWAP", "DSWAPP", "MVM", "MVMP", "DMVM", "DMVMP",
];
const INPUTS: [&str; 32] = [
    "X100", "X101", "X102", "X103", "X104", "X105", "X106", "X107", "X110", "X111", "X112", "X113",
    "X114", "X115", "X116", "X117", "X120", "X121", "X122", "X123", "X124", "X125", "X126", "X127",
    "X130", "X131", "X132", "X133", "X134", "X135", "X136", "X137",
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
    let gate = if body.starts_with("LD") || body.starts_with("STL") {
        ""
    } else {
        "LD X0"
    };
    let source = [
        "LD X1\nMOV H55AA E0\nMOV H1234 F0\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV H1234 D1067\nMOV K77 D1068",
        seed,gate,body,
        "LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24",
        "LDI X7\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305\nMOV K1234 D306",
        observe,"END",
    ].into_iter().filter(|part|!part.is_empty()).collect::<Vec<_>>().join("\n");
    assert!(
        source.lines().count() <= 512,
        "oversized indexed SWAP/MVM: {name}"
    );
    assert!(
        source.lines().count() * 13 <= 0x1ef0,
        "installed budget: {name}"
    );
    let mut indexed = false;
    for line in source.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.first().is_some_and(|op| FORMS.contains(op)) {
            assert_eq!(
                fields.len(),
                if fields[0].contains("SWAP") { 2 } else { 4 },
                "operand count: {name}: {line}"
            );
            indexed |= fields[1..].iter().any(|operand| {
                let b = operand.as_bytes();
                !operand.starts_with(['H', 'E', 'F'])
                    && b.len() >= 4
                    && matches!(b[b.len() - 2], b'E' | b'F')
                    && b[b.len() - 1].is_ascii_digit()
            });
        }
    }
    assert!(indexed, "missing indexed SWAP/MVM in {name}");
    case(&format!("indexed_swap_mask_{name}"), &source, stages())
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
    let actual: BTreeSet<_> = cases
        .iter()
        .flat_map(|entry| entry.source.lines())
        .filter_map(|line| line.split_whitespace().next())
        .collect();
    for op in FORMS {
        assert!(actual.contains(op), "missing {op}");
    }
}

pub(super) fn common_cases() -> Vec<ComparisonCase> {
    let mut cases = Vec::new();
    // Every E/F word is both a SWAP read/write modifier and all three MVM
    // modifiers. Numeric offsets count words; the three packed areas count bits.
    for wide in [false, true] {
        for bank in ['E', 'F'] {
            for index in 0..8u16 {
                let modifier = format!("{bank}{index}");
                let companion = index_base(index) + u16::from(bank == 'E');
                let suffix = if index % 2 == 0 { "" } else { "P" };
                let swap = format!("{}SWAP{suffix}", if wide { "D" } else { "" });
                let mvm = format!("{}MVM{suffix}", if wide { "D" } else { "" });
                let digits = if wide { 8 } else { 4 };
                let offset = if index % 2 == 0 { -1 } else { 1 };
                let seed = [
                    seed_words("D", 44, 12),
                    seed_words("D", 76, 12),
                    seed_words("D", 156, 40),
                    seed_words("T", 60, 12),
                    seed_words("C", 60, 12),
                    seed_bits("Y", 120, 48),
                    seed_bits("M", 120, 48),
                    seed_bits("S", 120, 48),
                    format!("MOV H6A5A D{companion}\nMOV K{offset} {modifier}"),
                ]
                .join("\n");
                let mut body = vec![format!(
                    "{swap} D160{modifier}\n{swap} T64{modifier}\n{swap} C64{modifier}\n{mvm} D48{modifier} D80{modifier} D160{modifier}"
                )];
                for (slot, area) in ["Y", "M", "S"].into_iter().enumerate() {
                    let group = format!("K{digits}{}{modifier}", address(area, 128));
                    body.push(format!("{swap} {group}\n{mvm} D48{modifier} D80{modifier} {group}\n{mvm} {group} {group} D{}{modifier}",180+slot*4));
                }
                body.push(format!("INC {modifier}\nOUT Y10"));
                let observe = format!(
                    "{}\n{}\n{}\n{}\n{}\n{}\n{}\nMOV {modifier} D290\nMOV D{companion} D291",
                    read_words("D", 156, 40, 200),
                    read_words("T", 60, 12, 240),
                    read_words("C", 60, 12, 252),
                    read_bits("Y", 120, 48, 270),
                    read_bits("M", 120, 48, 274),
                    read_bits("S", 120, 48, 278),
                    read_words("D", 44, 8, 280)
                );
                cases.push(scenario(
                    &format!("all_banks_{swap}_{mvm}_{modifier}"),
                    &seed,
                    &body.join("\n"),
                    &observe,
                ));
            }
        }
    }
    assert_eq!(cases.len(), 32);
    append_width_cases(&mut cases);
    append_numeric_boundaries(&mut cases);
    append_aliases(&mut cases);
    append_direct_f_regressions(&mut cases);
    append_flags_and_controls(&mut cases);
    append_ordinary_drand_vm_cases(&mut cases);
    check_cases(&cases, 94);
    cases
}

fn append_width_cases(cases: &mut Vec<ComparisonCase>) {
    for op in FORMS {
        for area in ["Y", "M", "S"] {
            let wide = op.starts_with('D');
            let max_digits = if wide { 8u16 } else { 4 };
            let mask = if wide { "HFFFFFFFF" } else { "HFFFF" };
            let is_swap = op.contains("SWAP");
            let snapshots = max_digits * if is_swap { 1 } else { 2 };
            let seed = format!(
                "{}\n{}\nDMOV H0FF0AA55 K8M64\nMOV K-1 E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K1 F2",
                seed_bits(area, 120, 48),
                seed_words("D", 80, snapshots * 3)
            );
            let mut body = Vec::new();
            for digits in 1..=max_digits {
                let reset = format!(
                    "LDI X7\nDMOV H89ABCDEF K8{}\nSET {}\nSET {}",
                    address(area, 128),
                    address(area, 127),
                    address(area, 128 + digits * 4)
                );
                if is_swap {
                    body.push(format!(
                        "{reset}\nLD X0\n{op} K{digits}{}F2\nLDI X7\n{}",
                        address(area, 127),
                        read_bits(area, 120, 48, 80 + (digits - 1) * 3)
                    ));
                } else {
                    // First preserve the source's zero extension in a full-width
                    // target; then expose a short packed mask and short target.
                    body.push(format!(
                        "{reset}\nLD X0\n{op} K{digits}X101E1 {mask} K{max_digits}{}F2\nLDI X7\n{}",
                        address(area, 127),
                        read_bits(area, 120, 48, 80 + (digits - 1) * 6)
                    ));
                    body.push(format!("{reset}\nLD X0\n{op} K{max_digits}X101E1 K{digits}M65E1 K{digits}{}F2\nLDI X7\n{}",address(area,127),read_bits(area,120,48,83+(digits-1)*6)));
                }
            }
            let observe = format!(
                "{}\n{}\nMOV E1 D280\nMOV F2 D281\nDMOV K8X100 D284\nDMOV K8M64 D286",
                read_bits(area, 120, 48, 200),
                read_words("D", 80, snapshots * 3, 220)
            );
            let mut entry = scenario(
                &format!("all_short_widths_{op}_{area}"),
                &seed,
                &body.join("\n"),
                &observe,
            );
            entry.stages[0].0.extend(
                INPUTS
                    .into_iter()
                    .enumerate()
                    .map(|(bit, input)| (input, (0xa55a33ccu32 >> bit) & 1 != 0)),
            );
            cases.push(entry);
        }
    }
    assert_eq!(cases.len(), 56);
}

fn append_numeric_boundaries(cases: &mut Vec<ComparisonCase>) {
    for op in FORMS {
        let wide = op.starts_with('D');
        let offset = if wide { -1 } else { 0 };
        let seed = format!(
            "{}\n{}\n{}\nMOV H55AA C197\nMOV H1234 C198\nMOV HABCD C199\nDMOV H12345678 C200\nDMOV H5AA53CC3 C201\nDMOV H11223344 C254\nDMOV H89ABCDEF C255\nDMOV H0FF0F00F D32\nMOV K{offset} E1\nMOV H6A5A F1\nMOV H7BBB E2\nMOV K{offset} F2\nMOV K{offset} E3\nMOV H1234 F3\nMOV K1 E4\nMOV H4567 F4",
            seed_words("D", 0, 4),
            seed_words("T", 0, 3),
            seed_words("T", 253, 3)
        );
        let body = if op.contains("SWAP") {
            if wide {
                format!(
                    "{op} T255E1\n{op} T1E1\n{op} D1E1\n{op} C199E1\n{op} C200E1\nDMOV C200 D80\n{op} C199E4\n{op} C254E4"
                )
            } else {
                format!(
                    "{op} T255E1\n{op} T0E1\n{op} D0E1\n{op} C198E1\n{op} C199E1\nDMOV C200 D80"
                )
            }
        } else if wide {
            format!(
                "{op} T255E1 C200F2 D1E3\n{op} C199E1 D33F2 C199E4\n{op} C199E4 T255F2 C200E3\nDMOV C200 D80\n{op} C254E4 D33F2 C254E4"
            )
        } else {
            format!(
                "{op} T255E1 C199F2 D0E3\n{op} C198E1 D32F2 T255E3\n{op} D32E1 T254F2 C199E3\nDMOV C200 D80"
            )
        };
        let observe = format!(
            "{}\n{}\n{}\nMOV C197 D230\nMOV C198 D231\nMOV C199 D232\nDMOV C200 D234\nDMOV C201 D236\nDMOV C254 D238\nDMOV C255 D240\nDMOV D80 D244\nMOV E1 D250\nMOV F2 D251\nMOV E3 D252\nMOV E4 D253",
            read_words("D", 0, 4, 200),
            read_words("T", 0, 3, 210),
            read_words("T", 253, 3, 220)
        );
        cases.push(scenario(
            &format!("numeric_timer_counter_boundaries_{op}"),
            &seed,
            &body,
            &observe,
        ));
    }
    assert_eq!(cases.len(), 64);
}

fn append_aliases(cases: &mut Vec<ComparisonCase>) {
    for op in FORMS {
        let (modifier, alias) = if op.ends_with('P') {
            ("F1", 1183u16)
        } else {
            ("E1", 1182)
        };
        let seed = format!(
            "MOV H55AA E1\nMOV H1234 F1\nMOV H5678 E2\nMOV K-1 F2\nMOV K0 E3\nMOV K0 F3\nMOV K1 {modifier}\nDMOV HAA55CC33 D32\nDMOV H0FF0F00F D34\nDMOV H0FF0F00F D47"
        );
        let first = if op.contains("SWAP") {
            format!("{op} D{}{modifier}", alias - 1)
        } else {
            format!("{op} D32{modifier} D48F2 D{}{modifier}", alias - 1)
        };
        let second = if op.contains("SWAP") {
            format!("{op} D33E3")
        } else {
            format!("{op} D32E3 D33F3 D33E3")
        };
        let body = format!(
            "LDI X7\nMOV K1 {modifier}\nLD X0\n{first}\nLDI X7\nMOV E1 D80\nMOV F1 D81\nMOV E2 D82\nMOV F2 D83\nLD X0\n{second}\nOUT Y10"
        );
        cases.push(scenario(
            &format!("own_index_cached_pair_and_shifted_overlap_{op}_{modifier}"),
            &seed,
            &body,
            &format!(
                "{}\n{}\nMOV E1 D240\nMOV F1 D241\nMOV E2 D242\nMOV F2 D243",
                read_words("D", 32, 4, 220),
                read_words("D", 80, 4, 230)
            ),
        ));
    }
    assert_eq!(cases.len(), 72);
}

fn append_direct_f_regressions(cases: &mut Vec<ComparisonCase>) {
    for op in ["SWAP", "SWAPP"] {
        let seed =
            "MOV H1234 F1\nMOV H55AA E2\nMOV H5678 D160\nMOV H6A5A D161\nMOV K0 E3\nMOV H4321 F3";
        let body = format!("{op} D160E3\n{op} F1");
        cases.push(scenario(
            &format!("direct_word_F_with_indexed_site_{op}"),
            seed,
            &body,
            "MOV F1 D220\nMOV E2 D221\nMOV D160 D222\nMOV D161 D223",
        ));
    }
    for op in ["DMVM", "DMVMP"] {
        for role in ["source", "mask", "target"] {
            let raw = match role {
                "source" => "HAA55CC33",
                "mask" => "H0FF0F00F",
                _ => "H12345678",
            };
            let seed = format!(
                "MOV H55AA E1\nDMOV {raw} D1183\nMOV H6A5A F2\nMOV K0 E3\nMOV K0 F3\nDMOV HAA55CC33 D32\nDMOV H0FF0F00F D48\nMOV H5AA5 D159\nDMOV H12345678 D160\nMOV H3CC3 D162"
            );
            let args = match role {
                "source" => "F1 D48F3 D160E3",
                "mask" => "D32E3 F1 D160F3",
                _ => "D32E3 D48F3 F1",
            };
            cases.push(scenario(&format!("direct_wide_F_{role}_canonical_pair_{op}"),&seed,&format!("{op} {args}"),"MOV E1 D220\nMOV F1 D221\nMOV E2 D222\nMOV F2 D223\nMOV D159 D230\nDMOV D160 D232\nMOV D162 D234"));
        }
    }
    assert_eq!(cases.len(), 80);
}

fn append_flags_and_controls(cases: &mut Vec<ComparisonCase>) {
    for base in ["SWAP", "DSWAP", "MVM", "DMVM"] {
        let seed = "SET M1018\nSET M1067\nSET M1068\nMOV K1 E1\nMOV H6A5A F1\nMOV K0 E3\nMOV H1234 F3\nDMOV H0000000A D32";
        let mut body = Vec::new();
        for (slot, suffix) in ["", "P"].into_iter().enumerate() {
            let args = if base.contains("SWAP") {
                "K1M1018E1"
            } else {
                "D32E3 K1M1018E1 K1M1018E1"
            };
            body.push(format!(
                "LDI X7\nMOV H000F K1M1019\nLD X0\n{base}{suffix} {args}\nLDI X7\nMOV K1M1019 D{}",
                80 + slot
            ));
        }
        cases.push(scenario(
            &format!("raw_flags_mask_old_target_overlap_{base}"),
            seed,
            &body.join("\n"),
            "MOV D80 D220\nMOV D81 D221\nMOV K1M1019 D222\nLD M1018\nOUT Y0",
        ));
    }
    assert_eq!(cases.len(), 84);
    for (slot, op) in FORMS.into_iter().enumerate() {
        let wide = op.starts_with('D');
        let mov = if wide { "DMOV" } else { "MOV" };
        let initial = if wide { "H12345678" } else { "H5678" };
        let reset = if wide { "H55555555" } else { "H5555" };
        let args = if op.contains("SWAP") {
            "D160E1"
        } else {
            "D32E2 D48F2 D160E1"
        };
        let seed = format!(
            "{}\n{}\n{mov} {initial} D160\nDMOV HAA55CC33 D32\nDMOV H0FF0F00F D48\nMOV K0 E1\nMOV H6A5A F1\nMOV K0 E2\nMOV K0 F2\nMOV K0 E3\nMOV H1234 F3\nMOV K0 D30\nMOV K0 D60",
            seed_words("D", 159, 8),
            seed_words("D", 80, 6)
        );
        let (label, extra_seed, body) = match slot {
            0 | 6 => (
                "mps_live_target",
                "".to_owned(),
                format!(
                    "MPS\n{op} {args}\nMRD\nMOV K2 E1\nMRD\n{op} {args}\nMPP\nOUT Y10\nLDI X7\nMOV K0 E1"
                ),
            ),
            1 | 4 => (
                "for_same_pc_true_false_true",
                "".to_owned(),
                format!(
                    "LDI X7\nMOV K0 D30\nFOR K3\nLDI X7\nMUL16 D30 K2 E3\nSET M1020\nRST M1021\nSET M1022\nLD= D30 K1\n{mov} {reset} D160\nLD= D30 K0\nOR= D30 K2\nAND X0\n{op} {args}\nLDI X7\nDMOV D160 D80E3\nINC D30\nNEXT"
                ),
            ),
            2 | 5 => (
                "cj_skipped_false_history",
                "".to_owned(),
                format!(
                    "LD X2\n{mov} {reset} D160\nLD X2\nCJ P10\nLD X0\n{op} {args}\nP10\nLDI X7\nINC D60"
                ),
            ),
            3 => (
                "inactive_step_true_logic",
                "ZRST S0 S1023\nRST M1040\nRST M1045\nRST M1047".to_owned(),
                format!("STL S0\nLDI X7\n{op} {args}\nRET\nLDI X7\nINC D60"),
            ),
            7 => (
                "master_false_true_logic",
                "".to_owned(),
                format!("LD X0\nMC N0\nLDI X7\n{op} {args}\nMCR N0\nLDI X7\nINC D60"),
            ),
            _ => unreachable!(),
        };
        let mut entry = scenario(
            &format!("control_{label}_{op}"),
            &format!("{seed}\n{extra_seed}"),
            &body,
            &format!(
                "{}\n{}\nMOV D30 D250\nMOV D60 D251\nMOV E1 D252",
                read_words("D", 159, 8, 220),
                read_words("D", 80, 6, 240)
            ),
        );
        if matches!(slot, 2 | 5) {
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
    assert_eq!(cases.len(), 92);
}

fn append_ordinary_drand_vm_cases(cases: &mut Vec<ComparisonCase>) {
    for op in ["DRAND", "DRANDP"] {
        let seed =
            "MOV K0 E1\nMOV H6A5A F1\nMOV H1234 D32\nDMOV H5555AAAA D20\nMOV K0 D60\nMOV K0 D61";
        let body =
            format!("LD X7\nSWAP D32E1\nLD X0\n{op} K7 K7 D20\nINC D60\nOUT Y10\nLDI X7\nINC D61");
        let mut entry = scenario(
            &format!("ordinary_{op}_equal_bounds_active_inert_tail_VM"),
            seed,
            &body,
            "DMOV D20 D220\nMOV D32 D222\nMOV D60 D230\nMOV D61 D231",
        );
        entry.stages = vec![
            (vec![("X1", true), ("X0", false), ("X7", false)], 1),
            (vec![("X1", false), ("X0", true)], 2),
            (vec![("X0", false)], 2),
            (vec![("X0", true)], 1),
        ];
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
            "{}\n{}\n{}\n{}\nDMOV HAA55CC33 D32\nDMOV H0FF0F00F D48\nMOV K{offset} E1\nMOV H6A5A F1\nMOV K0 E2\nMOV K-{} F2",
            seed_words("D", last - 2, 3),
            seed_bits("Y", 220, 36),
            seed_bits("M", 4060, 36),
            seed_bits("S", 988, 36),
            digits * 4 - 1
        );
        let args = if op.contains("SWAP") {
            format!("D{last}E1")
        } else {
            format!("D32E2 D48E2 D{last}E1")
        };
        let mut body = vec![format!("{op} {args}")];
        for (area, end) in [("Y", 255), ("M", 4095), ("S", 1023)] {
            let group = format!("K{digits}{}F2", address(area, end));
            let args = if op.contains("SWAP") {
                group
            } else {
                format!("D32E2 D48E2 {group}")
            };
            body.push(format!("LD X0\n{op} {args}"));
        }
        let observe = format!(
            "{}\n{}\n{}\n{}\nMOV E1 D250\nMOV F2 D251",
            read_words("D", last - 2, 3, 200),
            read_bits("Y", 220, 36, 220),
            read_bits("M", 4060, 36, 224),
            read_bits("S", 988, 36, 228)
        );
        cases.push(scenario(
            &format!("{}_last_numeric_and_packed_spans_{op}", target.name()),
            &seed,
            &body.join("\n"),
            &observe,
        ));
    }
    check_cases(&cases, 8);
    cases
}
