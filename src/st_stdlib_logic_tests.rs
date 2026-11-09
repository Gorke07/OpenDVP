//! Behavioural tests of the ST standard library, category "logic" (src/st/stdlib/logic.st).
#![allow(dead_code)]
use crate::plc::{self, Simulator};

/// Compiles `source`, runs `scans` scans of 100 ms each and returns the simulator.
fn run(source: &str, scans: usize, setup: impl Fn(&mut Simulator)) -> Simulator {
    let program = plc::compile_st(source).unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    let mut sim = Simulator::default();
    setup(&mut sim);
    for _ in 0..scans {
        sim.scan(&program, 100);
    }
    sim
}

/// Runs `scans` scans of 100 ms; before scan `i` (0-based) `input(i, sim)` sets inputs,
/// after it `sample(sim)` records a value.
fn trace<T>(
    source: &str,
    scans: usize,
    input: impl Fn(usize, &mut Simulator),
    sample: impl Fn(&Simulator) -> T,
) -> Vec<T> {
    let program = plc::compile_st(source).unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    let mut sim = Simulator::default();
    let mut out = Vec::new();
    for i in 0..scans {
        input(i, &mut sim);
        sim.scan(&program, 100);
        out.push(sample(&sim));
    }
    out
}

fn set(sim: &mut Simulator, device: &str, value: bool) {
    sim.memory.set_input(device.parse().unwrap(), value);
}

fn word(sim: &Simulator, device: &str) -> i16 {
    sim.memory.word(device.parse().unwrap())
}

fn bit(sim: &Simulator, device: &str) -> bool {
    sim.memory.bit(device.parse().unwrap())
}

fn real(sim: &Simulator, device: &str) -> f32 {
    let d: plc::Device = device.parse().unwrap();
    match crate::plc_monitor::read_memory(&sim.memory, &[(d, crate::st::DataType::Real)])[0] {
        crate::plc_monitor::Value::Real(v) => v,
        _ => unreachable!(),
    }
}

/// Program with one block instance `b` called with `call` (the argument list) each scan,
/// and each output `(o_NAME, device, type)` mapped to `device` and assigned `b.NAME`.
fn prog(block: &str, call: &str, outputs: &[(&str, &str, &str)]) -> String {
    let decls: String = outputs
        .iter()
        .map(|(name, at, ty)| format!(" {name} AT {at} : {ty};"))
        .collect();
    let copies: String = outputs
        .iter()
        .map(|(name, _, _)| format!(" {name} := b.{};", name.trim_start_matches("o_")))
        .collect();
    format!("PROGRAM P\nVAR b : {block};{decls} END_VAR\nb({call});{copies}\nEND_PROGRAM")
}

#[test]
fn r_trig_and_f_trig_pulse_for_one_scan() {
    let src = "PROGRAM P\nVAR r : R_TRIG; f : F_TRIG; up AT Y0 : BOOL; down AT Y1 : BOOL; END_VAR\nr(CLK := X0); f(CLK := X0); up := r.Q; down := f.Q;\nEND_PROGRAM";
    let program = plc::compile_st(src).unwrap();
    let mut sim = Simulator::default();
    let mut seen = Vec::new();
    for x0 in [false, true, true, false, false] {
        sim.memory.set_input("X0".parse().unwrap(), x0);
        sim.scan(&program, 100);
        seen.push((bit(&sim, "Y0"), bit(&sim, "Y1")));
    }
    assert_eq!(
        seen,
        [
            (false, false),
            (true, false),
            (false, false),
            (false, true),
            (false, false)
        ]
    );
}

#[test]
fn sr_is_set_dominant_and_rs_is_reset_dominant() {
    let src = "PROGRAM P\nVAR a : SR; b : RS; o0 AT Y0 : BOOL; o1 AT Y1 : BOOL; END_VAR\na(SET1 := X0, RESET := X1); b(SET := X0, RESET1 := X1); o0 := a.Q1; o1 := b.Q1;\nEND_PROGRAM";
    // (set, reset) per scan
    let steps = [
        (false, false),
        (true, false),
        (false, false),
        (true, true),
        (false, true),
        (false, false),
    ];
    let seen = trace(
        src,
        steps.len(),
        |i, s| {
            set(s, "X0", steps[i].0);
            set(s, "X1", steps[i].1);
        },
        |s| (bit(s, "Y0"), bit(s, "Y1")),
    );
    assert_eq!(
        seen,
        [
            (false, false),
            (true, true),
            (true, true),
            (true, false),
            (false, false),
            (false, false)
        ]
    );
}

#[test]
fn toggle_flips_on_rising_edges_and_resets() {
    let src = prog("TOGGLE", "CLK := X0, RST := X1", &[("o_Q", "Y0", "BOOL")]);
    let clk = [false, true, true, false, true, false, true, true];
    let rst = [false, false, false, false, false, false, true, false];
    let seen = trace(
        &src,
        clk.len(),
        |i, s| {
            set(s, "X0", clk[i]);
            set(s, "X1", rst[i]);
        },
        |s| bit(s, "Y0"),
    );
    assert_eq!(seen, [false, true, true, true, false, false, false, false]);
}

#[test]
fn edge_reports_both_edges() {
    let src = prog(
        "EDGE",
        "CLK := X0",
        &[
            ("o_RE", "Y0", "BOOL"),
            ("o_FE", "Y1", "BOOL"),
            ("o_Q", "Y2", "BOOL"),
        ],
    );
    let clk = [false, true, true, false, false];
    let seen = trace(
        &src,
        clk.len(),
        |i, s| set(s, "X0", clk[i]),
        |s| (bit(s, "Y0"), bit(s, "Y1"), bit(s, "Y2")),
    );
    assert_eq!(
        seen,
        [
            (false, false, false),
            (true, false, true),
            (false, false, false),
            (false, true, true),
            (false, false, false)
        ]
    );
}

#[test]
fn majority_votes_two_out_of_three() {
    let src = "PROGRAM P\nVAR y AT Y0 : BOOL; END_VAR\ny := MAJORITY(X0, X1, X2);\nEND_PROGRAM";
    let program = plc::compile_st(src).unwrap();
    for n in 0..8u8 {
        let mut sim = Simulator::default();
        set(&mut sim, "X0", n & 1 != 0);
        set(&mut sim, "X1", n & 2 != 0);
        set(&mut sim, "X2", n & 4 != 0);
        sim.scan(&program, 100);
        assert_eq!(bit(&sim, "Y0"), n.count_ones() >= 2, "inputs {n:03b}");
    }
}

/// Lengths of the runs of equal values, e.g. `[F,T,T,F]` -> `[1,2,1]`.
fn runs(v: &[bool]) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    for (i, x) in v.iter().enumerate() {
        if i > 0 && v[i - 1] == *x {
            *out.last_mut().unwrap() += 1;
        } else {
            out.push(1);
        }
    }
    out
}

/// Index of the first TRUE value.
fn first(v: &[bool]) -> Option<usize> {
    v.iter().position(|x| *x)
}

/// Bool trace of `Y0` for a block driven by `X0` from the `x0` pattern.
fn bool_trace(src: &str, x0: &[bool]) -> Vec<bool> {
    trace(src, x0.len(), |i, s| set(s, "X0", x0[i]), |s| bit(s, "Y0"))
}

fn pattern(parts: &[(bool, usize)]) -> Vec<bool> {
    parts
        .iter()
        .flat_map(|(v, n)| std::iter::repeat_n(*v, *n))
        .collect()
}

#[test]
fn clk_prg_pulses_once_per_period() {
    let src = prog("CLK_PRG", "PT := 500", &[("o_Q", "Y0", "BOOL")]);
    let q = bool_trace(&src, &[false; 40]);
    let at: Vec<usize> = (0..q.len()).filter(|i| q[*i]).collect();
    assert!(at.len() >= 6, "{q:?}");
    assert_eq!(at[0], 5, "first pulse PT ms after the start: {q:?}");
    for w in at.windows(2) {
        assert_eq!(w[1] - w[0], 5, "{q:?}");
    }
}

#[test]
fn blink_alternates_high_and_low_times() {
    let src = prog(
        "BLINK",
        "ENABLE := X0, LOW_MS := 300, HIGH_MS := 200",
        &[("o_Q", "Y0", "BOOL")],
    );
    let x0 = pattern(&[(false, 2), (true, 16), (false, 3)]);
    let q = bool_trace(&src, &x0);
    assert_eq!(&q[..2], [false, false]);
    assert!(q[2], "starts high on enable: {q:?}");
    assert_eq!(runs(&q[2..17]), [2, 3, 2, 3, 2, 3], "{q:?}");
    assert_eq!(&q[18..], [false, false, false]);
}

#[test]
fn debounce_ignores_short_glitches_on_both_edges() {
    let src = prog("DEBOUNCE", "IN := X0, PT := 300", &[("o_Q", "Y0", "BOOL")]);
    let x0 = pattern(&[
        (true, 2),
        (false, 2),
        (true, 8),
        (false, 2),
        (true, 2),
        (false, 8),
    ]);
    let q = bool_trace(&src, &x0);
    let expect = pattern(&[(false, 7), (true, 12), (false, 5)]);
    assert_eq!(q, expect);
}

#[test]
fn tonof_applies_separate_on_and_off_delays() {
    let src = prog(
        "TONOF",
        "IN := X0, ON_MS := 300, OFF_MS := 500",
        &[("o_Q", "Y0", "BOOL")],
    );
    let x0 = pattern(&[
        (true, 2),
        (false, 1),
        (true, 6),
        (false, 3),
        (true, 1),
        (false, 8),
    ]);
    let q = bool_trace(&src, &x0);
    let expect = pattern(&[(false, 6), (true, 12), (false, 3)]);
    assert_eq!(q, expect);
}

#[test]
fn pulse_ext_stretches_short_pulses_only() {
    let src = prog("PULSE_EXT", "IN := X0, PT := 400", &[("o_Q", "Y0", "BOOL")]);
    let x0 = pattern(&[(false, 1), (true, 1), (false, 7), (true, 8), (false, 3)]);
    let q = bool_trace(&src, &x0);
    let expect = pattern(&[(false, 1), (true, 4), (false, 4), (true, 8), (false, 3)]);
    assert_eq!(q, expect);
}

#[test]
fn tp_retrig_restarts_on_each_rising_edge() {
    let src = prog("TP_RETRIG", "IN := X0, PT := 300", &[("o_Q", "Y0", "BOOL")]);
    let x0 = pattern(&[(true, 1), (false, 1), (true, 1), (false, 10)]);
    let q = bool_trace(&src, &x0);
    let off = q.iter().position(|b| !*b);
    assert_eq!(off, Some(6), "{q:?}");
    assert!(q[6..].iter().all(|b| !*b), "{q:?}");
}

#[test]
fn ton_sec_counts_seconds_across_the_time_base() {
    let src = prog(
        "TON_SEC",
        "IN := X0, PT := 12",
        &[("o_Q", "Y0", "BOOL"), ("o_ET", "D20", "INT")],
    );
    let out = trace(
        &src,
        140,
        |i, s| set(s, "X0", i < 135),
        |s| (bit(s, "Y0"), word(s, "D20")),
    );
    let on = out.iter().position(|o| o.0).expect("Q never set");
    assert!((118..=124).contains(&on), "Q at scan {on}");
    assert!((4..=6).contains(&out[55].1), "ET {}", out[55].1);
    assert_eq!(out[134], (true, 12));
    assert!(out[..on].iter().all(|o| o.1 < 12));
    assert_eq!(out[136], (false, 0), "IN FALSE resets");
}

