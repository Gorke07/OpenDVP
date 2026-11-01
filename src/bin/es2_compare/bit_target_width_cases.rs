//! Bare-bit destination width regressions for the audited native handlers.
//! Explicit observations include both result bits and every surrounding bit.
use super::{Case, case};
use std::collections::BTreeSet;

const REQUIRED_SPELLINGS: &str =
    "TCMP TCMPP TZCP TZCPP DECMP DECMPP DEZCP DEZCPP BON BONP DBON DBONP";

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (vec![("X1", true), ("X0", false)], 1),
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

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for operation in ["TCMP", "TZCP", "DECMP", "DEZCP", "BON", "DBON"] {
        for pulse in [false, true] {
            let name = format!("{operation}{}", if pulse { "P" } else { "" });
            for (area, base) in [("Y", 128u16), ("M", 80), ("S", 80)] {
                let target = address(area, base);
                // -1..+8 includes the intended 1/3-bit destination and both
                // sides of the erroneous eight-bit native write. Alternating
                // guards include ON bits at +1,+3,+5,+7 and the outer +8 bit.
                let neighborhood = (base - 1..=base + 8)
                    .map(|index| {
                        let offset = i32::from(index) - i32::from(base);
                        let on = offset % 2 != 0 || offset == 8;
                        (address(area, index), on)
                    })
                    .collect::<Vec<_>>();
                let guard_seed = neighborhood
                    .iter()
                    .map(|(device, on)| format!("{} {device}", if *on { "SET" } else { "RST" }))
                    .collect::<Vec<_>>()
                    .join("\n");
                let observed = neighborhood
                    .iter()
                    .enumerate()
                    .map(|(index, (device, _))| format!("LD {device}\nOUT Y{index:o}"))
                    .collect::<Vec<_>>()
                    .join("\n");

                let (seed, operands, advance) = match operation {
                    // Current time visits 12:20:44, :45, :46. All triples
                    // remain valid; the target must change through all three
                    // comparison outcomes for the non-pulse instruction.
                    "TCMP" => (
                        "MOV K12 D20\nMOV K20 D21\nMOV K44 D22".to_owned(),
                        format!("K12 K20 K45 D20 {target}"),
                        "INC D22".to_owned(),
                    ),
                    // Range 12:20:45..12:20:46; current visits :44,:46,:48.
                    "TZCP" => (
                        "MOV K12 D0\nMOV K20 D1\nMOV K45 D2\nMOV K12 D10\nMOV K20 D11\nMOV K46 D12\nMOV K12 D20\nMOV K20 D21\nMOV K44 D22".to_owned(),
                        format!("D0 D10 D20 {target}"),
                        "INC D22\nINC D22".to_owned(),
                    ),
                    // Finite literal binary32 values avoid depending on an
                    // unrelated floating arithmetic instruction to advance.
                    // INC executes under X0; the subsequent source rewrites
                    // leave the just-observed comparison result intact.
                    "DECMP" => (
                        "DMOV HBF800000 D0\nDMOV H00000000 D4\nMOV K0 D10".to_owned(),
                        format!("D0 D4 {target}"),
                        "INC D10\nLD= D10 K1\nDMOV H00000000 D0\nLD= D10 K2\nDMOV H3F800000 D0".to_owned(),
                    ),
                    "DEZCP" => (
                        "DMOV HBF800000 D0\nDMOV H00000000 D4\nDMOV H3F800000 D8\nMOV K0 D10".to_owned(),
                        format!("D4 D8 D0 {target}"),
                        "INC D10\nLD= D10 K1\nDMOV H3F800000 D0\nLD= D10 K2\nDMOV H40000000 D0".to_owned(),
                    ),
                    // The highest bit changes ON/OFF/ON across active scans.
                    // BON's one result bit must never clear adjacent +1..+7.
                    "BON" => (
                        "MOV H8000 D0".to_owned(),
                        format!("D0 {target} K15"),
                        "WXOR D0 H8000 D0".to_owned(),
                    ),
                    "DBON" => (
                        "DMOV H80000000 D0".to_owned(),
                        format!("D0 {target} K31"),
                        "DXOR D0 H80000000 D0".to_owned(),
                    ),
                    _ => unreachable!(),
                };
                cases.push(case(
                    &format!("bit_target_width_{name}_{target}"),
                    &format!(
                        "LD X1\n{seed}\n{guard_seed}\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\nLD X0\n{name} {operands}\n{advance}\n{observed}\nLD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24\nLD X0\nORI X0\nMOV D1067 D300\nMOV D1068 D301\nEND"
                    ),
                    stages(),
                ));
            }
        }
    }
    let required: BTreeSet<_> = REQUIRED_SPELLINGS.split_whitespace().collect();
    assert_eq!(required.len(), 12, "bit-target fixture inventory changed");
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
        "bit-target fixture spellings missing: {missing:?}"
    );
    assert_eq!(cases.len(), 36, "bit-target fixture case count changed");
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
