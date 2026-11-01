//! Independent values, alias ordering and operand-role limits for data processing.
use crate::data_processing::DataAction;
use crate::plc::{Area, Device, Memory, ScanStatus, Simulator, compile_il};
use std::collections::BTreeSet;

fn d(text: &str) -> Device {
    text.parse().unwrap()
}
fn at(base: Device, offset: u16) -> Device {
    Device {
        index: base.index + offset,
        ..base
    }
}
fn put(m: &mut Memory, base: Device, width: u16, value: u32) {
    for n in 0..width {
        let bit = at(base, n);
        if bit.area == Area::X {
            m.set_input(bit, value & (1 << n) != 0);
        } else {
            m.set_internal(bit, value & (1 << n) != 0);
        }
    }
}
fn get(m: &Memory, base: Device, width: u16) -> u32 {
    (0..width).fold(0, |value, n| value | (u32::from(m.bit(at(base, n))) << n))
}
fn action(name: &str, args: &[&str]) -> DataAction {
    DataAction::parse(
        name,
        &args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(),
    )
    .unwrap()
}

#[test]
fn decoder_and_encoder_use_actual_index_words_and_snapshot_overlapping_input() {
    for (index, low, high) in [(0, 1028, 1029), (1, 1182, 1183), (7, 1194, 1195)] {
        let mut m = Memory::default();
        let e = format!("E{index}");
        let f = format!("F{index}");
        let e_word = d(&format!("D{low}"));
        let f_word = d(&format!("D{high}"));
        m.set_word(e_word, 0x7ffa);
        action("DECO", &[&e, &f, "K4"]).execute(&mut m);
        assert_eq!(m.word(f_word), 1024);
        action("ENCO", &[&f, &e, "K4"]).execute(&mut m);
        assert_eq!(m.word(e_word), 10);
        m.set_word(f_word, 0);
        action("ENCO", &[&f, &e, "K4"]).execute(&mut m);
        assert_eq!(m.word(e_word), 10, "empty encode holds its target");
        assert_eq!(m.word(d("D1067")), 0xe1a);
        assert_eq!(m.word(d("D1068")), 3);
    }
    let mut m = Memory::default();
    put(&mut m, d("M128"), 8, 3);
    action("DECO", &["M128", "M128", "K3"]).execute(&mut m);
    assert_eq!(
        get(&m, d("M128"), 8),
        8,
        "writes cannot change decoded input"
    );
    m.set_word(d("D1182"), 0x8101u16 as i16);
    action("ENCO", &["E1", "E1", "K4"]).execute(&mut m);
    assert_eq!(m.word(d("D1182")), 15);
}

#[test]
fn population_count_reads_only_declared_group_width_and_clears_wide_target_high_half() {
    for wide in [false, true] {
        let op = if wide { "DSUM" } else { "SUM" };
        for digits in 1..=if wide { 8 } else { 4 } {
            for area in ["X200", "Y200", "M128", "S128"] {
                let width = digits * 4;
                let mask = ((1u64 << width) - 1) as u32;
                let pattern = 0x8f55_a807 & mask;
                let mut m = Memory::default();
                put(&mut m, d(area), width, pattern);
                put(&mut m, at(d(area), width), 1, 1);
                m.set_double_word(d("D1182"), -1);
                m.set_system(1020, true);
                m.set_system(1021, true);
                m.set_system(1022, true);
                action(op, &[&format!("K{digits}{area}"), "E1"]).execute(&mut m);
                assert_eq!(m.word(d("D1182")), pattern.count_ones() as i16);
                assert_eq!(m.word(d("D1183")), if wide { 0 } else { -1 });
                assert!(m.system_bit(1020), "nonzero count preserves zero flag");
                assert!(m.system_bit(1021) && m.system_bit(1022));
            }
        }
    }
    let mut m = Memory::default();
    action("SUM", &["K1M128", "F1"]).execute(&mut m);
    assert_eq!(m.word(d("D1183")), 0);
    assert!(m.system_bit(1020));
}

#[test]
fn bit_test_short_groups_zero_extend_and_sample_input_before_output_alias() {
    for (op, digits, last) in [("BON", 4, 15), ("DBON", 8, 31)] {
        let mut m = Memory::default();
        put(&mut m, d("M128"), digits * 4, 1 << last);
        action(
            op,
            &[&format!("K{digits}M128"), "M128", &format!("K{last}")],
        )
        .execute(&mut m);
        assert!(m.bit(d("M128")));
        assert!(m.bit(at(d("M128"), last)));
        action(op, &["K1M128", "M128", &format!("K{last}")]).execute(&mut m);
        assert!(!m.bit(d("M128")), "outside a narrow group is zero");
        assert!(m.bit(at(d("M128"), last)), "outside bits are untouched");
    }
}