#[test]
fn ton_sec_with_zero_preset_is_immediate() {
    let src = prog("TON_SEC", "IN := X0, PT := 0", &[("o_Q", "Y0", "BOOL")]);
    assert_eq!(bool_trace(&src, &[false, true, true]), [false, true, true]);
}

#[test]
fn tof_sec_holds_after_input_falls() {
    let src = prog(
        "TOF_SEC",
        "IN := X0, PT := 2",
        &[("o_Q", "Y0", "BOOL"), ("o_ET", "D20", "INT")],
    );
    let out = trace(
        &src,
        40,
        |i, s| set(s, "X0", (2..5).contains(&i)),
        |s| (bit(s, "Y0"), word(s, "D20")),
    );
    let q: Vec<bool> = out.iter().map(|o| o.0).collect();
    assert_eq!(first(&q), Some(2));
    let off = (5..q.len()).find(|i| !q[*i]).expect("never released");
    assert!((24..=27).contains(&off), "released at {off}: {q:?}");
    assert!(q[off..].iter().all(|b| !*b), "{q:?}");
    assert_eq!(out[15].1, 1);
}

#[test]
fn tof_sec_input_returning_cancels_the_delay() {
    let src = prog("TOF_SEC", "IN := X0, PT := 1", &[("o_Q", "Y0", "BOOL")]);
    let x0 = pattern(&[(true, 2), (false, 5), (true, 3), (false, 20)]);
    let q = bool_trace(&src, &x0);
    assert!(q[..17].iter().all(|b| *b), "{q:?}");
    assert!(!q[29], "{q:?}");
}

#[test]
fn tonr_accumulates_and_holds_until_reset() {
    let src = prog(
        "TONR",
        "IN := X0, RST := X1, PT := 500",
        &[("o_Q", "Y0", "BOOL"), ("o_ET", "D20", "INT")],
    );
    let x0 = pattern(&[(true, 3), (false, 4), (true, 5), (false, 4)]);
    let out = trace(
        &src,
        x0.len(),
        |i, s| {
            set(s, "X0", x0[i]);
            set(s, "X1", i == 14);
        },
        |s| (bit(s, "Y0"), word(s, "D20")),
    );
    let held = out[3].1;
    assert!(held > 0 && held < 500, "{out:?}");
    assert!(out[3..7].iter().all(|o| *o == (false, held)), "{out:?}");
    assert_eq!(out[11], (true, 500), "{out:?}");
    assert_eq!(out[13], (true, 500), "kept while IN FALSE");
    assert_eq!(out[14], (false, 0), "RST clears");
}

#[test]
fn watchdog_faults_when_the_signal_stops() {
    let src = prog(
        "WATCHDOG",
        "ENABLE := X0, IN := X1, PT := 300, RST := X2",
        &[("o_FAULT", "Y0", "BOOL")],
    );
    let out = trace(
        &src,
        30,
        |i, s| {
            set(s, "X0", i < 28);
            set(
                s,
                "X1",
                (i < 10 && (i / 2) % 2 == 1) || ((16..19).contains(&i) && i % 2 == 0),
            );
            set(s, "X2", i == 20);
        },
        |s| bit(s, "Y0"),
    );
    assert!(
        out[..10].iter().all(|b| !*b),
        "toggling every 200 ms is healthy: {out:?}"
    );
    let f = first(&out).expect("no fault");
    assert!((12..=14).contains(&f), "fault at {f}: {out:?}");
    assert!(
        out[f..20].iter().all(|b| *b),
        "latched despite new toggles: {out:?}"
    );
    assert!(!out[20], "RST clears: {out:?}");
    assert!(out[27], "faults again after a new timeout: {out:?}");
    assert!(!out[28] && !out[29], "ENABLE FALSE clears: {out:?}");
}

#[test]
fn ctu_sw_counts_rising_edges_to_preset() {
    let src = prog(
        "CTU_SW",
        "CU := X0, R := X1, PV := 3",
        &[("o_Q", "Y0", "BOOL"), ("o_CV", "D20", "INT")],
    );
    let cu = [
        true, true, false, true, false, true, false, true, false, false,
    ];
    let out = trace(
        &src,
        cu.len(),
        |i, s| {
            set(s, "X0", cu[i]);
            set(s, "X1", i == 8);
        },
        |s| (bit(s, "Y0"), word(s, "D20")),
    );
    let cv: Vec<i16> = out.iter().map(|o| o.1).collect();
    assert_eq!(cv, [1, 1, 1, 2, 2, 3, 3, 4, 0, 0]);
    let q: Vec<bool> = out.iter().map(|o| o.0).collect();
    assert_eq!(
        q,
        [
            false, false, false, false, false, true, true, true, false, false
        ]
    );
}

#[test]
fn ctd_sw_counts_down_from_preset_and_stops_at_zero() {
    let src = prog(
        "CTD_SW",
        "CD := X0, LD := X1, PV := 2",
        &[("o_Q", "Y0", "BOOL"), ("o_CV", "D20", "INT")],
    );
    let cd = [false, true, false, true, false, true, false, false];
    let out = trace(
        &src,
        cd.len(),
        |i, s| {
            set(s, "X0", cd[i]);
            set(s, "X1", i == 0 || i == 7);
        },
        |s| (bit(s, "Y0"), word(s, "D20")),
    );
    assert_eq!(
        out,
        [
            (false, 2),
            (false, 1),
            (false, 1),
            (true, 0),
            (true, 0),
            (true, 0),
            (true, 0),
            (false, 2)
        ]
    );
}

#[test]
fn ctud_sw_counts_both_ways_and_saturates() {
    let src = prog(
        "CTUD_SW",
        "CU := X0, CD := X1, R := X2, LD := X3, PV := pv",
        &[
            ("o_QU", "Y0", "BOOL"),
            ("o_QD", "Y1", "BOOL"),
            ("o_CV", "D20", "INT"),
        ],
    )
    .replace("VAR b", "VAR pv AT D10 : INT; b");
    let program = plc::compile_st(&src).unwrap_or_else(|e| panic!("{e:?}\n{src}"));
    let mut sim = Simulator::default();
    let step = |sim: &mut Simulator, cu: bool, cd: bool, r: bool, ld: bool| {
        set(sim, "X0", cu);
        set(sim, "X1", cd);
        set(sim, "X2", r);
        set(sim, "X3", ld);
        sim.scan(&program, 100);
        (bit(sim, "Y0"), bit(sim, "Y1"), word(sim, "D20"))
    };
    let pv = |sim: &mut Simulator, v: i16| sim.memory.set_word("D10".parse().unwrap(), v);
    pv(&mut sim, 2);
    assert_eq!(step(&mut sim, false, false, false, false), (false, true, 0));
    assert_eq!(step(&mut sim, true, false, false, false), (false, false, 1));
    assert_eq!(
        step(&mut sim, false, false, false, false),
        (false, false, 1)
    );
    assert_eq!(step(&mut sim, true, false, false, false), (true, false, 2));
    assert_eq!(step(&mut sim, false, true, false, false), (false, false, 1));
    assert_eq!(
        step(&mut sim, false, false, false, false),
        (false, false, 1)
    );
    assert_eq!(
        step(&mut sim, true, true, false, false),
        (false, false, 1),
        "both edges cancel"
    );
    assert_eq!(
        step(&mut sim, false, false, false, false),
        (false, false, 1)
    );
    assert_eq!(step(&mut sim, false, true, false, false), (false, true, 0));
    assert_eq!(step(&mut sim, false, false, false, false), (false, true, 0));
    assert_eq!(
        step(&mut sim, false, true, false, false),
        (false, true, -1),
        "may go negative"
    );
    pv(&mut sim, 32767);
    assert_eq!(
        step(&mut sim, false, false, false, true),
        (true, false, 32767)
    );
    assert_eq!(
        step(&mut sim, true, false, false, false),
        (true, false, 32767),
        "saturates high"
    );
    pv(&mut sim, -32768);
    assert_eq!(
        step(&mut sim, false, false, false, true),
        (true, true, -32768)
    );
    assert_eq!(
        step(&mut sim, false, true, false, false),
        (true, true, -32768),
        "saturates low"
    );
    pv(&mut sim, 2);
    assert_eq!(
        step(&mut sim, false, false, true, true),
        (false, true, 0),
        "R beats LD"
    );
}

#[test]
fn sequencer_steps_through_timed_steps() {
    let src = prog(
        "SEQUENCER",
        "RUN := X0, CONT := X1, T1_MS := 200, T2_MS := 300, T3_MS := 0",
        &[
            ("o_STEP", "D20", "INT"),
            ("o_DONE", "Y0", "BOOL"),
            ("o_Q2", "Y2", "BOOL"),
        ],
    );
    let once = trace(
        &src,
        9,
        |i, s| set(s, "X0", i < 8),
        |s| (word(s, "D20"), bit(s, "Y0"), bit(s, "Y2")),
    );
    let steps: Vec<i16> = once.iter().map(|o| o.0).collect();
    assert_eq!(steps, [1, 1, 2, 2, 2, 0, 0, 0, 0], "{once:?}");
    let done: Vec<bool> = once.iter().map(|o| o.1).collect();
    assert_eq!(
        done,
        [false, false, false, false, false, true, true, true, false]
    );
    assert!(once[2].2 && !once[1].2);
    let cyc = trace(
        &src,
        12,
        |_, s| {
            set(s, "X0", true);
            set(s, "X1", true);
        },
        |s| word(s, "D20"),
    );
    assert_eq!(cyc, [1, 1, 2, 2, 2, 1, 1, 2, 2, 2, 1, 1]);
}

#[test]
fn interlock_keeps_outputs_mutually_exclusive() {
    let src = prog(
        "INTERLOCK",
        "IN1 := X0, IN2 := X1, IN3 := X2, IN4 := X3",
        &[
            ("o_ACTIVE", "D20", "INT"),
            ("o_Q1", "Y0", "BOOL"),
            ("o_Q2", "Y1", "BOOL"),
        ],
    );
    // Requests per scan as bit masks (bit 0 = IN1).
    let req = [
        0b0000u8, 0b0010, 0b0011, 0b0001, 0b1001, 0b1000, 0b0110, 0b0000,
    ];
    let out = trace(
        &src,
        req.len(),
        |i, s| {
            for (k, x) in ["X0", "X1", "X2", "X3"].iter().enumerate() {
                set(s, x, req[i] & (1 << k) != 0);
            }
        },
        |s| (word(s, "D20"), bit(s, "Y0"), bit(s, "Y1")),
    );
    let active: Vec<i16> = out.iter().map(|o| o.0).collect();
    assert_eq!(active, [0, 2, 2, 1, 1, 4, 2, 0]);
    assert_eq!(out[1], (2, false, true));
    assert_eq!(out[3], (1, true, false));
}

