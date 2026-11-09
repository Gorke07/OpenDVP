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
        "FUNCTION_BLOCK F",
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

fn run_with(source: &str, plan: &[(usize, &str, bool)], scans: usize) -> Simulator {
    let lowered = lower(source).unwrap();
    let p = compile_il(&lowered.il).unwrap_or_else(|e| panic!("{e}\n{}", lowered.il));
    let mut s = Simulator::default();
    for n in 0..scans {
        for (at, d, v) in plan {
            if *at == n {
                s.memory.set_input(d.parse::<Device>().unwrap(), *v);
            }
        }
        s.scan(&p, 10);
    }
    assert!(
        s.memory.execution_limitations.is_empty(),
        "{:?}",
        s.memory.execution_limitations
    );
    s
}
fn dword_at(s: &Simulator, d: &str) -> i32 {
    let base = d.parse::<Device>().unwrap();
    let hi = Device {
        area: base.area,
        index: base.index + 1,
    };
    (i32::from(s.memory.word(hi)) << 16) | i32::from(s.memory.word(base) as u16)
}

#[test]
fn numeric_conversions_between_int_dint_word_and_real() {
    let (_, s) = run(
        "VAR i AT D0:INT:=-7; r AT D2:REAL; d AT D4:DINT:=70000; r2 AT D6:REAL; back AT D8:INT; low AT D10:INT; w AT D11:WORD:=16#FFFF; wd AT D12:DINT; END_VAR
         r := INT_TO_REAL(i); r2 := DINT_TO_REAL(d); back := REAL_TO_INT(r); low := DINT_TO_INT(d); wd := WORD_TO_DINT(w);",
        1,
    );
    assert_eq!(f32::from_bits(dword_at(&s, "D2") as u32), -7.0);
    assert_eq!(f32::from_bits(dword_at(&s, "D6") as u32), 70000.0);
    assert_eq!(word(&s, "D8"), -7);
    assert_eq!(word(&s, "D10"), 70000_i32 as i16);
    assert_eq!(dword_at(&s, "D12"), 65535);
}
#[test]
fn abs_min_max_limit_sel_and_sqrt() {
    let (_, s) = run(
        "VAR a AT D0:INT:=-5; b AT D1:INT; c AT D2:INT; d AT D3:INT; e AT D4:INT; f AT D5:INT; x AT D6:REAL:=9.0; y AT D8:REAL; z AT D10:REAL; g AT D12:DINT:=-100000; h AT D14:DINT; k AT Y0:BOOL:=TRUE; END_VAR
         b := ABS(a); c := MIN(a, 3, 9); d := MAX(a, 3, 9); e := LIMIT(0, a, 10); f := SEL(k, 1, 2);
         y := SQRT(x); z := ABS(-2.5); h := ABS(g);",
        1,
    );
    assert_eq!(word(&s, "D1"), 5);
    assert_eq!(word(&s, "D2"), -5);
    assert_eq!(word(&s, "D3"), 9);
    assert_eq!(word(&s, "D4"), 0);
    assert_eq!(word(&s, "D5"), 2);
    assert_eq!(f32::from_bits(dword_at(&s, "D8") as u32), 3.0);
    assert_eq!(f32::from_bits(dword_at(&s, "D10") as u32), 2.5);
    assert_eq!(dword_at(&s, "D14"), 100000);
}
#[test]
fn ton_delays_and_resets() {
    let src =
        "VAR t : TON; run AT X0:BOOL; q AT Y0:BOOL; END_VAR t(IN := run, PT := T#500ms); q := t.Q;";
    // 10 ms scans: 500 ms needs about 50 scans after X0 rises.
    let s = run_with(src, &[(0, "X0", true)], 30);
    assert!(!bit(&s, "Y0"), "output must wait for PT");
    let s = run_with(src, &[(0, "X0", true)], 70);
    assert!(bit(&s, "Y0"));
    let s = run_with(src, &[(0, "X0", true), (70, "X0", false)], 72);
    assert!(!bit(&s, "Y0"));
}
#[test]
fn tof_holds_output_after_input_falls() {
    let src =
        "VAR t : TOF; run AT X0:BOOL; q AT Y0:BOOL; END_VAR t(IN := run, PT := T#500ms); q := t.Q;";
    let s = run_with(src, &[(0, "X0", true)], 5);
    assert!(bit(&s, "Y0"));
    let s = run_with(src, &[(0, "X0", true), (5, "X0", false)], 30);
    assert!(bit(&s, "Y0"), "still held 250 ms after IN fell");
    let s = run_with(src, &[(0, "X0", true), (5, "X0", false)], 80);
    assert!(!bit(&s, "Y0"), "released after PT");
}
#[test]
fn tp_makes_a_fixed_pulse() {
    let src =
        "VAR t : TP; run AT X0:BOOL; q AT Y0:BOOL; END_VAR t(IN := run, PT := T#500ms); q := t.Q;";
    let s = run_with(src, &[(2, "X0", true)], 20);
    assert!(bit(&s, "Y0"));
    // input still held, pulse ended: no new pulse without a new edge
    let s = run_with(src, &[(2, "X0", true)], 80);
    assert!(!bit(&s, "Y0"));
    let s = run_with(
        src,
        &[(2, "X0", true), (30, "X0", false), (31, "X0", true)],
        40,
    );
    assert!(
        bit(&s, "Y0"),
        "ignored retrigger keeps the original pulse running"
    );
}
#[test]
fn ctu_counts_rising_edges_and_resets() {
    let src = "VAR c : CTU; up AT X0:BOOL; clr AT X1:BOOL; q AT Y0:BOOL; n AT D0:INT; END_VAR c(CU := up, R := clr, PV := 3); q := c.Q; n := c.CV;";
    let mut plan = vec![];
    for k in 0..3 {
        plan.push((2 + 4 * k, "X0", true));
        plan.push((4 + 4 * k, "X0", false));
    }
    let s = run_with(src, &plan, 20);
    assert_eq!(word(&s, "D0"), 3);
    assert!(bit(&s, "Y0"));
    plan.push((21, "X1", true));
    let s = run_with(src, &plan, 24);
    assert_eq!(word(&s, "D0"), 0);
    assert!(!bit(&s, "Y0"));
}
#[test]
fn ctd_counts_down_from_the_loaded_preset() {
    let src = "VAR c : CTD; dn AT X0:BOOL; ld AT X1:BOOL; q AT Y0:BOOL; n AT D0:INT; END_VAR c(CD := dn, LD := ld, PV := 3); q := c.Q; n := c.CV;";
    let mut plan = vec![(1, "X1", true), (3, "X1", false)];
    let s = run_with(src, &plan, 6);
    assert_eq!(word(&s, "D0"), 3);
    assert!(!bit(&s, "Y0"));
    for k in 0..3 {
        plan.push((8 + 4 * k, "X0", true));
        plan.push((10 + 4 * k, "X0", false));
    }
    let s = run_with(src, &plan, 24);
    assert_eq!(word(&s, "D0"), 0);
    assert!(bit(&s, "Y0"));
}
#[test]
fn function_block_and_function_misuse_is_rejected() {
    for (source, needle) in [
        (
            "VAR t : TON; END_VAR t(IN := TRUE, PT := T#150ms);",
            "100 ms",
        ),
        (
            "VAR t : TON; r : REAL; END_VAR t(IN := TRUE, PT := r);",
            "PT bir INT",
        ),
        (
            "VAR c : CTU; d : DINT; END_VAR c(CU := TRUE, PV := d);",
            "PV bir INT",
        ),
        ("VAR t : TON; END_VAR t(IN := TRUE);", "PT"),
        (
            "VAR t : TON; b : BOOL; END_VAR b := t(IN := TRUE, PT := T#1s);",
            "fonksiyon bloğudur",
        ),
        ("VAR t : TON; END_VAR t.Q := TRUE;", "salt okunur"),
        ("VAR t : TON; b : BOOL; END_VAR b := t.CV;", "yok"),
        ("VAR n : INT; END_VAR n := FOO(1);", "Bilinmeyen fonksiyon"),
        ("VAR n : INT; r : REAL; END_VAR n := r;", "dönüşümü"),
        ("VAR n : INT; END_VAR n := SQRT(4);", "REAL"),
        ("VAR a AT T5 : TON; b AT T5 : TON; END_VAR", "kullanılıyor"),
    ] {
        let err = lower(source).expect_err(source).message;
        assert!(err.contains(needle), "{source}\n  -> {err}");
    }
}
#[test]
fn timer_instances_take_distinct_devices_and_can_be_pinned() {
    let lowered = lower(
        "VAR a : TON; b : TOF; c AT T7 : TON; k : CTU; END_VAR a(IN := TRUE, PT := T#1s); b(IN := TRUE, PT := T#1s); c(IN := TRUE, PT := T#1s); k(CU := TRUE, PV := 5);",
    )
    .unwrap();
    let addr = |n: &str| {
        lowered
            .symbols
            .iter()
            .find(|s| s.name == n)
            .unwrap()
            .address
            .clone()
    };
    assert_eq!(addr("A"), "T126");
    assert_eq!(addr("B"), "T125");
    assert_eq!(addr("C"), "T7");
    assert_eq!(addr("K"), "C111");
    assert!(lowered.il.contains("TMR T126 K10"));
}
#[test]
fn real_to_int_rounding_follows_the_target() {
    let (_, s) = run(
        "VAR a AT D0:INT; b AT D1:INT; c AT D2:DINT; r AT D4:REAL := 2.7; q AT D6:REAL := -2.7; END_VAR a := REAL_TO_INT(r); b := REAL_TO_INT(q); c := REAL_TO_DINT(r);",
        1,
    );
    // The target truncates toward zero.
    assert_eq!(
        (word(&s, "D0"), word(&s, "D1"), dword_at(&s, "D2")),
        (2, -2, 2)
    );
}
#[test]
fn timers_and_counters_example_compiles_and_runs() {
    let source = include_str!("../../examples/st-timers-counters.st");
    assert!(is_st_source(source));
    let s = run_with(source, &[(0, "X0", true)], 260);
    assert!(bit(&s, "Y0"), "2 s on-delay elapsed");
    assert_eq!(word(&s, "D4"), 20, "25.0 clamped to 20");
}

