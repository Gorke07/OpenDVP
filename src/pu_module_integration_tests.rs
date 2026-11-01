//! Independent PDF807..850 literal motion/input witnesses on explicit PU peers.
//! Module Y images are not CPU Y; genuine Delta editor codecs do not prove native capability.
use crate::file_memory::FirmwareVersion;
use crate::plc::{Device, Program, ScanStatus, Simulator, compile_il};
use crate::pu_module::{PuCpuModel, PuProfile, VirtualPuModule};
fn d(s: &str) -> Device {
    s.parse().unwrap()
}
// Error buffers use ordinary D2000+ storage: D1000 is the watchdog preset, so writing a successful zero there would fault the next scan.
const CONF: &str = "LD X20\nDPUCONF K0 K1 K1 K100 K0 K0 K100000 K0 K0 M10 M11 D2000\n";
fn sim() -> Simulator {
    let mut s = Simulator::default();
    s.memory.pu_module.profile = PuProfile {
        model: PuCpuModel::Es2,
        firmware: FirmwareVersion::new(4, 2),
        ..PuProfile::default()
    };
    s.memory
        .pu_module
        .configure(0, VirtualPuModule::e2())
        .unwrap();
    s.memory.set_input(d("X20"), true);
    s
}
fn scan(s: &mut Simulator, p: &Program) {
    s.scan(p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(
        s.memory.execution_limitations.is_empty(),
        "{:?}",
        s.memory.execution_limitations
    );
}
fn advance(s: &mut Simulator, us: u64) {
    crate::timing_io::advance_by(&mut s.memory, us);
}
fn position(s: &Simulator, axis: u8) -> i32 {
    s.memory.pu_module.position(0, axis).unwrap()
}
fn start(s: &mut Simulator, p: &Program) {
    scan(s, p);
    assert!(!s.memory.bit(d("M10")));
    advance(s, 1);
    assert!(s.memory.bit(d("M10")));
    s.memory.set_input(d("X20"), false);
    s.memory.set_input(d("X21"), true);
    scan(s, p);
    advance(s, 1);
}
#[test]
fn thirteen_documented_layouts_have_genuine_editor_headers_and_guarded_upload() {
    for (line, head, words) in [
        (
            "DPUCONF K0 K1 K1 K100 K0 K0 K100000 K0 K0 M10 M11 D2000",
            0x0900,
            49,
        ),
        ("PUSTAT K0 K1 X1 D20 M30 M31 M32 D202", 0x0c00, 17),
        ("DPUPLS K0 K1 K20 K1000 M30 M31 D202", 0xd700, 29),
        ("DPUDRI K0 K1 K20 K1000 M30 M31 D202", 0xd900, 29),
        ("DPUDRA K0 K1 K20 K1000 M30 M31 D202", 0xdb00, 29),
        ("DPUZRN K0 K1 K0 K1000 K100 M30 M31 D202", 0xdd00, 33),
        ("DPUJOG K0 K1 K1000 M30 M31 D202", 0xdf00, 25),
        ("DPUMPG K0 K1 K7 D20 D22 D24 D26 D28 M30 D202", 0xe300, 41),
        ("DPUCNT K0 K7 K100 X1 D20 D22 M30 D202", 0xe500, 33),
        ("PUX K0 K0 K5 K0 M30 M31 D202", 0xe800, 15),
        ("PUXP K0 K0 K5 K0 M30 M31 D202", 0xe810, 15),
        ("DPULS K0 K1 K-10 K10 M30 M31 D202", 0xeb00, 29),
        ("DPULSP K0 K1 K-10 K10 M30 M31 D202", 0xeb10, 29),
    ] {
        let q = compile_il(&format!("LD X0\n{line}\nMOV K7 D400\nEND")).unwrap();
        let l = crate::es2_encoder::local_execution_layout(&q).unwrap();
        assert_eq!(l.word_count, 1 + words + 5 + 1, "{line}");
        assert_eq!(l.action_pc, vec![1, 1 + words]);
        assert!(l.unencoded_ranges.is_empty());
        assert_eq!(l.known_word_at(1), Some(head));
        for pc in 1..1 + words {
            assert!(l.known_word_at(pc).is_some(), "{line} word{pc}");
        }
        assert!(crate::es2_encoder::encode(&q).is_err());
    }
}
#[test]
fn no_explicit_pu_module_never_returns_a_successful_configuration() {
    let q = compile_il(&format!("{CONF}END")).unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X20"), true);
    s.scan(&q, 0);
    advance(&mut s, 10000);
    assert!(!s.memory.bit(d("M10")));
    assert!(s.memory.bit(d("M11")) || !s.memory.execution_limitations.is_empty());
}
#[test]
fn config_rising_edge_latches_maximum_until_false_rearm_and_module_io_is_not_cpu_y() {
    let q=compile_il("LD X20\nDPUCONF K0 K1 K1 K100 K0 K0 D0 K0 K0 M10 M11 D2000\nLD X21\nDPUPLS K0 K1 K100 K10000 M30 M31 D202\nLD X30\nOUT Y0\nEND").unwrap();
    let mut s = sim();
    s.memory.set_double_word(d("D0"), 1000);
    s.memory.set_input(d("X30"), true);
    scan(&mut s, &q);
    advance(&mut s, 1);
    assert!(s.memory.bit(d("M10")));
    s.memory.set_double_word(d("D0"), 2000);
    s.memory.set_input(d("X21"), true);
    scan(&mut s, &q);
    advance(&mut s, 1);
    advance(&mut s, 10000);
    assert_eq!(position(&s, 1), 10);
    assert!(s.memory.bit(d("Y0")));
    assert!(s.memory.pulse_output.axis_owner(0).is_none());
}
#[test]
fn no_acceleration_pulses_finish_on_exact_count_and_negative_target_is_magnitude() {
    for (speed, end, direction) in [(1000, 20, false), (-1000, -20, true)] {
        let q = compile_il(&format!(
            "{CONF}LD X21\nDPUPLS K0 K1 K-20 K{speed} M30 M31 D202\nEND"
        ))
        .unwrap();
        let mut s = sim();
        start(&mut s, &q);
        advance(&mut s, 19999);
        assert_eq!(position(&s, 1), if end > 0 { 19 } else { -19 });
        assert!(!s.memory.bit(d("M30")));
        advance(&mut s, 1);
        assert_eq!(position(&s, 1), end);
        assert!(s.memory.bit(d("M30")));
        assert_eq!(s.memory.pu_module.output(0, 1), direction);
    }
}
#[test]
fn relative_then_absolute_uses_actual_module_position_and_signed_difference() {
    let q=compile_il(&format!("{CONF}LD X21\nDPUDRI K0 K1 K5 K-1000 M30 M31 D202\nLD X22\nDPUDRA K0 K1 K-3 K1000 M32 M33 D204\nEND")).unwrap();
    let mut s = sim();
    start(&mut s, &q);
    advance(&mut s, 5000);
    assert_eq!(position(&s, 1), 5);
    assert!(s.memory.bit(d("M30")));
    s.memory.set_input(d("X21"), false);
    s.memory.set_input(d("X22"), true);
    scan(&mut s, &q);
    advance(&mut s, 1);
    advance(&mut s, 8000);
    assert_eq!(position(&s, 1), -3);
    assert!(s.memory.bit(d("M32")));
}
#[test]
fn same_axis_is_exclusive_while_second_module_axis_remains_independent() {
    let q=compile_il("LD X20\nDPUCONF K0 K1 K1 K100 K0 K0 K100000 K0 K0 M10 M11 D2000\nDPUCONF K0 K2 K1 K100 K0 K0 K100000 K0 K0 M12 M13 D2002\nLD X21\nDPUPLS K0 K1 K10 K1000 M30 M31 D202\nDPUJOG K0 K1 K2000 M32 M33 D204\nDPUPLS K0 K2 K20 K1000 M34 M35 D206\nEND").unwrap();
    let mut s = sim();
    scan(&mut s, &q);
    advance(&mut s, 10);
    s.memory.set_input(d("X20"), false);
    s.memory.set_input(d("X21"), true);
    s.scan(&q, 0);
    advance(&mut s, 1);
    assert!(s.memory.bit(d("M33")));
    assert_eq!(s.memory.word(d("D204")) as u16, 0x1405);
    advance(&mut s, 10000);
    assert_eq!(position(&s, 1), 10);
    // The module serializes its two 1 us acknowledgements: axis 2 starts
    // one microsecond later and has not emitted its tenth pulse yet.
    assert_eq!(position(&s, 2), 9);
    advance(&mut s, 1);
    assert_eq!(position(&s, 2), 10);
    assert!(!s.memory.bit(d("M34")));
    advance(&mut s, 10000);
    assert_eq!(position(&s, 2), 20);
    assert!(s.memory.bit(d("M34")));
}
#[test]
fn false_mc_stops_pulse_output_without_completing_and_for_call_does_not_restart() {
    let q=compile_il(&format!("{CONF}LD X21\nFOR K3\nCALL P0\nNEXT\nFEND\nP0\nLD X21\nDPUPLS K0 K1 K20 K1000 M30 M31 D202\nSRET\nEND")).unwrap();
    let mut s = sim();
    start(&mut s, &q);
    advance(&mut s, 20000);
    assert_eq!(position(&s, 1), 20);
    assert!(s.memory.bit(d("M30")));
    let q = compile_il(&format!(
        "{CONF}LD X22\nMC N0\nLD X21\nDPUPLS K0 K1 K20 K1000 M30 M31 D202\nMCR N0\nEND"
    ))
    .unwrap();
    let mut s = sim();
    s.memory.set_input(d("X22"), true);
    start(&mut s, &q);
    advance(&mut s, 5000);
    assert_eq!(position(&s, 1), 5);
    s.memory.set_input(d("X22"), false);
    scan(&mut s, &q);
    advance(&mut s, 1);
    let stopped = position(&s, 1);
    advance(&mut s, 50000);
    assert_eq!(position(&s, 1), stopped);
    assert!(!s.memory.bit(d("M30")));
}
#[test]
fn jog_requires_zero_speed_before_direction_change() {
    let q = compile_il(&format!("{CONF}LD X21\nDPUJOG K0 K1 D0 M30 M31 D202\nEND")).unwrap();
    let mut s = sim();
    s.memory.set_double_word(d("D0"), 1000);
    start(&mut s, &q);
    advance(&mut s, 5000);
    assert_eq!(position(&s, 1), 5);
    s.memory.set_double_word(d("D0"), -1000);
    scan(&mut s, &q);
    advance(&mut s, 1);
    advance(&mut s, 5000);
    assert!(position(&s, 1) >= 5);
    assert!(!s.memory.pu_module.output(0, 1));
    s.memory.set_double_word(d("D0"), 0);
    scan(&mut s, &q);
    advance(&mut s, 1);
    s.memory.set_double_word(d("D0"), -1000);
    scan(&mut s, &q);
    advance(&mut s, 1);
    let at = position(&s, 1);
    advance(&mut s, 5000);
    assert_eq!(position(&s, 1), at - 5);
    assert!(s.memory.pu_module.output(0, 1));
}
#[test]
fn homing_255_sets_position_without_frequency_clamp_and_mode0_clears_it() {
    let q=compile_il(&format!("{CONF}LD X22\nDPUZRN K0 K1 K255 K12345 K100 M30 M31 D202\nLD X21\nDPUZRN K0 K1 K0 K0 K100 M32 M33 D204\nEND")).unwrap();
    let mut s = sim();
    scan(&mut s, &q);
    advance(&mut s, 1);
    s.memory.set_input(d("X20"), false);
    s.memory.set_input(d("X22"), true);
    scan(&mut s, &q);
    advance(&mut s, 1);
    assert_eq!(position(&s, 1), 12345);
    assert!(s.memory.bit(d("M30")));
    s.memory.set_input(d("X22"), false);
    s.memory.set_input(d("X21"), true);
    scan(&mut s, &q);
    advance(&mut s, 1);
    assert_eq!(position(&s, 1), 0);
    assert!(s.memory.bit(d("M32")));
}
#[test]
fn homing_requires_real_dog_leave_then_the_configured_z_edge() {
    let q=compile_il("LD X20\nDPUCONF K0 K1 K1 K100 K0 K0 K100000 K1 K0 M10 M11 D2000\nPUX K0 K0 K5 K0 M12 M13 D2002\nPUX K0 K1 K1 K0 M14 M15 D2004\nLD X21\nDPUZRN K0 K1 K3 K-1000 K100 M30 M31 D202\nEND").unwrap();
    let mut s = sim();
    scan(&mut s, &q);
    advance(&mut s, 10);
    s.memory.set_input(d("X20"), false);
    s.memory.set_input(d("X21"), true);
    scan(&mut s, &q);
    advance(&mut s, 1);
    let now = s.memory.timing_io.now_us;
    s.memory
        .pu_module
        .queue_input(0, 0, now + 1000, true)
        .unwrap();
    s.memory
        .pu_module
        .queue_input(0, 0, now + 2000, false)
        .unwrap();
    s.memory
        .pu_module
        .queue_input(0, 1, now + 3000, true)
        .unwrap();
    advance(&mut s, 2999);
    assert!(!s.memory.bit(d("M30")));
    advance(&mut s, 1);
    assert!(s.memory.bit(d("M30")));
}
#[test]
fn counter_reports_period_delta_not_hertz_and_zero_uses_an_edge() {
    let q = compile_il("LD X21\nDPUCNT K0 K7 K100 X1 D20 D22 M30 D202\nEND").unwrap();
    let mut s = sim();
    s.memory.set_input(d("X21"), true);
    scan(&mut s, &q);
    advance(&mut s, 1);
    let now = s.memory.timing_io.now_us;
    for j in 1..=4 {
        s.memory
            .pu_module
            .queue_encoder_edge(0, now + j * 1000, true, false)
            .unwrap();
        s.memory
            .pu_module
            .queue_encoder_edge(0, now + j * 1000 + 1, false, false)
            .unwrap();
    }
    advance(&mut s, 100000);
    scan(&mut s, &q);
    advance(&mut s, 1);
    assert_eq!(s.memory.double_word(d("D20")), 4);
    assert_eq!(s.memory.double_word(d("D22")), 4);
    s.memory.set_input(d("X1"), true);
    scan(&mut s, &q);
    advance(&mut s, 1);
    assert_eq!(s.memory.double_word(d("D20")), 0);
}
#[test]
fn mpg_rate_half_emits_only_two_pulses_from_four_actual_inputs() {
    let q = compile_il(&format!(
        "{CONF}LD X21\nDPUMPG K0 K1 K7 D20 D22 D24 D26 D28 M30 D202\nEND"
    ))
    .unwrap();
    let mut s = sim();
    s.memory.set_double_word(d("D24"), 0.5f32.to_bits() as i32);
    start(&mut s, &q);
    let now = s.memory.timing_io.now_us;
    for j in 1..=4 {
        s.memory
            .pu_module
            .queue_encoder_edge(0, now + j * 1000, true, false)
            .unwrap();
        s.memory
            .pu_module
            .queue_encoder_edge(0, now + j * 1000 + 1, false, false)
            .unwrap();
    }
    advance(&mut s, 100000);
    scan(&mut s, &q);
    advance(&mut s, 1);
    assert_eq!(s.memory.double_word(d("D20")), 4);
    assert_eq!(s.memory.double_word(d("D26")), 2);
    assert_eq!(position(&s, 1), 2);
    advance(&mut s, 100000);
    assert_eq!(position(&s, 1), 2);
}

#[test]
fn mpg_and_counter_cannot_both_take_the_same_module_encoder() {
    let q=compile_il(&format!("{CONF}LD X21\nDPUCNT K0 K7 K100 X1 D60 D62 M30 D200\nDPUMPG K0 K1 K7 D20 D22 D24 D26 D28 M31 D202\nEND")).unwrap();
    let mut s = sim();
    s.memory.set_double_word(d("D24"), 0.5f32.to_bits() as i32);
    scan(&mut s, &q);
    advance(&mut s, 1);
    s.memory.set_input(d("X20"), false);
    s.memory.set_input(d("X21"), true);
    s.scan(&q, 0);
    advance(&mut s, 1);
    assert!(s.memory.bit(d("M31")) || !s.memory.execution_limitations.is_empty());
    let now = s.memory.timing_io.now_us;
    for j in 1..=4 {
        s.memory
            .pu_module
            .queue_encoder_edge(0, now + j * 1000, true, false)
            .unwrap();
        s.memory
            .pu_module
            .queue_encoder_edge(0, now + j * 1000 + 1, false, false)
            .unwrap();
    }
    advance(&mut s, 100000);
    s.scan(&q, 0);
    advance(&mut s, 1);
    assert_eq!(s.memory.double_word(d("D60")), 4);
    assert_eq!(position(&s, 1), 0);
}
#[test]
fn pulse_config_forms_apply_limits_and_hardware_edges_without_false_completion() {
    for hardware in [false, true] {
        let bound = if hardware { 0 } else { 2 };
        let q=compile_il(&format!("{CONF}LD X20\nPUXP K0 K0 K13 K0 M12 M13 D2002\nDPULSP K0 K1 K0 K{bound} M14 M15 D2004\nLD X21\nDPUPLS K0 K1 K20 K1000 M30 M31 D202\nEND")).unwrap();
        let mut s = sim();
        scan(&mut s, &q);
        advance(&mut s, 10);
        s.memory.set_input(d("X20"), false);
        s.memory.set_input(d("X21"), true);
        scan(&mut s, &q);
        advance(&mut s, 1);
        if hardware {
            let at = s.memory.timing_io.now_us + 2500;
            s.memory.pu_module.queue_input(0, 0, at, true).unwrap();
        }
        advance(&mut s, 10000);
        assert_eq!(position(&s, 1), 2);
        assert!(s.memory.bit(d("M31")));
        assert_eq!(s.memory.word(d("D202")) as u16, 0x1406);
        assert!(!s.memory.bit(d("M30")));
    }
}
#[test]
fn implicit_status_pair_outside_model_range_cannot_partially_overwrite_ram() {
    let q = compile_il(&format!(
        "{CONF}LD X21\nPUSTAT K0 K1 X1 D4999 M30 M31 M32 D202\nEND"
    ))
    .unwrap();
    let mut s = sim();
    s.memory.data_register_limit = Some(4999);
    s.memory.set_word(d("D4999"), 321);
    scan(&mut s, &q);
    advance(&mut s, 1);
    s.memory.set_input(d("X20"), false);
    s.memory.set_input(d("X21"), true);
    s.scan(&q, 0);
    advance(&mut s, 10);
    assert_eq!(s.memory.word(d("D4999")), 321);
    assert!(s.memory.bit(d("M32")) || !s.memory.execution_limitations.is_empty());
    assert_eq!(position(&s, 1), 0);
}
