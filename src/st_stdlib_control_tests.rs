//! Behavioural tests of the ST standard library, category "control" (src/st/stdlib/control.st).
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

fn set_real(sim: &mut Simulator, device: &str, v: f32) {
    let d: plc::Device = device.parse().unwrap();
    let bits = v.to_bits();
    let next: plc::Device = format!("D{}", d.index + 1).parse().unwrap();
    sim.memory.set_word(d, bits as u16 as i16);
    sim.memory.set_word(next, (bits >> 16) as u16 as i16);
}

fn compile(source: &str) -> plc::Program {
    plc::compile_st(source).unwrap_or_else(|e| panic!("{e:?}\n{source}"))
}

/// Runs one 100 ms scan per element of `inputs`, writing each REAL into D10 first and
/// collecting `probe` after the scan.
fn trace<T>(source: &str, inputs: &[f32], probe: impl Fn(&Simulator) -> T) -> Vec<T> {
    let program = compile(source);
    let mut sim = Simulator::default();
    inputs
        .iter()
        .map(|&v| {
            set_real(&mut sim, "D10", v);
            sim.scan(&program, 100);
            probe(&sim)
        })
        .collect()
}

#[test]
fn hysteresis_switches_at_the_levels_and_holds_in_between() {
    let src = "PROGRAM P\nVAR h : HYSTERESIS; x AT D10 : REAL; q AT Y0 : BOOL; w AT Y1 : BOOL; END_VAR\n\
h(IN := x, LIM_H := 60.0, LIM_L := 40.0); q := h.Q; w := h.WIN;\nEND_PROGRAM";
    let seen = trace(
        src,
        &[50.0, 59.9, 60.0, 50.0, 40.1, 40.0, 50.0, 70.0],
        |s| (bit(s, "Y0"), bit(s, "Y1")),
    );
    assert_eq!(
        seen,
        [
            (false, true),
            (false, true),
            (true, false),
            (true, true),
            (true, true),
            (false, false),
            (false, true),
            (true, false)
        ]
    );
}

#[test]
fn hysteresis_first_scan_and_crossed_levels() {
    let src = "PROGRAM P\nVAR h : HYSTERESIS; x AT D10 : REAL; q AT Y0 : BOOL; w AT Y1 : BOOL; END_VAR\n\
h(IN := x, LIM_H := 60.0, LIM_L := 40.0); q := h.Q; w := h.WIN;\nEND_PROGRAM";
    // First scan: Q is FALSE unless IN >= LIM_H.
    assert_eq!(trace(src, &[59.9], |s| bit(s, "Y0")), [false]);
    assert_eq!(trace(src, &[60.0], |s| bit(s, "Y0")), [true]);
    // LIM_L >= LIM_H: Q is simply IN >= LIM_H and WIN stays FALSE.
    let src = src.replace(
        "LIM_H := 60.0, LIM_L := 40.0",
        "LIM_H := 50.0, LIM_L := 60.0",
    );
    let seen = trace(&src, &[55.0, 50.0, 49.9, 55.0, 70.0, 10.0], |s| {
        (bit(s, "Y0"), bit(s, "Y1"))
    });
    let q: Vec<bool> = seen.iter().map(|v| v.0).collect();
    assert_eq!(q, [true, true, false, true, true, false]);
    assert!(seen.iter().all(|v| !v.1), "{seen:?}");
}

#[test]
fn two_point_heats_with_hysteresis() {
    let src = "PROGRAM P\nVAR c : TWO_POINT; x AT D10 : REAL; q AT Y0 : BOOL; END_VAR\n\
c(PV := x, SP := 20.0, HYST := 2.0); q := c.Q;\nEND_PROGRAM";
    // Two-point rule: on at PV <= SP - HYST/2 = 19, off at PV >= SP + HYST/2 = 21.
    let seen = trace(
        src,
        &[20.0, 19.1, 19.0, 20.0, 20.9, 21.0, 20.0, 18.9],
        |s| bit(s, "Y0"),
    );
    assert_eq!(seen, [false, false, true, true, true, false, false, true]);
}

#[test]
fn two_point_cooling_inverts_the_action() {
    let src = "PROGRAM P\nVAR c : TWO_POINT; x AT D10 : REAL; q AT Y0 : BOOL; END_VAR\n\
c(PV := x, SP := 20.0, HYST := 2.0, COOL := TRUE); q := c.Q;\nEND_PROGRAM";
    let seen = trace(src, &[20.9, 21.0, 20.0, 19.1, 19.0, 20.0], |s| bit(s, "Y0"));
    assert_eq!(seen, [false, true, true, true, false, false]);
}

#[test]
fn two_point_zero_or_negative_hysteresis_switches_at_the_setpoint() {
    let src = "PROGRAM P\nVAR c : TWO_POINT; x AT D10 : REAL; q AT Y0 : BOOL; END_VAR\n\
c(PV := x, SP := 20.0, HYST := -4.0); q := c.Q;\nEND_PROGRAM";
    // Negative HYST counts as 0: on at PV <= 20 (on wins at PV = SP), off above.
    let seen = trace(src, &[20.1, 20.0, 20.1, 19.9], |s| bit(s, "Y0"));
    assert_eq!(seen, [false, true, false, true]);
}

#[test]
fn two_point_respects_minimum_on_and_off_times() {
    let src = "PROGRAM P\nVAR c : TWO_POINT; x AT D10 : REAL; q AT Y0 : BOOL; l AT Y1 : BOOL; END_VAR\n\
c(ENABLE := NOT X0, PV := x, SP := 20.0, HYST := 2.0, MIN_ON_MS := 500, MIN_OFF_MS := 300); q := c.Q; l := c.LOCKED;\nEND_PROGRAM";
    // Cold at once (first switch is free), then hot immediately: must stay on 500 ms;
    // cold again right after switching off: must stay off 300 ms.
    let mut inputs = vec![10.0];
    inputs.extend([30.0; 5]);
    inputs.extend([10.0; 6]);
    let seen = trace(src, &inputs, |s| (bit(s, "Y0"), bit(s, "Y1")));
    let q: Vec<bool> = seen.iter().map(|v| v.0).collect();
    assert_eq!(
        q,
        [
            true, true, true, true, true, // on 500 ms
            false, false, false, // off 300 ms
            true, true, true, true
        ]
    );
    assert!(
        seen[6].1 && seen[7].1,
        "a waiting switch-on is reported as LOCKED"
    );
    assert!(seen[1].1, "a waiting switch is reported as LOCKED");
    assert!(!seen[0].1);
    // Disabling turns the output off at once, regardless of the minimum on time.
    let program = compile(src);
    let mut sim = Simulator::default();
    set_real(&mut sim, "D10", 10.0);
    sim.scan(&program, 100);
    assert!(bit(&sim, "Y0"));
    sim.memory.set_input("X0".parse().unwrap(), true);
    sim.scan(&program, 100);
    assert!(!bit(&sim, "Y0"));
}

#[test]
fn limit_alarm_sets_and_clears_with_hysteresis() {
    let src = "PROGRAM P\nVAR a : LIMIT_ALARM; x AT D10 : REAL; hh AT Y0 : BOOL; h AT Y1 : BOOL; l AT Y2 : BOOL; ll AT Y3 : BOOL; any AT Y4 : BOOL; END_VAR\n\
a(IN := x, LIM_HH := 90.0, LIM_H := 80.0, LIM_L := 20.0, LIM_LL := 10.0, HYST := 2.0);\n\
hh := a.Q_HH; h := a.Q_H; l := a.Q_L; ll := a.Q_LL; any := a.Q;\nEND_PROGRAM";
    let seen = trace(
        src,
        &[
            50.0, 80.0, 90.0, 88.5, 87.9, 78.5, 77.9, 20.0, 10.0, 11.5, 12.1, 21.5, 22.1,
        ],
        |s| ["Y0", "Y1", "Y2", "Y3", "Y4"].map(|d| bit(s, d)),
    );
    let f = false;
    let t = true;
    assert_eq!(
        seen,
        [
            [f, f, f, f, f],
            [f, t, f, f, t],
            [t, t, f, f, t],
            [t, t, f, f, t],
            [f, t, f, f, t],
            [f, t, f, f, t],
            [f, f, f, f, f],
            [f, f, t, f, t],
            [f, f, t, t, t],
            [f, f, t, t, t],
            [f, f, t, f, t],
            [f, f, t, f, t],
            [f, f, f, f, f],
        ]
    );
}

#[test]
fn limit_alarm_treats_negative_hysteresis_as_zero() {
    // A negative HYST used to make the clear level lie above the set level (the alarm
    // then never latched consistently); it now counts as 0.
    let src = "PROGRAM P\nVAR a : LIMIT_ALARM; x AT D10 : REAL; h AT Y1 : BOOL; l AT Y2 : BOOL; any AT Y4 : BOOL; END_VAR\n\
a(IN := x, LIM_H := 80.0, LIM_L := 20.0, HYST := -5.0); h := a.Q_H; l := a.Q_L; any := a.Q;\nEND_PROGRAM";
    let seen = trace(src, &[80.0, 79.9, 80.0, 20.0, 20.1, 50.0], |s| {
        (bit(s, "Y1"), bit(s, "Y2"), bit(s, "Y4"))
    });
    let (f, t) = (false, true);
    assert_eq!(
        seen,
        [
            (t, f, t),
            (f, f, f),
            (t, f, t),
            (f, t, t),
            (f, f, f),
            (f, f, f)
        ]
    );
}

