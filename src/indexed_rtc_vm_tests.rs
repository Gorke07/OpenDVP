//! Fixed-input RTC handler contracts. No host/native clock is sampled here.
//! Native environmental qualification and background service are parent-owned.
use crate::indexed_address::IndexWord;
use crate::indexed_rtc::{IndexedRtcAction, RtcOutcome as Outcome};
use crate::plc::{Area, Device, Memory, ScanStatus, Simulator, compile_il};
use crate::rtc_clock::{RtcAction, RtcProfile};
use std::collections::BTreeSet;

const SEED: [u8; 7] = [24, 4, 2, 29, 12, 30, 7];
fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn at(base: Device, n: u16) -> Device {
    Device {
        index: base.index + n,
        ..base
    }
}
fn args(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}
fn a(s: &str) -> IndexedRtcAction {
    let v = args(s);
    IndexedRtcAction::parse(&v[0], &v[1..]).unwrap()
}
fn reject(s: &str) -> bool {
    let v = args(s);
    IndexedRtcAction::parse(&v[0], &v[1..]).is_err()
}
fn seeded() -> Memory {
    let mut m = Memory::default();
    m.rtc_clock.configure_frozen(SEED);
    m
}
fn put(m: &mut Memory, base: Device, fields: [i16; 7]) {
    for (n, value) in fields.into_iter().enumerate() {
        m.set_word(at(base, n as u16), value);
    }
}
fn get(m: &Memory, base: Device) -> [i16; 7] {
    std::array::from_fn(|n| m.word(at(base, n as u16)))
}
fn public(m: &Memory) -> [i16; 7] {
    std::array::from_fn(|n| m.word(d(&format!("D{}", 1319 - n))))
}
fn visit(s: &str, m: &mut Memory) -> (Outcome, BTreeSet<Device>) {
    let mut seen = BTreeSet::new();
    let out = a(s).execute(m, true, &mut seen);
    (out, seen)
}
fn flags(m: &Memory) -> [bool; 3] {
    [1020, 1021, 1022].map(|i| m.system_bit(i))
}
fn seed_flags(m: &mut Memory) {
    for i in 1020..=1022 {
        m.set_system(i, true);
    }
}
fn hydrate(dst: &mut Memory, src: &Memory, ds: &BTreeSet<Device>) {
    for &dev in ds {
        match dev.area {
            Area::X | Area::Y | Area::M | Area::S => dst.set_bit(dev, src.bit(dev)),
            Area::C if dev.index >= 200 => dst.set_double_word_raw(dev, src.double_word(dev)),
            _ => dst.set_word(dev, src.word(dev)),
        }
    }
}
fn complete(sim: &mut Simulator, p: &crate::plc::Program) {
    sim.scan(p, 0);
    assert_eq!(sim.last_scan, ScanStatus::Complete);
    assert!(
        sim.memory.execution_limitations.is_empty(),
        "{:?}",
        sim.memory.execution_limitations
    );
}

#[test]
fn four_forms_keep_exact_three_words_and_original_descriptor_bytes() {
    for (source, expected) in [
        ("TRD D5E1", [0x6a01, 0x0ae6, 0x8086]),
        ("TRD T10F7", [0x6a01, 0x14f6, 0xe082]),
        ("TWR C193E1", [0x6c01, 0x82e6, 0x2184]),
        ("TRDP C199F7", [0x6a11, 0x8ef6, 0xe184]),
        ("TWRP D5E1", [0x6c11, 0x0ae6, 0x8086]),
    ] {
        let action = a(source);
        assert_eq!(action.native_words().unwrap(), expected);
        assert_eq!(action.nominal_words(), 3);
        assert_eq!(action.label(), source);
        assert_eq!(action.is_read(), source.starts_with("TRD"));
        assert_eq!(action.is_write(), source.starts_with("TWR"));
    }
    for name in ["DTRD", "DTWR", "TRDPP", "TWRPP", "HOUR"] {
        assert!(!IndexedRtcAction::supports(name));
    }
}

#[test]
fn only_t_c_d_word_bases_accept_modifiers_without_premature_seven_word_rejection() {
    for source in [
        "TRD T255",
        "TWR C199",
        "TRDP D9999",
        "TWRP T255F7",
        "TRD C199E1",
        "TWR D9999E0",
    ] {
        assert!(!reject(source), "{source}");
    }
    for source in [
        "TRD E0",
        "TWR F7",
        "TRD K0",
        "TWR H0",
        "TRD K4M0",
        "TWR M0E0",
        "TRD Y0",
        "TWR X0F0",
        "TRD C200E0",
        "TWR C200F7",
        "TRD D0 D1",
        "TWR",
    ] {
        assert!(reject(source), "{source}");
    }
}

