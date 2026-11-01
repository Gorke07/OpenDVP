use super::*;
fn d(s: &str) -> Device {
    s.parse().unwrap()
}
#[test]
fn transitions_wait_for_end_snapshot_and_exit_cleanup_is_one_visit() {
    let p = compile("LD X1\nSET S0\nSTL S0\nOUT Y0\nLD X0\nSET S20\nSTL S20\nOUT Y1\nRET\nEND")
        .unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X1"), true);
    sim.scan(&p, 100);
    assert!(!sim.memory.bit(d("Y0")));
    assert!(sim.memory.bit(d("S0")));
    sim.memory.set_input(d("X1"), false);
    sim.scan(&p, 100);
    assert!(sim.memory.bit(d("Y0")));
    sim.memory.set_input(d("X0"), true);
    sim.scan(&p, 100);
    assert!(!sim.memory.bit(d("S0")));
    assert!(sim.memory.bit(d("S20")));
    assert!(sim.memory.bit(d("Y0")));
    assert!(!sim.memory.bit(d("Y1")));
    sim.scan(&p, 100);
    assert!(!sim.memory.bit(d("Y0")));
    assert!(sim.memory.bit(d("Y1")));
}
#[test]
fn real_nop_separates_static_stl_join_and_predecessors() {
    for (separator, keeps_s0) in [("", false), ("NOP\n", true)] {
        let p = compile(&format!(
            "LD X1\nSET S0\nSET S1\nSTL S0\n{separator}STL S1\nSET S20\nRET\nEND"
        ))
        .unwrap();
        let mut sim = Simulator::default();
        sim.memory.set_input(d("X1"), true);
        sim.scan(&p, 100);
        sim.memory.set_input(d("X1"), false);
        sim.scan(&p, 100);
        assert_eq!(sim.memory.bit(d("S0")), keeps_s0);
        assert!(!sim.memory.bit(d("S1")));
        assert!(sim.memory.bit(d("S20")));
    }
}
#[test]
fn running_flags_first_body_and_quiescent_endpoints_are_distinct() {
    let p = compile("LD M1000\nOUT Y0\nLD M1001\nOUT Y1\nLD M1002\nOUT Y2\nLD M1003\nOUT Y3\nEND")
        .unwrap();
    let mut sim = Simulator::default();
    sim.scan(&p, 100);
    assert_eq!(
        ["Y0", "Y1", "Y2", "Y3"].map(|x| sim.memory.bit(d(x))),
        [true, false, true, false]
    );
    assert_eq!(
        [1000, 1001, 1002, 1003].map(|n| sim.memory.system_bit(n)),
        [true, false, false, true]
    );
    sim.scan(&p, 100);
    assert_eq!(
        ["Y0", "Y1", "Y2", "Y3"].map(|x| sim.memory.bit(d(x))),
        [true, false, false, true]
    );
}
#[test]
fn atmr_projection_records_actions_before_and_after_the_gate() {
    let p = compile("LD X0\nOUT Y1\nATMR T0 K1\nOUT Y0\nEND").unwrap();
    let mut sim = Simulator::default();
    sim.memory.set_input(d("X0"), true);
    sim.scan(&p, 100);
    assert_eq!(sim.action_power[0], [true, true, false]);
    assert!(sim.memory.bit(d("Y1")));
    assert!(!sim.memory.bit(d("Y0")));
    sim.scan(&p, 100);
    assert_eq!(sim.action_power[0], [true, true, true]);
    assert!(sim.memory.bit(d("Y0")));
}
#[test]
fn documented_sixteen_predecessors_are_separate_from_installed_quota() {
    let mut memory = Memory::default();
    let mut control = crate::program_control::ControlState::default();
    memory.sfc.current.fill(true);
    for n in 20..36 {
        memory.set_bit(
            Device {
                area: Area::S,
                index: n,
            },
            true,
        );
        crate::sfc::SfcState::enter(
            &mut memory,
            &mut control,
            Device {
                area: Area::S,
                index: n,
            },
            n != 20,
        );
    }
    assert_eq!(memory.sfc.predecessors.len(), 16);
    assert!(!memory.system_bit(1048));
    crate::sfc::SfcState::transition(&mut memory, &mut control, d("S80"), true, true);
    assert!((20..36).all(|n| !memory.bit(Device {
        area: Area::S,
        index: n
    })));
    assert!(memory.bit(d("S80")));
}
