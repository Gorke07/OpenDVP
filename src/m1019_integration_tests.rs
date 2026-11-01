//! Public operands and arithmetic storage around M1019. LED assertions use
//! explicit local supply inputs; they make no physical or native-LED claim.
use crate::plc::{Device, Memory, Program, ScanStatus, Simulator, compile_il};
use crate::power_monitor::{ErrorLed, PowerBackend, PowerState, RecoveryPolicy, SupplyInput};

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}

fn active() -> Simulator {
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim.memory.set_input(d("X1"), true);
    PowerState::set_input(&mut sim.memory, SupplyInput::Normal);
    sim
}

fn run(sim: &mut Simulator, program: &Program) {
    sim.scan(program, 0);
    assert_eq!(sim.last_scan, ScanStatus::Complete);
    assert!(
        sim.memory.execution_limitations.is_empty(),
        "{:?}",
        sim.memory.execution_limitations
    );
}

fn put_nibble(memory: &mut Memory, value: u8) {
    for offset in 0..4 {
        memory.set_internal(
            d(&format!("M{}", 1019 + offset)),
            value & (1 << offset) != 0,
        );
    }
}

fn nibble(memory: &Memory) -> u8 {
    (0..4).fold(0, |value, offset| {
        value | (u8::from(memory.bit(d(&format!("M{}", 1019 + offset)))) << offset)
    })
}

#[test]
fn m1019_is_public_but_the_undefined_m1023_gap_stays_closed() {
    assert!("M1019".parse::<Device>().is_ok());
    assert!("M1023".parse::<Device>().is_err());
    for instruction in [
        "MOV K1 K1M1019",
        "DMOV K1 K1M1019",
        "MOV K1M1019 D0",
        "DMOV K1M1019 D0",
    ] {
        let program = compile_il(&format!("LD X0\n{instruction}\nEND")).unwrap();
        crate::es2_encoder::encode(&program).unwrap();
        for index in 1019..=1022 {
            assert!(program.devices.contains(&d(&format!("M{index}"))));
        }
    }
    for instruction in ["MOV K1 K1M1020", "MOV K1M1020 D0", "MOV K1 K2M1019"] {
        assert!(
            compile_il(&format!("LD X0\n{instruction}\nEND")).is_err(),
            "{instruction} includes the unsupported M1023 slot"
        );
    }
}

#[test]
fn full_result_group_targets_reject_the_second_chunk_through_m1023() {
    for name in ["MUL", "DIV", "DMUL", "DDIV"] {
        for pulse in ["", "P"] {
            let source = format!("LD X0\n{name}{pulse} K9 K2 K1M1019\nEND");
            assert!(compile_il(&source).is_err(), "{source}");
        }
    }
}

#[test]
fn an_indexed_result_cannot_shift_the_legal_nibble_into_m1023() {
    let program = compile_il("LD X0\nMUL16 K2 K4 K1M1019E1\nEND").unwrap();
    let mut sim = active();
    put_nibble(&mut sim.memory, 9);
    sim.memory.set_word(d("D1182"), 1);
    sim.scan(&program, 0);
    assert_eq!(sim.last_scan, ScanStatus::Complete);
    assert!(!sim.memory.execution_limitations.is_empty());
    assert_eq!(nibble(&sim.memory), 9, "qualification precedes carry clear");
    assert!(!sim.memory.system_bit(1067));
    assert!(!sim.memory.system_bit(1068));
}