#[test]
fn pulse_ext_stretches_a_pulse_right_after_the_previous_one() {
    let src = prog("PULSE_EXT", "IN := X0, PT := 400", &[("o_Q", "Y0", "BOOL")]);
    // First pulse ends (Q falls) at scan 4; the next 1-scan pulse comes at scan 5.
    let x0 = pattern(&[(true, 1), (false, 4), (true, 1), (false, 8)]);
    let q = bool_trace(&src, &x0);
    assert_eq!(&q[..5], [true, true, true, true, false], "{q:?}");
    assert!(
        q[5..9].iter().all(|b| *b),
        "second pulse stretched to PT: {q:?}"
    );
    assert!(q[11..].iter().all(|b| !*b), "{q:?}");
}

#[test]
fn tonr_accumulates_short_pulses() {
    let src = prog(
        "TONR",
        "IN := X0, RST := X1, PT := 500",
        &[("o_Q", "Y0", "BOOL"), ("o_ET", "D20", "INT")],
    );
    // IN is TRUE on every other scan, one scan at a time.
    let out = trace(
        &src,
        14,
        |i, s| set(s, "X0", i % 2 == 0),
        |s| (bit(s, "Y0"), word(s, "D20")),
    );
    assert_eq!(out[4], (false, 200), "{out:?}");
    assert_eq!(out[5], (false, 200), "kept while IN FALSE: {out:?}");
    assert_eq!(out[10], (true, 500), "{out:?}");
    assert!(!out[9].0, "{out:?}");
}

#[test]
fn tof_sec_holds_et_at_preset_after_expiry() {
    let src = prog(
        "TOF_SEC",
        "IN := X0, PT := 1",
        &[("o_Q", "Y0", "BOOL"), ("o_ET", "D20", "INT")],
    );
    let out = trace(
        &src,
        30,
        |i, s| set(s, "X0", i < 2 || i == 28),
        |s| (bit(s, "Y0"), word(s, "D20")),
    );
    assert_eq!(out[0], (true, 0));
    assert_eq!(out[20], (false, 1), "ET stays at PT after expiry: {out:?}");
    assert_eq!(out[27], (false, 1), "{out:?}");
    assert_eq!(out[28], (true, 0), "IN TRUE clears ET: {out:?}");
}

#[test]
fn sequencer_stops_when_t1_ms_becomes_zero_while_cycling() {
    let src = prog(
        "SEQUENCER",
        "RUN := TRUE, CONT := TRUE, T1_MS := pt1, T2_MS := 300, T3_MS := 0",
        &[("o_STEP", "D20", "INT"), ("o_DONE", "Y0", "BOOL")],
    )
    .replace("VAR b", "VAR pt1 AT D10 : INT; b");
    let out = trace(
        &src,
        10,
        |i, s| {
            s.memory
                .set_word("D10".parse().unwrap(), if i < 3 { 200 } else { 0 })
        },
        |s| (word(s, "D20"), bit(s, "Y0")),
    );
    let steps: Vec<i16> = out.iter().map(|o| o.0).collect();
    assert_eq!(steps, [1, 1, 2, 2, 2, 0, 0, 0, 0, 0], "{out:?}");
    assert!(out[5..].iter().all(|o| o.1), "DONE: {out:?}");
}

fn set_word(sim: &mut Simulator, device: &str, value: i16) {
    sim.memory.set_word(device.parse().unwrap(), value);
}

#[test]
fn debounce_and_tonof_with_zero_time_follow_the_input() {
    let x0 = pattern(&[(false, 1), (true, 2), (false, 1), (true, 1), (false, 2)]);
    let db = prog("DEBOUNCE", "IN := X0, PT := 0", &[("o_Q", "Y0", "BOOL")]);
    assert_eq!(bool_trace(&db, &x0), x0);
    let tf = prog(
        "TONOF",
        "IN := X0, ON_MS := 0, OFF_MS := 0",
        &[("o_Q", "Y0", "BOOL")],
    );
    assert_eq!(bool_trace(&tf, &x0), x0);
}

const CHAIN_OUT: &[(&str, &str, &str)] = &[
    ("o_STEP", "D20", "INT"),
    ("o_ET", "D21", "INT"),
    ("o_DONE", "Y0", "BOOL"),
    ("o_TIMEOUT", "Y1", "BOOL"),
    ("o_Q1", "Y2", "BOOL"),
    ("o_Q2", "Y3", "BOOL"),
    ("o_Q3", "Y4", "BOOL"),
];

#[test]
fn step_chain_advances_one_step_per_scan_and_finishes() {
    let src = prog(
        "STEP_CHAIN",
        "START := X0, TR1 := X1, TR2 := X2, TR3 := X3, STEPS := 3",
        CHAIN_OUT,
    );
    let out = trace(
        &src,
        10,
        |i, s| {
            set(s, "X0", (1..7).contains(&i) || i == 8);
            set(s, "X1", (2..5).contains(&i));
            set(s, "X2", (3..5).contains(&i));
            set(s, "X3", (3..5).contains(&i));
        },
        |s| {
            (
                word(s, "D20"),
                bit(s, "Y0"),
                (bit(s, "Y2"), bit(s, "Y3"), bit(s, "Y4")),
            )
        },
    );
    let steps: Vec<i16> = out.iter().map(|o| o.0).collect();
    // TR3 is already TRUE when step 3 begins: it still lasts one scan.
    assert_eq!(steps, [0, 1, 2, 3, 0, 0, 0, 0, 1, 1], "{out:?}");
    let done: Vec<bool> = out.iter().map(|o| o.1).collect();
    assert_eq!(
        done,
        [
            false, false, false, false, true, true, true, true, false, false
        ],
        "DONE until the next START edge (START held TRUE does not restart)"
    );
    assert_eq!(out[1].2, (true, false, false));
    assert_eq!(out[2].2, (false, true, false));
    assert_eq!(out[3].2, (false, false, true));
    assert_eq!(out[4].2, (false, false, false));
}

#[test]
fn step_chain_timeout_hold_and_reset() {
    let src = prog(
        "STEP_CHAIN",
        "START := X0, TR1 := X1, STEPS := 3, TMO_MS := 300, HOLD := X4, RST := X5",
        CHAIN_OUT,
    );
    let out = trace(
        &src,
        14,
        |i, s| {
            set(s, "X0", i == 0 || i == 12);
            set(s, "X1", i == 5 || i == 9);
            set(s, "X4", (4..8).contains(&i));
            set(s, "X5", (11..13).contains(&i));
        },
        |s| (word(s, "D20"), bit(s, "Y1"), word(s, "D21")),
    );
    assert_eq!(out[0], (1, false, 0), "START on the first scan: {out:?}");
    assert_eq!(out[2], (1, false, 200), "{out:?}");
    assert_eq!(out[3], (1, true, 300), "timeout reached: {out:?}");
    assert_eq!(out[5], (1, true, 300), "HOLD: frozen, TR1 ignored: {out:?}");
    assert_eq!(out[7], (1, true, 300), "{out:?}");
    assert_eq!(
        out[8],
        (1, true, 400),
        "time runs again after HOLD: {out:?}"
    );
    assert_eq!(out[9], (2, false, 0), "step change clears TIMEOUT: {out:?}");
    assert_eq!(out[10], (2, false, 100), "{out:?}");
    assert_eq!(out[11], (0, false, 0), "RST: {out:?}");
    assert_eq!(out[12], (0, false, 0), "RST beats START: {out:?}");
    assert_eq!(out[13], (0, false, 0), "START needs a new edge: {out:?}");
}

#[test]
fn step_chain_jumps_and_defaults_to_eight_steps() {
    let src = prog(
        "STEP_CHAIN",
        "START := X0, TR8 := X7, STEPS := 0, HOLD := X4, JUMP := X6, JUMP_TO := jt",
        CHAIN_OUT,
    )
    .replace("VAR b", "VAR jt AT D10 : INT; b");
    // (jump, jump_to, tr8, hold) per scan; START only on scan 0.
    let steps = [
        (false, 0, false, false),
        (true, 8, false, false),
        (true, 5, false, false),
        (false, 5, false, false),
        (true, 9, false, false),
        (false, 9, false, false),
        (false, 9, true, false),
        (true, 3, false, true),
        (false, 3, false, false),
        (true, 0, false, false),
    ];
    let out = trace(
        &src,
        steps.len(),
        |i, s| {
            set(s, "X0", i == 0);
            set(s, "X6", steps[i].0);
            set_word(s, "D10", steps[i].1);
            set(s, "X7", steps[i].2);
            set(s, "X4", steps[i].3);
        },
        |s| (word(s, "D20"), bit(s, "Y0")),
    );
    let st: Vec<i16> = out.iter().map(|o| o.0).collect();
    assert_eq!(st, [1, 8, 8, 8, 8, 8, 0, 3, 3, 0], "{out:?}");
    assert!(out[6].1, "TR8 ends an 8-step chain: {out:?}");
    assert!(!out[7].1 && !out[9].1, "JUMP clears DONE: {out:?}");
}

#[test]
fn shift_reg_shifts_on_rising_edges_and_resets() {
    let src = prog(
        "SHIFT_REG",
        "DATA := X0, CLK := X1, RST := X2",
        &[
            ("o_Q0", "Y0", "BOOL"),
            ("o_Q1", "Y1", "BOOL"),
            ("o_Q15", "Y2", "BOOL"),
            ("o_OUT", "D20", "WORD"),
        ],
    );
    let program = plc::compile_st(&src).unwrap_or_else(|e| panic!("{e:?}\n{src}"));
    let mut sim = Simulator::default();
    let mut step = |data: bool, clk: bool, rst: bool| {
        set(&mut sim, "X0", data);
        set(&mut sim, "X1", clk);
        set(&mut sim, "X2", rst);
        sim.scan(&program, 100);
        (
            bit(&sim, "Y0"),
            bit(&sim, "Y1"),
            bit(&sim, "Y2"),
            word(&sim, "D20"),
        )
    };
    assert_eq!(
        step(true, true, false),
        (true, false, false, 1),
        "edge on first scan"
    );
    assert_eq!(
        step(false, true, false),
        (true, false, false, 1),
        "held CLK"
    );
    assert_eq!(step(false, false, false), (true, false, false, 1));
    assert_eq!(step(false, true, false), (false, true, false, 2));
    for k in 2..16 {
        step(false, false, false);
        let o = step(false, true, false);
        assert_eq!(o.3 as u16, 1u16 << k, "after {k} shifts");
    }
    assert_eq!(step(false, false, false), (false, false, true, i16::MIN));
    assert_eq!(
        step(false, true, false),
        (false, false, false, 0),
        "bit 15 drops out"
    );
    for _ in 0..16 {
        step(true, false, false);
        step(true, true, false);
    }
    assert_eq!(step(true, false, false), (true, true, true, -1));
    assert_eq!(
        step(true, true, true),
        (false, false, false, 0),
        "RST beats CLK"
    );
    assert_eq!(step(true, true, false), (false, false, false, 0), "no edge");
}

