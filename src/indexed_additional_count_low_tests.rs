//! DCMPT valid low / missing high must remain an ordered observation.
use crate::additional_processing::CompareTableProfile as Profile;
use crate::indexed_additional::{AdditionalOutcome as Outcome, IndexedAdditionalAction};
use crate::plc::{Device, Memory};
use std::collections::BTreeSet;
fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn action(raw: &str) -> IndexedAdditionalAction {
    let w = raw
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    IndexedAdditionalAction::parse(&w[0], &w[1..]).unwrap()
}

#[test]
fn double_count_missing_high_observes_low_then_faults_without_out_of_model_capture() {
    for (profile, last) in [(Profile::CurrentEs2, 9999), (Profile::CurrentSs2, 4999)] {
        let mut m = Memory::default();
        m.compare_table_profile = profile;
        m.data_register_limit = Some(last);
        m.set_word(
            Device {
                index: last,
                ..d("D0")
            },
            2,
        );
        m.set_double_word(d("D10"), 5);
        m.set_double_word(d("D20"), 5);
        m.set_double_word(d("D100"), 777);
        let op = action(&format!("DCMPT D10E0 D20F0 D{last}E1 D100F1"));
        let mut fetched = Vec::new();
        let visit = op.qualify_native_with(&m, |_, ds| {
            assert!(
                ds.iter()
                    .all(|d0| d0.area != crate::plc::Area::D || d0.index <= last)
            );
            fetched.extend(ds.iter().copied());
            Ok(())
        });
        assert_eq!(visit.outcome, Outcome::PlcError { offset: 9 });
        let low = Device {
            index: last,
            ..d("D0")
        };
        assert!(visit.observed.contains(&low));
        let low_at = fetched.iter().position(|d0| *d0 == low).unwrap();
        let diagnostic_at = fetched.iter().position(|d0| *d0 == d("D1067")).unwrap();
        assert!(low_at < diagnostic_at);
        let mut seen = BTreeSet::new();
        assert_eq!(
            op.execute(&mut m, true, &mut seen),
            Outcome::PlcError { offset: 9 }
        );
        assert!(seen.contains(&low));
        if profile == Profile::CurrentEs2 {
            assert!(!seen.contains(&d("D10")) && !seen.contains(&d("D20")));
            assert!(!seen.contains(&d("D1183")));
            assert_eq!(m.word(d("D100")), 777);
            assert_eq!(m.native_numeric_scratch, Some(10));
        } else {
            // Existing legacy count-fault contract clears transient10,
            // clamps returned zero to one, and performs that comparison.
            assert!(seen.contains(&d("D10")) && seen.contains(&d("D20")));
            assert_eq!(m.word(d("D100")), 1);
            assert_eq!(m.native_numeric_scratch, Some(12));
        }
    }
}

#[test]
fn double_count_invalid_low_does_not_request_any_count_payload() {
    let mut m = Memory::default();
    m.set_word(d("D1182"), -1);
    let op = action("DCMPT D10E0 D20F0 D0E1 D100F1");
    let visit = op.qualify_native_with(&m, |_, ds| {
        assert!(!ds.contains(&d("D0")) && !ds.contains(&d("D1")));
        Ok(())
    });
    assert_eq!(visit.outcome, Outcome::PlcError { offset: 9 });
    assert!(!visit.observed.contains(&d("D10")));
    assert!(!visit.observed.contains(&d("D20")));
}
