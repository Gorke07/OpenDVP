//! Behavioural tests of the ST standard library, category "process" (src/st/stdlib/process.st).
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

/// A compiled program plus a simulator, for tests that change inputs between scans.
struct Rig {
    program: plc::Program,
    sim: Simulator,
}

impl Rig {
    fn new(source: &str) -> Self {
        let program = plc::compile_st(source).unwrap_or_else(|e| panic!("{e:?}\n{source}"));
        Rig {
            program,
            sim: Simulator::default(),
        }
    }
    fn set(&mut self, device: &str, v: bool) -> &mut Self {
        self.sim.memory.set_input(device.parse().unwrap(), v);
        self
    }
    fn set_word(&mut self, device: &str, v: i16) -> &mut Self {
        self.sim.memory.set_word(device.parse().unwrap(), v);
        self
    }
    fn scan(&mut self, n: usize) -> &mut Self {
        for _ in 0..n {
            self.sim.scan(&self.program, 100);
        }
        self
    }
    fn bit(&self, device: &str) -> bool {
        bit(&self.sim, device)
    }
    fn word(&self, device: &str) -> i16 {
        word(&self.sim, device)
    }
}

/// Wraps `body` in a program with the declarations `vars`.
fn prog(vars: &str, body: &str) -> String {
    format!(
        "PROGRAM P
VAR
{vars}
END_VAR
{body}
END_PROGRAM"
    )
}

// ---------------------------------------------------------------- MOTOR

fn motor_rig() -> Rig {
    idle(Rig::new(&prog(
        "m : MOTOR; run AT Y0 : BOOL; running AT Y1 : BOOL; fault AT Y2 : BOOL;",
        "m(START := X0, STOP := X1, FEEDBACK := X2, RST := X3, PERMIT := NOT X4, FB_MS := 500);
         run := m.RUN; running := m.RUNNING; fault := m.FAULT;",
    )))
}

/// One scan with all inputs FALSE: edge-started blocks ignore commands held at power-up.
fn idle(mut r: Rig) -> Rig {
    r.scan(1);
    r
}

#[test]
fn motor_starts_on_edge_and_faults_without_feedback() {
    let mut r = motor_rig();
    r.set("X0", true).scan(1);
    assert!(r.bit("Y0") && !r.bit("Y1") && !r.bit("Y2"));
    r.set("X0", false).scan(4);
    assert!(
        r.bit("Y0") && !r.bit("Y2"),
        "still within the feedback time"
    );
    r.scan(1);
    assert!(
        !r.bit("Y0") && r.bit("Y2"),
        "feedback timeout trips and drops RUN"
    );
    // Reset clears the fault but does not restart: a new START edge is needed.
    r.set("X3", true).scan(1).set("X3", false).scan(1);
    assert!(!r.bit("Y2") && !r.bit("Y0"));
    r.set("X0", true).scan(1);
    assert!(r.bit("Y0"), "a new START edge restarts");
}

#[test]
fn motor_runs_with_feedback_and_stops() {
    let mut r = motor_rig();
    r.set("X0", true)
        .scan(1)
        .set("X0", false)
        .set("X2", true)
        .scan(20);
    assert!(r.bit("Y0") && r.bit("Y1") && !r.bit("Y2"));
    r.set("X1", true).scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y1"));
    // STOP has priority over START.
    r.set("X0", true).set("X2", false).scan(1);
    assert!(!r.bit("Y0"));
    r.set("X1", false).set("X0", false).scan(10);
    assert!(!r.bit("Y0") && !r.bit("Y2"));
}

#[test]
fn motor_detects_lost_and_welded_feedback() {
    let mut r = motor_rig();
    r.set("X0", true)
        .set("X2", true)
        .scan(1)
        .set("X0", false)
        .scan(3);
    assert!(r.bit("Y1"));
    r.set("X2", false).scan(5);
    assert!(r.bit("Y0") && !r.bit("Y2"));
    r.scan(1);
    assert!(r.bit("Y2") && !r.bit("Y0"), "feedback lost while running");
    // Feedback present without a command (welded contactor): fault, and reset cannot clear it.
    r.set("X3", true)
        .scan(1)
        .set("X3", false)
        .set("X2", true)
        .scan(6);
    assert!(r.bit("Y2") && !r.bit("Y0"));
    r.set("X3", true).scan(1);
    assert!(r.bit("Y2"), "cause still present");
}

#[test]
fn motor_permit_and_disabled_monitoring() {
    let mut r = motor_rig();
    r.set("X4", true).set("X0", true).scan(1);
    assert!(!r.bit("Y0"), "no permit, no start");
    r.set("X0", false)
        .scan(1)
        .set("X4", false)
        .set("X0", true)
        .scan(1);
    assert!(r.bit("Y0"));
    r.set("X4", true).scan(1);
    assert!(!r.bit("Y0"), "permit loss stops immediately");
    // FB_MS 0 disables feedback monitoring.
    let mut r = Rig::new(&prog(
        "m : MOTOR; run AT Y0 : BOOL; fault AT Y2 : BOOL;",
        "m(START := X0, FB_MS := 0); run := m.RUN; fault := m.FAULT;",
    ));
    r.scan(1);
    r.set("X0", true).scan(400);
    assert!(r.bit("Y0") && !r.bit("Y2"));
}

// ---------------------------------------------------------------- MOTOR_REV

#[test]
fn motor_rev_changeover_waits_dead_time() {
    let mut r = Rig::new(&prog(
        "m : MOTOR_REV; f AT Y0 : BOOL; v AT Y1 : BOOL; w AT Y2 : BOOL; d AT D20 : INT;",
        "m(FWD := X0, REV := X1, STOP := X2, DEAD_MS := 500);\n\
         f := m.FWD_OUT; v := m.REV_OUT; w := m.WAITING; d := m.DIR;",
    ));
    r.scan(1);
    r.set("X0", true).scan(1).set("X0", false);
    assert!(
        r.bit("Y0") && !r.bit("Y1") && r.word("D20") == 1,
        "first start is immediate"
    );
    r.set("X1", true).scan(1).set("X1", false);
    assert!(!r.bit("Y0") && !r.bit("Y1") && r.bit("Y2") && r.word("D20") == 0);
    r.scan(5);
    assert!(!r.bit("Y0") && !r.bit("Y1"), "dead time running");
    r.scan(1);
    assert!(r.bit("Y1") && !r.bit("Y0") && !r.bit("Y2") && r.word("D20") == -1);
    // Both commands at once: forward wins, outputs are never both on.
    r.set("X0", true).set("X1", true);
    for _ in 0..10 {
        r.scan(1);
        assert!(!(r.bit("Y0") && r.bit("Y1")));
    }
    assert!(r.bit("Y0"));
    // STOP drops everything; an immediate restart waits for the dead time.
    r.set("X0", false).set("X1", false).set("X2", true).scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y1") && !r.bit("Y2"));
    r.set("X2", false).scan(1).set("X0", true).scan(1);
    assert!(!r.bit("Y0") && r.bit("Y2"));
    r.scan(4);
    assert!(r.bit("Y0"));
}

// ---------------------------------------------------------------- STAR_DELTA

#[test]
fn star_delta_sequence_and_gap() {
    let mut r = Rig::new(&prog(
        "s : STAR_DELTA; m AT Y0 : BOOL; st AT Y1 : BOOL; de AT Y2 : BOOL; run AT Y3 : BOOL;",
        "s(START := X0, STOP := X1, PERMIT := NOT X2, STAR_MS := 1000, GAP_MS := 200);\n\
         m := s.MAIN; st := s.STAR; de := s.DELTA; run := s.RUNNING;",
    ));
    r.scan(1);
    r.set("X0", true).scan(1).set("X0", false);
    assert!(r.bit("Y0") && r.bit("Y1") && !r.bit("Y2"));
    let mut trace = Vec::new();
    for _ in 0..14 {
        r.scan(1);
        assert!(!(r.bit("Y1") && r.bit("Y2")), "star and delta overlap");
        trace.push((r.bit("Y1"), r.bit("Y2")));
    }
    // Scans 2..10 star, 11..12 gap, 13.. delta.
    assert!(trace[..9].iter().all(|&(s, d)| s && !d));
    assert!(trace[9..11].iter().all(|&(s, d)| !s && !d));
    assert!(trace[11..].iter().all(|&(s, d)| !s && d));
    assert!(r.bit("Y0") && r.bit("Y3"));
    r.set("X2", true).scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y1") && !r.bit("Y2") && !r.bit("Y3"));
    // A restart begins in star again.
    r.set("X2", false).set("X0", true).scan(1);
    assert!(r.bit("Y1") && !r.bit("Y2"));
    r.set("X1", true).scan(1);
    assert!(!r.bit("Y0"));
}

// ---------------------------------------------------------------- VALVE

fn valve_rig() -> Rig {
    Rig::new(&prog(
        "v : VALVE; out AT Y0 : BOOL; op AT Y1 : BOOL; cl AT Y2 : BOOL; mv AT Y3 : BOOL; f AT Y4 : BOOL; code AT D20 : INT;",
        "v(OPEN := X0, LS_OPEN := X1, LS_CLOSED := X2, RST := X3, TRAVEL_MS := 500);\n\
         out := v.Q; op := v.IS_OPEN; cl := v.IS_CLOSED; mv := v.MOVING; f := v.FAULT; code := v.FAULT_CODE;",
    ))
}

#[test]
fn valve_opens_and_closes_within_travel_time() {
    let mut r = valve_rig();
    r.set("X2", true).scan(20);
    assert!(r.bit("Y2") && !r.bit("Y0") && !r.bit("Y4") && !r.bit("Y3"));
    r.set("X0", true).scan(1);
    assert!(r.bit("Y0") && r.bit("Y3"));
    r.set("X2", false).scan(2).set("X1", true).scan(1);
    assert!(r.bit("Y1") && !r.bit("Y3") && !r.bit("Y4"));
    r.scan(20);
    assert!(r.bit("Y0") && !r.bit("Y4"));
    r.set("X0", false)
        .scan(1)
        .set("X1", false)
        .scan(2)
        .set("X2", true)
        .scan(10);
    assert!(r.bit("Y2") && !r.bit("Y0") && !r.bit("Y4"));
}

#[test]
fn valve_fault_codes_and_fail_safe() {
    // Failed to open: output drops, code 1, TRAVEL_MS (500) plus two scans after the command.
    let mut r = valve_rig();
    r.set("X2", true).set("X0", true).scan(6);
    assert!(r.bit("Y0") && !r.bit("Y4"));
    r.scan(1);
    assert!(!r.bit("Y0") && r.bit("Y4") && r.word("D20") == 1);
    r.scan(10);
    assert!(
        !r.bit("Y0") && r.bit("Y4"),
        "fault stays latched without reset"
    );
    // Reset with the valve closed: the open command is retried.
    r.set("X3", true).scan(1).set("X3", false);
    assert!(r.bit("Y0") && !r.bit("Y4") && r.word("D20") == 0);

    // Failed to close (stuck open): code 2.
    let mut r = valve_rig();
    r.set("X1", true).scan(6);
    assert!(r.bit("Y4") && r.word("D20") == 2 && !r.bit("Y0"));

    // Both limit switches: discrepancy, code 3.
    let mut r = valve_rig();
    r.set("X0", true).set("X1", true).set("X2", true).scan(7);
    assert!(r.bit("Y4") && r.word("D20") == 3 && !r.bit("Y0"));
    assert!(!r.bit("Y1") && !r.bit("Y2"));
    // Reset does not clear while the cause persists... it re-trips after the travel time.
    r.set("X3", true).scan(1).set("X3", false).scan(6);
    assert!(r.bit("Y4") && r.word("D20") == 3);
}

#[test]
fn valve_detects_position_lost() {
    let mut r = valve_rig();
    r.set("X0", true).set("X1", true).scan(20);
    assert!(r.bit("Y1") && !r.bit("Y4"));
    r.set("X1", false).scan(6);
    assert!(r.bit("Y4") && r.word("D20") == 1 && !r.bit("Y0"));
}

/// A command change mid-travel restarts the travel time: reversing a half-open valve must not
/// trip on the time already spent opening.
#[test]
fn valve_reversal_mid_travel_restarts_travel_time() {
    let mut r = valve_rig();
    r.set("X2", true).scan(2);
    r.set("X0", true).scan(1).set("X2", false).scan(3);
    assert!(r.bit("Y0") && r.bit("Y3") && !r.bit("Y4"));
    // Close command after 400 ms of opening; the valve needs 400 ms to return.
    r.set("X0", false);
    for i in 0..4 {
        r.scan(1);
        assert!(
            !r.bit("Y4"),
            "tripped {} ms after the reversal",
            (i + 1) * 100
        );
    }
    r.set("X2", true).scan(1);
    assert!(r.bit("Y2") && !r.bit("Y4") && !r.bit("Y3"));
    // A close that is never confirmed still trips TRAVEL_MS after the reversal, code 2.
    let mut r = valve_rig();
    r.set("X2", true).scan(2);
    r.set("X0", true)
        .scan(1)
        .set("X2", false)
        .scan(3)
        .set("X0", false);
    r.scan(5);
    assert!(!r.bit("Y4"), "within TRAVEL_MS of the close command");
    r.scan(2);
    assert!(r.bit("Y4") && r.word("D20") == 2 && !r.bit("Y0"));
}

// ---------------------------------------------------------------- PUMP_ALT

#[test]
fn pump_alt_alternates_and_fails_over() {
    let mut r = Rig::new(&prog(
        "p : PUMP_ALT; p1 AT Y0 : BOOL; p2 AT Y1 : BOOL; fo AT Y2 : BOOL; al AT Y3 : BOOL; lead AT D20 : INT;",
        "p(DEMAND := X0, BOTH := X1, AVAIL1 := NOT X2, AVAIL2 := NOT X3);\n\
         p1 := p.PUMP1; p2 := p.PUMP2; fo := p.FAILOVER; al := p.NO_PUMP; lead := p.LEAD;",
    ));
    r.scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y1") && r.word("D20") == 1);
    r.set("X0", true).scan(3);
    assert!(r.bit("Y0") && !r.bit("Y1"), "first run uses pump 1");
    r.set("X0", false).scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y1") && r.word("D20") == 2);
    r.set("X0", true).scan(1);
    assert!(!r.bit("Y0") && r.bit("Y1"), "second run uses pump 2");
    // Lead pump trips during the run: standby takes over at once.
    r.set("X3", true).scan(1);
    assert!(r.bit("Y0") && !r.bit("Y1") && r.bit("Y2") && !r.bit("Y3"));
    r.set("X2", true).scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y1") && r.bit("Y3") && !r.bit("Y2"));
    r.set("X2", false).set("X3", false).set("X0", false).scan(1);
    assert!(!r.bit("Y3") && r.word("D20") == 1);
    // BOTH runs every available pump.
    r.set("X1", true).scan(1);
    assert!(r.bit("Y0") && r.bit("Y1"));
    r.set("X2", true).scan(1);
    assert!(!r.bit("Y0") && r.bit("Y1"));
}

