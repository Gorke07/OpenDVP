//! Independent raw-tag/profile witnesses; append as a separate test module.
use crate::additional_processing::CompareTableProfile as Profile;
use crate::indexed_additional::{AdditionalOutcome as Outcome, IndexedAdditionalAction};
use crate::plc::{Device, Memory};
use std::collections::BTreeSet;

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn a(raw: &str) -> IndexedAdditionalAction {
    let words = raw
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    IndexedAdditionalAction::parse(&words[0], &words[1..]).unwrap()
}
fn seeded(profile: Profile) -> Memory {
    let mut m = Memory::default();
    m.compare_table_profile = profile;
    m.set_double_word(d("D10"), 5);
    m.set_double_word(d("D20"), 5);
    m.set_double_word(d("D40"), 1);
    m.set_double_word(d("D100"), 777);
    m.set_system(1020, true);
    m.set_system(1021, false);
    m.set_system(1022, true);
    m
}

#[test]
fn all_source_banks_remain_syntax_valid_but_legacy_high_d_tags_hold_result() {
    for profile in [
        Profile::CurrentEs2,
        Profile::CurrentSs2,
        Profile::InstalledLegacy,
    ] {
        for form in ["CMPT", "CMPTP", "DCMPT", "DCMPTP"] {
            let wide = form.starts_with('D');
            for family in ['E', 'F'] {
                for bank in 0..8 {
                    for bad_left in [true, false] {
                        let modifier = format!("{family}{bank}");
                        let body = if bad_left {
                            format!("{form} D10{modifier} D20E0 D40F7 D100E1")
                        } else {
                            format!("{form} D10E0 D20{modifier} D40F7 D100E1")
                        };
                        let op = a(&body);
                        let original = op.native_words().unwrap();
                        let source_word = if bad_left {
                            2
                        } else if wide {
                            6
                        } else {
                            4
                        };
                        assert_eq!(original[source_word] & 0xff, 0x86 + bank / 2);
                        let mut m = seeded(profile);
                        let mut seen = BTreeSet::new();
                        assert_eq!(
                            op.execute(&mut m, true, &mut seen),
                            Outcome::Complete,
                            "{body}"
                        );
                        let suppressed = profile != Profile::CurrentEs2 && bank >= 2;
                        assert_eq!(
                            m.word(d("D100")),
                            if suppressed { 777 } else { 1 },
                            "{profile:?}: {body}"
                        );
                        assert_eq!(m.word(d("D1067")), 0);
                        assert_eq!(
                            [1020, 1021, 1022].map(|i| m.system_bit(i)),
                            [true, false, true]
                        );
                        let advanced = if suppressed && bad_left {
                            0
                        } else if wide {
                            2
                        } else {
                            1
                        };
                        assert_eq!(m.native_numeric_scratch, Some(10 + advanced));
                        assert!(
                            seen.contains(&d("D40")),
                            "count always precedes category dispatch"
                        );
                        if suppressed {
                            assert!(!seen.contains(&d("D20")));
                            assert_eq!(seen.contains(&d("D10")), !bad_left);
                            if wide {
                                assert_eq!(seen.contains(&d("D11")), !bad_left);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn legacy_high_banks_still_work_for_count_result_and_timer_counter_sources() {
    for profile in [Profile::CurrentSs2, Profile::InstalledLegacy] {
        for form in ["CMPT", "CMPTP", "DCMPT", "DCMPTP"] {
            for family in ['E', 'F'] {
                for bank in 0..8 {
                    let modifier = format!("{family}{bank}");
                    let mut m = seeded(profile);
                    m.set_double_word(d("T10"), 5);
                    m.set_double_word(d("C20"), 5);
                    let op = a(&format!(
                        "{form} T10{modifier} C20{modifier} D40{modifier} D100{modifier}"
                    ));
                    let mut seen = BTreeSet::new();
                    assert_eq!(op.execute(&mut m, true, &mut seen), Outcome::Complete);
                    assert_eq!(m.word(d("D100")), 1);
                    assert_eq!(m.word(d("D1067")), 0);
                }
            }
        }
    }
}

#[test]
fn legacy_right_category_rejection_is_after_left_payload_and_before_target_modifier() {
    for profile in [Profile::CurrentSs2, Profile::InstalledLegacy] {
        for form in ["CMPT", "DCMPT"] {
            let mut m = seeded(profile);
            m.set_word(d("D1183"), -1); // skipped F1 result index
            let mut fetched = Vec::new();
            let op = a(&format!("{form} D10E0 D20F7 D40E1 D0F1"));
            let visit = op.qualify_native_with(&m, |_, ds| {
                assert!(!ds.contains(&d("D20")) && !ds.contains(&d("D21")));
                assert!(!ds.contains(&d("D1183")));
                fetched.extend(ds.iter().copied());
                Ok(())
            });
            assert_eq!(visit.outcome, Outcome::Complete);
            assert!(fetched.contains(&d("D10")));
            assert_eq!(fetched.contains(&d("D11")), form == "DCMPT");
            assert!(
                fetched.iter().position(|d0| *d0 == d("D40")).unwrap()
                    < fetched.iter().position(|d0| *d0 == d("D10")).unwrap()
            );
            assert_eq!(m.word(d("D100")), 777);
            assert_eq!(m.native_numeric_scratch, None);
            assert_eq!(m.word(d("D1067")), 0);
        }
    }
}