#[test]
fn variable_preset_drives_timers_and_counters() {
    let src = "VAR t : TON; c : CTU; ms AT D0:INT:=500; pv AT D1:INT:=2; run AT X0:BOOL; up AT X1:BOOL; q AT Y0:BOOL; z AT Y1:BOOL; END_VAR                t(IN := run, PT := ms); c(CU := up, PV := pv); q := t.Q; z := c.Q;";
    let s = run_with(src, &[(0, "X0", true)], 30);
    assert!(!bit(&s, "Y0"));
    let s = run_with(src, &[(0, "X0", true)], 70);
    assert!(bit(&s, "Y0"), "PT taken from the variable (500 ms)");
    let mut plan = vec![];
    for k in 0..2 {
        plan.push((2 + 4 * k, "X1", true));
        plan.push((4 + 4 * k, "X1", false));
    }
    let s = run_with(src, &plan, 14);
    assert!(bit(&s, "Y1"), "PV taken from the variable (2)");
}
#[test]
fn ctud_counts_both_ways_with_limits() {
    let src = "VAR c : CTUD; up AT X0:BOOL; dn AT X1:BOOL; rs AT X2:BOOL; ld AT X3:BOOL; qu AT Y0:BOOL; qd AT Y1:BOOL; n AT D0:INT; END_VAR                c(CU := up, CD := dn, R := rs, LD := ld, PV := 3); qu := c.QU; qd := c.QD; n := c.CV;";
    let pulses = |dev: &'static str, start: usize, times: usize| {
        (0..times)
            .flat_map(move |k| [(start + 4 * k, dev, true), (start + 4 * k + 2, dev, false)])
            .collect::<Vec<_>>()
    };
    let s = run_with(src, &[], 3);
    assert!(bit(&s, "Y1"), "CV = 0 -> QD");
    let s = run_with(src, &pulses("X0", 2, 5), 30);
    assert_eq!(word(&s, "D0"), 3, "stops at PV");
    assert!(bit(&s, "Y0") && !bit(&s, "Y1"));
    let mut plan = pulses("X0", 2, 3);
    plan.extend(pulses("X1", 16, 1));
    let s = run_with(src, &plan, 30);
    assert_eq!(word(&s, "D0"), 2);
    let mut plan = pulses("X0", 2, 2);
    plan.push((14, "X2", true));
    let s = run_with(src, &plan, 20);
    assert_eq!(word(&s, "D0"), 0, "R clears");
    let s = run_with(src, &[(2, "X3", true), (4, "X3", false)], 8);
    assert_eq!(word(&s, "D0"), 3, "LD loads PV");
}
#[test]
fn int_mixes_with_real_without_explicit_conversion() {
    let (_, s) = run(
        "VAR i AT D0:INT:=3; d AT D1:DINT:=-4; r AT D4:REAL; q AT D6:REAL; b AT Y0:BOOL; END_VAR r := i * 2.5; q := d + r; b := r > i;",
        1,
    );
    assert_eq!(f32::from_bits(dword_at(&s, "D4") as u32), 7.5);
    assert_eq!(f32::from_bits(dword_at(&s, "D6") as u32), 3.5);
    assert!(bit(&s, "Y0"));
}

