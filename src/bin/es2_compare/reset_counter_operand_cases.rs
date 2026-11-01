//! Direct one-word numeric RST and installed16-bit CNT operand behavior.
//! No RSTP, indexed numeric reset, indexed preset or direct E/F CNT preset.
use super::{Case, case};
use std::collections::BTreeSet;

const COUNTERS: [u16; 6] = [0, 111, 112, 127, 128, 199];

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (
            vec![
                ("X1", true),
                ("X0", false),
                ("X2", false),
                ("X3", false),
                ("X7", false),
            ],
            1,
        ),
        (vec![("X1", false), ("X0", true)], 2),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}

fn changing_stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (
            vec![
                ("X1", true),
                ("X0", false),
                ("X2", false),
                ("X3", false),
                ("X7", false),
            ],
            1,
        ),
        (vec![("X1", false), ("X0", true)], 1),
        (vec![("X2", true)], 1),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}

fn fixture(
    name: &str,
    seed: &str,
    body: &str,
    observe: &str,
    visits: Vec<(Vec<(&'static str, bool)>, usize)>,
) -> Case {
    let source = [
        "LD X1",
        seed,
        "LDI X7\nSET M1020\nRST M1021\nSET M1022\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068",
        body,
        "LD M1020\nOUT Y300\nLD M1021\nOUT Y301\nLD M1022\nOUT Y302\nLD M1067\nOUT Y303\nLD M1068\nOUT Y304",
        "LDI X7\nMOV D1067 D300\nMOV D1068 D301",
        observe,
        "END",
    ].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n");
    assert!(
        source.lines().count() <= 512,
        "oversized reset/counter case {name}"
    );
    let mut definitions = BTreeSet::new();
    for line in source.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.first() == Some(&"CNT") {
            assert!(
                definitions.insert(fields[1]),
                "duplicate CNT definition: {name}"
            );
            assert_eq!(fields.len(), 3);
            assert!(fields[2].starts_with(['K', 'H', 'D']));
            if !fields[2].starts_with('H') {
                assert!(!fields[2].contains('E') && !fields[2].contains('F'));
            }
        }
    }
    case(&format!("reset_counter_operand_{name}"), &source, visits)
}

fn guard_counters(counter: u16) -> (u16, u16) {
    if counter == 0 {
        (1, 2)
    } else {
        (counter - 1, counter + 1)
    }
}

fn counter_seed(counter: u16, initial: i16, preset: i16) -> String {
    let (before, after) = guard_counters(counter);
    let after_seed = if after == 200 {
        "DMOV H12345678 C200".to_owned()
    } else {
        format!("MOV H3CC3 C{after}")
    };
    format!(
        "MOV H5AA5 C{before}\nMOV K{initial} C{counter}\n{after_seed}\nMOV H55AA D39\nMOV K{preset} D40\nMOV H6A5A D41\nMOV K0 D20\nMOV K0 D30"
    )
}

fn counter_observe(counter: u16) -> String {
    let (before, after) = guard_counters(counter);
    let after_read = if after == 200 {
        "DMOV C200 D203".to_owned()
    } else {
        format!("MOV C{after} D203")
    };
    format!(
        "LD C{counter}\nOUT Y0\nLD C{before}\nOUT Y1\nLD C{after}\nOUT Y2\nLDI X7\nMOV C{counter} D200\nMOV C{before} D202\n{after_read}\nMOV D39 D209\nMOV D40 D210\nMOV D41 D211\nMOV D20 D212\nMOV D30 D213"
    )
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();

    // Snapshot immediately after RST, then feed a new word for the next
    // visit. A held reset must execute again; an OFF visit must preserve it.
    for bank in ['E', 'F'] {
        for index in 0..8u16 {
            let low = if index == 0 { 1028 } else { 1180 + index * 2 };
            let alias = low + u16::from(bank == 'F');
            let target = format!("{bank}{index}");
            let seed = format!(
                "MOV H5AA5 D{}\nMOV HABCD D{alias}\nMOV H3CC3 D{}\nMOV H7777 D20",
                alias - 1,
                alias + 1
            );
            let body =
                format!("LD X0\nRST {target}\nLDI X7\nMOV {target} D20\nLD X0\nMOV H2468 D{alias}");
            let observe = format!(
                "MOV D20 D200\nMOV D{} D201\nMOV D{alias} D202\nMOV D{} D203\nMOV E{index} D210\nMOV F{index} D211\nMOV D{low} D212\nMOV D{} D213",
                alias - 1,
                alias + 1,
                low + 1
            );
            cases.push(fixture(
                &format!("reset_direct_{target}_one_word"),
                &seed,
                &body,
                &observe,
                stages(),
            ));
        }
    }
    for target in [1, 15, 127, 256, 4095, 4998] {
        let seed = format!(
            "MOV H5AA5 D{}\nMOV HABCD D{target}\nMOV H3CC3 D{}\nMOV H7777 D20",
            target - 1,
            target + 1
        );
        let body =
            format!("LD X0\nRST D{target}\nLDI X7\nMOV D{target} D20\nLD X0\nMOV H2468 D{target}");
        let observe = format!(
            "MOV D20 D200\nMOV D{} D201\nMOV D{target} D202\nMOV D{} D203",
            target - 1,
            target + 1
        );
        cases.push(fixture(
            &format!("reset_D{target}_one_word"),
            &seed,
            &body,
            &observe,
            stages(),
        ));
    }
    assert_eq!(cases.len(), 22);

    // Ordinary positive constants remain part of the corpus, alongside K0
    // and the signed-word maximum. A maximum preset starts one count below.
    for counter in COUNTERS {
        for preset in [0, 1, 2, 32767i16] {
            let initial = if preset == 32767 { 32766 } else { 0 };
            cases.push(fixture(
                &format!("cnt_C{counter}_constant_{preset}"),
                &counter_seed(counter, initial, preset),
                &format!("LD X0\nCNT C{counter} K{preset}\nLDI X7\nINC D20"),
                &counter_observe(counter),
                stages(),
            ));
        }
    }
    assert_eq!(cases.len(), 46);

    // D is read again on the held powered visit, although count increment
    // needs a new edge. Increasing a preset after completion must not clear
    // the done latch. Negative/zero presets also distinguish signed behavior.
    for counter in COUNTERS {
        for (name, preset, alternate) in [
            ("lower_while_held", 3, 1),
            ("increase_after_done", 1, 5),
            ("negative_first_edge", -2, 4),
            ("zero_while_held", 3, 0),
        ] {
            cases.push(fixture(
                &format!("cnt_C{counter}_dynamic_{name}"),
                &counter_seed(counter, 0, preset),
                &format!("LD X2\nMOV K{alternate} D40\nLD X0\nCNT C{counter} D40\nLDI X7\nINC D20"),
                &counter_observe(counter),
                changing_stages(),
            ));
        }
    }
    assert_eq!(cases.len(), 70);

    // True then visited-false RST removes inhibition before the fresh CNT.
    // On the second ON scan, clearing the counter does not clear that CNT
    // instruction's edge history, so held power is observably different.
    for counter in COUNTERS {
        cases.push(fixture(&format!("cnt_C{counter}_true_then_false_reset"),
            &counter_seed(counter, 7, 2),
            &format!("LD X0\nRST C{counter}\nLD X7\nRST C{counter}\nLD X0\nCNT C{counter} K2\nLDI X7\nINC D20"),
            &counter_observe(counter), stages()));
    }
    assert_eq!(cases.len(), 76);

    // Reset inhibition survives END and skipped reset instructions. Even an
    // inhibited powered CNT records its PC history; merely visiting false
    // RST on the next held scan does not create a fresh counting edge.
    for counter in [111, 128] {
        let visits = vec![
            (
                vec![
                    ("X1", true),
                    ("X0", false),
                    ("X2", false),
                    ("X3", false),
                    ("X7", false),
                ],
                1,
            ),
            (vec![("X1", false), ("X0", true)], 1),
            (vec![("X2", true)], 1),
            (vec![("X0", false), ("X2", false), ("X3", true)], 1),
            (vec![("X0", true)], 1),
            (vec![("X3", false)], 1),
            (vec![("X0", false)], 1),
            (vec![("X0", true)], 1),
        ];
        cases.push(fixture(&format!("cnt_C{counter}_reset_inhibit_survives_skipped_scans"),
            &counter_seed(counter, 0, 4),
            &format!("LD X3\nCJ P10\nLD X2\nRST C{counter}\nP10\nLD X0\nCNT C{counter} D40\nLDI X7\nINC D20"),
            &counter_observe(counter), visits));
    }
    assert_eq!(cases.len(), 78);

    for (counter, external) in [(0, -7), (199, 32767)] {
        cases.push(fixture(&format!("cnt_C{counter}_external_count_after_done"),
            &counter_seed(counter, 0, 1),
            &format!("LD X2\nMOV K{external} C{counter}\nMOV K10 D40\nLD X0\nCNT C{counter} D40\nLDI X7\nINC D20"),
            &counter_observe(counter), changing_stages()));
    }
    assert_eq!(cases.len(), 80);

    cases.push(fixture(
        "cnt_C127_same_pc_three_held_visits",
        &counter_seed(127, 0, 5),
        "FOR K3\nLD X0\nCNT C127 D40\nLDI X7\nINC D20\nNEXT",
        &counter_observe(127),
        stages(),
    ));
    cases.push(fixture("cnt_C128_same_pc_true_false_true_visits",
        &counter_seed(128, 0, 5),
        "LDI X7\nMOV K0 D30\nFOR K3\nLD= D30 K0\nOR= D30 K2\nAND X0\nCNT C128 D40\nLDI X7\nINC D30\nINC D20\nNEXT",
        &counter_observe(128), stages()));
    assert_eq!(cases.len(), 82);

    // CNT still sees an ordinary D preset; direct E/F syntax is used only by
    // the independent one-word reset. The canonical alias is read live.
    for (target, alias) in [("E1", 1182), ("F1", 1183)] {
        let seed = format!(
            "{}\nMOV H5AA5 D{}\nMOV K3 D{alias}\nMOV H3CC3 D{}",
            counter_seed(112, 0, 3),
            alias - 1,
            alias + 1
        );
        let observe = format!(
            "{}\nMOV D{} D220\nMOV D{alias} D221\nMOV D{} D222\nMOV E1 D223\nMOV F1 D224",
            counter_observe(112),
            alias - 1,
            alias + 1
        );
        cases.push(fixture(
            &format!("cnt_C112_preset_alias_reset_{target}"),
            &seed,
            &format!("LD X2\nRST {target}\nLD X0\nCNT C112 D{alias}\nLDI X7\nINC D20"),
            &observe,
            changing_stages(),
        ));
    }
    assert_eq!(cases.len(), 84);

    cases.push(fixture(
        "cnt_C199_signed_increment_wrap_before_comparison",
        &counter_seed(199, i16::MAX, i16::MAX),
        "LD X0\nCNT C199 D40\nLDI X7\nINC D20",
        &counter_observe(199),
        stages(),
    ));
    cases.push(fixture(
        "cnt_C0_negative_limit_first_edge",
        &counter_seed(0, i16::MIN, i16::MIN + 1),
        "LD X2\nMOV K32767 D40\nLD X0\nCNT C0 D40\nLDI X7\nINC D20",
        &counter_observe(0),
        changing_stages(),
    ));
    cases.push(fixture(
        "cnt_C128_maximum_dynamic_preset",
        &counter_seed(128, i16::MAX - 1, i16::MAX),
        "LD X0\nCNT C128 D40\nLDI X7\nINC D20",
        &counter_observe(128),
        stages(),
    ));
    cases.push(fixture(
        "cnt_C111_false_preserves_overpreset_seed",
        &counter_seed(111, 10, 2),
        "LD X0\nCNT C111 K2\nLDI X7\nINC D20",
        &counter_observe(111),
        stages(),
    ));
    for (c, preset) in [(0, "H0"), (112, "H1"), (128, "H7FFF"), (199, "H2")] {
        cases.push(fixture(
            &format!("cnt_C{c}_literal_{preset}"),
            &counter_seed(c, 0, 0),
            &format!("LD X0\nCNT C{c} {preset}"),
            &counter_observe(c),
            stages(),
        ));
    }
    for target in ["D40", "E1", "F1"] {
        cases.push(fixture(
            &format!("numeric_reset_inactive_step_{target}"),
            &format!("ZRST S0 S1023\nRST M1040\nRST M1045\nRST M1047\nMOV K123 {target}"),
            &format!("STL S0\nLD X0\nRST {target}\nRET"),
            &format!("LDI X7\nMOV {target} D200\nMOV K1S0 D201"),
            stages(),
        ));
    }
    cases.push(fixture(
        "cnt_inactive_step_context",
        &format!(
            "ZRST S0 S1023\nRST M1040\nRST M1045\nRST M1047\n{}",
            counter_seed(0, 0, 0)
        ),
        "STL S0\nLD X0\nCNT C0 K5\nRET",
        &counter_observe(0),
        stages(),
    ));
    assert_eq!(cases.len(), 96);
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
