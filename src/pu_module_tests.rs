use super::*;
fn dev(s: &str) -> Device {
    s.parse().unwrap()
}
fn action(line: &str) -> PuAction {
    let mut parts = line.split_whitespace();
    let op = parts.next().unwrap();
    PuAction::parse(op, &parts.map(str::to_owned).collect::<Vec<_>>()).unwrap()
}
fn memory() -> Memory {
    let mut m = Memory::default();
    m.pu_module.profile = PuProfile {
        model: PuCpuModel::Es2,
        firmware: FirmwareVersion::new(4, 2),
        ..Default::default()
    };
    m.pu_module.configure(0, VirtualPuModule::e2()).unwrap();
    m
}
fn advance(m: &mut Memory, at: u64) {
    PuState::advance(m, at);
}
#[test]
fn full_form_arities_lengths_and_editor_headers() {
    for (line, words) in [
        (
            "DPUCONF K0 K1 K1 K100 K100 K100 K100000 K0 K0 M0 M1 D100",
            49,
        ),
        ("PUSTAT K0 K1 X0 D10 M0 M1 M2 D100", 17),
        ("DPUPLS K0 K1 K10 K1000 M0 M1 D100", 29),
        ("DPUDRI K0 K1 K10 K1000 M0 M1 D100", 29),
        ("DPUDRA K0 K1 K10 K1000 M0 M1 D100", 29),
        ("DPUZRN K0 K1 K0 K0 K100 M0 M1 D100", 33),
        ("DPUJOG K0 K1 K1000 M0 M1 D100", 25),
        ("DPUMPG K0 K1 K7 D10 D12 D14 D16 D18 M0 D100", 41),
        ("DPUCNT K0 K7 K100 X0 D10 D12 M0 D100", 33),
        ("PUX K0 K0 K5 K0 M0 M1 D100", 15),
        ("DPULS K0 K1 K-100 K100 M0 M1 D100", 29),
    ] {
        let a = action(line);
        assert_eq!(a.words(), words);
        assert_eq!(
            a.header(),
            Some(match a.kind {
                PuKind::Configure => 0x0900,
                PuKind::Status => 0x0c00,
                PuKind::Pulse => 0xd700,
                PuKind::Relative => 0xd900,
                PuKind::Absolute => 0xdb00,
                PuKind::Home => 0xdd00,
                PuKind::Jog => 0xdf00,
                PuKind::Mpg => 0xe300,
                PuKind::Counter => 0xe500,
                PuKind::Input => 0xe800,
                PuKind::Limits => 0xeb00,
            })
        );
    }
    assert!(PuAction::parse("DPUDCNT", &[]).is_err());
}
#[test]
fn configured_virtual_module_applies_only_at_its_explicit_exchange_deadline() {
    let mut m = memory();
    let a = action("DPUCONF K0 K1 K9 K20000 K-1 K20000 K300000 K-200 K20000 M0 M1 D100");
    a.execute(&mut m, true, 1);
    assert!(!m.bit(dev("M0")));
    advance(&mut m, 1);
    let c = &m.pu_module.module(0).unwrap().axes[0].config;
    assert_eq!(
        (
            c.mode,
            c.start_hz,
            c.acceleration_ms,
            c.deceleration_ms,
            c.maximum_hz,
            c.z_count,
            c.offset
        ),
        (1, 10000, 0, 10000, 200000, -100, 10000)
    );
    assert!(m.bit(dev("M0")));
}
#[test]
fn external_exchange_is_not_an_automatic_successful_ack() {
    let mut m = memory();
    m.pu_module.module_mut(0).unwrap().exchange = PuExchange::External;
    let a = action("PUX K0 K0 K5 K0 M0 M1 D100");
    a.execute(&mut m, true, 1);
    advance(&mut m, 10);
    assert!(!m.bit(dev("M0")));
    let id = m.pu_module.outgoing().back().unwrap().id;
    m.pu_module.queue_reply(11, id, PuReply::Ack).unwrap();
    advance(&mut m, 11);
    assert!(m.bit(dev("M0")));
    assert_eq!(m.pu_module.module(0).unwrap().inputs[0].mode, 5);
}
#[test]
fn absent_module_and_silent_configured_module_are_distinct() {
    let mut m = memory();
    let a = action("PUX K1 K0 K5 K0 M0 M1 D100");
    a.execute(&mut m, true, 1);
    assert_eq!(m.word(dev("D100")) as u16, 0x1400);
    let a = action("PUX K0 K0 K5 K0 M2 M3 D102");
    m.pu_module.module_mut(0).unwrap().responding = false;
    a.execute(&mut m, true, 2);
    advance(&mut m, 100000);
    assert_eq!(m.word(dev("D102")) as u16, 0x1402);
    assert!(!m.bit(dev("M2")));
}
#[test]
fn train_signed_speed_and_count_magnitude_preserve_direction_at_completion() {
    let mut m = memory();
    let a = action("DPUPLS K0 K1 K-2 K-1000 M0 M1 D100");
    a.execute(&mut m, true, 1);
    advance(&mut m, 1);
    advance(&mut m, 2001);
    assert_eq!(m.pu_module.position(0, 1), Some(-2));
    assert!(m.bit(dev("M0")));
    assert!(m.pu_module.output(0, 1));
    assert!(!m.pu_module.output(0, 0));
    assert!(!m.bit(dev("Y1")));
}
#[test]
fn count_mode_defaults_and_period_capture_are_distinct_from_frequency() {
    let mut m = memory();
    let a = action("DPUCNT K0 K0 K100 X0 D10 D12 M0 D100");
    a.execute(&mut m, true, 1);
    advance(&mut m, 1);
    for (t, a, b) in [
        (1000, true, false),
        (2000, true, true),
        (3000, false, true),
        (4000, false, false),
    ] {
        m.pu_module.queue_encoder_edge(0, t, a, b).unwrap();
    }
    advance(&mut m, 100001);
    assert_eq!(m.double_word(dev("D10")), 4);
    assert_eq!(m.double_word(dev("D12")), 4);
    assert_eq!(m.pu_module.module(0).unwrap().encoder.mode, 4);
}
#[test]
fn config_edge_latches_and_abort_cancels_future_axis_motion() {
    let mut m = memory();
    let a = action("DPUCONF K0 K1 K1 K100 K0 K0 D20 K0 K0 M0 M1 D100");
    m.set_double_word(dev("D20"), 1000);
    a.execute(&mut m, true, 1);
    advance(&mut m, 1);
    m.set_double_word(dev("D20"), 2000);
    a.execute(&mut m, true, 1);
    assert_eq!(
        m.pu_module.module(0).unwrap().axes[0].config.maximum_hz,
        1000
    );
    let pulse = action("DPUPLS K0 K1 K0 K1000 M2 M3 D102");
    pulse.execute(&mut m, true, 2);
    advance(&mut m, 2);
    advance(&mut m, 10002);
    let pos = m.pu_module.position(0, 1);
    PuState::abort(&mut m);
    advance(&mut m, 100002);
    assert_eq!(m.pu_module.position(0, 1), pos);
    assert!(!m.pu_module.output(0, 0));
}
#[test]
fn fixed_slope_uses_maximum_not_requested_frequency() {
    let c = PuAxisConfig {
        start_hz: 100,
        maximum_hz: 100100,
        acceleration_ms: 1000,
        deceleration_ms: 1000,
        ..Default::default()
    };
    let mut a = PuAxis {
        config: c.clone(),
        busy: true,
        ..Default::default()
    };
    let mut run = AxisRun::new(RunMode::Ramp, false, None, 100.);
    run.ramp(&c, 10100.);
    run.advance(&mut a, 100000);
    assert!((a.speed_hz - 10100.).abs() < 1e-6);
    assert_eq!(a.position, 510);
}
#[test]
fn status_full_pair_rejects_model_boundary_without_partial_ram_write() {
    let mut m = memory();
    m.data_register_limit = Some(4999);
    m.set_word(dev("D4999"), 123);
    let a = action("PUSTAT K0 K1 X0 D4999 M0 M1 M2 D100");
    a.execute(&mut m, true, 1);
    advance(&mut m, 1);
    assert_eq!(m.word(dev("D4999")), 123);
    assert!(!m.execution_limitations.is_empty());
}