#[test]
fn hoa_selects_run_command_and_mode() {
    let src = prog(
        "HOA",
        "HAND := X0, AUTO := X1, AUTO_CMD := X2",
        &[
            ("o_Q", "Y0", "BOOL"),
            ("o_MODE", "D20", "INT"),
            ("o_FAULT", "Y1", "BOOL"),
        ],
    );
    let program = plc::compile_st(&src).unwrap();
    // (hand, auto, cmd) -> (Q, MODE, FAULT)
    let cases = [
        ((false, false, false), (false, 0, false)),
        ((false, false, true), (false, 0, false)),
        ((true, false, false), (true, 1, false)),
        ((true, false, true), (true, 1, false)),
        ((false, true, false), (false, 2, false)),
        ((false, true, true), (true, 2, false)),
        ((true, true, true), (false, 0, true)),
        ((true, true, false), (false, 0, true)),
    ];
    for ((h, a, c), expect) in cases {
        let mut sim = Simulator::default();
        set(&mut sim, "X0", h);
        set(&mut sim, "X1", a);
        set(&mut sim, "X2", c);
        sim.scan(&program, 100);
        assert_eq!(
            (bit(&sim, "Y0"), word(&sim, "D20"), bit(&sim, "Y1")),
            expect,
            "hand {h} auto {a} cmd {c}"
        );
    }
}

#[test]
fn batch_count_completes_batches_and_resets() {
    let src = prog(
        "BATCH_COUNT",
        "IN := X0, BATCH_SIZE := 3, RST := X1",
        &[
            ("o_Q", "Y0", "BOOL"),
            ("o_COUNT", "D20", "INT"),
            ("o_BATCHES", "D22", "DINT"),
        ],
    );
    // IN TRUE on even scans (rising edges at 2, 4, 6, ...), RST on scan 15.
    let out = trace(
        &src,
        18,
        |i, s| {
            set(s, "X0", i % 2 == 0);
            set(s, "X1", i == 15);
        },
        |s| (bit(s, "Y0"), word(s, "D20"), word(s, "D22")),
    );
    assert_eq!(
        out[0],
        (false, 0, 0),
        "IN TRUE on the first scan is not counted"
    );
    assert_eq!(out[2], (false, 1, 0));
    assert_eq!(out[4], (false, 2, 0));
    assert_eq!(out[6], (true, 0, 1), "batch complete: {out:?}");
    assert_eq!(out[7], (false, 0, 1), "Q lasts one scan");
    assert_eq!(out[12], (true, 0, 2), "{out:?}");
    assert_eq!(out[14], (false, 1, 2), "{out:?}");
    assert_eq!(out[15], (false, 0, 0), "RST clears: {out:?}");
    assert_eq!(out[16], (false, 1, 0), "{out:?}");
    let done = out.iter().filter(|o| o.0).count();
    assert_eq!(done, 2);
}

#[test]
fn batch_count_suspends_below_one_and_completes_when_size_is_lowered() {
    let src = prog(
        "BATCH_COUNT",
        "IN := X0, BATCH_SIZE := size, RST := FALSE",
        &[
            ("o_Q", "Y0", "BOOL"),
            ("o_COUNT", "D20", "INT"),
            ("o_BATCHES", "D22", "DINT"),
        ],
    )
    .replace("VAR b", "VAR size AT D10 : INT; b");
    let sizes = [5, 5, 5, 5, 5, 0, 0, 0, 0, 2, 2, 2];
    let out = trace(
        &src,
        sizes.len(),
        |i, s| {
            set(s, "X0", i % 2 == 1);
            set_word(s, "D10", sizes[i]);
        },
        |s| (bit(s, "Y0"), word(s, "D20"), word(s, "D22")),
    );
    assert_eq!(out[3], (false, 2, 0), "{out:?}");
    assert_eq!(out[7], (false, 2, 0), "size 0 suspends counting: {out:?}");
    assert_eq!(
        out[9],
        (true, 0, 1),
        "size lowered below the count: {out:?}"
    );
    assert_eq!(out[11], (false, 1, 1), "{out:?}");
}

#[test]
fn batch_count_keeps_its_totals_in_latched_memory() {
    let src = prog(
        "BATCH_COUNT",
        "IN := X0, BATCH_SIZE := 10, RST := FALSE",
        &[("o_COUNT", "D20", "INT")],
    );
    let lowered = crate::st::lower(&src).unwrap_or_else(|e| panic!("{e:?}"));
    let kept: Vec<&crate::st::StSymbol> = lowered.symbols.iter().filter(|s| s.retained).collect();
    assert_eq!(kept.len(), 2, "{:?}", lowered.symbols);
    for s in kept {
        let index: u16 = s.address[1..].parse().unwrap();
        let width = if s.data_type == crate::st::DataType::Dint {
            2
        } else {
            1
        };
        assert!(
            s.address.starts_with('D') && crate::alloc::Retain::word_is_latched(index, width),
            "{s:?}"
        );
    }
}

#[test]
fn cam4_windows_including_wrap_around_and_empty() {
    let src = prog(
        "CAM4",
        "POS := INT_TO_DINT(pos), ON1 := 100, OFF1 := 200, ON2 := 350, OFF2 := 50, ON4 := -50, OFF4 := 50",
        &[
            ("o_Q1", "Y0", "BOOL"),
            ("o_Q2", "Y1", "BOOL"),
            ("o_Q3", "Y2", "BOOL"),
            ("o_Q4", "Y3", "BOOL"),
        ],
    )
    .replace("VAR b", "VAR pos AT D10 : INT; b");
    let program = plc::compile_st(&src).unwrap_or_else(|e| panic!("{e:?}\n{src}"));
    // pos -> (Q1, Q2, Q3, Q4); window 3 has ON3 = OFF3 = 0 (never on).
    let cases = [
        (-60, (false, true, false, false)),
        (-50, (false, true, false, true)),
        (0, (false, true, false, true)),
        (49, (false, true, false, true)),
        (50, (false, false, false, false)),
        (99, (false, false, false, false)),
        (100, (true, false, false, false)),
        (199, (true, false, false, false)),
        (200, (false, false, false, false)),
        (349, (false, false, false, false)),
        (350, (false, true, false, false)),
        (3600, (false, true, false, false)),
    ];
    for (pos, expect) in cases {
        let mut sim = Simulator::default();
        set_word(&mut sim, "D10", pos);
        sim.scan(&program, 100);
        let got = (
            bit(&sim, "Y0"),
            bit(&sim, "Y1"),
            bit(&sim, "Y2"),
            bit(&sim, "Y3"),
        );
        assert_eq!(got, expect, "pos {pos}");
    }
}

const FREQ_OUT: &[(&str, &str, &str)] = &[
    ("o_OUT", "D20", "REAL"),
    ("o_RPM", "D22", "REAL"),
    ("o_ZERO", "Y0", "BOOL"),
];

#[test]
fn freq_meter_measures_hz_and_rpm_and_reports_standstill() {
    let src = prog(
        "FREQ_METER",
        "IN := X0, GATE_MS := 1000, PULSES_PER_REV := 5",
        FREQ_OUT,
    );
    // 5 Hz (one rising edge every 2 scans of 100 ms) until scan 21, then nothing.
    let out = trace(
        &src,
        32,
        |i, s| set(s, "X0", i < 21 && i % 2 == 0),
        |s| (real(s, "D20"), real(s, "D22"), bit(s, "Y0")),
    );
    assert_eq!(out[9], (0.0, 0.0, false), "no result before the first gate");
    assert_eq!(out[10], (5.0, 60.0, false), "{out:?}");
    assert_eq!(out[20], (5.0, 60.0, false), "{out:?}");
    assert_eq!(out[29], (5.0, 60.0, false), "held between gates");
    assert_eq!(
        out[30],
        (0.0, 0.0, true),
        "standstill after a gate without pulses"
    );
}

#[test]
fn freq_meter_slow_signal_short_gate_and_no_rpm_without_ppr() {
    let src = prog(
        "FREQ_METER",
        "IN := X0, GATE_MS := gate, PULSES_PER_REV := 0",
        FREQ_OUT,
    )
    .replace("VAR b", "VAR gate AT D10 : INT; b");
    // 2.5 Hz: one rising edge every 4 scans.
    let out = trace(
        &src,
        40,
        |i, s| {
            set_word(s, "D10", 2000);
            set(s, "X0", i % 4 == 1);
        },
        |s| (real(s, "D20"), real(s, "D22")),
    );
    assert_eq!(out[20], (2.5, 0.0), "{out:?}");
    assert_eq!(out[39].1, 0.0);
    // GATE_MS below 100 acts as 100: a result every scan.
    let out = trace(
        &src,
        6,
        |i, s| {
            set_word(s, "D10", 0);
            set(s, "X0", i % 2 == 1);
        },
        |s| real(s, "D20"),
    );
    assert_eq!(out[1..], [10.0, 0.0, 10.0, 0.0, 10.0], "{out:?}");
}

#[test]
fn freq_meter_stays_accurate_across_the_time_base_restart() {
    let src = prog(
        "FREQ_METER",
        "IN := X0, GATE_MS := 1000, PULSES_PER_REV := 1",
        FREQ_OUT,
    );
    let out = trace(
        &src,
        700,
        |i, s| set(s, "X0", i % 2 == 0),
        |s| real(s, "D20"),
    );
    for (i, f) in out.iter().enumerate().skip(10) {
        assert!((4.0..=6.0).contains(f), "scan {i}: {f}");
    }
}

#[test]
fn week_timer_windows_days_overnight_and_faults() {
    let src = prog(
        "WEEK_TIMER",
        "HOUR := hh, MINUTE := mm, DOW := dw, \
         DAYS1 := 16#1F, ON1_H := 7, ON1_MIN := 30, OFF1_H := 17, OFF1_MIN := 0, \
         DAYS2 := 16#10, ON2_H := 22, ON2_MIN := 0, OFF2_H := 6, OFF2_MIN := 0, \
         DAYS3 := 16#7F, ON3_H := 25, ON3_MIN := 0, OFF3_H := 23, OFF3_MIN := 0, \
         DAYS4 := 16#7F, ON4_H := 12, ON4_MIN := 0, OFF4_H := 13, OFF4_MIN := 0",
        &[
            ("o_Q", "Y0", "BOOL"),
            ("o_ACTIVE", "D20", "INT"),
            ("o_FAULT", "Y1", "BOOL"),
        ],
    )
    .replace(
        "VAR b",
        "VAR hh AT D10 : INT; mm AT D11 : INT; dw AT D12 : INT; b",
    );
    let program = plc::compile_st(&src).unwrap_or_else(|e| panic!("{e:?}\n{src}"));
    // (dow, hour, minute) -> (Q, ACTIVE, FAULT)
    let cases = [
        ((1, 7, 29), (false, 0, false)),
        ((1, 7, 30), (true, 1, false)),
        ((1, 16, 59), (true, 1, false)),
        ((1, 17, 0), (false, 0, false)),
        ((1, 12, 30), (true, 1, false)),
        ((6, 12, 30), (true, 4, false)),
        ((7, 12, 59), (true, 4, false)),
        ((7, 13, 0), (false, 0, false)),
        ((6, 7, 30), (false, 0, false)),
        ((5, 21, 59), (false, 0, false)),
        ((5, 22, 0), (true, 2, false)),
        ((5, 23, 59), (true, 2, false)),
        ((6, 0, 0), (true, 2, false)),
        ((6, 5, 59), (true, 2, false)),
        ((6, 6, 0), (false, 0, false)),
        ((5, 5, 0), (false, 0, false)),
        ((6, 23, 0), (false, 0, false)),
        ((1, 0, 0), (false, 0, false)),
        ((0, 12, 0), (false, 0, true)),
        ((8, 12, 0), (false, 0, true)),
        ((1, 24, 0), (false, 0, true)),
        ((1, -1, 0), (false, 0, true)),
        ((1, 12, 60), (false, 0, true)),
    ];
    for ((dw, hh, mm), expect) in cases {
        let mut sim = Simulator::default();
        set_word(&mut sim, "D10", hh);
        set_word(&mut sim, "D11", mm);
        set_word(&mut sim, "D12", dw);
        sim.scan(&program, 100);
        assert_eq!(
            (bit(&sim, "Y0"), word(&sim, "D20"), bit(&sim, "Y1")),
            expect,
            "dow {dw} {hh:02}:{mm:02}"
        );
    }
}