#[test]
fn all_sixteen_signed_word_banks_address_all_three_areas_in_both_roles() {
    for area in [Area::T, Area::C, Area::D] {
        for bank in 0..8 {
            for prefix in ['E', 'F'] {
                let suffix = format!("{prefix}{bank}");
                let alias = IndexWord::parse(&suffix).unwrap().device();
                let base = Device { area, index: 40 };
                let lexical = Device { area, index: 41 };
                let mut m = seeded();
                m.set_word(alias, -1);
                seed_flags(&mut m);
                assert_eq!(
                    visit(&format!("TRD {lexical}{suffix}"), &mut m).0,
                    Outcome::Complete
                );
                assert_eq!(get(&m, base), SEED.map(i16::from));
                assert_eq!(flags(&m), [true; 3]);
                put(&mut m, base, [4, 2, 12, 15, 15, 27, 30]);
                assert_eq!(
                    visit(&format!("TWR {lexical}{suffix}"), &mut m).0,
                    Outcome::Complete
                );
                assert_eq!(m.rtc_clock.shadow(), Some([4, 2, 12, 15, 15, 27, 30]));
                assert_eq!(m.word(alias), -1);
                assert_eq!(flags(&m), [true; 3]);
            }
        }
    }
}

#[test]
fn explicit_manual_seed_round_trip_is_independent_of_host_time() {
    let mut m = Memory::default();
    put(&mut m, d("D10"), [4, 2, 12, 15, 15, 27, 30]);
    assert_eq!(visit("TWR D10E0", &mut m).0, Outcome::Complete);
    assert_eq!(m.rtc_clock.profile(), Some(RtcProfile::FrozenSeed));
    assert_eq!(m.rtc_clock.shadow(), Some([4, 2, 12, 15, 15, 27, 30]));
    assert!(m.system_bit(1082));
    assert_eq!(visit("TRD D100F0", &mut m).0, Outcome::Complete);
    assert_eq!(get(&m, d("D100")), [4, 2, 12, 15, 15, 27, 30]);
    assert!(!m.system_bit(1067));
}

#[test]
fn trd_refreshes_before_destination_resolution_and_keeps_private_state() {
    let mut m = seeded();
    put(&mut m, d("D1313"), [777; 7]);
    m.set_system(1016, true);
    m.set_word(d("D1182"), -1);
    let (out, seen) = visit("TRD D0E1", &mut m);
    assert_eq!(out, Outcome::PlcError { offset: 1 });
    assert_eq!(public(&m), [2024, 4, 2, 29, 12, 30, 7]);
    assert_eq!(m.rtc_clock.shadow(), Some(SEED));
    assert!(!m.system_bit(1082));
    assert_eq!(m.word(d("D1068")), 1);
    assert!(seen.contains(&d("M1016")));
    assert!(seen.contains(&d("D1182")));
    assert!(!seen.contains(&d("D0")));
}

#[test]
fn unseeded_trd_does_not_read_modifier_or_manufacture_public_clock_values() {
    let mut m = Memory::default();
    put(&mut m, d("D1313"), [777; 7]);
    m.set_word(d("D1182"), -1);
    let (out, seen) = visit("TRD D0E1", &mut m);
    assert!(matches!(out, Outcome::Limitation(_)));
    assert!(seen.is_empty());
    assert_eq!(public(&m), [777; 7]);
    assert!(!m.system_bit(1067));
    assert!(!m.system_bit(1082));
    assert_eq!(m.rtc_clock.shadow(), None);
}

#[test]
fn twr_invalid_resolver_is_offset_one_before_public_payload_or_controls() {
    for (source, offset) in [("D0E1", -1), ("T255E1", 1), ("C199E1", 57), ("D4999E1", 1)] {
        let mut m = seeded();
        m.data_register_limit = Some(4999);
        put(&mut m, d("D1313"), [777; 7]);
        m.set_word(d("D1182"), offset);
        let (out, seen) = visit(&format!("TWR {source}"), &mut m);
        assert_eq!(out, Outcome::PlcError { offset: 1 });
        assert_eq!(public(&m), [777; 7]);
        assert_eq!(m.rtc_clock.shadow(), Some(SEED));
        assert!(!seen.contains(&d("M1016")));
        assert!(!seen.contains(&d("M1082")));
        assert!(!m.system_bit(1082));
        assert_eq!(m.word(d("D1068")), 1);
    }
}

