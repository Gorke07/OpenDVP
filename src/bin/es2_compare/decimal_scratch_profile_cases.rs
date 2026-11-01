//! The same bounded DEBCD IL runs against current and legacy ES2 profiles.
//! Public D5000/D5001 observations are explicit; expectations live outside IL.
//! SS2 exposes no D5000, and no infinity inputs are included here.
use super::{Case, Target, case};
use std::collections::BTreeSet;

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (vec![("X1", true), ("X0", false), ("X7", false)], 1),
        (vec![("X1", false), ("X0", true)], 2),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}

fn snapshot(slot: u16) -> String {
    format!(
        "LD M1020\nOUT Y{:o}\nLD M1021\nOUT Y{:o}\nLD M1022\nOUT Y{:o}\nLDI X7\nMOV D5000 D{}\nMOV D5001 D{}",
        64 + slot * 3,
        65 + slot * 3,
        66 + slot * 3,
        80 + slot * 2,
        81 + slot * 2
    )
}

fn scenario(name: &str, seed: &str, body: &str) -> Case {
    let source = format!(
        "LD X1\nMOV H1111 D4998\nMOV H2222 D4999\nMOV H1357 D5000\nMOV H2468 D5001\nMOV H5555 D5002\nMOV H6666 D5003\nMOV H7777 D5004\nMOV H1234 D31\nMOV H5678 D34\nMOV H0101 D159\nDMOV H55AAAA55 D160\nMOV H0202 D162\nMOV H0303 D163\nDMOV HAA5555AA D164\nMOV H0404 D166\nSET M1020\nSET M1021\nSET M1022\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\n{seed}\nLD X0\n{body}\nLD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y23\nLD M1068\nOUT Y24\nLDI X7\nMOV D5000 D200\nMOV D5001 D201\nMOV D4998 D210\nMOV D4999 D211\nMOV D5000 D212\nMOV D5001 D213\nMOV D5002 D214\nMOV D5003 D215\nMOV D5004 D216\nMOV D159 D220\nMOV D160 D221\nMOV D161 D222\nMOV D162 D223\nMOV D163 D224\nMOV D164 D225\nMOV D165 D226\nMOV D166 D227\nMOV D31 D230\nMOV D32 D231\nMOV D33 D232\nMOV D34 D233\nMOV D1067 D300\nMOV D1068 D301\nMOV K1234 D302\nEND"
    );
    assert!(
        source.lines().count() <= 512,
        "oversized decimal profile case {name}"
    );
    assert!(
        source.lines().count() * 9 <= 0x1ef0,
        "installed program budget: {name}"
    );
    case(
        &format!("decimal_scratch_profile_{name}"),
        &source,
        stages(),
    )
}

pub(super) fn cases(target: Target) -> Vec<Case> {
    if target == Target::Ss2 {
        return Vec::new();
    }
    let mut cases = Vec::new();

    // A changed live source on the held scan distinguishes normal execution
    // from pulse suppression; both profiles receive the identical source IL.
    for (name, op, initial, next) in [
        (
            "positive_ordinary_DEBCD",
            "DEBCD",
            0x4048f5c3u32,
            0x3e000000u32,
        ),
        ("negative_ordinary_DEBCDP", "DEBCDP", 0xc1b80000, 0xbe800000),
    ] {
        cases.push(scenario(
            name,
            &format!("DMOV H{initial:08X} D32"),
            &format!("{op} D32 D160\nDMOV H{next:08X} D32"),
        ));
    }

    // Zero/unordered branches must preserve pre-existing scratch words except
    // for an explicitly overlapping destination (covered separately below).
    for (name, op, raw) in [
        ("positive_zero_preserves_scratch", "DEBCD", 0x00000000u32),
        ("negative_zero_preserves_scratch", "DEBCDP", 0x80000000),
        ("positive_qnan_preserves_scratch", "DEBCD", 0x7fc12345),
        ("negative_qnan_preserves_scratch", "DEBCDP", 0xffc12345),
    ] {
        cases.push(scenario(
            name,
            &format!("DMOV H{raw:08X} D32"),
            &format!("{op} D32 D160"),
        ));
    }

    // Read the scratch pair immediately after each conversion, before a later
    // site can replace it. The minimum normal also exercises low-end clamping.
    for (name, op, subnormal, minimum) in [
        (
            "positive_subnormal_and_min_normal",
            "DEBCD",
            0x00000001u32,
            0x00800000u32,
        ),
        (
            "negative_subnormal_and_min_normal",
            "DEBCDP",
            0x80000001,
            0x80800000,
        ),
    ] {
        let seed = format!("DMOV H{subnormal:08X} D32\nDMOV H{minimum:08X} D36");
        let body = format!(
            "{op} D32 D160\n{}\nLD X0\n{op} D36 D164\n{}",
            snapshot(0),
            snapshot(1)
        );
        cases.push(scenario(name, &seed, &body));
    }

    // Largest finite values bound the high-end clamp decision without ever
    // supplying infinity. No expected coefficient/flag is embedded here.
    cases.push(scenario(
        "positive_negative_max_finite_clamp_boundary",
        "DMOV H7F7FFFFF D32\nDMOV HFF7FFFFF D36",
        &format!(
            "DEBCD D32 D160\n{}\nLD X0\nDEBCD D36 D164\n{}",
            snapshot(0),
            snapshot(1)
        ),
    ));

    for (dest, op, initial, next) in [
        (4999, "DEBCD", 0x4048f5c3u32, 0x00000000u32),
        (5000, "DEBCDP", 0xc1b80000, 0x80000001),
        (5001, "DEBCD", 0x00800000, 0x7fc12345),
    ] {
        cases.push(scenario(
            &format!("target_overlap_D{dest}_{op}"),
            &format!("DMOV H{initial:08X} D32"),
            &format!("{op} D32 D{dest}\nDMOV H{next:08X} D32"),
        ));
    }

    assert_eq!(cases.len(), 12);
    assert_eq!(
        cases
            .iter()
            .map(|entry| &entry.name)
            .collect::<BTreeSet<_>>()
            .len(),
        12
    );
    cases
}