// ---------------------------------------------------------------- LEAD_LAG

#[test]
fn lead_lag_stages_rotates_and_replaces() {
    let mut r = Rig::new(&prog(
        "l : LEAD_LAG; d AT D0 : INT; p1 AT Y0 : BOOL; p2 AT Y1 : BOOL; p3 AT Y2 : BOOL; sh AT Y3 : BOOL; n AT D20 : INT; lead AT D21 : INT;",
        "l(DEMAND := d, AVAIL1 := NOT X0, AVAIL2 := NOT X1, AVAIL3 := NOT X2, STAGE_MS := 500, DESTAGE_MS := 300);\n\
         p1 := l.PUMP1; p2 := l.PUMP2; p3 := l.PUMP3; sh := l.SHORTAGE; n := l.RUN_COUNT; lead := l.LEAD;",
    ));
    r.scan(1);
    assert!(!r.bit("Y0") && r.word("D20") == 0 && r.word("D21") == 1);
    r.set_word("D0", 2).scan(1);
    assert!(
        r.bit("Y0") && !r.bit("Y1") && r.word("D20") == 1,
        "first pump without delay"
    );
    r.scan(2);
    assert!(!r.bit("Y1"), "stage delay running");
    r.scan(10);
    assert!(r.bit("Y0") && r.bit("Y1") && !r.bit("Y2") && r.word("D20") == 2);
    r.set_word("D0", 3).scan(10);
    assert!(r.bit("Y0") && r.bit("Y1") && r.bit("Y2"));
    r.set_word("D0", 1).scan(1);
    assert_eq!(r.word("D20"), 3, "destage delay");
    r.scan(20);
    assert!(r.bit("Y0") && !r.bit("Y1") && !r.bit("Y2") && r.word("D20") == 1);
    // All stop at once; the lead rotates to pump 2.
    r.set_word("D0", 0).scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y1") && !r.bit("Y2") && r.word("D21") == 2);
    r.set_word("D0", 1).scan(1);
    assert!(!r.bit("Y0") && r.bit("Y1") && !r.bit("Y2"));
    // Lead pump unavailable: the next one takes over at once.
    r.set("X1", true).scan(1);
    assert!(!r.bit("Y1") && r.bit("Y2") && !r.bit("Y0"));
    r.set_word("D0", 3).scan(30);
    assert!(r.bit("Y0") && !r.bit("Y1") && r.bit("Y2") && r.bit("Y3") && r.word("D20") == 2);
    // Out-of-range demand is clamped.
    r.set("X1", false).set_word("D0", 7).scan(30);
    assert!(r.bit("Y0") && r.bit("Y1") && r.bit("Y2") && r.word("D20") == 3);
    r.set_word("D0", -4).scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y1") && !r.bit("Y2"));
}

// ---------------------------------------------------------------- RUNTIME

#[test]
fn runtime_counts_minutes_hours_and_service() {
    let mut r = Rig::new(&prog(
        "rt : RUNTIME; h AT D20 : DINT; m AT D22 : INT; s AT D23 : INT; due AT Y0 : BOOL;",
        "rt(RUN := X0, RST := X1, SERVICE_H := 1, ACK := X2);\n\
         h := rt.HOURS; m := rt.MINUTES; s := rt.SECONDS; due := rt.SERVICE_DUE;",
    ));
    r.set("X0", true).scan(655);
    assert_eq!(r.word("D22"), 1);
    let s = r.word("D23");
    assert!((2..=5).contains(&s), "seconds {s}");
    r.set("X0", false).scan(100);
    assert!(
        r.word("D22") == 1 && r.word("D23") == s,
        "holds while stopped"
    );
    assert!(!r.bit("Y0"));
    r.set("X1", true).scan(1);
    assert!(r.word("D20") == 0 && r.word("D22") == 0 && r.word("D23") == 0);
}

/// Runs one hour of virtual time (about a minute in a debug build), hence ignored by default:
/// `cargo test --lib st_stdlib_process -- --ignored`.
#[test]
#[ignore]
fn runtime_hour_rollover_and_service() {
    let mut r = Rig::new(&prog(
        "rt : RUNTIME; h AT D20 : DINT; m AT D22 : INT; s AT D23 : INT; left AT D24 : DINT; due AT Y0 : BOOL;",
        "rt(RUN := X0, RST := X1, SERVICE_H := 1, ACK := X2);\n\
         h := rt.HOURS; m := rt.MINUTES; s := rt.SECONDS; left := rt.REM_H; due := rt.SERVICE_DUE;",
    ));
    r.set("X1", true).scan(1).set("X1", false);
    assert_eq!(r.word("D24"), 1);
    r.set("X0", true).scan(36_655);
    assert_eq!(r.word("D20"), 1);
    assert!(r.bit("Y0"), "service interval of 1 h elapsed");
    assert_eq!(r.word("D24"), 0, "nothing left when due");
    r.set("X2", true).scan(1).set("X2", false).scan(1);
    assert!(!r.bit("Y0") && r.word("D20") == 1 && r.word("D24") == 1);
    r.set("X1", true).scan(1);
    assert!(r.word("D20") == 0 && r.word("D22") == 0 && r.word("D23") == 0);
}

// ---------------------------------------------------------------- CYCLE_COUNTER

#[test]
fn cycle_counter_counts_edges_and_service() {
    let mut r = Rig::new(&prog(
        "c : CYCLE_COUNTER; tot AT D20 : DINT; since AT D22 : DINT; rem AT D24 : DINT; due AT Y0 : BOOL;",
        "c(CYCLE := X0, RST := X1, SERVICE_N := 3, ACK := X2);\n\
         tot := c.TOTAL; since := c.SINCE; rem := c.REM_N; due := c.SERVICE_DUE;",
    ));
    r.scan(1);
    assert!(r.word("D20") == 0 && r.word("D24") == 3 && !r.bit("Y0"));
    r.set("X0", true).scan(5).set("X0", false).scan(1);
    assert_eq!(r.word("D20"), 1, "a held input counts once");
    r.set("X0", true).scan(1).set("X0", false).scan(1);
    assert!(r.word("D20") == 2 && r.word("D24") == 1 && !r.bit("Y0"));
    r.set("X0", true).scan(1).set("X0", false).scan(1);
    assert!(r.word("D24") == 0 && r.bit("Y0"));
    r.set("X0", true).scan(1).set("X0", false).scan(1);
    assert!(r.word("D22") == 4 && r.word("D24") == 0 && r.bit("Y0"));
    r.set("X2", true).scan(1).set("X2", false).scan(1);
    assert!(r.word("D20") == 4 && r.word("D22") == 0 && r.word("D24") == 3 && !r.bit("Y0"));
    r.set("X1", true).scan(1);
    assert!(r.word("D20") == 0 && r.word("D22") == 0);
}

// ---------------------------------------------------------------- ALARM

#[test]
fn alarm_delay_flash_silence_ack() {
    let mut r = Rig::new(&prog(
        "a : ALARM; act AT Y0 : BOOL; un AT Y1 : BOOL; lamp AT Y2 : BOOL; horn AT Y3 : BOOL;",
        "a(IN := X0, DELAY_MS := 300, ACK := X1, SILENCE := X2);\n\
         act := a.LATCHED; un := a.UNACK; lamp := a.LAMP; horn := a.HORN;",
    ));
    // A pulse shorter than the delay is ignored.
    r.set("X0", true).scan(2).set("X0", false).scan(5);
    assert!(!r.bit("Y0") && !r.bit("Y2") && !r.bit("Y3"));
    r.set("X0", true).scan(10);
    assert!(r.bit("Y0") && r.bit("Y1") && r.bit("Y3"));
    let mut lamp = Vec::new();
    for _ in 0..30 {
        r.scan(1);
        lamp.push(r.bit("Y2"));
    }
    assert!(
        lamp.contains(&true) && lamp.contains(&false),
        "lamp flashes"
    );
    r.set("X2", true).scan(1).set("X2", false).scan(1);
    assert!(!r.bit("Y3") && r.bit("Y1"), "silence stops the horn only");
    // Condition clears before acknowledge: alarm stays latched.
    r.set("X0", false).scan(3);
    assert!(r.bit("Y0") && r.bit("Y1"));
    r.set("X0", true).set("X1", true).scan(1);
    assert!(!r.bit("Y1") && !r.bit("Y3"));
    for _ in 0..10 {
        r.scan(1);
        assert!(r.bit("Y2"), "steady lamp while acknowledged and present");
    }
    r.set("X1", false).set("X0", false).scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y2"));
    // Acknowledged while cleared: goes off at once.
    let mut r = Rig::new(&prog(
        "a : ALARM; act AT Y0 : BOOL; lamp AT Y2 : BOOL; horn AT Y3 : BOOL;",
        "a(IN := X0, ACK := X1); act := a.LATCHED; lamp := a.LAMP; horn := a.HORN;",
    ));
    r.set("X0", true).scan(1);
    assert!(r.bit("Y0") && r.bit("Y3"), "no delay");
    r.set("X0", false).scan(1).set("X1", true).scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y2") && !r.bit("Y3"));
}

// ---------------------------------------------------------------- FIRST_OUT4

#[test]
fn first_out4_remembers_first_trip() {
    let mut r = Rig::new(&prog(
        "f : FIRST_OUT4; first AT D20 : INT; any AT Y0 : BOOL;",
        "f(IN1 := X0, IN2 := X1, IN3 := X2, IN4 := X3, ACK := X4); first := f.FIRST; any := f.ANY;",
    ));
    r.scan(1);
    assert!(r.word("D20") == 0 && !r.bit("Y0"));
    r.set("X2", true).scan(1).set("X0", true).scan(1);
    assert!(r.word("D20") == 3 && r.bit("Y0"));
    r.set("X4", true).scan(1);
    assert_eq!(r.word("D20"), 3, "no reset while an input is active");
    r.set("X0", false).set("X2", false).scan(1);
    assert_eq!(r.word("D20"), 0);
    r.set("X4", false).set("X1", true).set("X3", true).scan(1);
    assert_eq!(r.word("D20"), 2, "same scan resolves to the lowest number");
}

// ---------------------------------------------------------------- TANK

#[test]
fn tank_fill_drain_hysteresis_and_protection() {
    let mut r = Rig::new(&prog(
        "t : TANK; lv AT D0 : INT; out AT Y0 : BOOL; hh AT Y1 : BOOL; ll AT Y2 : BOOL;",
        "t(PV := INT_TO_REAL(lv), DRAIN := X0, ENABLE := NOT X1);\n\
         out := t.Q; hh := t.Q_HH; ll := t.Q_LL;",
    ));
    r.set_word("D0", 50).scan(1);
    assert!(!r.bit("Y0"));
    r.set_word("D0", 15).scan(1);
    assert!(r.bit("Y0"));
    r.set_word("D0", 50).scan(1);
    assert!(r.bit("Y0"), "keeps filling inside the band");
    r.set_word("D0", 80).scan(1);
    assert!(!r.bit("Y0"));
    r.set_word("D0", 50).scan(1);
    assert!(!r.bit("Y0"));
    r.set_word("D0", 10).scan(1);
    assert!(r.bit("Y0"));
    r.set("X1", true).scan(1);
    assert!(!r.bit("Y0"), "disable");
    r.set("X1", false).scan(1);
    assert!(r.bit("Y0"));
    // Mode change switches off; drain mode works inverted.
    r.set("X0", true).scan(1);
    assert!(!r.bit("Y0"));
    r.set_word("D0", 90).scan(1);
    assert!(r.bit("Y0"));
    r.set_word("D0", 50).scan(1);
    assert!(r.bit("Y0"));
    r.set_word("D0", 20).scan(1);
    assert!(!r.bit("Y0"));
    r.set_word("D0", 3).scan(1);
    assert!(r.bit("Y2") && !r.bit("Y1"));
    r.set_word("D0", 97).scan(1);
    assert!(r.bit("Y1") && !r.bit("Y2"));

    // Fail safe: HH stops filling even below HIGH_SP; LL stops draining above LOW_SP.
    let mut r = Rig::new(&prog(
        "t : TANK; lv AT D0 : INT; out AT Y0 : BOOL;",
        "t(PV := INT_TO_REAL(lv), DRAIN := X0, LIM_L := 2.0, LIM_H := 99.0); out := t.Q;",
    ));
    r.set_word("D0", 1).scan(1);
    assert!(r.bit("Y0"));
    r.set_word("D0", 96).scan(1);
    assert!(!r.bit("Y0"), "HH protection");
    r.set("X0", true).set_word("D0", 99).scan(1);
    assert!(r.bit("Y0"));
    r.set_word("D0", 4).scan(1);
    assert!(!r.bit("Y0"), "LL dry-run protection");
}

// ---------------------------------------------------------------- CONVEYOR

#[test]
fn conveyor_warns_runs_chains_and_faults() {
    let mut r = Rig::new(&prog(
        "c : CONVEYOR; up : CONVEYOR; horn AT Y0 : BOOL; run AT Y1 : BOOL; f AT Y2 : BOOL; urun AT Y3 : BOOL; st AT D20 : INT;",
        "c(START := X0, STOP := X1, PERMIT := NOT X2, MOTION := NOT X3, RST := X4, WARN_MS := 500, MOTION_MS := 300);\n\
         up(START := X5, PERMIT := c.RUN, WARN_MS := 200);\n\
         horn := c.HORN; run := c.RUN; f := c.FAULT; urun := up.RUN; st := c.STATE;",
    ));
    r.scan(1);
    r.set("X0", true).scan(1).set("X0", false);
    assert!(r.bit("Y0") && !r.bit("Y1") && r.word("D20") == 1);
    r.scan(3);
    assert!(r.bit("Y0") && !r.bit("Y1"), "still warning");
    r.scan(10);
    assert!(!r.bit("Y0") && r.bit("Y1") && r.word("D20") == 2);
    // Upstream conveyor runs only while this one runs (stop chain).
    r.set("X5", true).scan(10);
    assert!(r.bit("Y3"));
    r.set("X2", true).scan(1);
    assert!(!r.bit("Y1") && !r.bit("Y0"));
    r.scan(1);
    assert!(!r.bit("Y3"), "upstream stops with downstream");
    r.set("X2", false).scan(10);
    assert!(!r.bit("Y1"), "no automatic restart");
    // Belt slip: no motion while running trips the conveyor.
    r.set("X0", true).scan(1).set("X0", false).scan(10);
    assert!(r.bit("Y1"));
    r.set("X3", true).scan(10);
    assert!(r.bit("Y2") && !r.bit("Y1"));
    r.set("X0", true).scan(1).set("X0", false).scan(1);
    assert!(!r.bit("Y0"), "fault blocks start");
    r.set("X3", false)
        .set("X4", true)
        .scan(1)
        .set("X4", false)
        .scan(1);
    assert!(!r.bit("Y2") && !r.bit("Y1"));
    r.set("X1", true).set("X0", true).scan(1);
    assert!(!r.bit("Y0"), "stop has priority");
}