fn close(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

fn scans(program: &plc::Program, sim: &mut Simulator, n: usize) {
    for _ in 0..n {
        sim.scan(program, 100);
    }
}

fn set_x(sim: &mut Simulator, input: &str, on: bool) {
    sim.memory.set_input(input.parse().unwrap(), on);
}

const RAMP_SRC: &str = "PROGRAM P\nVAR r : RAMP; x AT D10 : REAL; o AT D20 : REAL; b AT Y0 : BOOL; END_VAR\n\
r(IN := x, RATE_S := 5.0, SET := X0); o := r.OUT; b := r.BUSY;\nEND_PROGRAM";

#[test]
fn ramp_limits_the_rate_per_second() {
    let program = compile(RAMP_SRC);
    let mut sim = Simulator::default();
    scans(&program, &mut sim, 1); // the first call takes OUT := IN = 0
    assert_eq!(real(&sim, "D20"), 0.0);
    set_real(&mut sim, "D10", 10.0);
    scans(&program, &mut sim, 10); // 10 samples of 100 ms -> 1 s -> 5 units
    assert!(close(real(&sim, "D20"), 5.0, 1e-3), "{}", real(&sim, "D20"));
    assert!(bit(&sim, "Y0"));
    scans(&program, &mut sim, 14);
    assert_eq!(real(&sim, "D20"), 10.0);
    assert!(!bit(&sim, "Y0"));
    // Falling target and SET.
    set_real(&mut sim, "D10", -2.0);
    scans(&program, &mut sim, 10);
    assert!(close(real(&sim, "D20"), 5.0, 1e-3), "{}", real(&sim, "D20"));
    set_x(&mut sim, "X0", true);
    scans(&program, &mut sim, 1);
    assert_eq!(real(&sim, "D20"), -2.0);
    assert!(!bit(&sim, "Y0"));
}

#[test]
fn ramp_starts_at_the_input_on_the_first_scan() {
    // No jump up from 0.0 at start-up: OUT = IN and BUSY FALSE from the first scan on.
    for n in [1, 2, 5] {
        let sim = run(RAMP_SRC, n, |s| set_real(s, "D10", 500.0));
        assert_eq!(real(&sim, "D20"), 500.0, "after {n} scans");
        assert!(!bit(&sim, "Y0"), "BUSY after {n} scans");
    }
}

#[test]
fn ramp_zero_or_negative_rate_freezes_the_output() {
    for rate in ["0.0", "-3.0"] {
        let src = RAMP_SRC.replace("RATE_S := 5.0", &format!("RATE_S := {rate}"));
        let seen = trace(&src, &[5.0, 10.0, 10.0, 10.0], |s| {
            (real(s, "D20"), bit(s, "Y0"))
        });
        assert_eq!(
            seen,
            [(5.0, false), (5.0, true), (5.0, true), (5.0, true)],
            "RATE_S {rate}"
        );
    }
}

#[test]
fn ramp_with_longer_sample_takes_bigger_steps() {
    let src = "PROGRAM P\nVAR r : RAMP; x AT D10 : REAL; o AT D20 : REAL; END_VAR\n\
r(IN := x, RATE_S := 2.0, SAMPLE_MS := 500); o := r.OUT;\nEND_PROGRAM";
    let program = compile(src);
    let mut sim = Simulator::default();
    scans(&program, &mut sim, 1); // OUT := IN = 0
    set_real(&mut sim, "D10", 100.0);
    // The first sample ends after 6 scans, later ones every 5 scans.
    scans(&program, &mut sim, 5);
    assert!(close(real(&sim, "D20"), 1.0, 1e-4), "{}", real(&sim, "D20"));
    scans(&program, &mut sim, 4);
    assert!(close(real(&sim, "D20"), 1.0, 1e-4));
    scans(&program, &mut sim, 1);
    assert!(close(real(&sim, "D20"), 2.0, 1e-4));
}

#[test]
fn setpoint_ramp_starts_at_pv_and_reaches_target() {
    let src = "PROGRAM P\nVAR r : SETPOINT_RAMP; o AT D20 : REAL; d AT Y0 : BOOL; END_VAR\n\
r(RUN := X0, TARGET := 20.0, PV := 10.0, RATE_MIN := 60.0); o := r.OUT; d := r.DONE;\nEND_PROGRAM";
    let program = compile(src);
    let mut sim = Simulator::default();
    scans(&program, &mut sim, 5);
    assert_eq!(real(&sim, "D20"), 10.0);
    assert!(!bit(&sim, "Y0"));
    set_x(&mut sim, "X0", true);
    scans(&program, &mut sim, 50); // 60/min = 1/s -> 5 s
    assert!(
        close(real(&sim, "D20"), 15.0, 0.11),
        "{}",
        real(&sim, "D20")
    );
    assert!(!bit(&sim, "Y0"));
    scans(&program, &mut sim, 60);
    assert_eq!(real(&sim, "D20"), 20.0);
    assert!(bit(&sim, "Y0"));
    set_x(&mut sim, "X0", false);
    scans(&program, &mut sim, 1);
    assert_eq!(real(&sim, "D20"), 10.0);
}

#[test]
fn setpoint_ramp_with_run_on_the_first_scan_starts_at_pv() {
    // RUN already TRUE at start-up (e.g. after a power cycle): the ramp must start at the
    // current process value, not at 0.0.
    let src = "PROGRAM P\nVAR r : SETPOINT_RAMP; o AT D20 : REAL; d AT Y0 : BOOL; END_VAR\n\
r(RUN := TRUE, TARGET := 20.0, PV := 150.0, RATE_MIN := 60.0); o := r.OUT; d := r.DONE;\nEND_PROGRAM";
    let seen = trace(src, &[0.0; 3], |s| (real(s, "D20"), bit(s, "Y0")));
    assert_eq!(seen[0], (150.0, false), "first scan");
    // 60/min = 0.1 per 100 ms sample, downwards towards TARGET; the first sample comes
    // 100 ms after the first call.
    assert!(close(seen[1].0, 149.9, 1e-3), "{seen:?}");
    assert!(close(seen[2].0, 149.8, 1e-3), "{seen:?}");
}

#[test]
fn setpoint_ramp_zero_or_negative_rate_jumps_to_the_target() {
    for rate in ["0.0", "-1.0"] {
        let src = format!(
            "PROGRAM P\nVAR r : SETPOINT_RAMP; o AT D20 : REAL; d AT Y0 : BOOL; END_VAR\n\
r(RUN := TRUE, TARGET := 20.0, PV := 10.0, RATE_MIN := {rate}); o := r.OUT; d := r.DONE;\nEND_PROGRAM"
        );
        let sim = run(&src, 1, |_| {});
        assert_eq!(real(&sim, "D20"), 20.0, "RATE_MIN {rate}");
        assert!(bit(&sim, "Y0"), "RATE_MIN {rate}");
    }
}

#[test]
fn lowpass_step_response_reaches_63_percent_after_one_time_constant() {
    let src = "PROGRAM P\nVAR f : LOWPASS; x AT D10 : REAL; o AT D20 : REAL; END_VAR\n\
f(IN := x, TAU_MS := 1000.0, SET := X0); o := f.OUT;\nEND_PROGRAM";
    let program = compile(src);
    let mut sim = Simulator::default();
    scans(&program, &mut sim, 1); // the first call initialises OUT := IN = 0
    assert_eq!(real(&sim, "D20"), 0.0);
    set_real(&mut sim, "D10", 100.0);
    scans(&program, &mut sim, 10); // 10 samples = 1000 ms
    let v = real(&sim, "D20");
    let expected = 100.0 * (1.0 - (1.0f32 / 1.1).powi(10));
    assert!(close(v, expected, 0.01), "{v} vs {expected}");
    assert!(v > 60.0 && v < 66.0, "{v}");
    scans(&program, &mut sim, 60);
    assert!(real(&sim, "D20") > 99.0);
    set_real(&mut sim, "D10", 0.0);
    scans(&program, &mut sim, 10); // 10 samples, ~39 % left
    assert!(
        close(real(&sim, "D20"), 100.0 - expected, 1.5),
        "{}",
        real(&sim, "D20")
    );
    set_real(&mut sim, "D10", 100.0);
    set_x(&mut sim, "X0", true);
    scans(&program, &mut sim, 1);
    assert_eq!(real(&sim, "D20"), 100.0);
}

#[test]
fn lowpass_starts_at_the_input_without_a_transient() {
    // OSCAT FT_PT1 semantics: a filter on a hot process value must not start at 0
    // (that would trip low alarms on the first scans).
    let src = "PROGRAM P\nVAR f : LOWPASS; o AT D20 : REAL; END_VAR\n\
f(IN := 500.0, TAU_MS := 5000.0); o := f.OUT;\nEND_PROGRAM";
    for n in [1, 2, 5] {
        let sim = run(src, n, |_| {});
        assert_eq!(real(&sim, "D20"), 500.0, "after {n} scans");
    }
}

#[test]
fn lowpass_with_zero_time_constant_passes_through() {
    let src = "PROGRAM P\nVAR f : LOWPASS; o AT D20 : REAL; END_VAR\n\
f(IN := 42.0, TAU_MS := 0.0); o := f.OUT;\nEND_PROGRAM";
    let sim = run(src, 2, |_| {});
    assert_eq!(real(&sim, "D20"), 42.0);
    // 0 or negative: OUT := IN at every sample (no filtering).
    for tau in ["0.0", "-500.0"] {
        let src = format!(
            "PROGRAM P\nVAR f : LOWPASS; x AT D10 : REAL; o AT D20 : REAL; END_VAR\n\
f(IN := x, TAU_MS := {tau}); o := f.OUT;\nEND_PROGRAM"
        );
        let seen = trace(&src, &[0.0, 100.0, 50.0, -7.0], |s| real(s, "D20"));
        assert_eq!(seen, [0.0, 100.0, 50.0, -7.0], "TAU_MS {tau}");
    }
}

#[test]
fn lowpass_time_constant_may_exceed_32767_ms() {
    let src = "PROGRAM P\nVAR f : LOWPASS; x AT D10 : REAL; o AT D20 : REAL; END_VAR\n\
f(IN := x, TAU_MS := 99900.0); o := f.OUT;\nEND_PROGRAM";
    let mut inputs = vec![0.0];
    inputs.extend([100.0; 10]);
    let seen = trace(src, &inputs, |s| real(s, "D20"));
    // 10 samples with a factor 100 / 100000 each.
    let expected = 100.0 * (1.0 - (1.0f32 - 0.001).powi(10));
    assert!(close(seen[10], expected, 1e-3), "{seen:?} vs {expected}");
}

#[test]
fn mov_avg_averages_the_last_n_samples() {
    let src = "PROGRAM P\nVAR m : MOV_AVG; x AT D10 : REAL; o AT D20 : REAL; c AT D22 : INT; END_VAR\n\
m(IN := x, N := 4, RST := X0); o := m.OUT; c := m.COUNT;\nEND_PROGRAM";
    let seen = trace(src, &[4.0, 8.0, 12.0, 16.0, 20.0, 24.0], |s| {
        (real(s, "D20"), word(s, "D22"))
    });
    assert_eq!(
        seen,
        [
            (4.0, 1),
            (6.0, 2),
            (8.0, 3),
            (10.0, 4),
            (14.0, 4),
            (18.0, 4)
        ]
    );
    let program = compile(src);
    let mut sim = Simulator::default();
    set_real(&mut sim, "D10", 10.0);
    scans(&program, &mut sim, 3);
    set_x(&mut sim, "X0", true);
    scans(&program, &mut sim, 1);
    assert_eq!((real(&sim, "D20"), word(&sim, "D22")), (0.0, 0));
    set_x(&mut sim, "X0", false);
    set_real(&mut sim, "D10", 2.0);
    scans(&program, &mut sim, 1);
    assert_eq!((real(&sim, "D20"), word(&sim, "D22")), (2.0, 1));
}

#[test]
fn mov_avg_clamps_the_window_to_16() {
    let src = "PROGRAM P\nVAR m : MOV_AVG; x AT D10 : REAL; o AT D20 : REAL; c AT D22 : INT; END_VAR\n\
m(IN := x, N := 40); o := m.OUT; c := m.COUNT;\nEND_PROGRAM";
    let inputs: Vec<f32> = (1..=20).map(|v| v as f32).collect();
    let seen = trace(src, &inputs, |s| (real(s, "D20"), word(s, "D22")));
    // Last 16 samples: 5..=20, mean 12.5.
    assert_eq!(*seen.last().unwrap(), (12.5, 16));
    // N below 1 is clamped to a window of 1: OUT = IN.
    let src = src.replace("N := 40", "N := 0");
    let seen = trace(&src, &[3.0, 7.0, -1.0], |s| {
        (real(s, "D20"), word(s, "D22"))
    });
    assert_eq!(seen, [(3.0, 1), (7.0, 1), (-1.0, 1)]);
}

#[test]
fn mov_avg_changing_n_restarts_the_window() {
    let src = "PROGRAM P\nVAR m : MOV_AVG; x AT D10 : REAL; n AT D30 : INT; o AT D20 : REAL; c AT D22 : INT; END_VAR\n\
m(IN := x, N := n); o := m.OUT; c := m.COUNT;\nEND_PROGRAM";
    let program = compile(src);
    let mut sim = Simulator::default();
    sim.memory.set_word("D30".parse().unwrap(), 4);
    for v in [2.0, 4.0, 6.0] {
        set_real(&mut sim, "D10", v);
        sim.scan(&program, 100);
    }
    assert_eq!((real(&sim, "D20"), word(&sim, "D22")), (4.0, 3));
    sim.memory.set_word("D30".parse().unwrap(), 2);
    set_real(&mut sim, "D10", 10.0);
    sim.scan(&program, 100);
    assert_eq!((real(&sim, "D20"), word(&sim, "D22")), (10.0, 1));
    set_real(&mut sim, "D10", 20.0);
    sim.scan(&program, 100);
    assert_eq!((real(&sim, "D20"), word(&sim, "D22")), (15.0, 2));
}

#[test]
fn integrator_integrates_per_second_and_clamps() {
    let src = "PROGRAM P\nVAR i : INTEGRATOR; o AT D20 : REAL; l AT Y0 : BOOL; END_VAR\n\
i(IN := 2.0, RST := X0, HOLD := X1, OUT_MAX := 3.0); o := i.OUT; l := i.LIM;\nEND_PROGRAM";
    let program = compile(src);
    let mut sim = Simulator::default();
    scans(&program, &mut sim, 11);
    assert!(close(real(&sim, "D20"), 2.0, 1e-4), "{}", real(&sim, "D20"));
    set_x(&mut sim, "X1", true);
    scans(&program, &mut sim, 5);
    assert!(close(real(&sim, "D20"), 2.0, 1e-4));
    set_x(&mut sim, "X1", false);
    scans(&program, &mut sim, 10);
    assert_eq!(real(&sim, "D20"), 3.0);
    assert!(bit(&sim, "Y0"));
    set_x(&mut sim, "X0", true);
    scans(&program, &mut sim, 1);
    assert_eq!(real(&sim, "D20"), 0.0);
    assert!(!bit(&sim, "Y0"));
}

#[test]
fn derivative_reports_rate_per_second() {
    let src = "PROGRAM P\nVAR d : DERIVATIVE; x AT D10 : REAL; o AT D20 : REAL; END_VAR\n\
d(IN := x); o := d.OUT;\nEND_PROGRAM";
    let seen = trace(src, &[0.0, 0.0, 1.0, 2.0, 3.0, 3.0], |s| real(s, "D20"));
    assert_eq!(seen, [0.0, 0.0, 10.0, 10.0, 10.0, 0.0]);
}

#[test]
fn derivative_reset_clears_the_output_and_forgets_the_last_sample() {
    let src = "PROGRAM P\nVAR d : DERIVATIVE; x AT D10 : REAL; o AT D20 : REAL; END_VAR\n\
d(IN := x, RST := X0); o := d.OUT;\nEND_PROGRAM";
    let program = compile(src);
    let mut sim = Simulator::default();
    for v in [0.0, 0.0, 1.0] {
        set_real(&mut sim, "D10", v);
        sim.scan(&program, 100);
    }
    assert_eq!(real(&sim, "D20"), 10.0);
    set_x(&mut sim, "X0", true);
    sim.scan(&program, 100);
    assert_eq!(real(&sim, "D20"), 0.0);
    // After RST the first sample only stores IN (no jump from the old LAST = 1.0); the
    // second one gives the rate again.
    set_x(&mut sim, "X0", false);
    set_real(&mut sim, "D10", 50.0);
    sim.scan(&program, 100);
    assert_eq!(real(&sim, "D20"), 0.0);
    set_real(&mut sim, "D10", 52.0);
    sim.scan(&program, 100);
    assert!(
        close(real(&sim, "D20"), 20.0, 1e-3),
        "{}",
        real(&sim, "D20")
    );
}

#[test]
fn integrator_reset_is_clamped_into_the_limits() {
    // RST: OUT := 0.0 clamped to OUT_MIN..OUT_MAX, so with OUT_MIN = 5 it gives 5.
    let src = "PROGRAM P\nVAR i : INTEGRATOR; o AT D20 : REAL; l AT Y0 : BOOL; END_VAR\n\
i(IN := 1.0, RST := TRUE, OUT_MIN := 5.0, OUT_MAX := 10.0); o := i.OUT; l := i.LIM;\nEND_PROGRAM";
    let sim = run(src, 3, |_| {});
    assert_eq!(real(&sim, "D20"), 5.0);
    assert!(bit(&sim, "Y0"));
}

#[test]
fn totalizer_counts_whole_units_and_fraction() {
    let src = "PROGRAM P\nVAR t : TOTALIZER; tot AT D20 : REAL; whole AT D24 : DINT; fr AT D26 : REAL; END_VAR\n\
t(FLOW := 5400.0, ENABLE := NOT X1, RST := X0); tot := t.TOTAL; whole := t.TOTAL_DINT; fr := t.FRAC;\nEND_PROGRAM";
    let program = compile(src);
    let mut sim = Simulator::default();
    scans(&program, &mut sim, 51); // 5 samples of 1 s at 1.5 units/s (first after 11 scans)
    assert!(close(real(&sim, "D20"), 7.5, 1e-3), "{}", real(&sim, "D20"));
    assert_eq!(word(&sim, "D24"), 7);
    assert!(close(real(&sim, "D26"), 0.5, 1e-3));
    set_x(&mut sim, "X1", true);
    scans(&program, &mut sim, 30);
    assert!(close(real(&sim, "D20"), 7.5, 1e-3));
    set_x(&mut sim, "X1", false);
    scans(&program, &mut sim, 51);
    assert!(
        close(real(&sim, "D20"), 15.0, 1e-3),
        "{}",
        real(&sim, "D20")
    );
    assert_eq!(word(&sim, "D24"), 15);
    set_x(&mut sim, "X0", true);
    scans(&program, &mut sim, 1);
    assert_eq!((real(&sim, "D20"), word(&sim, "D24")), (0.0, 0));
}

/// PID/PI around a first-order plant (gain 0.5, time constant 2 s) written in ST.
fn controller_loop(block: &str, extra: &str) -> String {
    format!(
        "PROGRAM P\nVAR c : {block}; pv AT D10 : REAL; y AT D20 : REAL; sp AT D30 : REAL; l AT Y0 : BOOL; END_VAR\n\
         IF X1 THEN sp := 1000.0; ELSE sp := 20.0; END_IF;\n\
         c(PV := pv, SP := sp, KP := 2.0, TI_MS := 2000.0{extra}); y := c.OUT;\n\
         pv := pv + (c.OUT * 0.5 - pv) * 0.05;\nEND_PROGRAM"
    )
}

#[test]
fn pid_drives_a_first_order_plant_to_setpoint() {
    let src = controller_loop("PID", ", TD_MS := 200.0, MANUAL := X0, MAN_OUT := 30.0");
    let program = compile(&src);
    let mut sim = Simulator::default();
    scans(&program, &mut sim, 600);
    assert!(
        close(real(&sim, "D10"), 20.0, 0.1),
        "pv {}",
        real(&sim, "D10")
    );
    assert!(
        close(real(&sim, "D20"), 40.0, 0.5),
        "y {}",
        real(&sim, "D20")
    );
}

#[test]
fn pid_manual_mode_is_bumpless() {
    let src = controller_loop("PID", ", MANUAL := X0, MAN_OUT := 30.0");
    let program = compile(&src);
    let mut sim = Simulator::default();
    set_x(&mut sim, "X0", true);
    scans(&program, &mut sim, 20);
    assert_eq!(real(&sim, "D20"), 30.0);
    set_x(&mut sim, "X0", false);
    scans(&program, &mut sim, 2);
    let y = real(&sim, "D20");
    assert!(close(y, 30.0, 1.5), "bump on manual -> auto: {y}");
    scans(&program, &mut sim, 600);
    assert!(
        close(real(&sim, "D10"), 20.0, 0.1),
        "pv {}",
        real(&sim, "D10")
    );
}

#[test]
fn pid_anti_windup_recovers_at_once_from_saturation() {
    let src = "PROGRAM P\nVAR c : PID; pv AT D10 : REAL; y AT D20 : REAL; sp : REAL; l AT Y0 : BOOL; END_VAR\n\
IF X1 THEN sp := 1000.0; ELSE sp := 20.0; END_IF;\n\
c(PV := pv, SP := sp, KP := 2.0, TI_MS := 2000.0, RST := X0); y := c.OUT; l := c.LIM;\n\
pv := pv + (c.OUT * 0.5 - pv) * 0.05;\nEND_PROGRAM";
    let program = compile(src);
    let mut sim = Simulator::default();
    set_x(&mut sim, "X1", true);
    scans(&program, &mut sim, 300);
    assert_eq!(real(&sim, "D20"), 100.0);
    assert!(bit(&sim, "Y0"));
    // pv is ~50 now; with SP back to 20 the output must leave the limit immediately.
    set_x(&mut sim, "X1", false);
    scans(&program, &mut sim, 2);
    assert!(real(&sim, "D20") < 1.0, "y {}", real(&sim, "D20"));
    set_x(&mut sim, "X0", true);
    scans(&program, &mut sim, 1);
    assert_eq!(real(&sim, "D20"), 0.0);
}

#[test]
fn pid_reset_has_priority_over_manual_and_man_out_is_clamped() {
    let src = "PROGRAM P\nVAR c : PID; y AT D20 : REAL; l AT Y0 : BOOL; END_VAR\n\
c(PV := 50.0, SP := 50.0, OUT_MIN := 10.0, OUT_MAX := 90.0, MANUAL := TRUE, MAN_OUT := 150.0, RST := X0); y := c.OUT; l := c.LIM;\nEND_PROGRAM";
    let sim = run(src, 1, |_| {});
    assert_eq!(
        (real(&sim, "D20"), bit(&sim, "Y0")),
        (90.0, true),
        "MAN_OUT clamped"
    );
    let sim = run(src, 2, |s| set_x(s, "X0", true));
    assert_eq!(real(&sim, "D20"), 10.0, "RST wins: 0.0 clamped to OUT_MIN");
}

#[test]
fn pid_direct_action_and_proportional_only() {
    let src = "PROGRAM P\nVAR c : PID; y AT D20 : REAL; e AT D22 : REAL; END_VAR\n\
c(PV := 4.0, SP := 10.0, KP := 2.0, TI_MS := 0.0, DIRECT := X0); y := c.OUT; e := c.E;\nEND_PROGRAM";
    let sim = run(src, 3, |_| {});
    assert_eq!((real(&sim, "D20"), real(&sim, "D22")), (12.0, 6.0));
    let sim = run(src, 3, |s| set_x(s, "X0", true));
    assert_eq!((real(&sim, "D20"), real(&sim, "D22")), (0.0, -6.0));
}

#[test]
fn pid_derivative_reacts_to_pv_changes_only() {
    let src = "PROGRAM P\nVAR c : PID; x AT D10 : REAL; y AT D20 : REAL; END_VAR\n\
c(PV := x, SP := 50.0, KP := 1.0, TI_MS := 0.0, TD_MS := 1000.0, OUT_MIN := -100.0); y := c.OUT;\nEND_PROGRAM";
    // PV rises 1 unit per 100 ms -> dPV/dt = 10/s -> D = -KP * TD * 10 = -10.
    let seen = trace(src, &[0.0, 0.0, 1.0, 2.0, 2.0], |s| real(s, "D20"));
    assert_eq!(seen[1], 50.0);
    assert!(close(seen[2], 49.0 - 10.0, 1e-3), "{seen:?}");
    assert!(close(seen[3], 48.0 - 10.0, 1e-3), "{seen:?}");
    assert!(close(seen[4], 48.0, 1e-3), "{seen:?}");
}

#[test]
fn pi_drives_a_first_order_plant_to_setpoint() {
    let src = controller_loop("PI", "");
    let program = compile(&src);
    let mut sim = Simulator::default();
    scans(&program, &mut sim, 600);
    assert!(
        close(real(&sim, "D10"), 20.0, 0.1),
        "pv {}",
        real(&sim, "D10")
    );
    // Saturated, then recovers immediately thanks to anti-windup.
    set_x(&mut sim, "X1", true);
    scans(&program, &mut sim, 300);
    assert_eq!(real(&sim, "D20"), 100.0);
    set_x(&mut sim, "X1", false);
    scans(&program, &mut sim, 2);
    assert!(real(&sim, "D20") < 1.0, "y {}", real(&sim, "D20"));
}

#[test]
fn pi_proportional_only_and_reset() {
    let src = "PROGRAM P\nVAR c : PI; y AT D20 : REAL; END_VAR\n\
c(PV := 4.0, SP := 10.0, KP := 2.0, TI_MS := 0.0, RST := X0); y := c.OUT;\nEND_PROGRAM";
    let sim = run(src, 2, |_| {});
    assert_eq!(real(&sim, "D20"), 12.0);
    let sim = run(src, 2, |s| set_x(s, "X0", true));
    assert_eq!(real(&sim, "D20"), 0.0);
}

#[test]
fn pi_and_pid_report_lim_at_either_output_limit() {
    for block in ["PI", "PID"] {
        let src = format!(
            "PROGRAM P\nVAR c : {block}; x AT D10 : REAL; y AT D20 : REAL; l AT Y0 : BOOL; END_VAR\n\
c(PV := x, SP := 50.0, KP := 1.0, TI_MS := 0.0); y := c.OUT; l := c.LIM;\nEND_PROGRAM"
        );
        // Scan 1 is before the first sample: OUT = 0.0 = OUT_MIN, so LIM is TRUE.
        let seen = trace(&src, &[0.0, 0.0, -100.0, 100.0, 40.0], |s| {
            (real(s, "D20"), bit(s, "Y0"))
        });
        assert_eq!(
            seen,
            [
                (0.0, true),
                (50.0, false),
                (100.0, true),
                (0.0, true),
                (10.0, false)
            ],
            "{block}"
        );
    }
}

#[test]
fn pi_and_pid_negative_times_disable_their_action() {
    // TI_MS < 0 = no integral action: the P-only output stays at KP * E.
    for block in ["PI", "PID"] {
        let src = format!(
            "PROGRAM P\nVAR c : {block}; y AT D20 : REAL; END_VAR\n\
c(PV := 4.0, SP := 10.0, KP := 2.0, TI_MS := -5.0); y := c.OUT;\nEND_PROGRAM"
        );
        let sim = run(&src, 30, |_| {});
        assert_eq!(real(&sim, "D20"), 12.0, "{block}");
    }
    // TD_MS < 0 = no derivative action.
    let src = "PROGRAM P\nVAR c : PID; x AT D10 : REAL; y AT D20 : REAL; END_VAR\n\
c(PV := x, SP := 50.0, KP := 1.0, TI_MS := 0.0, TD_MS := -1000.0, OUT_MIN := -100.0); y := c.OUT;\nEND_PROGRAM";
    let seen = trace(src, &[0.0, 0.0, 1.0, 2.0], |s| real(s, "D20"));
    assert_eq!(seen[1..], [50.0, 49.0, 48.0]);
}

#[test]
fn pid_integral_time_may_exceed_32767_ms() {
    // KP 1, E 10, TI 100000 ms: each 100 ms sample adds 10 * 100 / 100000 = 0.01.
    let src = "PROGRAM P\nVAR c : PID; y AT D20 : REAL; END_VAR\n\
c(PV := 40.0, SP := 50.0, KP := 1.0, TI_MS := 100000.0); y := c.OUT;\nEND_PROGRAM";
    let sim = run(src, 101, |_| {});
    assert!(
        close(real(&sim, "D20"), 11.0, 1e-3),
        "{}",
        real(&sim, "D20")
    );
}

fn pwm_pattern(duty: &str, n: usize) -> Vec<bool> {
    pwm_pattern_period(duty, 1000, n)
}

fn pwm_pattern_period(duty: &str, period: i32, n: usize) -> Vec<bool> {
    let src = format!(
        "PROGRAM P\nVAR p : PWM; q AT Y0 : BOOL; END_VAR\np(ENABLE := NOT X0, DUTY := {duty}, PT := {period}); q := p.Q;\nEND_PROGRAM"
    );
    let program = compile(&src);
    let mut sim = Simulator::default();
    (0..n)
        .map(|_| {
            sim.scan(&program, 100);
            bit(&sim, "Y0")
        })
        .collect()
}

#[test]
fn pwm_gives_duty_percent_of_each_period() {
    let q = pwm_pattern("30.0", 40);
    for period in q[10..].chunks(10) {
        assert_eq!(period.iter().filter(|b| **b).count(), 3, "{q:?}");
    }
    assert!(pwm_pattern("0.0", 30).iter().all(|b| !b));
    let full = pwm_pattern("100.0", 30);
    assert!(full.iter().all(|b| *b), "{full:?}");
    assert_eq!(pwm_pattern("150.0", 30), full);
    let half = pwm_pattern("50.0", 40);
    assert_eq!(half[10..].iter().filter(|b| **b).count(), 15, "{half:?}");
}

#[test]
fn pwm_disable_turns_q_off_and_restarts_the_period() {
    let src = "PROGRAM P\nVAR p : PWM; q AT Y0 : BOOL; END_VAR\n\
p(ENABLE := NOT X0, DUTY := 30.0, PT := 1000); q := p.Q;\nEND_PROGRAM";
    let program = compile(src);
    let mut sim = Simulator::default();
    let mut seen = Vec::new();
    for i in 0..12 {
        set_x(&mut sim, "X0", (5..7).contains(&i));
        sim.scan(&program, 100);
        seen.push(bit(&sim, "Y0"));
    }
    // Disabled in the middle of the off phase; re-enabling starts a new period at once.
    assert_eq!(
        seen,
        [
            true, true, true, false, false, false, false, true, true, true, false, false
        ]
    );
}

#[test]
fn pwm_rounds_the_on_time_to_the_nearest_100_ms() {
    // The first period counts too (it starts on the first call).
    let q = pwm_pattern("30.0", 10);
    assert_eq!(
        q,
        [
            true, true, true, false, false, false, false, false, false, false
        ]
    );
    // 4 % of 1000 ms is 40 ms: nearer to 0 than to 100 ms, so Q stays off
    // (it used to give a 100 ms pulse, i.e. 10 % instead of 4 %).
    assert!(pwm_pattern("4.0", 30).iter().all(|b| !b));
    assert_eq!(pwm_pattern("5.0", 30).iter().filter(|b| **b).count(), 3);
    assert!(pwm_pattern("96.0", 30).iter().all(|b| *b));
    // PT 350 runs as 300 ms (timer resolution); 60 % of it is 180 ms -> 200 ms,
    // i.e. 2 of every 3 scans (it used to be on all the time).
    let q = pwm_pattern_period("60.0", 350, 30);
    for period in q.chunks(3) {
        assert_eq!(period, [true, true, false], "{q:?}");
    }
}

#[test]
fn step3_opens_and_closes_with_the_two_point_rule() {
    // BAND 2, HYST 0.5: OPEN_OUT on at E >= 1.25, off at E <= 0.75; CLOSE_OUT mirrored.
    let src = "PROGRAM P\nVAR c : STEP3; x AT D10 : REAL; o AT Y0 : BOOL; cl AT Y1 : BOOL; e AT D20 : REAL; END_VAR\n\
c(ENABLE := NOT X0, PV := x, SP := 50.0, BAND := 2.0, HYST := 0.5); o := c.OPEN_OUT; cl := c.CLOSE_OUT; e := c.E;\nEND_PROGRAM";
    let seen = trace(
        src,
        &[
            50.0, 48.8, 48.75, 49.1, 49.25, 51.2, 51.25, 50.9, 50.75, 40.0,
        ],
        |s| (bit(s, "Y0"), bit(s, "Y1")),
    );
    let (f, t) = (false, true);
    assert_eq!(
        seen,
        [
            (f, f),
            (f, f),
            (t, f),
            (t, f),
            (f, f),
            (f, f),
            (f, t),
            (f, t),
            (f, f),
            (t, f)
        ]
    );
    let sim = run(src, 1, |s| {
        set_real(s, "D10", 40.0);
        set_x(s, "X0", true);
    });
    assert!(!bit(&sim, "Y0") && !bit(&sim, "Y1"));
    assert_eq!(real(&sim, "D20"), 10.0);
    // DIRECT: E = PV - SP, so a high PV opens.
    let src = src.replace("HYST := 0.5)", "HYST := 0.5, DIRECT := TRUE)");
    let seen = trace(&src, &[52.0, 48.0], |s| (bit(s, "Y0"), bit(s, "Y1")));
    assert_eq!(seen, [(t, f), (f, t)]);
}

#[test]
fn step3_first_scan_switches_only_beyond_a_switch_on_point() {
    let src = "PROGRAM P\nVAR c : STEP3; x AT D10 : REAL; o AT Y0 : BOOL; cl AT Y1 : BOOL; END_VAR\n\
c(PV := x, SP := 50.0, BAND := 2.0, HYST := 0.5); o := c.OPEN_OUT; cl := c.CLOSE_OUT;\nEND_PROGRAM";
    let first = |pv: f32| trace(src, &[pv], |s| (bit(s, "Y0"), bit(s, "Y1")))[0];
    assert_eq!(first(49.0), (false, false), "inside the hysteresis");
    assert_eq!(first(48.75), (true, false));
    assert_eq!(first(51.25), (false, true));
}

#[test]
fn step3_never_drives_both_directions_with_bad_parameters() {
    // HYST larger than BAND is clamped to BAND: OPEN_OUT on at E >= 2, off at E <= 0.
    let src = "PROGRAM P\nVAR c : STEP3; x AT D10 : REAL; o AT Y0 : BOOL; cl AT Y1 : BOOL; END_VAR\n\
c(PV := x, SP := 50.0, BAND := 2.0, HYST := 3.0); o := c.OPEN_OUT; cl := c.CLOSE_OUT;\nEND_PROGRAM";
    let seen = trace(src, &[47.5, 49.0, 50.0, 52.5, 50.5, 50.0], |s| {
        (bit(s, "Y0"), bit(s, "Y1"))
    });
    let (f, t) = (false, true);
    assert_eq!(seen, [(t, f), (t, f), (f, f), (f, t), (f, t), (f, f)]);
    // A negative dead band counts as 0; with no band and no hysteresis both would be due
    // at E = 0, so both stay off there.
    let src = src.replace("BAND := 2.0, HYST := 3.0", "BAND := -2.0, HYST := 0.0");
    let seen = trace(&src, &[50.0, 49.0, 50.0, 51.0], |s| {
        (bit(s, "Y0"), bit(s, "Y1"))
    });
    assert_eq!(seen, [(f, f), (t, f), (f, f), (f, t)]);
}

#[test]
fn split_range_maps_one_signal_to_two_outputs() {
    let src = "PROGRAM P\nVAR s : SPLIT_RANGE; x AT D10 : REAL; a AT D20 : REAL; b AT D22 : REAL; END_VAR\n\
s(IN := x, REV1 := X0); a := s.OUT1; b := s.OUT2;\nEND_PROGRAM";
    let seen = trace(src, &[-5.0, 0.0, 25.0, 50.0, 75.0, 100.0, 120.0], |s| {
        (real(s, "D20"), real(s, "D22"))
    });
    assert_eq!(
        seen,
        [
            (0.0, 0.0),
            (0.0, 0.0),
            (50.0, 0.0),
            (100.0, 0.0),
            (100.0, 50.0),
            (100.0, 100.0),
            (100.0, 100.0)
        ]
    );
    let src1 = src.replace("REV1 := X0", "SPLIT := 20.0, REV1 := TRUE");
    let seen = trace(&src1, &[10.0, 60.0], |s| (real(s, "D20"), real(s, "D22")));
    assert_eq!(seen, [(50.0, 0.0), (0.0, 50.0)]);
    let src2 = src.replace("REV1 := X0", "SPLIT := 20.0, REV2 := TRUE");
    let seen = trace(&src2, &[0.0, 10.0, 20.0, 60.0, 100.0], |s| {
        (real(s, "D20"), real(s, "D22"))
    });
    assert_eq!(
        seen,
        [
            (0.0, 100.0),
            (50.0, 100.0),
            (100.0, 100.0),
            (100.0, 50.0),
            (100.0, 0.0)
        ]
    );
    // SPLIT is clamped to 1..99, so there is never a division by zero.
    let src3 = src.replace("REV1 := X0", "SPLIT := 0.0");
    let seen = trace(&src3, &[0.5, 1.0, 50.5], |s| {
        (real(s, "D20"), real(s, "D22"))
    });
    assert_eq!(seen, [(50.0, 0.0), (100.0, 0.0), (100.0, 50.0)]);
}

#[test]
fn totalizer_total_is_retentive() {
    let src = "PROGRAM P\nVAR t : TOTALIZER; whole AT D24 : DINT; END_VAR\n\
t(FLOW := 1.0); whole := t.TOTAL_DINT;\nEND_PROGRAM";
    let lowered = crate::st::lower(src).unwrap_or_else(|e| panic!("{e:?}"));
    for name in ["ACC_WHOLE", "ACC_FRAC"] {
        let s = lowered
            .symbols
            .iter()
            .find(|s| s.name.ends_with(name))
            .unwrap_or_else(|| panic!("no symbol {name}"));
        assert!(s.retained, "{name} must be VAR_RETAIN");
        let index: u16 = s.address[1..].parse().unwrap();
        assert!(
            crate::alloc::Retain::word_is_latched(index, 2),
            "{name} at {}",
            s.address
        );
    }
    let out = lowered
        .symbols
        .iter()
        .find(|s| s.name.ends_with("TOTAL_DINT"))
        .unwrap();
    assert!(!out.retained);
}

#[test]
fn totalizer_ignores_negative_flow_and_bad_unit() {
    let src = "PROGRAM P\nVAR t : TOTALIZER; x AT D10 : REAL; tot AT D20 : REAL; END_VAR\n\
t(FLOW := x, UNIT_S := 1.0, SAMPLE_MS := 100); tot := t.TOTAL;\nEND_PROGRAM";
    let seen = trace(src, &[-5.0, -5.0, -5.0, 10.0, 10.0], |s| real(s, "D20"));
    assert_eq!(seen[2], 0.0);
    assert!(close(seen[4], 2.0, 1e-4), "{seen:?}");
    let src = src.replace("UNIT_S := 1.0", "UNIT_S := 0.0");
    let seen = trace(&src, &[10.0; 5], |s| real(s, "D20"));
    assert_eq!(seen[4], 0.0);
}

/// AIN on a 0..20 mA = 0..4000 module used for 4..20 mA: 4 mA = 800, 3.8 mA = 760,
/// 20.5 mA = 4100; 0..10 bar.
const AIN_SRC: &str = "PROGRAM P\nVAR a : AIN; raw AT D100 : INT; o AT D20 : REAL; f AT Y0 : BOOL; fc AT D30 : INT; END_VAR\n\
a(IN := raw, IN_MIN := 800, IN_MAX := 4000, OUT_MIN := 0.0, OUT_MAX := 10.0, FAULT_LO := 760, FAULT_HI := 4100, FAULT_MS := 500, SUBST := -1.0, SUBST_EN := X0);\n\
o := a.OUT; f := a.FAULT; fc := a.FAULT_CODE;\nEND_PROGRAM";

/// Per scan: (OUT, FAULT, FAULT_CODE).
fn ain_trace(src: &str, raws: &[i16], setup: impl Fn(&mut Simulator)) -> Vec<(f32, bool, i16)> {
    let program = compile(src);
    let mut sim = Simulator::default();
    setup(&mut sim);
    raws.iter()
        .map(|&r| {
            sim.memory.set_word("D100".parse().unwrap(), r);
            sim.scan(&program, 100);
            (real(&sim, "D20"), bit(&sim, "Y0"), word(&sim, "D30"))
        })
        .collect()
}

#[test]
fn ain_scales_and_clamps_to_engineering_units() {
    let seen = ain_trace(AIN_SRC, &[800, 2400, 4000, 780, 4050, 1200], |_| {});
    let out: Vec<f32> = seen.iter().map(|v| v.0).collect();
    // 780 and 4050 lie between the scale and the fault limits: clamped, no fault.
    assert_eq!(out, [0.0, 5.0, 10.0, 0.0, 10.0, 1.25]);
    assert!(seen.iter().all(|v| !v.1 && v.2 == 0));
    // A reversed raw range (IN_MIN > IN_MAX) works like SCALE.
    let src = AIN_SRC.replace(
        "IN_MIN := 800, IN_MAX := 4000",
        "IN_MIN := 4000, IN_MAX := 800",
    );
    let seen = ain_trace(&src, &[800, 2400, 4000, 4050], |_| {});
    let out: Vec<f32> = seen.iter().map(|v| v.0).collect();
    assert_eq!(out, [10.0, 5.0, 0.0, 0.0]);
}

#[test]
fn ain_wire_break_holds_the_last_good_value_and_faults_after_the_delay() {
    let mut raws = vec![2400, 2400];
    raws.extend([0; 7]);
    raws.extend([4000, 4000]);
    let seen = ain_trace(AIN_SRC, &raws, |_| {});
    // OUT freezes at 5.0 as soon as IN is bad; FAULT comes 500 ms after the first bad
    // scan; FAULT_CODE stays 0 during the delay, then shows 1 (low / wire break).
    for (i, v) in seen.iter().enumerate().take(9).skip(2) {
        assert_eq!(v.0, 5.0, "scan {i}");
        let faulted = i >= 7;
        assert_eq!(
            (v.1, v.2),
            (faulted, if faulted { 1 } else { 0 }),
            "scan {i}"
        );
    }
    // Back inside the limits: the fault and its code clear at once and OUT follows.
    assert_eq!(seen[9], (10.0, false, 0));
    // With SUBST_EN the substitute replaces OUT, but only once FAULT is set; FAULT_CODE
    // keeps the cause while SUBST is output.
    let seen = ain_trace(AIN_SRC, &raws, |s| set_x(s, "X0", true));
    assert_eq!(seen[6], (5.0, false, 0));
    assert_eq!(seen[7], (-1.0, true, 1));
    assert_eq!(seen[8], (-1.0, true, 1));
    assert_eq!(seen[9], (10.0, false, 0));
}

#[test]
fn ain_over_range_sets_the_high_fault() {
    let src = AIN_SRC.replace("FAULT_MS := 500", "FAULT_MS := 0");
    let seen = ain_trace(&src, &[3200, 5000, 4100, 4101], |_| {});
    assert_eq!(seen[0], (7.5, false, 0));
    assert_eq!(seen[1], (7.5, true, 2));
    assert_eq!(seen[2], (10.0, false, 0));
    assert_eq!(seen[3], (10.0, true, 2));
}

#[test]
fn ain_fault_code_follows_the_current_cause() {
    let src = AIN_SRC.replace("FAULT_MS := 500", "FAULT_MS := 0");
    // Low -> high without passing through the good range: the fault stays, the code
    // changes.
    let codes: Vec<i16> = ain_trace(&src, &[2400, 0, 5000, 0, 2400], |_| {})
        .iter()
        .map(|v| v.2)
        .collect();
    assert_eq!(codes, [0, 1, 2, 1, 0]);
    // FAULT_LO above FAULT_HI: both apply between them and code 1 wins.
    let src = src.replace(
        "FAULT_LO := 760, FAULT_HI := 4100",
        "FAULT_LO := 3000, FAULT_HI := 2000",
    );
    assert_eq!(ain_trace(&src, &[2500], |_| {})[0], (0.0, true, 1));
}

#[test]
fn ain_bad_from_the_start_outputs_out_min_and_a_short_glitch_does_not_fault() {
    // Inverted scale: OUT_MIN = 100 at IN_MIN.
    let src = AIN_SRC.replace(
        "OUT_MIN := 0.0, OUT_MAX := 10.0",
        "OUT_MIN := 100.0, OUT_MAX := 0.0",
    );
    let seen = ain_trace(&src, &[0, 0, 2400], |_| {});
    assert_eq!(seen[0], (100.0, false, 0));
    assert_eq!(seen[2], (50.0, false, 0));
    // A 400 ms dropout is shorter than FAULT_MS = 500.
    let seen = ain_trace(AIN_SRC, &[2400, 0, 0, 0, 0, 2400, 0, 0], |_| {});
    assert!(seen.iter().all(|v| !v.1 && v.2 == 0), "{seen:?}");
    // IN_MAX = IN_MIN gives OUT_MIN instead of a division by zero.
    let src = AIN_SRC.replace("IN_MAX := 4000", "IN_MAX := 800");
    let seen = ain_trace(&src, &[2400], |_| {});
    assert_eq!(seen[0].0, 0.0);
}

#[test]
fn ain_filter_starts_at_the_first_good_value() {
    let src = AIN_SRC.replace("SUBST := -1.0", "TAU_MS := 1000.0, SUBST := -1.0");
    let mut raws = vec![2400];
    raws.extend([4000; 10]);
    let seen = ain_trace(&src, &raws, |_| {});
    assert_eq!(seen[0].0, 5.0, "no start-up transient from 0");
    let v = seen[10].0;
    let expected = 5.0 + 5.0 * (1.0 - (1.0f32 / 1.1).powi(10));
    assert!(close(v, expected, 0.01), "{v} vs {expected}");
    // 0 or a negative TAU_MS: no filter, OUT follows at once.
    for tau in ["0.0", "-1000.0"] {
        let src = AIN_SRC.replace("SUBST := -1.0", &format!("TAU_MS := {tau}, SUBST := -1.0"));
        let out: Vec<f32> = ain_trace(&src, &[2400, 4000, 800], |_| {})
            .iter()
            .map(|v| v.0)
            .collect();
        assert_eq!(out, [5.0, 10.0, 0.0], "TAU_MS {tau}");
    }
}

const DEV_SRC: &str = "PROGRAM P\nVAR d : DEV_ALARM; x AT D10 : REAL; q AT Y0 : BOOL; dev AT D20 : REAL; END_VAR\n\
d(ENABLE := NOT X0, PV := x, SP := 50.0, BAND := 5.0, DELAY_MS := 300, INHIBIT_MS := 500); q := d.Q; dev := d.DEV;\nEND_PROGRAM";

#[test]
fn dev_alarm_waits_for_inhibit_and_delay() {
    let seen = trace(DEV_SRC, &[60.0; 12], |s| bit(s, "Y0"));
    // Inhibit 500 ms after the first call, then the deviation must last 300 ms.
    let first = seen.iter().position(|q| *q).unwrap();
    assert_eq!(first, 8, "{seen:?}");
    assert!(seen[8..].iter().all(|q| *q));
    let sim = run(DEV_SRC, 1, |s| set_real(s, "D10", 60.0));
    assert_eq!(real(&sim, "D20"), 10.0, "DEV is updated while inhibited");
}

#[test]
fn dev_alarm_band_is_exclusive_and_works_both_ways() {
    let src = DEV_SRC.replace(
        "DELAY_MS := 300, INHIBIT_MS := 500",
        "DELAY_MS := 0, INHIBIT_MS := 0",
    );
    let seen = trace(&src, &[55.0, 45.0, 55.1, 50.0, 44.9, 50.0], |s| {
        (bit(s, "Y0"), real(s, "D20"))
    });
    let q: Vec<bool> = seen.iter().map(|v| v.0).collect();
    assert_eq!(q, [false, false, true, false, true, false]);
    assert!(close(seen[4].1, -5.1, 1e-4));
    // A negative band counts as 0: any deviation alarms.
    let src = src.replace("BAND := 5.0", "BAND := -1.0");
    let seen = trace(&src, &[50.0, 50.5], |s| bit(s, "Y0"));
    assert_eq!(seen, [false, true]);
}

#[test]
fn dev_alarm_delay_restarts_and_enable_restarts_the_inhibit() {
    let program = compile(DEV_SRC);
    let mut sim = Simulator::default();
    set_real(&mut sim, "D10", 50.0);
    scans(&program, &mut sim, 10);
    // Deviation for 200 ms only, then back: no alarm.
    set_real(&mut sim, "D10", 60.0);
    scans(&program, &mut sim, 3);
    assert!(!bit(&sim, "Y0"));
    set_real(&mut sim, "D10", 50.0);
    scans(&program, &mut sim, 1);
    set_real(&mut sim, "D10", 60.0);
    scans(&program, &mut sim, 3);
    assert!(!bit(&sim, "Y0"), "the delay restarted");
    scans(&program, &mut sim, 1);
    assert!(bit(&sim, "Y0"));
    // Disable clears Q; re-enable suppresses again for INHIBIT_MS + DELAY_MS.
    set_x(&mut sim, "X0", true);
    scans(&program, &mut sim, 1);
    assert!(!bit(&sim, "Y0"));
    set_x(&mut sim, "X0", false);
    let mut seen = Vec::new();
    for _ in 0..10 {
        sim.scan(&program, 100);
        seen.push(bit(&sim, "Y0"));
    }
    assert_eq!(seen.iter().position(|q| *q), Some(8), "{seen:?}");
}

#[test]
fn dev_alarm_timing_survives_the_time_base_rollover() {
    // The internal time base restarts every 30 s; a delay running across it stays exact.
    let src = DEV_SRC.replace(
        "DELAY_MS := 300, INHIBIT_MS := 500",
        "DELAY_MS := 1000, INHIBIT_MS := 0",
    );
    let program = compile(&src);
    for start in [295usize, 300, 305] {
        let mut sim = Simulator::default();
        set_real(&mut sim, "D10", 50.0);
        scans(&program, &mut sim, start);
        set_real(&mut sim, "D10", 60.0);
        let mut n = 0;
        while !bit(&sim, "Y0") {
            sim.scan(&program, 100);
            n += 1;
            assert!(n < 30, "no alarm (start {start})");
        }
        assert_eq!(n, 11, "start {start}");
    }
}

const ACT_SRC: &str = "PROGRAM P\nVAR a : ACTUATOR_3P; x AT D10 : REAL; o AT Y0 : BOOL; c AT Y1 : BOOL; p AT D20 : REAL; END_VAR\n\
a(ENABLE := NOT X0, SP := x, TRAVEL_S := 1.0, MIN_PULSE_MS := 200, BAND := 2.0); o := a.OPEN_OUT; c := a.CLOSE_OUT; p := a.POS;\nEND_PROGRAM";

type ActLog = Vec<(bool, bool, f32)>;

struct Act {
    program: plc::Program,
    sim: Simulator,
    log: ActLog,
}

impl Act {
    fn new() -> Self {
        Act {
            program: compile(ACT_SRC),
            sim: Simulator::default(),
            log: Vec::new(),
        }
    }
    /// Runs `n` scans with setpoint `sp`, checks the interlock and returns those scans.
    fn run(&mut self, sp: f32, n: usize) -> ActLog {
        set_real(&mut self.sim, "D10", sp);
        let from = self.log.len();
        for _ in 0..n {
            self.sim.scan(&self.program, 100);
            let v = (
                bit(&self.sim, "Y0"),
                bit(&self.sim, "Y1"),
                real(&self.sim, "D20"),
            );
            assert!(
                !(v.0 && v.1),
                "OPEN_OUT and CLOSE_OUT together at scan {}",
                self.log.len()
            );
            if let Some(prev) = self.log.last() {
                assert!(
                    !(prev.0 && v.1) && !(prev.1 && v.0),
                    "reversal without a pause at scan {}",
                    self.log.len()
                );
            }
            self.log.push(v);
        }
        self.log[from..].to_vec()
    }
    fn last(&self) -> (bool, bool, f32) {
        *self.log.last().unwrap()
    }
}

fn opens(s: &[(bool, bool, f32)]) -> usize {
    s.iter().filter(|v| v.0).count()
}

fn closes(s: &[(bool, bool, f32)]) -> usize {
    s.iter().filter(|v| v.1).count()
}

#[test]
fn actuator_3p_synchronises_closed_at_start_then_follows_the_setpoint() {
    let mut a = Act::new();
    // Start-up: POS assumed 100 %, close for 1.0 s travel + 0.5 s overdrive.
    let s = a.run(50.0, 16);
    assert_eq!(s[0], (false, true, 100.0));
    assert_eq!(closes(&s), 15, "{s:?}");
    assert_eq!(opens(&s), 0);
    assert_eq!(s[15], (false, false, 0.0));
    // 0 -> 50 % at 10 % per 100 ms: OPEN for 5 scans.
    let s = a.run(50.0, 8);
    assert_eq!(opens(&s), 5, "{s:?}");
    assert_eq!(a.last(), (false, false, 50.0));
}

#[test]
fn actuator_3p_dead_band_minimum_pulse_and_reversal() {
    let mut a = Act::new();
    a.run(50.0, 30);
    assert_eq!(a.last(), (false, false, 50.0));
    // Inside the dead band (|SP - POS| <= BAND/2 = 1): no move.
    let s = a.run(51.0, 5);
    assert_eq!(opens(&s) + closes(&s), 0);
    // 1.5 % away: one minimum pulse of 200 ms = 20 % (this BAND is deliberately too small
    // for the pulse, see the doc; the block would correct back on the next scans).
    let s = a.run(51.5, 3);
    assert_eq!(
        s.iter().map(|v| v.0).collect::<Vec<_>>(),
        [true, true, false]
    );
    assert_eq!(a.last().2, 70.0, "{s:?}");
    // Back to 30 %: closes for 4 scans (the pause before a reversal is checked in run()).
    let s = a.run(30.0, 8);
    assert_eq!(closes(&s), 4, "{s:?}");
    assert_eq!(a.last(), (false, false, 30.0));
}

#[test]
fn actuator_3p_overdrives_at_the_open_end_and_stops_when_disabled() {
    let mut a = Act::new();
    a.run(0.0, 20);
    assert_eq!(a.last(), (false, false, 0.0));
    // 100 %: 1.0 s travel + 0.5 s overdrive.
    let s = a.run(100.0, 20);
    assert_eq!(opens(&s), 15, "{s:?}");
    assert_eq!(a.last(), (false, false, 100.0));
    // SP stays at 100: no further drive.
    let s = a.run(100.0, 10);
    assert_eq!(opens(&s), 0);
    // SP above 100 is clamped: no drive either.
    let s = a.run(150.0, 5);
    assert_eq!(opens(&s) + closes(&s), 0);
    // Disable in the middle of a move: both off at once, POS kept.
    a.run(40.0, 3);
    set_x(&mut a.sim, "X0", true);
    let s = a.run(40.0, 5);
    assert!(s.iter().all(|v| !v.0 && !v.1));
    let held = s[0].2;
    assert!(s.iter().all(|v| v.2 == held));
    assert!(held > 40.0 && held < 100.0, "{held}");
    set_x(&mut a.sim, "X0", false);
    a.run(40.0, 10);
    assert_eq!(a.last(), (false, false, 40.0));
}

#[test]
fn actuator_3p_travel_time_below_0_1_s_counts_as_0_1_s() {
    // Start-up synchronisation closes for 1.5 * TRAVEL_S; with 0.1 s that is 150 ms,
    // i.e. 2 scans of 100 ms. 0.0 and a negative value behave the same.
    for travel in ["0.1", "0.0", "-5.0"] {
        let src = ACT_SRC.replace("TRAVEL_S := 1.0", &format!("TRAVEL_S := {travel}"));
        let program = compile(&src);
        let mut sim = Simulator::default();
        let mut closes = 0;
        for _ in 0..10 {
            sim.scan(&program, 100);
            closes += bit(&sim, "Y1") as usize;
        }
        assert_eq!(closes, 2, "TRAVEL_S {travel}");
        assert_eq!(real(&sim, "D20"), 0.0, "TRAVEL_S {travel}");
    }
}

const HC_SRC: &str = "PROGRAM P\nVAR h : HEAT_CURVE; x AT D10 : REAL; o AT D20 : REAL; END_VAR\n\
h(T_OUTDOOR := x, NIGHT := X0); o := h.OUT;\nEND_PROGRAM";

#[test]
fn heat_curve_follows_the_design_point_and_clamps() {
    // Defaults: room 20, design -12 -> 70: slope 50/32 = 1.5625, limits 20..75.
    let seen = trace(HC_SRC, &[-12.0, 4.0, 20.0, 30.0, -30.0], |s| real(s, "D20"));
    assert_eq!(seen, [70.0, 45.0, 20.0, 20.0, 75.0]);
    // Night setback 5 K: TR = 15 -> 15 + 1.5625 * 11.
    let program = compile(HC_SRC);
    let mut sim = Simulator::default();
    set_real(&mut sim, "D10", 4.0);
    set_x(&mut sim, "X0", true);
    sim.scan(&program, 100);
    assert!(
        close(real(&sim, "D20"), 32.1875, 1e-3),
        "{}",
        real(&sim, "D20")
    );
}

#[test]
fn heat_curve_explicit_slope_shift_and_degenerate_parameters() {
    let src = HC_SRC.replace("NIGHT := X0", "SLOPE := 1.0, SHIFT := 3.0, NIGHT := X0");
    let seen = trace(&src, &[0.0, 10.0], |s| real(s, "D20"));
    assert_eq!(seen, [43.0, 33.0]);
    // Design outdoor temperature not below the room setpoint: slope 0, OUT = TR + SHIFT.
    let src = HC_SRC.replace(
        "NIGHT := X0",
        "T_DES_OUT := 25.0, SHIFT := 10.0, NIGHT := X0",
    );
    let seen = trace(&src, &[-20.0], |s| real(s, "D20"));
    assert_eq!(seen, [30.0]);
    // OUT_MIN above OUT_MAX: OUT_MAX wins. Negative SETBACK counts as 0.
    let src = HC_SRC.replace(
        "NIGHT := X0",
        "OUT_MIN := 60.0, OUT_MAX := 50.0, NIGHT := X0",
    );
    assert_eq!(trace(&src, &[10.0], |s| real(s, "D20")), [50.0]);
    let src = HC_SRC.replace("NIGHT := X0", "SETBACK := -5.0, NIGHT := TRUE");
    assert_eq!(trace(&src, &[4.0], |s| real(s, "D20")), [45.0]);
}

const CONTROL_ST: &str = include_str!("st/stdlib/control.st");

/// Names of the blocks defined in control.st.
fn control_blocks() -> Vec<&'static str> {
    CONTROL_ST
        .lines()
        .filter_map(|l| l.strip_prefix("FUNCTION_BLOCK "))
        .map(str::trim)
        .collect()
}

