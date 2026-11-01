//! Explicit simulator input-image writes; no physical-input forcing claim.
use super::{Case, case};
pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for digits in [1, 2, 4] {
        for pulse in [false, true] {
            for pointer in [0, 15] {
                let op = if pulse { "MBWRP" } else { "MBWR" };
                cases.push(case(&format!("matrix_input_{op}_K{digits}_bit{pointer}"),
                    &format!("LDI X375\nMOV K{pointer} D10\nLD X374\nOUT M1096\nLD X377\n{op} K{digits}X0 K1 D10\nLD X0\nOUT Y0\nLD X17\nOUT Y1\nEND"),
                    vec![(vec![("X377",false),("X374",true)],1),(vec![("X377",true)],2),(vec![("X374",false)],1),(vec![("X377",false)],1),(vec![("X377",true)],1)]));
            }
        }
    }
    cases
}