#[test]
fn effective_c200_and_c255_are_accepted_resolver_bases_with_later_handler_faults() {
    for offset in [1, 56] {
        for write in [false, true] {
            let mut m = seeded();
            m.set_word(d("D1028"), offset);
            m.set_double_word_raw(d("C200"), 0x12345678);
            let (out, seen) = visit(if write { "TWR C199E0" } else { "TRD C199E0" }, &mut m);
            assert_eq!(
                out,
                if write {
                    Outcome::PlcError { offset: 3 }
                } else {
                    Outcome::PartialRead { written: 0 }
                }
            );
            assert_eq!(m.double_word(d("C200")), 0x12345678);
            assert!(!seen.contains(&d("C200")));
            assert!(!seen.contains(&d("C255")));
            assert!(!m.system_bit(1082));
            assert_eq!(m.system_bit(1067), write);
            if !write {
                assert_eq!(public(&m), SEED.map(i16::from));
            }
        }
    }
}

#[test]
fn trd_model_endpoints_copy_only_existing_words_without_plc_error() {
    for (base, last) in [
        ("T255", 9999),
        ("C199", 9999),
        ("D4999", 4999),
        ("D9999", 9999),
    ] {
        let mut m = seeded();
        m.data_register_limit = Some(last);
        seed_flags(&mut m);
        let (out, seen) = visit(&format!("TRD {base}E0"), &mut m);
        assert_eq!(out, Outcome::PartialRead { written: 1 });
        assert_eq!(m.word(d(base)), 24);
        assert!(!m.system_bit(1067));
        assert!(!m.system_bit(1082));
        assert_eq!(flags(&m), [true; 3]);
        assert!(seen.contains(&d(base)));
        assert!(!seen.contains(&at(d(base), 1)));
    }
}

#[test]
fn trd_clipped_two_word_copy_does_not_touch_counter_natural_storage() {
    let mut m = seeded();
    m.set_double_word_raw(d("C200"), 0x12345678);
    let (out, seen) = visit("TRD C198F0", &mut m);
    assert_eq!(out, Outcome::PartialRead { written: 2 });
    assert_eq!(m.word(d("C198")), 24);
    assert_eq!(m.word(d("C199")), 4);
    assert_eq!(m.double_word(d("C200")), 0x12345678);
    assert!(!seen.contains(&d("C200")));
}

#[test]
fn twr_complete_extent_failure_precedes_payload_and_preserves_m1082_both_levels() {
    for (base, last) in [
        ("T250", 9999),
        ("C194", 9999),
        ("D4994", 4999),
        ("D9994", 9999),
    ] {
        for old_ack in [false, true] {
            let mut m = seeded();
            m.data_register_limit = Some(last);
            m.set_system(1082, old_ack);
            put(&mut m, d("D1313"), [777; 7]);
            let (out, seen) = visit(&format!("TWR {base}E1"), &mut m);
            assert_eq!(out, Outcome::PlcError { offset: 3 });
            assert_eq!(m.system_bit(1082), old_ack);
            assert_eq!(public(&m), [777; 7]);
            assert_eq!(m.rtc_clock.shadow(), Some(SEED));
            assert!(!seen.contains(&d(base)));
            assert!(!seen.contains(&d("M1016")));
            assert!(!seen.contains(&d("M1082")));
            assert_eq!(m.word(d("D1068")), 3);
        }
    }
}

#[test]
fn last_complete_blocks_and_negative_lexical_endpoint_rescue_are_legal() {
    for (lexical, effective, last) in [
        ("T255", "T249", 9999),
        ("C199", "C193", 9999),
        ("D4999", "D4993", 4999),
        ("D9999", "D9993", 9999),
    ] {
        let mut m = seeded();
        m.data_register_limit = Some(last);
        m.set_word(d("D1028"), -6);
        assert_eq!(
            visit(&format!("TRD {lexical}E0"), &mut m).0,
            Outcome::Complete
        );
        assert_eq!(get(&m, d(effective)), SEED.map(i16::from));
        put(&mut m, d(effective), [4, 2, 12, 15, 15, 27, 30]);
        assert_eq!(
            visit(&format!("TWR {lexical}E0"), &mut m).0,
            Outcome::Complete
        );
        assert_eq!(m.rtc_clock.shadow(), Some([4, 2, 12, 15, 15, 27, 30]));
    }
}