#[test]
fn control_blocks_leave_room_in_the_512_instruction_program_limit() {
    // The simulator accepts at most 512 IL instructions per program and every call is
    // inlined, so each block must leave room for the caller and a second block.
    for b in control_blocks() {
        let src = format!("PROGRAM P\nVAR b : {b}; END_VAR\nb();\nEND_PROGRAM");
        let lowered = crate::st::lower(&src).unwrap_or_else(|e| panic!("{b}: {e:?}"));
        let n = lowered.il.lines().filter(|l| !l.trim().is_empty()).count();
        assert!(n < 400, "{b} lowers to {n} IL lines");
    }
}

#[test]
fn every_control_example_compiles() {
    // Each doc's one-line Example (English and Turkish) must be valid ST. Free variables
    // used by the examples are declared REAL here (total_m3 is DINT).
    let reals = [
        "level",
        "temp",
        "pressure",
        "raw_temp",
        "speed_sp",
        "speed",
        "sp",
        "flow",
        "flow_avg",
        "flow_ls",
        "litres",
        "level_rate",
        "flow_m3h",
        "valve",
        "pump_speed",
        "cool_valve",
        "heat_valve",
        "bar",
        "t_out",
        "supply_sp",
        "heat_demand",
        "ctrl_out",
        "valve_sp",
    ];
    let lines: Vec<&str> = CONTROL_ST.lines().collect();
    let mut examples = 0;
    for (i, line) in lines.iter().enumerate() {
        let l = line.trim();
        let Some(ex) = l
            .strip_prefix("Example: ")
            .or_else(|| l.strip_prefix("Örnek: "))
        else {
            continue;
        };
        let block = lines[i..]
            .iter()
            .find_map(|x| x.strip_prefix("FUNCTION_BLOCK "))
            .unwrap()
            .trim();
        let inst = ex.split('(').next().unwrap().trim();
        let mut decl = format!("{inst} : {block}; total_m3 : DINT;");
        for r in reals {
            let used = ex
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .any(|w| w == r);
            if used {
                decl.push_str(&format!(" {r} : REAL;"));
            }
        }
        let src = format!("PROGRAM P\nVAR {decl} END_VAR\n{ex}\nEND_PROGRAM");
        if let Err(e) = plc::compile_st(&src) {
            panic!("{e:?}\n{src}");
        }
        examples += 1;
    }
    assert_eq!(examples, 2 * control_blocks().len());
}

