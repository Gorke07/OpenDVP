//! Genuine C descriptors and bounded physical halfword spans for matrix handlers.
//! Direct binary-natural fixtures are retained diagnostics; actual first runs
//! proved hang/state corruption, and the default suite withholds those sources.
//! C199 -> C200 low/high is safe. C255+n3 would enter D0 and is NOT a native case.
use super::{Case, case};
use opendvp::es2_bridge::Target;

pub fn unsafe_installed_case(name: &str) -> bool {
    name.starts_with("matrix_counter_")
        && ((name.contains("_natural_silent_") && !name.contains("MINV"))
            || name.starts_with("matrix_counter_scratch_first_natural_holds")
            || name.starts_with("matrix_counter_scratch_group_first_second_natural")
            || name.starts_with("matrix_counter_scratch_group_first_dest_natural"))
}

fn initial(pointer: i16, settings: &[(u16, bool)]) -> String {
    let mut s = String::from(
        "MOV H2468 D600\nMOV H1357 D601\nBMOV D600 C198 K2\nDMOV H89ABCDEF C200\nDMOV HCAFEBABE C201\nDMOV H76543210 C255\n",
    );
    for (base, words) in [
        (300, [0xaaaa, 0x5555, 0x0f0f, 0x8001]),
        (320, [0x3333, 0xcccc, 0x1234, 0xfedc]),
    ] {
        for (offset, value) in words.into_iter().enumerate() {
            s.push_str(&format!("MOV H{value:04X} D{}\n", base + offset));
        }
    }
    s.push_str("FMOV K-777 D400 K8\nFMOV K-777 D100 K6\nMOV K30 D0\nMOV K10 D1\nMOV K20 D2\nMOV K3 D3\nMOV K1 D4\nMOV K2 D5\nMOV K7 D750\n");
    s.push_str(&format!("MOV K{pointer} D500\n"));
    for n in 256..272 {
        s.push_str(&format!(
            "{} M{n}\n",
            if n % 3 == 0 { "RST" } else { "SET" }
        ));
    }
    for n in 1088..=1099 {
        s.push_str(&format!(
            "{} M{n}\n",
            if settings.iter().any(|(flag, on)| *flag == n && *on) {
                "SET"
            } else {
                "RST"
            }
        ));
    }
    s.push_str("SET M1020\nSET M1021\nSET M1022\nSET M1029\n");
    s
}
fn readback() -> &'static str {
    // DMOV observes each complete natural counter after possible halfword writes.
    "LD X3\nMOV C199 D700\nDMOV C198 D702\nDMOV C200 D704\nDMOV C201 D706\nDMOV C255 D708\nEND"
}
fn staged(name: &str, body: &str, pointer: i16, flags: &[(u16, bool)]) -> Case {
    case(
        &format!("matrix_counter_{name}"),
        &format!(
            "LD X1\n{}LD X0\n{body}\nMOV K123 D900\n{}",
            initial(pointer, flags),
            readback()
        ),
        vec![
            (vec![("X1", true), ("X0", false), ("X3", true)], 1),
            (vec![("X1", false), ("X0", true)], 3),
            (vec![("X0", false)], 1),
            (vec![("X0", true)], 3),
        ],
    )
}
fn scratch(name: &str, body: &str) -> Case {
    case(
        &format!("matrix_counter_scratch_{name}"),
        &format!(
            "LD X1\n{}LD X2\nSER D750 K-253 D760 K1\n{body}\nLD X0\nSORT D0 K3 K2 D100 K2\nMOV K123 D900\n{}",
            initial(0, &[]),
            readback()
        ),
        vec![
            (
                vec![("X1", true), ("X0", false), ("X2", false), ("X3", true)],
                1,
            ),
            (vec![("X1", false), ("X0", true)], 1),
            (vec![("X2", true)], 4),
            (vec![("X2", false)], 2),
            (vec![("X0", false)], 1),
            (vec![("X0", true), ("X2", true)], 3),
        ],
    )
}

