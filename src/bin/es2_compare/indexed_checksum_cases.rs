//! DRAFT: addressed LRC/LRCP/CRC/CRCP. Parent owns admission and execution.
//! Ordinary manual vectors are fixed literals, never obtained from production
//! checksum code or native outputs. Four SORT probes are parity-only witnesses.
use super::{Case, Target, case};
use std::collections::BTreeSet;

const FORMS: [&str; 4] = ["LRC", "LRCP", "CRC", "CRCP"];
const LRC_BYTES: &[u8] = b"010307080006";
const CRC_BYTES: &[u8] = &[1, 6, 7, 6, 0x17, 0x70];

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (vec![("X1", true), ("X0", false), ("X7", false)], 1),
        (vec![("X1", false), ("X0", true)], 2),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}
fn lrc(op: &str) -> bool {
    op.starts_with("LRC")
}
fn alias(bank: char, index: u16) -> u16 {
    (if index == 0 { 1028 } else { 1180 + 2 * index }) + u16::from(bank == 'F')
}
fn active(op: &str) -> &'static str {
    if op.ends_with('P') {
        "LD= D310 K2\nOR= D310 K5"
    } else {
        "LD X0"
    }
}
fn data(op: &str) -> &'static [u8] {
    if lrc(op) { LRC_BYTES } else { CRC_BYTES }
}
fn result(op: &str, low: bool) -> [u16; 2] {
    match (lrc(op), low) {
        (true, false) => [0x4537, 0x6bb6], // Manual E7, first ASCII digit HIGH.
        (true, true) => [0x45, 0x37],
        (false, false) => [0xab66, 0x6bb6], // Manual Modbus check AB66.
        (false, true) => [0x66, 0xab],
    }
}
fn bytes(base: u16, input: &[u8], low: bool) -> String {
    let words: Vec<u16> = if low {
        // Deliberately nonzero upper bytes prove M1161's low-byte reads.
        input.iter().map(|b| 0xa500 | u16::from(*b)).collect()
    } else {
        input
            .chunks(2)
            .map(|p| u16::from(p[0]) | (u16::from(*p.get(1).unwrap_or(&0x5a)) << 8))
            .collect()
    };
    words
        .iter()
        .enumerate()
        .map(|(i, v)| format!("MOV H{v:04X} D{}", usize::from(base) + i))
        .collect::<Vec<_>>()
        .join("\n")
}
fn target_seed(base: u16) -> String {
    format!(
        "MOV H1357 D{}\nMOV H5AA5 D{base}\nMOV H6BB6 D{}\nMOV H2468 D{}",
        base - 1,
        base + 1,
        base + 2
    )
}
fn eq(d: u16, v: u16) -> String {
    format!("LD= D{d} H{v:04X}")
}

// Independent size accounting is intentionally limited to the fixture's
// pre-header instructions. No production encoder, parser or checksum is used.
fn prefix_words(source: &str) -> u16 {
    source
        .lines()
        .map(|line| match line.split_whitespace().next().unwrap() {
            "LD" | "LDI" | "SET" => 1,
            // RST uses header 0700 plus its two-WORD logical-bit descriptor.
            "RST" => 3,
            "INC" => 3,
            "MOV" => 5,
            "DMOV" => 9,
            "LD=" | "OR=" | "AND=" => 5,
            other => panic!("unaccounted fixture prefix opcode {other}"),
        })
        .sum()
}