#[test]
fn ms_accumulators_do_not_overflow_at_the_documented_32767_limit() {
    // DEV_ALARM: DELAY_MS = 32767 must alarm after 32.8 s, not wrap around forever.
    let src = DEV_SRC.replace(
        "DELAY_MS := 300, INHIBIT_MS := 500",
        "DELAY_MS := 32767, INHIBIT_MS := 0",
    );
    let sim = run(&src, 340, |s| set_real(s, "D10", 60.0));
    assert!(bit(&sim, "Y0"), "DEV_ALARM DELAY_MS 32767 never alarms");
    let src = DEV_SRC.replace(
        "DELAY_MS := 300, INHIBIT_MS := 500",
        "DELAY_MS := 0, INHIBIT_MS := 32767",
    );
    let sim = run(&src, 340, |s| set_real(s, "D10", 60.0));
    assert!(bit(&sim, "Y0"), "DEV_ALARM INHIBIT_MS 32767 never ends");
    // AIN: FAULT_MS = 32767.
    let src = AIN_SRC.replace("FAULT_MS := 500", "FAULT_MS := 32767");
    let seen = ain_trace(&src, &[0; 340], |_| {});
    assert!(seen.last().unwrap().1, "AIN FAULT_MS 32767 never faults");
    // ACTUATOR_3P: MIN_PULSE_MS = 32767 still ends the pulse.
    let src = ACT_SRC
        .replace("MIN_PULSE_MS := 200", "MIN_PULSE_MS := 32767")
        .replace("TRAVEL_S := 1.0", "TRAVEL_S := 100.0");
    let program = compile(&src);
    let mut sim = Simulator::default();
    set_real(&mut sim, "D10", 0.0);
    scans(&program, &mut sim, 1600); // start-up sync: 150 s
    assert!(!bit(&sim, "Y1"), "start-up sync finished");
    set_real(&mut sim, "D10", 10.0);
    scans(&program, &mut sim, 400);
    assert!(
        !bit(&sim, "Y0"),
        "ACTUATOR_3P MIN_PULSE_MS 32767 pulse never ends, pos {}",
        real(&sim, "D20")
    );
}