#[test]
fn watchdog_rst_held_restarts_the_timeout_on_release() {
    let src = prog(
        "WATCHDOG",
        "ENABLE := TRUE, IN := FALSE, PT := 300, RST := X2",
        &[("o_FAULT", "Y0", "BOOL")],
    );
    // The signal never changes; RST is held for scans 0..9.
    let out = trace(&src, 20, |i, s| set(s, "X2", i < 10), |s| bit(s, "Y0"));
    assert!(out[..10].iter().all(|b| !*b), "no FAULT while RST: {out:?}");
    assert!(
        out[10..13].iter().all(|b| !*b),
        "a full timeout after RST is released: {out:?}"
    );
    let f = first(&out).expect("no fault after release");
    assert!((13..=15).contains(&f), "fault at {f}: {out:?}");
}

#[test]
fn step_chain_hold_blocks_start_and_et_saturates() {
    let src = prog(
        "STEP_CHAIN",
        "START := X0, TR1 := FALSE, STEPS := 2, TMO_MS := 32767, HOLD := X4",
        CHAIN_OUT,
    );
    let out = trace(
        &src,
        380,
        |i, s| {
            set(s, "X0", i == 1 || i == 5);
            set(s, "X4", i < 4);
        },
        |s| (word(s, "D20"), word(s, "D21"), bit(s, "Y1")),
    );
    assert!(
        out[..5].iter().all(|o| *o == (0, 0, false)),
        "START during HOLD is ignored: {:?}",
        &out[..6]
    );
    assert_eq!(out[5], (1, 0, false), "a new START after HOLD starts");
    assert!(!out[300].2, "no timeout yet: {:?}", out[300]);
    assert_eq!(
        out[379],
        (1, 32767, true),
        "ET stops at 32767, TMO_MS = 32767 is reachable"
    );
}

#[test]
fn week_timer_window_may_end_at_24_00() {
    let src = prog(
        "WEEK_TIMER",
        "HOUR := hh, MINUTE := mm, DOW := dw, \
         DAYS1 := 16#01, ON1_H := 0, ON1_MIN := 0, OFF1_H := 24, OFF1_MIN := 0, \
         DAYS2 := 16#7F, ON2_H := 22, ON2_MIN := 0, OFF2_H := 24, OFF2_MIN := 30, \
         DAYS3 := 16#7F, ON3_H := 20, ON3_MIN := 0, OFF3_H := 24, OFF3_MIN := 0",
        &[("o_ACTIVE", "D20", "INT")],
    )
    .replace(
        "VAR b",
        "VAR hh AT D10 : INT; mm AT D11 : INT; dw AT D12 : INT; b",
    );
    let program = plc::compile_st(&src).unwrap_or_else(|e| panic!("{e:?}\n{src}"));
    // (dow, hour, minute) -> ACTIVE; window 2 (24:30) is invalid and never on.
    let cases = [
        ((1, 0, 0), 1),
        ((1, 12, 0), 1),
        ((1, 23, 59), 1),
        ((2, 0, 0), 0),
        ((2, 19, 59), 0),
        ((2, 20, 0), 3),
        ((2, 23, 59), 3),
        ((3, 0, 0), 0),
    ];
    for ((dw, hh, mm), expect) in cases {
        let mut sim = Simulator::default();
        set_word(&mut sim, "D10", hh);
        set_word(&mut sim, "D11", mm);
        set_word(&mut sim, "D12", dw);
        sim.scan(&program, 100);
        assert_eq!(word(&sim, "D20"), expect, "dow {dw} {hh:02}:{mm:02}");
    }
}

/// Declarations of the free variables used by the documented example of each logic block.
fn example_decls(block: &str) -> Option<&'static str> {
    Some(match block {
        "R_TRIG" => "cnt : INT;",
        "CLK_PRG" => "secs : INT;",
        "BLINK" => "alarm : BOOL;",
        "DEBOUNCE" => "level_ok : BOOL;",
        "TONOF" => "hot : BOOL;",
        "TP_RETRIG" => "moving : BOOL;",
        "TOF_SEC" => "burner : BOOL;",
        "TONR" => "valve_open : BOOL; full : BOOL;",
        "WATCHDOG" => "running : BOOL;",
        "SHIFT_REG" => "bad_part : BOOL;",
        "HOA" => "pump_auto : BOOL;",
        "CAM4" => "angle : DINT;",
        "FREQ_METER" => "speed : REAL;",
        "WEEK_TIMER" => "hh : INT; mm : INT; wd : INT;",
        "TON_DSEC" => "hmi_ds : INT;",
        "OP_MODE" => "home : BOOL; cyc_end : BOOL;",
        "RECIPE_STEP" => {
            "cyc_on : BOOL; w_mode : INT; w_ds : INT; act : INT; at_tgt : BOOL; idx : INT;"
        }
        "F_TRIG" | "SR" | "RS" | "TOGGLE" | "EDGE" | "MAJORITY" | "PULSE_EXT" | "TON_SEC"
        | "CTU_SW" | "CTD_SW" | "CTUD_SW" | "BATCH_COUNT" | "INTERLOCK" | "SEQUENCER"
        | "STEP_CHAIN" => "dummy : BOOL;",
        _ => return None,
    })
}

fn example_line<'a>(doc: &'a str, marker: &str) -> &'a str {
    let line = doc
        .lines()
        .find(|l| l.trim_start().starts_with(marker))
        .unwrap_or_else(|| panic!("no {marker} line in\n{doc}"));
    line.trim_start()[marker.len()..].trim()
}

#[test]
fn every_logic_example_compiles_and_matches_in_both_languages() {
    let mut checked = 0;
    for e in crate::st::library_entries()
        .into_iter()
        .filter(|e| e.category == "logic")
    {
        let ex = example_line(&e.doc, "Example:");
        assert_eq!(ex, example_line(&e.doc_tr, "Örnek:"), "{}", e.name);
        let extra = example_decls(&e.name)
            .unwrap_or_else(|| panic!("add the example declarations of {}", e.name));
        let inst = if e.kind == "FUNCTION" {
            assert!(ex.contains(&format!("{}(", e.name)), "{}: {ex}", e.name);
            String::new()
        } else {
            let name = ex.split('(').next().unwrap().trim();
            format!(" {name} : {};", e.name)
        };
        let src = format!("PROGRAM P\nVAR{inst} {extra} END_VAR\n{ex}\nEND_PROGRAM");
        plc::compile_st(&src).unwrap_or_else(|err| panic!("{}: {err:?}\n{src}", e.name));
        checked += 1;
    }
    assert_eq!(checked, 32, "logic blocks checked");
}

// ---------------------------------------------------------------------------------------
// First-scan and limit behaviour documented for the existing timers.

#[test]
fn tonof_with_in_true_at_power_up_switches_on_after_on_ms() {
    let src = prog(
        "TONOF",
        "IN := X0, ON_MS := 300, OFF_MS := 500",
        &[("o_Q", "Y0", "BOOL")],
    );
    let q = bool_trace(&src, &[true; 6]);
    assert_eq!(q, [false, false, false, true, true, true]);
}

#[test]
fn tonof_with_equal_times_behaves_like_debounce() {
    let x0 = pattern(&[
        (true, 2),
        (false, 1),
        (true, 6),
        (false, 2),
        (true, 1),
        (false, 6),
    ]);
    let tf = prog(
        "TONOF",
        "IN := X0, ON_MS := 300, OFF_MS := 300",
        &[("o_Q", "Y0", "BOOL")],
    );
    let db = prog("DEBOUNCE", "IN := X0, PT := 300", &[("o_Q", "Y0", "BOOL")]);
    assert_eq!(bool_trace(&tf, &x0), bool_trace(&db, &x0));
}

#[test]
fn pulse_ext_counts_in_true_on_the_first_scan_as_an_edge() {
    let src = prog("PULSE_EXT", "IN := X0, PT := 400", &[("o_Q", "Y0", "BOOL")]);
    let x0 = pattern(&[(true, 1), (false, 7)]);
    let q = bool_trace(&src, &x0);
    assert_eq!(q, pattern(&[(true, 4), (false, 4)]), "{q:?}");
}

#[test]
fn tonr_starts_at_zero_and_accumulates_from_the_first_scan() {
    let src = prog(
        "TONR",
        "IN := X0, RST := FALSE, PT := 500",
        &[("o_Q", "Y0", "BOOL"), ("o_ET", "D20", "INT")],
    );
    let out = trace(
        &src,
        7,
        |_, s| set(s, "X0", true),
        |s| (bit(s, "Y0"), word(s, "D20")),
    );
    assert_eq!(out[0], (false, 0), "{out:?}");
    assert_eq!(out[4], (false, 400), "{out:?}");
    assert_eq!(out[5], (true, 500), "PT ms after the first scan: {out:?}");
}

#[test]
fn watchdog_counts_from_the_first_call_with_enable() {
    let src = prog(
        "WATCHDOG",
        "ENABLE := X0, IN := X1, PT := 300, RST := FALSE",
        &[("o_FAULT", "Y0", "BOOL")],
    );
    // IN FALSE from power-up: the timeout runs from the first scan.
    let out = trace(&src, 6, |_, s| set(s, "X0", true), |s| bit(s, "Y0"));
    assert_eq!(first(&out), Some(3), "{out:?}");
    // IN TRUE on the first scan counts as a change: one scan later.
    let out = trace(
        &src,
        6,
        |_, s| {
            set(s, "X0", true);
            set(s, "X1", true);
        },
        |s| bit(s, "Y0"),
    );
    assert_eq!(first(&out), Some(4), "{out:?}");
    // ENABLE FALSE at first: the timeout starts with ENABLE.
    let out = trace(&src, 10, |i, s| set(s, "X0", i >= 4), |s| bit(s, "Y0"));
    assert_eq!(first(&out), Some(7), "{out:?}");
}

#[test]
fn tof_sec_with_zero_or_negative_preset_has_no_run_on() {
    for pt in [0, -3] {
        let src = prog(
            "TOF_SEC",
            "IN := X0, PT := pt",
            &[("o_Q", "Y0", "BOOL"), ("o_ET", "D20", "INT")],
        )
        .replace("VAR b", "VAR pt AT D10 : INT; b");
        let x0 = pattern(&[(false, 1), (true, 3), (false, 3), (true, 1), (false, 2)]);
        let out = trace(
            &src,
            x0.len(),
            |i, s| {
                set_word(s, "D10", pt);
                set(s, "X0", x0[i]);
            },
            |s| (bit(s, "Y0"), word(s, "D20")),
        );
        let q: Vec<bool> = out.iter().map(|o| o.0).collect();
        assert_eq!(q, x0, "PT {pt}: Q = IN");
        assert!(out.iter().all(|o| o.1 == 0), "PT {pt}: ET 0: {out:?}");
    }
}

