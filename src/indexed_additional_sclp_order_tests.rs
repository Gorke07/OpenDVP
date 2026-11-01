//! Append as a separate test module; frozen original core/tests remain unchanged.
use crate::indexed_additional::{AdditionalOutcome, IndexedAdditionalAction};
use crate::plc::{Device, Memory};
use std::collections::BTreeSet;

fn dev(raw: &str) -> Device {
    raw.parse().unwrap()
}

fn action(raw: &str) -> IndexedAdditionalAction {
    let words = raw
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    IndexedAdditionalAction::parse(&words[0], &words[1..]).unwrap()
}

#[test]
fn sclp_zero_span_skips_result_limits_target_index_and_mode_locally() {
    for name in ["SCLP", "SCLPP"] {
        let mut m = Memory::default();
        m.set_word(dev("D1311"), 50);
        m.set_word(dev("D1312"), 50);
        m.set_word(dev("D1313"), 24);
        m.set_word(dev("D1314"), 4);
        m.set_word(dev("D1183"), -1); // F1: the skipped target would be invalid.
        m.set_word(dev("D20"), 777);
        m.set_system(1020, true);
        m.set_system(1021, false);
        m.set_system(1022, true);
        let mut seen = BTreeSet::new();
        let outcome = action(&format!("{name} K25 D1311E0 D0F1")).execute(&mut m, true, &mut seen);
        assert_eq!(outcome, AdditionalOutcome::PlcError { offset: 3 });
        assert_eq!(m.word(dev("D1067")), 0x0e1a);
        assert_eq!(m.word(dev("D1068")), 3);
        assert_eq!(m.word(dev("D20")), 777);
        assert_eq!(
            [1020, 1021, 1022].map(|i| m.system_bit(i)),
            [true, false, true]
        );
        assert!(seen.contains(&dev("D1311")) && seen.contains(&dev("D1312")));
        for absent in ["D1313", "D1314", "D1183", "M1162"] {
            assert!(
                !seen.contains(&dev(absent)),
                "{name} unexpectedly read {absent}"
            );
        }
        assert_eq!([m.word(dev("D1313")), m.word(dev("D1314"))], [24, 4]);
    }
}

#[test]
fn sclp_zero_span_native_clone_does_not_request_unneeded_rtc_public_cells() {
    let mut local = Memory::default();
    // Local values would permit scaling. The independently captured native
    // min/max produce the error before any D1313..D1319 RTC public input.
    local.set_word(dev("D1311"), 100);
    local.set_word(dev("D1312"), 0);
    local.set_word(dev("D1183"), -1);
    local.set_word(dev("D20"), 777);
    let mut requested = Vec::new();
    let visit = action("SCLP K25 D1311E0 D0F1").qualify_native_with(&local, |m, ds| {
        for d in ds {
            if (1313..=1319).contains(&d.index) && d.area == crate::plc::Area::D {
                return Err(format!("unneeded environmental RTC input {d}"));
            }
            assert_ne!(*d, dev("D1183"));
            requested.push(*d);
            if *d == dev("D1311") || *d == dev("D1312") {
                m.set_word(*d, 50);
            }
        }
        Ok(())
    });
    assert_eq!(visit.outcome, AdditionalOutcome::PlcError { offset: 3 });
    let min = requested.iter().position(|d| *d == dev("D1312")).unwrap();
    let max = requested.iter().position(|d| *d == dev("D1311")).unwrap();
    assert!(min < max);
    assert!(!visit.observed.contains(&dev("D1313")));
    assert!(!visit.observed.contains(&dev("D1314")));
    assert_eq!(
        [local.word(dev("D1311")), local.word(dev("D1312"))],
        [100, 0]
    );
    assert_eq!(local.word(dev("D20")), 777);
    assert_eq!(local.word(dev("D1067")), 0);
}

#[test]
fn sclp_nonzero_span_captures_min_max_high_low_in_that_order() {
    let mut local = Memory::default();
    for (i, value) in [100, 0, 1000, 0].into_iter().enumerate() {
        local.set_word(
            Device {
                index: 20 + i as u16,
                ..dev("D20")
            },
            value,
        );
    }
    let mut table_reads = Vec::new();
    let visit = action("SCLP K25 D20E0 D100F1").qualify_native_with(&local, |_, ds| {
        table_reads.extend(
            ds.iter()
                .copied()
                .filter(|d| d.area == crate::plc::Area::D && (20..=23).contains(&d.index)),
        );
        Ok(())
    });
    assert_eq!(visit.outcome, AdditionalOutcome::Complete);
    assert_eq!(
        table_reads,
        [dev("D21"), dev("D20"), dev("D22"), dev("D23")]
    );
    let mut seen = BTreeSet::new();
    assert_eq!(
        action("SCLP K25 D20E0 D100F1").execute(&mut local, true, &mut seen),
        AdditionalOutcome::Complete
    );
    assert_eq!(local.word(dev("D100")), 250);
}