#[test]
fn ain_default_fault_limits_are_off_for_every_int() {
    // The defaults must never fault, not even at the INT extremes (-32767 used to trip
    // on IN = -32768).
    let src = "PROGRAM P
VAR a : AIN; raw AT D100 : INT; f AT Y0 : BOOL; END_VAR
a(IN := raw, FAULT_MS := 0); f := a.FAULT;
END_PROGRAM";
    let program = compile(src);
    let mut sim = Simulator::default();
    for raw in [-32768i16, 32767, 0] {
        sim.memory.set_word("D100".parse().unwrap(), raw);
        sim.scan(&program, 100);
        assert!(!bit(&sim, "Y0"), "IN {raw}");
    }
}

#[test]
fn pid_and_pi_output_stays_inside_the_limits_before_the_first_sample() {
    // With a long SAMPLE_MS the output used to be 0.0 (below OUT_MIN) for a whole sample,
    // e.g. stopping a pump whose minimum speed is OUT_MIN.
    for block in ["PID", "PI"] {
        let src = format!(
            "PROGRAM P
VAR c : {block}; y AT D20 : REAL; END_VAR
c(PV := 50.0, SP := 50.0, SAMPLE_MS := 5000, OUT_MIN := 20.0, OUT_MAX := 80.0); y := c.OUT;
END_PROGRAM"
        );
        for n in [1, 10, 49] {
            let sim = run(&src, n, |_| {});
            assert_eq!(real(&sim, "D20"), 20.0, "{block} after {n} scans");
        }
        // The first sample (after 5 s) keeps it there: E = 0, so P = 0 and I = 0 -> OUT_MIN.
        let sim = run(&src, 60, |_| {});
        assert_eq!(real(&sim, "D20"), 20.0, "{block} after the first sample");
    }
}