// ---------------------------------------------------------------- HEATER

#[test]
fn heater_hysteresis_min_times_and_trip() {
    let mut r = Rig::new(&prog(
        "h : HEATER; t AT D0 : INT; out AT Y0 : BOOL; trip AT Y1 : BOOL;",
        "h(PV := INT_TO_REAL(t), SP := 50.0, HYST := 2.0, LIM_HH := 90.0, ENABLE := NOT X0, RST := X1, MIN_ON_MS := 500, MIN_OFF_MS := 1000);\n\
         out := h.Q; trip := h.FAULT;",
    ));
    r.set_word("D0", 40).scan(1);
    assert!(r.bit("Y0"), "first switch-on is immediate");
    r.set_word("D0", 55).scan(4);
    assert!(r.bit("Y0"), "minimum on time");
    r.scan(1);
    assert!(!r.bit("Y0"), "off 500 ms after switching on");
    r.set_word("D0", 40).scan(9);
    assert!(!r.bit("Y0"), "minimum off time");
    r.scan(1);
    assert!(r.bit("Y0"), "on 1000 ms after switching off");
    r.set_word("D0", 49).scan(20);
    assert!(r.bit("Y0"), "inside the hysteresis band");
    // Over-temperature trips at once and latches.
    r.set_word("D0", 95).scan(1);
    assert!(!r.bit("Y0") && r.bit("Y1"));
    r.set_word("D0", 40).scan(20);
    assert!(!r.bit("Y0") && r.bit("Y1"));
    r.set("X1", true).scan(1).set("X1", false).scan(15);
    assert!(r.bit("Y0") && !r.bit("Y1"));
    // Disable switches off at once, ignoring MIN_ON.
    r.set("X0", true).scan(1);
    assert!(!r.bit("Y0"));
    // Reset does not clear the trip while still too hot.
    r.set("X0", false)
        .set_word("D0", 92)
        .scan(1)
        .set("X1", true)
        .scan(1);
    assert!(r.bit("Y1"));
}

// ---------------------------------------------------------------- DOSING

#[test]
fn dosing_counts_pulses_and_stops_at_batch() {
    let mut r = Rig::new(&prog(
        "d : DOSING; v AT Y0 : BOOL; done AT Y1 : BOOL; f AT Y2 : BOOL; vol AT D30 : REAL; n AT D20 : DINT;",
        "d(START := X0, STOP := X1, PULSE := X2, VOL_PER_PULSE := 0.5, SP := 3.0, PREACT := 0.5, NO_FLOW_MS := 500);\n\
         v := d.VALVE_OUT; done := d.DONE; f := d.FAULT; vol := d.VOLUME; n := d.PULSES;",
    ));
    r.scan(1);
    r.set("X0", true).scan(1).set("X0", false);
    assert!(r.bit("Y0") && !r.bit("Y1"));
    for _ in 0..4 {
        r.set("X2", true).scan(1).set("X2", false).scan(1);
    }
    assert!(r.bit("Y0") && (real(&r.sim, "D30") - 2.0).abs() < 1e-4);
    r.set("X2", true).scan(1).set("X2", false).scan(1);
    assert!(
        !r.bit("Y0") && r.bit("Y1") && !r.bit("Y2"),
        "closes at SP - PREACT"
    );
    r.set("X2", true).scan(1).set("X2", false).scan(1);
    assert!(
        r.word("D20") == 6 && (real(&r.sim, "D30") - 3.0).abs() < 1e-4,
        "overrun counted"
    );
    // New batch, no flow: fault closes the valve.
    r.set("X0", true).scan(1).set("X0", false);
    assert!(r.bit("Y0") && !r.bit("Y1") && r.word("D20") == 0);
    r.scan(10);
    assert!(!r.bit("Y0") && r.bit("Y2") && !r.bit("Y1"));
    // Abort.
    r.set("X0", true)
        .scan(1)
        .set("X0", false)
        .set("X1", true)
        .scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y1") && !r.bit("Y2"));
}

// ---------------------------------------------------------------- GATE

#[test]
fn gate_limits_photo_eye_and_timeout() {
    let mut r = Rig::new(&prog(
        "g : GATE; oo AT Y0 : BOOL; co AT Y1 : BOOL; rv AT Y2 : BOOL; f AT Y3 : BOOL; st AT D20 : INT;",
        "g(OPEN := X0, CLOSE := X1, STOP := X2, PHOTO := X3, LS_OPEN := X4, LS_CLOSED := X5, RST := X6, TRAVEL_MS := 1000);\n\
         oo := g.OPEN_OUT; co := g.CLOSE_OUT; rv := g.REVERSED; f := g.FAULT; st := g.STATE;",
    ));
    r.scan(1);
    r.set("X5", true).set("X0", true).scan(1).set("X0", false);
    assert!(r.bit("Y0") && !r.bit("Y1") && r.word("D20") == 1);
    r.set("X5", false).scan(3).set("X4", true).scan(1);
    assert!(!r.bit("Y0") && r.word("D20") == 0);
    // Close, interrupted by the photo-eye: reverses to open.
    r.set("X1", true).scan(1).set("X1", false);
    assert!(r.bit("Y1") && !r.bit("Y0"));
    r.set("X4", false).scan(2).set("X3", true).scan(1);
    assert!(
        !r.bit("Y0") && !r.bit("Y1") && r.bit("Y2") && r.word("D20") == 1,
        "both outputs off for one scan on reversal"
    );
    r.scan(1);
    assert!(r.bit("Y0") && !r.bit("Y1") && r.bit("Y2"));
    r.set("X4", true).scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y1"));
    // Close is ignored while the beam is interrupted.
    r.set("X1", true).scan(1).set("X1", false).scan(1);
    assert!(!r.bit("Y1"));
    // Close never reaches the limit: travel timeout.
    r.set("X3", false).set("X1", true).scan(1).set("X1", false);
    assert!(r.bit("Y1") && !r.bit("Y2"));
    r.set("X4", false);
    for _ in 0..20 {
        r.scan(1);
        assert!(!(r.bit("Y0") && r.bit("Y1")));
    }
    assert!(!r.bit("Y1") && r.bit("Y3"));
    r.set("X0", true).scan(1).set("X0", false).scan(1);
    assert!(!r.bit("Y0"), "fault blocks commands");
    r.set("X6", true)
        .scan(1)
        .set("X6", false)
        .set("X0", true)
        .scan(1);
    assert!(r.bit("Y0") && !r.bit("Y3"));
    r.set("X2", true).scan(1);
    assert!(!r.bit("Y0"));
}

// ---------------------------------------------------------------- review findings

/// No automatic (re)start at power-up: a start command (or a cycle sensor) already TRUE on the
/// first scan must not start a motor, gate or batch, nor count a cycle.
#[test]
fn commands_held_at_power_up_are_ignored() {
    let cases = [
        ("x : MOTOR;", "x(START := X0, FB_MS := 0); o := x.RUN;"),
        ("x : MOTOR_REV;", "x(FWD := X0); o := x.FWD_OUT;"),
        ("x : STAR_DELTA;", "x(START := X0); o := x.MAIN;"),
        ("x : CONVEYOR;", "x(START := X0); o := x.HORN;"),
        ("x : GATE;", "x(OPEN := X0); o := x.OPEN_OUT;"),
        (
            "x : DOSING;",
            "x(START := X0, SP := 10.0, NO_FLOW_MS := 0); o := x.VALVE_OUT;",
        ),
        ("x : CYCLE_COUNTER;", "x(CYCLE := X0); o := x.TOTAL > 0;"),
        ("x : HYD_PUMP;", "x(START := X0); o := x.MOTOR_OUT;"),
        ("x : CYLINDER;", "x(EXTEND := X0); o := x.EXT_OUT;"),
        ("x : AXIS_2SPEED;", "x(FWD := X0); o := x.FWD_OUT;"),
        (
            "x : VFD;",
            "x(RUN := X0, SPEED_HZ := 10.0); o := x.FWD_OUT;",
        ),
    ];
    for (decl, body) in cases {
        let mut r = Rig::new(&prog(&format!("{decl} o AT Y0 : BOOL;"), body));
        r.set("X0", true).scan(5);
        assert!(!r.bit("Y0"), "{decl} acted on a command held at power-up");
        r.set("X0", false).scan(1).set("X0", true).scan(1);
        assert!(r.bit("Y0"), "{decl} must act on a real edge");
    }
}

/// GAP_MS 0 must still leave at least one scan between star off and delta on.
#[test]
fn star_delta_zero_gap_breaks_before_make() {
    let mut r = Rig::new(&prog(
        "s : STAR_DELTA; st AT Y1 : BOOL; de AT Y2 : BOOL; step AT D20 : INT;",
        "s(START := X0, STAR_MS := 300, GAP_MS := 0); st := s.STAR; de := s.DELTA; step := s.STATE;",
    ));
    r.scan(1).set("X0", true);
    let mut prev_star = false;
    let mut saw_gap = false;
    for _ in 0..10 {
        r.scan(1);
        let (st, de) = (r.bit("Y1"), r.bit("Y2"));
        assert!(
            !(prev_star && de),
            "delta switched on in the scan star dropped"
        );
        saw_gap |= r.word("D20") == 2 && !st && !de;
        prev_star = st;
    }
    assert!(saw_gap && r.bit("Y2"));
}

/// A second START edge during a running batch must not clear the count (overdose).
#[test]
fn dosing_start_during_batch_is_ignored() {
    let mut r = Rig::new(&prog(
        "d : DOSING; v AT Y0 : BOOL; done AT Y1 : BOOL; n AT D20 : DINT;",
        "d(START := X0, PULSE := X2, SP := 5.0, NO_FLOW_MS := 0);
         v := d.VALVE_OUT; done := d.DONE; n := d.PULSES;",
    ));
    r.scan(1).set("X0", true).scan(1).set("X0", false).scan(1);
    for _ in 0..3 {
        r.set("X2", true).scan(1).set("X2", false).scan(1);
    }
    r.set("X0", true).scan(1).set("X0", false).scan(1);
    assert_eq!(
        r.word("D20"),
        3,
        "restart during the batch cleared the count"
    );
    for _ in 0..2 {
        r.set("X2", true).scan(1).set("X2", false).scan(1);
    }
    assert!(!r.bit("Y0") && r.bit("Y1") && r.word("D20") == 5);
    // After completion a new START does begin a new batch.
    r.set("X0", true).scan(1).set("X0", false).scan(1);
    assert!(r.bit("Y0") && !r.bit("Y1") && r.word("D20") == 0);
}

/// Demand falling while a stage delay runs: the destage delay starts from the change.
#[test]
fn lead_lag_direction_change_restarts_delay() {
    let mut r = Rig::new(&prog(
        "l : LEAD_LAG; d AT D0 : INT; n AT D20 : INT;",
        "l(DEMAND := d, STAGE_MS := 500, DESTAGE_MS := 1000); n := l.RUN_COUNT;",
    ));
    r.set_word("D0", 2).scan(20);
    assert_eq!(r.word("D20"), 2);
    r.set_word("D0", 3).scan(4);
    assert_eq!(r.word("D20"), 2, "stage delay still running");
    r.set_word("D0", 1).scan(8);
    assert_eq!(r.word("D20"), 2, "destage used the elapsed staging time");
    r.scan(5);
    assert_eq!(r.word("D20"), 1);
}

/// Documented minimum on time: MIN_ON_MS plus two scan times.
#[test]
fn heater_min_on_time_is_exact() {
    let mut r = Rig::new(&prog(
        "h : HEATER; t AT D0 : INT; out AT Y0 : BOOL;",
        "h(PV := INT_TO_REAL(t), SP := 50.0, MIN_ON_MS := 500); out := h.Q;",
    ));
    r.set_word("D0", 40).scan(1);
    assert!(r.bit("Y0"));
    r.set_word("D0", 60);
    for i in 0..4 {
        r.scan(1);
        assert!(r.bit("Y0"), "switched off after {} ms", (i + 1) * 100);
    }
    r.scan(1);
    assert!(!r.bit("Y0"), "on for exactly MIN_ON_MS (5 scans of 100 ms)");
}

// ---------------------------------------------------------------- HEATER (two-point rule)

/// Two-point rule: on at PV <= SP - HYST/2, off at PV >= SP + HYST/2.
#[test]
fn heater_switches_at_half_band_around_setpoint() {
    let mut r = Rig::new(&prog(
        "h : HEATER; t AT D0 : INT; out AT Y0 : BOOL;",
        "h(PV := INT_TO_REAL(t), SP := 50.0, HYST := 4.0); out := h.Q;",
    ));
    r.set_word("D0", 49).scan(1);
    assert!(!r.bit("Y0"), "first scan inside the band: stays off");
    r.set_word("D0", 48).scan(1);
    assert!(r.bit("Y0"), "on at SP - HYST/2");
    r.set_word("D0", 51).scan(1);
    assert!(r.bit("Y0"), "still on below SP + HYST/2");
    r.set_word("D0", 52).scan(1);
    assert!(!r.bit("Y0"), "off at SP + HYST/2");
    r.set_word("D0", 49).scan(1);
    assert!(!r.bit("Y0"), "stays off inside the band");
}

// ---------------------------------------------------------------- PUMP_ALT (rotation by hours)

