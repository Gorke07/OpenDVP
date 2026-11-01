//! Documented virtual DZRN/IRQ routing. No installed scheduler/hardware parity.
//! PDF651..659: DOG X4/X6 IRQ exclusion lasts until homing completion;
//! M1307 additionally reserves X5/X7. User masks/pending history remain owned
//! by InterruptState. EC5/SE2 extra DOG inputs have no generic X0..7 IRQ slot.
use crate::interrupt::{InterruptId, mask_flags};
use crate::motion_basic::{MotionAction, MotionBasicState};
use crate::plc::{Device, Memory};
use crate::pulse_output::{PulseModel, PulseState};
use crate::timing_io;
use std::collections::BTreeMap;

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn irq(raw: &str) -> InterruptId {
    raw.parse().unwrap()
}
fn memory() -> Memory {
    let mut m = Memory::default();
    PulseState::set_running(&mut m, true);
    let definitions = ["I001", "I400", "I401", "I501", "I600", "I601", "I701"]
        .into_iter()
        .enumerate()
        .map(|(i, name)| (irq(name), 100 + i as u16))
        .collect::<BTreeMap<_, _>>();
    m.interrupts.configure(definitions, 0).unwrap();
    m.interrupts.on_ei([false; 3]);
    m
}
fn home(dog: &str, output: &str) -> MotionAction {
    MotionAction::parse(
        "DZRN",
        &["K1000", "K100", dog, output]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>(),
    )
    .unwrap()
}
fn arm(m: &mut Memory, dog: &str, output: &str, pc: u16) -> MotionAction {
    let a = home(dog, output);
    a.execute(m, true, pc);
    assert!(m.motion_basic.suppresses_external_irq(d(dog)));
    assert!(
        m.execution_limitations.is_empty(),
        "{:?}",
        m.execution_limitations
    );
    a
}
fn advance(m: &mut Memory, at_us: u64) {
    let delta = at_us.checked_sub(m.timing_io.now_us).unwrap();
    timing_io::advance_by(m, delta);
    // The VM normally advances the interrupt event clock at dispatch points.
    // These source-level tests explicitly supply the same deterministic time.
    m.interrupts.advance_to(at_us).unwrap();
    assert!(
        m.execution_limitations.is_empty(),
        "{:?}",
        m.execution_limitations
    );
}
fn edge(m: &mut Memory, at_us: u64, input: &str, high: bool) {
    m.timing_io
        .enqueue_input_edge(at_us, d(input), high)
        .unwrap();
    advance(m, at_us);
}

#[test]
fn homing_dog_rising_and_terminal_falling_edges_do_not_enqueue_irq() {
    for (dog, output, rising, falling) in
        [("X4", "Y0", "I401", "I400"), ("X6", "Y2", "I601", "I600")]
    {
        let mut m = memory();
        arm(&mut m, dog, output, 1);
        edge(&mut m, 100, dog, true);
        assert!(!m.interrupts.pending().contains_key(&irq(rising)));
        assert!(m.motion_basic.suppresses_external_irq(d(dog)));
        edge(&mut m, 200, dog, false);
        // Classify the terminal edge using ownership BEFORE home_edge turns
        // the phase into Done. Otherwise it leaks into the falling handler.
        assert!(!m.interrupts.pending().contains_key(&irq(falling)));
        assert!(!m.motion_basic.suppresses_external_irq(d(dog)));
        edge(&mut m, 300, dog, true);
        assert_eq!(m.interrupts.pending()[&irq(rising)].at_us, 300);
    }
}

#[test]
fn left_limit_irq_exclusion_is_enabled_and_latched_by_m1307() {
    for (dog, output, limit, id) in [("X4", "Y0", "X5", "I501"), ("X6", "Y2", "X7", "I701")] {
        let mut enabled = memory();
        enabled.set_system(1307, true);
        arm(&mut enabled, dog, output, 1);
        assert!(enabled.motion_basic.suppresses_external_irq(d(limit)));
        enabled.set_system(1307, false);
        edge(&mut enabled, 100, limit, true);
        assert!(!enabled.interrupts.pending().contains_key(&irq(id)));

        let mut disabled = memory();
        arm(&mut disabled, dog, output, 1);
        assert!(!disabled.motion_basic.suppresses_external_irq(d(limit)));
        edge(&mut disabled, 100, limit, true);
        assert!(disabled.interrupts.pending().contains_key(&irq(id)));
    }
}

#[test]
fn homing_preserves_unrelated_irq_capture_and_existing_user_masked_pending() {
    let mut m = memory();
    m.set_system(1050, true);
    m.set_system(1054, true);
    m.interrupts.request(irq("I401"), 0).unwrap();
    let prior = m.interrupts.pending()[&irq("I401")];
    arm(&mut m, "X4", "Y0", 1);
    edge(&mut m, 100, "X4", true);
    edge(&mut m, 150, "X0", true);
    assert_eq!(m.interrupts.pending()[&irq("I401")], prior);
    assert_eq!(m.interrupts.pending()[&irq("I001")].at_us, 150);
    assert!(m.system_bit(1050) && m.system_bit(1054));
    let masks = mask_flags(&m);
    assert!(m.interrupts.dispatch(masks).is_none());
    edge(&mut m, 200, "X4", false);
    assert_eq!(m.interrupts.pending()[&irq("I401")], prior);
    assert!(m.system_bit(1050) && m.system_bit(1054));
    m.set_system(1050, false);
    let masks = mask_flags(&m);
    assert_eq!(m.interrupts.dispatch(masks).map(|v| v.0), Some(irq("I001")));
    m.interrupts.finish_handler().unwrap();
    assert_eq!(m.interrupts.pending()[&irq("I401")], prior);
}