// First-scan behaviour stated in the "First scan:" notes that no test above covers.

#[test]
fn limit_alarm_first_scan_sets_only_reached_limits() {
    let src = "PROGRAM P\nVAR a : LIMIT_ALARM; x AT D10 : REAL; hh AT Y0 : BOOL; h AT Y1 : BOOL; l AT Y2 : BOOL; ll AT Y3 : BOOL; any AT Y4 : BOOL; END_VAR\n\
a(IN := x, LIM_HH := 90.0, LIM_H := 80.0, LIM_L := 20.0, LIM_LL := 10.0, HYST := 2.0);\n\
hh := a.Q_HH; h := a.Q_H; l := a.Q_L; ll := a.Q_LL; any := a.Q;\nEND_PROGRAM";
    let first = |x: f32| {
        trace(src, &[x], |s| {
            ["Y0", "Y1", "Y2", "Y3", "Y4"].map(|d| bit(s, d))
        })[0]
    };
    let (f, t) = (false, true);
    // 89 lies inside the Q_HH hysteresis band (88..90): Q_HH starts FALSE.
    assert_eq!(first(89.0), [f, t, f, f, t]);
    assert_eq!(first(90.0), [t, t, f, f, t]);
    // 11 lies inside the Q_LL hysteresis band (10..12): Q_LL starts FALSE.
    assert_eq!(first(11.0), [f, f, t, f, t]);
    assert_eq!(first(50.0), [f, f, f, f, f]);
}

