//! ST `VAR_RETAIN`: allocation from the latched ranges and their reporting.
use crate::alloc::{Alloc, Retain};
use crate::plc::{Simulator, compile_il};
use crate::st::{LowerOptions, lower, lower_with};

fn addr(l: &crate::st::LoweredSt, name: &str) -> (String, bool) {
    let s = l.symbols.iter().find(|s| s.name == name).unwrap();
    (s.address.clone(), s.retained)
}
fn index(a: &str) -> u16 {
    a[1..].parse().unwrap()
}

#[test]
fn retained_variables_land_in_the_latched_range_and_normal_ones_do_not() {
    let l = lower(
        "VAR_RETAIN cnt : INT; big : DINT; r : REAL; flag : BOOL; END_VAR\n\
         VAR tmp : INT; ok : BOOL; END_VAR\n\
         cnt := cnt + 1; tmp := cnt; flag := ok;",
    )
    .unwrap();
    let r = Retain::default();
    for (n, width) in [("CNT", 1), ("BIG", 2), ("R", 2)] {
        let (a, retained) = addr(&l, n);
        assert!(retained, "{n}");
        assert!(a.starts_with('D') && index(&a) >= r.d.0 && index(&a) + width - 1 <= r.d.1);
        assert!(Retain::word_is_latched(index(&a), width as usize), "{n} {a}");
    }
    let (f, retained) = addr(&l, "FLAG");
    assert!(retained && f.starts_with('M') && Retain::bit_is_latched(index(&f)));
    assert!((r.m.0..=r.m.1).contains(&index(&f)));
    let alloc = Alloc::default();
    for n in ["TMP", "OK"] {
        let (a, retained) = addr(&l, n);
        assert!(!retained);
        let range = if a.starts_with('D') { alloc.d } else { alloc.m };
        assert!((range.0..=range.1).contains(&index(&a)), "{n} {a}");
    }
}
#[test]
fn var_retain_spellings_and_mixed_sections() {
    let l = lower(
        "PROGRAM P\nVAR a : INT; END_VAR\nVAR RETAIN b : INT; END_VAR\nVAR_RETAIN c : INT; END_VAR\nVAR d : INT; END_VAR\nb := a + c + d;\nEND_PROGRAM",
    )
    .unwrap();
    let flags: Vec<bool> = ["A", "B", "C", "D"].iter().map(|n| addr(&l, n).1).collect();
    assert_eq!(flags, vec![false, true, true, false]);
    assert!(crate::st::is_st_source("VAR_RETAIN x : INT; END_VAR x := 1;"));
}
#[test]
fn retained_arrays_use_the_latched_range() {
    let l = lower(
        "VAR_RETAIN t : ARRAY[0..9] OF DINT; END_VAR VAR i : INT; END_VAR FOR i := 0 TO 9 DO t[i] := i; END_FOR;",
    )
    .unwrap();
    let (a, retained) = addr(&l, "T");
    assert!(retained && Retain::word_is_latched(index(&a), 20), "{a}");
}
#[test]
fn retained_program_runs() {
    let l = lower("VAR_RETAIN n : INT; END_VAR n := n + 1;").unwrap();
    let p = compile_il(&l.il).unwrap();
    let mut s = Simulator::default();
    for _ in 0..3 {
        s.scan(&p, 10);
    }
    let (a, _) = addr(&l, "N");
    assert_eq!(s.memory.word(a.parse().unwrap()), 3);
}
#[test]
fn custom_retain_range_exhaustion_and_messages() {
    let options = LowerOptions {
        retain: Retain {
            d: (2000, 2002),
            m: (2048, 2048),
        },
        ..Default::default()
    };
    let l = lower_with("VAR_RETAIN a : DINT; b : INT; f : BOOL; END_VAR", &options).unwrap();
    assert_eq!(addr(&l, "A").0, "D2000");
    assert_eq!(addr(&l, "B").0, "D2002");
    assert_eq!(addr(&l, "F").0, "M2048");
    let e = lower_with("VAR_RETAIN a : DINT; b : DINT; END_VAR", &options).unwrap_err();
    assert!(e.message.contains("alanı dolu") && e.message.contains("D2000..D2002"), "{e:?}");
    assert!(e.message.contains("RETAIN"));
    let e = lower_with("VAR_RETAIN a : BOOL; b : BOOL; END_VAR", &options).unwrap_err();
    assert!(e.message.contains("M2048..M2048"), "{e:?}");
    // Exhausting the retain pool must not touch the normal pool, and vice versa.
    lower_with("VAR_RETAIN a : DINT; END_VAR VAR n : INT; END_VAR", &options).unwrap();
}
#[test]
fn retain_avoids_globals_and_explicit_addresses() {
    let options = LowerOptions {
        retain: Retain {
            d: (2000, 2005),
            m: (2048, 2060),
        },
        globals: vec![crate::st::Global {
            name: "g".into(),
            data_type: crate::st::DataType::Int,
            address: "D2000".into(),
        }],
        ..Default::default()
    };
    let l = lower_with("VAR_RETAIN a : INT; END_VAR a := D2001;", &options).unwrap();
    // D2000 is a global; D2001 is spelled directly (reserved with its neighbour D2002).
    assert_eq!(addr(&l, "A").0, "D2003");
}
#[test]
fn invalid_retain_options_and_declarations_are_rejected() {
    let bad = LowerOptions {
        retain: Retain {
            d: (4000, 4100),
            m: (2048, 2303),
        },
        ..Default::default()
    };
    let e = lower_with("VAR_RETAIN a : INT; END_VAR", &bad).unwrap_err();
    assert!(e.message.contains("Kalıcı D"), "{e:?}");
    // The options are only checked when a retained variable needs them.
    lower_with("VAR a : INT; END_VAR a := 1;", &bad).unwrap();
    let e = lower("VAR_RETAIN a : INT := 3; END_VAR").unwrap_err();
    assert!(e.message.contains("VAR_RETAIN başlangıç"), "{e:?}");
}
#[test]
fn explicitly_mapped_retained_variables_must_be_latched() {
    let l = lower("VAR_RETAIN a AT D2500 : INT; b AT M600 : BOOL; END_VAR a := 1;").unwrap();
    assert_eq!(l.symbols.iter().filter(|s| s.retained).count(), 2);
    for src in [
        "VAR_RETAIN a AT D4000 : INT; END_VAR",
        "VAR_RETAIN a AT D2500 : ARRAY[0..2000] OF INT; END_VAR",
        "VAR_RETAIN a AT D3919 : DINT; END_VAR",
        "VAR_RETAIN a AT Y0 : BOOL; END_VAR",
        "VAR_RETAIN a AT M800 : BOOL; END_VAR",
    ] {
        let e = lower(src).unwrap_err();
        assert!(
            e.message.contains("latched") || e.message.contains("512"),
            "{src}: {e:?}"
        );
    }
}