#[test]
fn wide_bit_index_f_reads_following_word_and_invalid_index_holds_output() {
    let a = action("DBON", &["H80000000", "M128", "F1"]);
    let mut m = Memory::default();
    m.set_word(d("D1183"), 31);
    m.set_word(d("D1184"), 0);
    a.execute(&mut m);
    assert!(m.bit(d("M128")));
    m.set_word(d("D1184"), 1);
    a.execute(&mut m);
    assert!(m.bit(d("M128")));
    assert_eq!(m.word(d("D1067")), 0xe1a);
    assert_eq!(m.word(d("D1068")), 9);
    let mut observed = BTreeSet::new();
    a.devices(&mut observed);
    assert!(observed.contains(&d("D1183")) && observed.contains(&d("D1184")));
}

#[test]
fn mean_groups_advance_by_declared_width_and_snapshot_before_narrow_target_write() {
    for (op, max_digits) in [("MEAN", 4), ("DMEAN", 8)] {
        for digits in 1..=max_digits {
            for area in ["X200", "Y200", "M128", "S128"] {
                let mut m = Memory::default();
                let base = d(area);
                for (i, value) in [9, 2, 7].into_iter().enumerate() {
                    put(&mut m, at(base, i as u16 * digits * 4), digits * 4, value);
                }
                m.set_internal(d("M511"), true);
                m.set_internal(d("M516"), true);
                let a = action(op, &[&format!("K{digits}{area}"), "K1M512", "K3"]);
                a.execute(&mut m);
                assert_eq!(get(&m, d("M512"), 4), 6, "{op} K{digits}{area}");
                assert!(m.bit(d("M511")) && m.bit(d("M516")));
                assert_eq!(a.display_value(&m), "6");
            }
        }
    }
    let mut m = Memory::default();
    for (offset, value) in [(0, 1), (4, 2), (8, 12)] {
        put(&mut m, at(d("M128"), offset), 4, value);
    }
    action("MEAN", &["K1M128", "K1M132", "K3"]).execute(&mut m);
    assert_eq!(get(&m, d("M128"), 12), 0xc51);
}

#[test]
fn mean_count_is_sampled_before_aliased_target_and_observes_original_span() {
    for op in ["MEAN", "DMEAN"] {
        let p = compile_il(&format!("LD X200\n{op} K1M128 K1M200 K1M200\nEND")).unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X200"), true);
        for (offset, value) in [(0, 4), (4, 8), (8, 12)] {
            put(&mut s.memory, at(d("M128"), offset), 4, value);
        }
        put(&mut s.memory, d("M200"), 4, 3);
        s.scan(&p, 0);
        assert_eq!(s.last_scan, ScanStatus::Complete);
        assert_eq!(get(&s.memory, d("M200"), 4), 8);
        assert!(s.observed_devices.contains(&d("M139")));
        assert!(!s.observed_devices.contains(&d("M140")));
    }
}

#[test]
fn wide_mean_count_f_uses_adjacent_d_and_preserves_target_on_error() {
    let a = action("DMEAN", &["K1M128", "E1", "F2"]);
    let mut m = Memory::default();
    put(&mut m, d("M128"), 8, 0xc4);
    m.set_word(d("D1185"), 2);
    m.set_word(d("D1186"), 0);
    a.execute(&mut m);
    assert_eq!(m.double_word(d("D1182")), 8);
    m.set_word(d("D1186"), -1);
    a.execute(&mut m);
    assert_eq!(m.double_word(d("D1182")), 8);
    assert_eq!(m.word(d("D1067")), 0xe1a);
    assert_eq!(m.word(d("D1068")), 9);
}

#[test]
fn pulse_data_instructions_hold_then_rearm_with_index_and_group_operands() {
    for (body, before, after) in [
        ("DECOP E1 F1 K4", 3, 8),
        ("ENCOP E1 F1 K4", 8, 3),
        ("SUMP E1 F1", 7, 3),
        ("DSUMP K8M128 E1", 7, 3),
        ("BONP K4M128 M512 E1", 0, 1),
        ("DBONP K8M128 M512 E1", 0, 1),
        ("MEANP K1M128 F1 K1", 7, 7),
        ("DMEANP K1M128 E1 K1", 7, 7),
    ] {
        let p = compile_il(&format!("LD X200\n{body}\nEND")).unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X200"), true);
        s.memory.set_double_word(d("D1182"), before);
        put(
            &mut s.memory,
            d("M128"),
            32,
            if body.contains("BON") {
                1
            } else {
                before as u32
            },
        );
        s.scan(&p, 0);
        let result = |m: &Memory| {
            if body.contains("BON") {
                i32::from(m.bit(d("M512")))
            } else if body.starts_with('D') && !body.starts_with("DECO") {
                m.double_word(d("D1182"))
            } else {
                i32::from(m.word(d("D1183")))
            }
        };
        assert_eq!(result(&s.memory), after, "{body}");
        if !body.starts_with("DSUMP") && !body.starts_with("DMEANP") {
            s.memory.set_word(d("D1182"), 0);
        }
        put(&mut s.memory, d("M128"), 32, 0);
        let held = result(&s.memory);
        s.scan(&p, 0);
        assert_eq!(result(&s.memory), held, "powered pulse holds {body}");
        s.memory.set_input(d("X200"), false);
        s.scan(&p, 0);
        s.memory.set_input(d("X200"), true);
        s.scan(&p, 0);
        let expected = if body.starts_with("DECO") {
            1
        } else if body.starts_with("ENCO") {
            held
        } else {
            0
        };
        assert_eq!(result(&s.memory), expected, "rearmed {body}");
    }
}

