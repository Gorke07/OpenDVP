use crate::plc::{self, Simulator};

fn d(s: &str) -> plc::Device {
    s.parse().unwrap()
}

#[test]
fn example_runs_pi_sampling_in_the_actual_vm_and_holds_when_disabled() {
    let p = plc::compile_il(include_str!("../examples/pid.il")).unwrap();
    assert!(p.requires_vm());
    assert!(crate::es2_encoder::encode(&p).unwrap_err().contains("PID"));
    let mut s = Simulator::default();
    s.memory.set_input(d("X0"), true);
    for scan in 0..6 {
        s.scan(&p, 20);
        assert!(
            s.memory.execution_limitations.is_empty(),
            "{:?}",
            s.memory.execution_limitations
        );
        // E=30, P=60, I raw adds E*0.1 at each100ms sample, Ki=0.5.
        assert_eq!(s.memory.word(d("D20")), if scan < 5 { 61 } else { 63 });
    }
    assert_eq!(s.memory.pid.owner_count(), 1);
    let i = s.memory.double_word(d("D110"));
    s.memory.set_input(d("X0"), false);
    s.memory.set_input(d("X1"), true);
    for _ in 0..10 {
        s.scan(&p, 20);
    }
    assert_eq!(s.memory.word(d("D1")), 21);
    assert_eq!(s.memory.word(d("D20")), 63);
    assert_eq!(s.memory.double_word(d("D110")), i);
}

#[test]
fn distinct_pid_sites_cannot_silently_alias_parameter_ownership() {
    let e = plc::compile_il("LD X0\nPID D0 D1 D100 D20\nDPID D2 D4 D119 D22\nEND").unwrap_err();
    assert_eq!(e.line, 3);
    assert!(e.message.contains("çakış"));
    assert!(plc::compile_il("LD X0\nPID D0 D1 D100 D20\nDPID D2 D4 D120 D22\nEND").is_ok());
}

#[test]
fn dpid_local_plan_and_display_preserve_full_output_width() {
    let p = plc::compile_il("LD X0\nDPID D0 D2 D100 D20\nEND").unwrap();
    let a = &p.networks[0].actions[0];
    assert_eq!(a.label(), "DPID D0 D2 D100 D20");
    let mut memory = plc::Memory::default();
    memory.set_double_word(d("D20"), 100_001);
    let plc::Action::Pid(pid) = a else {
        panic!("typed DPID action missing")
    };
    assert!(pid.display_value(&memory).contains("100001"));
    assert!(crate::es2_encoder::encode(&p).unwrap_err().contains("DPID"));
    assert!(plc::compile_il("LD X0\nPIDP D0 D1 D100 D20\nEND").is_err());
    assert!(plc::compile_il("LD X0\nDPID D9999 D0 D100 D20\nEND").is_err());
}