#[test]
fn array_initializers_fill_elements_on_the_first_scan() {
    let (_, s) = run(
        "VAR a AT D100 : ARRAY[0..3] OF INT := [10, -20, 2(7)]; f AT M10 : ARRAY[0..2] OF BOOL := [TRUE, FALSE, TRUE]; END_VAR",
        2,
    );
    assert_eq!(
        [
            word(&s, "D100"),
            word(&s, "D101"),
            word(&s, "D102"),
            word(&s, "D103")
        ],
        [10, -20, 7, 7]
    );
    assert!(bit(&s, "M10") && !bit(&s, "M11") && bit(&s, "M12"));
    for bad in [
        "VAR a : ARRAY[0..1] OF INT := [1, 2, 3]; END_VAR",
        "VAR a : ARRAY[0..1] OF INT := 5; END_VAR",
        "VAR a : INT := [1]; END_VAR",
    ] {
        assert!(lower(bad).is_err(), "{bad}");
    }
}
#[test]
fn bool_array_accepts_a_variable_index() {
    let (_, s) = run(
        "VAR f AT M10 : ARRAY[0..7] OF BOOL; i AT D0:INT:=3; y AT Y0:BOOL; z AT Y1:BOOL; END_VAR f[i] := TRUE; y := f[i]; z := f[i + 1];",
        2,
    );
    assert!(bit(&s, "M13"), "f[3] is M13");
    assert!(!bit(&s, "M14"));
    assert!(bit(&s, "Y0") && !bit(&s, "Y1"));
}
#[test]
fn real_mod_keeps_the_sign_of_the_dividend() {
    let (_, s) = run(
        "VAR a AT D0:REAL:=7.5; b AT D2:REAL:=-7.5; d AT D4:REAL:=2.0; x AT D6:REAL; y AT D8:REAL; END_VAR x := a MOD d; y := b MOD d;",
        1,
    );
    assert_eq!(f32::from_bits(dword_at(&s, "D6") as u32), 1.5);
    assert_eq!(f32::from_bits(dword_at(&s, "D8") as u32), -1.5);
}
#[test]
fn strings_assign_copy_compare_and_measure() {
    let (_, s) = run(
        "VAR s AT D200 : STRING[10] := 'hi'; t AT D210 : STRING[10]; n AT D0:INT; eq AT Y0:BOOL; ne AT Y1:BOOL; other AT Y2:BOOL; END_VAR          t := s; n := LEN(t); eq := t = 'hi'; ne := s <> 'ho'; other := s = t AND s <> 'hi';",
        2,
    );
    assert_eq!(word(&s, "D200"), 2);
    assert_eq!(
        word(&s, "D201") as u16,
        u16::from(b'h') | u16::from(b'i') << 8
    );
    assert_eq!(word(&s, "D210"), 2);
    assert_eq!(word(&s, "D0"), 2);
    assert!(bit(&s, "Y0") && bit(&s, "Y1") && !bit(&s, "Y2"));
    for (bad, needle) in [
        ("VAR s : STRING[3]; END_VAR s := 'toolong';", "kapasitesi"),
        ("VAR s : STRING[3]; n : INT; END_VAR n := s;", "STRING"),
        (
            "VAR s : STRING[3]; t : STRING[8]; END_VAR s := t;",
            "sığmaz",
        ),
        ("VAR s : STRING[3]; END_VAR s[0] := 1;", "STRING"),
        ("VAR n : INT; END_VAR n := 'a';", "Metin"),
    ] {
        let err = lower(bad).expect_err(bad).message;
        assert!(
            err.contains(needle),
            "{bad}
  -> {err}"
        );
    }
}
#[test]
fn struct_variables_expand_into_named_fields() {
    let src = "TYPE Point : STRUCT x : INT := 5; y : REAL; END_STRUCT END_TYPE                TYPE Pair : STRUCT a : Point; n : INT; t : TON; END_STRUCT END_TYPE                VAR p : Point; q : Pair; r AT D0:INT; w AT D2:REAL; up AT Y0:BOOL; END_VAR                p.y := 2.5; r := p.x + 1; q.a.x := 9; q.n := q.a.x * 2; w := p.y;                q.t(IN := TRUE, PT := T#100ms); up := q.t.Q;";
    let lowered = lower(src).unwrap();
    for name in ["P.X", "P.Y", "Q.A.X", "Q.A.Y", "Q.N", "Q.T"] {
        assert!(
            lowered.symbols.iter().any(|s| s.name == name),
            "{name} missing"
        );
    }
    let (_, s) = run(src, 20);
    assert_eq!(word(&s, "D0"), 6);
    assert_eq!(f32::from_bits(dword_at(&s, "D2") as u32), 2.5);
    assert!(
        lower("TYPE P : STRUCT x : INT; END_STRUCT END_TYPE VAR a AT D0 : P; END_VAR").is_err()
    );
    assert!(
        lower("TYPE P : STRUCT x : INT; END_STRUCT END_TYPE VAR a : P; END_VAR a.z := 1;").is_err()
    );
}

#[test]
fn data_types_example_compiles_and_runs() {
    let source = include_str!("../../examples/st-data-types.st");
    assert!(is_st_source(source));
    let lowered = lower(source).unwrap();
    let address = |n: &str| {
        lowered
            .symbols
            .iter()
            .find(|s| s.name == n)
            .unwrap()
            .address
            .clone()
    };
    let s = run_with(source, &[], 3);
    assert!(bit(&s, &address("IS_IDLE")), "mode = 'idle'");
    assert!(bit(&s, &address("Y.RUN")), "LEN > 3 and speed > 50");
    assert_eq!(word(&s, &address("X.SPEED")), 100);
}