// ---------------------------------------------------------------------------------------
// First-scan behaviour documented in the "First scan:" notes.

#[test]
fn sr_and_rs_act_by_level_on_the_first_scan() {
    let src = "PROGRAM P\nVAR a : SR; b : RS; o0 AT Y0 : BOOL; o1 AT Y1 : BOOL; END_VAR\na(SET1 := X0, RESET := X1); b(SET := X0, RESET1 := X1); o0 := a.Q1; o1 := b.Q1;\nEND_PROGRAM";
    // (set, reset) on the first scan -> (SR.Q1, RS.Q1)
    let cases = [
        ((false, false), (false, false)),
        ((true, false), (true, true)),
        ((true, true), (true, false)),
    ];
    for ((s, r), expect) in cases {
        let out = trace(
            src,
            1,
            |_, sim| {
                set(sim, "X0", s);
                set(sim, "X1", r);
            },
            |sim| (bit(sim, "Y0"), bit(sim, "Y1")),
        );
        assert_eq!(out[0], expect, "set {s}, reset {r}");
    }
}

#[test]
fn blink_with_enable_true_at_power_up_starts_high_at_once() {
    let src = prog(
        "BLINK",
        "ENABLE := X0, LOW_MS := 300, HIGH_MS := 200",
        &[("o_Q", "Y0", "BOOL")],
    );
    let q = bool_trace(&src, &[true; 15]);
    assert!(q[0], "TRUE in the first scan: {q:?}");
    assert_eq!(runs(&q), [2, 3, 2, 3, 2, 3], "{q:?}");
}

#[test]
fn ton_sec_with_in_true_at_power_up_starts_with_the_first_scan() {
    let src = prog(
        "TON_SEC",
        "IN := X0, PT := pt",
        &[("o_Q", "Y0", "BOOL"), ("o_ET", "D20", "INT")],
    )
    .replace("VAR b", "VAR pt AT D10 : INT; b");
    let out = trace(
        &src,
        25,
        |_, s| {
            set_word(s, "D10", 2);
            set(s, "X0", true);
        },
        |s| (bit(s, "Y0"), word(s, "D20")),
    );
    assert_eq!(out[0], (false, 0), "{out:?}");
    let q: Vec<bool> = out.iter().map(|o| o.0).collect();
    let on = first(&q).expect("Q never set");
    assert!((19..=21).contains(&on), "Q at scan {on}: {out:?}");
    assert_eq!(out[on], (true, 2), "{out:?}");
    for pt in [0, -5] {
        let out = trace(
            &src,
            2,
            |_, s| {
                set_word(s, "D10", pt);
                set(s, "X0", true);
            },
            |s| (bit(s, "Y0"), word(s, "D20")),
        );
        assert_eq!(out, [(true, 0), (true, 0)], "PT {pt}");
    }
}

#[test]
fn ctud_sw_counts_inputs_already_true_on_the_first_scan() {
    let src = prog(
        "CTUD_SW",
        "CU := X0, CD := X1, R := X2, LD := X3, PV := 5",
        &[("o_CV", "D20", "INT")],
    );
    // (cu, cd, r, ld) held from power-up -> CV in the first two scans
    let cases = [
        ((false, false, false, false), 0),
        ((true, false, false, false), 1),
        ((false, true, false, false), -1),
        ((true, true, false, false), 0),
        ((true, false, false, true), 5),
        ((true, false, true, true), 0),
    ];
    for ((cu, cd, r, ld), cv) in cases {
        let out = trace(
            &src,
            2,
            |_, s| {
                set(s, "X0", cu);
                set(s, "X1", cd);
                set(s, "X2", r);
                set(s, "X3", ld);
            },
            |s| word(s, "D20"),
        );
        assert_eq!(out, [cv, cv], "cu {cu}, cd {cd}, r {r}, ld {ld}");
    }
}

#[test]
fn interlock_request_true_on_the_first_scan_switches_on_at_once() {
    let src = prog(
        "INTERLOCK",
        "IN1 := X0, IN2 := X1, IN3 := X2, IN4 := X3",
        &[
            ("o_ACTIVE", "D20", "INT"),
            ("o_Q2", "Y1", "BOOL"),
            ("o_Q3", "Y2", "BOOL"),
        ],
    );
    let out = trace(
        &src,
        2,
        |_, s| {
            set(s, "X1", true);
            set(s, "X2", true);
        },
        |s| (word(s, "D20"), bit(s, "Y1"), bit(s, "Y2")),
    );
    assert_eq!(
        out,
        [(2, true, false), (2, true, false)],
        "lowest number wins"
    );
}

#[test]
fn sequencer_with_run_true_at_power_up_is_in_step_1_at_once() {
    let src = prog(
        "SEQUENCER",
        "RUN := X0, CONT := FALSE, T1_MS := 200, T2_MS := 0",
        &[("o_STEP", "D20", "INT"), ("o_Q1", "Y1", "BOOL")],
    );
    let out = trace(
        &src,
        1,
        |_, s| set(s, "X0", true),
        |s| (word(s, "D20"), bit(s, "Y1")),
    );
    assert_eq!(out[0], (1, true));
}

// ---------------------------------------------------------------------------------------
// TON_DSEC

const DSEC_OUT: &[(&str, &str, &str)] = &[
    ("o_Q", "Y0", "BOOL"),
    ("o_ET", "D20", "INT"),
    ("o_REM", "D21", "INT"),
];

fn dsec_src() -> String {
    prog("TON_DSEC", "IN := X0, PT := pt", DSEC_OUT).replace("VAR b", "VAR pt AT D10 : INT; b")
}

/// (Q, ET, REM) per scan; `inp(i)` gives (IN, PT) for scan `i`.
fn dsec_trace(scans: usize, inp: impl Fn(usize) -> (bool, i16)) -> Vec<(bool, i16, i16)> {
    trace(
        &dsec_src(),
        scans,
        |i, s| {
            let (x, pt) = inp(i);
            set(s, "X0", x);
            set_word(s, "D10", pt);
        },
        |s| (bit(s, "Y0"), word(s, "D20"), word(s, "D21")),
    )
}

#[test]
fn ton_dsec_short_delay_counts_tenths_and_remaining_time() {
    let out = dsec_trace(30, |_| (true, 25));
    assert!(!out[0].0, "IN TRUE on the first scan: Q FALSE");
    let on = out.iter().position(|o| o.0).expect("Q never set");
    assert!((24..=25).contains(&on), "Q at scan {on}: {out:?}");
    assert!(out[..24].iter().all(|o| !o.0), "{out:?}");
    assert_eq!((out[on].1, out[on].2), (25, 0));
    for (i, o) in out[..on].iter().enumerate() {
        assert_eq!(o.2, 25 - o.1, "REM = PT - ET at scan {i}");
        assert_eq!(o.1 as usize, i, "ET in 0.1 s at scan {i}");
    }
    assert!(out[on..].iter().all(|o| *o == (true, 25, 0)), "{out:?}");
}

#[test]
fn ton_dsec_long_delay_loses_at_most_one_scan_per_30_s() {
    let out = dsec_trace(6030, |_| (true, 6000));
    let on = out.iter().position(|o| o.0).expect("Q never set");
    assert!((6000..=6020).contains(&on), "Q at scan {on}");
    for w in out[..on].windows(2) {
        assert!(w[1].1 >= w[0].1, "ET never decreases: {w:?}");
    }
    // 30 s laps: ET has not lost more than one scan per lap so far.
    assert!(out[3010].1 >= 3000, "ET at scan 3010: {}", out[3010].1);
    assert_eq!(out[on], (true, 6000, 0));
}

#[test]
fn ton_dsec_in_false_resets_at_once_and_restarts_from_zero() {
    let out = dsec_trace(40, |i| (i != 20, 50));
    assert_eq!(out[19], (false, 19, 31));
    assert_eq!(out[20], (false, 0, 50), "IN FALSE: Q FALSE, ET 0, REM = PT");
    assert_eq!(out[21], (false, 0, 50), "restart counts from 0");
    assert_eq!(out[31], (false, 10, 40));
}

#[test]
fn ton_dsec_zero_or_negative_preset_follows_in() {
    for pt in [0, -5] {
        let x0 = [false, true, true, false, true, false];
        let out = dsec_trace(x0.len(), |i| (x0[i], pt));
        let q: Vec<bool> = out.iter().map(|o| o.0).collect();
        assert_eq!(q, x0, "PT {pt}");
        assert!(out.iter().all(|o| o.1 == 0 && o.2 == 0), "PT {pt}: {out:?}");
    }
}

#[test]
fn ton_dsec_preset_lowered_below_et_switches_at_once() {
    let out = dsec_trace(50, |i| (true, if i < 40 { 100 } else { 30 }));
    assert_eq!(out[39], (false, 39, 61));
    assert_eq!(out[40], (true, 30, 0), "PT lowered below ET");
    assert_eq!(out[45], (true, 30, 0));
}

#[test]
#[ignore = "33 000 scans, about a minute in a debug build"]
fn ton_dsec_maximum_preset_reaches_q_without_overflow() {
    let out = dsec_trace(33_000, |_| (true, 32767));
    let on = out.iter().position(|o| o.0).expect("Q never set");
    assert!((32767..=32767 + 110).contains(&on), "Q at scan {on}");
    for w in out.windows(2) {
        assert!(w[0].1 >= 0 && w[0].2 >= 0, "{w:?}");
        assert!(w[1].1 >= w[0].1, "ET never decreases: {w:?}");
    }
    assert_eq!(out[on], (true, 32767, 0));
}

// ---------------------------------------------------------------------------------------
// OP_MODE

#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Op {
    mode: i16,
    estop: bool,
    auto: bool,
    manual: bool,
    pendant: bool,
    settled: bool,
    req: bool,
    left: bool,
    cyc: bool,
}

#[derive(Clone, Copy, Default)]
struct OpIn {
    ok: bool,
    rst: bool,
    auto: bool,
    pend: bool,
    start: bool,
    stop: bool,
    home: bool,
    end: bool,
    cont: bool,
}

const OP_OUT: &[(&str, &str, &str)] = &[
    ("o_MODE", "D20", "INT"),
    ("o_ESTOP", "Y0", "BOOL"),
    ("o_AUTO", "Y1", "BOOL"),
    ("o_MANUAL", "Y2", "BOOL"),
    ("o_PENDANT", "Y3", "BOOL"),
    ("o_SETTLED", "Y4", "BOOL"),
    ("o_AUTO_REQ", "Y5", "BOOL"),
    ("o_AUTO_LEFT", "Y6", "BOOL"),
    ("o_CYCLE_ON", "Y7", "BOOL"),
];