#[test]
fn dev_alarm_first_scan_alarms_only_without_inhibit_and_delay() {
    let with = |delay: &str, inhibit: &str| {
        DEV_SRC.replace(
            "DELAY_MS := 300, INHIBIT_MS := 500",
            &format!("DELAY_MS := {delay}, INHIBIT_MS := {inhibit}"),
        )
    };
    let first = |src: &str| trace(src, &[60.0], |s| bit(s, "Y0"))[0];
    assert!(first(&with("0", "0")), "INHIBIT_MS = 0 and DELAY_MS = 0");
    assert!(
        !first(&with("0", "500")),
        "the first call starts the inhibit time"
    );
    assert!(
        !first(&with("300", "0")),
        "the delay starts on the first call"
    );
    assert!(
        !trace(&with("0", "0"), &[55.0], |s| bit(s, "Y0"))[0],
        "|PV - SP| = BAND"
    );
}

#[test]
fn integrator_first_scan_output_is_zero_clamped_into_the_limits() {
    let src = "PROGRAM P\nVAR i : INTEGRATOR; o AT D20 : REAL; l AT Y0 : BOOL; END_VAR\n\
i(IN := 1.0); o := i.OUT; l := i.LIM;\nEND_PROGRAM";
    // OUT = 0.0 on the first scan; the first sample (scan 2, 100 ms later) adds 0.1.
    let seen = trace(src, &[0.0, 0.0], |s| (real(s, "D20"), bit(s, "Y0")));
    assert_eq!(seen[0], (0.0, false));
    assert!(close(seen[1].0, 0.1, 1e-5), "{seen:?}");
    // 0.0 below OUT_MIN: OUT = OUT_MIN and LIM from the first scan on.
    let src = src.replace("IN := 1.0", "IN := 1.0, OUT_MIN := 5.0, OUT_MAX := 10.0");
    assert_eq!(
        trace(&src, &[0.0], |s| (real(s, "D20"), bit(s, "Y0")))[0],
        (5.0, true)
    );
}

