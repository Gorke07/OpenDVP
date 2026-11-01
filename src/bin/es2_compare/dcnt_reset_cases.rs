//! InstalledLegacy C200..234: persistent bare-RST inhibition and DCNT edges.
//! ZRST clears the value/contact without changing bare-RST inhibition.
//! C235 is only an adjacent value guard; no physical counter is exercised.
use super::{Case, case};
use std::collections::BTreeSet;

type Stages = Vec<(Vec<(&'static str, bool)>, usize)>;

const COUNTERS: [u16; 6] = [200, 223, 224, 231, 232, 234];

fn initial(down: bool) -> Vec<(&'static str, bool)> {
    vec![
        ("X1", true),
        ("X0", false),
        ("X2", false),
        ("X3", false),
        ("X4", down),
        ("X5", false),
        ("X7", false),
    ]
}

fn stages(down: bool) -> Stages {
    vec![
        (initial(down), 1),
        (vec![("X1", false), ("X0", true)], 2),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}

fn changing_stages(down: bool, changed_down: bool) -> Stages {
    vec![
        (initial(down), 1),
        (vec![("X1", false), ("X0", true)], 1),
        (vec![("X2", true), ("X4", changed_down)], 1),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}

fn held_reset_stages(down: bool) -> Stages {
    vec![
        (initial(down), 1),
        (vec![("X1", false), ("X0", true)], 1),
        (vec![("X2", true)], 1),
        (vec![("X2", false)], 1),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}

// The first edge can count. True RST then inhibits; OFF and fresh ON while
// RST is skipped cross END twice. False RST on held ON releases inhibition
// without restoring the consumed edge. Only the final OFF/ON can count.
fn skipped_reset_stages(down: bool) -> Stages {
    vec![
        (initial(down), 1),
        (vec![("X1", false), ("X0", true)], 1),
        (vec![("X2", true)], 1),
        (vec![("X0", false), ("X2", false), ("X3", true)], 1),
        (vec![("X0", true)], 1),
        (vec![("X3", false)], 1),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}

fn context_stages(down: bool) -> Stages {
    vec![
        (initial(down), 1),
        (vec![("X1", false), ("X0", true), ("X2", true)], 1),
        (vec![("X0", false), ("X2", false), ("X3", true)], 1),
        (vec![("X0", true)], 1),
        (vec![("X5", true)], 1),
        (vec![("X5", false)], 1),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}

fn counter_seed(counter: u16, value: i32, preset: i32) -> String {
    let before = counter - 1;
    let after = counter + 1;
    let before_seed = if before == 199 {
        "MOV H5AA5 C199".to_owned()
    } else {
        format!("DMOV H5AA51234 C{before}")
    };
    format!(
        "{before_seed}\nDMOV K{value} C{counter}\nDMOV H12345678 C{after}\nMOV H55AA D39\nDMOV K{preset} D40\nMOV H6A5A D42\nMOV K0 D20\nMOV K0 D25\nMOV H1357 D29\nMOV K0 D30\nMOV H2468 D31\nDMOV K0 D50\nMOV H1357 D203"
    )
}

fn counter_observe(counter: u16) -> String {
    let before = counter - 1;
    let after = counter + 1;
    let before_read = if before == 199 {
        "MOV C199 D202".to_owned()
    } else {
        format!("DMOV C{before} D202")
    };
    format!(
        "LD C{counter}\nOUT Y0\nLD C{before}\nOUT Y1\nLD C{after}\nOUT Y2\nLD M{}\nOUT Y3\nLD S0\nOUT Y4\nLD S1\nOUT Y5\nLDI X7\nDMOV C{counter} D200\n{before_read}\nDMOV C{after} D204\nMOV D39 D209\nDMOV D40 D210\nMOV D42 D212\nMOV D20 D213\nMOV D30 D214\nMOV D25 D215\nDMOV D50 D216\nMOV D29 D218\nMOV D31 D219",
        1000 + counter
    )
}

fn fixture(
    name: &str,
    counter: u16,
    value: i32,
    preset: i32,
    extra_seed: &str,
    body: &str,
    visits: Stages,
) -> Case {
    assert!((200..235).contains(&counter));
    let seed = counter_seed(counter, value, preset);
    let observe = counter_observe(counter);
    let direction = format!("LD X4\nOUT M{}", 1000 + counter);
    let source = [
        "LD X1",
        &seed,
        extra_seed,
        &direction,
        "LDI X7\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068",
        body,
        "LD M1020\nOUT Y300\nLD M1021\nOUT Y301\nLD M1022\nOUT Y302\nLD M1067\nOUT Y303\nLD M1068\nOUT Y304",
        "LDI X7\nMOV D1067 D300\nMOV D1068 D301\nMOV K1234 D25\nINC D20",
        &observe,
        "END",
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join("\n");
    assert!(source.lines().count() <= 512, "oversized DCNT case {name}");
    let definitions: Vec<_> = source
        .lines()
        .filter(|line| line.starts_with("DCNT "))
        .collect();
    assert_eq!(definitions.len(), 1, "one DCNT site required: {name}");
    let fields: Vec<_> = definitions[0].split_whitespace().collect();
    assert_eq!(fields.len(), 3);
    assert_eq!(fields[1], format!("C{counter}"));
    assert!(fields[2].starts_with(['K', 'D']));
    if let Some(preset) = fields[2].strip_prefix('D') {
        assert!(preset.parse::<u16>().unwrap() < 4999);
    }
    case(&format!("dcnt_reset_{name}"), &source, visits)
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();

    // Six software-counter boundaries, in both directions. The second held
    // scan clears the value again, but false RST does not clear PC history.
    for counter in COUNTERS {
        for down in [false, true] {
            let direction = if down { "down" } else { "up" };
            let preset = if down { 0 } else { 1 };
            cases.push(fixture(
                &format!("C{counter}_true_then_false_rst_{direction}"),
                counter,
                7,
                preset,
                "",
                &format!(
                    "LD X0\nRST C{counter}\nLD X7\nRST C{counter}\nLD X0\nDCNT C{counter} K{preset}"
                ),
                stages(down),
            ));
        }
    }
    assert_eq!(cases.len(), 12);

    for counter in COUNTERS {
        for down in [false, true] {
            let direction = if down { "down" } else { "up" };
            cases.push(fixture(
                &format!("C{counter}_skipped_rst_persists_and_consumes_edge_{direction}"),
                counter,
                0,
                if down { -1 } else { 1 },
                "",
                &format!("LD X3\nCJ P10\nLD X2\nRST C{counter}\nP10\nLD X0\nDCNT C{counter} D40"),
                skipped_reset_stages(down),
            ));
        }
    }
    assert_eq!(cases.len(), 24);

    // Unlike bare RST, a ZRST immediately before DCNT permits a fresh edge.
    // The held scan still has its count cleared without another increment.
    for counter in COUNTERS {
        for down in [false, true] {
            let direction = if down { "down" } else { "up" };
            cases.push(fixture(
                &format!("C{counter}_zrst_allows_fresh_edge_{direction}"),
                counter,
                7,
                if down { 0 } else { 1 },
                "",
                &format!("LD X0\nZRST C{counter} C{counter}\nLD X0\nDCNT C{counter} D40"),
                stages(down),
            ));
        }
    }
    assert_eq!(cases.len(), 36);

    for counter in [200, 234] {
        for down in [false, true] {
            let direction = if down { "down" } else { "up" };
            cases.push(fixture(
                &format!("C{counter}_zrst_preserves_existing_inhibit_{direction}"),
                counter,
                0,
                if down { -1 } else { 1 },
                "",
                &format!("LD X3\nCJ P10\nLD X2\nRST C{counter}\nP10\nLD X0\nZRST C{counter} C{counter}\nLD X0\nDCNT C{counter} D40"),
                skipped_reset_stages(down),
            ));
        }
    }
    assert_eq!(cases.len(), 40);

    // Capture the value immediately after DCNT and before the later reset.
    // Releasing that reset on held power cannot manufacture another edge.
    for counter in [223, 232] {
        for down in [false, true] {
            let direction = if down { "down" } else { "up" };
            cases.push(fixture(
                &format!("C{counter}_rst_after_held_does_not_rearm_{direction}"),
                counter,
                0,
                if down { -1 } else { 1 },
                "",
                &format!(
                    "LD X0\nDCNT C{counter} D40\nLDI X7\nDMOV C{counter} D50\nLD X2\nRST C{counter}"
                ),
                held_reset_stages(down),
            ));
        }
    }
    assert_eq!(cases.len(), 44);

    for down in [false, true] {
        let direction = if down { "down" } else { "up" };
        cases.push(fixture(
            &format!("C224_same_pc_three_held_visits_{direction}"),
            224,
            0,
            if down { -2 } else { 2 },
            "",
            "LD X2\nRST C224\nLDI X7\nMOV K0 D30\nFOR K3\nLD X0\nDCNT C224 D40\nLDI X7\nINC D30\nNEXT",
            held_reset_stages(down),
        ));
        cases.push(fixture(
            &format!("C231_same_pc_true_false_true_visits_{direction}"),
            231,
            0,
            if down { -2 } else { 2 },
            "",
            "LD X7\nRST C231\nLDI X7\nMOV K0 D30\nFOR K3\nLD= D30 K0\nOR= D30 K2\nAND X0\nDCNT C231 D40\nLDI X7\nINC D30\nNEXT",
            stages(down),
        ));
    }
    assert_eq!(cases.len(), 48);

    // Inactive STL skips RST itself, including its false-instruction release.
    // S1 is enabled on a held scan; the following END makes it active before
    // the later OFF/ON edge. S0/S1 and both counter neighbors are observed.
    for counter in [200, 234] {
        for down in [false, true] {
            let direction = if down { "down" } else { "up" };
            cases.push(fixture(
                &format!("C{counter}_inactive_stl_rst_preserves_inhibit_{direction}"),
                counter,
                0,
                if down { -1 } else { 1 },
                "ZRST S0 S1023\nRST M1040\nRST M1045\nRST M1047\nSET S0",
                &format!("LD X3\nCJ P10\nLD X2\nRST C{counter}\nP10\nLD X5\nSET S1\nSTL S1\nLD X7\nRST C{counter}\nRET\nLD X0\nDCNT C{counter} D40"),
                context_stages(down),
            ));
        }
    }
    assert_eq!(cases.len(), 52);

    // Constant and full-DWORD presets distinguish equality-only behavior
    // from 16-bit preset reads, saturating count, or CNT-style done gating.
    for (name, counter, value, preset, dynamic, down, changed_down) in [
        ("up_wrap_K", 200, i32::MAX, i32::MIN, false, false, false),
        (
            "up_then_down_wrap_D",
            234,
            i32::MAX,
            i32::MIN,
            true,
            false,
            true,
        ),
        ("zero_equal_K", 223, -1, 0, false, false, false),
        ("negative_equal_K", 224, -3, -2, false, false, true),
        ("negative_down_D", 231, -1, -1, true, true, false),
        ("above_preset_not_equal_D", 232, 10, 2, true, false, false),
        (
            "full_high_half_preset_D",
            231,
            65535,
            65536,
            true,
            false,
            false,
        ),
        (
            "direction_changes_only_on_edge_K",
            200,
            1,
            2,
            false,
            false,
            true,
        ),
    ] {
        let operand = if dynamic {
            "D40".to_owned()
        } else {
            format!("K{preset}")
        };
        cases.push(fixture(
            &format!("C{counter}_{name}"),
            counter,
            value,
            preset,
            "",
            &format!("LD X7\nRST C{counter}\nLD X0\nDCNT C{counter} {operand}"),
            changing_stages(down, changed_down),
        ));
    }
    assert_eq!(cases.len(), 60);

    for (name, counter, preset, alternate) in [
        ("preset_increase_after_done_held", 223, 1, 5),
        ("preset_lower_to_current_count_held", 232, 3, 1),
    ] {
        cases.push(fixture(
            &format!("C{counter}_{name}"),
            counter,
            0,
            preset,
            "",
            &format!(
                "LD X7\nRST C{counter}\nLD X2\nDMOV K{alternate} D40\nLD X0\nDCNT C{counter} D40"
            ),
            changing_stages(false, false),
        ));
    }

    cases.push(fixture(
        "C224_external_value_after_done_held",
        224,
        0,
        1,
        "",
        "LD X7\nRST C224\nLD X2\nDMOV K10 C224\nDMOV K20 D40\nLD X0\nDCNT C224 D40",
        changing_stages(false, false),
    ));

    let mut inhibited_change = skipped_reset_stages(false);
    inhibited_change[2].0.extend([("X4", true), ("X5", true)]);
    cases.push(fixture(
        "C234_direction_and_signed_preset_change_while_inhibited",
        234,
        0,
        1,
        "",
        "LD X5\nDMOV K-1 D40\nLD X3\nCJ P10\nLD X2\nRST C234\nP10\nLD X0\nDCNT C234 D40",
        inhibited_change,
    ));
    assert_eq!(cases.len(), 64);
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
