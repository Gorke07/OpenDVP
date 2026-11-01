use super::*;
use crate::plc::{Device, Simulator, compile_il};

fn run(source: &str, scans: usize) -> (LoweredSt, Simulator) {
    let lowered = lower(source).unwrap();
    let p = compile_il(&lowered.il).unwrap_or_else(|e| panic!("{e}\n{}", lowered.il));
    let mut s = Simulator::default();
    for _ in 0..scans {
        s.scan(&p, 10);
    }
    assert_eq!(
        s.scans, scans as u64,
        "scan did not complete: {:?}",
        s.last_scan
    );
    assert!(
        s.memory.execution_limitations.is_empty(),
        "{:?}",
        s.memory.execution_limitations
    );
    (lowered, s)
}
fn word(s: &Simulator, d: &str) -> i16 {
    s.memory.word(d.parse::<Device>().unwrap())
}
fn bit(s: &Simulator, d: &str) -> bool {
    s.memory.bit(d.parse::<Device>().unwrap())
}

#[test]
fn arithmetic_precedence_and_persistent_first_scan_initializer() {
    let (_, s) = run(
        "PROGRAM Main\nVAR n AT D0 : INT := 2; END_VAR\nn := n + 3 * 4; END_PROGRAM",
        3,
    );
    assert_eq!(word(&s, "D0"), 38);
}
#[test]
fn bool_assignment_writes_false_and_if_skip_holds() {
    let (_, s) = run(
        "VAR a AT Y0:BOOL; b AT Y1:BOOL := TRUE; END_VAR a := FALSE; IF FALSE THEN b := FALSE; END_IF;",
        1,
    );
    assert!(!bit(&s, "Y0"));
    assert!(bit(&s, "Y1"));
}
#[test]
fn elsif_case_ranges_and_bit_logic() {
    let (_, s) = run(
        "VAR n AT D0:INT:=4; v AT D1:INT; b AT Y0:BOOL; END_VAR IF n=1 THEN v:=1; ELSIF n=4 THEN v:=2; ELSE v:=3; END_IF; CASE n OF 1,2:v:=10; 3..5:v:=20; ELSE v:=30; END_CASE; b := NOT FALSE AND TRUE XOR FALSE OR FALSE;",
        1,
    );
    assert_eq!(word(&s, "D1"), 20);
    assert!(bit(&s, "Y0"));
}
#[test]
fn unsigned_bitstring_comparisons_and_masking() {
    let (_, s) = run(
        "VAR w AT D0:WORD:=16#FFFF; d AT D2:DWORD:=16#FFFFFFFF; b AT Y0:BOOL; c AT Y1:BOOL; END_VAR b:=w>16#7FFF; c:=d>16#7FFFFFFF; w:=w AND 16#00FF; d:=NOT d;",
        1,
    );
    assert!(bit(&s, "Y0") && bit(&s, "Y1"));
    assert_eq!(word(&s, "D0"), 255);
    assert_eq!(s.memory.double_word("D2".parse().unwrap()), 0);
}
#[test]
fn signed_native_arithmetic_wrap_division_modulo_and_extension() {
    let (_, s) = run(
        "VAR a AT D0:INT:=32767; q AT D1:INT; r AT D2:INT; d AT D4:DINT; END_VAR a:=a+1; q:=-23/7; r:=-23 MOD 7; d:=a;",
        1,
    );
    assert_eq!(word(&s, "D0"), i16::MIN);
    assert_eq!(word(&s, "D1"), -3);
    assert_eq!(word(&s, "D2"), -2);
    assert_eq!(s.memory.double_word("D4".parse().unwrap()), -32768);
}
#[test]
fn captured_for_bounds_negative_steps_exit_and_loops() {
    let (_, s) = run(
        "VAR i:INT; n:INT:=3; count AT D0:INT; END_VAR FOR i:=1 TO n DO n:=1; count:=count+1; END_FOR; FOR i:=5 TO 1 BY -2 DO count:=count+1; END_FOR; WHILE count<8 DO count:=count+1; END_WHILE; REPEAT count:=count+1; IF count=10 THEN EXIT; END_IF; UNTIL count>20 END_REPEAT; RETURN; count:=99;",
        1,
    );
    assert_eq!(word(&s, "D0"), 10);
}
#[test]
fn for_endpoint_wrap_terminates_with_captured_count() {
    let (_, s) = run(
        "VAR i AT D0:INT; n AT D1:INT; END_VAR FOR i:=32766 TO 32767 DO n:=n+1; END_FOR;",
        1,
    );
    assert_eq!(word(&s, "D0"), -32768);
    assert_eq!(word(&s, "D1"), 2);
}
#[test]
fn real_uses_binary32_target_instructions() {
    let (l, s) = run(
        "VAR x AT D0:REAL:=1.5; y AT D2:REAL; b AT Y0:BOOL; END_VAR y:=(x+2.5)*2.0; b:=y=8.0;",
        1,
    );
    assert!(l.il.contains("DADDR"));
    assert!(l.il.contains("DMULR"));
    assert_eq!(
        s.memory.double_word("D2".parse().unwrap()) as u32,
        8.0f32.to_bits()
    );
    assert!(bit(&s, "Y0"));
}
#[test]
fn allocation_excludes_explicit_addresses_and_wide_pairs() {
    let l=lower("VAR a:INT; b:DINT; c:BOOL; ext AT D4000:DINT; extbit AT M800:BOOL; END_VAR D4002:=1; a:=2; b:=3; c:=TRUE;").unwrap();
    let a = l.symbols.iter().find(|s| s.name == "A").unwrap();
    assert_ne!(a.address, "D4000");
    assert_ne!(a.address, "D4001");
    assert_ne!(a.address, "D4002");
    assert_ne!(a.address, "D4003");
    assert_ne!(
        l.symbols.iter().find(|s| s.name == "C").unwrap().address,
        "M800"
    );
}
#[test]
fn percent_mapping_and_read_only_diagnostics() {
    let l=lower("VAR input AT %IX1.7:BOOL; output AT %QX1.7:BOOL; n AT %MW20:INT; END_VAR output:=input; n:=1;").unwrap();
    assert_eq!(l.symbols[0].address, "X17");
    assert_eq!(l.symbols[1].address, "Y17");
    assert_eq!(l.symbols[2].address, "D20");
    for addr in [
        "M1000", "M1001", "M1002", "M1003", "M1011", "M1012", "M1013", "M1014", "X0",
    ] {
        assert!(lower(&format!("{addr}:=TRUE;")).is_err(), "{addr}");
    }
}
#[test]
fn comments_strings_errors_and_source_mapping() {
    assert!(!is_st_source("// PROGRAM Main\nLD X0\nOUT Y0\nEND"));
    assert!(is_st_source(
        "(* VAR fake (* nested *) *)\n VAR a:INT; END_VAR a:=1;"
    ));
    assert!(!is_st_source("(* PROGRAM *)\nLD M1000\nMOV K1 D0\nEND"));
    let l = lower("// heading\nVAR n:INT; END_VAR\nn := 1+2;\n").unwrap();
    assert_eq!(l.il.lines().count(), l.line_map.len());
    assert!(l.line_map.iter().all(|n| *n == 3));
    let e = lower("\n\nD0 := 'literal // not comment';").unwrap_err();
    assert_eq!(e.line, 3);
    assert!(e.message.contains("sütun"));
}
#[test]
fn unsupported_and_exhaustion_fail_clearly_without_ignored_statements() {
    for src in [
        "FUNCTION_BLOCK F END_FUNCTION_BLOCK",
        "VAR t:TIME; END_VAR",
        "VAR x:INT; END_VAR x:=UNKNOWN(1);",
        "EXIT;",
        "FOR D0:=1 TO 2 BY 0 DO END_FOR;",
        "PROGRAM P VAR n:INT:=",
        "IF TRUE THEN",
        "VAR i:INT; END_VAR FOR i:=1 TO 2 DO i:=9; END_FOR;",
    ] {
        assert!(lower(src).is_err(), "{src}");
    }
    let huge = format!("D0:={};", vec!["1"; 2000].join("+"));
    assert!(lower(&huge).unwrap_err().message.contains("128"));
    let nested = format!(
        "{}D0:=1;{}",
        "WHILE TRUE DO ".repeat(200),
        " END_WHILE;".repeat(200)
    );
    assert!(lower(&nested).is_err());
}

