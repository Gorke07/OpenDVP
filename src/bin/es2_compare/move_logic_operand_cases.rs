//! MOV/CML/logic/NEG operand differentials. Constructing these fixtures is
//! not a native compatibility result; the runner compares independent reads.
use super::{Case, case};

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

fn scenario(name: &str, seed: &str, body: &str, guards: &[(&str, bool)], observe: &str) -> Case {
    let guard_seed = guards
        .iter()
        .map(|(name, on)| format!("{} {name}", if *on { "SET" } else { "RST" }))
        .collect::<Vec<_>>()
        .join("\n");
    let guard_reads = guards
        .iter()
        .enumerate()
        .map(|(index, (name, _))| format!("LD {name}\nOUT Y{:o}", index + 3))
        .collect::<Vec<_>>()
        .join("\n");
    let observe = if observe.is_empty() {
        String::new()
    } else {
        // Read register guards on every scan using only the two control inputs.
        format!("LD X0\nORI X0\n{observe}\n")
    };
    case(
        &format!("move_logic_operand_{name}"),
        &format!(
            "LD X1\n{seed}\n{guard_seed}\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nLD X0\n{body}\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\n{guard_reads}\nLD M1067\nOUT Y10\nLD M1068\nOUT Y11\n{observe}END"
        ),
        stages(),
    )
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();

    // 24 spellings x three writable bit areas = 72 lifecycle cases. Source
    // groups change after the tested action, exposing a held P instruction.
    // NEG instead toggles its nibble 5 <-> B on every enabled normal visit.
    for (word, double) in [
        ("MOV", "DMOV"),
        ("CML", "DCML"),
        ("WAND", "DAND"),
        ("WOR", "DOR"),
        ("WXOR", "DXOR"),
        ("NEG", "DNEG"),
    ] {
        for wide in [false, true] {
            let bits = if wide { 32 } else { 16 };
            let inc = if wide { "DINC" } else { "INC" };
            let op = if wide { double } else { word };
            for pulse in [false, true] {
                let mnemonic = format!("{op}{}", if pulse { "P" } else { "" });
                for (area, base) in [("Y", 128), ("M", 192), ("S", 192)] {
                    let target = address(area, base);
                    let guard_names = [
                        address(area, base - 2),
                        address(area, base - 1),
                        address(area, base + 4),
                        address(area, base + 5),
                    ];
                    let guards = [
                        (guard_names[0].as_str(), false),
                        (guard_names[1].as_str(), true),
                        (guard_names[2].as_str(), true),
                        (guard_names[3].as_str(), false),
                    ];
                    let mask = match word {
                        "WOR" => 8,
                        "WXOR" => 10,
                        _ => 15,
                    };
                    let seed = format!(
                        "{}\n{}\n{}",
                        seed_bits("M", 64, bits, 5),
                        seed_bits("M", 112, bits, mask),
                        seed_bits(area, base, 4, 5),
                    );
                    let body = if word == "NEG" {
                        format!("{mnemonic} K1{target}")
                    } else {
                        let source = format!("K{}M64", bits / 4);
                        let operands = if matches!(word, "MOV" | "CML") {
                            source
                        } else {
                            format!("{source} K{}M112", bits / 4)
                        };
                        format!("{mnemonic} {operands} K1{target}\n{inc} K{}M64", bits / 4)
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
        }
    }

    // A short source clears unused bits INSIDE the declared destination.
    // Word MOV preserves D21 and M208..223; DMOV clears its high word/bits.
    cases.push(scenario(
        "short_move_source_clears_only_declared_width",
        &format!(
            "{}\n{}\n{}\nMOV HFFFF D20\nMOV H55AA D21\nDMOV HFFFFFFFF D30",
            seed_bits("M", 64, 16, 0xabf5),
            seed_bits("M", 192, 32, u32::MAX),
            seed_bits("M", 256, 32, u32::MAX),
        ),
        "MOV K1M64 D20\nMOV K1M64 K4M192\nDMOV K1M64 D30\nDMOV K1M64 K8M256",
        &[
            ("M191", true),
            ("M224", false),
            ("M255", true),
            ("M288", false),
        ],
        "MOV D21 D40\nMOV K4M208 D41",
    ));

    // Zero extension occurs BEFORE complement: 5 becomes FFFA / FFFFFFFA,
    // rather than merely complementing the source's four available bits.
    cases.push(scenario(
        "complement_zero_extended_full_width",
        &format!("{}\nMOV H55AA D21", seed_bits("M", 64, 4, 5)),
        "CML K1M64 D20\nDCML K1M64 D30\nCML K1M64 K4M192\nDCML K1M64 K8M256",
        &[
            ("M191", true),
            ("M208", false),
            ("M255", true),
            ("M288", false),
        ],
        "MOV D21 D40",
    ));

    // Shifted overlaps expose a writer that starts before the whole source
    // value is read, including two overlapping logical sources.
    cases.push(scenario(
        "source_destination_overlap_snapshot",
        &format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}",
            seed_bits("M", 64, 16, 0x1234),
            seed_bits("M", 80, 4, 0xa),
            seed_bits("M", 128, 32, 0x1234_5678),
            seed_bits("M", 160, 8, 0xab),
            seed_bits("M", 320, 32, 0x1234_5678),
            seed_bits("M", 384, 32, 0x1234_5678),
            seed_bits("M", 416, 8, 0xab),
        ),
        "MOV K4M64 K4M68\nDMOV K8M128 K8M136\nWXOR K4M320 K4M328 K4M324\nDCML K8M384 K8M392",
        &[
            ("M63", false),
            ("M84", true),
            ("M127", true),
            ("M168", false),
        ],
        "",
    ));

    cases.push(scenario(
        "word_E_F_alias_shared_words",
        "MOV H1234 D1182\nMOV H00F0 D1183\nMOV K41 D1181\nMOV K42 D1184",
        "MOV F1 E1\nCML E1 F1\nWXOR E1 F1 E1\nNEG F1",
        &[],
        "MOV D1182 D20\nMOV D1183 D21\nMOV D1181 D22\nMOV D1184 D23",
    ));

    // Direct E2 is low D1184 / high D1185, not E2/E3. The adjacent index
    // words stay intact while the final full-pair XOR becomes FFFFFFFF.
    cases.push(scenario(
        "double_E_pair_alias_and_adjacent_half",
        "DMOV H12345678 D1184\nMOV K41 D1183\nMOV K42 D1186",
        "DMOV E2 D20\nDCML E2 E2\nDXOR E2 D20 E2",
        &[],
        "MOV D1184 D30\nMOV D1185 D31\nMOV D1183 D32\nMOV D1186 D33",
    ));

    cases.push(scenario(
        "negation_minimum_wrap",
        &format!(
            "{}\n{}\nDMOV H80000000 D1028\nDMOV H12345678 D1182",
            seed_bits("M", 64, 16, 0x8000),
            seed_bits("M", 128, 32, 0x8000_0000),
        ),
        "NEG K4M64\nDNEG K8M128\nDNEG E0",
        &[
            ("M63", false),
            ("M80", true),
            ("M127", true),
            ("M160", false),
        ],
        "DMOV D1028 D20\nDMOV D1182 D22",
    ));

    // Ordinary stored timer/counter values only: no TMR/CNT or clock input.
    cases.push(scenario(
        "word_timer_counter_storage",
        "MOV H1234 D0\nMOV K41 T254\nMOV K42 C198",
        "MOV D0 T255\nCML T255 C199\nWAND C199 H00FF T255\nWOR T255 H0100 C199\nWXOR C199 H000F T255\nNEG C199\nINC D0",
        &[],
        "MOV T254 D20\nMOV T255 D21\nMOV C198 D22\nMOV C199 D23",
    ));

    // C201 is not the high word of C200, and C255 is a complete natural
    // double-word counter at the model boundary. Neighbor values are read.
    cases.push(scenario(
        "natural_counter_double_targets_independent_neighbors",
        &format!(
            "{}\nDMOV K41 C201\nDMOV K42 C254",
            seed_bits("M", 64, 32, 0x1234_5678),
        ),
        "DMOV K8M64 C200\nDCML C200 C255\nDXOR C255 C200 C255\nDNEG C200\nDINC K8M64",
        &[],
        "DMOV C200 D20\nDMOV C201 D22\nDMOV C254 D24\nDMOV C255 D26",
    ));

    // Keep the native batch bounded: 72 lifecycle + eight focused cases.
    debug_assert_eq!(cases.len(), 80);
    cases
}
