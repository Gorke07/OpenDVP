use super::*;
fn action(name: &str, args: &[&str]) -> ModuleIoAction {
    ModuleIoAction::parse(
        name,
        &args.iter().map(|v| v.to_string()).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn memory() -> Memory {
    let mut m = Memory::default();
    m.module_io
        .configure(0, VirtualModule::ad_e2(0x0110).unwrap())
        .unwrap();
    m
}
fn clean(m: &Memory) {
    assert!(
        m.execution_limitations.is_empty(),
        "{:?}",
        m.execution_limitations
    );
}
#[test]
fn four_base_forms_have_genuine_headers_and_narrow_matrix() {
    for (name, header, words) in [
        ("FROM", 0xac00, 9),
        ("DFROM", 0xad00, 17),
        ("TO", 0xae00, 9),
        ("DTO", 0xaf00, 17),
    ] {
        let a = action(name, &["K0", "K0", "D0", "K1"]);
        assert_eq!(a.header(), header);
        assert_eq!(a.words(), words);
        assert_eq!(a.operands().len(), 4);
    }
    for args in [
        ["K0", "K0", "K0", "K1"],
        ["M0", "K0", "D0", "K1"],
        ["K0", "T0", "D0", "K1"],
        ["K0", "K0", "K4M0", "K1"],
        ["K0", "K0", "E0", "K1"],
    ] {
        assert!(ModuleIoAction::parse("FROM", &args.map(String::from)).is_err());
    }
}
#[test]
fn absent_module_never_fabricates_zero_read_or_success_ledger() {
    let mut m = Memory::default();
    m.set_word(d(0), 123);
    action("FROM", &["K0", "K0", "D0", "K1"]).execute(&mut m, true, 10);
    assert_eq!(m.word(d(0)), 123);
    assert!(m.module_io.last_transfer.is_none());
    assert!(!m.execution_limitations.is_empty());
}
#[test]
fn identity_defaults_and_unconfigured_adc_are_distinct() {
    let mut m = memory();
    action("FROM", &["K0", "K0", "D0", "K2"]).execute(&mut m, true, 10);
    assert_eq!(m.word(d(0)), 0x80);
    assert_eq!(m.word(d(1)), 0x0110);
    assert!(m.module_io.read_cr(0, 20).is_err());
    clean(&m);
}
#[test]
fn from_whole_block_is_atomic_when_later_cr_is_reserved() {
    let mut m = memory();
    m.set_word(d(0), 100);
    m.set_word(d(1), 200);
    action("FROM", &["K0", "K5", "D0", "K2"]).execute(&mut m, true, 10);
    assert_eq!((m.word(d(0)), m.word(d(1))), (100, 200));
    assert!(!m.execution_limitations.is_empty());
}
#[test]
fn wide_count_is_pairs_low_word_first_and_signed_bits_are_preserved() {
    let mut m = memory();
    m.module_io
        .set_samples(0, [-1, i16::MIN, 123, 456])
        .unwrap();
    action("DFROM", &["K0", "K20", "D0", "K2"]).execute(&mut m, true, 10);
    assert_eq!(m.double_word(d(0)), 0x8000_ffffu32 as i32);
    assert_eq!(m.double_word(d(2)), (456 << 16) | 123);
    assert_eq!(m.module_io.last_transfer.as_ref().unwrap().words, 4);
    clean(&m);
}
#[test]
fn dto_and_two_word_to_have_identical_cr_result() {
    let mut a = memory();
    let mut b = memory();
    for m in [&mut a, &mut b] {
        m.set_word(d(0), 2);
        m.set_word(d(1), 3);
    }
    action("DTO", &["K0", "K2", "D0", "K1"]).execute(&mut a, true, 10);
    action("TO", &["K0", "K2", "D0", "K2"]).execute(&mut b, true, 10);
    assert_eq!(a.module_io.read_cr(0, 2), b.module_io.read_cr(0, 2));
    assert_eq!(a.module_io.read_cr(0, 3), b.module_io.read_cr(0, 3));
    clean(&a);
    clean(&b);
}
#[test]
fn write_preflight_does_not_partially_modify_good_cr_before_bad_cr() {
    let mut module = VirtualModule::ad_e2(0x0110).unwrap();
    assert!(module.write_batch(4, &[2, 3, 4], 0).is_err());
    assert_eq!(module.read_cr(4).unwrap(), 0);
    assert_eq!(module.read_cr(5).unwrap(), 0);
}
#[test]
fn readonly_identity_and_averages_cannot_be_overwritten() {
    let mut module = VirtualModule::ad_e2(0x0110).unwrap();
    for cr in [0, 1, 12, 20, 43] {
        assert!(module.write_batch(cr, &[123], 0).is_err());
    }
    assert_eq!(module.read_cr(0).unwrap(), 0x80);
}
#[test]
fn dynamic_controls_are_snapshotted_before_aliasing_read_destination() {
    let mut m = memory();
    m.set_word(d(0), 0);
    m.set_word(d(1), 0);
    m.set_word(d(2), 2);
    action("FROM", &["D0", "D1", "D0", "D2"]).execute(&mut m, true, 10);
    assert_eq!(m.word(d(0)), 0x80);
    assert_eq!(m.word(d(1)), 0x0110);
    clean(&m);
}
#[test]
fn false_visit_holds_all_module_and_cpu_state() {
    let mut m = memory();
    let before = m.module_io.read_cr(0, 2);
    action("TO", &["K0", "K2", "K5", "K1"]).execute(&mut m, false, 10);
    assert_eq!(m.module_io.read_cr(0, 2), before);
    assert!(m.module_io.last_transfer.is_none());
    clean(&m);
}
#[test]
fn cpu_firmware_count_limit_and_complete_d_end_span_are_checked() {
    let mut m = memory();
    m.module_io
        .set_cpu(ModuleCpuProfile::new(ModuleCpu::Es2, 2, 80).unwrap())
        .unwrap();
    action("FROM", &["K0", "K0", "D0", "K5"]).execute(&mut m, true, 10);
    assert!(!m.execution_limitations.is_empty());
    m.execution_limitations.clear();
    m.data_register_limit = Some(4999);
    m.module_io.set_samples(0, [1, 2, 3, 4]).unwrap();
    action("DFROM", &["K0", "K20", "D4998", "K1"]).execute(&mut m, true, 10);
    assert_eq!(m.word(d(4998)), 1);
    assert_eq!(m.word(d(4999)), 2);
    clean(&m);
    action("FROM", &["K0", "K20", "D4999", "K2"]).execute(&mut m, true, 10);
    assert!(!m.execution_limitations.is_empty());
}
#[test]
fn cpu_module_compatibility_is_explicit_and_change_is_atomic() {
    let mut bus = VirtualModuleBus::default();
    assert!(
        bus.configure(0, VirtualModule::ad_s2(0x0500).unwrap())
            .is_err()
    );
    bus.configure(0, VirtualModule::ad_e2(0x0110).unwrap())
        .unwrap();
    assert!(
        bus.set_cpu(ModuleCpuProfile::new(ModuleCpu::Ss2, 3, 0).unwrap())
            .is_err()
    );
    assert_eq!(bus.cpu().model, ModuleCpu::Es2);
    bus.remove(0);
    bus.set_cpu(ModuleCpuProfile::new(ModuleCpu::Ss2, 3, 0).unwrap())
        .unwrap();
    bus.configure(0, VirtualModule::ad_s2(0x0500).unwrap())
        .unwrap();
    assert_eq!(bus.read_cr(0, 0).unwrap(), 0x90);
}
#[test]
fn disabled_channels_have_model_specific_defined_results_without_samples() {
    let mut e = VirtualModule::ad_e2(0x0110).unwrap();
    e.write_batch(2, &[u16::MAX], 0).unwrap();
    assert_eq!(e.read_cr(20).unwrap(), 32767);
    assert_eq!(e.read_cr(12).unwrap(), 32767);
    let mut s = VirtualModule::ad_s2(0x0500).unwrap();
    s.write_batch(1, &[7], 0).unwrap();
    assert_eq!(s.read_cr(12).unwrap(), 0);
    assert_eq!(s.read_cr(6).unwrap(), 0);
}
#[test]
fn e2_mode_change_resets_calibration_only_for_affected_channel() {
    let mut e = VirtualModule::ad_e2(0x0110).unwrap();
    e.set_samples([1, 2, 3, 4]);
    e.write_batch(28, &[123], 0).unwrap();
    e.write_batch(34, &[17000], 0).unwrap();
    e.write_batch(2, &[1], 0).unwrap();
    assert_eq!(e.read_cr(28).unwrap(), 0);
    assert_eq!(e.read_cr(34).unwrap(), 16000);
    assert!(e.read_cr(20).is_err());
    assert_eq!(e.read_cr(21).unwrap(), 2);
    e.write_batch(2, &[6], 0).unwrap();
    assert!(e.write_batch(28, &[1], 0).is_err());
}
#[test]
fn s2_mode_change_does_not_invent_e2_calibration_reset_and_fw_gate_is_real() {
    let mut s = VirtualModule::ad_s2(0x0416).unwrap();
    s.write_batch(18, &[100], 0).unwrap();
    s.write_batch(1, &[1], 0).unwrap();
    assert_eq!(s.read_cr(18).unwrap(), 100);
    assert!(s.write_batch(1, &[4], 0).is_err());
    assert_eq!(s.read_cr(30).unwrap() & 4, 4);
    let mut newer = VirtualModule::ad_s2(0x0500).unwrap();
    newer.write_batch(1, &[4], 0).unwrap();
    assert_eq!(newer.read_cr(1).unwrap(), 4);
}
#[test]
fn e2_unsaved_and_saved_parameters_have_different_power_cycle_results() {
    let mut e = VirtualModule::ad_e2(0x0110).unwrap();
    e.write_batch(8, &[20], 0).unwrap();
    e.power_cycle(0).unwrap();
    assert_eq!(e.read_cr(8).unwrap(), 10);
    e.write_batch(8, &[20], 0).unwrap();
    e.write_batch(41, &[0x5678], 0).unwrap();
    e.write_batch(8, &[30], 0).unwrap();
    e.power_cycle(1).unwrap();
    assert_eq!(e.read_cr(8).unwrap(), 20);
    assert!(e.read_cr(20).is_err());
}
#[test]
fn e2_reset_requires_supported_firmware_and_one_second_before_power_cycle() {
    let mut old = VirtualModule::ad_e2(0x0109).unwrap();
    assert!(old.write_batch(0, &[0x4352], 0).is_err());
    let mut e = VirtualModule::ad_e2(0x0110).unwrap();
    e.write_batch(8, &[20], 0).unwrap();
    e.write_batch(41, &[0x5678], 0).unwrap();
    e.write_batch(0, &[0x4352], 0).unwrap();
    e.power_cycle(999_999).unwrap();
    assert_eq!(e.read_cr(8).unwrap(), 20);
    e.write_batch(0, &[0x4352], 1_000_000).unwrap();
    e.power_cycle(2_000_000).unwrap();
    assert_eq!(e.read_cr(8).unwrap(), 10);
    assert_eq!(e.read_cr(0).unwrap(), 0x80);
}
#[test]
fn s2_lock_latch_factory_reset_and_reserved_policy_are_separate() {
    let mut s = VirtualModule::ad_s2(0x0500).unwrap();
    s.write_batch(33, &[1], 0).unwrap();
    assert!(s.write_batch(18, &[10], 0).is_err());
    s.write_batch(33, &[2], 0).unwrap();
    s.write_batch(18, &[100], 0).unwrap();
    s.power_cycle(0).unwrap();
    assert_eq!(s.read_cr(18).unwrap(), 0);
    s.write_batch(31, &[99], 0).unwrap();
    s.write_batch(33, &[4], 0).unwrap();
    assert_eq!(s.read_cr(31).unwrap(), 1);
    assert!(s.write_batch(33, &[8], 0).is_err());
}
#[test]
fn s2_communication_identity_has_no_modbus_plus_one_address_mistake() {
    let mut s = VirtualModule::ad_s2(0x0500).unwrap();
    assert_eq!(s.read_cr(31).unwrap(), 1);
    assert_eq!(s.read_cr(32).unwrap(), 2);
    s.write_batch(32, &[0xc310], 0).unwrap();
    assert_eq!(s.read_cr(32).unwrap(), 0xc310);
    assert!(s.write_batch(32, &[0xc210], 0).is_err());
    assert!(s.read_cr(0x4000).is_err());
}
#[test]
fn e2_limits_use_signed_injected_results_and_errors_are_module_local() {
    let mut e = VirtualModule::ad_e2(0x0110).unwrap();
    e.write_batch(102, &[100], 0).unwrap();
    e.write_batch(108, &[(-100i16) as u16], 0).unwrap();
    e.write_batch(100, &[1], 0).unwrap();
    e.set_samples([101, 0, 0, 0]);
    assert_eq!(e.read_cr(101).unwrap(), 256);
    assert_eq!(e.read_cr(43).unwrap() & 4, 4);
    e.set_samples([-101, 0, 0, 0]);
    assert_eq!(e.read_cr(101).unwrap(), 1);
    e.set_samples([100, 0, 0, 0]);
    assert_eq!(e.read_cr(101).unwrap(), 0);
}
#[test]
fn explicit_conversion_events_are_bounded_and_not_advanced_by_plc_reads() {
    let mut e = VirtualModule::ad_e2(0x0110).unwrap();
    for _ in 0..9 {
        assert!(e.push_conversion_event([10, 20, 30, 40]).is_err());
    }
    assert!(e.read_cr(12).is_err());
    e.push_conversion_event([20, 30, 40, 50]).unwrap();
    assert_eq!(e.read_cr(12).unwrap(), 11);
    assert_eq!(e.read_cr(20).unwrap(), 20);
    for _ in 0..100 {
        assert_eq!(e.read_cr(12).unwrap(), 11);
    }
    assert_eq!(e.history[0].len(), 10);
}
#[test]
fn reset_instruction_state_preserves_external_configuration_and_readings() {
    let mut m = memory();
    m.module_io.set_samples(0, [1, 2, 3, 4]).unwrap();
    action("FROM", &["K0", "K20", "D0", "K1"]).execute(&mut m, true, 10);
    m.module_io.reset_instruction_state();
    assert!(m.module_io.last_transfer.is_none());
    assert_eq!(m.module_io.read_cr(0, 20).unwrap(), 1);
    assert_eq!(m.module_io.modules().count(), 1);
}

#[test]
fn rolled_back_limit_changes_do_not_leak_candidate_alarm_bits() {
    let mut module = VirtualModule::ad_e2(0x0110).unwrap();
    module.set_samples([200, 0, 0, 0]);
    assert_eq!(module.read_cr(43).unwrap(), 0);
    // Candidate enables CH1, acknowledges status, sets upper=100 (alarm),
    // then fails because CH2 upper=-32768 is below its default lower=-32000.
    assert!(module.write_batch(100, &[1, 0, 100, 0x8000], 0).is_err());
    assert_eq!(module.read_cr(100).unwrap(), 0);
    assert_eq!(module.read_cr(101).unwrap(), 0);
    assert_eq!(module.read_cr(102).unwrap(), 32000);
    assert_eq!(module.read_cr(103).unwrap(), 32000);
    assert_eq!(module.read_cr(43).unwrap(), 0x0800);
}

#[test]
fn rolled_back_factory_reset_cannot_clear_original_module_faults() {
    let mut module = VirtualModule::ad_s2(0x0500).unwrap();
    module.write_batch(31, &[99], 0).unwrap();
    assert!(module.write_batch(2, &[0], 0).is_err());
    assert_eq!(module.read_cr(30).unwrap(), 64);
    // Reset is only a candidate; the following firmware CR is read-only.
    assert!(module.write_batch(33, &[4, 0], 0).is_err());
    assert_eq!(module.read_cr(31).unwrap(), 99);
    assert_eq!(module.read_cr(30).unwrap(), 64);
    assert_eq!(module.read_cr(33).unwrap(), 0);
}

#[test]
fn cpu_restart_preserves_pending_module_reset_elapsed_time_and_ledger_epoch() {
    let mut m = memory();
    m.timing_io.now_us = 10_000_000;
    action("TO", &["K0", "K8", "K20", "K1"]).execute(&mut m, true, 1);
    action("TO", &["K0", "K41", "H5678", "K1"]).execute(&mut m, true, 2);
    action("TO", &["K0", "K0", "H4352", "K1"]).execute(&mut m, true, 3);
    m.module_io.restart_cpu_clock(10_500_000).unwrap();
    m.module_io.reset_instruction_state();
    m.timing_io.now_us = 500_000;
    action("FROM", &["K0", "K8", "D0", "K1"]).execute(&mut m, true, 4);
    assert_eq!(
        m.module_io.last_transfer.as_ref().unwrap().at_us,
        11_000_000
    );
    assert_eq!(m.word(d(0)), 20);
    m.module_io.power_cycle(0, 500_000).unwrap();
    assert_eq!(m.module_io.read_cr(0, 8).unwrap(), 10);
    clean(&m);
}

#[test]
fn repeated_cpu_epochs_accumulate_without_falsely_qualifying_early_module_reset() {
    let mut m = memory();
    m.timing_io.now_us = 10_000_000;
    action("TO", &["K0", "K8", "K20", "K1"]).execute(&mut m, true, 1);
    action("TO", &["K0", "K41", "H5678", "K1"]).execute(&mut m, true, 2);
    action("TO", &["K0", "K0", "H4352", "K1"]).execute(&mut m, true, 3);
    m.module_io.restart_cpu_clock(10_100_000).unwrap();
    m.module_io.restart_cpu_clock(200_000).unwrap();
    assert_eq!(m.module_io.module_time_us(300_000).unwrap(), 10_600_000);
    m.module_io.power_cycle(0, 300_000).unwrap();
    assert_eq!(m.module_io.read_cr(0, 8).unwrap(), 20);
    clean(&m);
}

#[test]
fn external_clock_overflow_is_explicit_and_does_not_wrap_offset() {
    let mut bus = VirtualModuleBus::default();
    bus.restart_cpu_clock(u64::MAX).unwrap();
    assert!(bus.restart_cpu_clock(1).is_err());
    assert_eq!(bus.module_time_us(0).unwrap(), u64::MAX);
    assert!(bus.module_time_us(1).is_err());
}