#[test]
fn aliases_cannot_mutate_for_control_and_case_hex_labels_are_canonical() {
    for source in [
        "VAR i AT D0:INT; END_VAR FOR i:=1 TO 3 DO D0:=0; END_FOR;",
        "VAR i AT D1:INT; wide AT D0:DINT; END_VAR FOR i:=1 TO 3 DO wide:=0; END_FOR;",
        "VAR i AT D0:INT; j AT D0:INT; END_VAR FOR i:=1 TO 3 DO FOR j:=1 TO 2 DO END_FOR; END_FOR;",
        "CASE D0 OF -1:D1:=1; 16#FFFF:D1:=2; END_CASE;",
        "CASE D0 OF 0..16#FFFF:D1:=1; END_CASE;",
    ] {
        assert!(lower(source).is_err(), "{source}");
    }
    let (_, s) = run(
        "D0:=-1; CASE D0 OF 16#8000..16#FFFF:D1:=7; ELSE D1:=9; END_CASE;",
        1,
    );
    assert_eq!(word(&s, "D1"), 7);
}

#[test]
fn invalid_st_tail_still_selects_the_st_frontend() {
    for s in [
        "PROGRAM Main\nD0 := [1];\nEND_PROGRAM",
        "(* lead *) VAR x:INT; END_VAR (* unclosed",
        "D0 (* comment *) := [1];",
    ] {
        assert!(is_st_source(s));
        assert!(lower(s).is_err());
    }
    assert!(!is_st_source("'PROGRAM Main'"));
    assert!(!is_st_source("// D0 := 1;\nLD X0\nEND"));
}