#[test]
fn direct_indexed_and_group_writes_share_the_canonical_control_bit() {
    for (writer, bit, group) in [
        ("LD X0\nOUT M1019", "M1019", "K1M1019"),
        ("LD X0\nOUT M1018E1", "M1018E1", "K1M1018E1"),
        ("LD X1\nMOV D10 K1M1019", "M1019", "K1M1019"),
    ] {
        let program = compile_il(&format!(
            "{writer}\nLD {bit}\nOUT Y0\nLD X1\nMOV {group} D0\nEND"
        ))
        .unwrap();
        let mut sim = active();
        sim.memory.set_word(d("D1182"), 1);
        sim.memory.set_internal(d("M1018"), true);
        sim.memory.set_internal(d("M1026"), true);
        for value in [true, false, true] {
            sim.memory.set_input(d("X0"), value);
            sim.memory.set_word(d("D10"), i16::from(value));
            run(&mut sim, &program);
            assert_eq!(sim.memory.system_bit(1019), value, "{writer}");
            assert_eq!(sim.memory.bit(d("Y0")), value, "{writer}");
            assert_eq!(sim.memory.word(d("D0")), i16::from(value), "{writer}");
            assert!(sim.memory.system_bit(1018));
            assert!(sim.memory.system_bit(1026));
            assert_eq!(sim.memory.word(d("D1182")), 1);
        }
    }
}

#[test]
fn same_scan_on_then_off_writes_service_the_monitor_at_each_real_write() {
    for actions in [
        "SET M1019\nRST M1019",
        "SET M1018E1\nRST M1018E1",
        "MOV K1 K1M1019\nMOV K0 K1M1019",
    ] {
        let program = compile_il(&format!("LD X0\n{actions}\nEND")).unwrap();
        let mut sim = active();
        sim.memory.set_word(d("D1182"), 1);
        PowerState::set_input(&mut sim.memory, SupplyInput::Unstable);
        assert_eq!(sim.memory.power_monitor.led(), ErrorLed::Unknown);
        run(&mut sim, &program);
        assert!(!sim.memory.system_bit(1019));
        // Looking only at the final OFF value would miss the ON instability.
        assert_eq!(
            sim.memory.power_monitor.led(),
            ErrorLed::Flashing,
            "{actions}"
        );
    }
}

#[test]
fn fitting_products_can_set_carry_through_the_result_store_itself() {
    for name in ["MUL16", "MUL32"] {
        for pulse in ["", "P"] {
            for target in ["K1M1019", "K1M1018E1"] {
                let program =
                    compile_il(&format!("LD X0\n{name}{pulse} K2 K4 {target}\nEND")).unwrap();
                let mut sim = active();
                sim.memory.set_word(d("D1182"), 1);
                sim.memory.set_internal(d("M1018"), true);
                sim.memory.set_internal(d("M1026"), true);
                put_nibble(&mut sim.memory, 7);
                run(&mut sim, &program);
                assert_eq!(nibble(&sim.memory), 8, "{name}{pulse} {target}");
                assert!(sim.memory.system_bit(1022));
                assert!(sim.memory.system_bit(1018) && sim.memory.system_bit(1026));
            }
        }
    }
}

#[test]
fn overflowing_group_products_set_carry_after_zero_store_despite_sticky_errors() {
    for (name, operands) in [("MUL16", "K300 K300"), ("MUL32", "K65536 K65536")] {
        for pulse in ["", "P"] {
            for target in ["K1M1019", "K1M1018E1"] {
                for error in [None, Some(1067), Some(1068)] {
                    let program =
                        compile_il(&format!("LD X0\n{name}{pulse} {operands} {target}\nEND"))
                            .unwrap();
                    let mut sim = active();
                    sim.memory.set_word(d("D1182"), 1);
                    put_nibble(&mut sim.memory, 7);
                    if let Some(flag) = error {
                        sim.memory.set_system(flag, true);
                    }
                    run(&mut sim, &program);
                    assert_eq!(nibble(&sim.memory), 8, "{name}{pulse} {target} {error:?}");
                    assert_eq!(sim.memory.system_bit(1067), error == Some(1067));
                    assert_eq!(sim.memory.system_bit(1068), error == Some(1068));
                }
            }
        }
    }
}