#[test]
fn trd_public_overlap_uses_live_reverse_sources_not_a_seven_value_snapshot() {
    let mut m = seeded();
    m.set_word(d("D1312"), 777);
    m.set_word(d("D1320"), 888);
    assert_eq!(visit("TRD D1313E0", &mut m).0, Outcome::Complete);
    assert_eq!(get(&m, d("D1313")), [24, 4, 2, 29, 2, 4, 24]);
    assert_eq!(m.rtc_clock.shadow(), Some(SEED));
    assert_eq!(m.word(d("D1312")), 777);
    assert_eq!(m.word(d("D1320")), 888);
}

#[test]
fn twr_public_overlap_uses_live_forward_sources_and_reverse_stores() {
    let mut m = seeded();
    put(&mut m, d("D1313"), [24, 4, 2, 29, 12, 30, 0]);
    assert_eq!(visit("TWR D1313F0", &mut m).0, Outcome::Complete);
    assert_eq!(m.rtc_clock.shadow(), Some([24, 4, 2, 29, 2, 4, 24]));
    assert_eq!(public(&m), [24, 4, 2, 29, 2, 4, 24]);
}

#[test]
fn trd_cached_target_survives_year_store_overwriting_its_own_index() {
    let mut m = seeded();
    m.set_word(d("D1182"), 0);
    m.set_word(d("D1181"), 777);
    m.set_word(d("D1189"), 888);
    m.set_word(d("D1207"), 999);
    assert_eq!(visit("TRD D1182E1", &mut m).0, Outcome::Complete);
    assert_eq!(get(&m, d("D1182")), SEED.map(i16::from));
    assert_eq!(m.word(d("D1181")), 777);
    assert_eq!(m.word(d("D1189")), 888);
    assert_eq!(m.word(d("D1207")), 999);
}

#[test]
fn twr_source_can_alias_its_signed_word_modifier_without_pair_index_poison() {
    let mut m = seeded();
    put(&mut m, d("D1182"), [0, 7, 12, 31, 23, 59, 58]);
    assert_eq!(visit("TWR D1182E1", &mut m).0, Outcome::Complete);
    assert_eq!(m.rtc_clock.shadow(), Some([0, 7, 12, 31, 23, 59, 58]));
    assert_eq!(m.word(d("D1183")), 7);
}

#[test]
fn twr_corrects_all_bad_fields_and_keeps_private_seed_on_error() {
    let mut m = seeded();
    put(&mut m, d("D10"), [100, 0, 13, 0, 24, -1, 60]);
    seed_flags(&mut m);
    let (out, seen) = visit("TWR D10E0", &mut m);
    assert_eq!(out, Outcome::PlcError { offset: 3 });
    assert_eq!(public(&m), [0, 1, 1, 1, 0, 0, 0]);
    assert_eq!(m.rtc_clock.shadow(), Some(SEED));
    assert!(m.system_bit(1082));
    assert_eq!(flags(&m), [true; 3]);
    assert!(!seen.contains(&d("M1016")));
    for index in 10..=16 {
        assert!(seen.contains(&d(&format!("D{index}"))));
    }
    assert_eq!(m.word(d("D1067")), 0xe1a);
    assert_eq!(m.word(d("D1068")), 3);
}

#[test]
fn bad_day_keeps_normalized_year_then_valid_recovery_restores_four_digit_presentation() {
    let mut m = seeded();
    m.set_system(1016, true);
    put(&mut m, d("D10"), [2004, 2, 12, 0, 15, 27, 30]);
    assert_eq!(
        visit("TWR D10E0", &mut m).0,
        Outcome::PlcError { offset: 3 }
    );
    assert_eq!(public(&m), [4, 2, 12, 1, 15, 27, 30]);
    assert_eq!(m.rtc_clock.shadow(), Some(SEED));
    let error = (m.word(d("D1067")), m.word(d("D1068")));
    m.set_word(d("D13"), 15);
    assert_eq!(visit("TWR D10E0", &mut m).0, Outcome::Complete);
    assert_eq!(public(&m), [2004, 2, 12, 15, 15, 27, 30]);
    assert_eq!(m.rtc_clock.shadow(), Some([4, 2, 12, 15, 15, 27, 30]));
    assert_eq!((m.word(d("D1067")), m.word(d("D1068"))), error);
}

