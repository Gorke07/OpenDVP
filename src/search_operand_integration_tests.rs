//! Independent literal results for SER's direct E/F search-value operands.
use crate::plc::{Area, Device, Memory, ScanStatus, Simulator, compile_il};
use crate::search_ops::SearchAction;

// Literal manual aliases; deliberately independent of CalendarIndices.
const INDEX_WORDS: [(u16, u16); 8] = [
    (1028, 1029),
    (1182, 1183),
    (1184, 1185),
    (1186, 1187),
    (1188, 1189),
    (1190, 1191),
    (1192, 1193),
    (1194, 1195),
];

fn d(text: &str) -> Device {
    text.parse().unwrap()
}
fn at(base: Device, offset: u16) -> Device {
    Device {
        index: base.index + offset,
        ..base
    }
}
fn action(name: &str, args: &[&str]) -> SearchAction {
    SearchAction::parse(
        name,
        &args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn put_words(m: &mut Memory, base: Device, wide: bool, values: &[i32]) {
    for (index, value) in values.iter().enumerate() {
        let target = at(base, index as u16 * if wide { 2 } else { 1 });
        if wide {
            m.set_double_word(target, *value);
        } else {
            m.set_word(target, *value as i16);
        }
    }
}
fn results(m: &Memory, base: Device, wide: bool) -> [i32; 5] {
    std::array::from_fn(|index| {
        let target = at(base, index as u16 * if wide { 2 } else { 1 });
        if wide {
            m.double_word(target)
        } else {
            i32::from(m.word(target))
        }
    })
}
fn put_bits(m: &mut Memory, base: Device, width: u16, value: i32) {
    for bit in 0..width {
        let target = at(base, bit);
        let enabled = value as u32 & (1 << bit) != 0;
        if target.area == Area::X {
            m.set_input(target, enabled);
        } else {
            m.set_internal(target, enabled);
        }
    }
}
fn get_bits(m: &Memory, base: Device, width: u16) -> u32 {
    (0..width).fold(0, |value, bit| {
        value | (u32::from(m.bit(at(base, bit))) << bit)
    })
}

#[test]
fn every_word_index_reads_its_literal_alias_and_uses_signed_extrema() {
    for (index, (low, high)) in INDEX_WORDS.into_iter().enumerate() {
        let mut m = Memory::default();
        put_words(&mut m, d("D0"), false, &[-7, 300, -7, -100, 300, -100]);
        m.set_word(d(&format!("D{low}")), -7);
        m.set_word(d(&format!("D{high}")), 300);
        for flag in 1020..=1022 {
            m.set_system(flag, true);
        }
        for (needle, expected) in [
            (format!("E{index}"), [2, 0, 2, 5, 4]),
            (format!("F{index}"), [2, 1, 4, 5, 4]),
        ] {
            put_words(&mut m, d("D40"), false, &[-1; 5]);
            action("SER", &["D0", &needle, "D40", "K6"]).execute(&mut m);
            assert_eq!(results(&m, d("D40"), false), expected, "{needle}");
            assert!((1020..=1022).all(|flag| m.system_bit(flag)));
            assert_eq!(m.word(d(&format!("D{low}"))), -7);
            assert_eq!(m.word(d(&format!("D{high}"))), 300);
        }
        m.set_word(d(&format!("D{high}")), 123);
        action("SER", &["D0", &format!("F{index}"), "D40", "K6"]).execute(&mut m);
        assert_eq!(results(&m, d("D40"), false), [0, 0, 0, 5, 4]);
    }
}

#[test]
fn every_double_e_combines_low_and_high_alias_words_before_signed_search() {
    for (index, (low, high)) in INDEX_WORDS.into_iter().enumerate() {
        let mut m = Memory::default();
        put_words(
            &mut m,
            d("D0"),
            true,
            &[70000, -70000, 65536, -70000, -80000, -80000, 70000],
        );
        m.set_word(d(&format!("D{low}")), 0xee90u16 as i16);
        m.set_word(d(&format!("D{high}")), -2);
        m.set_word(d("D39"), 1234);
        m.set_word(d("D50"), 2345);
        put_words(&mut m, d("D40"), true, &[-1; 5]);
        let op = action("DSER", &["D0", &format!("E{index}"), "D40", "K7"]);
        op.execute(&mut m);
        assert_eq!(results(&m, d("D40"), true), [2, 1, 3, 5, 6]);
        for high_word in [41, 43, 45, 47, 49] {
            assert_eq!(m.word(d(&format!("D{high_word}"))), 0);
        }
        // Keeping the low word and changing only the high word makes 61072,
        // which is absent. Reading E as only its low word would miss both cases.
        m.set_word(d(&format!("D{high}")), 0);
        op.execute(&mut m);
        assert_eq!(results(&m, d("D40"), true), [0, 0, 0, 5, 6]);
        assert_eq!(m.word(d("D39")), 1234);
        assert_eq!(m.word(d("D50")), 2345);
    }
}

#[test]
fn aliased_index_needle_and_complete_source_are_snapshotted_before_results() {
    for (needle, expected) in [("E1", [3, 0, 4, 3, 4]), ("F1", [1, 1, 1, 3, 4])] {
        let mut m = Memory::default();
        put_words(&mut m, d("D1182"), false, &[7, -3, 7, -9, 7]);
        action("SER", &["D1182", needle, "D1182", "K5"]).execute(&mut m);
        assert_eq!(results(&m, d("D1182"), false), expected, "{needle}");
    }
    let mut m = Memory::default();
    put_words(&mut m, d("D1182"), true, &[70000, -7, 70000, -8, 70000]);
    action("DSER", &["D1182", "E1", "D1182", "K5"]).execute(&mut m);
    assert_eq!(results(&m, d("D1182"), true), [3, 0, 4, 3, 4]);
}

#[test]
fn packed_sources_keep_instruction_signed_width_and_packed_results_keep_guards() {
    for (
        name,
        source,
        source_base,
        source_bits,
        values,
        needle,
        target,
        target_base,
        target_bits,
        expected,
    ) in [
        (
            "SER",
            "K4X200",
            "X200",
            16,
            [-1, 2, -1, -5, 2],
            -1,
            "K1Y200",
            "Y200",
            4,
            [2, 0, 2, 3, 4],
        ),
        (
            "DSER",
            "K4M0",
            "M0",
            16,
            [65535, 7, 65535, 0, 9],
            65535,
            "K1S128",
            "S128",
            4,
            [2, 0, 2, 3, 2],
        ),
        (
            "DSER",
            "K8Y0",
            "Y0",
            32,
            [-70000, 9, -70000, -80000, 9],
            -70000,
            "K2M128",
            "M128",
            8,
            [2, 0, 2, 3, 4],
        ),
    ] {
        let mut m = Memory::default();
        for (index, value) in values.into_iter().enumerate() {
            put_bits(
                &mut m,
                at(d(source_base), index as u16 * source_bits),
                source_bits,
                value,
            );
        }
        m.set_double_word(d("D1182"), needle);
        let start = d(target_base);
        let before = Device {
            index: start.index - 1,
            ..start
        };
        let after = at(start, target_bits * 5);
        m.set_internal(before, true);
        m.set_internal(after, true);
        for index in 0..5 {
            put_bits(&mut m, at(start, index * target_bits), target_bits, -1);
        }
        action(name, &[source, "E1", target, "K5"]).execute(&mut m);
        let actual: [u32; 5] = std::array::from_fn(|index| {
            get_bits(&m, at(start, index as u16 * target_bits), target_bits)
        });
        assert_eq!(actual, expected, "{name} {source} -> {target}");
        assert!(m.bit(before) && m.bit(after));
    }
}

#[test]
fn invalid_counts_with_index_needles_hold_all_results_and_keep_count_error_offsets() {
    for (name, needle, invalid, wide, offset) in [
        ("SER", "E0", "K0", false, 7),
        ("SER", "F7", "K257", false, 7),
        ("SER", "F1", "D20", false, 7),
        ("DSER", "E0", "K0", true, 13),
        ("DSER", "E7", "K129", true, 13),
        ("DSER", "E1", "D20", true, 13),
    ] {
        let mut m = Memory::default();
        m.set_word(d("D20"), -1);
        m.set_word(d("D21"), -1);
        put_words(&mut m, d("D40"), wide, &[51, 52, 53, 54, 55]);
        m.set_system(1020, true);
        m.set_system(1022, true);
        action(name, &["D0", needle, "D40", invalid]).execute(&mut m);
        assert_eq!(results(&m, d("D40"), wide), [51, 52, 53, 54, 55]);
        assert_eq!(m.word(d("D1067")), 0xe1a);
        assert_eq!(m.word(d("D1068")), offset);
        assert!(m.system_bit(1067) && m.system_bit(1068));
        assert!(m.system_bit(1020) && !m.system_bit(1021) && m.system_bit(1022));
    }
}

#[test]
fn all_four_spellings_observe_changed_index_values_only_when_powered_or_rearmed() {
    for (name, wide, pulse) in [
        ("SER", false, false),
        ("SERP", false, true),
        ("DSER", true, false),
        ("DSERP", true, true),
    ] {
        let p = compile_il(&format!("LD X200\n{name} D0 E1 D40 K4\nEND")).unwrap();
        let mut s = Simulator::default();
        put_words(&mut s.memory, d("D0"), wide, &[-7, 9, -7, 0]);
        put_words(&mut s.memory, d("D40"), wide, &[99; 5]);
        s.memory.set_double_word(d("D1182"), -7);
        s.scan(&p, 0);
        assert_eq!(results(&s.memory, d("D40"), wide), [99; 5], "{name}");
        s.memory.set_input(d("X200"), true);
        s.scan(&p, 0);
        assert_eq!(
            results(&s.memory, d("D40"), wide),
            [2, 0, 2, 2, 1],
            "{name}"
        );
        s.memory.set_double_word(d("D1182"), 9);
        s.scan(&p, 0);
        let held = if pulse {
            [2, 0, 2, 2, 1]
        } else {
            [1, 1, 1, 2, 1]
        };
        assert_eq!(results(&s.memory, d("D40"), wide), held, "{name}");
        s.memory.set_input(d("X200"), false);
        s.memory.set_double_word(d("D1182"), 0);
        s.scan(&p, 0);
        assert_eq!(results(&s.memory, d("D40"), wide), held, "{name}");
        s.memory.set_input(d("X200"), true);
        s.scan(&p, 0);
        assert_eq!(s.last_scan, ScanStatus::Complete);
        assert_eq!(
            results(&s.memory, d("D40"), wide),
            [1, 3, 3, 2, 1],
            "{name}"
        );
    }
}

#[test]
fn labels_roundtrip_and_tracking_exposes_exact_index_aliases() {
    for (index, (low, high)) in INDEX_WORDS.into_iter().enumerate() {
        for (name, needle, first, second) in [
            ("SER", format!("E{index}"), low, None),
            ("SERP", format!("F{index}"), high, None),
            ("DSER", format!("E{index}"), low, Some(high)),
            ("DSERP", format!("E{index}"), low, Some(high)),
        ] {
            let body = format!("{name} D0 {needle} D40 K3");
            let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
            assert!(p.devices.contains(&d(&format!("D{first}"))), "{body}");
            if let Some(second) = second {
                assert!(p.devices.contains(&d(&format!("D{second}"))), "{body}");
            } else {
                let unused = if first == low { high } else { low };
                assert!(!p.devices.contains(&d(&format!("D{unused}"))), "{body}");
            }
            let label = p.networks[0].actions[0].label();
            assert_eq!(label, body);
            let restored = compile_il(&format!("LD X200\n{label}\nEND")).unwrap();
            assert_eq!(p.networks[0].actions, restored.networks[0].actions);
            assert_eq!(p.devices, restored.devices);
        }
    }
}

#[test]
fn direct_index_role_limits_and_malformed_modifier_banks_remain_rejected() {
    for name in ["SER", "SERP", "DSER", "DSERP"] {
        for operands in [
            "E1 K1 D40 K1",
            "F1 K1 D40 K1",
            "D0 K1 E1 K1",
            "D0 K1 F1 K1",
            "D0 E1 D40 E1",
            "D0 E1 D40 F1",
            "D0 E1 D40 T0",
            "D0 E1 D40 K1M0",
            "D0 E8 D40 K1",
            "D0 F8 D40 K1",
            "D0 D5E8 D40 K1",
        ] {
            let body = format!("{name} {operands}");
            assert!(
                compile_il(&format!("LD X200\n{body}\nEND")).is_err(),
                "{body}"
            );
        }
    }
    for name in ["DSER", "DSERP"] {
        for index in 0..8 {
            let body = format!("{name} D0 F{index} D40 K1");
            assert!(
                compile_il(&format!("LD X200\n{body}\nEND")).is_err(),
                "{body}"
            );
        }
    }
}
