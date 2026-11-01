//! DMOVR literal-to-packed targets: widths, signs, pulse edges and area ends.
use super::{Case, case};

fn address(area: &str, n: u16) -> String {
    if area == "Y" {
        format!("Y{n:o}")
    } else {
        format!("{area}{n}")
    }
}
fn scenario(name: &str, op: &str, literal: &str, area: &str, start: u16, digits: u16) -> Case {
    let width = digits * 4;
    let target = format!("K{digits}{}", address(area, start));
    let seed = (start.saturating_sub(1)..=(start + width).min(if area == "S" {
        1023
    } else if area == "Y" {
        255
    } else {
        4095
    }))
        .map(|n| format!("SET {}", address(area, n)))
        .collect::<Vec<_>>()
        .join("\n");
    let before = if start > 0 {
        format!("LD {}\nOUT Y0\n", address(area, start - 1))
    } else {
        String::new()
    };
    let after = if start + width
        <= if area == "S" {
            1023
        } else if area == "Y" {
            255
        } else {
            4095
        } {
        format!("LD {}\nOUT Y1\n", address(area, start + width))
    } else {
        String::new()
    };
    case(
        &format!("float_move_operand_{name}"),
        &format!(
            "LD X1\n{seed}\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV H1234 D1067\nMOV K77 D1068\nLD X0\n{op} {literal} {target}\nMOV K123 D302\nINC D303\n{before}{after}LD M1020\nOUT Y2\nLD M1021\nOUT Y3\nLD M1022\nOUT Y4\nLD M1067\nOUT Y5\nLD M1068\nOUT Y6\nLDI X7\nMOV D1067 D300\nMOV D1068 D301\nEND"
        ),
        vec![
            (vec![("X1", true)], 1),
            (vec![("X1", false), ("X0", true)], 2),
            (vec![("X0", false)], 1),
            (vec![("X0", true)], 1),
        ],
    )
}
pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (area, start) in [("Y", 32), ("M", 128), ("S", 500)] {
        for digits in 1..=8 {
            for op in ["DMOVR", "DMOVRP"] {
                cases.push(scenario(
                    &format!("{op}_K{digits}{area}_fraction"),
                    op,
                    if digits % 2 == 0 { "F-1.2" } else { "F1.2" },
                    area,
                    start,
                    digits,
                ));
            }
        }
    }
    for (name, literal) in [
        ("zero", "F0"),
        ("negative_zero", "F-0.0"),
        ("subnormal", "F1e-45"),
        ("negative_subnormal", "F-1e-45"),
        ("normal_min", "F1.17549435e-38"),
        ("max", "F3.4028235e38"),
    ] {
        for op in ["DMOVR", "DMOVRP"] {
            cases.push(scenario(&format!("{op}_{name}"), op, literal, "M", 2000, 8));
        }
    }
    for (area, start) in [("Y", 224), ("M", 4064), ("S", 992)] {
        for op in ["DMOVR", "DMOVRP"] {
            cases.push(scenario(
                &format!("{op}_{area}_last_full_group"),
                op,
                "F-1.2",
                area,
                start,
                8,
            ));
        }
    }
    assert_eq!(cases.len(), 66);
    cases
}
