//! Opt-in test against a separately owned installed SS2 simulator process.
use crate::es2_bridge::{DEFAULT_SS2_SIMULATOR, Session, Target};
use crate::plc::Device;
use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

#[test]
#[ignore = "requires the installed, digest-verified COMMGR 2.11 SS2 simulator"]
fn repeated_scans_observe_native_io_and_independent_ladder_state() {
    let mut session = Session::launch(
        Path::new(DEFAULT_SS2_SIMULATOR),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("launch isolated SS2 session");
    assert_eq!(session.target(), Target::Ss2);
    let x0: Device = "X0".parse().unwrap();
    let y0: Device = "Y0".parse().unwrap();
    let initial = session
        .upload("LD X0\nOUT Y0\nEND")
        .expect("upload and verify");
    assert_eq!(initial.scan, 0);
    let inputs = [false, true, true, false, true, false];
    for (index, on) in inputs.into_iter().enumerate() {
        let result = session.step(&[(x0, on)]).expect("native and local scan");
        assert_eq!(result.scan, (index + 1) as u64);
        assert_eq!(result.mismatches, 0);
        for device in [x0, y0] {
            let row = result
                .rows
                .iter()
                .find(|row| row.device == device && row.kind == "bit")
                .expect("native bit snapshot");
            assert_eq!(
                row.original,
                u32::from(on),
                "{device} native scan {}",
                index + 1
            );
            assert_eq!(
                row.local,
                u32::from(on),
                "{device} local scan {}",
                index + 1
            );
        }
        assert_eq!(result.local.power.first(), Some(&on));
        assert_eq!(
            result.local.action_power.first().and_then(|v| v.first()),
            Some(&on)
        );
    }
}

#[test]
#[ignore = "requires the installed, digest-verified COMMGR 2.11 SS2 simulator"]
fn worker_accepts_repeated_async_steps_and_returns_fresh_snapshots() {
    use std::{
        future::Future,
        task::{Context, Poll, Wake, Waker},
        thread,
    };
    struct ThreadWake(thread::Thread);
    impl Wake for ThreadWake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::from(Arc::new(ThreadWake(thread::current())));
        let mut context = Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(value) => return value,
                Poll::Pending => thread::park(),
            }
        }
    }
    block_on(async {
        let bridge = crate::es2_bridge::launch(DEFAULT_SS2_SIMULATOR.to_string())
            .await
            .expect("launch isolated SS2 worker");
        assert!(bridge.read_program_words().await.is_err());
        let x0: Device = "X0".parse().unwrap();
        let y0: Device = "Y0".parse().unwrap();
        let initial = bridge
            .upload("LD X0\nOUT Y0\nEND".into())
            .await
            .expect("upload");
        assert_eq!(initial.scan, 0);
        let words = bridge
            .read_program_words()
            .await
            .expect("serialized program readback");
        let expected =
            crate::es2_encoder::encode(&crate::plc::compile("LD X0\nOUT Y0\nEND").unwrap())
                .unwrap();
        assert_eq!(words, expected.words);
        let discovered = bridge
            .discover_program_words()
            .await
            .expect("boundary-based readback");
        assert_eq!(discovered, expected.words);
        for (i, on) in [true, false, true, false].into_iter().enumerate() {
            let snapshot = bridge.step(vec![(x0, on)]).await.expect("step");
            assert_eq!(snapshot.scan, (i + 1) as u64);
            let output = snapshot
                .rows
                .iter()
                .find(|r| r.device == y0 && r.kind == "bit")
                .expect("Y0 snapshot");
            assert_eq!(output.original, u32::from(on));
            assert_eq!(snapshot.mismatches, 0);
        }
        bridge.disconnect();
    });
}

