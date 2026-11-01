//! Current ES2 versus legacy SS2 table-compare behavior, including x87 unordered cases.
use super::{Case, Target, case};
const LEFT: [u32; 11] = [
    0xbf800000, 0xbf800000, 0x80000000, 0, 0x7f800000, 0xff800000, 0x7fc00001, 0x7fc00001,
    0x7f800001, 1, 0x80000001,
];
const RIGHT: [u32; 11] = [
    0xc0000000, 0x3f800000, 0, 0x80000000, 0x7f800000, 0x7f800000, 0, 0x7fc00001, 0x3f800000, 0, 0,
];
fn scenario(name: &str, seed: &str, body: &str) -> Case {
    case(
        &format!("cmpt_profile_{name}"),
        &format!(
            "LD X1\n{seed}\nSET M1020\nSET M1021\nSET M1022\nRST M1067\nRST M1068\nMOV H1234 D1067\nMOV K77 D1068\nLD X0\n{body}\nMOV K123 D302\nINC D303\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLDI X7\nMOV D1067 D300\nMOV D1068 D301\nEND"
        ),
        vec![
            (vec![("X1", true)], 1),
            (vec![("X1", false), ("X0", true)], 2),
            (vec![("X0", false)], 1),
            (vec![("X0", true)], 1),
        ],
    )
}
pub fn cases(target: Target) -> Vec<Case> {
    let mut cases = Vec::new();
    let mut arrays = String::new();
    for (start, values) in [(0, LEFT), (40, RIGHT)] {
        for (i, value) in values.into_iter().enumerate() {
            arrays.push_str(&format!("DMOV H{value:08X} D{}\n", start + i * 2));
        }
    }
    for floating in [false, true] {
        for pulse in [false, true] {
            for mode in [0, 1, 2, 3, 4, 5, 255] {
                cases.push(scenario(&format!("float_{floating}_pulse_{pulse}_relation_{mode}"),
                    &format!("{arrays}{} M1162\nDMOV HFFFFFFFF D100\nSET M127\nSET M128\nSET M129\nSET M130\nSET M131\nSET M132",if floating {"SET"} else {"RST"}),
                    &format!("DCMPT{} D0 D40 H{:08X} D100\nDCMPT{} D0 D40 H{:08X} K1M128",if pulse {"P"} else {""},0x12340000|(mode<<8)|11,if pulse {"P"} else {""},(mode<<8)|11)));
            }
        }
    }
    let last = if target == Target::Ss2 { 4999 } else { 9999 };
    for op in ["CMPT", "CMPTP", "DCMPT", "DCMPTP"] {
        for side in ["left", "right"] {
            let (left, right) = if side == "left" {
                (format!("D{last}"), "D0".to_string())
            } else {
                ("D0".to_string(), format!("D{last}"))
            };
            cases.push(scenario(
                &format!("{op}_partial_{side}"),
                &format!("RST M1162\nDIV16 K-7 K1 D{last}\nDMOV K65529 D0\nDMOV H12345678 D100"),
                &format!("{op} {left} {right} K3 D100"),
            ));
        }
    }
    for op in ["DCMPT", "DCMPTP"] {
        for source in [format!("D{last}"), "T255".to_string(), "C199".to_string()] {
            cases.push(scenario(
                &format!("{op}_last_high_{source}"),
                &format!("RST M1162\nDIV16 K-1 K1 {source}\nDMOV K65535 D0\nDMOV H12345678 D100"),
                &format!("{op} {source} D0 K1 D100"),
            ));
        }
    }
    assert_eq!(cases.len(), 42);
    cases
}