#[test]
fn group_mean_full_width_is_signed_short_width_is_unsigned_and_double_divides_each_term() {
    for (op, digits, values, expected) in [
        ("MEAN", 4, [4, 0xffff], 1),
        ("DMEAN", 8, [4, 0xffff_ffff], 2),
        ("MEAN", 3, [0xfff, 1], 2048),
        ("DMEAN", 7, [0xfff_ffff, 1], 0x800_0000),
    ] {
        let mut m = Memory::default();
        for (i, value) in values.into_iter().enumerate() {
            put(
                &mut m,
                at(d("M128"), i as u16 * digits * 4),
                digits * 4,
                value,
            );
        }
        action(op, &[&format!("K{digits}M128"), "E1", "K2"]).execute(&mut m);
        assert_eq!(
            if op == "DMEAN" {
                m.double_word(d("D1182"))
            } else {
                i32::from(m.word(d("D1182")))
            },
            expected
        );
    }
}

#[test]
fn numeric_mean_partial_model_range_remains_usable_with_new_destinations() {
    for (op, input, expected) in [("MEAN", -7, 13), ("DMEAN", -7, 12)] {
        let mut m = Memory::default();
        m.data_register_limit = Some(4999);
        m.set_word(d("D4999"), input);
        m.set_word(d("D5000"), 1234);
        m.set_internal(d("M511"), true);
        m.set_internal(d("M516"), true);
        let a = action(op, &["D4999", "K1M512", "K2"]);
        let mut observed = BTreeSet::new();
        a.observed_devices(&m, &mut observed);
        assert!(observed.contains(&d("D4999")));
        assert!(!observed.contains(&d("D5000")));
        a.execute(&mut m);
        assert_eq!(get(&m, d("M512"), 4), expected);
        assert!(m.bit(d("M511")) && m.bit(d("M516")));
    }
}

#[test]
fn group_mean_endpoint_policy_reads_valid_low_bits_and_keeps_requested_denominator() {
    // PDF valid-address policy. Native raw-pointer behavior at this boundary
    // is separately qualified; this is not a claim of complete native parity.
    for (op, digits) in [("MEAN", 1), ("DMEAN", 1)] {
        let mut m = Memory::default();
        put(&mut m, d("S1018"), 6, 0x3f);
        let a = action(op, &[&format!("K{digits}S1018"), "D0", "K3"]);
        let mut observed = BTreeSet::new();
        a.observed_devices(&m, &mut observed);
        for index in 1018..=1023 {
            assert!(observed.contains(&d(&format!("S{index}"))));
        }
        assert!(
            !observed
                .iter()
                .any(|device| device.area == Area::S && device.index > 1023)
        );
        a.execute(&mut m);
        assert_eq!(m.word(d("D0")), 6, "(15+3+0)/3: {op}");
    }
}

#[test]
fn role_matrix_does_not_admit_kn_or_indices_where_manual_excludes_them() {
    for body in [
        "DECO K1M0 D0 K1",
        "DECO E1 K4M0 K1",
        "ENCO K1M0 D0 K1",
        "ENCO K1 E1 K1",
        "ENCO E1 K1M0 K1",
        "SUM E1 K1M0",
        "DSUM F1 E1",
        "DSUM E1 F1",
        "BON E1 Y0 K1M0",
        "DBON F1 Y0 K0",
        "MEAN E1 D0 K1",
        "MEAN F1 D0 K1",
        "MEAN K1 D0 K1",
        "DMEAN K1M0 F1 K1",
        "SQR E1 D0",
        "SQR K1M0 D0",
        "SQR D0 E1",
        "DSQR T0 D0",
    ] {
        assert!(
            compile_il(&format!("LD X0\n{body}\nEND")).is_err(),
            "{body}"
        );
    }
}