/// With ROT_H > 0 the lead does not change on stop; unconnected AVAIL inputs mean available.
#[test]
fn pump_alt_rotation_by_hours_does_not_alternate_on_stop() {
    let mut r = Rig::new(&prog(
        "p : PUMP_ALT; p1 AT Y0 : BOOL; p2 AT Y1 : BOOL; lead AT D20 : INT; lh AT D21 : INT;",
        "p(DEMAND := X0, ROT_H := 2); p1 := p.PUMP1; p2 := p.PUMP2; lead := p.LEAD; lh := p.LEAD_H;",
    ));
    r.scan(1);
    for _ in 0..4 {
        r.set("X0", true).scan(3);
        assert!(r.bit("Y0") && !r.bit("Y1") && r.word("D20") == 1);
        r.set("X0", false).scan(2);
        assert!(!r.bit("Y0") && !r.bit("Y1") && r.word("D20") == 1);
    }
    assert_eq!(r.word("D21"), 0);
}

/// Runs a little over one hour of virtual time, hence ignored by default:
/// `cargo test --lib st_stdlib_process -- --ignored`.
#[test]
#[ignore]
fn pump_alt_swaps_lead_after_rot_h_run_hours() {
    let mut r = Rig::new(&prog(
        "p : PUMP_ALT; p1 AT Y0 : BOOL; p2 AT Y1 : BOOL; lead AT D20 : INT; lh AT D21 : INT;",
        "p(DEMAND := X0, ROT_H := 1); p1 := p.PUMP1; p2 := p.PUMP2; lead := p.LEAD; lh := p.LEAD_H;",
    ));
    r.set("X0", true).scan(35_900);
    assert!(
        r.bit("Y0") && !r.bit("Y1") && r.word("D20") == 1,
        "less than an hour"
    );
    let mut swapped = false;
    for _ in 0..1_000 {
        r.scan(1);
        assert!(
            !(r.bit("Y0") && r.bit("Y1")),
            "both pumps ran during the swap"
        );
        if r.word("D20") == 2 {
            swapped = true;
            break;
        }
    }
    assert!(swapped, "no swap after one run hour");
    assert!(
        !r.bit("Y0") && r.bit("Y1") && r.word("D21") == 0,
        "pump 2 takes over at once"
    );
    // Stopping no longer alternates.
    r.set("X0", false).scan(1).set("X0", true).scan(1);
    assert!(r.bit("Y1") && r.word("D20") == 2);
}

// ---------------------------------------------------------------- VFD

fn vfd_rig(params: &str) -> Rig {
    idle(Rig::new(&prog(
        "v : VFD; sp AT D0 : INT; fw AT Y0 : BOOL; rv AT Y1 : BOOL; ro AT Y2 : BOOL; ats AT Y3 : BOOL; f AT Y4 : BOOL; ao AT D20 : INT; hz AT D30 : REAL;",
        &format!(
            "v(RUN := X0, REV := X1, RST := X2, FAULT_IN := X3, READY_IN := NOT X4, SPEED_HZ := INT_TO_REAL(sp){params});\n\
             fw := v.FWD_OUT; rv := v.REV_OUT; ro := v.RESET_OUT; ats := v.AT_SPEED; f := v.FAULT; ao := v.OUT; hz := v.REF_HZ;"
        ),
    )))
}

#[test]
fn vfd_ramps_up_and_down_in_the_plc() {
    let mut r = vfd_rig(", MAX_HZ := 50.0, ACC_MS := 1000, DEC_MS := 500, OUT_MAX := 4000");
    r.set_word("D0", 50).scan(3);
    assert!(!r.bit("Y0") && r.word("D20") == 0, "no RUN, no output");
    r.set("X0", true).scan(1);
    assert!(
        r.bit("Y0") && !r.bit("Y1") && !r.bit("Y3"),
        "run output at once"
    );
    let mut prev = r.word("D20");
    let mut scans = 1;
    while !r.bit("Y3") {
        r.scan(1);
        scans += 1;
        let ao = r.word("D20");
        assert!(
            ao >= prev && ao - prev <= 401,
            "acceleration step {prev} -> {ao}"
        );
        prev = ao;
        assert!(scans < 30, "never reached the setpoint");
    }
    assert!((9..=13).contains(&scans), "1 s ramp took {scans} scans");
    assert!(r.word("D20") == 4000 && (real(&r.sim, "D30") - 50.0).abs() < 1e-3);
    // RUN off: the PLC ramps down with DEC_MS and keeps the run output until 0 Hz.
    r.set("X0", false).scan(1);
    assert!(r.bit("Y0") && !r.bit("Y3") && r.word("D20") < 4000);
    let mut scans = 1;
    while r.bit("Y0") {
        let before = r.word("D20");
        r.scan(1);
        scans += 1;
        assert!(before - r.word("D20") <= 801, "deceleration step");
        assert!(scans < 20, "never stopped");
    }
    assert!((4..=8).contains(&scans), "0.5 s ramp took {scans} scans");
    assert!(r.word("D20") == 0 && !r.bit("Y1"));
}

#[test]
fn vfd_reverses_through_zero_speed() {
    let mut r = vfd_rig(", ACC_MS := 1000, DEC_MS := 1000");
    r.set_word("D0", 25).set("X0", true).scan(20);
    assert!(
        r.bit("Y0") && r.bit("Y3") && r.word("D20") == 2000,
        "25 Hz of 50 Hz"
    );
    r.set("X1", true);
    let mut min_ao = i16::MAX;
    let mut saw_fwd_after_request = false;
    for _ in 0..30 {
        r.scan(1);
        let (f, v) = (r.bit("Y0"), r.bit("Y1"));
        assert!(!(f && v), "forward and reverse together");
        if f {
            saw_fwd_after_request = true;
        }
        if v {
            min_ao = min_ao.min(r.word("D20"));
        }
    }
    assert!(saw_fwd_after_request, "decelerates forward first");
    assert!(
        min_ao <= 400,
        "reverse starts from (near) zero, min {min_ao}"
    );
    assert!(r.bit("Y1") && r.bit("Y3") && r.word("D20") == 2000);
}

#[test]
fn vfd_limits_speed_and_steps_without_ramp() {
    let mut r = vfd_rig(", MAX_HZ := 60.0, ACC_MS := 0, DEC_MS := 0, OUT_MAX := 1000");
    r.set_word("D0", 80).set("X0", true).scan(1);
    assert!(
        r.bit("Y0") && r.word("D20") == 0 && !r.bit("Y3"),
        "run output first, speed follows in the next scan"
    );
    r.scan(1);
    assert!(
        r.bit("Y3") && r.word("D20") == 1000,
        "clamped to MAX_HZ, no ramp"
    );
    r.set_word("D0", 30).scan(1);
    assert_eq!(r.word("D20"), 500);
    r.set_word("D0", -5).scan(1);
    assert!(
        !r.bit("Y0") && !r.bit("Y1") && r.word("D20") == 0,
        "negative setpoint stops"
    );
    r.set_word("D0", 0).scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y3"), "0 Hz keeps the drive stopped");
}

#[test]
fn vfd_fault_coasts_latches_and_resets_with_a_pulse() {
    let mut r = vfd_rig(", ACC_MS := 1000");
    r.set_word("D0", 50).set("X0", true).scan(5);
    assert!(r.bit("Y0") && r.word("D20") > 0);
    r.set("X3", true).scan(1);
    assert!(
        !r.bit("Y0") && r.bit("Y4") && r.word("D20") == 0,
        "fault coasts at once"
    );
    r.set("X3", false).scan(3);
    assert!(r.bit("Y4") && !r.bit("Y0"), "fault latched");
    // Reset pulse about 500 ms; RUN still held: no automatic restart.
    r.set("X2", true).scan(1).set("X2", false);
    assert!(r.bit("Y2") && !r.bit("Y4"));
    let mut pulse = 1;
    while r.bit("Y2") {
        r.scan(1);
        pulse += 1;
        assert!(pulse < 20);
    }
    assert!((5..=7).contains(&pulse), "reset pulse {pulse} scans");
    r.scan(5);
    assert!(!r.bit("Y0"), "RUN must be released after a fault");
    r.set("X0", false).scan(1).set("X0", true).scan(1);
    assert!(r.bit("Y0"));
    // A fault still present is not cleared by the reset.
    r.set("X3", true)
        .scan(1)
        .set("X2", true)
        .scan(1)
        .set("X2", false)
        .scan(10);
    assert!(r.bit("Y4") && !r.bit("Y0"));
    // No reset pulse while running.
    let mut r = vfd_rig("");
    r.set_word("D0", 20)
        .set("X0", true)
        .scan(3)
        .set("X2", true)
        .scan(1);
    assert!(r.bit("Y0") && !r.bit("Y2"));
}

#[test]
fn vfd_not_ready_coasts_without_fault_and_needs_new_run() {
    let mut r = vfd_rig(", ACC_MS := 500");
    r.set_word("D0", 40).set("X0", true).scan(10);
    assert!(r.bit("Y0") && r.bit("Y3"));
    r.set("X4", true).scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y4") && r.word("D20") == 0);
    r.set("X4", false).scan(5);
    assert!(!r.bit("Y0"), "no automatic restart when ready returns");
    r.set("X0", false).scan(1).set("X0", true).scan(1);
    assert!(r.bit("Y0"));
    // Not ready while stopped: RUN pressed later works once ready.
    let mut r = vfd_rig("");
    r.set_word("D0", 40)
        .set("X4", true)
        .scan(2)
        .set("X4", false)
        .scan(1);
    r.set("X0", true).scan(1);
    assert!(r.bit("Y0"));
}

// ---------------------------------------------------------------- HYD_PUMP

fn hyd_rig(unload_ms: i32) -> Rig {
    idle(Rig::new(&prog(
        "h : HYD_PUMP; m AT Y0 : BOOL; dv AT Y1 : BOOL; lv AT Y2 : BOOL; rd AT Y3 : BOOL; f AT Y4 : BOOL; uo AT Y5 : BOOL; code AT D20 : INT; st AT D21 : INT;",
        &format!(
            "h(START := X0, STOP := X1, PERMIT := NOT X2, PHASE_OK := NOT X3, OVERLOAD := X4, DEMAND := X5, RST := X6, PAUSE := X7, UNLOAD_MS := {unload_ms});\n\
             m := h.MOTOR_OUT; dv := h.DUMP_VALVE; lv := h.LOAD_VALVE; rd := h.READY; f := h.FAULT; uo := h.UNIT_ON; code := h.FAULT_CODE; st := h.STATE;"
        ),
    )))
}

#[test]
fn hyd_pump_unloaded_start_then_loads_on_demand() {
    let mut r = hyd_rig(500);
    r.set("X5", true).set("X0", true).scan(1).set("X0", false);
    assert!(r.bit("Y0") && r.bit("Y1") && !r.bit("Y2") && !r.bit("Y3") && r.word("D21") == 1);
    let mut scans = 1;
    while r.word("D21") == 1 {
        assert!(r.bit("Y1") && !r.bit("Y2"), "unloaded during run-up");
        r.scan(1);
        scans += 1;
        assert!(scans < 20);
    }
    assert!((6..=7).contains(&scans), "run-up took {scans} scans");
    assert!(
        r.word("D21") == 3 && r.bit("Y2") && !r.bit("Y1") && r.bit("Y3"),
        "loads with demand"
    );
    r.set("X5", false).scan(1);
    assert!(
        r.word("D21") == 2 && r.bit("Y1") && !r.bit("Y2") && r.bit("Y0") && r.bit("Y3"),
        "idle unloaded"
    );
    r.set("X5", true).scan(1);
    assert!(r.word("D21") == 3 && r.bit("Y2"));
    r.set("X1", true).scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y1") && !r.bit("Y2") && !r.bit("Y3") && r.word("D21") == 0);
    // PERMIT loss stops without a fault; no automatic restart.
    r.set("X1", false)
        .set("X0", true)
        .scan(1)
        .set("X0", false)
        .scan(10);
    assert!(r.bit("Y0"));
    r.set("X2", true).scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y4"));
    r.set("X2", false).scan(5);
    assert!(!r.bit("Y0"));
}

#[test]
fn hyd_pump_zero_unload_time_still_starts_unloaded() {
    let mut r = hyd_rig(0);
    r.set("X5", true).set("X0", true).scan(1);
    assert!(
        r.word("D21") == 1 && r.bit("Y1") && !r.bit("Y2"),
        "at least one unloaded scan"
    );
    r.scan(2);
    assert!(r.word("D21") == 3 && r.bit("Y2"));
}

#[test]
fn hyd_pump_overload_and_phase_faults() {
    let mut r = hyd_rig(300);
    r.set("X0", true).scan(1).set("X0", false).scan(5);
    assert!(r.bit("Y3"));
    r.set("X4", true).scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y1") && r.bit("Y4") && r.word("D20") == 1);
    r.set("X6", true).scan(1).set("X6", false);
    assert!(r.bit("Y4"), "reset ignored while overload is present");
    r.set("X3", true).scan(1);
    assert_eq!(r.word("D20"), 1, "first cause kept");
    r.set("X4", false)
        .set("X3", false)
        .set("X6", true)
        .scan(1)
        .set("X6", false)
        .scan(1);
    assert!(
        !r.bit("Y4") && r.word("D20") == 0 && !r.bit("Y0"),
        "reset does not restart"
    );
    // Phase-sequence fault at standstill blocks the start.
    r.set("X3", true).scan(1);
    assert!(r.bit("Y4") && r.word("D20") == 2);
    r.set("X0", true).scan(1).set("X0", false).scan(1);
    assert!(!r.bit("Y0"));
    r.set("X3", false)
        .set("X6", true)
        .scan(1)
        .set("X6", false)
        .scan(1);
    r.set("X0", true).scan(1);
    assert!(r.bit("Y0") && !r.bit("Y4"));
}

// ---------------------------------------------------------------- MOTOR_REV (permit)

/// PERMIT loss drops the running direction at once and cancels the request: no restart when
/// PERMIT returns.
#[test]
fn motor_rev_permit_loss_cancels_request() {
    let mut r = Rig::new(&prog(
        "m : MOTOR_REV; f AT Y0 : BOOL; v AT Y1 : BOOL; w AT Y2 : BOOL; d AT D20 : INT;",
        "m(FWD := X0, REV := X1, PERMIT := NOT X3, DEAD_MS := 300);\n\
         f := m.FWD_OUT; v := m.REV_OUT; w := m.WAITING; d := m.DIR;",
    ));
    r.scan(1);
    r.set("X1", true).scan(1).set("X1", false).scan(1);
    assert!(r.bit("Y1") && r.word("D20") == -1);
    r.set("X3", true).scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y1") && !r.bit("Y2") && r.word("D20") == 0);
    r.set("X3", false).scan(10);
    assert!(!r.bit("Y0") && !r.bit("Y1") && !r.bit("Y2"), "no restart");
    // No permit: a command is not stored.
    r.set("X3", true)
        .set("X0", true)
        .scan(1)
        .set("X0", false)
        .scan(1);
    r.set("X3", false).scan(10);
    assert!(!r.bit("Y0") && !r.bit("Y2"));
}

