//! Indexed comparison and predicate differential fixtures, without BLD.
//! These are bounded input programs, not native compatibility claims.
use super::{Case, case};
use std::collections::BTreeSet;

const REQUIRED: &str = "
CMP CMPP ZCP ZCPP DCMP DCMPP DZCP DZCPP
LD= LD> LD< LD<> LD<= LD>= AND= AND> AND< AND<> AND<= AND>= OR= OR> OR< OR<> OR<= OR>=
DLD= DLD> DLD< DLD<> DLD<= DLD>= DAND= DAND> DAND< DAND<> DAND<= DAND>= DOR= DOR> DOR< DOR<> DOR<= DOR>=
LD& LD| LD^ AND& AND| AND^ OR& OR| OR^
DLD& DLD| DLD^ DAND& DAND| DAND^ DOR& DOR| DOR^
LDZ= LDZ> LDZ< LDZ<> LDZ<= LDZ>= ANDZ= ANDZ> ANDZ< ANDZ<> ANDZ<= ANDZ>= ORZ= ORZ> ORZ< ORZ<> ORZ<= ORZ>=
DLDZ= DLDZ> DLDZ< DLDZ<> DLDZ<= DLDZ>= DANDZ= DANDZ> DANDZ< DANDZ<> DANDZ<= DANDZ>= DORZ= DORZ> DORZ< DORZ<> DORZ<= DORZ>=
";
const RELATIONS: [&str; 6] = ["=", ">", "<", "<>", "<=", ">="];

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

fn seed_values(area: &str, base: u16, values: &[i32], wide: bool) -> String {
    let mov = if wide { "DMOV" } else { "MOV" };
    let stride = if wide { 2 } else { 1 };
    values
        .iter()
        .enumerate()
        .map(|(i, v)| format!("{mov} K{v} {area}{}", base + i as u16 * stride))
        .collect::<Vec<_>>()
        .join("\n")
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

fn contact(prefix: &str, name: &str, args: &str, output: usize) -> String {
    let before = match prefix {
        "LD" => "",
        "AND" => "LD X0\n",
        "OR" => "LDI X0\n",
        _ => unreachable!(),
    };
    // LD predicates still execute while disabled; their visible rung is gated.
    let after = if prefix == "LD" { "\nAND X0" } else { "" };
    format!("{before}{name} {args}{after}\nOUT Y{output:o}")
}

fn scenario(name: &str, seed: &str, advance: &str, body: &str, observe: &str) -> Case {
    let advance = if advance.is_empty() {
        String::new()
    } else {
        format!("LD X0\n{advance}")
    };
    // Advancing an index may change ALU flags. Restore a known flag pattern
    // before the tested comparison so its own preservation remains observable.
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
        "oversized indexed comparison {name}"
    );
    case(&format!("indexed_comparison_{name}"), &source, stages())
}