#[test]
#[ignore = "requires the installed, digest-verified COMMGR 2.11 SS2 simulator"]
fn plc_monitor_reads_match_the_native_simulator_state() {
    use crate::plc_monitor::{Value, read_values};
    use crate::st::DataType;
    let mut session = Session::launch(
        Path::new(DEFAULT_SS2_SIMULATOR),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("launch isolated SS2 session");
    let x0: Device = "X0".parse().unwrap();
    session
        .upload(
            "LD X0\nMOV K-5 D10\nDMOV K100000 D20\nMOV H1234 D300\nSET M5\nSET M1600\nOUT Y2\nEND",
        )
        .expect("upload");
    let result = session.step(&[(x0, true)]).expect("scan");
    assert_eq!(result.mismatches, 0);
    // A second, independent connection: the monitor reads like a remote tool would.
    let mut link = crate::es2_protocol::Client::connect(session.port(), 1).expect("connect");
    let dev = |s: &str| s.parse::<Device>().unwrap();
    let wanted = [
        (dev("D10"), DataType::Int),
        (dev("D20"), DataType::Dint),
        (dev("D300"), DataType::Word),
        (dev("M5"), DataType::Bool),
        (dev("M1600"), DataType::Bool),
        (dev("Y2"), DataType::Bool),
        (dev("X0"), DataType::Bool),
        (dev("Y3"), DataType::Bool),
    ];
    let values = read_values(&mut link, &wanted).expect("monitor read");
    assert_eq!(
        values,
        [
            Value::Int(-5),
            Value::Dint(100_000),
            Value::Word(0x1234),
            Value::Bool(true),
            Value::Bool(true),
            Value::Bool(true),
            Value::Bool(true),
            Value::Bool(false),
        ]
    );
    // The simulator's own scan snapshot agrees with what the monitor read.
    for row in &result.rows {
        let Some(i) = wanted.iter().position(|(d, _)| *d == row.device) else {
            continue;
        };
        let monitored = match values[i] {
            Value::Bool(b) => u32::from(b),
            Value::Int(v) => v as u16 as u32,
            Value::Word(v) => u32::from(v),
            // The snapshot lists a 32-bit value as its low word (`word`) and as `dword`.
            Value::Dint(v) if row.kind == "dword" => v as u32,
            Value::Dint(v) => v as u32 & 0xFFFF,
            _ => continue,
        };
        assert_eq!(row.original, monitored, "{} {}", row.device, row.kind);
    }
}

/// The ST code generator's patterns run alike on the native SS2 simulator and the local model:
/// INT `+`/`-` through zero-extended DADD/DSUB (with overflow), BOOL networks of contacts,
/// comparison contacts and ANB/ORB blocks (including De Morgan negation), and TON_DSEC on one
/// native timer.
#[test]
#[ignore = "requires the installed, digest-verified COMMGR 2.11 SS2 simulator"]
fn st_code_generation_matches_the_native_simulator() {
    let source = "PROGRAM t
VAR
    in0 AT X0 : BOOL; in1 AT X1 : BOOL; in2 AT X2 : BOOL; in3 AT X3 : BOOL; in4 AT X4 : BOOL;
    in5 AT X5 : BOOL;
    out1 AT Y0 : BOOL; out2 AT Y1 : BOOL; out3 AT Y2 : BOOL; out4 AT Y3 : BOOL; out5 AT Y4 : BOOL;
    out6 AT Y5 : BOOL; out7 AT Y6 : BOOL;
    a AT D100 : INT; b AT D101 : INT; c AT D102 : INT; d AT D103 : INT; e AT D104 : INT;
    big AT D110 : DINT; r AT D120 : REAL; et AT D130 : INT; rem AT D131 : INT;
    td : TON_DSEC;
END_VAR
a := a + 12345;
b := b - 23456;
c := a + b;
d := 30000 - c;
e := -32768 - a + (b - 1);
big := big + 70000;
r := r + 0.5;
out1 := (in0 AND NOT in1) OR (in2 AND (in3 OR NOT in4));
out2 := NOT (in0 OR in1) XOR (a > b);
out3 := (a > 0 AND b <= 100) OR (big < 1000000 AND r >= 3.0) OR NOT (c = d);
out4 := in5 AND NOT (in0 AND (in1 OR (in2 AND (in3 OR (in4 AND (a <> 0 OR (b > 5 AND c < 7)))))));
out5 := out5 XOR in0;
IF NOT (in2 OR a < b) THEN
    out7 := NOT out7;
END_IF;
td(IN := in1 OR in3, PT := 5);
out6 := td.Q;
et := td.ET;
rem := td.REM;
END_PROGRAM
";
    let il = crate::st::lower(source).expect("lower").il;
    let mut session = Session::launch(
        Path::new(DEFAULT_SS2_SIMULATOR),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("launch isolated SS2 session");
    session.upload(&il).unwrap_or_else(|e| panic!("{e}\n{il}"));
    let inputs: Vec<Device> = (0..6).map(|i| format!("X{i}").parse().unwrap()).collect();
    // A fixed pseudo-random input sequence; long runs of X1/X3 let the timer finish.
    let mut seed = 0x2545_f491_u32;
    let mut held = 0u32;
    let mut seen_on = 0u32;
    for scan in 0..400 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        if scan % 40 == 0 {
            held = seed & 0b1010;
        }
        let bits = (seed & 0b11_0101) | held;
        let step: Vec<(Device, bool)> = inputs
            .iter()
            .enumerate()
            .map(|(i, d)| (*d, bits >> i & 1 == 1))
            .collect();
        let result = session.step(&step).expect("native and local scan");
        for device in ["Y0", "Y3", "Y5", "Y6", "D102", "D104", "D130"] {
            let device: Device = device.parse().unwrap();
            assert!(result.rows.iter().any(|r| r.device == device), "{device} is compared");
        }
        for row in &result.rows {
            if row.device.area == crate::plc::Area::Y && row.kind == "bit" && row.original == 1 {
                seen_on |= 1 << row.device.index;
            }
        }
        let differing: Vec<String> = result
            .rows
            .iter()
            .filter(|r| r.original != r.local)
            .map(|r| format!("{} {} native {} local {}", r.device, r.kind, r.original, r.local))
            .collect();
        assert_eq!(result.mismatches, 0, "scan {scan}: {differing:?}\n{il}");
    }
    // Every output Y0..Y6 was TRUE at some scan, so each network was exercised both ways.
    assert_eq!(seen_on & 0x7f, 0x7f, "outputs seen on: {seen_on:b}");
}

/// Shared block bodies run alike on the native SS2 simulator and the local model: CALL /
/// SRET subroutines after FEND, index-addressed (E6 / E7) words and bits, and each instance's
/// own TMR between the CALLs. Five block types, two instances each, every body shared.
#[test]
#[ignore = "requires the installed, digest-verified COMMGR 2.11 SS2 simulator"]
fn shared_block_bodies_match_the_native_simulator() {
    let source = "PROGRAM t
VAR
    i0 AT X0 : BOOL; i1 AT X1 : BOOL; i2 AT X2 : BOOL; i3 AT X3 : BOOL; i4 AT X4 : BOOL;
    i5 AT X5 : BOOL; i6 AT X6 : BOOL; i7 AT X7 : BOOL;
    c1 : CYLINDER; c2 : CYLINDER; f1 : TOF_SEC; f2 : TOF_SEC;
    r1 : RECIPE_STEP; r2 : RECIPE_STEP; s1 : STACK_LIGHT; s2 : STACK_LIGHT;
    a1 : ALARM; a2 : ALARM;
    cmd1 AT D200 : INT; cmd2 AT D201 : INT; st1 AT D202 : INT; et1 AT D203 : INT;
    fc1 AT D204 : INT; fc2 AT D205 : INT;
END_VAR
c1(EXTEND := i0, RETRACT := i1, LS_EXT := i2, LS_RET := i3, TRAVEL_MS := 1500, RST := i7);
c2(EXTEND := i1, RETRACT := i0, LS_EXT := i3, LS_RET := i2, TRAVEL_MS := 800, RST := i7);
Y0 := c1.EXT_OUT; Y1 := c1.RET_OUT; Y2 := c1.FAULT; fc1 := c1.FAULT_CODE;
Y3 := c2.EXT_OUT; Y4 := c2.RET_OUT; fc2 := c2.FAULT_CODE;
f1(IN := i4, PT := 1);
f2(IN := i5, PT := 2);
Y5 := f1.Q; Y6 := f2.Q; et1 := f1.ET;
r1(RUN := i6, TRIG := i4, WAIT_MODE := 1, WAIT_DS := 3, ACTION := 2, AT_TARGET := i2,
   TMO_MS := 2000, RST := i7);
r2(RUN := i6 AND i0, TRIG := i5, WAIT_MODE := 0, WAIT_DS := 0, ACTION := 1, AT_TARGET := i3,
   TMO_MS := 0, RST := i7);
cmd1 := r1.CMD; cmd2 := r2.CMD; st1 := r1.STATE; Y7 := r1.DONE; Y10 := r2.FAULT;
s1(GREEN_FLASH := i0, RED_ON := i1, HORN_ON := i2, HORN_MS := 1500, SILENCE := i7,
   FLASH_MS := 300);
s2(YELLOW_FLASH := i3, TEST := i5, FLASH_MS := 500);
Y11 := s1.GREEN; Y12 := s1.RED; Y13 := s1.HORN; Y14 := s2.YELLOW; Y15 := s2.GREEN;
a1(IN := i2 AND i3, DELAY_MS := 500, ACK := i7);
a2(IN := i4 AND NOT i5, DELAY_MS := 0, ACK := i6);
Y16 := a1.LAMP; Y17 := a2.HORN;
END_PROGRAM
";
    let options = crate::st::LowerOptions {
        share: crate::st::Share::Always,
        ..Default::default()
    };
    let lowered = crate::st::lower_with(source, &options).expect("lower");
    assert_eq!(
        lowered.shared_blocks,
        ["ALARM", "CYLINDER", "RECIPE_STEP", "STACK_LIGHT", "TOF_SEC"]
    );
    let il = lowered.il;
    let mut session = Session::launch(
        Path::new(DEFAULT_SS2_SIMULATOR),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("launch isolated SS2 session");
    session.upload(&il).unwrap_or_else(|e| panic!("{e}\n{il}"));
    let inputs: Vec<Device> = (0..8).map(|i| format!("X{i}").parse().unwrap()).collect();
    let mut seed = 0x1357_9bdf_u32;
    let mut bits = 0u32;
    let mut seen_on = 0u32;
    for scan in 0..500 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        // Inputs change now and then, so timers and sequences get to run.
        if seed.is_multiple_of(5) {
            bits ^= 1 << ((seed >> 8) % 7);
        }
        // Reset / acknowledge (X7) only as a short pulse now and then.
        bits = bits & 0x7f | u32::from(scan % 97 == 50) << 7;
        let step: Vec<(Device, bool)> = inputs
            .iter()
            .enumerate()
            .map(|(i, d)| (*d, bits >> i & 1 == 1))
            .collect();
        let result = session.step(&step).expect("native and local scan");
        for row in &result.rows {
            if row.device.area == crate::plc::Area::Y && row.kind == "bit" && row.original == 1 {
                seen_on |= 1 << row.device.index;
            }
        }
        let differing: Vec<String> = result
            .rows
            .iter()
            .filter(|r| r.original != r.local)
            .map(|r| format!("{} {} native {} local {}", r.device, r.kind, r.original, r.local))
            .collect();
        assert_eq!(result.mismatches, 0, "scan {scan}: {differing:?}\n{il}");
    }
    // Most outputs were TRUE at some scan, so the bodies were exercised.
    assert!(seen_on.count_ones() >= 10, "outputs seen on: {seen_on:o}");
}