// ---------------------------------------------------------------- VFD (reversal detail)

/// The reverse run output comes on in the scan the reference reaches 0 Hz and never with a
/// non-zero reference left over from the forward run.
#[test]
fn vfd_switches_direction_only_at_zero_reference() {
    let mut r = vfd_rig(", ACC_MS := 500, DEC_MS := 500");
    r.set_word("D0", 50).set("X0", true).scan(10);
    assert!(r.bit("Y0") && r.word("D20") == 4000);
    r.set("X1", true);
    let mut prev_fwd = true;
    let mut switched = false;
    for _ in 0..20 {
        r.scan(1);
        let (f, v) = (r.bit("Y0"), r.bit("Y1"));
        assert!(!(f && v));
        if v && prev_fwd {
            assert_eq!(r.word("D20"), 0, "reverse started with a speed reference");
            switched = true;
        }
        prev_fwd = f;
    }
    assert!(switched && r.bit("Y1") && r.word("D20") == 4000);
}

// ---------------------------------------------------------------- documentation examples

/// Every `Example:` line of process.st compiles inside a small program, and the Turkish
/// `Örnek:` line is identical.
#[test]
fn every_process_example_compiles() {
    let src = include_str!("st/stdlib/process.st");
    let extra = "lvl : REAL; temp : REAL; hz : REAL; hours : DINT; heat_step : BOOL;";
    let mut en: Option<String> = None;
    let mut tr: Option<String> = None;
    let mut blocks = 0;
    for line in src.lines().map(str::trim) {
        if let Some(e) = line.strip_prefix("Example:") {
            en = Some(e.trim().to_string());
        } else if let Some(e) = line.strip_prefix("Örnek:") {
            tr = Some(e.trim().to_string());
        } else if let Some(name) = line.strip_prefix("FUNCTION_BLOCK ") {
            let name = name.trim();
            let ex = en
                .take()
                .unwrap_or_else(|| panic!("{name}: no Example line"));
            assert_eq!(
                tr.take().as_deref(),
                Some(ex.as_str()),
                "{name}: Örnek differs"
            );
            let inst = ex.split('(').next().unwrap().trim();
            let source = prog(&format!("{inst} : {name}; {extra}"), &ex);
            plc::compile_st(&source).unwrap_or_else(|e| panic!("{name}: {e:?}\n{source}"));
            blocks += 1;
        }
    }
    assert_eq!(blocks, 21);
}

// ---------------------------------------------------------------- GATE (break before make)

/// A reversal by command never switches one direction off and the other on in the same scan.
#[test]
fn gate_reversal_breaks_before_make() {
    let mut r = Rig::new(&prog(
        "g : GATE; oo AT Y0 : BOOL; co AT Y1 : BOOL; st AT D20 : INT;",
        "g(OPEN := X0, CLOSE := X1, TRAVEL_MS := 5000); oo := g.OPEN_OUT; co := g.CLOSE_OUT; st := g.STATE;",
    ));
    r.scan(1);
    r.set("X0", true).scan(1).set("X0", false).scan(3);
    assert!(r.bit("Y0"));
    let (mut prev_o, mut prev_c) = (true, false);
    let mut check = |r: &mut Rig| {
        r.scan(1);
        let (o, c) = (r.bit("Y0"), r.bit("Y1"));
        assert!(
            !(o && c) && !(prev_o && c) && !(prev_c && o),
            "direct reversal"
        );
        prev_o = o;
        prev_c = c;
    };
    r.set("X1", true);
    check(&mut r);
    r.set("X1", false);
    check(&mut r);
    assert!(r.bit("Y1") && r.word("D20") == 2);
    r.set("X0", true);
    check(&mut r);
    r.set("X0", false);
    check(&mut r);
    assert!(r.bit("Y0") && r.word("D20") == 1);
}

/// Stuck open with OPEN FALSE: RST cannot clear the fault; commanding OPEN and resetting does.
#[test]
fn valve_reset_rules_for_a_stuck_open_valve() {
    let mut r = valve_rig();
    r.set("X1", true).scan(7);
    assert!(r.bit("Y4") && r.word("D20") == 2);
    r.set("X3", true).scan(1).set("X3", false).scan(1);
    assert!(
        r.bit("Y4") && r.word("D20") == 2 && !r.bit("Y0"),
        "re-trips at once"
    );
    r.set("X0", true)
        .set("X3", true)
        .scan(1)
        .set("X3", false)
        .scan(10);
    assert!(
        !r.bit("Y4") && r.bit("Y0") && r.bit("Y1"),
        "open is what was commanded"
    );
}

// ---------------------------------------------------------------- consistency items (first scan, renames)

/// FIRST is 0 at start; an input already TRUE on the first scan becomes FIRST in that scan.
#[test]
fn first_out4_first_scan() {
    let src = prog(
        "f : FIRST_OUT4; first AT D20 : INT;",
        "f(IN1 := X0, IN2 := X1, IN3 := X2, IN4 := X3, ACK := X4); first := f.FIRST;",
    );
    let mut r = Rig::new(&src);
    r.scan(1);
    assert_eq!(r.word("D20"), 0);
    let mut r = Rig::new(&src);
    r.set("X2", true).scan(1);
    assert_eq!(
        r.word("D20"),
        3,
        "an input TRUE on the first scan is FIRST at once"
    );
}

/// OPEN TRUE at power-up: Q in the first scan, no fault; the travel time starts in the second
/// scan, so a valve that does not move trips TRAVEL_MS plus two scans after power-up.
#[test]
fn valve_first_scan_with_open_true() {
    let mut r = valve_rig();
    r.set("X0", true).set("X2", true).scan(1);
    assert!(r.bit("Y0") && !r.bit("Y4") && r.bit("Y3"));
    r.scan(5);
    assert!(r.bit("Y0") && !r.bit("Y4"), "travel time still running");
    r.scan(1);
    assert!(!r.bit("Y0") && r.bit("Y4") && r.word("D20") == 1);
}

/// RST is a level: while it is TRUE nothing is counted. SERVICE_N <= 0: no alarm, nothing left.
#[test]
fn cycle_counter_reset_level_and_no_service_interval() {
    let mut r = Rig::new(&prog(
        "c : CYCLE_COUNTER; tot AT D20 : DINT; rem AT D24 : DINT; due AT Y0 : BOOL;",
        "c(CYCLE := X0, RST := X1, SERVICE_N := -5, ACK := X2);\n\
         tot := c.TOTAL; rem := c.REM_N; due := c.SERVICE_DUE;",
    ));
    r.scan(1);
    for _ in 0..3 {
        r.set("X0", true).scan(1).set("X0", false).scan(1);
    }
    assert!(r.word("D20") == 3 && r.word("D24") == 0 && !r.bit("Y0"));
    r.set("X1", true);
    for _ in 0..3 {
        r.set("X0", true).scan(1).set("X0", false).scan(1);
    }
    assert_eq!(r.word("D20"), 0, "nothing counts while RST is TRUE");
    r.set("X1", false).set("X0", true).scan(1);
    assert_eq!(r.word("D20"), 1);
}

/// REM_H: whole hours left; 0 when SERVICE_H <= 0.
#[test]
fn runtime_rem_h_hours_left() {
    for (service, left) in [(5, 5), (1, 1), (0, 0), (-3, 0)] {
        let mut r = Rig::new(&prog(
            "rt : RUNTIME; left AT D20 : DINT; due AT Y0 : BOOL;",
            &format!(
                "rt(RUN := X0, RST := X1, SERVICE_H := {service}, ACK := X2); left := rt.REM_H; due := rt.SERVICE_DUE;"
            ),
        ));
        r.set("X1", true)
            .scan(1)
            .set("X1", false)
            .set("X0", true)
            .scan(20);
        assert_eq!(r.word("D20"), left, "SERVICE_H {service}");
        assert!(!r.bit("Y0"));
    }
}

/// LOCKED shows a switch waiting for the minimum time; a negative HYST counts as 0.
#[test]
fn heater_locked_and_negative_hysteresis() {
    let mut r = Rig::new(&prog(
        "h : HEATER; t AT D0 : INT; out AT Y0 : BOOL; lk AT Y1 : BOOL;",
        "h(PV := INT_TO_REAL(t), SP := 50.0, HYST := 2.0, MIN_ON_MS := 1000); out := h.Q; lk := h.LOCKED;",
    ));
    r.set_word("D0", 40).scan(1);
    assert!(r.bit("Y0") && !r.bit("Y1"));
    r.set_word("D0", 60).scan(3);
    assert!(
        r.bit("Y0") && r.bit("Y1"),
        "wants off, minimum on time running"
    );
    r.scan(10);
    assert!(!r.bit("Y0") && !r.bit("Y1"));
    let mut r = Rig::new(&prog(
        "h : HEATER; t AT D0 : INT; out AT Y0 : BOOL;",
        "h(PV := INT_TO_REAL(t), SP := 50.0, HYST := -4.0); out := h.Q;",
    ));
    r.set_word("D0", 50).scan(1);
    assert!(r.bit("Y0"), "HYST < 0 acts as 0: on at PV = SP");
    r.set_word("D0", 51).scan(1);
    assert!(!r.bit("Y0"), "off just above SP");
}

// ---------------------------------------------------------------- HYD_PUMP (pause, unit on)

#[test]
fn hyd_pump_pause_keeps_the_unit_on_and_restarts_unloaded() {
    let mut r = hyd_rig(500);
    assert!(!r.bit("Y5"), "unit off in STATE 0");
    r.set("X5", true)
        .set("X0", true)
        .scan(1)
        .set("X0", false)
        .scan(10);
    assert!(r.word("D21") == 3 && r.bit("Y5"));
    r.set("X7", true).scan(1);
    assert!(
        r.word("D21") == 4
            && !r.bit("Y0")
            && !r.bit("Y1")
            && !r.bit("Y2")
            && !r.bit("Y3")
            && r.bit("Y5"),
        "paused: everything off, unit on"
    );
    r.scan(10);
    assert_eq!(r.word("D21"), 4);
    r.set("X7", false).scan(1);
    assert!(r.word("D21") == 1 && r.bit("Y0") && r.bit("Y1") && !r.bit("Y2") && r.bit("Y5"));
    let mut scans = 1;
    while r.word("D21") == 1 {
        r.scan(1);
        scans += 1;
        assert!(scans < 20);
    }
    assert!(
        (6..=7).contains(&scans),
        "run-up after the pause took {scans} scans"
    );
    assert!(r.word("D21") == 3 && r.bit("Y2") && r.bit("Y3"));
}

#[test]
fn hyd_pump_stop_and_fault_win_over_pause() {
    let mut r = hyd_rig(300);
    r.set("X0", true).scan(1).set("X0", false).scan(5);
    r.set("X7", true).scan(1);
    assert_eq!(r.word("D21"), 4);
    r.set("X1", true).scan(1);
    assert!(r.word("D21") == 0 && !r.bit("Y5"), "STOP in STATE 4");
    r.set("X1", false).set("X7", false).scan(3);
    assert_eq!(r.word("D21"), 0, "no restart without a START edge");
    // Fault while paused.
    r.set("X0", true).scan(1).set("X0", false).scan(5);
    r.set("X7", true).scan(1).set("X4", true).scan(1);
    assert!(r.word("D21") == 0 && r.bit("Y4") && r.word("D20") == 1 && !r.bit("Y5"));
    // PERMIT loss while paused.
    let mut r = hyd_rig(300);
    r.set("X7", true).set("X0", true).scan(1).set("X0", false);
    assert_eq!(r.word("D21"), 4);
    r.set("X2", true).scan(1);
    assert!(r.word("D21") == 0 && !r.bit("Y4") && !r.bit("Y5"));
}

#[test]
fn hyd_pump_pause_in_state_0_and_start_while_paused() {
    let mut r = hyd_rig(300);
    r.set("X7", true).scan(5);
    assert!(
        r.word("D21") == 0 && !r.bit("Y5"),
        "PAUSE alone does nothing"
    );
    r.set("X0", true).scan(1).set("X0", false);
    assert!(
        r.word("D21") == 4 && r.bit("Y5") && !r.bit("Y0"),
        "START while paused: 0 -> 4"
    );
    r.scan(3);
    assert_eq!(r.word("D21"), 4);
    r.set("X7", false).scan(1);
    assert!(r.word("D21") == 1 && r.bit("Y0"));
}

// ---------------------------------------------------------------- CYLINDER

fn cyl_rig(params: &str) -> Rig {
    idle(Rig::new(&prog(
        "c : CYLINDER; eo AT Y0 : BOOL; ro AT Y1 : BOOL; ie AT Y2 : BOOL; ir AT Y3 : BOOL; mv AT Y4 : BOOL; f AT Y5 : BOOL; code AT D20 : INT;",
        &format!(
            "c(EXTEND := X0, RETRACT := X1, MANUAL := X2, JOG_EXT := X3, JOG_RET := X4, PERMIT := NOT X5, LS_EXT := X6, LS_RET := X7, RST := X10{params});\n\
             eo := c.EXT_OUT; ro := c.RET_OUT; ie := c.IS_EXT; ir := c.IS_RET; mv := c.MOVING; f := c.FAULT; code := c.FAULT_CODE;"
        ),
    )))
}

#[test]
fn cylinder_extends_and_stops_at_the_end_in_the_same_scan() {
    let mut r = cyl_rig(", TRAVEL_MS := 1000");
    r.set("X7", true).scan(1);
    assert!(r.bit("Y3") && !r.bit("Y2") && !r.bit("Y4"));
    r.set("X0", true).scan(1);
    assert!(r.bit("Y0") && !r.bit("Y1") && r.bit("Y4"));
    r.set("X7", false).scan(3);
    assert!(r.bit("Y0") && r.bit("Y4") && !r.bit("Y3"));
    r.set("X6", true).scan(1);
    assert!(!r.bit("Y0") && r.bit("Y2") && !r.bit("Y4") && !r.bit("Y5"));
    r.scan(30);
    assert!(
        !r.bit("Y0") && !r.bit("Y5"),
        "resting at the end is no fault"
    );
}