struct Fixture<'a> {
    name: &'a str,
    op: &'a str,
    low: bool,
    setup: String,
    source: String,
    count: String,
    target: String,
    // Concrete cells are fixture-resolved, not inferred from indexed labels.
    reads: Vec<u16>,
    before: [u16; 2],
    after: [u16; 2],
    error_offset: Option<u16>,
    sticky: bool,
    guards: [String; 2],
}
fn fixture(f: Fixture<'_>) -> Case {
    assert!(FORMS.contains(&f.op));
    assert!((1..=2).contains(&f.reads.len()));
    assert!(
        !f.sticky || f.error_offset.is_none(),
        "sticky tests hold an existing error on success"
    );
    let mut pre = "LD X1\nMOV K0 D310\nLDI X7\nINC D310".to_owned();
    for bank in ['E', 'F'] {
        for i in 0..8 {
            pre.push_str(&format!("\nMOV K0 {bank}{i}"));
        }
    }
    pre.push_str(&format!("\n{} M1161\nSET M1020\nRST M1021\nSET M1022\n{} M1067\n{} M1068\nRST M832\nRST M833\nMOV H1234 D1067\nMOV K77 D1068\n{}\nLDI X7\nMOV H{:04X} D280\nMOV H{:04X} D281\nMOV H1234 D282\nMOV K77 D283\nMOV K{} D284\nLD X0",
        if f.low {"SET"} else {"RST"}, if f.sticky {"SET"} else {"RST"}, if f.sticky {"SET"} else {"RST"},
        f.setup, f.before[0], f.before[1], if f.sticky {3} else {0}));
    let pc = prefix_words(&pre);
    let instruction = format!("{} {} {} {}", f.op, f.source, f.count, f.target);
    let mut expected = format!(
        "{}\nMOV H{:04X} D280\nMOV H{:04X} D281",
        active(f.op),
        f.after[0],
        f.after[1]
    );
    if let Some(offset) = f.error_offset {
        assert!([1, 3, 5].contains(&offset));
        expected.push_str(&format!(
            "\nMOV HE1A D282\nMOV K{} D283\nMOV K3 D284",
            pc + offset
        ));
    }
    let mut observe = "LDI X7\nMOV K1234 D306".to_owned();
    for (i, d) in f.reads.iter().enumerate() {
        observe.push_str(&format!("\nMOV D{d} D{}", 220 + i));
    }
    let second = if f.reads.len() == 2 {
        "LD= D221 D281"
    } else {
        "LDI X7"
    };
    let source = format!(
        "{pre}\n{instruction}\n{expected}\n{observe}\nLD M1067\nOUT M830\nLD M1068\nOUT M831\nLD= D220 D280\nOUT M840\n{second}\nOUT M841\n{}\nOUT M842\nLD= D1067 D282\nAND= D1068 D283\nOUT M843\nLD= K1M830 D284\nOUT M844\nLD M1020\nANI M1021\nAND M1022\nOUT M845\nLD= D306 K1234\nOUT M846\n{}\nOUT M847\nLDI X7\nMOV K2M840 D308\nEND",
        f.guards[0], f.guards[1]
    );
    assert!(source.lines().count() < 220, "{} fixture size", f.name);
    assert_eq!(
        source
            .lines()
            .filter(|line| line.starts_with("OUT M84"))
            .count(),
        8
    );
    case(
        &format!("indexed_checksum_literal_oracle_{}", f.name),
        &source,
        stages(),
    )
}
fn ordinary_fixture<'a>(
    name: &'a str,
    op: &'a str,
    low: bool,
    setup: String,
    operands: [String; 3],
    base: u16,
) -> Fixture<'a> {
    let [source, count, target] = operands;
    Fixture {
        name,
        op,
        low,
        setup,
        source,
        count,
        target,
        reads: vec![base, base + 1],
        before: [0x5aa5, 0x6bb6],
        after: result(op, low),
        error_offset: None,
        sticky: false,
        guards: [
            format!("{}\nAND= D{} H2468", eq(base - 1, 0x1357), base + 2),
            "LDI X7".into(),
        ],
    }
}

