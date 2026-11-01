//! Register-only time probes; native successful TADD/TSUB PC defects are
//! retained separately in rtc-protocol-runtime-first.json, never passing parity.
use super::{Case, case};
use opendvp::es2_bridge::Target;

fn seed(base: &str, values: [i16; 3]) -> String {
    if !base.starts_with('D') {
        return seed("D500", values) + &format!("BMOV D500 {base} K3\n");
    }
    let index = base[1..].parse::<u16>().unwrap();
    values
        .into_iter()
        .enumerate()
        .map(|(i, v)| format!("MOV K{v} {}{}\n", &base[..1], index + i as u16))
        .collect()
}
fn staged(name: &str, init: &str, body: &str) -> Case {
    case(
        &format!("calendar_{name}"),
        &format!(
            "LD X1\n{init}SET M1020\nSET M1021\nSET M1022\nSET M10\nSET M11\nSET M12\nLD X0\n{body}\nMOV K123 D900\nEND"
        ),
        vec![
            (vec![("X1", true), ("X0", false)], 1),
            (vec![("X1", false), ("X0", true)], 2),
            (vec![("X0", false)], 1),
            (vec![("X0", true)], 1),
        ],
    )
}
pub fn cases(target: Target) -> Vec<Case> {
    let mut out = Vec::new();
    for op in ["TCMP", "TCMPP"] {
        for bank in ['E', 'F'] {
            for index in 0..8 {
                let alias =
                    if index == 0 { 1028 } else { 1180 + index * 2 } + i32::from(bank == 'F');
                let init = seed("D20", [12, 20, 45]) + &format!("MOV K12 D{alias}\n");
                out.push(staged(
                    &format!("{op}_index_{bank}{index}"),
                    &init,
                    &format!("{op} {bank}{index} K20 K45 D20 M10"),
                ));
            }
            out.push(staged(
                &format!("{op}_bare_{bank}"),
                &(seed("D20", [12, 20, 45])
                    + &format!("MOV K12 D{}\n", if bank == 'E' { 1028 } else { 1029 })),
                &format!("{op} {bank} K20 K45 D20 M10"),
            ));
        }
        for (i, values) in [
            [0, 0, 0],
            [12, 20, 44],
            [12, 20, 45],
            [12, 20, 46],
            [23, 59, 59],
            [24, 0, 0],
            [0, -1, 0],
            [0, 0, 60],
        ]
        .into_iter()
        .enumerate()
        {
            out.push(staged(
                &format!("{op}_current_{i}"),
                &seed("D20", values),
                &format!("{op} K12 K20 K45 D20 M10"),
            ));
        }
        for (i, setpoint) in [
            "K-1 K20 K45",
            "K24 K20 K45",
            "K12 K60 K45",
            "K12 K20 K-1",
            "K12 H14 H2D",
        ]
        .into_iter()
        .enumerate()
        {
            out.push(staged(
                &format!("{op}_scalar_{i}"),
                &seed("D20", [12, 20, 45]),
                &format!("{op} {setpoint} D20 M10"),
            ));
        }
        for source in ["T10", "C10", "D20"] {
            out.push(staged(
                &format!("{op}_area_{source}"),
                &seed(source, [12, 20, 45]),
                &format!("{op} K12 K20 K45 {source} Y10"),
            ));
        }
        out.push(staged(
            &format!("{op}_packed"),
            &(seed("D20", [1, 2, 3]) + "SET M0\nSET M5\nSET M8\nSET M9\n"),
            &format!("{op} K1M0 K1M4 K1M8 D20 S10"),
        ));
    }
    for op in ["TZCP", "TZCPP"] {
        for (i, (low, high, current)) in [
            ([6, 0, 0], [18, 0, 0], [5, 59, 59]),
            ([6, 0, 0], [18, 0, 0], [6, 0, 0]),
            ([6, 0, 0], [18, 0, 0], [18, 0, 0]),
            ([6, 0, 0], [18, 0, 0], [18, 0, 1]),
            ([18, 0, 0], [6, 0, 0], [12, 0, 0]),
            ([18, 0, 0], [6, 0, 0], [23, 0, 0]),
            ([12, 0, 0], [12, 0, 0], [12, 0, 0]),
            ([24, 0, 0], [18, 0, 0], [12, 0, 0]),
            ([6, 0, 0], [18, 60, 0], [12, 0, 0]),
            ([6, 0, 0], [18, 0, 0], [12, 0, -1]),
        ]
        .into_iter()
        .enumerate()
        {
            let init = seed("D0", low) + &seed("D10", high) + &seed("D20", current);
            out.push(staged(
                &format!("{op}_range_{i}"),
                &init,
                &format!("{op} D0 D10 D20 M10"),
            ));
        }
        let init = seed("T10", [6, 0, 0]) + &seed("C10", [18, 0, 0]) + &seed("D20", [12, 0, 0]);
        out.push(staged(
            &format!("{op}_mixed_areas"),
            &init,
            &format!("{op} T10 C10 D20 S10"),
        ));
    }
    for op in ["TADD", "TADDP", "TSUB", "TSUBP"] {
        for (i, (left, right)) in [
            ([1, 2, -1], [4, 5, 6]),
            ([1, 2, 3], [4, 5, 60]),
            ([1, 60, 3], [4, 5, 6]),
            ([1, 2, 3], [4, -1, 6]),
            ([24, 2, 3], [4, 5, 6]),
            ([1, 2, 3], [-1, 5, 6]),
        ]
        .into_iter()
        .enumerate()
        {
            let init = seed("D0", left) + &seed("D10", right) + &seed("D20", [21, 22, 23]);
            out.push(staged(
                &format!("{op}_invalid_partial_{i}"),
                &init,
                &format!("{op} D0 D10 D20"),
            ));
        }
        out.push(case(
            &format!("calendar_{op}_disabled"),
            &format!("LD X0\n{op} D0 D10 D20\nMOV K123 D900\nEND"),
            vec![(vec![("X0", false)], 2)],
        ));
    }
    let end = if target == Target::Ss2 { 4997 } else { 9997 };
    out.push(staged(
        "TCMP_last_triple",
        &seed(&format!("D{end}"), [12, 20, 45]),
        &format!("TCMP K12 K20 K45 D{end} M10"),
    ));
    out
}