#[test]
fn cylinder_command_for_a_reached_end_gives_no_output() {
    let mut r = cyl_rig("");
    r.set("X6", true).scan(1).set("X0", true);
    for _ in 0..5 {
        r.scan(1);
        assert!(!r.bit("Y0") && !r.bit("Y4"));
    }
}

#[test]
fn cylinder_both_commands_and_break_before_make() {
    let mut r = cyl_rig("");
    r.set("X0", true).set("X1", true).scan(3);
    assert!(!r.bit("Y0") && !r.bit("Y1"), "both commands: no output");
    r.set("X1", false).scan(1);
    assert!(r.bit("Y0"));
    r.set("X0", false).set("X1", true).scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y1"), "one scan with both off");
    r.scan(1);
    assert!(r.bit("Y1") && !r.bit("Y0"));
}

#[test]
fn cylinder_manual_jog_and_mode_separation() {
    let mut r = cyl_rig("");
    r.set("X2", true).scan(2);
    r.set("X3", true).scan(1);
    assert!(r.bit("Y0"));
    r.scan(3);
    assert!(r.bit("Y0"), "moves while the jog is held");
    r.set("X3", false).scan(1);
    assert!(!r.bit("Y0"), "stops when released");
    r.set("X0", true).scan(3);
    assert!(!r.bit("Y0"), "EXTEND is ignored in manual");
    r.set("X0", false).set("X2", false).scan(2);
    r.set("X4", true).scan(3);
    assert!(!r.bit("Y1"), "jog is ignored in auto");
}

#[test]
fn cylinder_travel_timeout_and_re_arm() {
    let mut r = cyl_rig(", TRAVEL_MS := 1000");
    r.set("X0", true);
    let mut n = 0;
    loop {
        r.scan(1);
        n += 1;
        if r.bit("Y5") {
            break;
        }
        assert!(r.bit("Y0") && n < 20);
    }
    assert!((10..=12).contains(&n), "timeout after {n} scans");
    assert!(!r.bit("Y0") && !r.bit("Y1") && !r.bit("Y4") && r.word("D20") == 1);
    r.scan(10);
    assert!(!r.bit("Y0"), "a held EXTEND does not restart");
    r.set("X10", true).scan(1).set("X10", false).scan(3);
    assert!(
        !r.bit("Y5") && r.word("D20") == 0 && !r.bit("Y0"),
        "RST with EXTEND held: no motion"
    );
    r.set("X0", false).scan(1).set("X0", true).scan(1);
    assert!(r.bit("Y0"));
}

#[test]
fn cylinder_retract_timeout_code_2() {
    let mut r = cyl_rig(", TRAVEL_MS := 500");
    r.set("X1", true).scan(8);
    assert!(r.bit("Y5") && r.word("D20") == 2 && !r.bit("Y1"));
}

#[test]
fn cylinder_both_end_sensors() {
    let mut r = cyl_rig(", TRAVEL_MS := 1000");
    r.set("X0", true).scan(2);
    assert!(r.bit("Y0"));
    r.set("X6", true).set("X7", true).scan(1);
    assert!(r.bit("Y5") && r.word("D20") == 3 && !r.bit("Y0") && !r.bit("Y1"));
    assert!(!r.bit("Y2") && !r.bit("Y3"));
    r.set("X10", true).scan(1);
    assert!(
        r.bit("Y5") && r.word("D20") == 3,
        "RST with the cause present re-latches"
    );
    r.set("X10", false).set("X7", false).scan(1);
    assert!(r.bit("Y5") && r.word("D20") == 3, "latched");
    // TRAVEL_MS 0: no code 3 and no timeout.
    let mut r = cyl_rig(", TRAVEL_MS := 0");
    r.set("X6", true).set("X7", true).scan(5);
    assert!(!r.bit("Y5"));
    r.set("X6", false)
        .set("X7", false)
        .set("X0", true)
        .scan(500);
    assert!(r.bit("Y0") && !r.bit("Y5"), "TRAVEL_MS 0 never times out");
}

#[test]
fn cylinder_fault_in_manual_allows_limited_jogs() {
    let mut r = cyl_rig(", TRAVEL_MS := 500");
    r.set("X0", true).scan(10);
    assert!(r.bit("Y5") && r.word("D20") == 1);
    r.set("X0", false).set("X2", true).scan(2);
    r.set("X3", true).scan(1);
    assert!(r.bit("Y0"), "jog moves despite FAULT");
    for _ in 0..5 {
        r.scan(1);
        assert!(r.bit("Y0"));
    }
    r.scan(1);
    assert!(
        !r.bit("Y0") && r.bit("Y5") && r.word("D20") == 1,
        "jog cut after TRAVEL_MS, first cause kept"
    );
    r.scan(3);
    assert!(!r.bit("Y0"), "a held jog does not restart");
    r.set("X3", false).scan(1).set("X3", true).scan(1);
    assert!(r.bit("Y0"), "released and pressed again: moves again");
    // Back to auto with the fault still latched: no output until RST.
    r.set("X3", false)
        .set("X2", false)
        .scan(2)
        .set("X0", true)
        .scan(2);
    assert!(!r.bit("Y0"));
}

#[test]
fn cylinder_permit_loss_needs_a_new_command() {
    let mut r = cyl_rig("");
    r.set("X0", true).scan(2);
    assert!(r.bit("Y0"));
    r.set("X5", true).scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y5"));
    r.set("X5", false).scan(3);
    assert!(!r.bit("Y0"), "PERMIT back with EXTEND held: no motion");
    r.set("X0", false).scan(1).set("X0", true).scan(1);
    assert!(r.bit("Y0"));
}

#[test]
fn cylinder_mode_change_with_a_jog_held_does_not_move() {
    let mut r = cyl_rig("");
    r.set("X3", true).scan(2);
    assert!(!r.bit("Y0"));
    r.set("X2", true).scan(3);
    assert!(!r.bit("Y0"), "auto -> manual with the jog held");
    r.set("X3", false).scan(1).set("X3", true).scan(1);
    assert!(r.bit("Y0"));
}

#[test]
fn cylinder_coupled_pair_needs_both_sensors() {
    let mut r = cyl_rig(", LS_EXT2 := X11");
    r.set("X0", true).scan(1);
    assert!(r.bit("Y0"));
    r.set("X6", true).scan(2);
    assert!(
        r.bit("Y0") && r.bit("Y4") && !r.bit("Y2"),
        "one sensor of the pair"
    );
    r.set("X11", true).scan(1);
    assert!(!r.bit("Y0") && r.bit("Y2") && !r.bit("Y4"));
}

#[test]
fn cylinder_hold_end_clamps_at_the_end() {
    let mut r = cyl_rig(", HOLD_END := X12, TRAVEL_MS := 500");
    r.set("X12", true).set("X0", true).scan(2);
    r.set("X6", true).scan(1);
    assert!(r.bit("Y0") && !r.bit("Y4") && r.bit("Y2"));
    r.scan(10);
    assert!(r.bit("Y0") && !r.bit("Y5"), "no timeout at the end");
    r.set("X0", false).scan(1);
    assert!(!r.bit("Y0"));
}

// ---------------------------------------------------------------- AXIS_2SPEED

fn axis_rig(params: &str) -> Rig {
    idle(Rig::new(&prog(
        "a : AXIS_2SPEED; fo AT Y0 : BOOL; ro AT Y1 : BOOL; so AT Y2 : BOOL; mv AT Y3 : BOOL; f AT Y4 : BOOL; code AT D20 : INT;",
        &format!(
            "a(FWD := X0, REV := X1, MANUAL := X2, JOG_FWD := X3, JOG_REV := X4, PERMIT := NOT X5, STOP_FWD := X6, STOP_REV := X7, LS_FWD_OK := NOT X10, LS_REV_OK := NOT X11, SLOW_FWD := X12, SLOW_REV := X13, SLOW_REQ := X14, RST := X15{params});\n\
             fo := a.FWD_OUT; ro := a.REV_OUT; so := a.SLOW_OUT; mv := a.MOVING; f := a.FAULT; code := a.FAULT_CODE;"
        ),
    )))
}

#[test]
fn axis_slows_down_and_stops_at_the_target() {
    let mut r = axis_rig("");
    r.set("X0", true).scan(1);
    assert!(r.bit("Y0") && !r.bit("Y2") && r.bit("Y3"), "fast");
    r.set("X12", true).scan(1);
    assert!(r.bit("Y0") && r.bit("Y2"), "slow-down sensor");
    r.set("X12", false).scan(2);
    assert!(
        r.bit("Y0") && r.bit("Y2"),
        "latched after the sensor is passed"
    );
    r.set("X6", true).scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y2") && !r.bit("Y3") && !r.bit("Y4"));
    r.set("X0", false).set("X1", true).scan(1);
    assert!(r.bit("Y1") && !r.bit("Y2"), "next start is fast");
    // The forward slow sensor has no effect on reverse travel.
    r.set("X12", true).scan(2);
    assert!(r.bit("Y1") && !r.bit("Y2"));
}

#[test]
fn axis_slow_sensor_of_the_other_direction_has_no_effect() {
    let mut r = axis_rig("");
    r.set("X0", true).set("X13", true).scan(3);
    assert!(r.bit("Y0") && !r.bit("Y2"));
}

#[test]
fn axis_start_inside_the_slow_zone_is_slow_at_once() {
    let mut r = axis_rig("");
    r.set("X12", true).scan(1).set("X0", true).scan(1);
    assert!(
        r.bit("Y0") && r.bit("Y2"),
        "slow from the first output scan"
    );
}

#[test]
fn axis_slow_request_only_while_moving() {
    let mut r = axis_rig("");
    r.set("X14", true).scan(2);
    assert!(!r.bit("Y2"), "SLOW_OUT never without a direction");
    r.set("X0", true).scan(1);
    assert!(r.bit("Y0") && r.bit("Y2"));
    r.set("X14", false).scan(1);
    assert!(r.bit("Y0") && !r.bit("Y2"));
    r.set("X14", true).set("X0", false).scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y2"));
}

#[test]
fn axis_overtravel_fault_reset_and_moving_away() {
    let mut r = axis_rig("");
    r.set("X0", true).scan(2);
    r.set("X10", true).scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y1") && r.bit("Y4") && r.word("D20") == 2);
    r.set("X15", true).scan(1).set("X15", false).scan(2);
    assert!(
        !r.bit("Y4") && r.word("D20") == 0 && !r.bit("Y0"),
        "RST with FWD held: no motion"
    );
    r.set("X0", false).scan(1).set("X1", true).scan(1);
    assert!(
        r.bit("Y1"),
        "moves away while the forward limit is still open"
    );
    r.set("X1", false).scan(1).set("X0", true).scan(3);
    assert!(
        !r.bit("Y0") && !r.bit("Y4"),
        "into the open limit: no output, no new fault"
    );
    // Reverse overtravel: code 3.
    let mut r = axis_rig("");
    r.set("X1", true).scan(2).set("X11", true).scan(1);
    assert!(!r.bit("Y1") && r.bit("Y4") && r.word("D20") == 3);
}

#[test]
fn axis_stop_sensor_before_the_limit_is_no_fault() {
    let mut r = axis_rig("");
    r.set("X0", true).scan(2);
    r.set("X6", true).scan(1);
    assert!(!r.bit("Y0"));
    r.set("X10", true).scan(3);
    assert!(!r.bit("Y4"));
    // Stop sensor and limit in the same scan: no fault either.
    let mut r = axis_rig("");
    r.set("X0", true).scan(2);
    r.set("X6", true).set("X10", true).scan(2);
    assert!(!r.bit("Y0") && !r.bit("Y4"));
}

#[test]
fn axis_travel_timeout() {
    let mut r = axis_rig(", TRAVEL_MS := 1000");
    r.set("X0", true);
    let mut n = 0;
    while !r.bit("Y4") {
        r.scan(1);
        n += 1;
        assert!(n < 20);
    }
    assert!((10..=12).contains(&n), "timeout after {n} scans");
    assert!(!r.bit("Y0") && !r.bit("Y2") && r.word("D20") == 1);
    // A direction change restarts the travel time.
    let mut r = axis_rig(", TRAVEL_MS := 1000");
    r.set("X0", true)
        .scan(9)
        .set("X0", false)
        .set("X1", true)
        .scan(10);
    assert!(r.bit("Y1") && !r.bit("Y4"));
    // TRAVEL_MS 0: no monitoring.
    let mut r = axis_rig(", TRAVEL_MS := 0");
    r.set("X0", true).scan(500);
    assert!(r.bit("Y0") && !r.bit("Y4"));
}

#[test]
fn axis_both_commands_and_reversal_gap() {
    let mut r = axis_rig("");
    r.set("X0", true).set("X1", true).scan(3);
    assert!(!r.bit("Y0") && !r.bit("Y1"));
    r.set("X1", false).scan(1);
    assert!(r.bit("Y0"));
    r.set("X0", false).set("X1", true).scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y1"), "one scan gap");
    r.scan(1);
    assert!(r.bit("Y1"));
}

#[test]
fn axis_permit_loss_clears_everything() {
    let mut r = axis_rig("");
    r.set("X0", true)
        .set("X12", true)
        .scan(1)
        .set("X12", false)
        .scan(1);
    assert!(r.bit("Y0") && r.bit("Y2"));
    r.set("X5", true).scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y2") && !r.bit("Y3") && !r.bit("Y4"));
    r.set("X5", false).scan(3);
    assert!(!r.bit("Y0"), "re-arm needed");
    r.set("X0", false).scan(1).set("X0", true).scan(1);
    assert!(r.bit("Y0") && !r.bit("Y2"), "slow-down latch cleared");
}

#[test]
fn axis_manual_jog_and_fault_recovery() {
    let mut r = axis_rig("");
    r.set("X2", true).scan(2).set("X3", true).scan(1);
    assert!(r.bit("Y0"));
    r.set("X3", false).scan(1);
    assert!(!r.bit("Y0"));
    // Mode change with a jog held does not move.
    r.set("X2", false).scan(2).set("X3", true).scan(2);
    assert!(!r.bit("Y0"));
    r.set("X2", true).scan(3);
    assert!(!r.bit("Y0"));
    r.set("X3", false).scan(1).set("X3", true).scan(1);
    assert!(r.bit("Y0"));
    // Overtravel while jogging: FAULT; jogging away works, into the limit does not.
    r.set("X10", true).scan(1);
    assert!(!r.bit("Y0") && r.bit("Y4") && r.word("D20") == 2);
    r.set("X3", false).scan(1).set("X4", true).scan(1);
    assert!(
        r.bit("Y1") && r.bit("Y4"),
        "jog away during FAULT in MANUAL"
    );
    r.set("X4", false).scan(1).set("X3", true).scan(2);
    assert!(!r.bit("Y0"), "no jog into the open limit");
}

