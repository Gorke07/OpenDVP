use crate::plc::{self, Simulator};

fn run(source: &str, scans: usize, setup: impl Fn(&mut Simulator)) -> Simulator {
    let program = plc::compile_st(source).unwrap_or_else(|e| panic!("{e:?}"));
    let mut sim = Simulator::default();
    setup(&mut sim);
    for _ in 0..scans {
        sim.scan(&program, 100);
    }
    sim
}

#[test]
fn function_block_instances_keep_separate_state() {
    let src = "FUNCTION_BLOCK Counter\nVAR_INPUT en : BOOL; step : INT; END_VAR\nVAR_OUTPUT total : INT; END_VAR\nIF en THEN total := total + step; END_IF;\nEND_FUNCTION_BLOCK\n\
PROGRAM Main\nVAR a : Counter; b : Counter; ra AT D20 : INT; rb AT D21 : INT; END_VAR\n\
a(en := TRUE, step := 2); b(en := X0, step := 5);\nra := a.total; rb := b.total;\nEND_PROGRAM";
    let sim = run(src, 3, |s| s.memory.set_input("X0".parse().unwrap(), true));
    assert_eq!(sim.memory.word("D20".parse().unwrap()), 6);
    assert_eq!(sim.memory.word("D21".parse().unwrap()), 15);
}

#[test]
fn function_block_can_own_timers_and_nest_blocks() {
    let src = "FUNCTION_BLOCK Delay\nVAR_INPUT go : BOOL; END_VAR\nVAR_OUTPUT done : BOOL; END_VAR\nVAR t : TON; END_VAR\n\
t(IN := go, PT := T#200ms); done := t.Q;\nEND_FUNCTION_BLOCK\n\
FUNCTION_BLOCK Outer\nVAR_INPUT go : BOOL; END_VAR\nVAR_OUTPUT ok : BOOL; END_VAR\nVAR d : Delay; END_VAR\n\
d(go := go); ok := d.done;\nEND_FUNCTION_BLOCK\n\
PROGRAM Main\nVAR o : Outer; y AT Y0 : BOOL; END_VAR\no(go := X0); y := o.ok;\nEND_PROGRAM";
    let early = run(src, 1, |s| s.memory.set_input("X0".parse().unwrap(), true));
    assert!(!early.memory.bit("Y0".parse().unwrap()));
    let late = run(src, 6, |s| s.memory.set_input("X0".parse().unwrap(), true));
    assert!(late.memory.bit("Y0".parse().unwrap()));
}

#[test]
fn functions_return_values_in_expressions() {
    let src = "FUNCTION Clamp : INT\nVAR_INPUT v : INT; hi : INT; END_VAR\nIF v > hi THEN Clamp := hi; ELSE Clamp := v; END_IF;\nEND_FUNCTION\n\
PROGRAM Main\nVAR r AT D20 : INT; s AT D21 : INT; END_VAR\nr := Clamp(50, 10) + 1;\ns := Clamp(v := 3, hi := 10);\nEND_PROGRAM";
    let sim = run(src, 1, |_| {});
    assert_eq!(sim.memory.word("D20".parse().unwrap()), 11);
    assert_eq!(sim.memory.word("D21".parse().unwrap()), 3);
}

#[test]
fn block_errors_are_explicit() {
    for src in [
        "FUNCTION_BLOCK F\nVAR_INPUT a : INT; END_VAR\nRETURN;\nEND_FUNCTION_BLOCK\nPROGRAM P\nVAR f : F; END_VAR\nf(a := 1);\nEND_PROGRAM",
        "FUNCTION_BLOCK F\nVAR_INPUT a : INT; END_VAR\nEND_FUNCTION_BLOCK\nPROGRAM P\nVAR f : F; END_VAR\nf(b := 1);\nEND_PROGRAM",
        "FUNCTION G : INT\nVAR_INPUT a : INT; END_VAR\nG := a;\nEND_FUNCTION\nPROGRAM P\nVAR n : INT; END_VAR\nWHILE G(n) < 3 DO n := n + 1; END_WHILE;\nEND_PROGRAM",
        "FUNCTION G : INT\nVAR_INPUT a : INT; END_VAR\nG := G(a);\nEND_FUNCTION\nPROGRAM P\nVAR n : INT; END_VAR\nn := G(1);\nEND_PROGRAM",
    ] {
        assert!(plc::compile_st(src).is_err(), "{src}");
    }
}

#[test]
fn readme_motor_example_runs() {
    let src = "FUNCTION_BLOCK Motor\nVAR_INPUT start, stop : BOOL; END_VAR\nVAR_OUTPUT run : BOOL; END_VAR\nrun := (start OR run) AND NOT stop;\nEND_FUNCTION_BLOCK\nPROGRAM Main\nVAR m1 : Motor; END_VAR\nm1(start := X0, stop := X1);  Y0 := m1.run;\nEND_PROGRAM";
    let program = plc::compile_st(src).unwrap();
    let mut sim = Simulator::default();
    for (x0, x1, y0) in [(true, false, true), (false, false, true), (false, true, false)] {
        sim.memory.set_input("X0".parse().unwrap(), x0);
        sim.memory.set_input("X1".parse().unwrap(), x1);
        sim.scan(&program, 100);
        assert_eq!(sim.memory.bit("Y0".parse().unwrap()), y0);
    }
}