pub(super) fn common_cases() -> Vec<Case> {
    let mut out = Vec::new();
    bank_cases(&mut out); // 32
    literal_count_cases(&mut out); // 8
    alias_cases(&mut out); // 8
    invalid_count_cases(&mut out); // 8
    invalid_ascii_cases(&mut out); // 4
    sticky_cases(&mut out); // 2
    held_update_cases(&mut out); // 2
    scratch_cases(&mut out); // 4 parity-only, six-stage public SORT witnesses
    scratch_zero_literal_cases(&mut out); // 2 ordinary/indexed fixed public witnesses
    checked(out, 70)
}
fn checked(cases: Vec<Case>, count: usize) -> Vec<Case> {
    assert_eq!(cases.len(), count);
    assert_eq!(
        cases.iter().map(|c| &c.name).collect::<BTreeSet<_>>().len(),
        count
    );
    for op in FORMS {
        assert!(cases.iter().any(|c| {
            c.source
                .lines()
                .any(|l| l.split_whitespace().next() == Some(op))
        }));
    }
    cases
}
fn bank_cases(out: &mut Vec<Case>) {
    for bank in ['E', 'F'] {
        for i in 0..8u16 {
            for low in [false, true] {
                let ordinal = i + if bank == 'F' { 8 } else { 0 };
                let op = FORMS[(usize::from(ordinal) + usize::from(low)) % 4];
                let offset = if i % 2 == 0 { -1i16 } else { 1 };
                let s = (100i16 + offset) as u16;
                let n = (40i16 + offset) as u16;
                let d = (200i16 + offset) as u16;
                let name = format!(
                    "all_three_roles_{op}_{bank}{i}_{}_{}",
                    if low { "low" } else { "packed" },
                    if offset < 0 { "minus1" } else { "plus1" }
                );
                let setup = format!(
                    "MOV K{offset} {bank}{i}\nMOV H1234 D{}\n{}\nMOV K{} D{n}\n{}",
                    alias(bank, i) + 1,
                    bytes(s, data(op), low),
                    data(op).len(),
                    target_seed(d)
                );
                out.push(fixture(ordinary_fixture(
                    &name,
                    op,
                    low,
                    setup,
                    [
                        format!("D100{bank}{i}"),
                        format!("D40{bank}{i}"),
                        format!("D200{bank}{i}"),
                    ],
                    d,
                )));
            }
        }
    }
}
fn literal_count_cases(out: &mut Vec<Case>) {
    for op in FORMS {
        for low in [false, true] {
            let name = format!(
                "white_count_{op}_{}",
                if low { "hex_low" } else { "decimal_packed" }
            );
            // ABCDEF decodes AB+CD+EF=267h, so the LRC is 99h. This
            // independent uppercase vector complements the manual digit-only one.
            let input = if lrc(op) && !low {
                b"ABCDEF".as_slice()
            } else {
                data(op)
            };
            let setup = format!("{}\n{}", bytes(100, input, low), target_seed(200));
            let count = if low {
                format!("H{:X}", input.len())
            } else {
                format!("K{}", input.len())
            };
            // Alternate which gray role is addressed while preserving direct fields.
            let source = if low { "D100" } else { "D100E0" };
            let mut f = ordinary_fixture(
                &name,
                op,
                low,
                setup,
                [source.into(), count, "D200F7".into()],
                200,
            );
            if lrc(op) && !low {
                f.after[0] = 0x3939;
            }
            out.push(fixture(f));
        }
    }
}
fn alias_cases(out: &mut Vec<Case>) {
    for algorithm in ["LRC", "CRC"] {
        for kind in 0..4 {
            let op = match (algorithm, kind % 2) {
                ("LRC", 0) => "LRC",
                ("LRC", _) => "LRCP",
                ("CRC", 0) => "CRC",
                _ => "CRCP",
            };
            let low = kind % 2 == 0;
            let name = format!("alias_{op}_{kind}");
            let mut f = ordinary_fixture(
                &name,
                op,
                low,
                String::new(),
                [
                    "D100E0".into(),
                    format!("K{}", data(op).len()),
                    "D200F0".into(),
                ],
                200,
            );
            match kind {
                0 => {
                    // Complete source is consumed before overlapping result writes.
                    f.setup = bytes(100, data(op), low);
                    f.target = "D100F0".into();
                    f.reads = vec![100, 101];
                    f.before = [
                        0xa500 | u16::from(data(op)[0]),
                        0xa500 | u16::from(data(op)[1]),
                    ];
                    f.guards = [eq(102, 0xa500 | u16::from(data(op)[2])), "LDI X7".into()];
                }
                1 => {
                    // Result overwrites the already snapshotted dynamic count.
                    f.setup = format!(
                        "{}\nMOV K{} D40\nMOV H6BB6 D41\nMOV H2468 D42",
                        bytes(100, data(op), low),
                        data(op).len()
                    );
                    f.count = "D40E0".into();
                    f.target = "D40F0".into();
                    f.reads = vec![40, 41];
                    f.before = [data(op).len() as u16, 0x6bb6];
                    f.guards = [eq(42, 0x2468), "LDI X7".into()];
                }
                2 => {
                    // First output byte changes E1; second must use cached D1183.
                    f.setup = format!(
                        "{}\nMOV K0 E1\nMOV H6BB6 D1183\nMOV H2468 D1184",
                        bytes(100, data(op), low)
                    );
                    f.target = "D1182E1".into();
                    f.reads = vec![1182, 1183];
                    f.before = [0, 0x6bb6];
                    f.guards = [eq(1184, 0x2468), "LDI X7".into()];
                }
                _ => {
                    // Count itself is the signed source/destination offset WORD.
                    let n = data(op).len() as u16;
                    let d = 200 + n;
                    f.setup = format!(
                        "MOV K{n} E0\n{}\n{}",
                        bytes(100 + n, data(op), low),
                        target_seed(d)
                    );
                    f.count = "D1028F0".into();
                    f.target = "D200E0".into();
                    f.reads = vec![d, d + 1];
                    f.guards = [
                        format!("{}\nAND= D{} H2468", eq(d - 1, 0x1357), d + 2),
                        format!("LD= E0 K{n}"),
                    ];
                }
            }
            out.push(fixture(f));
        }
    }
}
fn invalid_count_cases(out: &mut Vec<Case>) {
    for algorithm in ["LRC", "CRC"] {
        for (i, n) in (if algorithm == "LRC" {
            vec![0, -1, 257, 3]
        } else {
            vec![0, -1, 257, -32768]
        })
        .into_iter()
        .enumerate()
        {
            let op = match (algorithm, i % 2) {
                ("LRC", 0) => "LRC",
                ("LRC", _) => "LRCP",
                ("CRC", 0) => "CRC",
                _ => "CRCP",
            };
            let name = format!("invalid_count_{op}_{n}_skips_bad_source_and_target");
            let mut f = ordinary_fixture(
                &name,
                op,
                i % 2 != 0,
                format!("MOV K-1 E1\nMOV K-1 F1\nMOV K{n} D40\n{}", target_seed(200)),
                ["D0E1".into(), "D40E0".into(), "D0F1".into()],
                200,
            );
            // These addresses are never visited: this is the proven domain branch,
            // not an assertion about native invalid-address continuation.
            f.after = f.before;
            f.error_offset = Some(3);
            out.push(fixture(f));
        }
    }
}
fn invalid_ascii_cases(out: &mut Vec<Case>) {
    for op in ["LRC", "LRCP"] {
        for low in [false, true] {
            let name = format!(
                "invalid_ASCII_{op}_{}",
                if low {
                    "low_rejects_lowercase"
                } else {
                    "packed_raw_negative_word"
                }
            );
            let mut f = ordinary_fixture(
                &name,
                op,
                low,
                format!(
                    "{}\nMOV K-1 F1\n{}",
                    bytes(100, &[b'0', b'0', b'0', if low { b'a' } else { 0x80 }], low),
                    target_seed(200)
                ),
                ["D100E0".into(), "K4".into(), "D0F1".into()],
                200,
            );
            f.after = f.before;
            f.error_offset = Some(1);
            out.push(fixture(f));
        }
    }
}
fn sticky_cases(out: &mut Vec<Case>) {
    for op in ["LRCP", "CRCP"] {
        let name = format!("sticky_error_success_{op}");
        let mut f = ordinary_fixture(
            &name,
            op,
            false,
            format!("{}\n{}", bytes(100, data(op), false), target_seed(200)),
            [
                "D100E0".into(),
                format!("K{}", data(op).len()),
                "D200F0".into(),
            ],
            200,
        );
        f.sticky = true;
        out.push(fixture(f));
    }
}
fn held_update_cases(out: &mut Vec<Case>) {
    for op in ["LRCP", "CRCP"] {
        let name = format!("held_rearm_source_changes_{op}");
        // Every scan already reseeds the target; a held P site must leave the
        // sentinel. At scan3 the new source is different but valid; scan5 is
        // restored to the original literal vector, making rearm discriminating.
        let mut setup = format!(
            "{}\n{}\nLD= D310 K3",
            bytes(100, data(op), false),
            target_seed(200)
        );
        setup.push_str(if lrc(op) {
            "\nMOV H3030 D100"
        } else {
            "\nMOV HFFFF D100"
        });
        out.push(fixture(ordinary_fixture(
            &name,
            op,
            false,
            setup,
            [
                "D100E0".into(),
                format!("K{}", data(op).len()),
                "D200F0".into(),
            ],
            200,
        )));
    }
}

