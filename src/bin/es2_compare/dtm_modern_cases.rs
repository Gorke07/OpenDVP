//! Exact-current COMMGR ES2 DTM 30..36 comparisons. The caller must gate
//! these cases by the verified modern simulator digest, never by model alone.
//! Each explicitly seeded queue slot is also a tracked comparison device,
//! including stale slots beyond its current pointer and both guard sentinels.
use super::{Case, case};
use std::collections::BTreeSet;

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (vec![("X1", true), ("X0", false)], 1),
        (vec![("X1", false), ("X0", true)], 2),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}

fn words(base: u16, values: &[i16]) -> String {
    values
        .iter()
        .enumerate()
        .map(|(i, value)| format!("MOV K{value} D{}", base + i as u16))
        .collect::<Vec<_>>()
        .join("\n")
}

fn word_queue(base: u16, pointer: i16, capacity: u16) -> String {
    let mut values = vec![0x6aa6, pointer];
    values.extend((1..=capacity).map(|i| (19 + i * 37) as i16));
    values.push(0x3553);
    words(base - 1, &values)
}

fn bits(base: u16, capacity: u16) -> String {
    (0..capacity)
        .map(|i| {
            format!(
                "{} M{}",
                if i % 3 == 0 || i % 7 == 2 {
                    "SET"
                } else {
                    "RST"
                },
                base + i
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn bit_queue(base: u16, capacity: u16) -> String {
    format!(
        "SET M{}\n{}\nRST M{}",
        base - 1,
        bits(base, capacity),
        base + capacity
    )
}

fn scenario(name: &str, seed: &str, body: &str, observe: &str) -> Case {
    let source = [
        "LD X1",
        seed,
        "SET M1020\nRST M1021\nSET M1022\nSET M1029\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\nMOV K-55 D900\nMOV K0 D901",
        "LD X0",
        body,
        "MOV K123 D900\nINC D901",
        "LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1029\nOUT Y23\nLD M1067\nOUT Y24\nLD M1068\nOUT Y25",
        "LD X0\nORI X0\nMOV D1067 D902\nMOV D1068 D903",
        observe,
        "END",
    ]
    .into_iter()
    .filter(|section| !section.is_empty())
    .collect::<Vec<_>>()
    .join("\n");
    assert!(source.lines().count() <= 512, "oversized DTM case {name}");
    case(&format!("dtm_modern_{name}"), &source, stages())
}

fn word_case(op: &str, mode: u16, pointer: i16, count: i16, tag: &str) -> Case {
    let capacity = count.clamp(1, 256) as u16;
    let (source, dest) = if mode == 31 { (20, 100) } else { (100, 20) };
    scenario(
        &format!("word_{mode}_{op}_{tag}"),
        &format!(
            "{}\nMOV K-123 D19\nMOV K77 D20\nMOV K234 D21",
            word_queue(100, pointer, capacity)
        ),
        &format!(
            "{op} D{source} D{dest} K{mode} K{count}{}",
            if mode == 31 { "\nINC D20" } else { "" }
        ),
        &format!(
            "MOV D100 D910\nMOV D101 D911\nMOV D{} D912\nMOV D{} D913\nMOV D20 D914",
            100 + capacity,
            101 + capacity
        ),
    )
}

fn bit_case(op: &str, mode: u16, pointer: i16, count: i16, tag: &str) -> Case {
    let capacity = count.clamp(1, 256) as u16;
    let queue_base = if capacity == 256 { 2200 } else { 128 };
    let (source, dest) = if mode == 34 { (20, 100) } else { (100, 20) };
    scenario(
        &format!("bit_{mode}_{op}_{tag}"),
        &format!(
            "MOV K-123 D19\nMOV K64 D20\nMOV K234 D21\nMOV K-321 D99\nMOV K{queue_base} D100\nMOV K{pointer} D101\nMOV K432 D102\nSET M63\nSET M64\nRST M65\n{}",
            bit_queue(queue_base, capacity)
        ),
        &format!(
            "{op} D{source} D{dest} K{mode} K{count}{}",
            if mode == 34 { "\nALT M64" } else { "" }
        ),
        "MOV D100 D910\nMOV D101 D911\nLD M64\nOUT Y7",
    )
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();

    // Reversal counts exercise both clamps and the complete 256-word span.
    for op in ["DTM", "DTMP"] {
        for count in [-1i16, 0, 1, 4, 256, 32767] {
            let capacity = count.clamp(1, 256) as u16;
            let values = (0..capacity)
                .map(|i| (0x1234u16.wrapping_add(i.wrapping_mul(257))) as i16)
                .collect::<Vec<_>>();
            cases.push(scenario(
                &format!("reverse_{op}_count_{count}"),
                &format!(
                    "MOV H6AA6 D2199\n{}\nMOV H3553 D{}\nMOV H5AA5 D2999\nFMOV H33CC D3000 K{capacity}\nMOV H3CC3 D{}",
                    words(2200, &values),
                    2200 + capacity,
                    3000 + capacity
                ),
                &format!("{op} D2200 D3000 K30 K{count}\nINC D2200"),
                &format!(
                    "MOV D3000 D910\nMOV D{} D911\nMOV D{} D912",
                    2999 + capacity,
                    3000 + capacity
                ),
            ));
        }
        for (tag, source, dest) in [
            ("same", 20, 20),
            ("forward_live", 20, 21),
            ("backward", 21, 20),
        ] {
            cases.push(scenario(
                &format!("reverse_{op}_overlap_{tag}"),
                &words(19, &[0x6aa6, 1, 2, 4, 8, 16, 0x3553]),
                &format!("{op} D{source} D{dest} K30 K4"),
                "BMOV D19 D910 K7",
            ));
        }
        cases.push(scenario(
            &format!("reverse_{op}_forward_overlap_then_sort_scratch"),
            &format!(
                "{}\n{}\n{}",
                words(0, &[1, 2, 4, 8, 16, 0x3553]),
                words(400, &[30, 10, 20, 3, 1, 2]),
                words(500, &[-123; 6])
            ),
            &format!("{op} D0 D1 K30 K4\nSORT D400 K3 K2 D500 K1"),
            "BMOV D0 D910 K6\nBMOV D500 D920 K6",
        ));
    }
    assert_eq!(cases.len(), 20);

    // Header is the next insertion slot for word queues: one is empty and
    // capacity+1 is full. All capacity slots retain distinctive stale values.
    for mode in 31..=33 {
        for op in ["DTM", "DTMP"] {
            for pointer in [-3, 0, 1, 3, 5, 6] {
                let count = if pointer == 0 { 1 } else { 4 };
                cases.push(word_case(
                    op,
                    mode,
                    pointer,
                    count,
                    &format!("pointer_{pointer}_count_{count}"),
                ));
            }
        }
        for count in [-1i16, 0, 256] {
            let pointer = if count == 256 {
                128
            } else if mode == 31 {
                1
            } else {
                2
            };
            cases.push(word_case(
                "DTM",
                mode,
                pointer,
                count,
                &format!("count_{count}"),
            ));
        }
    }
    assert_eq!(cases.len(), 65);

    for op in ["DTM", "DTMP"] {
        for (tag, source, pointer) in [
            ("source_header_after_normalization", 100, -3),
            ("source_first_payload", 101, 2),
            ("source_last_payload", 104, 4),
        ] {
            cases.push(scenario(
                &format!("word_31_{op}_{tag}"),
                &word_queue(100, pointer, 4),
                &format!("{op} D{source} D100 K31 K4"),
                "BMOV D99 D910 K7",
            ));
        }
        for (mode, tag, dest) in [
            (32, "output_header_then_live_decrement", 100),
            (32, "output_unread_payload_before_shift", 102),
            (32, "output_last_payload_before_shift", 104),
            (33, "output_header_then_reloaded_clear", 100),
            (33, "output_selected_last_slot_is_cleared", 103),
            (33, "output_stale_tail", 104),
        ] {
            // Small positive payloads keep the live-header LIFO alias within
            // its allocated storage, even on the second and rearmed scans.
            cases.push(scenario(
                &format!("word_{mode}_{op}_{tag}"),
                &words(99, &[0x6aa6, 4, 1, 2, 3, 4, 0x3553]),
                &format!("{op} D100 D{dest} K{mode} K4"),
                "BMOV D99 D910 K7",
            ));
        }
    }
    assert_eq!(cases.len(), 83);

    for (mode, source, dest, seed, tag) in [
        (
            31,
            20,
            999,
            "MOV K77 D20\nMOV K1 D999\nMOV H3553 D998",
            "capacity_touches_D1000",
        ),
        (
            31,
            20,
            9999,
            "MOV K77 D20\nMOV K1 D9999\nMOV H3553 D9998",
            "capacity_touches_D10000",
        ),
        (
            32,
            999,
            20,
            "MOV K2 D999\nMOV K77 D20\nMOV H3553 D998",
            "source_capacity_touches_D1000",
        ),
        (
            33,
            9999,
            20,
            "MOV K2 D9999\nMOV K77 D20\nMOV H3553 D9998",
            "source_capacity_touches_D10000",
        ),
        (
            32,
            100,
            1500,
            "MOV K2 D100\nMOV K11 D101\nMOV H3553 D102\nMOV K77 D1500",
            "special_output_silent_hold",
        ),
        (
            33,
            100,
            1500,
            "MOV K2 D100\nMOV K11 D101\nMOV H3553 D102\nMOV K77 D1500",
            "special_output_silent_hold",
        ),
    ] {
        cases.push(scenario(
            &format!("word_{mode}_{tag}"),
            seed,
            &format!("DTM D{source} D{dest} K{mode} K1"),
            "",
        ));
    }
    assert_eq!(cases.len(), 89);

    // Bit queues store a bit address in D and their used length in D+1.
    for mode in 34..=36 {
        for op in ["DTM", "DTMP"] {
            for pointer in [-3, 0, 1, 2, 4, 5] {
                let count = if pointer == 0 { 1 } else { 4 };
                cases.push(bit_case(
                    op,
                    mode,
                    pointer,
                    count,
                    &format!("pointer_{pointer}_count_{count}"),
                ));
            }
        }
        for count in [-1i16, 0, 256] {
            let pointer = if count == 256 {
                128
            } else if mode == 34 {
                0
            } else {
                1
            };
            cases.push(bit_case(
                "DTM",
                mode,
                pointer,
                count,
                &format!("count_{count}"),
            ));
        }
    }
    assert_eq!(cases.len(), 134);

    for op in ["DTM", "DTMP"] {
        for (mode, tag, source, dest, output_bit) in [
            (34, "source_and_target_same_base_pointer", 100, 100, 130),
            (34, "source_pointer_aliases_header", 101, 100, 130),
            (
                35,
                "target_pointer_aliases_header_before_decrement",
                100,
                101,
                130,
            ),
            (
                36,
                "target_pointer_aliases_header_after_decrement",
                100,
                101,
                130,
            ),
            (35, "target_overlaps_unread_shift_source", 100, 20, 130),
            (36, "target_is_selected_last_bit", 100, 20, 130),
            (36, "target_is_first_queue_bit", 100, 20, 128),
        ] {
            cases.push(scenario(
                &format!("bit_{mode}_{op}_{tag}"),
                &format!(
                    "MOV K{output_bit} D20\nMOV K-321 D99\nMOV K128 D100\nMOV K3 D101\nMOV K432 D102\n{}\n{}",
                    bit_queue(128, 4),
                    bits(0, 6)
                ),
                &format!("{op} D{source} D{dest} K{mode} K4"),
                "MOV D100 D910\nMOV D101 D911\nMOV K1M128 D912\nMOV K2M0 D913",
            ));
        }
    }
    assert_eq!(cases.len(), 148);

    for (mode, queue_base, output_bit, pointer, count, tag) in [
        (34, 1500, 64, 0, 4, "special_queue_target_silent_hold"),
        (34, 4095, 64, 0, 1, "queue_capacity_end4096_silent_hold"),
        (35, 128, 1500, 2, 4, "special_output_silent_hold"),
        (35, 4094, 64, 2, 2, "queue_capacity_end4096_silent_hold"),
        (36, 128, 1500, 2, 4, "special_output_silent_hold"),
        (36, 4094, 64, 2, 2, "queue_capacity_end4096_silent_hold"),
        (35, 128, 4095, 2, 4, "single_output_M4095_allowed"),
        (36, 128, 4095, 2, 4, "single_output_M4095_allowed"),
    ] {
        let (source, dest) = if mode == 34 { (20, 100) } else { (100, 20) };
        // Do not seed/write special M1500 merely to test a rejecting guard.
        let queue_seed = if queue_base == 1500 {
            String::new()
        } else {
            bits(queue_base, count)
        };
        cases.push(scenario(
            &format!("bit_{mode}_{tag}"),
            &format!(
                "MOV K{output_bit} D20\nMOV K{queue_base} D100\nMOV K{pointer} D101\nMOV H3553 D102\nSET M63\nSET M64\nRST M65\nSET M4093\nSET M4095\n{queue_seed}"
            ).trim_end(),
            &format!("DTM D{source} D{dest} K{mode} K{count}"),
            "MOV D100 D910\nMOV D101 D911\nLD M4095\nOUT Y7",
        ));
    }
    assert_eq!(cases.len(), 156);

    // Mode30 processes the valid last word before its post-write exit. Neither
    // source references D10000; only the retained native cursor reaches it.
    cases.push(scenario(
        "reverse_source_last_register_partial_exit",
        "MOV H6AA6 D9998\nMOV H1234 D9999\nMOV H5AA5 D99\nMOV H33CC D100\nMOV H55AA D101\nMOV H3553 D102",
        "DTM D9999 D100 K30 K2",
        "MOV D9999 D910\nMOV D100 D911\nMOV D101 D912",
    ));
    cases.push(scenario(
        "reverse_destination_last_register_partial_exit_pulse",
        "MOV H6AA6 D19\nMOV H1234 D20\nMOV H5678 D21\nMOV H3553 D22\nMOV H5AA5 D9998\nMOV H33CC D9999",
        "DTMP D20 D9999 K30 K2\nINC D20",
        "MOV D20 D910\nMOV D9998 D911\nMOV D9999 D912",
    ));
    assert_eq!(cases.len(), 158);

    let names = cases.iter().map(|case| &case.name).collect::<BTreeSet<_>>();
    assert_eq!(names.len(), cases.len());
    let forms = cases
        .iter()
        .flat_map(|case| case.source.lines())
        .filter_map(|line| line.split_whitespace().next())
        .filter(|op| matches!(*op, "DTM" | "DTMP"))
        .collect::<BTreeSet<_>>();
    assert_eq!(forms, BTreeSet::from(["DTM", "DTMP"]));
    assert_eq!(
        cases.iter().map(|case| case.source.lines().count()).max(),
        Some(305)
    );
    cases
}
