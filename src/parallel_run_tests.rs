use crate::parallel_run::ParallelRunAction;
use crate::plc::{Area, Device, Memory};
use std::collections::BTreeSet;

#[test]
fn compiled_four_forms_have_genuine_words_pc_and_pulse_lifecycle() {
    for (name, digits, header, words, pulse) in [
        ("PRUN", 4, 0xb200, 5, false),
        ("PRUNP", 4, 0xb210, 5, true),
        ("DPRUN", 8, 0xb300, 9, false),
        ("DPRUNP", 8, 0xb310, 9, true),
    ] {
        let source = format!("LD X0\n{name} K{digits}X20 K{digits}M10\nMOV K7 D0\nEND");
        let program = crate::plc::compile_il(&source).unwrap();
        let encoded = crate::es2_encoder::encode(&program).unwrap();
        assert_eq!(encoded.words[1], header);
        assert_eq!(encoded.action_pc, vec![1, 1 + words]);
        let mut sim = crate::plc::Simulator::default();
        sim.memory.set_input(d("X0"), true);
        sim.memory.set_input(d("X20"), true);
        sim.scan(&program, 0);
        assert_eq!(sim.last_scan, crate::plc::ScanStatus::Complete);
        assert!(sim.memory.bit(d("M10")));
        sim.memory.set_input(d("X20"), false);
        sim.scan(&program, 0);
        assert_eq!(sim.memory.bit(d("M10")), pulse);
        sim.memory.set_input(d("X0"), false);
        sim.scan(&program, 0);
        assert_eq!(sim.memory.bit(d("M10")), pulse);
        sim.memory.set_input(d("X0"), true);
        sim.scan(&program, 0);
        assert!(!sim.memory.bit(d("M10")));
        assert_eq!(sim.memory.word(d("D0")), 7);
        assert!(sim.memory.execution_limitations.is_empty());
    }
}

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn action(name: &str, source: &str, dest: &str) -> ParallelRunAction {
    ParallelRunAction::parse(name, &[source.into(), dest.into()]).unwrap()
}
fn seed(m: &mut Memory, area: Area, first: u16, pattern: &str) {
    for (i, v) in pattern.bytes().enumerate() {
        let device = Device {
            area,
            index: first + i as u16,
        };
        if area == Area::X {
            m.set_input(device, v == b'1');
        } else {
            m.set_internal(device, v == b'1');
        }
    }
}
fn bits(m: &Memory, area: Area, first: u16, n: u16) -> String {
    (first..first + n)
        .map(|index| {
            if m.bit(Device { area, index }) {
                '1'
            } else {
                '0'
            }
        })
        .collect()
}

#[test]
fn forms_and_operand_matrix_are_typed() {
    for (name, max, header, words) in [("PRUN", 4, 0xb200, 5), ("DPRUN", 8, 0xb300, 9)] {
        for n in 1..=max {
            for s in ["X20", "M10"] {
                for t in ["Y20", "M20"] {
                    let a = action(name, &format!("K{n}{s}"), &format!("K{n}{t}"));
                    assert_eq!(a.header(), header);
                    assert_eq!(a.words(), words);
                    assert_eq!(a.operands().len(), 2);
                    assert_eq!(a.wide(), name == "DPRUN");
                }
            }
        }
        for (s, t) in [
            ("D0", "K1M0"),
            ("K1Y0", "K1M0"),
            ("K1S0", "K1M0"),
            ("K1M0", "K1X0"),
            ("K1M0", "K1S0"),
        ] {
            assert!(ParallelRunAction::parse(name, &[s.into(), t.into()]).is_err());
        }
        assert!(
            ParallelRunAction::parse(name, &[format!("K{}M0", max + 1), "K1M20".into()]).is_err()
        );
    }
    assert!(ParallelRunAction::parse("PRUNP", &["K1M0".into(), "K1M20".into()]).is_err());
}

#[test]
fn pdf_octal_to_decimal_preserves_gap_bits() {
    let mut m = Memory::default();
    seed(&mut m, Area::X, 16, "1010010111000011");
    m.set_internal(d("M18"), true);
    m.set_internal(d("M19"), true);
    action("PRUN", "K4X20", "K4M10").execute(&mut m, true, 1);
    assert_eq!(bits(&m, Area::M, 10, 18), "101001011111000011");
}