pub(super) fn model_cases(target: Target) -> Vec<Case> {
    let last = match target {
        Target::Es2 => 9999u16,
        Target::Ss2 => 4999u16,
    };
    let mut out = Vec::new();
    for op in FORMS {
        let name = format!(
            "{op}_packed_last_word_D{last}_{}",
            if lrc(op) {
                "complete"
            } else {
                "odd_truncated_native_extension"
            }
        );
        let mut f = ordinary_fixture(
            &name,
            op,
            false,
            format!("MOV H3030 D{last}\n{}", target_seed(200)),
            [
                format!("D{last}E0"),
                if lrc(op) { "K2".into() } else { "K3".into() },
                "D200F0".into(),
            ],
            200,
        );
        f.after[0] = if lrc(op) { 0x3030 } else { 0xa415 };
        out.push(fixture(f));
        let name = format!("{op}_low_last_destination_D{last}_partial_then_E1A5");
        let mut f = ordinary_fixture(
            &name,
            op,
            true,
            format!(
                "{}\nMOV H1357 D{}\nMOV H5AA5 D{last}",
                bytes(100, data(op), true),
                last - 1
            ),
            [
                "D100E0".into(),
                format!("K{}", data(op).len()),
                format!("D{last}F0"),
            ],
            200,
        );
        f.reads = vec![last];
        f.guards = [eq(last - 1, 0x1357), "LDI X7".into()];
        f.error_offset = Some(5);
        out.push(fixture(f));
    }
    checked(out, 8)
}

