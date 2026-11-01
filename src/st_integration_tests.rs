use crate::plc::{self, Simulator};

#[test]
fn st_runs_in_the_shared_vm_and_preserves_variables_between_scans() {
    let source = "PROGRAM Counter\nVAR\n count AT D20 : INT := 7;\n enabled AT X0 : BOOL;\n lamp AT Y0 : BOOL;\nEND_VAR\nIF enabled THEN\n count := count + 2;\nEND_IF;\nlamp := count >= 11;\nEND_PROGRAM";
    let program = plc::compile_st(source).unwrap();
    assert!(program.source_is_st);
    assert!(program.requires_source_editing());
    assert!(
        !crate::es2_encoder::encode(&program)
            .unwrap()
            .words
            .is_empty()
    );
    let mut sim = Simulator::default();
    sim.memory.set_input("X0".parse().unwrap(), true);
    for (expected, lamp) in [(9, false), (11, true)] {
        sim.scan(&program, 100);
        assert_eq!(sim.memory.word("D20".parse().unwrap()), expected);
        assert_eq!(sim.memory.bit("Y0".parse().unwrap()), lamp);
    }
    sim.memory.set_input("X0".parse().unwrap(), false);
    sim.scan(&program, 100);
    assert_eq!(sim.memory.word("D20".parse().unwrap()), 11);
}

#[test]
fn language_detection_ignores_comment_text_and_explicit_il_stays_il() {
    let il = "; PROGRAM Example := ST text in an IL comment\nLD X0\nOUT Y0\nEND";
    assert!(!plc::compile(il).unwrap().source_is_st);
    let st = "(* LD X0 *)\nPROGRAM Test\nY0 := X0;\nEND_PROGRAM";
    assert!(plc::compile(st).unwrap().source_is_st);
    assert!(plc::compile_il(st).is_err());
}

#[test]
fn st_diagnostics_refer_to_the_original_source() {
    let diagnostic = plc::compile_st(
        "PROGRAM Bad\nVAR\n value : INT;\nEND_VAR\n\nvalue := unknown + 1;\nEND_PROGRAM",
    )
    .unwrap_err();
    assert_eq!(diagnostic.line, 6);
    assert!(plc::compile_st("PROGRAM Bad\nM1000 := FALSE;\nEND_PROGRAM").is_err());
}

#[test]
fn an_unbounded_st_loop_hits_the_existing_vm_budget() {
    let program =
        plc::compile_st("PROGRAM Endless\nWHILE TRUE DO\nD20 := D20 + 1;\nEND_WHILE;\nEND_PROGRAM")
            .unwrap();
    let mut sim = Simulator::default();
    sim.control.hard_step_limit = 200;
    sim.scan(&program, 100);
    assert!(matches!(sim.last_scan, plc::ScanStatus::Fault(_)));
    assert_eq!(
        sim.scans, 0,
        "a failed loop must not report a completed scan"
    );
    assert_eq!(sim.control.hard_step_limit, 200);
}