#[test]
fn wrapping_add_subtract_use_all_half_word_carries_and_signed_edges() {
    for (a, b) in [
        (i32::MAX, 1),
        (i32::MIN, -1),
        (-1, 1),
        (65535, 1),
        (-65536, 1),
        (0, i32::MIN),
        (i32::MAX, i32::MIN),
    ] {
        let (_, s) = run(
            &format!(
                "VAR a AT D0:DINT; b AT D2:DINT; sum AT D4:DINT; diff AT D6:DINT; END_VAR a:={a}; b:={b}; sum:=a+b; diff:=a-b;"
            ),
            1,
        );
        assert_eq!(
            s.memory.double_word("D4".parse().unwrap()),
            a.wrapping_add(b),
            "{a}+{b}"
        );
        assert_eq!(
            s.memory.double_word("D6".parse().unwrap()),
            a.wrapping_sub(b),
            "{a}-{b}"
        );
    }
}

#[test]
fn integer_zero_divisor_raises_target_error_and_holds_public_results() {
    for op in ["/", "MOD"] {
        let (_, s) = run(
            &format!(
                "VAR out AT D0:INT:=77; later AT D1:INT:=88; END_VAR out:=9 {op} 0; later:=99;"
            ),
            1,
        );
        assert_eq!(word(&s, "D0"), 77);
        assert_eq!(word(&s, "D1"), 88);
        assert!(s.memory.system_bit(1067));
        assert_eq!(s.memory.word("D1067".parse().unwrap()) as u16, 0xe19);
    }
}

#[test]
fn real_invalid_operand_holds_results_instead_of_committing_stale_scratch() {
    for bits in [0u32, 0x8000_0000, 1, 0x7fc0_0001] {
        let l = lower(
            "VAR src AT D0:REAL; out AT D2:REAL; later AT D4:INT; END_VAR out:=2.0/src; later:=99;",
        )
        .unwrap();
        let p = compile_il(&l.il).unwrap();
        let mut s = Simulator::default();
        s.memory.set_double_word("D0".parse().unwrap(), bits as i32);
        s.memory
            .set_double_word("D2".parse().unwrap(), 77f32.to_bits() as i32);
        s.memory.set_word("D4".parse().unwrap(), 88);
        s.scan(&p, 10);
        assert_eq!(
            s.memory.double_word("D2".parse().unwrap()) as u32,
            77f32.to_bits()
        );
        assert_eq!(word(&s, "D4"), 88);
        assert!(s.memory.system_bit(1067));
        assert_eq!(s.scans, 1);
    }
    let l = lower("VAR src AT D0:REAL; out AT D2:REAL; END_VAR out:=src+1.0;").unwrap();
    let p = compile_il(&l.il).unwrap();
    let mut s = Simulator::default();
    s.memory
        .set_double_word("D0".parse().unwrap(), 0x7fc0_0001u32 as i32);
    s.memory
        .set_double_word("D2".parse().unwrap(), 77f32.to_bits() as i32);
    s.scan(&p, 10);
    assert_eq!(
        s.memory.double_word("D2".parse().unwrap()) as u32,
        77f32.to_bits()
    );
    assert!(s.memory.system_bit(1067));
}