#[test]
fn both_group_sources_snapshot_carry_before_multiply_clears_it() {
    for name in ["MUL16", "MUL32"] {
        for pulse in ["", "P"] {
            for group in ["K1M1019", "K1M1018E1"] {
                let program =
                    compile_il(&format!("LD X0\n{name}{pulse} {group} {group} D0\nEND")).unwrap();
                let mut sim = active();
                sim.memory.set_word(d("D1182"), 1);
                put_nibble(&mut sim.memory, 9);
                run(&mut sim, &program);
                assert_eq!(sim.memory.double_word(d("D0")), 81, "{name}{pulse} {group}");
                assert_eq!(
                    nibble(&sim.memory),
                    1,
                    "only the source carry bit is cleared"
                );
            }
        }
    }
}

#[test]
fn held_pulse_does_not_clear_carry_or_resolve_a_changed_invalid_target() {
    for name in ["MUL16P", "MUL32P"] {
        let program = compile_il(&format!("LD X0\n{name} K2 K4 K1M1018E1\nEND")).unwrap();
        let mut sim = active();
        sim.memory.set_word(d("D1182"), 1);
        run(&mut sim, &program);
        assert_eq!(nibble(&sim.memory), 8);
        put_nibble(&mut sim.memory, 3);
        sim.memory.set_word(d("D1182"), -30000);
        run(&mut sim, &program);
        assert_eq!(nibble(&sim.memory), 3);
        sim.memory.set_input(d("X0"), false);
        run(&mut sim, &program);
        assert_eq!(nibble(&sim.memory), 3);
        sim.memory.set_word(d("D1182"), 1);
        sim.memory.set_input(d("X0"), true);
        run(&mut sim, &program);
        assert_eq!(nibble(&sim.memory), 8);
    }
}

#[test]
fn full_arithmetic_can_read_the_legal_group_without_extending_its_source_span() {
    for name in ["MUL", "DIV", "DMUL", "DDIV"] {
        for pulse in ["", "P"] {
            let program = compile_il(&format!("LD X0\n{name}{pulse} K1M1019 K2 D0\nEND")).unwrap();
            let mut sim = active();
            put_nibble(&mut sim.memory, 9);
            let wide = matches!(name, "DMUL" | "DDIV");
            let division = name.contains("DIV");
            let guard = if wide { "D4" } else { "D2" };
            sim.memory.set_word(d(guard), 0x5a5a);
            run(&mut sim, &program);
            let low = if wide {
                sim.memory.double_word(d("D0"))
            } else {
                i32::from(sim.memory.word(d("D0")))
            };
            let high = if wide {
                sim.memory.double_word(d("D2"))
            } else {
                i32::from(sim.memory.word(d("D1")))
            };
            assert_eq!((low, high), if division { (4, 1) } else { (18, 0) });
            assert_eq!(nibble(&sim.memory), 9, "{name}{pulse} preserves flags");
            assert_eq!(sim.memory.word(d(guard)), 0x5a5a);
        }
    }
}

