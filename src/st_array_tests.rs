//! ST `ARRAY[lo..hi] OF T`: lowered, compiled and executed on the shared simulator.
use crate::plc::{Device, Simulator, compile_il};
use crate::st::{LoweredSt, lower};

fn run(source: &str, scans: usize) -> (LoweredSt, Simulator) {
    let lowered = lower(source).unwrap_or_else(|e| panic!("{e:?}"));
    let p = compile_il(&lowered.il).unwrap_or_else(|e| panic!("{e}\n{}", lowered.il));
    let mut s = Simulator::default();
    for _ in 0..scans {
        s.scan(&p, 10);
    }
    assert_eq!(s.scans, scans as u64, "{:?}\n{}", s.last_scan, lowered.il);
    assert!(
        s.memory.execution_limitations.is_empty(),
        "{:?}\n{}",
        s.memory.execution_limitations,
        lowered.il
    );
    (lowered, s)
}
fn word(s: &Simulator, d: &str) -> i16 {
    s.memory.word(d.parse::<Device>().unwrap())
}
fn dword(s: &Simulator, d: &str) -> i32 {
    s.memory.double_word(d.parse::<Device>().unwrap())
}
fn error(source: &str) -> String {
    lower(source).unwrap_err().message
}

#[test]
fn for_loop_fills_an_array_and_sums_it() {
    let (l, s) = run(
        "VAR a : ARRAY[0..9] OF INT; i : INT; sum AT D0 : INT; END_VAR\n\
         FOR i := 0 TO 9 DO a[i] := i * i; END_FOR;\n\
         sum := 0;\n\
         FOR i := 0 TO 9 DO sum := sum + a[i]; END_FOR;",
        1,
    );
    assert_eq!(word(&s, "D0"), 285);
    let a = l.symbols.iter().find(|x| x.name == "A").unwrap();
    assert_eq!(a.array, Some((0, 9)));
    assert!(l.il.contains("E0"), "variable index must use E0:\n{}", l.il);
    // The ten elements are consecutive words from one allocation.
    let base: u16 = a.address[1..].parse().unwrap();
    for k in 0..10u16 {
        assert_eq!(word(&s, &format!("D{}", base + k)), (k * k) as i16);
    }
}
#[test]
fn constant_index_is_a_fixed_device_without_e0() {
    let (l, s) = run(
        "VAR a AT D100 : ARRAY[5..8] OF INT; END_VAR\na[5] := 11; a[8] := a[5] + 1;",
        1,
    );
    assert_eq!((word(&s, "D100"), word(&s, "D103")), (11, 12));
    assert!(!l.il.contains("E0"), "{}", l.il);
}
#[test]
fn variable_index_reads_and_writes_with_a_nonzero_lower_bound() {
    // Lower bound 3 folds into the base address; 10 words from D200 (explicit AT mapping).
    let (_, s) = run(
        "VAR a AT D200 : ARRAY[3..7] OF INT; k : INT := 5; r AT D0 : INT; END_VAR\n\
         a[k] := 77; a[k - 1] := 66; r := a[k] + a[k - 1];\n",
        2,
    );
    // a[3] = D200, a[4] = D201, a[5] = D202.
    assert_eq!((word(&s, "D201"), word(&s, "D202")), (66, 77));
    assert_eq!(word(&s, "D0"), 143);
}
#[test]
fn mapped_array_with_lower_bound_beyond_its_base_uses_a_runtime_subtract() {
    // lo*width (100) exceeds the base (D10): E0 must come from i - lo.
    let (l, s) = run(
        "VAR a AT D10 : ARRAY[100..103] OF INT; i : INT; END_VAR\n\
         FOR i := 100 TO 103 DO a[i] := i - 99; END_FOR;",
        1,
    );
    assert!(l.il.contains("SUB"), "{}", l.il);
    assert_eq!(
        (0..4).map(|k| word(&s, &format!("D{}", 10 + k))).collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
}
#[test]
fn negative_lower_bound() {
    let (_, s) = run(
        "VAR a AT D50 : ARRAY[-2..2] OF INT; i : INT; END_VAR\n\
         FOR i := -2 TO 2 DO a[i] := i * 10; END_FOR;",
        1,
    );
    let got: Vec<i16> = (0..5).map(|k| word(&s, &format!("D{}", 50 + k))).collect();
    assert_eq!(got, vec![-20, -10, 0, 10, 20]);
}
#[test]
fn read_write_with_variable_index_and_expression_index() {
    let (_, s) = run(
        "VAR a AT D300 : ARRAY[0..4] OF INT; j : INT := 2; r1 AT D0 : INT; r2 AT D1 : INT; END_VAR\n\
         a[0] := 5; a[j] := 9; a[j + 1] := a[j] + a[0]; r1 := a[j + 1]; r2 := a[a[0] - 3] ;",
        2,
    );
    assert_eq!(word(&s, "D0"), 14);
    assert_eq!(word(&s, "D1"), 9, "nested index a[a[0]-3] = a[2]");
}
#[test]
fn double_word_elements_are_scaled_by_two() {
    let (l, s) = run(
        "VAR a AT D400 : ARRAY[0..3] OF DINT; i : INT; t AT D0 : DINT; step : DINT := 100000; END_VAR\n\
         FOR i := 0 TO 3 DO a[i] := step; step := step + 100000; END_FOR;\n\
         t := 0; FOR i := 0 TO 3 DO t := t + a[i]; END_FOR; a[2] := -7;",
        1,
    );
    assert!(l.il.contains("MUL16"), "{}", l.il);
    assert_eq!(dword(&s, "D400"), 100000);
    assert_eq!(dword(&s, "D402"), 200000);
    assert_eq!(dword(&s, "D404"), -7, "a[2] overwritten after the sum");
    assert_eq!(dword(&s, "D406"), 400000);
    assert_eq!(dword(&s, "D0"), 1_000_000);
}
#[test]
fn real_elements_and_word_elements() {
    let (_, s) = run(
        "VAR r AT D500 : ARRAY[0..2] OF REAL; w AT D520 : ARRAY[0..1] OF WORD; i : INT; END_VAR\n\
         FOR i := 0 TO 2 DO r[i] := 1.5; END_FOR; r[1] := r[0] + r[2]; w[0] := 16#00F0; w[1] := w[0] AND 16#0FF0;",
        1,
    );
    let f = |d: &str| f32::from_bits(dword(&s, d) as u32);
    assert_eq!((f("D500"), f("D502"), f("D504")), (1.5, 3.0, 1.5));
    assert_eq!((word(&s, "D520"), word(&s, "D521")), (0xF0, 0xF0));
}
#[test]
fn automatic_allocation_is_contiguous_and_excludes_other_variables() {
    let (l, _) = run(
        "VAR x : INT; a : ARRAY[1..4] OF DINT; y : INT; END_VAR x := 1; y := 2; a[1] := 3;",
        1,
    );
    let addr = |n: &str| -> u16 {
        l.symbols.iter().find(|s| s.name == n).unwrap().address[1..]
            .parse()
            .unwrap()
    };
    let (x, a, y) = (addr("X"), addr("A"), addr("Y"));
    assert!(!(a..a + 8).contains(&x) && !(a..a + 8).contains(&y));
}
#[test]
fn bool_arrays_with_constant_indices() {
    let (l, s) = run(
        "VAR f : ARRAY[0..3] OF BOOL; o AT Y0 : BOOL; END_VAR f[2] := TRUE; f[0] := NOT f[2]; o := f[2];",
        1,
    );
    assert!(s.memory.bit("Y0".parse().unwrap()));
    let a = l.symbols.iter().find(|x| x.name == "F").unwrap();
    assert!(a.address.starts_with('M'));
    assert!(
        error("VAR f : ARRAY[0..3] OF BOOL; i : INT; END_VAR f[i] := TRUE;")
            .contains("BOOL dizisinde")
    );
}
#[test]
fn constant_index_out_of_range_is_a_compile_error() {
    let m = error("VAR a : ARRAY[1..4] OF INT; END_VAR a[5] := 1;");
    assert!(m.contains("a[5]".to_uppercase().as_str()) && m.contains("1..4"), "{m}");
    assert!(error("VAR a : ARRAY[1..4] OF INT; x : INT; END_VAR x := a[0];").contains("A[0]"));
    assert!(error("VAR a : ARRAY[1..4] OF INT; x : INT; END_VAR x := a[-1];").contains("A[-1]"));
    // The upper bound itself is fine.
    lower("VAR a : ARRAY[1..4] OF INT; x : INT; END_VAR x := a[4];").unwrap();
}
#[test]
fn declaration_errors() {
    assert!(error("VAR a : ARRAY[0..512] OF INT; END_VAR").contains("512"));
    lower("VAR a : ARRAY[0..511] OF INT; END_VAR a[0] := 1;").unwrap();
    assert!(error("VAR a : ARRAY[4..1] OF INT; END_VAR").contains("ters"));
    assert!(error("VAR n : INT; a : ARRAY[0..n] OF INT; END_VAR").contains("sabiti"));
    assert!(error("VAR a : ARRAY[0..3] OF INT := 1; END_VAR").contains("başlangıç"));
    assert!(
        error("VAR a : ARRAY[0..3] OF INT; END_VAR a := 1;").contains("dizidir"),
        "whole-array use needs an index"
    );
    assert!(error("VAR a : INT; END_VAR a[0] := 1;").contains("dizi değil"));
    assert!(error("VAR a : ARRAY[0..3] OF INT; r : REAL; END_VAR a[r] := 1;").contains("INT"));
    assert!(error("VAR a AT D4998 : ARRAY[0..3] OF INT; END_VAR").contains("D4999"));
    assert!(error("VAR a AT Y0 : ARRAY[0..3] OF BOOL; END_VAR").contains("M adresine"));
    assert!(error("VAR a : ARRAY[0..3] OF STRING; END_VAR").contains("veri tipi"));
}
#[test]
fn explicitly_mapped_arrays_are_reserved_from_automatic_allocation() {
    let mut options = crate::st::LowerOptions::default();
    options.alloc.d = (600, 612);
    let l = crate::st::lower_with(
        "VAR a AT D600 : ARRAY[0..3] OF INT; b : INT; END_VAR b := 1; a[0] := 2;",
        &options,
    )
    .unwrap();
    let b = l.symbols.iter().find(|x| x.name == "B").unwrap();
    assert_eq!(b.address, "D604");
}