#[test]
fn suppression_lasts_through_clear_pulse_and_releases_at_exact_done_deadline() {
    let mut m = memory();
    m.set_system(1346, true);
    arm(&mut m, "X4", "Y0", 1);
    edge(&mut m, 100, "X4", true);
    edge(&mut m, 200, "X4", false);
    assert!(m.physical_output(d("Y4")));
    assert!(m.motion_basic.suppresses_external_irq(d("X4")));
    assert!(!m.system_bit(1029));
    advance(&mut m, 20_199);
    assert!(m.motion_basic.suppresses_external_irq(d("X4")));
    // Deadline completion precedes same-timestamp physical input delivery.
    edge(&mut m, 20_200, "X4", true);
    assert!(!m.physical_output(d("Y4")));
    assert!(m.system_bit(1029));
    assert!(!m.motion_basic.suppresses_external_irq(d("X4")));
    assert_eq!(m.interrupts.pending()[&irq("I401")].at_us, 20_200);
}

#[test]
fn suppression_remains_during_z_search_until_final_home_event() {
    let mut m = memory();
    m.set_word(d("D1312"), 1);
    m.set_system(1307, true);
    arm(&mut m, "X4", "Y0", 1);
    edge(&mut m, 100, "X4", true);
    edge(&mut m, 200, "X4", false);
    assert!(m.motion_basic.suppresses_external_irq(d("X4")));
    assert!(m.motion_basic.suppresses_external_irq(d("X5")));
    edge(&mut m, 300, "X2", true);
    assert!(m.system_bit(1029));
    assert!(!m.motion_basic.suppresses_external_irq(d("X4")));
    assert!(!m.motion_basic.suppresses_external_irq(d("X5")));
}

#[test]
fn false_release_and_abort_restore_irq_without_rearming_or_mutating_masks() {
    for abort in [false, true] {
        let mut m = memory();
        m.set_system(1055, true);
        let a = arm(&mut m, "X4", "Y0", 1);
        if abort {
            MotionBasicState::abort(&mut m);
        } else {
            a.execute(&mut m, false, 1);
        }
        assert!(!m.motion_basic.suppresses_external_irq(d("X4")));
        assert!(m.system_bit(1055));
        edge(&mut m, 100, "X4", true);
        assert!(m.interrupts.pending().contains_key(&irq("I401")));
    }
}

#[test]
fn immediate_input_edit_uses_same_homing_irq_exclusion_as_queued_edges() {
    let mut m = memory();
    arm(&mut m, "X4", "Y0", 1);
    m.set_input(d("X4"), true);
    assert!(!m.interrupts.pending().contains_key(&irq("I401")));
    m.set_input(d("X4"), false);
    assert!(!m.interrupts.pending().contains_key(&irq("I400")));
    assert!(!m.motion_basic.suppresses_external_irq(d("X4")));
    m.set_input(d("X4"), true);
    assert!(m.interrupts.pending().contains_key(&irq("I401")));
}

#[test]
fn first_two_homing_channels_keep_irq_exclusion_on_qualified_cpu_profiles() {
    for model in [
        PulseModel::Es2,
        PulseModel::Ss2,
        PulseModel::Ec5,
        PulseModel::Se2,
    ] {
        let mut m = memory();
        m.pulse_output.model = model;
        arm(&mut m, "X6", "Y2", 1);
        assert!(!m.motion_basic.suppresses_external_irq(d("X0")));
        assert!(!m.motion_basic.suppresses_external_irq(d("M6")));
        edge(&mut m, 100, "X6", true);
        assert!(!m.interrupts.pending().contains_key(&irq("I601")));
    }
}

#[test]
fn compiled_ei_delay_dispatches_unrelated_and_post_home_edges_only() {
    use crate::plc::{ScanStatus, Simulator, compile_il};
    let p = compile_il("EI\nLD X20\nDZRN K1000 K100 X4 Y0\nDELAY K20\nFEND\nI001\nLD M1000\nINC D100\nIRET\nI400\nLD M1000\nINC D101\nIRET\nI401\nLD M1000\nINC D102\nIRET\nEND").unwrap();
    let mut s = Simulator::default();
    s.memory.set_input(d("X20"), true);
    for (at, input, high) in [
        (500, "X4", true),
        (750, "X0", true),
        (1000, "X4", false),
        (1500, "X4", true),
    ] {
        s.memory
            .timing_io
            .enqueue_input_edge(at, d(input), high)
            .unwrap();
    }
    s.scan(&p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(
        s.memory.execution_limitations.is_empty(),
        "{:?}",
        s.memory.execution_limitations
    );
    assert_eq!(s.memory.word(d("D100")), 1);
    assert_eq!(s.memory.word(d("D101")), 0);
    assert_eq!(s.memory.word(d("D102")), 1);
    assert!(s.memory.system_bit(1029));
    assert!(s.memory.interrupts.pending().is_empty());
}