#[test]
fn range_only_fields_accept_february31_and_independent_weekday() {
    let mut m = seeded();
    put(&mut m, d("D10"), [24, 7, 2, 31, 23, 59, 59]);
    assert_eq!(visit("TWR D10F0", &mut m).0, Outcome::Complete);
    assert_eq!(m.rtc_clock.shadow(), Some([24, 7, 2, 31, 23, 59, 59]));
    assert!(!m.system_bit(1067));
}

#[test]
fn old_sticky_error_does_not_change_this_visit_outcome_or_get_overwritten() {
    let mut m = seeded();
    m.operation_error(0xe19, 17);
    put(&mut m, d("D10"), [100, 0, 13, 0, 24, -1, 60]);
    assert_eq!(
        visit("TWR D10E0", &mut m).0,
        Outcome::PlcError { offset: 3 }
    );
    assert_eq!((m.word(d("D1067")), m.word(d("D1068"))), (0xe19, 17));
    assert!(m.system_bit(1082));
    put(&mut m, d("D10"), [4, 2, 12, 15, 15, 27, 30]);
    assert_eq!(visit("TWR D10E0", &mut m).0, Outcome::Complete);
    assert_eq!(visit("TRD D100F0", &mut m).0, Outcome::Complete);
    assert_eq!((m.word(d("D1067")), m.word(d("D1068"))), (0xe19, 17));
    assert_eq!(get(&m, d("D100")), [4, 2, 12, 15, 15, 27, 30]);
}

#[test]
fn twr_preserves_backend_identity_and_installed_phase_but_resets_virtual_fraction() {
    for profile in [
        RtcProfile::FrozenSeed,
        RtcProfile::VirtualGregorian,
        RtcProfile::InstalledShadow,
    ] {
        let mut m = Memory::default();
        match profile {
            RtcProfile::FrozenSeed => m.rtc_clock.configure_frozen(SEED),
            RtcProfile::VirtualGregorian => {
                m.rtc_clock
                    .configure_virtual([2024, 4, 2, 29, 12, 30, 7])
                    .unwrap();
                m.rtc_clock.advance_monotonic_us(500_000).unwrap();
            }
            RtcProfile::InstalledShadow => {
                m.rtc_clock.configure_installed_shadow(SEED, 13).unwrap()
            }
        }
        put(&mut m, d("D10"), [4, 2, 12, 15, 15, 27, 30]);
        assert_eq!(visit("TWR D10E0", &mut m).0, Outcome::Complete);
        assert_eq!(m.rtc_clock.profile(), Some(profile));
        assert_eq!(m.rtc_clock.shadow(), Some([4, 2, 12, 15, 15, 27, 30]));
        if profile == RtcProfile::VirtualGregorian {
            assert_eq!(m.rtc_clock.fraction_us(), 0);
        }
        if profile == RtcProfile::InstalledShadow {
            assert_eq!(m.rtc_clock.previous_native_second(), Some(13));
        }
    }
}

#[test]
fn handler_does_not_sample_or_advance_installed_clock_or_apply_m1017_service() {
    let mut m = Memory::default();
    m.rtc_clock.configure_installed_shadow(SEED, 13).unwrap();
    m.set_system(1017, true);
    m.set_system(1039, true);
    assert_eq!(visit("TRD D100E0", &mut m).0, Outcome::Complete);
    assert_eq!(get(&m, d("D100")), SEED.map(i16::from));
    assert_eq!(m.rtc_clock.shadow(), Some(SEED));
    assert_eq!(m.rtc_clock.previous_native_second(), Some(13));
    assert!(!m.system_bit(1082));
    // This is only the instruction body. It does not claim either relay freezes
    // an actual simulator's independently running background engine.
}

#[test]
fn all_four_false_visits_skip_seed_controls_indices_and_every_store() {
    for name in ["TRD", "TRDP", "TWR", "TWRP"] {
        let mut m = Memory::default();
        put(&mut m, d("D1313"), [777; 7]);
        m.set_word(d("D1182"), -1);
        seed_flags(&mut m);
        let mut seen = BTreeSet::new();
        assert_eq!(
            a(&format!("{name} D0E1")).execute(&mut m, false, &mut seen),
            Outcome::Nominal
        );
        assert!(seen.is_empty());
        assert!(m.execution_limitations.is_empty());
        assert!(!m.system_bit(1067));
        assert!(!m.system_bit(1082));
        assert_eq!(public(&m), [777; 7]);
        assert_eq!(flags(&m), [true; 3]);
    }
}