#[test]
fn decimal_source_ignores_holes_and_clips_destination() {
    let mut m = Memory::default();
    seed(&mut m, Area::M, 10, "100110101101011001");
    seed(&mut m, Area::Y, 16, "1111111111111111");
    action("PRUN", "K4M10", "K3Y20").execute(&mut m, true, 0);
    assert_eq!(bits(&m, Area::Y, 16, 16), "1001101001011111");
    let mut observed = BTreeSet::new();
    action("PRUN", "K4M10", "K1Y20").observed_devices(&m, &mut observed);
    assert!(observed.contains(&d("M27"))); // full source is read despite write clipping
}

#[test]
fn native_literal_live_overlap_goldens_narrow_and_wide() {
    // ES2 raw original capture, prun-native-runtime.json, first active visit.
    for (name, before, after) in [
        (
            "PRUN",
            "001001000110010010100100100100100100100100100100",
            "0010010000100100",
        ),
        (
            "DPRUN",
            "001001000110010010100100001001000100100100100100",
            "00100100001001001001001000100010",
        ),
    ] {
        let mut m = Memory::default();
        seed(&mut m, Area::M, 10, before);
        let n = if name == "PRUN" { 4 } else { 8 };
        action(name, &format!("K{n}M10"), &format!("K{n}M20")).execute(&mut m, true, 1);
        assert_eq!(bits(&m, Area::M, 20, (n * 4) as u16), after);
    }
}

#[test]
fn alignment_error_reports_the_actual_operand_pc_and_latches_first_error() {
    for (name, source, dest, offset) in [
        ("PRUN", "K1X21", "K1M10", 1),
        ("PRUN", "K1M10", "K1Y21", 3),
        ("DPRUN", "K1M11", "K1M20", 1),
        ("DPRUN", "K1M10", "K1M21", 5),
    ] {
        let mut m = Memory::default();
        action(name, source, dest).execute(&mut m, true, 40);
        assert_eq!(m.word(d("D1067")), 0xe1a);
        assert_eq!(m.word(d("D1068")), 40 + offset);
        action("PRUN", "K1M1", "K1M20").execute(&mut m, true, 99);
        assert_eq!(m.word(d("D1068")), 40 + offset);
    }
}

#[test]
fn false_power_preserves_flags_destination_and_numeric_scratch() {
    let mut m = Memory::default();
    m.native_numeric_scratch = Some(-123);
    m.set_internal(d("M20"), true);
    m.set_system(1029, true);
    action("PRUN", "K1M1", "K1M21").execute(&mut m, false, 0);
    assert!(m.bit(d("M20")));
    assert!(m.system_bit(1029));
    assert!(!m.system_bit(1067));
    assert_eq!(m.native_numeric_scratch, Some(-123));
}

#[test]
fn expanded_model_boundary_is_atomic_and_short_valid_group_works() {
    let mut m = Memory::default();
    m.relay_limit = Some(27);
    seed(&mut m, Area::X, 0, "111111111111111111111111");
    action("PRUN", "K4X0", "K4M10").execute(&mut m, true, 0);
    assert_eq!(bits(&m, Area::M, 10, 18), "111111110011111111");
    let before = bits(&m, Area::M, 10, 18);
    action("DPRUN", "K6X0", "K6M10").execute(&mut m, true, 0);
    assert_eq!(bits(&m, Area::M, 10, 18), before);
    assert!(!m.execution_limitations.is_empty());
    assert!(!m.system_bit(1067));
}

#[test]
fn unequal_group_lengths_and_all_digit_counts_keep_unwritten_tail() {
    for wide in [false, true] {
        let name = if wide { "DPRUN" } else { "PRUN" };
        let max = if wide { 8 } else { 4 };
        for sn in 1..=max {
            for dn in 1..=max {
                let mut m = Memory::default();
                for i in 0..40 {
                    m.set_input(
                        Device {
                            area: Area::X,
                            index: i,
                        },
                        true,
                    );
                }
                action(name, &format!("K{sn}X0"), &format!("K{dn}M10")).execute(&mut m, true, 0);
                for i in 0..32 {
                    let index = 10 + i + 2 * (i / 8);
                    assert_eq!(
                        m.bit(Device {
                            area: Area::M,
                            index
                        }),
                        i < 4 * sn.min(dn)
                    );
                }
            }
        }
    }
}
