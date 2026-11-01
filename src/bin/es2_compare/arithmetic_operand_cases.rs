//! Bounded ADD/SUB/INC/DEC operand differentials. These fixtures compare
//! independent local/native reads; merely constructing them is not a pass.
use super::{Case, case};

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (vec![("X1", true), ("X0", false)], 1),
        (vec![("X1", false), ("X0", true)], 2),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}

fn seed_bits(base: u16, bits: u16, value: u32) -> String {
    (0..bits)
        .map(|bit| {
            format!(
                "{} M{}",
                if value & (1u32 << bit) != 0 {
                    "SET"
                } else {
                    "RST"
                },
                base + bit,
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
    case(
        &format!("arithmetic_operand_{name}"),
        &format!(
            "LD X1\n{seed}\n{guard_seed}\nSET M1020\nSET M1021\nSET M1022\nRST M1067\nRST M1068\nLD X0\n{body}\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\n{guard_reads}\nLD M1067\nOUT Y10\nLD M1068\nOUT Y11\n{observe}\nEND"
        ),
        stages(),
    )
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let nibble_guards = [("M30", false), ("M31", true), ("M36", true), ("M37", false)];
    let word_guards = [("M30", false), ("M31", true), ("M48", true), ("M49", false)];

    // Each of the sixteen spellings writes one nibble. The neighboring bits
    // are initialized both ON and OFF and explicitly read after every scan.
    // Incrementing D0 after ADD/SUB distinguishes a held pulse from a normal
    // instruction without changing the arithmetic flags under observation.
    for wide in [false, true] {
        let width = if wide { "D" } else { "" };
        let mov = if wide { "DMOV" } else { "MOV" };
        let inc = if wide { "DINC" } else { "INC" };
        for op in ["ADD", "SUB", "INC", "DEC"] {
            for pulse in [false, true] {
                let name = format!("{width}{op}{}", if pulse { "P" } else { "" });
                let initial = match op {
                    "INC" => 15,
                    "DEC" => 0,
                    _ => 9,
                };
                let source = if op == "SUB" { 1 } else { 14 };
                let seed = format!("{}\n{mov} K{source} D0", seed_bits(32, 4, initial));
                let body = if matches!(op, "ADD" | "SUB") {
                    format!("{name} D0 K1 K1M32\n{inc} D0")
                } else {
                    format!("{name} K1M32")
                };
                cases.push(scenario(
                    &format!("lifecycle_{name}"),
                    &seed,
                    &body,
                    &nibble_guards,
                    "",
                ));
            }
        }
    }

    // Independent mathematical/storage distinctions: 15+1 is a nonzero
    // full-word result despite a zero nibble; corrected signed MAX+1 is zero
    // and carry; 65535+1 is nonzero in 32 bits despite a zero 16-bit group.
    for (name, body, bits, guards) in [
        (
            "word_nonzero_truncated_to_zero",
            "ADD K15 K1 K1M32",
            4,
            &nibble_guards,
        ),
        (
            "word_carry_truncated_to_zero",
            "ADD K32767 K1 K1M32",
            4,
            &nibble_guards,
        ),
        (
            "double_nonzero_truncated_to_zero",
            "DADD K65535 K1 K4M32",
            16,
            &word_guards,
        ),
    ] {
        cases.push(scenario(name, &seed_bits(32, bits, 9), body, guards, ""));
    }

    // K4 in a 32-bit operation is zero-extended: FFFF means 65535, not -1.
    cases.push(scenario(
        "double_short_source_zero_extension",
        &format!("{}\n{}", seed_bits(0, 16, 0xffff), seed_bits(32, 16, 9)),
        "DADD K4M0 K1 K4M32",
        &word_guards,
        "",
    ));

    // Word E1/F1 share the canonical D1182/D1183 aliases. Each action reads
    // both old source values before writing its aliased destination.
    cases.push(scenario(
        "word_EF_alias_read_write",
        "MOV K7 D1182\nMOV K3 D1183\nMOV K41 D1181\nMOV K42 D1184",
        "ADD E1 F1 E1\nSUB F1 E1 F1",
        &nibble_guards,
        "LDI X7\nMOV D1182 D20\nMOV D1183 D21\nMOV D1181 D22\nMOV D1184 D23",
    ));

    // E2 is the complete D1184/D1185 pair; the first increment carries from
    // low word FFFF into its F2 high word. Alias neighbors are also observed.
    cases.push(scenario(
        "double_E_pair_alias_carry",
        "DMOV H0001FFFF D1184\nMOV K41 D1183\nMOV K42 D1186",
        "DADD E2 K1 E2",
        &nibble_guards,
        "LDI X7\nMOV D1184 D20\nMOV D1185 D21\nMOV D1183 D22\nMOV D1186 D23",
    ));

    // The writable nibble lies inside the source word, at a different start.
    // Reading source bits after partial output writes would change the result.
    cases.push(scenario(
        "word_source_destination_bit_overlap",
        &seed_bits(32, 16, 15),
        "ADD K4M32 K1 K1M33",
        &[("M30", false), ("M31", true), ("M48", true), ("M49", false)],
        "",
    ));

    // Two sources overlap each other and the 32-bit destination. The second
    // source is only four digits and must remain zero-extended at this width.
    cases.push(scenario(
        "double_two_sources_destination_overlap",
        &format!(
            "{}\n{}",
            seed_bits(0, 32, 0x0001_0003),
            seed_bits(32, 16, 0xaaaa)
        ),
        "DSUB K8M0 K4M16 K8M16",
        &[("M15", false), ("M48", true), ("M49", false)],
        "",
    ));
    cases
}