#[test]
fn native_twr_clone_reads_actual_payload_in_order_without_committing_private_results() {
    let mut local = seeded();
    put(&mut local, d("D10"), [4, 2, 12, 15, 15, 27, 30]);
    let mut actual = local.clone();
    put(&mut actual, d("D10"), [100, 0, 13, 0, 24, -1, 60]);
    let q = a("TWR D10E0").qualify_native_with(&local, |clone, ds| {
        hydrate(clone, &actual, ds);
        Ok(())
    });
    assert_eq!(q.outcome, Outcome::PlcError { offset: 3 });
    assert_eq!(local.rtc_clock.shadow(), Some(SEED));
    assert!(!local.system_bit(1082));
    assert!(!local.system_bit(1067));
    assert_eq!(get(&local, d("D10")), [4, 2, 12, 15, 15, 27, 30]);
    assert_eq!(public(&local), [0; 7]);
}

#[test]
fn native_twr_overlap_replay_never_recaptures_public_rows_already_written() {
    let mut actual = seeded();
    put(&mut actual, d("D1313"), [24, 4, 2, 29, 12, 30, 0]);
    let local = seeded();
    let mut fetched = BTreeSet::<Device>::new();
    let q = a("TWR D1313E0").qualify_native_with(&local, |clone, ds| {
        fetched.extend(ds);
        hydrate(clone, &actual, ds);
        Ok(())
    });
    assert_eq!(q.outcome, Outcome::Complete);
    for yes in ["D1313", "D1314", "D1315", "D1316"] {
        assert!(fetched.contains(&d(yes)));
    }
    for no in ["D1317", "D1318", "D1319"] {
        assert!(!fetched.contains(&d(no)));
        assert!(q.observed.contains(&d(no)));
    }
    assert_eq!(local.rtc_clock.shadow(), Some(SEED));
    assert_eq!(public(&local), [0; 7]);
}

#[test]
fn native_trd_refresh_owns_all_public_rows_before_overlap_or_index_capture() {
    let local = seeded();
    let mut actual = seeded();
    put(&mut actual, d("D1313"), [777; 7]);
    let mut fetched = BTreeSet::<Device>::new();
    let mut cloned_public_at_index = None;
    let q = a("TRD D1313E1").qualify_native_with(&local, |clone, ds| {
        if ds.contains(&d("D1182")) {
            cloned_public_at_index = Some(public(clone));
        }
        fetched.extend(ds);
        hydrate(clone, &actual, ds);
        Ok(())
    });
    assert_eq!(q.outcome, Outcome::Complete);
    assert_eq!(cloned_public_at_index, Some(SEED.map(i16::from)));
    for index in 1313..=1319 {
        assert!(!fetched.contains(&d(&format!("D{index}"))));
    }
    assert_eq!(public(&local), [0; 7]);
    assert_eq!(local.rtc_clock.shadow(), Some(SEED));
}

#[test]
fn native_same_value_source_alias_write_still_blocks_recapture() {
    let mut actual = seeded();
    put(&mut actual, d("D1313"), [24, 4, 2, 29, 2, 4, 24]);
    let local = actual.clone();
    let mut fetched = BTreeSet::<Device>::new();
    let q = a("TWR D1313F0").qualify_native_with(&local, |clone, ds| {
        fetched.extend(ds);
        hydrate(clone, &actual, ds);
        Ok(())
    });
    assert_eq!(q.outcome, Outcome::Complete);
    // First store writes year24 into an already24 D1319. The last source read
    // remains owned by that store, despite equal bytes.
    assert!(!fetched.contains(&d("D1319")));
    assert_eq!(local.rtc_clock.shadow(), Some(SEED));
}

#[test]
fn native_capture_failure_never_publishes_partial_clock_to_local_state() {
    let mut local = seeded();
    put(&mut local, d("D10"), [4, 2, 12, 15, 15, 27, 30]);
    let q = a("TWR D10E0").qualify_native_with(&local, |_, ds| {
        if ds.contains(&d("D13")) {
            Err("capture unavailable".into())
        } else {
            Ok(())
        }
    });
    assert_eq!(q.outcome, Outcome::Limitation("capture unavailable".into()));
    assert!(q.observed.contains(&d("D1317")));
    assert!(!q.observed.contains(&d("D1316")));
    assert_eq!(public(&local), [0; 7]);
    assert_eq!(local.rtc_clock.shadow(), Some(SEED));
    assert!(!local.system_bit(1082));
}