pub(super) fn cases(_target: Target) -> Vec<Case> {
    let mut out = Vec::new();
    for pulse in [false, true] {
        let suffix = if pulse { "P" } else { "" };
        // Ordinary low-counter descriptors reach natural counters by physical
        // +2byte row traversal. Each output span stops before C255 high +1.
        for family in ["MAND", "MOR", "MXOR", "MXNR", "MINV"] {
            let name = format!("{family}{suffix}");
            let source = if family == "MINV" {
                format!("{name} C199 D400 K4")
            } else {
                format!("{name} C199 D300 D400 K4")
            };
            out.push(staged(
                &format!("{name}_C199_cross_source"),
                &source,
                0,
                &[],
            ));
            let dest = if family == "MINV" {
                format!("{name} D300 C199 K4")
            } else {
                format!("{name} D300 D320 C199 K4")
            };
            out.push(staged(&format!("{name}_C199_cross_dest"), &dest, 0, &[]));
            let alias = if family == "MINV" {
                format!("{name} C199 C199 K4")
            } else {
                format!("{name} C199 D300 C199 K4")
            };
            out.push(staged(&format!("{name}_C199_cross_alias"), &alias, 0, &[]));
        }
        out.push(staged(
            &format!("MINV{suffix}_C198_two_low_then_natural"),
            &format!("MINV{suffix} C198 D400 K4"),
            0,
            &[],
        ));
        // Direct natural starts are accepted by these six address-based bodies.
        // Two rows at C255 are its low/high halves, not C255/C256.
        for base in ["C200", "C255"] {
            for family in ["MCMP", "MBRD", "MBWR", "MBS", "MBR", "MBC"] {
                let name = format!("{family}{suffix}");
                let body = match family {
                    "MCMP" => format!("{name} {base} D300 K2 D500"),
                    "MBRD" | "MBWR" => format!("{name} {base} K2 D500"),
                    "MBS" | "MBR" => format!("{name} {base} D400 K2"),
                    _ => format!("{name} {base} K2 D500"),
                };
                let flags = [
                    (1088, true),
                    (1090, true),
                    (1093, true),
                    (1096, true),
                    (1097, base == "C255"),
                    (1098, true),
                ];
                out.push(staged(
                    &format!("{name}_{base}_two_halves"),
                    &body,
                    15,
                    &flags,
                ));
            }
        }
        for family in ["MBS", "MBR"] {
            let name = format!("{family}{suffix}");
            out.push(staged(
                &format!("{name}_natural_C255_dest"),
                &format!("{name} D300 C255 K2"),
                0,
                &[(1096, true), (1097, family == "MBR")],
            ));
            out.push(staged(
                &format!("{name}_C199_cross_source_dest"),
                &format!("{name} C199 C199 K4"),
                0,
                &[(1096, true), (1097, true)],
            ));
        }
        // Genuine compiler natural descriptor sets decoderflag40. These bodies
        // silently advance nominalPC and leave data unchanged; never substitute
        // a synthetic low-bank descriptor merely to force computation.
        for family in ["MAND", "MOR", "MXOR", "MXNR"] {
            let name = format!("{family}{suffix}");
            for (position, body) in [
                ("left", format!("{name} C200 D300 D400 K2")),
                ("right", format!("{name} D300 C200 D400 K2")),
                ("dest", format!("{name} D300 D320 C200 K2")),
            ] {
                out.push(staged(
                    &format!("{name}_natural_silent_{position}"),
                    &body,
                    0,
                    &[],
                ));
            }
        }
        for (position, body) in [
            ("source", format!("MINV{suffix} C200 D400 K2")),
            ("dest", format!("MINV{suffix} D300 C200 K2")),
        ] {
            out.push(staged(
                &format!("MINV{suffix}_natural_silent_{position}"),
                &body,
                0,
                &[],
            ));
        }
        // Test partial decoder scratch using a subsequent REAL word SORT. SER
        // seeds FF03. S1natural holds it; groupS1/S2natural and groupS1/Dnatural
        // leave0103. MINV rejects without changing the prior scratch.
        for (label, body) in [
            (
                "first_natural_holds",
                format!("MAND{suffix} C200 D300 D400 K1"),
            ),
            (
                "group_first_second_natural",
                format!("MAND{suffix} K4M256 C200 D400 K1"),
            ),
            (
                "group_first_dest_natural",
                format!("MAND{suffix} K4M256 D300 C200 K1"),
            ),
            (
                "invert_group_source_natural_dest_holds",
                format!("MINV{suffix} K4M256 C200 K1"),
            ),
        ] {
            out.push(scratch(&format!("{label}{suffix}"), &body));
        }
        // Scalar C200 differs from a matrix address: numeric16 read produces
        // E1A/zero and suppresses its later write. MBC output-only silently holds.
        for family in ["MCMP", "MBRD", "MBWR", "MBC"] {
            let name = format!("{family}{suffix}");
            let body = if family == "MCMP" {
                format!("{name} D300 D320 K2 C200")
            } else {
                format!("{name} D300 K2 C200")
            };
            out.push(staged(
                &format!("{name}_scalar_C200_read_error_or_output_hold"),
                &body,
                0,
                &[(1088, true), (1093, true), (1096, true), (1098, true)],
            ));
        }
    }
    out
}
