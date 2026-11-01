//! Indexed basic bit instructions only: six contacts and OUT/SET/RST.
//! Signed E/F offsets are bit offsets; no direct edge mnemonic takes a suffix.
use super::{Case, case};
use std::collections::BTreeSet;

const CONTACTS: [&str; 6] = ["LD", "LDI", "AND", "ANI", "OR", "ORI"];
const WRITES: [&str; 3] = ["OUT", "SET", "RST"];

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (
            vec![
                ("X1", true),
                ("X0", false),
                ("X7", false),
                ("X74", false),
                ("X75", false),
                ("X76", false),
                ("X77", true),
                ("X100", false),
                ("X101", true),
                ("X102", false),
                ("X103", true),
                ("X104", true),
                ("X105", false),
                ("X106", true),
                ("X107", false),
                ("X110", false),
                ("X111", true),
                ("X112", false),
                ("X113", true),
            ],
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

fn seed_bits(area: &str, base: u16, value: u16) -> String {
    (0..16)
        .map(|bit| {
            format!(
                "{} {}",
                if value & (1 << bit) != 0 {
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

fn source_seed(area: &str) -> String {
    if area == "X" {
        String::new()
    } else {
        seed_bits(area, 60, 0xa5a8)
    }
}

fn target_seed(area: &str, base: u16, op: &str) -> String {
    // Candidate targets are base+2..7; the outside bits contain mixed guards.
    let value = if op == "SET" { 0xa501 } else { 0xa5fd };
    seed_bits(area, base, value)
}

fn read_window(area: &str, base: u16, output: u16) -> String {
    format!("MOV K4{} D{output}", address(area, base))
}

fn contact(op: &str, source: &str, output: usize) -> String {
    let before = match op {
        "LD" | "LDI" => "",
        "AND" | "ANI" => "LD X0\n",
        "OR" | "ORI" => "LDI X0\n",
        _ => unreachable!(),
    };
    format!("{before}{op} {source}\nOUT Y{output:o}")
}

fn index_seed(modifier: &str, value: i16) -> String {
    let bank = modifier.as_bytes()[0] as char;
    let n = modifier[1..].parse::<u16>().unwrap();
    let base = if n == 0 { 1028 } else { 1180 + n * 2 };
    let alias = base + u16::from(bank == 'F');
    let other = base + u16::from(bank == 'E');
    format!("MOV H6A5A D{other}\nMOV K{value} D{alias}")
}

fn scenario(name: &str, seed: &str, advance: &str, body: &str, observe: &str) -> Case {
    let advance = if advance.is_empty() {
        String::new()
    } else {
        format!("LD X0\n{advance}")
    };
    let source = [
        "LD X1\nMOV H55AA D1028\nMOV H1234 D1029",
        seed,
        &advance,
        "LDI X7\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068",
        body,
        "LD M1020\nOUT Y300\nLD M1021\nOUT Y301\nLD M1022\nOUT Y302\nLD M1067\nOUT Y303\nLD M1068\nOUT Y304",
        "LD X0\nORI X0\nMOV D1067 D300\nMOV D1068 D301\nMOV E0 D302\nMOV F0 D303\nMOV D1028 D304\nMOV D1029 D305",
        observe,
        "END",
    ].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n");
    assert!(
        source.lines().count() <= 512,
        "oversized indexed bit case {name}"
    );
    assert!(
        source.lines().any(|line| {
            let mut fields = line.split_whitespace();
            let Some(op) = fields.next() else {
                return false;
            };
            (CONTACTS.contains(&op) || WRITES.contains(&op))
                && fields
                    .next()
                    .is_some_and(|s| s.contains('E') || s.contains('F'))
        }),
        "case has no indexed basic instruction: {name}"
    );
    case(&format!("indexed_bit_{name}"), &source, stages())
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();

    // All six contact forms on all four source areas and both modifier banks.
    // Source bits at62..65 are false,true,false,true; X uses equivalent inputs.
    for op in CONTACTS {
        for area in ["X", "Y", "M", "S"] {
            for modifier in ["E1", "F2"] {
                let seed = format!("{}\n{}", source_seed(area), index_seed(modifier, -2));
                cases.push(scenario(
                    &format!("contact_{op}_{area}_{modifier}"),
                    &seed,
                    &format!("INC {modifier}"),
                    &contact(op, &format!("{}{modifier}", address(area, 64)), 0),
                    &format!("{}\nMOV {modifier} D210", read_window(area, 60, 200)),
                ));
            }
        }
    }
    assert_eq!(cases.len(), 48);

    // False OUT clears only the current effective bit. SET/RST leave it and
    // former targets unchanged; all surrounding16bits are captured each scan.
    for op in WRITES {
        for area in ["Y", "M", "S"] {
            for modifier in ["E1", "F2"] {
                let seed = format!(
                    "{}\n{}",
                    target_seed(area, 156, op),
                    index_seed(modifier, -2)
                );
                cases.push(scenario(
                    &format!("write_{op}_{area}_{modifier}"),
                    &seed,
                    &format!("INC {modifier}"),
                    &format!("LD X0\n{op} {}{modifier}", address(area, 160)),
                    &format!("{}\nMOV {modifier} D210", read_window(area, 156, 200)),
                ));
            }
        }
    }
    assert_eq!(cases.len(), 66);

    // All16 modifier words occur in all nine instruction roles. Nonzero
    // companion words expose accidental32-bit reads of the E/F index pair.
    for bank in ['E', 'F'] {
        for index in 0..8 {
            let modifier = format!("{bank}{index}");
            let seed = format!(
                "{}\n{}\n{}\n{}\n{}\n{}\n{}",
                source_seed("Y"),
                source_seed("M"),
                source_seed("S"),
                target_seed("Y", 156, "OUT"),
                target_seed("M", 156, "SET"),
                target_seed("S", 156, "RST"),
                index_seed(&modifier, -1)
            );
            let body = [
                contact("LD", &format!("X100{modifier}"), 0),
                contact("LDI", &format!("Y100{modifier}"), 1),
                contact("AND", &format!("M64{modifier}"), 2),
                contact("ANI", &format!("S64{modifier}"), 3),
                contact("OR", &format!("X100{modifier}"), 4),
                contact("ORI", &format!("Y100{modifier}"), 5),
                format!("LD X0\nOUT Y240{modifier}\nSET M160{modifier}\nRST S160{modifier}"),
            ]
            .join("\n");
            cases.push(scenario(&format!("all_forms_{modifier}"), &seed,
                &format!("INC {modifier}"), &body,
                &format!("MOV K4X74 D200\nMOV K4Y74 D201\nMOV K4M60 D202\nMOV K4S60 D203\nMOV K4Y234 D210\nMOV K4M156 D211\nMOV K4S156 D212\nMOV E{index} D220\nMOV F{index} D221")));
        }
    }
    assert_eq!(cases.len(), 82);

    // Read an updated offset within one rung/program visit, not just at the
    // beginning of a scan. Each write site has a distinct base descriptor.
    for op in CONTACTS {
        let mut body = Vec::new();
        for (slot, value) in [-1, 0, 1].into_iter().enumerate() {
            body.push(format!("LDI X7\nMOV K{value} E0"));
            body.push(contact(op, "M64E0", slot));
        }
        cases.push(scenario(
            &format!("same_scan_contact_{op}"),
            &format!("{}\n{}", source_seed("M"), index_seed("E0", -1)),
            "",
            &body.join("\n"),
            "MOV K4M60 D200\nMOV E0 D210\nMOV F0 D211",
        ));
    }
    for op in WRITES {
        let mut body = Vec::new();
        for (slot, value) in [-1, 0, 1].into_iter().enumerate() {
            body.push(format!(
                "LDI X7\nMOV K{value} F0\n{} X0\n{op} Y{:o}F0",
                if slot == 1 { "LDI" } else { "LD" },
                128 + slot
            ));
        }
        cases.push(scenario(
            &format!("same_scan_write_{op}"),
            &format!("{}\n{}", target_seed("Y", 124, op), index_seed("F0", -1)),
            "",
            &body.join("\n"),
            "MOV K4Y174 D200\nMOV E0 D210\nMOV F0 D211",
        ));
    }
    assert_eq!(cases.len(), 91);

    for op in CONTACTS {
        let seed = format!("{}\n{}", source_seed("M"), index_seed("E1", -2));
        let body = if matches!(op, "LD" | "LDI") {
            format!("{op} M64E1\nMPS\nAND X0\nOUT Y0\nMRD\nANI X0\nOUT Y1\nMPP\nOR X0\nOUT Y2")
        } else {
            // MOV is an output instruction: reload the saved branch again
            // before appending the next indexed AND/ANI/OR/ORI condition.
            format!(
                "LD X0\nMPS\n{op} M64E1\nOUT Y0\nMRD\nMOV K0 E1\nMRD\n{op} M64E1\nOUT Y1\nMPP\n{op} M64E1\nOUT Y2"
            )
        };
        cases.push(scenario(
            &format!("branch_stack_{op}"),
            &seed,
            "INC E1",
            &body,
            "MOV K4M60 D200\nMOV E1 D210\nMOV F1 D211",
        ));
    }
    assert_eq!(cases.len(), 97);

    // The LD source changes address while the following NP/PN stays at one
    // stable instruction site. Markers expose missing or spuriously rearmed
    // edge state, with one unconditional successor count per scan.
    for area in ["X", "Y", "M", "S"] {
        for edge in ["NP", "PN"] {
            let seed = format!(
                "{}\n{}\nMOV K0 D20\nMOV K0 D21",
                source_seed(area),
                index_seed("F7", -2)
            );
            let body = format!(
                "LD {}F7\n{edge}\nOUT Y0\nINC D20\nLDI X7\nINC D21",
                address(area, 64)
            );
            cases.push(scenario(
                &format!("stable_edge_{edge}_{area}"),
                &seed,
                "INC F7",
                &body,
                &format!(
                    "{}\nMOV D20 D210\nMOV D21 D211\nMOV E7 D212\nMOV F7 D213",
                    read_window(area, 60, 200)
                ),
            ));
        }
    }
    assert_eq!(cases.len(), 105);

    // Repeated visits to one output descriptor are loop execution, not
    // duplicate output sites. Every iteration selects another bounded bit.
    for op in WRITES {
        for area in ["Y", "M", "S"] {
            let seed = format!(
                "{}\n{}\n{}\nMOV K0 D20",
                source_seed("M"),
                target_seed(area, 156, op),
                index_seed("E1", -1)
            );
            let body = format!(
                "LDI X7\nMOV K-1 E1\nFOR K3\nLD M64E1\nAND X0\n{op} {}E1\nLDI X7\nINC E1\nINC D20\nNEXT",
                address(area, 160)
            );
            cases.push(scenario(
                &format!("bounded_loop_{op}_{area}"),
                &seed,
                "",
                &body,
                &format!(
                    "{}\nMOV K4M60 D201\nMOV E1 D210\nMOV F1 D211\nMOV D20 D212",
                    read_window(area, 156, 200)
                ),
            ));
        }
    }
    assert_eq!(cases.len(), 114);

    for op in WRITES {
        for area in ["Y", "M", "S"] {
            let seed = format!(
                "{}\n{}\n{}\n{}\nMOV K0 D20",
                source_seed("M"),
                target_seed(area, 156, op),
                index_seed("E1", -2),
                index_seed("F2", -2)
            );
            let body = format!(
                "LD X0\nMC N0\nLD M64E1\n{op} {}F2\nMCR N0\nLDI X7\nINC D20",
                address(area, 160)
            );
            cases.push(scenario(
                &format!("master_reset_{op}_{area}"),
                &seed,
                "INC E1\nINC F2",
                &body,
                &format!(
                    "{}\nMOV K4M60 D201\nMOV E1 D210\nMOV F2 D211\nMOV D20 D212",
                    read_window(area, 156, 200)
                ),
            ));
        }
    }
    assert_eq!(cases.len(), 123);

    // Use the existing basic STL/RET reference shape without dynamic S-state
    // transition targets. This checks indexed Y/M coils under an active step.
    for op in WRITES {
        for area in ["Y", "M"] {
            let seed = format!(
                "ZRST S0 S1023\nRST M1040\nRST M1045\nRST M1047\nSET S0\n{}\n{}\n{}\n{}\nMOV K0 D20",
                source_seed("M"),
                target_seed(area, 156, op),
                index_seed("E1", -2),
                index_seed("F2", -2)
            );
            let body = format!(
                "STL S0\nLD X0\nAND M64E1\n{op} {}F2\nRET\nLDI X7\nINC D20",
                address(area, 160)
            );
            cases.push(scenario(
                &format!("step_context_{op}_{area}"),
                &seed,
                "INC E1\nINC F2",
                &body,
                &format!(
                    "{}\nMOV K4M60 D201\nMOV K1S0 D202\nMOV E1 D210\nMOV F2 D211\nMOV D20 D212",
                    read_window(area, 156, 200)
                ),
            ));
        }
    }
    assert_eq!(cases.len(), 129);

    // Two different modifier words select the same physical source/target.
    // A subsequent indexed LD must see the completed write, not a cached bit.
    for (op, area) in [("OUT", "Y"), ("SET", "M"), ("RST", "S")] {
        let seed = format!(
            "{}\n{}\n{}",
            source_seed(area),
            index_seed("E1", -1),
            index_seed("F2", -1)
        );
        let body = format!(
            "{} {}E1\nAND X0\n{op} {}F2\nLD {}E1\nOUT Y0\nLD X0\nMOV K0 E1\nMOV K0 F2\nLD {}E1\nOUT Y1",
            if op == "RST" { "LD" } else { "LDI" },
            address(area, 64),
            address(area, 64),
            address(area, 64),
            address(area, 64)
        );
        cases.push(scenario(
            &format!("source_target_alias_{op}_{area}"),
            &seed,
            "",
            &body,
            &format!(
                "{}\nMOV E1 D210\nMOV F1 D211\nMOV E2 D212\nMOV F2 D213",
                read_window(area, 60, 200)
            ),
        ));
    }
    assert_eq!(cases.len(), 132);

    // The indexed handlers use the live logic bit and out_enabled, ignoring
    // the MC master gate. LDI X7 makes that distinction visible while X0 is
    // false; the candidate bit must still receive OUT/SET/RST's true action.
    for op in WRITES {
        for area in ["Y", "M", "S"] {
            let seed = format!(
                "{}\n{}\nMOV K0 D20",
                target_seed(area, 156, op),
                index_seed("F2", -2)
            );
            let body = format!(
                "LD X0\nMC N0\nLDI X7\n{op} {}F2\nMCR N0\nLDI X7\nINC D20",
                address(area, 160)
            );
            cases.push(scenario(
                &format!("master_false_true_logic_{op}_{area}"),
                &seed,
                "INC F2",
                &body,
                &format!(
                    "{}\nMOV E2 D210\nMOV F2 D211\nMOV D20 D212",
                    read_window(area, 156, 200)
                ),
            ));
        }
    }
    assert_eq!(cases.len(), 141);

    // Seeding S0 establishes its current/previous snapshots at the first
    // END, matching the existing STL fixtures. Indexed S writes are raw bit
    // stores: they must neither deactivate S0 nor consult M1040, and S1 is
    // retained as an adjacent step guard. Effective targets stay at S162..165.
    for op in WRITES {
        for inhibited in [false, true] {
            let seed = format!(
                "ZRST S0 S1023\n{} M1040\nRST M1045\nRST M1047\nSET S0\n{}\n{}\nMOV K0 D20",
                if inhibited { "SET" } else { "RST" },
                target_seed("S", 156, op),
                index_seed("F2", -2)
            );
            let body = format!("STL S0\nLD X0\n{op} S164F2\nRET\nLDI X7\nINC D20");
            cases.push(scenario(
                &format!("step_raw_S_{op}_M1040_{}", u8::from(inhibited)),
                &seed,
                "INC F2",
                &body,
                "LD S0\nOUT Y0\nLD S1\nOUT Y1\nLD M1040\nOUT Y2\nLDI X7\nMOV K1S0 D200\nMOV K4S156 D201\nMOV E2 D210\nMOV F2 D211\nMOV D20 D212",
            ));
        }
    }
    assert_eq!(cases.len(), 147);

    let actual: BTreeSet<_> = cases
        .iter()
        .flat_map(|item| item.source.lines())
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let op = words.next()?;
            let arg = words.next()?;
            ((CONTACTS.contains(&op) || WRITES.contains(&op))
                && (arg.contains('E') || arg.contains('F')))
            .then_some(op)
        })
        .collect();
    let required: BTreeSet<_> = CONTACTS.into_iter().chain(WRITES).collect();
    assert_eq!(actual, required, "missing indexed bit instruction");
    assert_eq!(
        cases
            .iter()
            .map(|item| &item.name)
            .collect::<BTreeSet<_>>()
            .len(),
        cases.len()
    );
    cases
}