#[test]
fn native_main_qualification_distinguishes_known_extent_error_from_partial_and_bad_resolver() {
    let mut m = seeded();
    m.data_register_limit = Some(4999);
    let extent = a("TWR D4999E0").qualify_native(&m);
    assert_eq!(extent.outcome, Outcome::PlcError { offset: 3 });
    assert!(!extent.observed.contains(&d("D4999")));
    assert!(matches!(
        a("TRD D4999E0").qualify_native(&m).outcome,
        Outcome::Limitation(_)
    ));
    m.set_word(d("D1028"), 1);
    assert!(matches!(
        a("TWR D4999E0").qualify_native(&m).outcome,
        Outcome::Limitation(_)
    ));
    assert!(matches!(
        a("TRD D4999E0").qualify_native(&m).outcome,
        Outcome::Limitation(_)
    ));
    assert_eq!(m.rtc_clock.shadow(), Some(SEED));
    assert!(!m.system_bit(1067));
}

#[test]
fn static_and_monitor_inventory_preserve_defined_model_endpoint_error_paths() {
    let mut ds = BTreeSet::new();
    a("TWR D4999").devices(&mut ds);
    assert!(ds.contains(&d("D4999")));
    assert!(!ds.contains(&d("D5000")));
    let mut m = seeded();
    m.data_register_limit = Some(4999);
    let write = a("TWR D4999E0");
    let read = a("TRD D4999E0");
    assert_eq!(
        write.monitor_input_devices(&m).unwrap(),
        BTreeSet::from([d("D1028")])
    );
    assert_eq!(
        write.capture_devices(&m).unwrap(),
        BTreeSet::from([d("D1028")])
    );
    assert_eq!(
        read.capture_devices(&m).unwrap(),
        BTreeSet::from([d("D1028"), d("D4999")])
    );
    assert_eq!(read.control_devices(), [d("M1016")]);
    assert_eq!(read.display_value(&m), "0 / — / — / — / — / — / —");
}

#[test]
fn typed_legacy_adapter_preserves_same_handler_results_without_source_reparse() {
    let read = IndexedRtcAction::from_legacy(&RtcAction::Read { dest: d("D100") });
    let write = IndexedRtcAction::from_legacy(&RtcAction::Write { source: d("D10") });
    assert_eq!(read.label(), "TRD D100");
    assert_eq!(write.label(), "TWR D10");
    let mut m = seeded();
    put(&mut m, d("D10"), [4, 2, 12, 15, 15, 27, 30]);
    assert_eq!(
        write.execute(&mut m, true, &mut BTreeSet::new()),
        Outcome::Complete
    );
    assert_eq!(
        read.execute(&mut m, true, &mut BTreeSet::new()),
        Outcome::Complete
    );
    assert_eq!(get(&m, d("D100")), [4, 2, 12, 15, 15, 27, 30]);
}

#[test]
fn successful_and_error_rtc_visits_hold_alu_flags_and_numeric_scratch() {
    for body in ["TRD D100E0", "TWR D10E0", "TWR T255E0"] {
        let mut m = seeded();
        seed_flags(&mut m);
        m.native_numeric_scratch = Some(-1234);
        put(&mut m, d("D10"), [4, 2, 12, 15, 15, 27, 30]);
        visit(body, &mut m);
        assert_eq!(flags(&m), [true; 3]);
        assert_eq!(m.native_numeric_scratch, Some(-1234));
    }
}