#[test]
fn plc_control_writes_do_not_manufacture_a_supply_classification() {
    let program = compile_il("LD X0\nSET M1019\nMOV K1 K1M1019\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    run(&mut sim, &program);
    assert!(sim.memory.system_bit(1019));
    assert_eq!(sim.memory.power_monitor.input(), SupplyInput::Unknown);
    assert_eq!(sim.memory.power_monitor.led(), ErrorLed::Unknown);
    assert_eq!(sim.memory.power_monitor.normal_for_us(), 0);
}

#[test]
fn stopped_explicit_time_recovers_only_after_more_than_two_normal_seconds() {
    let mut sim = Simulator::default();
    PowerState::set_input(&mut sim.memory, SupplyInput::Below17800Mv);
    assert_eq!(sim.memory.power_monitor.led(), ErrorLed::Flashing);
    PowerState::set_input(&mut sim.memory, SupplyInput::Normal);
    PowerState::advance(&mut sim.memory, 2_000_000);
    assert_eq!(sim.memory.power_monitor.led(), ErrorLed::Flashing);
    PowerState::advance(&mut sim.memory, 1);
    assert_eq!(sim.memory.power_monitor.led(), ErrorLed::Off);
    assert_eq!(sim.memory.power_monitor.normal_for_us(), 2_000_001);
    assert_eq!(sim.scans, 0);
    assert_eq!(
        sim.elapsed_ms, 0,
        "power time was supplied without a PLC scan"
    );
    assert!(!sim.memory.system_bit(1019));
}

#[test]
fn power_restart_clears_control_and_history_but_preserves_external_configuration() {
    for (backend, input, expected) in [
        (
            PowerBackend::DocumentedVirtual,
            SupplyInput::Unstable,
            ErrorLed::Unknown,
        ),
        (
            PowerBackend::DocumentedVirtual,
            SupplyInput::Below17800Mv,
            ErrorLed::Flashing,
        ),
        (
            PowerBackend::InstalledStorageOnly,
            SupplyInput::Below17800Mv,
            ErrorLed::Unavailable,
        ),
    ] {
        let mut sim = Simulator::default();
        PowerState::set_backend(&mut sim.memory, backend);
        PowerState::set_recovery_policy(&mut sim.memory, RecoveryPolicy::ReuseContinuousNormal);
        PowerState::set_input(&mut sim.memory, input);
        sim.memory.set_internal(d("M1019"), true);
        PowerState::advance(&mut sim.memory, 3_000_000);
        sim.restart_preserving_file_store();
        assert!(!sim.memory.system_bit(1019));
        assert_eq!(sim.memory.power_monitor.input(), input);
        assert_eq!(sim.memory.power_monitor.backend(), backend);
        assert_eq!(
            sim.memory.power_monitor.recovery_policy(),
            RecoveryPolicy::ReuseContinuousNormal
        );
        assert_eq!(sim.memory.power_monitor.power_elapsed_us(), 0);
        assert_eq!(sim.memory.power_monitor.normal_for_us(), 0);
        assert_eq!(sim.memory.power_monitor.led(), expected);
    }
}

#[test]
fn storage_backend_runs_plc_writes_without_replaying_virtual_voltage_history() {
    let program = compile_il("LD X0\nMUL16 K2 K4 K1M1019\nSET M1019\nEND").unwrap();
    let mut sim = active();
    PowerState::set_backend(&mut sim.memory, PowerBackend::InstalledStorageOnly);
    PowerState::set_input(&mut sim.memory, SupplyInput::Below17800Mv);
    run(&mut sim, &program);
    PowerState::advance(&mut sim.memory, 3_000_000);
    assert_eq!(nibble(&sim.memory), 9);
    assert_eq!(sim.memory.power_monitor.led(), ErrorLed::Unavailable);
    assert_eq!(sim.memory.power_monitor.normal_for_us(), 0);
    PowerState::set_input(&mut sim.memory, SupplyInput::Normal);
    PowerState::set_backend(&mut sim.memory, PowerBackend::DocumentedVirtual);
    assert_eq!(sim.memory.power_monitor.led(), ErrorLed::Off);
    assert!(sim.memory.system_bit(1019));
}

#[test]
fn moving_an_indexed_output_does_not_clear_its_former_m1019_target() {
    let program = compile_il("LD X0\nOUT M1018E1\nEND").unwrap();
    let mut sim = active();
    sim.memory.set_word(d("D1182"), 1);
    PowerState::set_input(&mut sim.memory, SupplyInput::Unstable);
    run(&mut sim, &program);
    assert!(sim.memory.system_bit(1019));
    assert_eq!(sim.memory.power_monitor.led(), ErrorLed::Flashing);
    sim.memory.set_word(d("D1182"), 0);
    sim.memory.set_input(d("X0"), false);
    run(&mut sim, &program);
    assert!(!sim.memory.system_bit(1018));
    assert!(sim.memory.system_bit(1019));
    assert_eq!(sim.memory.power_monitor.led(), ErrorLed::Flashing);
}