/// Runs OP_MODE with the inputs on X0..X7, X10 plus `extra` arguments.
fn op_trace(extra: &str, scans: usize, inp: impl Fn(usize) -> OpIn) -> Vec<Op> {
    let call = format!(
        "ESTOP_OK := X0, RST := X1, AUTO_SEL := X2, PENDANT_SEL := X3, START := X4, \
         STOP := X5, HOME_OK := X6, CYCLE_END := X7, CONT := X10{extra}"
    );
    let src = prog("OP_MODE", &call, OP_OUT);
    trace(
        &src,
        scans,
        |i, s| {
            let v = inp(i);
            for (d, x) in [
                ("X0", v.ok),
                ("X1", v.rst),
                ("X2", v.auto),
                ("X3", v.pend),
                ("X4", v.start),
                ("X5", v.stop),
                ("X6", v.home),
                ("X7", v.end),
                ("X10", v.cont),
            ] {
                set(s, d, x);
            }
        },
        |s| Op {
            mode: word(s, "D20"),
            estop: bit(s, "Y0"),
            auto: bit(s, "Y1"),
            manual: bit(s, "Y2"),
            pendant: bit(s, "Y3"),
            settled: bit(s, "Y4"),
            req: bit(s, "Y5"),
            left: bit(s, "Y6"),
            cyc: bit(s, "Y7"),
        },
    )
}

fn ok_home() -> OpIn {
    OpIn {
        ok: true,
        home: true,
        ..OpIn::default()
    }
}

#[test]
fn op_mode_power_up_needs_a_reset_edge_and_gives_no_pulses() {
    let out = op_trace("", 10, |i| OpIn {
        auto: true,
        start: true,
        rst: i == 5,
        ..ok_home()
    });
    assert!(out[..5].iter().all(|o| o.estop && o.mode == 0), "{out:?}");
    assert!(out[..5].iter().all(|o| !o.auto && !o.manual && !o.settled));
    assert_eq!(out[5].mode, 3, "RST edge clears: {:?}", out[5]);
    assert!(out[5..].iter().all(|o| !o.estop && o.auto && o.mode == 3));
    assert!(out.iter().all(|o| !o.req && !o.left && !o.cyc), "{out:?}");
    // An RST held from power-up is not an edge.
    let out = op_trace("", 6, |_| OpIn {
        rst: true,
        ..ok_home()
    });
    assert!(out.iter().all(|o| o.estop && o.mode == 0), "{out:?}");
}

#[test]
fn op_mode_without_reset_need_follows_the_chain_at_once() {
    let out = op_trace(", RST_NEEDED := FALSE", 6, |i| OpIn {
        auto: true,
        ok: i != 3,
        home: true,
        ..OpIn::default()
    });
    assert_eq!(out[0].mode, 3, "{:?}", out[0]);
    assert!(out[3].estop && out[3].mode == 0);
    assert!(
        !out[4].estop && out[4].mode == 3,
        "no reset needed: {:?}",
        out[4]
    );
    assert!(out.iter().all(|o| !o.req), "{out:?}");
}

#[test]
fn op_mode_selector_edges_and_pendant() {
    let auto = |i: usize| (3..6).contains(&i);
    let out = op_trace(", RST_NEEDED := FALSE", 9, |i| OpIn {
        auto: auto(i),
        pend: i >= 4,
        ..ok_home()
    });
    let req: Vec<usize> = (0..9).filter(|i| out[*i].req).collect();
    let left: Vec<usize> = (0..9).filter(|i| out[*i].left).collect();
    assert_eq!(req, [3], "{out:?}");
    assert_eq!(left, [6], "{out:?}");
    let modes: Vec<i16> = out.iter().map(|o| o.mode).collect();
    assert_eq!(modes, [1, 1, 1, 3, 3, 3, 2, 2, 2]);
    assert!(out[4].pendant && out[4].auto, "PENDANT also TRUE in auto");
    assert!(out[7].pendant && out[7].manual && !out[7].auto);
    assert!(!out[2].pendant && out[2].manual);
}

#[test]
fn op_mode_estop_drops_everything_and_needs_a_new_reset_edge() {
    let out = op_trace(", SETTLE_MS := 0", 14, |i| OpIn {
        auto: true,
        pend: true,
        ok: !(4..7).contains(&i),
        rst: i == 1 || (5..9).contains(&i) || i == 10,
        start: i == 2,
        ..ok_home()
    });
    assert!(out[0].estop);
    assert!(
        !out[1].estop && out[1].mode == 3 && !out[1].req,
        "{:?}",
        out[1]
    );
    assert!(out[2].cyc && out[3].cyc && out[3].settled && out[3].pendant);
    let e = out[4];
    assert!(e.estop && e.mode == 0, "{e:?}");
    assert!(
        !e.auto && !e.manual && !e.pendant && !e.settled && !e.cyc,
        "{e:?}"
    );
    assert!(
        out[7..10].iter().all(|o| o.estop),
        "RST held across the return does not reset: {out:?}"
    );
    assert!(
        !out[10].estop && out[10].mode == 3,
        "new RST edge: {:?}",
        out[10]
    );
    assert!(
        out.iter().all(|o| !o.req),
        "no AUTO_REQ when the e-stop clears"
    );
    assert!(out[10..].iter().all(|o| !o.cyc), "{out:?}");
}

#[test]
fn op_mode_settle_time_restarts_on_selector_changes_and_estop_end() {
    let out = op_trace(", RST_NEEDED := FALSE, SETTLE_MS := 1000", 110, |i| OpIn {
        auto: (20..60).contains(&i),
        pend: i >= 40,
        ok: !(80..83).contains(&i),
        ..ok_home()
    });
    let settled: Vec<bool> = out.iter().map(|o| o.settled).collect();
    let off_after = |c: usize| (c..110).take_while(|i| !settled[*i]).count();
    assert!((10..=11).contains(&off_after(0)), "power-up: {settled:?}");
    for c in [20, 40, 60] {
        assert!(settled[c - 1], "settled before the change at {c}");
        let n = off_after(c);
        assert!((10..=11).contains(&n), "change at {c}: off for {n} scans");
    }
    let n = off_after(80);
    assert!((13..=14).contains(&n), "e-stop 3 scans + settle: {n}");
    // SETTLE_MS 0: SETTLED = NOT ESTOP, also in the scan of a change.
    let out = op_trace(", RST_NEEDED := FALSE, SETTLE_MS := 0", 10, |i| OpIn {
        auto: i % 2 == 1,
        pend: i % 3 == 0,
        ok: i != 6,
        ..ok_home()
    });
    for (i, o) in out.iter().enumerate() {
        assert_eq!(o.settled, !o.estop, "scan {i}");
    }
}

#[test]
fn op_mode_cycle_start_stop_end_and_continuous() {
    // (start, stop, home, end, cont, auto) per scan.
    let steps = [
        (false, false, false, false, false, true), // 0
        (true, false, false, false, false, true),  // 1: no HOME_OK
        (false, false, true, false, false, true),  // 2
        (true, false, true, false, false, true),   // 3: start
        (false, false, true, false, false, true),  // 4
        (false, true, true, false, false, true),   // 5: stop
        (true, true, true, false, false, true),    // 6: start while stop
        (true, false, true, false, false, true),   // 7: stuck start, no edge
        (false, false, true, false, false, true),  // 8
        (true, false, true, false, false, true),   // 9: start
        (true, false, true, true, false, true),    // 10: cycle end
        (true, false, true, false, false, true),   // 11: start stuck
        (true, false, true, false, false, true),   // 12
        (false, false, true, false, false, true),  // 13
        (true, false, true, false, true, true),    // 14: start, continuous
        (true, false, true, true, true, true),     // 15: cycle end ignored
        (false, false, true, false, true, true),   // 16
        (false, false, true, false, true, false),  // 17: leave auto
        (true, false, true, false, false, true),   // 18: back, start edge
        (false, false, true, false, false, true),  // 19
        (false, false, true, false, false, true),  // 20
        (false, false, true, false, false, false), // 21: leave auto
        (false, false, true, false, false, true),  // 22
        (true, false, true, true, false, true),    // 23: start and end edges
    ];
    let out = op_trace(", RST_NEEDED := FALSE", steps.len(), |i| {
        let (start, stop, home, end, cont, auto) = steps[i];
        OpIn {
            start,
            stop,
            home,
            end,
            cont,
            auto,
            ..ok_home()
        }
    });
    let cyc: Vec<bool> = out.iter().map(|o| o.cyc).collect();
    let expect = [
        false, false, false, true, true, false, false, false, false, true, false, false, false,
        false, true, true, true, false, true, true, true, false, false, false,
    ];
    assert_eq!(cyc, expect);
}

#[test]
fn op_mode_start_wired_to_the_selector_starts_in_the_same_scan() {
    let call = ", RST_NEEDED := FALSE, START := X2";
    let out = op_trace(call, 6, |i| OpIn {
        auto: i >= 3,
        ..ok_home()
    });
    assert!(out[..3].iter().all(|o| !o.cyc));
    assert!(out[3].cyc && out[3].req && out[3].mode == 3, "{:?}", out[3]);
    assert!(out[5].cyc);
}

// ---------------------------------------------------------------------------------------
// RECIPE_STEP

#[derive(Clone, Copy)]
struct RsIn {
    run: bool,
    trig: bool,
    at: bool,
    rst: bool,
    mode: i16,
    ds: i16,
    act: i16,
    tmo: i16,
}

impl Default for RsIn {
    fn default() -> Self {
        RsIn {
            run: true,
            trig: false,
            at: false,
            rst: false,
            mode: 1,
            ds: 0,
            act: 3,
            tmo: 0,
        }
    }
}

/// (CMD, STATE, DONE, FAULT, FAULT_CODE) per scan.
type Rs = (i16, i16, bool, bool, i16);

fn rs_trace(extra: &str, scans: usize, inp: impl Fn(usize) -> RsIn) -> Vec<Rs> {
    let call = format!(
        "RUN := X0, TRIG := X1, AT_TARGET := X2, RST := X3, WAIT_MODE := wm, WAIT_DS := wd, \
         ACTION := ac, TMO_MS := tm{extra}"
    );
    let src = prog(
        "RECIPE_STEP",
        &call,
        &[
            ("o_CMD", "D20", "INT"),
            ("o_STATE", "D21", "INT"),
            ("o_DONE", "Y0", "BOOL"),
            ("o_FAULT", "Y1", "BOOL"),
            ("o_FAULT_CODE", "D22", "INT"),
        ],
    )
    .replace(
        "VAR b",
        "VAR wm AT D10 : INT; wd AT D11 : INT; ac AT D12 : INT; tm AT D13 : INT; b",
    );
    trace(
        &src,
        scans,
        |i, s| {
            let v = inp(i);
            set(s, "X0", v.run);
            set(s, "X1", v.trig);
            set(s, "X2", v.at);
            set(s, "X3", v.rst);
            set_word(s, "D10", v.mode);
            set_word(s, "D11", v.ds);
            set_word(s, "D12", v.act);
            set_word(s, "D13", v.tmo);
        },
        |s| {
            (
                word(s, "D20"),
                word(s, "D21"),
                bit(s, "Y0"),
                bit(s, "Y1"),
                word(s, "D22"),
            )
        },
    )
}