// ---------------------------------------------------------------- STACK_LIGHT

fn stack_rig(params: &str) -> Rig {
    Rig::new(&prog(
        "s : STACK_LIGHT; g AT Y0 : BOOL; y AT Y1 : BOOL; rd AT Y2 : BOOL; h AT Y3 : BOOL;",
        &format!(
            "s(GREEN_ON := X0, GREEN_FLASH := X1, YELLOW_ON := X2, YELLOW_FLASH := X3, RED_ON := X4, RED_FLASH := X5, HORN_ON := X6, HORN_FLASH := X7, SILENCE := X10, TEST := X11{params});\n\
             g := s.GREEN; y := s.YELLOW; rd := s.RED; h := s.HORN;"
        ),
    ))
}

#[test]
fn stack_light_steady_lamps() {
    let mut r = stack_rig("");
    r.set("X0", true).set("X4", true);
    for _ in 0..20 {
        r.scan(1);
        assert!(r.bit("Y0") && !r.bit("Y1") && r.bit("Y2") && !r.bit("Y3"));
    }
}

#[test]
fn stack_light_flashing_lamps_share_one_clock() {
    let mut r = stack_rig("");
    r.set("X1", true).set("X3", true);
    let mut trace = Vec::new();
    for _ in 0..40 {
        r.scan(1);
        assert_eq!(r.bit("Y0"), r.bit("Y1"), "same phase");
        trace.push(r.bit("Y0"));
    }
    // 500 ms on, 500 ms off, on phase first (also on the first scan).
    for (i, &on) in trace.iter().enumerate() {
        assert_eq!(on, (i / 5) % 2 == 0, "scan {}", i + 1);
    }
    // A colour that starts flashing later joins the running phase.
    r.set("X5", true);
    for _ in 0..12 {
        r.scan(1);
        assert_eq!(r.bit("Y2"), r.bit("Y0"));
    }
}

#[test]
fn stack_light_lamp_test_excludes_the_horn() {
    let mut r = stack_rig("");
    r.set("X11", true).scan(3);
    assert!(r.bit("Y0") && r.bit("Y1") && r.bit("Y2") && !r.bit("Y3"));
}

#[test]
fn stack_light_horn_time_limit_and_re_arm() {
    let mut r = stack_rig(", HORN_MS := 1000");
    r.scan(3).set("X6", true);
    let mut on = 0;
    for _ in 0..30 {
        r.scan(1);
        on += r.bit("Y3") as usize;
    }
    assert_eq!(on, 10, "horn sounds for HORN_MS");
    assert!(!r.bit("Y3"), "stays off while the request is held");
    r.set("X6", false).scan(1).set("X6", true).scan(1);
    assert!(r.bit("Y3"), "a new request sounds again");
}

#[test]
fn stack_light_silence() {
    let mut r = stack_rig("");
    r.set("X6", true).scan(2);
    assert!(r.bit("Y3"));
    r.set("X10", true).scan(1).set("X10", false).scan(5);
    assert!(!r.bit("Y3"), "silenced while the request stays");
    r.set("X6", false).scan(1).set("X6", true).scan(1);
    assert!(r.bit("Y3"));
}

#[test]
fn stack_light_intermittent_horn_follows_the_clock_and_its_limit() {
    let mut r = stack_rig(", HORN_MS := 3000");
    r.set("X7", true).set("X3", true);
    let mut sounded = 0;
    for i in 0..60 {
        r.scan(1);
        if i < 30 {
            assert_eq!(r.bit("Y3"), r.bit("Y1"), "scan {}", i + 1);
        } else {
            assert!(!r.bit("Y3"), "limited by HORN_MS");
        }
        sounded += r.bit("Y3") as usize;
    }
    assert_eq!(sounded, 15);
}

#[test]
fn stack_light_horn_unlimited() {
    let mut r = stack_rig("");
    r.set("X6", true).scan(500);
    assert!(r.bit("Y3"));
}

// ---------------------------------------------------------------- SOAK_TIMER

fn soak_rig(params: &str) -> Rig {
    Rig::new(&prog(
        "k : SOAK_TIMER; t AT D0 : INT; sm AT D2 : INT; soak AT Y0 : BOOL; done AT Y1 : BOOL; f AT Y2 : BOOL; em AT D20 : INT; es AT D21 : INT; rem AT D22 : INT; tm AT D23 : INT; ts AT D24 : INT; st AT D25 : INT;",
        &format!(
            "k(RUN := X0, PV := INT_TO_REAL(t), SP := 100.0, SOAK_MIN := sm, RST := X1{params});\n\
             soak := k.SOAKING; done := k.DONE; f := k.FAULT; em := k.ET_MIN; es := k.ET_S; rem := k.REM_MIN; tm := k.TOT_MIN; ts := k.TOT_S; st := k.STATE;"
        ),
    ))
}

fn soak_s(r: &Rig) -> i32 {
    r.word("D20") as i32 * 60 + r.word("D21") as i32
}

fn total_s(r: &Rig) -> i32 {
    r.word("D23") as i32 * 60 + r.word("D24") as i32
}

#[test]
fn soak_timer_heats_then_soaks() {
    let mut r = soak_rig("");
    r.set_word("D2", 5).scan(3);
    assert!(
        r.word("D25") == 0 && !r.bit("Y0") && total_s(&r) == 0,
        "RUN FALSE"
    );
    r.set_word("D0", 50).set("X0", true).scan(100);
    assert!(r.word("D25") == 1 && !r.bit("Y0") && soak_s(&r) == 0);
    assert!((9..=10).contains(&total_s(&r)), "total {}", total_s(&r));
    r.set_word("D0", 100).scan(1);
    assert!(r.word("D25") == 2 && r.bit("Y0"));
    r.scan(100);
    assert!((9..=10).contains(&soak_s(&r)), "soak {}", soak_s(&r));
    assert_eq!(r.word("D22"), 5, "remaining minutes rounded up");
}

#[test]
fn soak_timer_band() {
    // BAND 0: a drop after arrival keeps counting.
    let mut r = soak_rig("");
    r.set_word("D2", 5)
        .set_word("D0", 100)
        .set("X0", true)
        .scan(50);
    r.set_word("D0", 50).scan(100);
    assert!(
        r.bit("Y0") && (14..=15).contains(&soak_s(&r)),
        "soak {}",
        soak_s(&r)
    );
    // BAND 5: below SP - 5 the count pauses, SOAKING stays TRUE.
    let mut r = soak_rig(", BAND := 5.0");
    r.set_word("D2", 5)
        .set_word("D0", 100)
        .set("X0", true)
        .scan(50);
    let before = soak_s(&r);
    r.set_word("D0", 96).scan(30);
    assert!(soak_s(&r) > before, "inside the band");
    let inside = soak_s(&r);
    r.set_word("D0", 94).scan(100);
    assert!(
        r.bit("Y0") && soak_s(&r) <= inside + 1,
        "paused below SP - BAND"
    );
    let paused = soak_s(&r);
    r.set_word("D0", 95).scan(100);
    assert!(soak_s(&r) >= paused + 9, "resumes at SP - BAND");
}

#[test]
fn soak_timer_done_after_soak_min_and_latched() {
    let mut r = soak_rig("");
    r.set_word("D2", 1).set_word("D0", 100).set("X0", true);
    let mut n = 0;
    while !r.bit("Y1") {
        r.scan(1);
        n += 1;
        assert!(n < 700);
    }
    assert!((600..=602).contains(&n), "DONE after {n} scans");
    assert!(r.word("D25") == 3 && r.word("D22") == 0);
    r.set("X0", false).scan(20);
    assert!(
        r.bit("Y1") && r.word("D25") == 3,
        "DONE stays after RUN drops"
    );
    r.set("X1", true).scan(1);
    assert!(!r.bit("Y1") && r.word("D25") == 0);
}

#[test]
fn soak_timer_lowering_soak_min_finishes_at_once() {
    let mut r = soak_rig("");
    r.set_word("D2", 10)
        .set_word("D0", 100)
        .set("X0", true)
        .scan(1300);
    assert!(!r.bit("Y1") && r.word("D20") == 2);
    r.set_word("D2", 1).scan(1);
    assert!(r.bit("Y1") && r.word("D22") == 0);
}

#[test]
fn soak_timer_run_pause_keeps_times_and_needs_a_new_arrival() {
    let mut r = soak_rig("");
    r.set_word("D2", 5)
        .set_word("D0", 100)
        .set("X0", true)
        .scan(200);
    let soaked = soak_s(&r);
    let total = total_s(&r);
    r.set("X0", false).scan(100);
    assert!(!r.bit("Y0") && r.word("D25") == 0);
    assert!(
        soak_s(&r) == soaked && total_s(&r) == total,
        "times kept, not counting"
    );
    r.set_word("D0", 50).set("X0", true).scan(100);
    assert!(
        r.word("D25") == 1 && !r.bit("Y0") && soak_s(&r) == soaked,
        "heating again"
    );
    assert!(total_s(&r) >= total + 9);
    r.set_word("D0", 100).scan(100);
    assert!(r.word("D25") == 2 && soak_s(&r) >= soaked + 9);
}

#[test]
fn soak_timer_reset_clears_everything() {
    let mut r = soak_rig(", HEAT_MAX_MIN := 1");
    r.set_word("D2", 1)
        .set_word("D0", 50)
        .set("X0", true)
        .scan(700);
    assert!(r.bit("Y2"));
    r.set_word("D0", 100).scan(700);
    assert!(r.bit("Y1"));
    r.set("X1", true).scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y1") && !r.bit("Y2"));
    assert!(soak_s(&r) == 0 && total_s(&r) == 0 && r.word("D22") == 1);
}

#[test]
fn soak_timer_heat_up_limit() {
    let mut r = soak_rig(", HEAT_MAX_MIN := 1");
    r.set_word("D2", 5).set_word("D0", 50).set("X0", true);
    let mut n = 0;
    while !r.bit("Y2") {
        r.scan(1);
        n += 1;
        assert!(n < 700);
    }
    assert!((600..=602).contains(&n), "FAULT after {n} scans");
    r.scan(100);
    assert!(r.bit("Y2") && total_s(&r) >= 69, "counting goes on");
    // No heat-up fault once SP has been reached.
    let mut r = soak_rig(", HEAT_MAX_MIN := 1");
    r.set_word("D2", 50)
        .set_word("D0", 100)
        .set("X0", true)
        .scan(10);
    r.set_word("D0", 50).scan(1000);
    assert!(!r.bit("Y2"));
}

#[test]
fn soak_timer_long_run_accuracy_and_rounding() {
    let mut r = soak_rig("");
    r.set_word("D2", 20).set_word("D0", 100).set("X0", true);
    let mut prev = (0, 0);
    let mut wraps = 0;
    for _ in 0..6000 {
        r.scan(1);
        let now = (r.word("D20"), r.word("D21"));
        assert!((0..60).contains(&now.1));
        if now.1 < prev.1 {
            assert!(
                prev.1 == 59 && now.1 == 0 && now.0 == prev.0 + 1,
                "{prev:?} -> {now:?}"
            );
            wraps += 1;
        }
        prev = now;
    }
    let s = soak_s(&r);
    assert!((598..=600).contains(&s), "600 s measured as {s} s");
    assert_eq!(wraps, 9);
    // 9:59 elapsed of 20 min: 10:01 left, shown rounded up as 11.
    assert_eq!(r.word("D22"), 20 - r.word("D20"));
    assert_eq!(r.word("D22"), 11);
}

#[test]
fn soak_timer_zero_soak_min_is_done_at_arrival() {
    let mut r = soak_rig("");
    r.set_word("D2", 0)
        .set_word("D0", 50)
        .set("X0", true)
        .scan(20);
    assert!(!r.bit("Y1") && r.word("D25") == 1);
    r.set_word("D0", 100).scan(1);
    assert!(r.bit("Y1") && r.word("D25") == 3);
}

// ---------------------------------------------------------------- size (words per call, labels)

/// The `Example:` call of `name` (its first statement), with the instance renamed to `x`.
fn example_call(name: &str) -> String {
    let src = include_str!("st/stdlib/process.st");
    let at = src
        .find(&format!("FUNCTION_BLOCK {name}\n"))
        .unwrap_or_else(|| panic!("{name} not found"));
    let line = src[..at]
        .lines()
        .rev()
        .find_map(|l| l.trim().strip_prefix("Example:"))
        .unwrap()
        .trim();
    let call = &line[..line.find(");").unwrap() + 2];
    format!("x{}", &call[call.find('(').unwrap()..])
}

/// Words one more call adds (two calls minus one call, encoded) and the jump labels `P<n>` it
/// adds, for the documented example call.
fn example_size(name: &str) -> (usize, usize) {
    let call = example_call(name);
    let src = |n: usize| {
        let decl: String = (0..n).map(|i| format!("x{i} : {name}; ")).collect();
        let body: String = (0..n)
            .map(|i| call.replacen("x(", &format!("x{i}("), 1))
            .collect();
        prog(
            &format!("{decl} lvl : REAL; temp : REAL; hz : REAL; heat_step : BOOL;"),
            &body,
        )
    };
    let words = |n: usize| {
        let s = src(n);
        // Measured with every call expanded in place, as the Size: lines describe it.
        let program = plc::compile_st_with(&s, &crate::st::LowerOptions { share: crate::st::Share::Never, ..Default::default() })
            .unwrap_or_else(|e| panic!("{e:?}\n{s}"));
        crate::es2_encoder::encode(&program)
            .unwrap_or_else(|e| panic!("{e}\n{s}"))
            .words
            .len()
    };
    let labels = |n: usize| {
        let il = crate::st::lower_with(&src(n), &crate::st::LowerOptions { share: crate::st::Share::Never, ..Default::default() }).unwrap().il;
        il.split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|w| {
                w.len() > 1 && w.starts_with('P') && w[1..].bytes().all(|b| b.is_ascii_digit())
            })
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    };
    (words(2) - words(1), labels(2) - labels(1))
}