#[test]
fn pid_first_scan_manual_at_once_and_no_derivative_on_the_first_sample() {
    let src = "PROGRAM P\nVAR c : PID; x AT D10 : REAL; y AT D20 : REAL; END_VAR\n\
c(PV := x, SP := 50.0, KP := 1.0, TI_MS := 0.0, TD_MS := 1000.0, OUT_MIN := -100.0); y := c.OUT;\nEND_PROGRAM";
    // PV is 10 from the start: the first sample must not see a step from 0 to 10
    // (that would add KP * TD * -10 / 0.1 s = -100).
    let seen = trace(src, &[10.0, 10.0, 10.0], |s| real(s, "D20"));
    assert_eq!(seen, [0.0, 40.0, 40.0]);
    let src = src.replace(
        "OUT_MIN := -100.0",
        "OUT_MIN := -100.0, MANUAL := TRUE, MAN_OUT := 30.0",
    );
    assert_eq!(trace(&src, &[10.0], |s| real(s, "D20")), [30.0]);
}

#[test]
fn totalizer_shows_the_retained_total_on_the_first_scan_after_power_up() {
    let src = "PROGRAM P\nVAR t : TOTALIZER; tot AT D20 : REAL; whole AT D24 : DINT; fr AT D26 : REAL; END_VAR\n\
t(FLOW := 5400.0, ENABLE := NOT X1); tot := t.TOTAL; whole := t.TOTAL_DINT; fr := t.FRAC;\nEND_PROGRAM";
    let program = compile(src);
    let mut sim = Simulator::default();
    scans(&program, &mut sim, 51); // 7.5 units, see totalizer_counts_whole_units_and_fraction
    assert!(close(real(&sim, "D20"), 7.5, 1e-3), "{}", real(&sim, "D20"));
    // Power cycle: a fresh simulator that keeps only the retentive (latched) words.
    let lowered = crate::st::lower(src).unwrap_or_else(|e| panic!("{e:?}"));
    let mut fresh = Simulator::default();
    for name in ["ACC_WHOLE", "ACC_FRAC"] {
        let s = lowered
            .symbols
            .iter()
            .find(|s| s.name.ends_with(name))
            .unwrap_or_else(|| panic!("no symbol {name}"));
        let index: u16 = s.address[1..].parse().unwrap();
        for i in index..index + 2 {
            let d: plc::Device = format!("D{i}").parse().unwrap();
            fresh.memory.set_word(d, sim.memory.word(d));
        }
    }
    set_x(&mut fresh, "X1", true); // paused, so the scan takes no sample
    fresh.scan(&program, 100);
    assert!(
        close(real(&fresh, "D20"), 7.5, 1e-3),
        "{}",
        real(&fresh, "D20")
    );
    assert_eq!(word(&fresh, "D24"), 7);
    assert!(
        close(real(&fresh, "D26"), 0.5, 1e-3),
        "{}",
        real(&fresh, "D26")
    );
}
