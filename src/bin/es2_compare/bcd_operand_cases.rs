//! Bounded BCD/BIN operand differentials. Only the runner's independent
//! local/native observations establish compatibility for these fixtures.
use super::{Case, case};
use std::collections::BTreeSet;

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
        .map(|(index, (name, _))| format!("LD {name}\nOUT Y{:o}", index + 5))
        .collect::<Vec<_>>()
        .join("\n");
    case(
        &format!("bcd_operand_{name}"),
        &format!(
            "LD X1\n{seed}\n{guard_seed}\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\nLD X0\n{body}\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\n{guard_reads}\nLD X0\nORI X0\nMOV D1067 D300\nMOV D1068 D301\n{observe}\nEND"
        ),
        stages(),
    )
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();

    // Eight spellings x three writable bit areas. BCD sees 28,29,30; BIN
    // sees packed 20,21,22. Sources advance after the tested action, so a
    // held pulse and a normal instruction have independently visible results.
    for wide in [false, true] {
        let width = if wide { "D" } else { "" };
        let bits = if wide { 32 } else { 16 };
        for op in ["BCD", "BIN"] {
            for pulse in [false, true] {
                let mnemonic = format!("{width}{op}{}", if pulse { "P" } else { "" });
                for (area, base) in [("Y", 128), ("M", 192), ("S", 192)] {
                    let target = address(area, base);
                    let names = [
                        address(area, base - 2),
                        address(area, base - 1),
                        address(area, base + 4),
                        address(area, base + 5),
                    ];
                    let guards = [
                        (names[0].as_str(), false),
                        (names[1].as_str(), true),
                        (names[2].as_str(), true),
                        (names[3].as_str(), false),
                    ];
                    cases.push(scenario(
                        &format!("lifecycle_{mnemonic}_{target}"),
                        &format!(
                            "{}\n{}",
                            seed_bits("M", 64, bits, if op == "BCD" { 28 } else { 0x20 }),
                            seed_bits(area, base, 4, 5),
                        ),
                        &format!(
                            "{mnemonic} K{}M64 K1{target}\n{width}INC K{}M64",
                            bits / 4,
                            bits / 4
                        ),
                        &guards,
                        "",
                    ));
                }
            }
        }
    }

    // Maximum valid values convert; the next positive value and negative
    // input leave their destinations untouched and latch the first error PC.
    cases.push(scenario(
        "word_bcd_limits_and_invalid_target_hold",
        &format!(
            "{}\n{}\n{}\nMOV K-1 D1182\nMOV H55AA D1183",
            seed_bits("M", 64, 16, 9999),
            seed_bits("M", 96, 16, 10000),
            seed_bits("M", 192, 4, 5)
        ),
        "BCD K4M64 D20\nBCD K4M96 K1M192\nBCD E1 F1",
        &[("M191", true), ("M196", false)],
        "MOV D1182 D30\nMOV D1183 D31",
    ));
    cases.push(scenario(
        "double_bcd_limits_and_invalid_target_hold",
        &format!(
            "{}\n{}\n{}\nDMOV K-1 D1182",
            seed_bits("M", 64, 32, 99_999_999),
            seed_bits("M", 112, 32, 100_000_000),
            seed_bits("M", 192, 16, 0x55aa)
        ),
        "DBCD K8M64 E2\nDBCD K8M112 K4M192\nDBCD E1 E1",
        &[("M191", true), ("M208", false)],
        "DMOV D1182 D30\nDMOV D1184 D32",
    ));

    // Invalid digits in interior/high nibbles must fail even for a narrow
    // target; a later valid conversion still runs while the error is latched.
    cases.push(scenario(
        "word_invalid_bcd_digits_and_later_valid_conversion",
        &format!(
            "{}\n{}\n{}\nMOV HFADE D1182\nMOV H55AA D1183",
            seed_bits("M", 64, 16, 0x9a99),
            seed_bits("M", 96, 16, 0x9999),
            seed_bits("M", 192, 4, 5)
        ),
        "BIN K4M64 K1M192\nBIN E1 F1\nBIN K4M96 D20",
        &[("M191", true), ("M196", false)],
        "MOV D1182 D30\nMOV D1183 D31",
    ));
    cases.push(scenario(
        "double_invalid_bcd_digits_and_later_valid_conversion",
        &format!(
            "{}\n{}\n{}\nDMOV HFFFFFFFF D1182",
            seed_bits("M", 64, 32, 0x9a99_9999),
            seed_bits("M", 112, 32, 0x9999_9999),
            seed_bits("M", 192, 4, 5)
        ),
        "DBIN K8M64 K1M192\nDBIN E1 E1\nDBIN K8M112 E2",
        &[("M191", true), ("M196", false)],
        "DMOV D1182 D30\nDMOV D1184 D32",
    ));

    // K4 FFFF is positive 65535 at double width. Narrow destination writes
    // occur only after full source validation/conversion, preserving guards.
    cases.push(scenario(
        "short_sources_zero_extend_before_conversion_and_partial_write",
        &format!(
            "{}\n{}\n{}\n{}",
            seed_bits("M", 64, 4, 15),
            seed_bits("M", 80, 4, 9),
            seed_bits("M", 96, 16, 0x9999),
            seed_bits("M", 128, 16, 0xffff)
        ),
        "BCD K1M64 K1M192\nBIN K1M80 E1\nDBIN K4M96 K1S192\nDBCD K4M128 K4Y200",
        &[
            ("M191", true),
            ("M196", false),
            ("S191", true),
            ("S196", false),
            ("Y177", true),
            ("Y220", false),
        ],
        "MOV D1182 D30",
    ));
    cases.push(scenario(
        "word_E_F_alias_roundtrip",
        "MOV K1234 D1182\nMOV H55AA D1183\nMOV K41 D1181\nMOV K42 D1184",
        "BCD E1 F1\nBIN F1 E1",
        &[],
        "MOV D1182 D30\nMOV D1183 D31\nMOV D1181 D32\nMOV D1184 D33",
    ));
    cases.push(scenario(
        "double_E_pair_alias_roundtrip",
        "DMOV K12345678 D1182\nMOV K41 D1181\nMOV K42 D1184",
        "DBCD E1 E1\nDBIN E1 E1",
        &[],
        "DMOV D1182 D30\nMOV D1181 D32\nMOV D1184 D33",
    ));
    cases.push(scenario(
        "shifted_source_destination_groups_snapshot",
        &format!(
            "{}\n{}\n{}\n{}",
            seed_bits("M", 64, 16, 1234),
            seed_bits("M", 80, 4, 5),
            seed_bits("M", 128, 32, 0x1234_5678),
            seed_bits("M", 160, 4, 5)
        ),
        "BCD K4M64 K4M68\nDBIN K8M128 K8M132",
        &[
            ("M63", true),
            ("M84", false),
            ("M127", true),
            ("M164", false),
        ],
        "",
    ));
    cases.push(scenario(
        "natural_double_counter_and_word_storage",
        "DMOV K99999999 C200\nDMOV K41 C201\nDMOV K42 C254\nMOV K9999 C199\nMOV K43 T254",
        "DBCD C200 C255\nDBIN C255 C200\nBCD C199 T255\nBIN T255 C199",
        &[],
        "DMOV C200 D20\nDMOV C201 D22\nDMOV C254 D24\nDMOV C255 D26\nMOV C199 D28\nMOV T254 D29\nMOV T255 D30",
    ));

    // X20 is varied through the external input stages; KnY and KnS sources
    // exercise their own bit areas rather than projecting all groups to M.
    let mut input_case = scenario(
        "input_output_and_step_bit_sources",
        &format!(
            "{}\n{}",
            seed_bits("Y", 128, 16, 0x1234),
            seed_bits("S", 128, 32, 12_345_678)
        ),
        "BCD K1X20 K1M192\nDBIN K1X20 E1\nBIN K4Y200 E2\nDBCD K8S128 E3",
        &[("M191", true), ("M196", false)],
        "DMOV D1182 D30\nMOV D1184 D32\nDMOV D1186 D34",
    );
    for (index, (inputs, _)) in input_case.stages.iter_mut().enumerate() {
        inputs.push(("X20", index % 2 == 1));
        inputs.push(("X21", index == 3));
    }
    cases.push(input_case);

    // The second ON scan produces an error. OFF restores a valid source;
    // the next successful conversion must preserve both latched error words.
    cases.push(scenario(
        "successful_recovery_preserves_latched_error",
        "MOV K9999 D1182\nMOV H55AA D1183",
        "BCD E1 F1\nINC E1\nLDI X0\nANI X1\nMOV K5 D1182",
        &[],
        "MOV D1182 D30\nMOV D1183 D31",
    ));
    // Invalid sources are installed immediately after the first pulse. The
    // held scan must not evaluate them; the next rising edge must report them.
    cases.push(scenario(
        "pulse_skips_invalid_held_source_then_errors_on_next_edge",
        "MOV K1234 D1182\nMOV H55AA D1183\nDMOV K12345678 D1184\nDMOV H55AA55AA D1186",
        "BCDP E1 F1\nMOV K10000 D1182\nDBCDP E2 E3\nDMOV K100000000 D1184",
        &[],
        "MOV D1183 D30\nDMOV D1186 D32",
    ));

    let required: BTreeSet<_> = "BCD BCDP BIN BINP DBCD DBCDP DBIN DBINP"
        .split_whitespace()
        .collect();
    let actual: BTreeSet<_> = cases
        .iter()
        .flat_map(|entry| {
            entry
                .source
                .lines()
                .filter_map(|line| line.split_whitespace().next())
        })
        .collect();
    let missing: Vec<_> = required.difference(&actual).copied().collect();
    assert!(
        missing.is_empty(),
        "BCD/BIN fixture spellings missing: {missing:?}"
    );
    assert_eq!(cases.len(), 36, "BCD/BIN fixture case count changed");
    assert_eq!(
        cases
            .iter()
            .map(|entry| &entry.name)
            .collect::<BTreeSet<_>>()
            .len(),
        36
    );
    cases
}
