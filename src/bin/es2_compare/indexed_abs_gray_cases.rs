//! Draft: addressed ABS and Gray conversions. No native execution implied.
//! Literal vectors below are independent of the production conversion code.
//! Generic low-M remapping and invalid visited spans remain outside this corpus.
use super::{Case as ComparisonCase, Target, case};
use std::collections::BTreeSet;

const FORMS: [&str; 12] = [
    "ABS", "ABSP", "DABS", "DABSP", "GRY", "GRYP", "DGRY", "DGRYP", "GBIN", "GBINP", "DGBIN",
    "DGBINP",
];
const INPUTS: [&str; 32] = [
    "X100", "X101", "X102", "X103", "X104", "X105", "X106", "X107", "X110", "X111", "X112", "X113",
    "X114", "X115", "X116", "X117", "X120", "X121", "X122", "X123", "X124", "X125", "X126", "X127",
    "X130", "X131", "X132", "X133", "X134", "X135", "X136", "X137",
];
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
fn index_base(index: u16) -> u16 {
    if index == 0 { 1028 } else { 1180 + index * 2 }
}
fn mov(op: &str) -> &'static str {
    if op.starts_with('D') { "DMOV" } else { "MOV" }
}
fn absolute(op: &str) -> bool {
    op.contains("ABS")
}
fn instruction(op: &str, source: &str, target: &str) -> String {
    if absolute(op) {
        format!("{op} {target}")
    } else {
        format!("{op} {source} {target}")
    }
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
                if i == 0 || i + 1 == count {
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
        seed,gate,body,
        "LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24",
        "LDI X7\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305\nMOV K1234 D306",
        observe,"END",
    ].into_iter().filter(|s|!s.is_empty()).collect::<Vec<_>>().join("\n");
    assert!(source.lines().count() <= 512, "source budget: {name}");
    assert!(
        source.lines().count() * 9 <= 0x1ef0,
        "installed WORD budget: {name}"
    );
    let mut indexed = false;
    for line in source.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.first().is_some_and(|op| FORMS.contains(op)) {
            assert_eq!(
                fields.len(),
                if absolute(fields[0]) { 2 } else { 3 },
                "{name}: {line}"
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
    assert!(indexed, "missing indexed ABS/Gray action: {name}");
    case(&format!("indexed_abs_gray_{name}"), &source, stages())
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
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    for op in FORMS {
        assert!(names.contains(op), "missing {op}");
    }
}

pub(super) fn common_cases() -> Vec<ComparisonCase> {
    let mut cases = Vec::new();
    // Every bank is exercised in read/write ABS and both Gray roles at both
    // widths. The same scan changes its signed WORD offset after all sites.
    for wide in [false, true] {
        for bank in ['E', 'F'] {
            for index in 0..8u16 {
                let modifier = format!("{bank}{index}");
                let pair = index_base(index);
                let alias = pair + u16::from(bank == 'F');
                let companion = pair + u16::from(bank == 'E');
                let pulse = if (index + u16::from(bank == 'F')) % 2 == 0 {
                    ""
                } else {
                    "P"
                };
                let prefix = if wide { "D" } else { "" };
                let abs = format!("{prefix}ABS{pulse}");
                let gray = format!("{prefix}GRY{pulse}");
                let binary = format!("{prefix}GBIN{pulse}");
                let digits = if wide { 8 } else { 4 };
                let delta = if index % 2 == 0 { -1 } else { 1 };
                let poison = if bank == 'F' && index < 7 {
                    format!("MOV H6A5A D{}", alias + 1)
                } else {
                    String::new()
                };
                let mut seed = vec![
                    seed_words("D", 44, 16),
                    seed_words("D", 156, 56),
                    seed_words("T", 60, 12),
                    seed_words("C", 60, 12),
                ];
                for i in 44..60 {
                    seed.push(format!("MOV K{} D{i}", 3 + (i % 7)));
                }
                for area in ["Y", "M", "S"] {
                    seed.push(seed_bits(area, 120, 48));
                    seed.push(format!("DMOV K6513 K8{}", address(area, 128)));
                }
                seed.push(format!(
                    "MOV H5AA5 D{companion}\n{poison}\nMOV K{delta} {modifier}"
                ));
                let mut body = vec![format!(
                    "{abs} D160{modifier}\n{abs} T64{modifier}\n{abs} C64{modifier}\n{gray} D48{modifier} D180{modifier}\n{binary} D52{modifier} D184{modifier}\n{gray} T64{modifier} D188{modifier}\n{binary} C64{modifier} D192{modifier}"
                )];
                for (i, area) in ["Y", "M", "S"].into_iter().enumerate() {
                    let group = format!("K{digits}{}{modifier}", address(area, 128));
                    body.push(format!("{abs} {group}\n{gray} {group} D{}{modifier}\n{binary} D52{modifier} {group}",196+i*4));
                }
                body.push(format!("INC {modifier}\nOUT Y10"));
                let observe = format!(
                    "{}\n{}\n{}\n{}\n{}\n{}\nMOV {modifier} D290\nMOV D{companion} D291",
                    read_words("D", 156, 56, 320),
                    read_words("T", 60, 12, 240),
                    read_words("C", 60, 12, 252),
                    read_bits("Y", 120, 48, 270),
                    read_bits("M", 120, 48, 274),
                    read_bits("S", 120, 48, 278)
                );
                cases.push(scenario(
                    &format!("all_banks_{abs}_{modifier}"),
                    &seed.join("\n"),
                    &body.join("\n"),
                    &observe,
                ));
            }
        }
    }
    assert_eq!(cases.len(), 32);
    append_widths(&mut cases);
    append_literal_vectors(&mut cases);
    append_numeric_boundaries(&mut cases);
    append_aliases(&mut cases);
    append_errors(&mut cases);
    append_direct_indices(&mut cases);
    append_controls(&mut cases);
    check_cases(&cases, 116);
    cases
}

fn append_widths(cases: &mut Vec<ComparisonCase>) {
    for (slot, op) in FORMS.into_iter().enumerate() {
        let wide = op.starts_with('D');
        let max = if wide { 8u16 } else { 4 };
        let area = ["Y", "M", "S"][slot % 3];
        let seed = format!(
            "{}\n{}\nMOV K-1 E1\nMOV H6A5A F1\nMOV K1 F2\nMOV H5AA5 E2",
            seed_bits(area, 120, 48),
            seed_words("D", 80, max * 4)
        );
        let mut body = Vec::new();
        for digits in 1..=max {
            let target = format!("K{digits}{}F2", address(area, 127));
            let reset = format!(
                "LDI X7\nDMOV HFFFFFFFF K8{}\nSET {}\nSET {}",
                address(area, 128),
                address(area, 127),
                address(area, 128 + digits * 4)
            );
            if absolute(op) {
                // Short groups are positive even when every declared bit is1.
                body.push(format!(
                    "{reset}\nLD X0\n{op} {target}\nLDI X7\n{}",
                    read_bits(area, 120, 48, 80 + (digits - 1) * 4)
                ));
            } else {
                // X inputs are injected as inputs, never written by PLC IL.
                // First snapshot full conversion of a short source. A second
                // site then narrows that result into a guarded short target.
                body.push(format!("{reset}\nLD X0\n{op} K{digits}X101E1 D{}\n{op} K{digits}X101E1 {target}\nLDI X7\n{}",160+(digits-1)*2,read_bits(area,120,48,80+(digits-1)*4)));
            }
        }
        let observe = format!(
            "{}\n{}\nDMOV K8X100 D280\nMOV E1 D282\nMOV F2 D283",
            read_words("D", 80, max * 4, 320),
            read_words("D", 160, max * 2, 240)
        );
        let mut entry = scenario(
            &format!("all_widths_zeroextend_truncate_{op}_{area}"),
            &seed,
            &body.join("\n"),
            &observe,
        );
        entry.stages[0].0.extend(
            INPUTS
                .into_iter()
                .enumerate()
                .map(|(bit, input)| (input, (0x1234567fu32 >> bit) & 1 != 0)),
        );
        cases.push(entry);
    }
    assert_eq!(cases.len(), 44);
}

// Literal (input, result) tables; no production function computes the oracle.
// During an enabled non-held visit M800+slot reports equality. Other visits
// intentionally reset the source, exposing P suppression in captured results.
fn vectors(op: &str) -> Vec<(u32, u32)> {
    let wide = op.starts_with('D');
    if absolute(op) {
        if wide {
            vec![
                (0, 0),
                (1, 1),
                (0xffffffff, 1),
                (0x80000000, 0x80000000),
                (0xffff8000, 32768),
                (0xffed2979, 1234567),
                (0x7fffffff, 0x7fffffff),
            ]
        } else {
            vec![
                (0, 0),
                (1, 1),
                (0xffff, 1),
                (0x8000, 0x8000),
                (0x8001, 0x7fff),
                (0xe68f, 6513),
                (0x7fff, 0x7fff),
            ]
        }
    } else if op.contains("GRY") {
        vec![
            (0, 0),
            (1, 1),
            (2, 3),
            (3, 2),
            (6513, 0x15c9),
            (
                if wide { 0x7fffffff } else { 0x7fff },
                if wide { 0x40000000 } else { 0x4000 },
            ),
        ]
    } else {
        vec![
            (0, 0),
            (1, 1),
            (3, 2),
            (2, 3),
            (0x15c9, 6513),
            (
                if wide { 0x7fffffff } else { 0x7fff },
                if wide { 0x55555555 } else { 0x5555 },
            ),
        ]
    }
}
fn append_literal_vectors(cases: &mut Vec<ComparisonCase>) {
    for op in FORMS {
        let wide = op.starts_with('D');
        let compare = if wide { "DLD=" } else { "LD=" };
        let m = mov(op);
        let table = vectors(op);
        let seed = format!(
            "MOV K0 E1\nMOV H6A5A F1\nMOV K0 F2\nMOV H5AA5 E2\n{}\n{}",
            seed_words("D", 63, 5),
            seed_words("D", 159, 5)
        );
        let mut body = Vec::new();
        for (i, (input, result)) in table.iter().enumerate() {
            let action = instruction(op, "D64E1", if absolute(op) { "D64E1" } else { "D160F2" });
            let read = if absolute(op) { "D64" } else { "D160" };
            body.push(format!("LDI X7\n{m} H{input:08X} D64\nLD X0\n{action}\nLDI X7\n{m} {read} D{}\n{m} H{result:08X} D{}\n{compare} D{} D{}\nOUT M{}",80+i*2,240+i*2,80+i*2,240+i*2,800+i));
        }
        cases.push(scenario(
            &format!("literal_min_sign_and_gray_inverse_oracles_{op}"),
            &seed,
            &body.join("\n"),
            &format!(
                "{}\n{}\nMOV D63 D280\nMOV D66 D281\nMOV D159 D282\nMOV D162 D283",
                read_words("D", 80, table.len() as u16 * 2, 320),
                read_words("D", 240, table.len() as u16 * 2, 350)
            ),
        ));
    }
    assert_eq!(cases.len(), 56);
}

fn append_numeric_boundaries(cases: &mut Vec<ComparisonCase>) {
    for op in FORMS {
        let wide = op.starts_with('D');
        let seed = "MOV H1357 C197\nMOV H8001 C198\nMOV H7FFF C199\nDMOV H00123456 C200\nDMOV H00654321 C201\nDMOV H55667788 C254\nDMOV H7FFFFFFF C255\nMOV H5AA5 T253\nMOV H8001 T254\nMOV H0002 T255\nMOV K0 E1\nMOV H6A5A F1\nMOV K1 F2\nMOV K-1 E3\nMOV K0 E4\nMOV H1234 F4";
        let body = if wide {
            if absolute(op) {
                format!(
                    "{op} C198E1\n{op} C200E3\nDMOV C200 D80\n{op} C199F2\n{op} C255E1\n{op} T255E3"
                )
            } else {
                format!(
                    "{op} C200E3 D160E1\n{op} C199F2 C200E3\nDMOV C200 D80\n{op} C200E1 C255E1\n{op} C198E1 T255E3"
                )
            }
        } else {
            instruction(op, "C199E1", "T255E1")
                + "\n"
                + &instruction(op, "T255E1", "C199E1")
                + "\n"
                + &instruction(op, "C198F2", "D160E4")
        };
        cases.push(scenario(&format!("T255_C198_C199_C200_C255_effective_width_{op}"),seed,&body,&format!("{}\n{}\nDMOV C200 D230\nDMOV C201 D232\nDMOV C254 D234\nDMOV C255 D236\nDMOV D80 D238\nDMOV D160 D240",read_words("C",197,3,220),read_words("T",253,3,224))));
    }
    assert_eq!(cases.len(), 68);
}

fn append_aliases(cases: &mut Vec<ComparisonCase>) {
    for op in FORMS {
        let wide = op.starts_with('D');
        let (modifier, alias) = if op.ends_with('P') {
            ("F1", 1183u16)
        } else {
            ("E1", 1182)
        };
        let m = mov(op);
        let input = if wide { "H12345678" } else { "H1971" };
        let seed = format!(
            "MOV H55AA E1\nMOV H6A5A F1\nMOV H2468 E2\nMOV H7788 F2\nMOV K0 E3\nMOV H1234 F3\n{m} {input} D32\n{}",
            seed_words("D", 63, 5)
        );
        let refresh = if absolute(op) {
            format!("MOV K-1 {modifier}\nMOV HFFFF D{}", alias + 1)
        } else {
            format!("MOV K1 {modifier}")
        };
        let target = format!(
            "D{}{modifier}",
            if absolute(op) { alias + 1 } else { alias - 1 }
        );
        let first = instruction(op, "D32E3", &target);
        let overlap = instruction(op, "D64E3", if absolute(op) { "D64E3" } else { "D65E3" });
        let body = format!(
            "LDI X7\n{refresh}\n{m} {input} D64\nLD X0\n{first}\nLDI X7\nMOV E1 D80\nMOV F1 D81\nMOV E2 D82\nMOV F2 D83\nLD X0\n{overlap}\nOUT Y10"
        );
        cases.push(scenario(
            &format!("own_index_cached_high_store_shifted_overlap_{op}"),
            &seed,
            &body,
            &format!(
                "{}\n{}\nMOV E1 D240\nMOV F1 D241\nMOV E2 D242\nMOV F2 D243",
                read_words("D", 63, 5, 220),
                read_words("D", 80, 4, 230)
            ),
        ));
    }
    assert_eq!(cases.len(), 80);
}

fn append_errors(cases: &mut Vec<ComparisonCase>) {
    for op in FORMS.into_iter().filter(|op| !absolute(op)) {
        let m = mov(op);
        let invalid = if op.starts_with('D') {
            "H80000000"
        } else {
            "H8000"
        };
        let seed = format!(
            "{m} K7 D32\nMOV K0 E1\nMOV H6A5A F1\nMOV K0 F2\nMOV K1 E2\nMOV HA K1M128\n{}",
            seed_words("D", 0, 4)
        );
        // Initial OFF has a valid source and target. Powered visits change
        // both together; invalid source must skip F2 and D0-1. Recovery repairs
        // the index unconditionally before every following positive site.
        let body = format!(
            "LD X0\n{m} {invalid} D32\nMOV K-1 F2\nLD X0\n{op} D32E1 D0F2\nLDI X7\nMOV D1067 D80\nMOV D1068 D81\nDMOV D0 D82\nLD X0\n{op} D32E1 K1M127E2\nLDI X7\nMOV K1M128 D84\n{m} K7 D32\nMOV K0 F2\nLD X0\n{op} D32E1 D0F2\nOUT Y10"
        );
        cases.push(scenario(
            &format!("negative_before_bad_target_and_short_truncation_sticky_recovery_{op}"),
            &seed,
            &body,
            &format!(
                "{}\n{}\nMOV K1M128 D240\nMOV F2 D241",
                read_words("D", 0, 4, 220),
                read_words("D", 80, 5, 230)
            ),
        ));
    }
    assert_eq!(cases.len(), 88);
}

fn append_direct_indices(cases: &mut Vec<ComparisonCase>) {
    for op in FORMS {
        let wide = op.starts_with('D');
        let m = mov(op);
        let input = if absolute(op) { "K-12345" } else { "K6513" };
        let seed = format!(
            "{m} {input} E1\n{m} {input} D32\n{m} {input} D160\nMOV H55AA E7\nMOV H6A5A F7\nMOV K0 E3\nMOV H1234 F3\nMOV H5AA5 E4\nMOV K0 F4"
        );
        let body = if absolute(op) {
            format!("{op} D160E3\n{op} {}", if wide { "E1" } else { "F1" })
        } else {
            format!(
                "{op} {} D160E3\n{op} D32E3 {}",
                if wide { "E1" } else { "F1" },
                if wide { "E7" } else { "F7" }
            )
        };
        // Word F1 needs its own seed; it is not the high half of word E1.
        let extra = if wide {
            String::new()
        } else {
            format!("\nMOV {input} F1")
        };
        cases.push(scenario(&format!("direct_{}_canonical_roles_with_modified_site_{op}",if wide {"E_pair"} else {"F_word_only"}),&format!("{seed}{extra}"),&body,"DMOV D160 D220\nMOV E1 D230\nMOV F1 D231\nMOV E2 D232\nMOV E7 D234\nMOV F7 D235\nMOV E3 D240"));
    }
    assert_eq!(cases.len(), 100);
}

fn append_controls(cases: &mut Vec<ComparisonCase>) {
    for (slot, op) in FORMS.into_iter().enumerate() {
        let m = mov(op);
        let seed = format!(
            "{}\n{m} K6513 D32\n{m} K-12345 D160\n{m} K-23456 D162\nMOV K0 E1\nMOV H6A5A F1\nMOV K0 E2\nMOV H1234 F2\nMOV K1 F3\nSET M1018\nMOV K0 E4\nMOV K0 D30\nMOV K0 D60\n{}",
            seed_words("D", 159, 8),
            seed_words("D", 80, 6)
        );
        let action = instruction(op, "D32E2", "D160E1");
        let control = match slot {
            2 => 3,
            3 => 2,
            _ => slot % 6,
        };
        let (label, extra, body) = match control {
            0 => (
                "MPS_live_target",
                "".to_owned(),
                format!(
                    "MPS\n{action}\nMRD\nMOV K2 E1\nMRD\n{action}\nMPP\nOUT Y10\nLDI X7\nMOV K0 E1"
                ),
            ),
            1 => (
                "FOR_same_PC_true_false_true",
                "".to_owned(),
                format!(
                    "LDI X7\nMOV K0 D30\nFOR K3\nLDI X7\nMUL16 D30 K2 E4\nLD= D30 K1\n{m} K-17 D160\nLD= D30 K0\nOR= D30 K2\nAND X0\n{action}\nLDI X7\nDMOV D160 D80E4\nINC D30\nNEXT"
                ),
            ),
            2 => (
                "CJ_skipped_false_history",
                "".to_owned(),
                format!(
                    "LD X2\n{m} K-17 D160\nLD X2\nCJ P10\nLD X0\n{action}\nP10\nLDI X7\nINC D60"
                ),
            ),
            3 => (
                "MC_false_with_true_logic",
                "".to_owned(),
                format!("LD X0\nMC N0\nLDI X7\n{action}\nMCR N0\nLDI X7\nINC D60"),
            ),
            4 => (
                "STL_inactive_true_logic",
                "ZRST S0 S1023\nRST M1040\nRST M1045\nRST M1047".to_owned(),
                format!("STL S0\nLDI X7\n{action}\nRET\nLDI X7\nINC D60"),
            ),
            5 => (
                "STL_active_true_logic",
                "ZRST S0 S1023\nSET S0\nRST M1040\nRST M1045\nRST M1047".to_owned(),
                format!("STL S0\nLDI X7\n{action}\nRET\nLDI X7\nINC D60"),
            ),
            _ => unreachable!(),
        };
        let flag_action = instruction(op, "D32E2", "K1M1018F3");
        let body = format!("{body}\nLDI X7\nMOV H5 K1M1019\nLD X0\n{flag_action}");
        let observe = format!(
            "{}\n{}\nMOV K1M1019 D240\nMOV E1 D241\nMOV D30 D242\nMOV D60 D243\nLD M1018\nOUT Y11\nLD S0\nOUT Y12\nLD S1\nOUT Y13",
            read_words("D", 159, 8, 220),
            read_words("D", 80, 6, 230)
        );
        let mut entry = scenario(
            &format!("control_{label}_raw_flag_group_{op}"),
            &format!("{seed}\n{extra}"),
            &body,
            &observe,
        );
        if control == 2 {
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
    assert_eq!(cases.len(), 112);
    // Separate changing-source and changing-target sites distinguish a pulse
    // instruction from a cached result or a pulse keyed by effective address.
    for op in ["GRYP", "DGRYP", "GBINP", "DGBINP"] {
        let seed = format!(
            "MOV H3 K1M1019\nSET M1018\nMOV K0 E1\nMOV H6A5A F1\nMOV K0 F2\nMOV K1 F3\n{}",
            seed_words("D", 159, 8)
        );
        let body = format!("{op} K1M1018F3 D160F2\nMOV HF K1M1019\nMOV K2 F2\nOUT Y10");
        cases.push(scenario(
            &format!("pulse_live_flag_source_and_moving_target_{op}"),
            &seed,
            &body,
            &format!(
                "{}\nMOV K1M1019 D240\nMOV F2 D242\nLD M1018\nOUT Y11",
                read_words("D", 159, 8, 220)
            ),
        ));
    }
    assert_eq!(cases.len(), 116);
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
        let digits = bits / 4;
        let m = mov(op);
        let offset = if wide { -1 } else { 0 };
        let mut seed = vec![
            seed_words("D", last - 2, 3),
            format!(
                "{m} K6513 D32\n{m} K6513 D{}\nMOV K{offset} E1\nMOV H6A5A F1\nMOV K0 E3\nMOV H1234 F3\nMOV K-{} F2",
                last - u16::from(wide),
                bits - 1
            ),
        ];
        for (area, base) in [("Y", 220), ("M", 4060), ("S", 988)] {
            seed.push(seed_bits(area, base, 36));
        }
        let target_operand = format!("D{last}E1");
        let mut body = if absolute(op) {
            vec![instruction(op, "", &target_operand)]
        } else {
            vec![
                instruction(op, &target_operand, "D160E3"),
                instruction(op, "D32E3", &target_operand),
            ]
        };
        for (area, end) in [("Y", 255), ("M", 4095), ("S", 1023)] {
            let group = format!("K{digits}{}F2", address(area, end));
            body.push(instruction(op, "D32E3", &group));
            if !absolute(op) {
                body.push(instruction(op, &group, "D180E3"));
            }
        }
        let observe = format!(
            "{}\n{}\n{}\n{}\nDMOV D160 D250\nDMOV D180 D252\nMOV E1 D254\nMOV F2 D255",
            read_words("D", last - 2, 3, 200),
            read_bits("Y", 220, 36, 220),
            read_bits("M", 4060, 36, 224),
            read_bits("S", 988, 36, 228)
        );
        cases.push(scenario(
            &format!(
                "model_{}_last_word_pair_and_packed_spans_{op}",
                target.name()
            ),
            &seed.join("\n"),
            &body.join("\n"),
            &observe,
        ));
    }
    check_cases(&cases, 12);
    cases
}