fn scratch_cases(out: &mut Vec<Case>) {
    for (op, low, word, variant) in [
        ("LRC", false, 0x3030u16, "packed_valid_positive"),
        ("LRCP", false, 0x8030, "packed_invalid_negative"),
        ("LRC", true, 0x3030, "low_holds_scratch"),
        ("CRC", false, 0x3030, "crc_holds_scratch"),
    ] {
        // Retained SER/SORT protocol: initialize the SAME SORT site first;
        // SER publishes zero scratch before the tested checksum. No private
        // scratch address is exposed as a PLC register. Numeric SORT outputs
        // and completion are differential witnesses, not new literal oracles.
        let source = format!(
            "LD X1\nMOV K0 E0\nMOV K0 F0\nMOV K7 D750\nMOV K30 D400\nMOV K10 D401\nMOV K20 D402\nMOV K3 D403\nMOV K1 D404\nMOV K2 D405\nMOV K-777 D500\nMOV K-777 D501\nMOV K-777 D502\nMOV K-777 D503\nMOV K-777 D504\nMOV K-777 D505\nRST M1029\nMOV H{word:04X} D100\nMOV H3030 D101\n{} M1161\nLD X4\nSER D750 K0 D760 K1\nLD X2\n{op} D100E0 K2 D200F0\nLD X0\nSORT D400 K3 K2 D500 K2\nOUT Y10\nLDI X7\nMOV D500 D220\nMOV D501 D221\nMOV D502 D222\nMOV D503 D223\nMOV D504 D224\nMOV D505 D225\nMOV D200 D226\nMOV D201 D227\nMOV D1067 D228\nMOV D1068 D229\nLD M1029\nOUT Y11\nLD M1067\nOUT Y12\nLD M1068\nOUT Y13\nEND",
            if low { "SET" } else { "RST" }
        );
        let mut c = case(
            &format!("indexed_checksum_private_SORT_scratch_{variant}_{op}"),
            &source,
            vec![
                (
                    vec![
                        ("X1", true),
                        ("X4", true),
                        ("X0", false),
                        ("X2", false),
                        ("X7", false),
                    ],
                    1,
                ),
                (vec![("X1", false), ("X4", false), ("X0", true)], 1),
                (vec![("X4", true), ("X2", true)], 2),
                (vec![("X2", false)], 1),
                (vec![("X0", false), ("X4", false)], 1),
                (vec![("X0", true), ("X2", true), ("X4", true)], 1),
            ],
        );
        // Keep the explicit non-oracle name; parent marker detection must not
        // treat private state or an implementation-produced answer as truth.
        assert!(!c.name.contains("literal_oracle"));
        c.name.shrink_to_fit();
        out.push(c);
    }
}