#[test]
fn public_trdp_uses_header_pulse_history_and_rearms_with_fixed_clock_seed() {
    let p = compile_il("LD X0\nTRDP D100E0\nLDI X7\nMOV K1234 D60\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.rtc_clock.configure_frozen(SEED);
    complete(&mut sim, &p);
    assert_eq!(get(&sim.memory, d("D100")), [0; 7]);
    sim.memory.set_input(d("X0"), true);
    complete(&mut sim, &p);
    assert_eq!(get(&sim.memory, d("D100")), SEED.map(i16::from));
    sim.memory
        .rtc_clock
        .configure_frozen([4, 2, 12, 15, 15, 27, 30]);
    complete(&mut sim, &p);
    assert_eq!(get(&sim.memory, d("D100")), SEED.map(i16::from));
    sim.memory.set_input(d("X0"), false);
    complete(&mut sim, &p);
    sim.memory.set_input(d("X0"), true);
    complete(&mut sim, &p);
    assert_eq!(get(&sim.memory, d("D100")), [4, 2, 12, 15, 15, 27, 30]);
    assert_eq!(sim.memory.word(d("D60")), 1234);
}

#[test]
fn public_twrp_keeps_committed_shadow_while_held_then_rearms() {
    let p = compile_il("LD X0\nTWRP D10F0\nLDI X7\nMOV K1234 D60\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.rtc_clock.configure_frozen(SEED);
    put(&mut sim.memory, d("D10"), [4, 2, 12, 15, 15, 27, 30]);
    complete(&mut sim, &p);
    assert_eq!(sim.memory.rtc_clock.shadow(), Some(SEED));
    sim.memory.set_input(d("X0"), true);
    complete(&mut sim, &p);
    assert_eq!(
        sim.memory.rtc_clock.shadow(),
        Some([4, 2, 12, 15, 15, 27, 30])
    );
    put(&mut sim.memory, d("D10"), [5, 3, 1, 1, 1, 2, 3]);
    complete(&mut sim, &p);
    assert_eq!(
        sim.memory.rtc_clock.shadow(),
        Some([4, 2, 12, 15, 15, 27, 30])
    );
    sim.memory.set_input(d("X0"), false);
    complete(&mut sim, &p);
    sim.memory.set_input(d("X0"), true);
    complete(&mut sim, &p);
    assert_eq!(sim.memory.rtc_clock.shadow(), Some([5, 3, 1, 1, 1, 2, 3]));
    assert_eq!(sim.memory.word(d("D60")), 1234);
}

#[test]
fn public_cj_and_master_false_skip_rtc_access_with_unseeded_invalid_indices() {
    let p=compile_il("LD X1\nCJ P0\nLD X0\nTRD D0E1\nP0\nLD X7\nMC N0\nLDI X7\nTWR D0F1\nMCR N0\nLDI X7\nMOV K1234 D60\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X1"), true);
    sim.memory.set_input(d("X0"), true);
    sim.memory.set_word(d("D1182"), -1);
    sim.memory.set_word(d("D1183"), -1);
    complete(&mut sim, &p);
    assert_eq!(sim.memory.word(d("D60")), 1234);
    assert_eq!(sim.memory.rtc_clock.shadow(), None);
    assert!(!sim.memory.system_bit(1067));
    assert!(!sim.memory.system_bit(1082));
}

#[test]
fn rtc_decoder_diagnostic_replay_preserves_normal_domain_guards_and_local_state() {
    for (source, modifier, value, expected) in [
        (
            "TRD D9999E1",
            "D1182",
            0,
            Outcome::PartialRead { written: 1 },
        ),
        (
            "TRD C199F7",
            "D1195",
            1,
            Outcome::PartialRead { written: 0 },
        ),
        ("TRDP T0E1", "D1182", -1, Outcome::PlcError { offset: 1 }),
        ("TWRP C199F7", "D1195", 57, Outcome::PlcError { offset: 1 }),
    ] {
        let mut m = seeded();
        m.set_word(d(modifier), value);
        let action = a(source);
        assert!(matches!(
            action.qualify_native(&m).outcome,
            Outcome::Limitation(_)
        ));
        let diag = action.qualify_native_decoder_with(&m, |_, _| Ok(()));
        assert_eq!(diag.outcome, expected, "{source}");
        assert_eq!(m.rtc_clock.shadow(), Some(SEED));
        assert!(!m.system_bit(1067));
        assert!(!m.system_bit(1068));
        assert!(!m.system_bit(1082));
        assert_eq!(public(&m), [0; 7]);
    }
}

#[test]
fn rtc_decoder_diagnostic_capture_failure_never_manufactures_a_plc_error() {
    let m = seeded();
    let q = a("TWR D0E1")
        .qualify_native_decoder_with(&m, |_, _| Err("diagnostic capture failed".into()));
    assert_eq!(
        q.outcome,
        Outcome::Limitation("diagnostic capture failed".into())
    );
    assert!(!m.system_bit(1067));
    assert_eq!(m.rtc_clock.shadow(), Some(SEED));
}