#[test]
fn status_zero_is_live_rising_and_does_not_repeat_while_held() {
    let mut m = memory();
    let a = action("PUSTAT K0 K1 X0 D10 M0 M1 M2 D100");
    m.pu_module.module_mut(0).unwrap().axes[0].position = 123;
    a.execute(&mut m, true, 1);
    advance(&mut m, 1);
    assert_eq!(m.double_word(dev("D10")), 123);
    m.set_input(dev("X0"), true);
    a.execute(&mut m, true, 1);
    assert_eq!(m.double_word(dev("D10")), 0);
    m.pu_module.module_mut(0).unwrap().axes[0].position = 45;
    a.execute(&mut m, true, 1);
    assert_eq!(m.double_word(dev("D10")), 45);
    m.set_input(dev("X0"), false);
    a.execute(&mut m, true, 1);
    m.set_input(dev("X0"), true);
    a.execute(&mut m, true, 1);
    assert_eq!(m.double_word(dev("D10")), 0);
}
#[test]
fn right_module_mapping_uses_physical_slot_and_m1183_holds_cpu_words() {
    let mut m = memory();
    m.pu_module.configure(2, VirtualPuModule::e2()).unwrap();
    {
        let module = m.pu_module.module_mut(2).unwrap();
        module.inputs[0].level = true;
        module.inputs[4].level = true;
        module.axes[0].busy = true;
        module.axes[0].state_negative = true;
        module.axes[1].state_done = true;
        module.axes[1].error = 0x1406;
    }
    PuState::service_controls(&mut m);
    assert_eq!(m.word(dev("D9920")), 17);
    let state = m.word(dev("D9921")) as u16;
    assert_eq!(state & 0x82, 0x82);
    assert_eq!(state & 0x1900, 0x1900);
    m.set_system(1183, true);
    m.set_word(dev("D9920"), 123);
    m.set_word(dev("D9921"), 456);
    m.pu_module.module_mut(2).unwrap().inputs[0].level = false;
    PuState::service_controls(&mut m);
    assert_eq!(m.word(dev("D9920")), 123);
    assert_eq!(m.word(dev("D9921")), 456);
    m.set_system(1183, false);
    PuState::service_controls(&mut m);
    assert_eq!(m.word(dev("D9920")), 16);
}
#[test]
fn mapping_completion_and_limit_bits_survive_until_next_motion_start() {
    let mut m = memory();
    let a = action("DPUPLS K0 K1 K1 K-1000 M0 M1 D100");
    a.execute(&mut m, true, 1);
    advance(&mut m, 1001);
    assert_eq!((m.word(dev("D9901")) as u16) & 0x88, 0x88);
    a.execute(&mut m, false, 1);
    a.execute(&mut m, true, 1);
    advance(&mut m, 1002);
    assert_eq!((m.word(dev("D9901")) as u16) & 8, 0);
    assert!(!m.pu_module.module(0).unwrap().axes[0].position_overflow);
}

#[test]
fn cpu_profile_and_restart_restore_documented_mapping_default() {
    for (model, firmware, disabled) in [
        (PuCpuModel::Es2, FirmwareVersion::new(4, 2), false),
        (PuCpuModel::Sa2, FirmwareVersion::new(3, 4), true),
        (PuCpuModel::Sx2, FirmwareVersion::new(3, 4), true),
        (PuCpuModel::Se, FirmwareVersion::new(2, 14), true),
        (PuCpuModel::Se2, FirmwareVersion::new(1, 0), true),
    ] {
        let mut sim = crate::plc::Simulator::default();
        PuState::set_profile(
            &mut sim.memory,
            PuProfile {
                model,
                firmware,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(sim.memory.system_bit(1183), disabled);
        sim.memory.set_system(1183, !disabled);
        sim.restart_preserving_file_store();
        assert_eq!(sim.memory.system_bit(1183), disabled);
    }
}