#[test]
fn recipe_step_trigger_mode_waits_for_a_new_pedal_edge() {
    let out = rs_trace("", 12, |i| RsIn {
        run: i >= 3,
        trig: i < 5 || i == 6,
        at: i == 9,
        mode: 0,
        ..RsIn::default()
    });
    assert_eq!(out[2], (0, 0, false, false, 0));
    assert_eq!(out[3], (0, 1, false, false, 0), "RUN rises: waiting");
    assert_eq!(
        out[5],
        (0, 1, false, false, 0),
        "TRIG held before RUN: no start"
    );
    assert_eq!(out[6], (0, 2, false, false, 0), "TRIG edge");
    assert_eq!(out[7], (3, 2, false, false, 0), "CMD in the next call");
    assert_eq!(out[8], (3, 2, false, false, 0));
    assert_eq!(out[9], (3, 3, true, false, 0), "DONE with CMD still set");
    assert_eq!(out[10], (0, 1, false, false, 0), "next step waits, CMD 0");
    assert_eq!(out[11], (0, 1, false, false, 0));
}

#[test]
fn recipe_step_time_mode_waits_wait_ds() {
    let out = rs_trace("", 30, |_| RsIn {
        mode: 1,
        ds: 20,
        act: 2,
        ..RsIn::default()
    });
    let c = out.iter().position(|o| o.0 == 2).expect("no CMD");
    assert!((20..=22).contains(&c), "CMD at scan {c}: {out:?}");
    assert!(out[..c].iter().all(|o| o.1 == 1 || o.1 == 2));
}

#[test]
fn recipe_step_without_wait_gives_cmd_in_the_second_call() {
    for ds in [0, -5] {
        let out = rs_trace("", 6, |i| RsIn {
            run: i >= 2,
            ds,
            act: 4,
            ..RsIn::default()
        });
        assert_eq!(out[2], (0, 2, false, false, 0), "WAIT_DS {ds}");
        assert_eq!(out[3], (4, 2, false, false, 0), "WAIT_DS {ds}");
    }
}

#[test]
fn recipe_step_empty_action_completes_in_two_calls_without_fault() {
    let out = rs_trace("", 6, |_| RsIn {
        mode: 7,
        act: 0,
        ..RsIn::default()
    });
    assert_eq!(out[0], (0, 2, false, false, 0));
    assert_eq!(out[1], (0, 3, true, false, 0));
    assert_eq!(out[2], (0, 2, false, false, 0), "next step at once");
    assert_eq!(out[3], (0, 3, true, false, 0));
}

#[test]
fn recipe_step_invalid_wait_mode_faults_until_corrected() {
    let out = rs_trace("", 8, |i| RsIn {
        mode: if i < 5 { 5 } else { 1 },
        act: 2,
        ..RsIn::default()
    });
    assert!(
        out[..5].iter().all(|o| *o == (0, 1, false, true, 1)),
        "{out:?}"
    );
    assert_eq!(out[5], (0, 2, false, false, 0), "corrected: proceeds");
    assert_eq!(out[6], (2, 2, false, false, 0));
}

#[test]
fn recipe_step_invalid_action_faults_until_corrected() {
    let acts = [11, 11, 11, 4, 4, -1, -1, 4];
    let out = rs_trace("", acts.len(), |i| RsIn {
        act: acts[i],
        ..RsIn::default()
    });
    assert_eq!(out[0], (0, 2, false, false, 0));
    assert_eq!(out[1], (0, 2, false, true, 2), "ACTION 11 > MAX_ACTION 10");
    assert_eq!(out[2], (0, 2, false, true, 2));
    assert_eq!(out[3], (4, 2, false, false, 0), "corrected");
    assert_eq!(out[5], (0, 2, false, true, 2), "ACTION -1");
    assert_eq!(out[7], (4, 2, false, false, 0));
    // A smaller MAX_ACTION.
    let out = rs_trace(", MAX_ACTION := 3", 3, |_| RsIn {
        act: 4,
        ..RsIn::default()
    });
    assert_eq!(out[2], (0, 2, false, true, 2));
}

#[test]
fn recipe_step_timeout_latches_until_rst_and_then_retries() {
    let out = rs_trace("", 40, |i| RsIn {
        act: 5,
        tmo: 1000,
        at: i == 15 || i == 30 || i == 34,
        rst: i == 17 || i == 32,
        ..RsIn::default()
    });
    assert_eq!(out[1], (5, 2, false, false, 0));
    let f = out.iter().position(|o| o.4 == 3).expect("no timeout");
    assert!((11..=13).contains(&f), "timeout at {f}: {out:?}");
    assert!(
        out[f..17].iter().all(|o| *o == (0, 2, false, true, 3)),
        "{out:?}"
    );
    assert!(!out[15].2, "AT_TARGET ignored while timed out");
    assert_eq!(out[17], (5, 2, false, false, 0), "RST: CMD back");
    let f2 = (18..40)
        .find(|i| out[*i].4 == 3)
        .expect("no second timeout");
    assert!((28..=30).contains(&f2), "fresh timeout after RST: {f2}");
    assert!(!out[30].2, "{out:?}");
    assert_eq!(out[32], (5, 2, false, false, 0));
    assert_eq!(
        out[34],
        (5, 3, true, false, 0),
        "AT_TARGET after RST finishes"
    );
}

#[test]
fn recipe_step_timeout_does_not_carry_over_to_the_next_step() {
    // Step 1 (ACTION 5) times out; the program then skips it by pointing at an empty step
    // (ACTION 0), which completes it. The next step (ACTION 4) must start without a fault.
    let out = rs_trace("", 24, |i| RsIn {
        act: match i {
            0..=14 => 5,
            15 => 0,
            _ => 4,
        },
        tmo: 1000,
        ..RsIn::default()
    });
    assert_eq!(out[14], (0, 2, false, true, 3), "{out:?}");
    assert_eq!(
        out[15],
        (0, 3, true, false, 0),
        "empty step completes: {out:?}"
    );
    assert_eq!(out[16], (0, 2, false, false, 0), "{out:?}");
    assert_eq!(
        out[17],
        (4, 2, false, false, 0),
        "fresh step, no fault: {out:?}"
    );
}

#[test]
fn recipe_step_run_false_resets_everything_in_the_same_scan() {
    let out = rs_trace("", 30, |i| RsIn {
        run: i != 6 && i != 25,
        act: if i < 10 { 12 } else { 5 },
        tmo: 500,
        ..RsIn::default()
    });
    assert_eq!(out[5], (0, 2, false, true, 2));
    assert_eq!(out[6], (0, 0, false, false, 0), "RUN FALSE with code 2");
    let f = (10..25).find(|i| out[*i].4 == 3).expect("no timeout");
    assert!(out[f..25].iter().all(|o| o.4 == 3));
    assert_eq!(out[25], (0, 0, false, false, 0), "RUN FALSE clears code 3");
    assert_eq!(out[26], (0, 2, false, false, 0));
    assert_eq!(out[27], (5, 2, false, false, 0), "fresh start");
}

#[test]
fn recipe_step_trigger_held_at_power_up_does_not_start() {
    let out = rs_trace("", 6, |i| RsIn {
        mode: 0,
        trig: i < 5,
        ..RsIn::default()
    });
    assert!(out.iter().all(|o| *o == (0, 1, false, false, 0)), "{out:?}");
}

#[test]
fn recipe_step_without_timeout_never_faults() {
    let out = rs_trace("", 400, |_| RsIn {
        act: 9,
        tmo: 0,
        ..RsIn::default()
    });
    assert!(out[1..].iter().all(|o| *o == (9, 2, false, false, 0)));
}

#[test]
fn recipe_step_runs_a_table_with_no_idle_scan_between_steps() {
    let src = "PROGRAM P
VAR rcp : RECIPE_STEP; idx : INT; a : INT; m : INT; d : INT;
    cmd AT D20 : INT; st AT D21 : INT; ix AT D23 : INT; END_VAR
CASE idx OF
    0: a := 3; m := 1; d := 0;
    1: a := 0; m := 7; d := 0;
    2: a := 7; m := 1; d := 2;
ELSE
    a := 0;
END_CASE;
rcp(RUN := idx < 3, TRIG := FALSE, WAIT_MODE := m, WAIT_DS := d, ACTION := a,
    AT_TARGET := TRUE, TMO_MS := 0, RST := FALSE);
IF rcp.DONE THEN idx := idx + 1; END_IF;
cmd := rcp.CMD; st := rcp.STATE; ix := idx;
END_PROGRAM";
    let out = trace(
        src,
        10,
        |_, _| {},
        |s| (word(s, "D20"), word(s, "D21"), word(s, "D23")),
    );
    let cmd: Vec<i16> = out.iter().map(|o| o.0).collect();
    let st: Vec<i16> = out.iter().map(|o| o.1).collect();
    assert_eq!(cmd, [0, 3, 0, 0, 0, 0, 0, 7, 0, 0], "{out:?}");
    assert_eq!(st, [2, 3, 2, 3, 1, 1, 2, 3, 0, 0], "{out:?}");
    assert_eq!(out[9].2, 3);
}

// ---------------------------------------------------------------------------------------
// Sizes of the new blocks (words per call and jump labels), checked against the docs.

fn size_of(block: &str, call: &str) -> (usize, usize) {
    let src = |n: usize| {
        let decls: String = (0..n).map(|k| format!(" b{k} : {block};")).collect();
        let calls: String = (0..n).map(|k| format!("b{k}({call});\n")).collect();
        format!("PROGRAM P\nVAR{decls} END_VAR\n{calls}END_PROGRAM")
    };
    let words = |n: usize| {
        // Measured with every call expanded in place, as the Size: lines describe it.
        let program =
            plc::compile_st_with(&src(n), &crate::st::LowerOptions { share: crate::st::Share::Never, ..Default::default() }).unwrap_or_else(|e| panic!("{e:?}"));
        crate::es2_encoder::encode(&program).unwrap().words.len()
    };
    let labels = |n: usize| {
        crate::st::lower_with(&src(n), &crate::st::LowerOptions { share: crate::st::Share::Never, ..Default::default() })
            .unwrap()
            .il
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|t| {
                t.len() > 1 && t.starts_with('P') && t[1..].bytes().all(|b| b.is_ascii_digit())
            })
            .map(str::to_string)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    };
    (words(2) - words(1), labels(2) - labels(1))
}

/// The number after "Size: " (or "Size: about ") in the English doc of `block`.
fn documented_size(block: &str) -> usize {
    let e = crate::st::library_entries()
        .into_iter()
        .find(|e| e.name == block)
        .unwrap();
    let at = e.doc.find("Size: ").expect("no Size: line") + "Size: ".len();
    e.doc[at..]
        .trim_start_matches("about ")
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
fn new_logic_blocks_stay_within_their_size_budget() {
    let cases = [
        ("TON_DSEC", "IN := X0, PT := 50", 400, 4),
        (
            "OP_MODE",
            "ESTOP_OK := X0, RST := X1, AUTO_SEL := X2, START := X4",
            300,
            4,
        ),
        (
            "RECIPE_STEP",
            "RUN := X0, TRIG := X1, WAIT_MODE := 0, WAIT_DS := 0, ACTION := 1, AT_TARGET := X2, TMO_MS := 0, RST := X3",
            900,
            8,
        ),
    ];
    for (block, call, max_words, max_labels) in cases {
        let (w, l) = size_of(block, call);
        println!("{block}: {w} words per call, {l} labels");
        assert!(w <= max_words, "{block}: {w} words");
        assert!(l <= max_labels, "{block}: {l} labels");
        let doc = documented_size(block);
        assert!(
            doc.abs_diff(w) <= w / 10,
            "{block}: doc says {doc}, measured {w}"
        );
    }
}