fn scratch_zero_literal_cases(out: &mut Vec<Case>) {
    for indexed in [false, true] {
        let target = if indexed { "D220F1" } else { "D220" };
        let source = if indexed { "D200E1" } else { "D200" };
        // This is the parent's unchanged-image BEFORE fixture, with explicit
        // guard/marker observations added. The independent expectation comes
        // from the closed SORT zero-threshold contract: no row can advance, so
        // its first forward copy stays [30,10,20,3,1,2] and M1029 remains OFF.
        // The BEFORE native report corroborates the counterexample; its output
        // is not used to generate or select any expected value here.
        let text = format!(
            "LDI M10\nMOV K30 D0\nMOV K10 D1\nMOV K20 D2\nMOV K3 D3\nMOV K1 D4\nMOV K2 D5\nFMOV K-777 D100 K6\nMOV K0 D200\nMOV K0 E1\nMOV K0 F1\nMOV H55AA D220\nMOV H66BB D221\nRST M1161\nRST M1029\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\nSET M10\nLDI X7\nLRC {source} K2 {target}\nSORT D0 K3 K2 D100 K2\nMOV K1234 D300\nLD= D100 K30\nAND= D101 K10\nAND= D102 K20\nOUT M840\nLD= D103 K3\nAND= D104 K1\nAND= D105 K2\nOUT M841\nLDI M1029\nOUT M842\nLD M1067\nAND M1068\nAND= D1067 HE1A\nOUT M843\nLD= D220 H55AA\nAND= D221 H66BB\nOUT M844\nLD= D300 K1234\nOUT M845\nLD= E1 K0\nAND= F1 K0\nOUT M846\nLDI M1161\nOUT M847\nLDI X7\nMOV K2M840 D308\nEND"
        );
        out.push(case(
            &format!(
                "indexed_checksum_literal_oracle_packed_invalid_zero_SORT_{}",
                if indexed {
                    "addressed"
                } else {
                    "ordinary_shared_seam"
                }
            ),
            &text,
            vec![(vec![("X7", false)], 5)],
        ));
    }
}
