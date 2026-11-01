//! Compiled VM checks for explicitly configured virtual modules, not hardware parity.
use crate::module_io::{ModuleCpu, ModuleCpuProfile, ModuleKind, SampleFrame, VirtualModule};
use crate::plc::{Device, ScanStatus, Simulator, compile_il};
fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn setup(kind: ModuleKind) -> Simulator {
    let mut s = Simulator::default();
    let cpu = if kind == ModuleKind::Dvp04AdE2 {
        ModuleCpu::Es2
    } else {
        ModuleCpu::Ss2
    };
    s.memory
        .module_io
        .set_cpu(
            ModuleCpuProfile::new(
                cpu,
                if cpu == ModuleCpu::Es2 { 3 } else { 2 },
                if cpu == ModuleCpu::Es2 { 46 } else { 80 },
            )
            .unwrap(),
        )
        .unwrap();
    s.memory
        .module_io
        .configure(
            0,
            if kind == ModuleKind::Dvp04AdE2 {
                VirtualModule::ad_e2(0x0110)
            } else {
                VirtualModule::ad_s2(0x0500)
            }
            .unwrap(),
        )
        .unwrap();
    s
}
fn scan(s: &mut Simulator, source: &str) {
    let p = compile_il(source).unwrap();
    s.scan(&p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(
        s.memory.execution_limitations.is_empty(),
        "{:?}",
        s.memory.execution_limitations
    );
}
fn cr(s: &Simulator, n: u16) -> u16 {
    s.memory.module_io.read_cr(0, n).unwrap()
}
fn x(s: &mut Simulator, on: bool) {
    s.memory.set_input(d("X0"), on);
}
#[test]
fn all_eight_genuine_headers_lengths_and_later_word_offsets() {
    for (form, header, words) in [
        ("FROM", 0xAC00, 9),
        ("FROMP", 0xAC10, 9),
        ("DFROM", 0xAD00, 17),
        ("DFROMP", 0xAD10, 17),
        ("TO", 0xAE00, 9),
        ("TOP", 0xAE10, 9),
        ("DTO", 0xAF00, 17),
        ("DTOP", 0xAF10, 17),
    ] {
        let p = compile_il(&format!("LD X0\n{form} K0 K2 D20 K1\nMOV K7 D100\nEND")).unwrap();
        let layout = crate::es2_encoder::local_execution_layout(&p).unwrap();
        assert_eq!(layout.known_word_at(1), Some(header), "{form}");
        assert_eq!(layout.action_pc[0], 1);
        assert_eq!(layout.action_pc[1], 1 + words);
        assert!(
            crate::es2_encoder::encode(&p).is_err(),
            "installed handlers only skip"
        );
    }
}
#[test]
fn explicit_e2_sample_frame_preserves_distinct_average_and_present() {
    let mut s = setup(ModuleKind::Dvp04AdE2);
    s.memory
        .module_io
        .set_sample_frame(
            0,
            SampleFrame {
                present: [100, -200, 300, -400],
                average: [90, -190, 290, -390],
            },
        )
        .unwrap();
    x(&mut s, true);
    scan(
        &mut s,
        "LD X0\nFROM K0 K12 D100 K4\nFROM K0 K20 D110 K4\nEND",
    );
    assert_eq!(s.memory.word(d("D100")), 90);
    assert_eq!(s.memory.word(d("D111")), -200);
}
#[test]
fn explicit_ss2_profile_uses_s2_cr_map() {
    let mut s = setup(ModuleKind::Dvp04AdS2);
    s.memory.module_io.set_samples(0, [1, 2, 3, 4]).unwrap();
    x(&mut s, true);
    scan(
        &mut s,
        "LD X0\nFROM K0 K0 D100 K1\nFROM K0 K12 D110 K4\nEND",
    );
    assert_eq!(s.memory.word(d("D100")), 0x90);
    assert_eq!(s.memory.word(d("D113")), 4);
}
#[test]
fn double_transfer_low_high_word_order_and_two_elements() {
    let mut s = setup(ModuleKind::Dvp04AdE2);
    for (i, v) in [1, 2, 3, 4].into_iter().enumerate() {
        s.memory.set_word(d(&format!("D{}", 20 + i)), v);
    }
    x(&mut s, true);
    scan(&mut s, "LD X0\nDTO K0 K8 D20 K2\nDFROM K0 K8 D100 K2\nEND");
    for i in 0..4 {
        assert_eq!(cr(&s, 8 + i), 1 + i);
        assert_eq!(s.memory.word(d(&format!("D{}", 100 + i))), 1 + i as i16);
    }
}
#[test]
fn top_first_held_false_rearm_uses_real_vm_edge_history() {
    let mut s = setup(ModuleKind::Dvp04AdE2);
    let p = compile_il("LD X0\nTOP K0 K8 D20 K1\nEND").unwrap();
    s.memory.set_word(d("D20"), 3);
    x(&mut s, true);
    s.scan(&p, 0);
    assert_eq!(cr(&s, 8), 3);
    s.memory.set_word(d("D20"), 7);
    s.scan(&p, 0);
    assert_eq!(cr(&s, 8), 3);
    x(&mut s, false);
    s.scan(&p, 0);
    assert_eq!(cr(&s, 8), 3);
    x(&mut s, true);
    s.scan(&p, 0);
    assert_eq!(cr(&s, 8), 7);
}
#[test]
fn fromp_holds_destination_until_next_rising_visit() {
    let mut s = setup(ModuleKind::Dvp04AdE2);
    let p = compile_il("LD X0\nFROMP K0 K20 D100 K1\nEND").unwrap();
    s.memory.module_io.set_samples(0, [10; 4]).unwrap();
    x(&mut s, true);
    s.scan(&p, 0);
    s.memory.module_io.set_samples(0, [20; 4]).unwrap();
    s.scan(&p, 0);
    assert_eq!(s.memory.word(d("D100")), 10);
    x(&mut s, false);
    s.scan(&p, 0);
    x(&mut s, true);
    s.scan(&p, 0);
    assert_eq!(s.memory.word(d("D100")), 20);
}
#[test]
fn absent_module_is_explicit_limitation_and_holds_destination() {
    let mut s = Simulator::default();
    s.memory.set_word(d("D100"), 77);
    x(&mut s, true);
    s.scan(&compile_il("LD X0\nFROM K0 K0 D100 K1\nEND").unwrap(), 0);
    assert_eq!(s.memory.word(d("D100")), 77);
    assert!(!s.memory.execution_limitations.is_empty());
}
#[test]
fn missing_samples_do_not_manufacture_zero() {
    let mut s = setup(ModuleKind::Dvp04AdE2);
    s.memory.set_word(d("D100"), 77);
    x(&mut s, true);
    s.scan(&compile_il("LD X0\nFROM K0 K20 D100 K1\nEND").unwrap(), 0);
    assert_eq!(s.memory.word(d("D100")), 77);
    assert!(!s.memory.execution_limitations.is_empty());
}
#[test]
fn disabled_contact_holds_and_does_not_require_module() {
    let mut s = Simulator::default();
    s.memory.set_word(d("D100"), 77);
    scan(&mut s, "LD X0\nFROM K0 K0 D100 K1\nEND");
    assert_eq!(s.memory.word(d("D100")), 77);
}
#[test]
fn readonly_second_word_rejects_entire_write_transaction() {
    let mut s = setup(ModuleKind::Dvp04AdE2);
    x(&mut s, true);
    s.memory.set_word(d("D20"), 5);
    s.memory.set_word(d("D21"), 88);
    s.scan(&compile_il("LD X0\nTO K0 K11 D20 K2\nEND").unwrap(), 0);
    assert_eq!(cr(&s, 11), 10);
    assert!(!s.memory.execution_limitations.is_empty());
}
#[test]
fn reserved_second_cr_rejects_entire_read_transaction() {
    let mut s = setup(ModuleKind::Dvp04AdE2);
    x(&mut s, true);
    s.memory.set_word(d("D100"), 77);
    s.memory.set_word(d("D101"), 88);
    s.scan(&compile_il("LD X0\nFROM K0 K5 D100 K2\nEND").unwrap(), 0);
    assert_eq!(s.memory.word(d("D100")), 77);
    assert_eq!(s.memory.word(d("D101")), 88);
    assert!(!s.memory.execution_limitations.is_empty());
}
#[test]
fn ss2_full_double_destination_extent_rejected_before_any_write() {
    let mut s = setup(ModuleKind::Dvp04AdS2);
    s.memory.data_register_limit = Some(4999);
    x(&mut s, true);
    s.memory.set_word(d("D4999"), 77);
    s.scan(&compile_il("LD X0\nDFROM K0 K2 D4999 K1\nEND").unwrap(), 0);
    assert_eq!(s.memory.word(d("D4999")), 77);
    assert!(!s.memory.execution_limitations.is_empty());
}
#[test]
fn dynamic_count_is_snapshotted_before_overlapping_read_dest() {
    let mut s = setup(ModuleKind::Dvp04AdE2);
    x(&mut s, true);
    s.memory.set_word(d("D100"), 2);
    scan(&mut s, "LD X0\nFROM K0 K8 D100 D100\nEND");
    assert_eq!(s.memory.word(d("D100")), 10);
    assert_eq!(s.memory.word(d("D101")), 10);
    assert_eq!(s.memory.module_io.last_transfer.as_ref().unwrap().words, 2);
}
#[test]
fn source_order_write_then_read_observes_committed_register() {
    let mut s = setup(ModuleKind::Dvp04AdE2);
    x(&mut s, true);
    scan(&mut s, "LD X0\nTO K0 K8 K3 K1\nFROM K0 K8 D100 K1\nEND");
    assert_eq!(s.memory.word(d("D100")), 3);
}
#[test]
fn mc_false_does_not_mutate_module() {
    let mut s = setup(ModuleKind::Dvp04AdE2);
    scan(
        &mut s,
        "LD X0\nMC N0\nLD M1000\nTO K0 K8 K3 K1\nMCR N0\nEND",
    );
    assert_eq!(cr(&s, 8), 10);
}
#[test]
fn call_inside_for_reuses_actual_module_instruction_pc() {
    let mut s = setup(ModuleKind::Dvp04AdE2);
    x(&mut s, true);
    let p = compile_il(
        "LD X0\nFOR K3\nCALL P0\nNEXT\nFEND\nP0\nLD X0\nTO K0 K8 K3 K1\nINC D200\nSRET\nEND",
    )
    .unwrap();
    s.scan(&p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(s.memory.execution_limitations.is_empty());
    assert_eq!(s.memory.word(d("D200")), 3);
    let layout = crate::es2_encoder::local_execution_layout(&p).unwrap();
    let pc = s.memory.module_io.last_transfer.as_ref().unwrap().pc;
    assert!(layout.action_pc.contains(&pc));
    assert_eq!(cr(&s, 8), 3);
}
#[test]
fn first_scan_startup_configuration_is_not_repeated() {
    let mut s = setup(ModuleKind::Dvp04AdE2);
    let p = compile_il("LD M1002\nTO K0 K8 K3 K1\nLD X0\nTO K0 K8 K7 K1\nEND").unwrap();
    s.scan(&p, 0);
    assert_eq!(cr(&s, 8), 3);
    x(&mut s, true);
    s.scan(&p, 0);
    assert_eq!(cr(&s, 8), 7);
    x(&mut s, false);
    s.scan(&p, 0);
    assert_eq!(cr(&s, 8), 7);
}
#[test]
fn cpu_ram_restart_preserves_external_module_configuration_and_samples() {
    let mut s = setup(ModuleKind::Dvp04AdE2);
    s.memory.module_io.set_samples(0, [123; 4]).unwrap();
    x(&mut s, true);
    scan(&mut s, "LD X0\nTO K0 K8 K3 K1\nMOV K99 D200\nEND");
    s.restart_preserving_file_store();
    assert_eq!(s.memory.word(d("D200")), 0);
    assert_eq!(cr(&s, 8), 3);
    assert_eq!(cr(&s, 20), 123);
    assert!(s.memory.module_io.last_transfer.is_none());
}
#[test]
fn module_power_cycle_differs_from_cpu_ram_restart() {
    let mut s = setup(ModuleKind::Dvp04AdE2);
    x(&mut s, true);
    scan(&mut s, "LD X0\nTO K0 K8 K3 K1\nEND");
    s.memory.module_io.power_cycle(0, 0).unwrap();
    assert_eq!(cr(&s, 8), 10);
    scan(&mut s, "LD X0\nTO K0 K8 K3 K1\nTO K0 K41 H5678 K1\nEND");
    s.memory.module_io.power_cycle(0, 0).unwrap();
    assert_eq!(cr(&s, 8), 3);
}
#[test]
fn cpu_restart_preserves_elapsed_time_for_pending_external_module_reset() {
    let mut s = setup(ModuleKind::Dvp04AdE2);
    x(&mut s, true);
    scan(&mut s, "LD X0\nTO K0 K8 K3 K1\nTO K0 K41 H5678 K1\nEND");
    s.memory.timing_io.now_us = 10_000_000;
    scan(&mut s, "LD X0\nTO K0 K0 H4352 K1\nEND");
    s.restart_preserving_file_store();
    assert_eq!(s.memory.timing_io.now_us, 0);
    assert!(s.memory.execution_limitations.is_empty());
    s.memory.timing_io.now_us = 1_000_000;
    s.memory
        .module_io
        .power_cycle(0, s.memory.timing_io.now_us)
        .unwrap();
    assert_eq!(cr(&s, 8), 10);
}
#[test]
fn published_s2_packed_mode_setting_and_disabled_read() {
    let mut s = setup(ModuleKind::Dvp04AdS2);
    x(&mut s, true);
    scan(&mut s, "LD X0\nTO K0 K1 H0688 K1\nFROM K0 K1 D100 K1\nEND");
    assert_eq!(s.memory.word(d("D100")), 0x688);
    scan(&mut s, "LD X0\nTO K0 K1 K7 K1\nFROM K0 K12 D110 K1\nEND");
    assert_eq!(s.memory.word(d("D110")), 0);
}
#[test]
fn invalid_operand_matrices_are_compile_errors() {
    for line in [
        "FROM K0 K0 T0 K1",
        "TO K0 K2 M0 K1",
        "FROM E0 K0 D20 K1",
        "TO K0 K2 D20 T0",
    ] {
        assert!(
            compile_il(&format!("LD X0\n{line}\nEND")).is_err(),
            "{line}"
        );
    }
}

#[test]
fn inactive_sfc_step_holds_module_then_explicit_initial_step_runs() {
    let mut s = setup(ModuleKind::Dvp04AdE2);
    let p = compile_il("LD X1\nSET S0\nSTL S0\nLD M1000\nTO K0 K8 K3 K1\nRET\nEND").unwrap();
    s.scan(&p, 0);
    assert_eq!(cr(&s, 8), 10);
    s.memory.set_input(d("X1"), true);
    s.scan(&p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(s.memory.execution_limitations.is_empty());
    // SET changes the live S image; STL reads the previous END snapshot.
    // Match transitions_wait_for_end_snapshot_and_exit_cleanup_is_one_visit.
    assert!(s.memory.bit(d("S0")));
    assert_eq!(cr(&s, 8), 10);
    s.memory.set_input(d("X1"), false);
    s.scan(&p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(s.memory.execution_limitations.is_empty());
    assert_eq!(cr(&s, 8), 3);
}
#[test]
fn old_cpu_firmware_rejects_six_words_atomically() {
    let mut s = setup(ModuleKind::Dvp04AdE2);
    s.memory
        .module_io
        .set_cpu(ModuleCpuProfile::new(ModuleCpu::Es2, 2, 99).unwrap())
        .unwrap();
    x(&mut s, true);
    s.scan(&compile_il("LD X0\nTO K0 K8 K3 K6\nEND").unwrap(), 0);
    assert_eq!(cr(&s, 8), 10);
    assert!(!s.memory.execution_limitations.is_empty());
}