fn result_window(area: &str, base: u16, out: u16) -> String {
    format!("MOV K4{} D{out}", address(area, base))
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();

    // Eight action spellings, each with Y/M/S three-bit result windows. The
    // result base moves by one bit each active scan and must hold when OFF.
    for wide in [false, true] {
        let width = if wide { "D" } else { "" };
        let stride = if wide { 2 } else { 1 };
        for family in ["CMP", "ZCP"] {
            for pulse in [false, true] {
                let op = format!("{width}{family}{}", if pulse { "P" } else { "" });
                for area in ["Y", "M", "S"] {
                    let (left, right, value) = if family == "CMP" {
                        ([17, -2, 0, 2], [0, 0, 0, 0], [0, 0, 0, 0])
                    } else {
                        ([-2, -2, -2, -2], [2, 2, 2, 2], [0, -3, 0, 3])
                    };
                    let seed = format!(
                        "{}\n{}\n{}\n{}\nMOV K0 E1\nMOV H6A5A F1\nMOV H3333 E2\nMOV K-{stride} F2\nMOV K0 E3\nMOV K-3 F3",
                        seed_values("D", 80, &left, wide),
                        seed_values("D", 120 - stride, &right, wide),
                        seed_values("D", 140, &value, wide),
                        seed_bits(area, 152, 16, 0xa55a),
                    );
                    let advance = format!(
                        "ADD K{stride} E1 E1\nADD K{stride} F2 F2\nADD K{stride} E3 E3\nINC F3"
                    );
                    let args = if family == "CMP" {
                        "D80E1 D120F2"
                    } else {
                        "D80E1 D120F2 D140E3"
                    };
                    let observe = format!(
                        "{}\n{}\n{}\n{}\nMOV E1 D260\nMOV F1 D261\nMOV E2 D262\nMOV F2 D263\nMOV E3 D264\nMOV F3 D265",
                        result_window(area, 152, 200),
                        read_words("D", 80, 8, 210),
                        read_words("D", 118, 8, 220),
                        read_words("D", 140, 8, 230)
                    );
                    cases.push(scenario(
                        &format!("lifecycle_{op}_{area}"),
                        &seed,
                        &advance,
                        &format!("LD X0\n{op} {args} {}F3", address(area, 160)),
                        &observe,
                    ));
                }
            }
        }
    }
    assert_eq!(cases.len(), 24);

    // All36 signed headers. Numeric source classes rotate through T/C/D,
    // keeping low counters below200 and including signed high-half changes.
    for wide in [false, true] {
        let width = if wide { "D" } else { "" };
        let stride = if wide { 2 } else { 1 };
        for (p, prefix) in ["LD", "AND", "OR"].into_iter().enumerate() {
            for (r, relation) in RELATIONS.into_iter().enumerate() {
                let a = ["D", "T", "C"][(p + r) % 3];
                let b = ["D", "T", "C"][(p + r + 1) % 3];
                let op = format!("{width}{prefix}{relation}");
                let seed = format!(
                    "{}\n{}\nMOV K0 E1\nMOV H6A5A F1\nMOV H1234 E2\nMOV K-{stride} F2",
                    seed_values(
                        a,
                        80,
                        &[
                            7,
                            if wide { -70000 } else { -1 },
                            0,
                            if wide { 70000 } else { 1 }
                        ],
                        wide
                    ),
                    seed_values(b, 120 - stride, &[0, 0, 0, 0], wide)
                );
                let observe = format!(
                    "{}\n{}\nMOV E1 D250\nMOV F1 D251\nMOV E2 D252\nMOV F2 D253",
                    read_words(a, 80, 8, 200),
                    read_words(b, 118, 8, 220)
                );
                cases.push(scenario(
                    &format!("signed_{op}"),
                    &seed,
                    &format!("ADD K{stride} E1 E1\nADD K{stride} F2 F2"),
                    &contact(prefix, &op, &format!("{a}80E1 {b}120F2"), 0),
                    &observe,
                ));
            }
        }
    }
    assert_eq!(cases.len(), 60);

    // All18 bitwise headers; every legal Kn width appears, including zero
    // extension of short groups in32-bit predicates. Offsets count bits.
    for wide in [false, true] {
        let width = if wide { "D" } else { "" };
        let max = if wide { 8 } else { 4 };
        for (p, prefix) in ["LD", "AND", "OR"].into_iter().enumerate() {
            for (r, relation) in ["&", "|", "^"].into_iter().enumerate() {
                let ordinal = p * 3 + r;
                let a = ["M", "S", "Y"][ordinal % 3];
                let b = ["M", "S", "Y"][(ordinal + 1) % 3];
                let n = ordinal % max + 1;
                let m = (ordinal + 2) % max + 1;
                let op = format!("{width}{prefix}{relation}");
                let seed = format!(
                    "{}\n{}\nMOV K-1 E1\nMOV H6A5A F1\nMOV H3333 E2\nMOV K-2 F2",
                    seed_bits(a, 60, 40, 0x55_5555_5555),
                    seed_bits(b, 108, 40, 0xaa_aaaa_aaaa)
                );
                let args = format!("K{n}{}E1 K{m}{}F2", address(a, 64), address(b, 112));
                let observe = format!(
                    "DMOV K8{} D200\nMOV K2{} D202\nDMOV K8{} D210\nMOV K2{} D212\nMOV E1 D220\nMOV F1 D221\nMOV E2 D222\nMOV F2 D223",
                    address(a, 60),
                    address(a, 92),
                    address(b, 108),
                    address(b, 140)
                );
                cases.push(scenario(
                    &format!("logic_{op}_K{n}_K{m}"),
                    &seed,
                    "INC E1\nADD K2 F2 F2",
                    &contact(prefix, &op, &args, 0),
                    &observe,
                ));
            }
        }
    }
    assert_eq!(cases.len(), 78);

    // All36 absolute headers have three independently indexed numeric roles.
    // Direct E/F operands are intentionally absent from this family.
    for wide in [false, true] {
        let width = if wide { "D" } else { "" };
        let stride = if wide { 2 } else { 1 };
        for (p, prefix) in ["LD", "AND", "OR"].into_iter().enumerate() {
            for (r, relation) in RELATIONS.into_iter().enumerate() {
                let a = ["D", "T", "C"][(p + r) % 3];
                let b = ["D", "T", "C"][(p + r + 1) % 3];
                let c = ["D", "T", "C"][(p + r + 2) % 3];
                let op = format!("{width}{prefix}Z{relation}");
                let seed = format!(
                    "{}\n{}\n{}\nMOV K0 E1\nMOV H6A5A F1\nMOV H3333 E2\nMOV K-{stride} F2\nMOV K0 E3\nMOV H4567 F3",
                    seed_values(a, 40, &[7, -2, -1, 0], wide),
                    seed_values(b, 80 - stride, &[0, 0, 0, 0], wide),
                    seed_values(c, 120, &[1, -1, -1, -1], wide)
                );
                let args = format!("{a}40E1 {b}80F2 {c}120E3");
                let observe = format!(
                    "{}\n{}\n{}\nMOV E1 D260\nMOV F2 D261\nMOV E3 D262",
                    read_words(a, 40, 8, 200),
                    read_words(b, 78, 8, 220),
                    read_words(c, 120, 8, 240)
                );
                cases.push(scenario(
                    &format!("absolute_{op}"),
                    &seed,
                    &format!("ADD K{stride} E1 E1\nADD K{stride} F2 F2\nADD K{stride} E3 E3"),
                    &contact(prefix, &op, &args, 0),
                    &observe,
                ));
            }
        }
    }
    assert_eq!(cases.len(), 114);

    // Every E/F index word is used at both widths, with a poisoned companion
    // word. Three changes in one scan also drive three-bit indexed targets.
    for bank in ['E', 'F'] {
        for index in 0..8u16 {
            let base = if index == 0 { 1028 } else { 1180 + index * 2 };
            let alias = base + u16::from(bank == 'F');
            let other = base + u16::from(bank == 'E');
            let modifier = format!("{bank}{index}");
            for wide in [false, true] {
                let width = if wide { "D" } else { "" };
                let mov = if wide { "DMOV" } else { "MOV" };
                let seed = format!(
                    "{mov} K-2 D79\n{mov} K0 D119\n{mov} K3 D81\n{mov} K3 D121\n{mov} K-7 D83\n{mov} K1 D123\n{mov} K8 D143\nMOV H6A5A D{other}\nMOV K-1 D{alias}\n{}",
                    seed_bits("M", 156, 16, 0xa55a)
                );
                let body = [
                    format!("LD X0\nMOV K-1 {modifier}"),
                    contact("LD", &format!("{width}LD<"), &format!("D80{modifier} D120{modifier}"), 0),
                    format!("LD X0\n{width}CMP D80{modifier} D120{modifier} M160{modifier}\nMOV K1 {modifier}"),
                    contact("LD", &format!("{width}LD="), &format!("D80{modifier} D120{modifier}"), 1),
                    format!("LD X0\n{width}CMP D80{modifier} D120{modifier} M160{modifier}\nMOV K3 {modifier}"),
                    contact("LD", &format!("{width}LDZ="), &format!("D80{modifier} D120{modifier} D140{modifier}"), 2),
                    format!("LD X0\n{width}CMP D80{modifier} D120{modifier} M160{modifier}"),
                ].join("\n");
                cases.push(scenario(&format!("all_indices_{width}_{modifier}"), &seed, "", &body,
                    &format!("MOV K4M156 D200\nMOV E{index} D210\nMOV F{index} D211\nMOV D{base} D212\nMOV D{} D213", base + 1)));
            }
        }
    }
    assert_eq!(cases.len(), 146);

    // All permitted source classes occupy each of the three absolute roles,
    // and both comparison-action sources, including KnX in the third role.
    for area in ["D", "T", "C", "X", "Y", "M", "S"] {
        for wide in [false, true] {
            let width = if wide { "D" } else { "" };
            let numeric = matches!(area, "D" | "T" | "C");
            let digits = if wide { 8 } else { 4 };
            let (seed, a, b, c, observe) = if numeric {
                (
                    format!(
                        "{}\n{}\n{}",
                        seed_words(area, 38, 12, 0x8a12),
                        seed_words(area, 78, 12, 0x1357),
                        seed_words(area, 118, 12, 0x2468)
                    ),
                    format!("{area}40E1"),
                    format!("{area}80F2"),
                    format!("{area}120E3"),
                    format!(
                        "{}\n{}\n{}",
                        read_words(area, 38, 12, 200),
                        read_words(area, 78, 12, 220),
                        read_words(area, 118, 12, 240)
                    ),
                )
            } else {
                (
                    if area == "X" {
                        String::new()
                    } else {
                        format!(
                            "{}\n{}",
                            seed_bits(area, 12, 48, 0xa55a_1234_5678),
                            seed_bits(area, 60, 64, 0x1357_9bdf_2468_ace0)
                        )
                    },
                    format!("K{digits}{}E1", address(area, 16)),
                    format!("K{digits}{}F2", address(area, 48)),
                    format!("K{digits}{}E3", address(area, 80)),
                    format!(
                        "DMOV K8{} D200\nDMOV K8{} D202\nDMOV K8{} D204\nMOV K4{} D206",
                        address(area, 12),
                        address(area, 44),
                        address(area, 76),
                        address(area, 108)
                    ),
                )
            };
            let seed = format!(
                "{seed}\nMOV K1 E1\nMOV H6A5A F1\nMOV H3333 E2\nMOV K-1 F2\nMOV K2 E3\nMOV K-1 F3\n{}",
                seed_bits("M", 156, 28, 0x0a55_a55a)
            );
            let body = [
                contact("LD", &format!("{width}LD>="), &format!("{a} {b}"), 0),
                contact("LD", &format!("{width}LD&"), &format!("{a} {b}"), 1),
                contact("LD", &format!("{width}LDZ<="), &format!("{a} {b} {c}"), 2),
                format!("LD X0\n{width}CMP {a} {b} M160F3\n{width}ZCP {a} {b} {c} M168F3"),
            ]
            .join("\n");
            let mut item = scenario(
                &format!("all_source_roles_{width}_{area}"),
                &seed,
                "INC E1\nINC F2\nINC E3",
                &body,
                &format!(
                    "{observe}\nMOV K4M156 D270\nMOV K3M172 D271\nMOV E1 D280\nMOV F2 D281\nMOV E3 D282"
                ),
            );
            if area == "X" {
                item.stages[0].0.extend([
                    ("X21", true),
                    ("X27", true),
                    ("X44", true),
                    ("X60", true),
                    ("X77", true),
                    ("X101", true),
                    ("X123", true),
                    ("X145", true),
                    ("X157", true),
                    ("X163", true),
                ]);
            }
            cases.push(item);
        }
    }
    assert_eq!(cases.len(), 160);

    // Exact signed minima/maxima, wrapped subtraction and negative/minimum
    // thresholds. Every relation and LD/AND/OR composition is visible.
    for wide in [false, true] {
        let width = if wide { "D" } else { "" };
        let mov = if wide { "DMOV" } else { "MOV" };
        let (min, max) = if wide {
            (i32::MIN, i32::MAX)
        } else {
            (i16::MIN as i32, i16::MAX as i32)
        };
        for (edge, (a, b, c)) in [(min, 0, 0), (max, min, 1), (min, max, 1), (0, 0, min)]
            .into_iter()
            .enumerate()
        {
            let seed = format!(
                "{mov} K{a} D81\n{mov} K{b} D119\n{mov} K{c} D142\nMOV H6A5A E0\nMOV K1 F0\nMOV K-1 E1\nMOV K2 F1"
            );
            let mut body = Vec::new();
            for (p, prefix) in ["LD", "AND", "OR"].into_iter().enumerate() {
                for (r, relation) in RELATIONS.into_iter().enumerate() {
                    body.push(contact(
                        prefix,
                        &format!("{width}{prefix}Z{relation}"),
                        "D80F0 D120E1 D140F1",
                        p * 6 + r,
                    ));
                }
            }
            cases.push(scenario(
                &format!("absolute_edges_{width}_{edge}"),
                &seed,
                "",
                &body.join("\n"),
                "DMOV D81 D240\nDMOV D119 D242\nDMOV D142 D244\nMOV E1 D250\nMOV F1 D251",
            ));
        }
    }
    assert_eq!(cases.len(), 168);

    for wide in [false, true] {
        let width = if wide { "D" } else { "" };
        let stride = if wide { 2 } else { 1 };
        for pulse in [false, true] {
            let op = format!("{width}ZCP{}", if pulse { "P" } else { "" });
            let seed = format!(
                "{}\n{}\n{}\n{}\nMOV K0 E1\nMOV H6A5A F1\nMOV H3333 E2\nMOV K-{stride} F2\nMOV K0 E3\nMOV K-3 F3",
                seed_values("D", 80, &[5, 5, 5, 5], wide),
                seed_values("D", 120 - stride, &[-5, -5, -5, -5], wide),
                seed_values("D", 140, &[0, -6, 0, 6], wide),
                seed_bits("M", 152, 16, 0xa55a)
            );
            cases.push(scenario(
                &format!("reversed_bounds_{op}"),
                &seed,
                &format!("ADD K{stride} E1 E1\nADD K{stride} F2 F2\nADD K{stride} E3 E3\nINC F3"),
                &format!("LD X0\n{op} D80E1 D120F2 D140E3 M160F3"),
                "MOV K4M152 D200\nMOV E1 D210\nMOV F2 D211\nMOV E3 D212\nMOV F3 D213",
            ));
        }
    }
    assert_eq!(cases.len(), 172);

    // Source snapshots are taken before the overlapping three-bit action.
    // CMP overlaps S1; ZCP separately overlaps each source at both widths/P.
    for wide in [false, true] {
        let width = if wide { "D" } else { "" };
        let digits = if wide { 8 } else { 4 };
        let bits = if wide { 32 } else { 16 };
        let mov = if wide { "DMOV" } else { "MOV" };
        for family in ["CMP", "ZCP"] {
            for pulse in [false, true] {
                let op = format!("{width}{family}{}", if pulse { "P" } else { "" });
                let targets: &[&str] = if family == "CMP" {
                    &["M64E1"]
                } else {
                    &["M64E1", "M112F2", "M160E3"]
                };
                for (role, target) in targets.iter().enumerate() {
                    let seed = format!(
                        "{}\n{}\n{}\n{}\n{}\n{}\nMOV K1 E1\nMOV H6A5A F1\nMOV H3333 E2\nMOV K-1 F2\nMOV K2 E3\nMOV H4567 F3",
                        seed_bits("M", 60, 40, 0xa5_a55a_a55a),
                        seed_bits("M", 108, 40, 0x5a_5aa5_5aa5),
                        seed_bits("M", 158, 40, 0xa5_9696_6996),
                        seed_bits("M", 65, bits, 1),
                        seed_bits("M", 111, bits, 1),
                        seed_bits("M", 162, bits, 1)
                    );
                    let args = if family == "CMP" {
                        format!("K{digits}M64E1 K{digits}M112F2")
                    } else {
                        format!("K{digits}M64E1 K{digits}M112F2 K{digits}M160E3")
                    };
                    let body = format!(
                        "LD X0\n{mov} K{digits}M65 D200\n{mov} K{digits}M111 D202\n{mov} K{digits}M162 D204\n{op} {args} {target}"
                    );
                    cases.push(scenario(&format!("overlap_{op}_source_{role}"), &seed, "", &body,
                        "DMOV K8M60 D220\nMOV K2M92 D222\nDMOV K8M108 D230\nMOV K2M140 D232\nDMOV K8M158 D240\nMOV K2M190 D242\nMOV E1 D250\nMOV F2 D251\nMOV E3 D252"));
                }
            }
        }
    }
    assert_eq!(cases.len(), 188);

    // Indexed wide counter resolution must follow the effective counter's
    // storage class: C199 is low16 with high0; C200 is a natural32 register.
    for base in [199, 200] {
        for modifier in ["E0", "F0"] {
            let seed = format!(
                "MOV H8000 C198\nMOV HFFFF C199\nDMOV H80000000 C200\nDMOV HFFFFFFFF C201\nDMOV K1 C202\nMOV H6A5A E0\nMOV H3333 F0\nMOV K-1 {modifier}\n{}",
                seed_bits("M", 156, 16, 0xa55a)
            );
            let mut body = Vec::new();
            for (phase, offset) in [-1, 0, 1].into_iter().enumerate() {
                body.push(format!("LD X0\nMOV K{offset} {modifier}"));
                let source = format!("C{base}{modifier}");
                body.push(contact(
                    "LD",
                    "DLD=",
                    &format!("{source} K65535"),
                    phase * 4,
                ));
                body.push(contact(
                    "LD",
                    "DLD<",
                    &format!("{source} K0"),
                    phase * 4 + 1,
                ));
                body.push(contact(
                    "LD",
                    "DLD&",
                    &format!("{source} H80000000"),
                    phase * 4 + 2,
                ));
                body.push(contact(
                    "LD",
                    "DLDZ=",
                    &format!("K0 K0 {source}"),
                    phase * 4 + 3,
                ));
                body.push(format!("LD X0\nDCMP {source} K0 M160{modifier}"));
            }
            cases.push(scenario(&format!("counter_transition_C{base}_{modifier}"), &seed, "", &body.join("\n"),
                "MOV K4M156 D200\nDMOV C198 D240\nDMOV C199 D242\nDMOV C200 D244\nDMOV C201 D246\nDMOV C202 D248"));
        }
    }
    assert_eq!(cases.len(), 192);

    // An indexed action selects source-VM execution for the whole program.
    // These ordinary word ORZ contacts must retain their native six-padding-
    // word shortcut metadata there too. Opposite X0 gates exercise both paths
    // in every scan; D0 changes only after both contacts, so the first and
    // second active visits also distinguish evaluated false from true.
    for (relation, initial, changed) in [("=", 1, 0), ("<>", 0, 1)] {
        let seed = format!(
            "MOV K{initial} D0\nMOV K0 D20\nMOV K0 D21\nMOV K0 D22\nMOV K5 D81\nMOV K1 E1\nMOV H6A5A F1\nMOV H3333 E2\nMOV K-1 F2\n{}",
            seed_bits("M", 156, 16, 0xa55a)
        );
        let body = format!(
            "LD X0\nCMP D80E1 K5 M160F2\nLD X0\nORZ{relation} D0 K0 K0\nOUT Y0\nINC D20\nLDI X0\nORZ{relation} D0 K0 K0\nOUT Y1\nINC D22\nLD X0\nORI X0\nINC D21\nLD X0\nMOV K{changed} D0"
        );
        cases.push(scenario(
            &format!("ordinary_ORZ{relation}_shortcut_in_indexed_vm"),
            &seed,
            "",
            &body,
            "MOV D0 D200\nMOV D20 D201\nMOV D21 D202\nMOV D22 D203\nMOV K4M156 D204\nMOV E1 D210\nMOV F1 D211\nMOV E2 D212\nMOV F2 D213",
        ));
    }
    assert_eq!(cases.len(), 194);

    let required: BTreeSet<_> = REQUIRED.split_whitespace().collect();
    assert_eq!(required.len(), 98);
    let actual: BTreeSet<_> = cases
        .iter()
        .flat_map(|item| item.source.lines())
        .filter_map(|line| line.split_whitespace().next())
        .filter(|op| required.contains(op))
        .collect();
    assert_eq!(actual, required, "missing indexed comparison header");
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