/// Every "- Size: about N words per call, M jump labels" line matches a measurement of the
/// example call: the labels exactly, the words within 10 %.
#[test]
fn process_size_lines_match_the_measurement() {
    let src = include_str!("st/stdlib/process.st");
    let mut checked = 0;
    let mut doc = String::new();
    let mut report = Vec::new();
    for line in src.lines() {
        if line.starts_with("(*!") && !line.starts_with("(*!tr") {
            doc.clear();
        }
        doc.push_str(line);
        doc.push('\n');
        let Some(name) = line.strip_prefix("FUNCTION_BLOCK ") else {
            continue;
        };
        let Some(at) = doc.find("- Size: about ") else {
            continue;
        };
        let nums: Vec<usize> = doc[at + 14..]
            .split(|c: char| !c.is_ascii_digit())
            .filter(|w| !w.is_empty())
            .take(2)
            .map(|w| w.parse().unwrap())
            .collect();
        let (words, labels) = example_size(name.trim());
        report.push(format!("{name}: {words} words, {labels} labels"));
        assert_eq!(labels, nums[1], "{name}: labels");
        assert!(
            words.abs_diff(nums[0]) * 10 <= nums[0],
            "{name}: documented {} words, measured {words}",
            nums[0]
        );
        checked += 1;
    }
    assert!(checked >= 7, "{checked} Size lines\n{}", report.join("\n"));
}

/// The multi-instance blocks have no jump labels at all and stay within their word budget.
#[test]
fn cylinder_and_axis_use_no_labels_and_fit_their_budget() {
    for (name, budget) in [("CYLINDER", 310), ("AXIS_2SPEED", 320)] {
        let (words, labels) = example_size(name);
        assert_eq!(labels, 0, "{name} uses jump labels");
        assert!(words <= budget, "{name}: {words} words > {budget}");
    }
}

// ---------------------------------------------------------------- review findings (new blocks)

impl Rig {
    /// `n` scans of `ms` milliseconds each (real PLC scans are usually far below 100 ms).
    fn scan_ms(&mut self, n: usize, ms: u64) -> &mut Self {
        for _ in 0..n {
            self.sim.scan(&self.program, ms);
        }
        self
    }
}

/// With scans faster than the 100 ms timer tick the horn still sounds HORN_MS and the lamps
/// still flash FLASH_MS on / FLASH_MS off: restarting the common time base must not count as an
/// extra 100 ms horn step.
#[test]
fn stack_light_horn_time_and_flash_with_fast_scans() {
    for flash in [100, 500] {
        let mut r = stack_rig(&format!(", HORN_MS := 1000, FLASH_MS := {flash}"));
        r.scan_ms(7, 20).set("X6", true);
        let mut on = 0;
        for _ in 0..150 {
            r.scan_ms(1, 20);
            on += r.bit("Y3") as usize;
        }
        assert!(
            (45..=55).contains(&on),
            "FLASH_MS {flash}: horn sounded {on} scans of 20 ms instead of about 50"
        );
        // Flash half period with 20 ms scans: FLASH_MS / 20 scans (one tick of tolerance).
        r.set("X1", true);
        let mut runs = Vec::new();
        let mut last = r.bit("Y0");
        let mut len = 0;
        for _ in 0..400 {
            r.scan_ms(1, 20);
            let now = r.bit("Y0");
            if now == last {
                len += 1;
            } else {
                runs.push(len);
                len = 1;
                last = now;
            }
        }
        let half = flash as usize / 20;
        // runs[0] and runs[1] end in the phase that was running when GREEN_FLASH rose.
        assert!(runs.len() > 4);
        for &n in &runs[2..] {
            assert!(
                n >= half && n <= half + 5,
                "FLASH_MS {flash}: half period {n} scans {runs:?}"
            );
        }
    }
}

/// A command held at the target: the stop sensor stops the axis; if the sensor turns FALSE
/// again (run-out past a short cam) the held command moves on (documented).
#[test]
fn axis_held_command_resumes_when_the_stop_sensor_clears() {
    let mut r = axis_rig("");
    r.set("X0", true).scan(2).set("X6", true).scan(2);
    assert!(!r.bit("Y0") && !r.bit("Y4"));
    r.set("X6", false).scan(1);
    assert!(r.bit("Y0"), "the held command resumes");
    r.set("X0", false).scan(1).set("X6", false).scan(1);
    assert!(!r.bit("Y0"));
}

/// Soak and total time are counted in whole-second ticks of a free-running time base: a soak
/// of 0.3 s counts 0 or 1 s, never more, whatever the phase.
#[test]
fn soak_timer_counts_whole_second_ticks() {
    for phase in 0..10 {
        let mut r = soak_rig("");
        r.set_word("D2", 5).set_word("D0", 100).scan(phase);
        r.set("X0", true).scan(3).set("X0", false).scan(20);
        assert!(
            (0..=1).contains(&soak_s(&r)) && (0..=1).contains(&total_s(&r)),
            "phase {phase}: soak {} s, total {} s",
            soak_s(&r),
            total_s(&r)
        );
    }
}

/// A negative BAND acts like 0 (classic): after arrival a PV drop keeps the soak time counting.
#[test]
fn soak_timer_negative_band_is_classic() {
    let mut r = soak_rig(", BAND := -5.0");
    r.set_word("D2", 5)
        .set_word("D0", 100)
        .set("X0", true)
        .scan(20);
    let before = soak_s(&r);
    r.set_word("D0", 50).scan(100);
    assert!(
        r.bit("Y0") && soak_s(&r) >= before + 9,
        "soak {}",
        soak_s(&r)
    );
}

// ---------------------------------------------------------------- first scan and power cycle

impl Rig {
    /// Power cycle: a fresh simulator that keeps only the retentive (latched) D and M ranges used
    /// by `VAR_RETAIN`; every other device, timer and input starts cleared.
    fn power_cycle(&mut self) -> &mut Self {
        let range = crate::alloc::Retain::default();
        let mut fresh = Simulator::default();
        for i in range.d.0..=range.d.1 {
            let d: plc::Device = format!("D{i}").parse().unwrap();
            fresh.memory.set_word(d, self.sim.memory.word(d));
        }
        for i in range.m.0..=range.m.1 {
            let d: plc::Device = format!("M{i}").parse().unwrap();
            fresh.memory.set_bit(d, self.sim.memory.bit(d));
        }
        self.sim = fresh;
        self
    }
}

/// First scan: LEAD 1 at the very first power-up; DEMAND is a level, so a DEMAND held at
/// power-up starts the lead pump in the first scan. LEAD survives a power cycle, and the power
/// loss itself does not alternate.
#[test]
fn pump_alt_first_scan_and_power_cycle_keep_the_lead() {
    let mut r = Rig::new(&prog(
        "p : PUMP_ALT; p1 AT Y0 : BOOL; p2 AT Y1 : BOOL; lead AT D20 : INT;",
        "p(DEMAND := X0); p1 := p.PUMP1; p2 := p.PUMP2; lead := p.LEAD;",
    ));
    r.set("X0", true).scan(1);
    assert!(
        r.bit("Y0") && !r.bit("Y1") && r.word("D20") == 1,
        "pump 1 in the first scan"
    );
    r.set("X0", false).scan(1);
    assert_eq!(r.word("D20"), 2, "alternated on stop");
    r.set("X0", true).scan(2);
    assert!(!r.bit("Y0") && r.bit("Y1"));
    // Power loss while pump 2 runs; DEMAND is TRUE again at power-up.
    r.power_cycle().set("X0", true).scan(1);
    assert!(
        !r.bit("Y0") && r.bit("Y1") && r.word("D20") == 2,
        "retained lead, no alternation by the power loss"
    );
    r.set("X0", false).scan(1);
    assert_eq!(r.word("D20"), 1);
}

/// First scan: LEAD 1, RUN_COUNT 0; with DEMAND > 0 the first pump starts in the first scan.
#[test]
fn lead_lag_first_scan_starts_the_first_pump() {
    let mut r = Rig::new(&prog(
        "l : LEAD_LAG; d AT D0 : INT; p1 AT Y0 : BOOL; p2 AT Y1 : BOOL; n AT D20 : INT; lead AT D21 : INT;",
        "l(DEMAND := d, STAGE_MS := 500); p1 := l.PUMP1; p2 := l.PUMP2; n := l.RUN_COUNT; lead := l.LEAD;",
    ));
    r.set_word("D0", 2).scan(1);
    assert!(r.bit("Y0") && !r.bit("Y1") && r.word("D20") == 1 && r.word("D21") == 1);
    r.scan(10);
    assert!(
        r.bit("Y1") && r.word("D20") == 2,
        "second pump after STAGE_MS"
    );
}

/// First scan: the outputs show the retained meter at once; counting resumes from there.
#[test]
fn runtime_first_scan_shows_the_retained_meter() {
    let mut r = Rig::new(&prog(
        "rt : RUNTIME; h AT D20 : DINT; m AT D22 : INT; s AT D23 : INT; left AT D24 : DINT;",
        "rt(RUN := X0, SERVICE_H := 1); h := rt.HOURS; m := rt.MINUTES; s := rt.SECONDS; left := rt.REM_H;",
    ));
    r.scan(1);
    assert!(r.word("D20") == 0 && r.word("D22") == 0 && r.word("D23") == 0);
    assert_eq!(r.word("D24"), 1);
    r.set("X0", true).scan(100);
    let s = r.word("D23");
    assert!((9..=10).contains(&s), "seconds {s}");
    r.power_cycle().scan(1);
    assert!(
        r.word("D23") == s && r.word("D22") == 0 && r.word("D24") == 1,
        "retained meter in the first scan"
    );
    r.set("X0", true).scan(51);
    let t = r.word("D23");
    assert!((s + 4..=s + 5).contains(&t), "resumed from {s}: {t}");
}

/// First scan: a CYCLE held at power-up is not counted; TOTAL and SINCE are retained.
#[test]
fn cycle_counter_power_cycle_keeps_totals_and_ignores_a_held_cycle() {
    let mut r = Rig::new(&prog(
        "c : CYCLE_COUNTER; tot AT D20 : DINT; since AT D22 : DINT; rem AT D24 : DINT;",
        "c(CYCLE := X0, SERVICE_N := 10); tot := c.TOTAL; since := c.SINCE; rem := c.REM_N;",
    ));
    r.set("X0", true).scan(1);
    assert!(
        r.word("D20") == 0 && r.word("D24") == 10,
        "held at power-up"
    );
    r.set("X0", false).scan(1);
    for _ in 0..3 {
        r.set("X0", true).scan(1).set("X0", false).scan(1);
    }
    r.set("X0", true).scan(1);
    assert_eq!(r.word("D20"), 4);
    r.power_cycle().set("X0", true).scan(1);
    assert!(
        r.word("D20") == 4 && r.word("D22") == 4 && r.word("D24") == 6,
        "retained, and the CYCLE held over the power cycle is not counted again"
    );
    r.set("X0", false).scan(1).set("X0", true).scan(1);
    assert_eq!(r.word("D20"), 5);
}

/// First scan: nothing latched; IN held at power-up raises a new alarm (at once with
/// DELAY_MS = 0, else after DELAY_MS from the first scan); ACK held at power-up is an edge.
#[test]
fn alarm_first_scan() {
    let src = |delay: i32| {
        prog(
            "a : ALARM; act AT Y0 : BOOL; un AT Y1 : BOOL; lamp AT Y2 : BOOL; horn AT Y3 : BOOL;",
            &format!(
                "a(IN := X0, DELAY_MS := {delay}, ACK := X1);\n\
                 act := a.LATCHED; un := a.UNACK; lamp := a.LAMP; horn := a.HORN;"
            ),
        )
    };
    let mut r = Rig::new(&src(0));
    r.scan(1);
    assert!(!r.bit("Y0") && !r.bit("Y1") && !r.bit("Y2") && !r.bit("Y3"));
    let mut r = Rig::new(&src(0));
    r.set("X0", true).scan(1);
    assert!(
        r.bit("Y0") && r.bit("Y1") && r.bit("Y2") && r.bit("Y3"),
        "new alarm in the first scan"
    );
    let mut r = Rig::new(&src(300));
    r.set("X0", true).scan(2);
    assert!(!r.bit("Y0"), "delay running");
    r.scan(3);
    assert!(
        r.bit("Y0") && r.bit("Y3"),
        "DELAY_MS counted from the first scan"
    );
    let mut r = Rig::new(&src(0));
    r.set("X0", true).set("X1", true).scan(1);
    assert!(
        r.bit("Y0") && !r.bit("Y1") && !r.bit("Y3") && r.bit("Y2"),
        "an ACK held at power-up acts as a rising edge"
    );
}

/// First scan: a horn request held at power-up is a new request; HORN_MS counts from then.
#[test]
fn stack_light_horn_request_at_power_up_sounds() {
    let mut r = stack_rig(", HORN_MS := 500");
    r.set("X6", true).scan(1);
    assert!(r.bit("Y3"), "horn in the first scan");
    r.scan(10);
    assert!(!r.bit("Y3"), "HORN_MS counted from the first scan");
}

/// First scan: Q is on only if PV is already at the switching level of the mode.
#[test]
fn tank_first_scan() {
    for (drain, level, on) in [
        (false, 50, false),
        (false, 20, true),
        (true, 50, false),
        (true, 80, true),
    ] {
        let mut r = Rig::new(&prog(
            "t : TANK; lv AT D0 : INT; out AT Y0 : BOOL;",
            "t(PV := INT_TO_REAL(lv), DRAIN := X0); out := t.Q;",
        ));
        r.set("X0", drain).set_word("D0", level).scan(1);
        assert_eq!(r.bit("Y0"), on, "DRAIN {drain}, PV {level}");
    }
}

/// First scan: PV >= LIM_HH latches FAULT at once and keeps Q off.
#[test]
fn heater_first_scan_over_temperature() {
    let mut r = Rig::new(&prog(
        "h : HEATER; t AT D0 : INT; out AT Y0 : BOOL; f AT Y1 : BOOL;",
        "h(PV := INT_TO_REAL(t), SP := 50.0, LIM_HH := 90.0); out := h.Q; f := h.FAULT;",
    ));
    r.set_word("D0", 95).scan(1);
    assert!(r.bit("Y1") && !r.bit("Y0"));
}

/// First scan: the valve stays closed with START held; a PULSE held at power-up counts once.
#[test]
fn dosing_first_scan() {
    let mut r = Rig::new(&prog(
        "d : DOSING; v AT Y0 : BOOL; n AT D20 : DINT;",
        "d(START := X0, PULSE := X2, SP := 5.0, NO_FLOW_MS := 0); v := d.VALVE_OUT; n := d.PULSES;",
    ));
    r.set("X0", true).set("X2", true).scan(3);
    assert!(!r.bit("Y0") && r.word("D20") == 1);
}
