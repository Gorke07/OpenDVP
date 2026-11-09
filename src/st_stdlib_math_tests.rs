//! Behavioural tests of the ST standard library, category "math" (src/st/stdlib/math.st).
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

/// Evaluates an INT expression in one scan (result through `D20`).
fn int_expr(expr: &str) -> i16 {
    let src = format!("PROGRAM P\nVAR r AT D20 : INT; END_VAR\nr := {expr};\nEND_PROGRAM");
    word(&run(&src, 1, |_| {}), "D20")
}

/// Evaluates a DINT expression in one scan (result through `D20`/`D21`).
fn dint_expr(expr: &str) -> i32 {
    let src = format!("PROGRAM P\nVAR r AT D20 : DINT; END_VAR\nr := {expr};\nEND_PROGRAM");
    let sim = run(&src, 1, |_| {});
    (word(&sim, "D20") as u16 as i32) | ((word(&sim, "D21") as i32) << 16)
}

/// Evaluates a REAL expression in one scan (result through `D20`/`D21`).
fn real_expr(expr: &str) -> f32 {
    let src = format!("PROGRAM P\nVAR r AT D20 : REAL; END_VAR\nr := {expr};\nEND_PROGRAM");
    real(&run(&src, 1, |_| {}), "D20")
}

/// Evaluates a BOOL expression in one scan (result through `Y0`).
fn bool_expr(expr: &str) -> bool {
    let src = format!("PROGRAM P\nVAR q AT Y0 : BOOL; END_VAR\nq := {expr};\nEND_PROGRAM");
    bit(&run(&src, 1, |_| {}), "Y0")
}

/// Evaluates a WORD expression in one scan (result through `D20`, as u16).
fn word_expr(expr: &str) -> u16 {
    let src = format!("PROGRAM P\nVAR r AT D20 : WORD; END_VAR\nr := {expr};\nEND_PROGRAM");
    word(&run(&src, 1, |_| {}), "D20") as u16
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1e-4 * b.abs().max(1.0)
}

macro_rules! assert_real {
    ($expr:expr, $want:expr) => {{
        let got = real_expr($expr);
        assert!(close(got, $want), "{} = {got}, want {}", $expr, $want);
    }};
}

#[test]
fn scale_maps_raw_to_engineering_and_clamps() {
    assert_real!("SCALE(2000, 0, 4000, 0.0, 100.0)", 50.0);
    assert_real!("SCALE(0, 0, 4000, -50.0, 150.0)", -50.0);
    assert_real!("SCALE(4000, 0, 4000, -50.0, 150.0)", 150.0);
    // Clamped outside the raw range.
    assert_real!("SCALE(5000, 0, 4000, 0.0, 100.0)", 100.0);
    assert_real!("SCALE(-10, 0, 4000, 0.0, 100.0)", 0.0);
    // Reversed ranges and negative raw values.
    assert_real!("SCALE(1000, 4000, 0, 0.0, 100.0)", 75.0);
    assert_real!("SCALE(-16000, -32000, 32000, 0.0, 10.0)", 2.5);
    assert_real!("SCALE(32767, -32768, 32767, 0.0, 1.0)", 1.0);
    // Zero raw range does not divide by zero.
    assert_real!("SCALE(7, 100, 100, 3.0, 9.0)", 3.0);
}

#[test]
fn scale_i2i_uses_dint_intermediate_and_rounds() {
    assert_eq!(int_expr("SCALE_I2I(16000, 0, 32000, 0, 10000)"), 5000);
    // 32000 * 30000 overflows 16 bits but not the DINT intermediate.
    assert_eq!(int_expr("SCALE_I2I(32000, 0, 32000, 0, 30000)"), 30000);
    assert_eq!(int_expr("SCALE_I2I(1, 0, 3, 0, 2)"), 1); // 0.667 -> 1
    assert_eq!(int_expr("SCALE_I2I(1, 0, 4, 0, 2)"), 1); // 0.5 -> 1
    assert_eq!(int_expr("SCALE_I2I(1, 0, 4, 0, -2)"), -1); // -0.5 -> -1
    assert_eq!(int_expr("SCALE_I2I(1, 0, 3, 0, -2)"), -1); // -0.667 -> -1
    assert_eq!(int_expr("SCALE_I2I(1, 0, 5, 0, 2)"), 0); // 0.4 -> 0
    assert_eq!(int_expr("SCALE_I2I(-5, 0, 100, 10, 20)"), 10);
    assert_eq!(int_expr("SCALE_I2I(500, 0, 100, 10, 20)"), 20);
    assert_eq!(int_expr("SCALE_I2I(25, 100, 0, 0, 1000)"), 750);
    assert_eq!(int_expr("SCALE_I2I(32767, -32768, 32767, 0, 1000)"), 1000);
    assert_eq!(
        int_expr("SCALE_I2I(-32768, -32768, 32767, -1000, 1000)"),
        -1000
    );
    assert_eq!(int_expr("SCALE_I2I(5, 9, 9, 42, 50)"), 42);
}

/// The doc example "-32768..32767 -> -16000..16000 is fine" must really work: the old
/// "* 2" rounding trick overflowed the DINT intermediate (2 * 65535 * 32000 > 2^31).
#[test]
fn scale_i2i_full_16_bit_span_does_not_overflow() {
    assert_eq!(
        int_expr("SCALE_I2I(32767, -32768, 32767, -16000, 16000)"),
        16000
    );
    assert_eq!(
        int_expr("SCALE_I2I(-32768, -32768, 32767, -16000, 16000)"),
        -16000
    );
    assert_eq!(int_expr("SCALE_I2I(0, -32768, 32767, -16000, 16000)"), 0); // 0.24 -> 0
    assert_eq!(
        int_expr("SCALE_I2I(32767, -32768, 32767, 16000, -16000)"),
        -16000
    );
    assert_eq!(
        int_expr("SCALE_I2I(16384, -32768, 32767, -16000, 16000)"),
        8000
    ); // 8000.12
    // Exact halves round away from zero; odd denominators round correctly.
    assert_eq!(int_expr("SCALE_I2I(3, 0, 2, 0, 5)"), 5); // clamped
    assert_eq!(int_expr("SCALE_I2I(1, 0, 2, 0, 5)"), 3); // 2.5 -> 3
    assert_eq!(int_expr("SCALE_I2I(1, 0, 2, 0, -5)"), -3); // -2.5 -> -3
    assert_eq!(int_expr("SCALE_I2I(2, 0, 7, 0, 2)"), 1); // 0.571 -> 1
    assert_eq!(int_expr("SCALE_I2I(1, 0, 7, 0, 3)"), 0); // 0.429 -> 0
}

#[test]
fn unscale_maps_engineering_to_raw_clamped_and_rounded() {
    assert_eq!(int_expr("UNSCALE(50.0, 0.0, 100.0, 0, 4000)"), 2000);
    assert_eq!(int_expr("UNSCALE(150.0, 0.0, 100.0, 0, 4000)"), 4000);
    assert_eq!(int_expr("UNSCALE(-1.0, 0.0, 100.0, 0, 4000)"), 0);
    assert_eq!(int_expr("UNSCALE(0.0125, 0.0, 100.0, 0, 4000)"), 1); // 0.5 -> 1
    assert_eq!(int_expr("UNSCALE(0.01, 0.0, 100.0, 0, 4000)"), 0); // 0.4 -> 0
    assert_eq!(int_expr("UNSCALE(0.0, -10.0, 10.0, -32000, 32000)"), 0);
    assert_eq!(
        int_expr("UNSCALE(-10.0, -10.0, 10.0, -32000, 32000)"),
        -32000
    );
    assert_eq!(
        int_expr("UNSCALE(-9.9999, -10.0, 10.0, -32000, 32000)"),
        -32000
    );
    assert_eq!(int_expr("UNSCALE(25.0, 0.0, 100.0, 4000, 0)"), 3000);
    assert_eq!(int_expr("UNSCALE(5.0, 3.0, 3.0, 17, 900)"), 17);
    // Round trip with SCALE.
    assert_eq!(
        int_expr("UNSCALE(SCALE(1234, 0, 4000, 0.0, 10.0), 0.0, 10.0, 0, 4000)"),
        1234
    );
}

#[test]
fn norm_lerp_map() {
    assert_real!("NORM(25.0, 0.0, 100.0)", 0.25);
    assert_real!("NORM(-5.0, 0.0, 100.0)", 0.0);
    assert_real!("NORM(500.0, 0.0, 100.0)", 1.0);
    assert_real!("NORM(80.0, 100.0, 0.0)", 0.2);
    assert_real!("NORM(1.0, 2.0, 2.0)", 0.0);
    assert_real!("LERP(10.0, 20.0, 0.25)", 12.5);
    assert_real!("LERP(10.0, 20.0, 0.0)", 10.0);
    assert_real!("LERP(10.0, 20.0, 1.0)", 20.0);
    assert_real!("LERP(-4.0, 4.0, 1.5)", 8.0);
    assert_real!("MAP(5.0, 0.0, 10.0, 100.0, 200.0)", 150.0);
    assert_real!("MAP(20.0, 0.0, 10.0, 100.0, 200.0)", 300.0);
    assert_real!("MAP(-1.0, 0.0, 10.0, 0.0, -50.0)", 5.0);
    assert_real!("MAP(1.0, 3.0, 3.0, 7.0, 8.0)", 7.0);
}

#[test]
fn range_and_epsilon_checks() {
    assert!(bool_expr("IN_RANGE(5, 0, 10)"));
    assert!(bool_expr("IN_RANGE(0, 0, 10)"));
    assert!(bool_expr("IN_RANGE(10, 0, 10)"));
    assert!(!bool_expr("IN_RANGE(11, 0, 10)"));
    assert!(!bool_expr("IN_RANGE(-1, 0, 10)"));
    assert!(bool_expr("IN_RANGE(-5, 0, -10)"));
    assert!(bool_expr("IN_RANGE_R(0.5, 0.0, 1.0)"));
    assert!(!bool_expr("IN_RANGE_R(1.001, 0.0, 1.0)"));
    assert!(bool_expr("IN_RANGE_R(-2.0, -1.0, -3.0)"));
    assert!(bool_expr("ALMOST_EQ(0.1 + 0.2, 0.3, 0.0001)"));
    assert!(!bool_expr("ALMOST_EQ(1.0, 1.01, 0.001)"));
    assert!(bool_expr("ALMOST_EQ(-1.0, -1.0005, -0.001)"));
}

#[test]
fn deadband_and_sign() {
    assert_real!("DEADBAND(0.4, 0.5)", 0.0);
    assert_real!("DEADBAND(-0.5, 0.5)", 0.0);
    assert_real!("DEADBAND(-0.6, 0.5)", -0.6);
    assert_real!("DEADBAND(3.0, 0.0)", 3.0);
    assert_eq!(int_expr("SIGN(-32768)"), -1);
    assert_eq!(int_expr("SIGN(0)"), 0);
    assert_eq!(int_expr("SIGN(7)"), 1);
    assert_eq!(int_expr("SIGN_R(-0.001)"), -1);
    assert_eq!(int_expr("SIGN_R(0.0)"), 0);
    assert_eq!(int_expr("SIGN_R(1.0E10)"), 1);
}

#[test]
fn round_trunc_frac() {
    assert_eq!(dint_expr("ROUND(2.5)"), 3);
    assert_eq!(dint_expr("ROUND(-2.5)"), -3);
    assert_eq!(dint_expr("ROUND(2.4999)"), 2);
    assert_eq!(dint_expr("ROUND(-2.4999)"), -2);
    assert_eq!(dint_expr("ROUND(0.0)"), 0);
    assert_eq!(dint_expr("ROUND(0.49999997)"), 0);
    assert_eq!(dint_expr("ROUND(100000.5)"), 100001);
    assert_eq!(dint_expr("ROUND(-1000000.0)"), -1000000);
    assert_real!("TRUNC(2.7)", 2.0);
    assert_real!("TRUNC(-2.7)", -2.0);
    assert_real!("TRUNC(1.0E20)", 1.0e20);
    assert_real!("TRUNC(-1.0E20)", -1.0e20);
    assert_real!("FRAC(2.75)", 0.75);
    assert_real!("FRAC(-2.75)", -0.75);
    assert_real!("FRAC(5.0)", 0.0);
    assert_real!("FRAC(1.0E20)", 0.0);
}

#[test]
fn powers_and_safe_division() {
    assert_real!("SQR(-3.0)", 9.0);
    assert_real!("SQR(0.5)", 0.25);
    assert_real!("HYPOT(3.0, -4.0)", 5.0);
    assert_real!("HYPOT(0.0, 0.0)", 0.0);
    assert_eq!(int_expr("SAFE_DIV(7, 2, -1)"), 3);
    assert_eq!(int_expr("SAFE_DIV(-7, 2, -1)"), -3);
    assert_eq!(int_expr("SAFE_DIV(7, 0, -1)"), -1);
    assert_eq!(int_expr("SAFE_DIV(-32768, -1, 0)"), 32767);
    assert_real!("SAFE_DIV_R(1.0, 4.0, 0.0)", 0.25);
    assert_real!("SAFE_DIV_R(1.0, 0.0, 99.0)", 99.0);
    assert_real!("PERCENT(25.0, 200.0)", 12.5);
    assert_real!("PERCENT(300.0, 200.0)", 150.0);
    assert_real!("PERCENT(5.0, 0.0)", 0.0);
    // A zero divisor must not end the scan: later statements still run.
    let src = "PROGRAM P\nVAR a AT D20 : INT; b AT D21 : INT; z : INT := 0; zr : REAL; c AT D22 : REAL; END_VAR\n\
a := SAFE_DIV(10, z, 5); c := SAFE_DIV_R(1.0, zr, 2.0) + PERCENT(1.0, zr) + NORM(1.0, zr, zr); b := 9;\nEND_PROGRAM";
    let sim = run(src, 1, |_| {});
    assert_eq!((word(&sim, "D20"), word(&sim, "D21")), (5, 9));
    assert!(close(real(&sim, "D22"), 2.0));
}

#[test]
fn three_value_statistics() {
    for (a, b, c) in [
        (1, 2, 3),
        (3, 1, 2),
        (2, 3, 1),
        (1, 3, 2),
        (-5, -5, 0),
        (7, 7, 7),
        (-32768, 32767, 0),
    ] {
        let mut v = [a, b, c];
        v.sort();
        let args = format!("{a}, {b}, {c}");
        assert_eq!(int_expr(&format!("MIN3({args})")), v[0] as i16, "{args}");
        assert_eq!(int_expr(&format!("MED3({args})")), v[1] as i16, "{args}");
        assert_eq!(int_expr(&format!("MAX3({args})")), v[2] as i16, "{args}");
        let args_r = format!("{a}.0, {b}.0, {c}.0");
        assert_real!(&format!("MIN3_R({args_r})"), v[0] as f32);
        assert_real!(&format!("MED3_R({args_r})"), v[1] as f32);
        assert_real!(&format!("MAX3_R({args_r})"), v[2] as f32);
    }
    assert_eq!(int_expr("AVG3(30000, 30000, 30000)"), 30000);
    assert_eq!(int_expr("AVG3(-32768, -32768, -32768)"), -32768);
    assert_eq!(int_expr("AVG3(1, 2, 2)"), 1);
    assert_eq!(int_expr("AVG3(-1, -2, -2)"), -1);
    assert_real!("AVG3_R(1.0, 2.0, 2.0)", 5.0 / 3.0);
    assert_real!("AVG3_R(-3.0, 0.0, 3.0)", 0.0);
}

#[test]
fn unit_conversions() {
    assert_real!("C_TO_F(100.0)", 212.0);
    assert_real!("C_TO_F(-40.0)", -40.0);
    assert_real!("F_TO_C(32.0)", 0.0);
    assert_real!("F_TO_C(98.6)", 37.0);
    assert_real!("BAR_TO_PSI(1.0)", 14.503774);
    assert_real!("PSI_TO_BAR(14.503774)", 1.0);
    assert_real!("PSI_TO_BAR(BAR_TO_PSI(6.5))", 6.5);
}

/// Every math block is inlined at each call site, so program steps grow with each call:
/// keep each block small (120 instructions; a few heavier blocks have their own cap
/// below). Prints the cost per call (`--nocapture`).
#[test]
fn math_blocks_stay_small() {
    let base = plc::compile_st("PROGRAM P\nVAR r : INT; END_VAR\nr := 0;\nEND_PROGRAM")
        .unwrap()
        .instructions;
    let mut too_big = Vec::new();
    for e in crate::st::library_entries() {
        if e.category != "math" {
            continue;
        }
        let src = if e.kind == "FUNCTION" {
            let args = e
                .inputs
                .iter()
                .map(|p| match p.ty.as_str() {
                    "BOOL" => "FALSE",
                    "REAL" => "0.0",
                    _ => "0",
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "PROGRAM P\nVAR r : {}; END_VAR\nr := {}({args});\nEND_PROGRAM",
                e.returns.as_deref().unwrap(),
                e.name
            )
        } else {
            format!("PROGRAM P\nVAR b : {}; END_VAR\nb();\nEND_PROGRAM", e.name)
        };
        let cost = plc::compile_st(&src).unwrap().instructions - base;
        println!("{:<12} {cost}", e.name);
        let cap = match e.name.as_str() {
            "SCALE_I2I" => 300,
            // 16 REAL parameters copied into two arrays plus the segment walk.
            "CURVE8" => 250,
            "UNSCALE" | "BCD_TO_INT" | "FIFO16" => 150,
            // Bit-by-bit rotation (two carries) and the running statistics.
            "ROL_W" | "ROR_W" | "MIN_MAX_AVG" => 140,
            _ => 120,
        };
        if cost > cap {
            too_big.push(format!("{} costs {cost}", e.name));
        }
    }
    assert!(too_big.is_empty(), "instructions per call: {too_big:?}");
}

#[test]
fn single_bit_operations_on_words() {
    assert_eq!(word_expr("BIT_MASK(0)"), 0x0001);
    assert_eq!(word_expr("BIT_MASK(15)"), 0x8000);
    assert_eq!(word_expr("BIT_MASK(16)"), 0);
    assert_eq!(word_expr("BIT_MASK(-1)"), 0);
    for pos in 0..16 {
        let mask = 1u16 << pos;
        assert_eq!(word_expr(&format!("BIT_MASK({pos})")), mask);
        assert!(
            bool_expr(&format!("GET_BIT(16#{mask:04X}, {pos})")),
            "{pos}"
        );
        assert!(
            !bool_expr(&format!("GET_BIT(16#{:04X}, {pos})", !mask)),
            "{pos}"
        );
        assert_eq!(word_expr(&format!("SET_BIT(16#0000, {pos})")), mask);
        assert_eq!(word_expr(&format!("CLR_BIT(16#FFFF, {pos})")), !mask);
        assert_eq!(
            word_expr(&format!("TOGGLE_BIT(16#A5A5, {pos})")),
            0xA5A5 ^ mask
        );
    }
    assert!(!bool_expr("GET_BIT(16#FFFF, 16)"));
    assert_eq!(word_expr("SET_BIT(16#1234, 20)"), 0x1234);
    assert_eq!(word_expr("CLR_BIT(16#1234, -3)"), 0x1234);
    assert_eq!(word_expr("TOGGLE_BIT(16#1234, 99)"), 0x1234);
    // Bit number from a variable, toggled twice -> unchanged.
    let src = "PROGRAM P\nVAR w AT D20 : WORD; n : INT := 9; g AT Y0 : BOOL; END_VAR\n\
w := SET_BIT(w, n); g := GET_BIT(w, n); w := TOGGLE_BIT(TOGGLE_BIT(w, 3), 3);\nEND_PROGRAM";
    let sim = run(src, 1, |_| {});
    assert_eq!(word(&sim, "D20") as u16, 0x0200);
    assert!(bit(&sim, "Y0"));
}

#[test]
fn bit_count_word_value_and_bytes() {
    assert_eq!(int_expr("BIT_COUNT(16#0000)"), 0);
    assert_eq!(int_expr("BIT_COUNT(16#FFFF)"), 16);
    assert_eq!(int_expr("BIT_COUNT(16#8001)"), 2);
    assert_eq!(int_expr("BIT_COUNT(16#A5A5)"), 8);
    assert_eq!(dint_expr("WORD_VALUE(16#FFFF)"), 65535);
    assert_eq!(dint_expr("WORD_VALUE(16#8000)"), 32768);
    assert_eq!(dint_expr("WORD_VALUE(16#7FFF)"), 32767);
    assert_eq!(dint_expr("WORD_VALUE(16#0000)"), 0);
    assert_eq!(int_expr("LO_BYTE(16#12AB)"), 0xAB);
    assert_eq!(int_expr("HI_BYTE(16#12AB)"), 0x12);
    assert_eq!(int_expr("HI_BYTE(16#FF00)"), 0xFF);
    assert_eq!(word_expr("MAKE_WORD(16#12, 16#AB)"), 0x12AB);
    assert_eq!(word_expr("MAKE_WORD(255, 255)"), 0xFFFF);
    assert_eq!(word_expr("MAKE_WORD(16#1FF, -1)"), 0xFFFF);
    assert_eq!(word_expr("SWAP_BYTES(16#12AB)"), 0xAB12);
    assert_eq!(word_expr("SWAP_BYTES(16#00FF)"), 0xFF00);
}

#[test]
fn pack8_and_unpack8() {
    let src = "PROGRAM P\nVAR p : PACK8; u : UNPACK8; w AT D20 : WORD; o0 AT Y0 : BOOL; o1 AT Y1 : BOOL;\n\
o2 AT Y2 : BOOL; o3 AT Y3 : BOOL; o4 AT Y4 : BOOL; o5 AT Y5 : BOOL; o6 AT Y6 : BOOL; o7 AT Y7 : BOOL; END_VAR\n\
p(IN0 := TRUE, IN1 := FALSE, IN2 := TRUE, IN3 := TRUE, IN4 := FALSE, IN5 := FALSE, IN6 := FALSE, IN7 := X0);\n\
w := p.OUT;\nu(IN := w OR 16#FF00);\n\
o0 := u.Q0; o1 := u.Q1; o2 := u.Q2; o3 := u.Q3; o4 := u.Q4; o5 := u.Q5; o6 := u.Q6; o7 := u.Q7;\nEND_PROGRAM";
    let outputs = |sim: &Simulator| {
        (0..8)
            .map(|i| bit(sim, &format!("Y{i}")))
            .collect::<Vec<_>>()
    };
    let sim = run(src, 1, |_| {});
    assert_eq!(word(&sim, "D20") as u16, 0b0000_1101);
    assert_eq!(
        outputs(&sim),
        [true, false, true, true, false, false, false, false]
    );
    let sim = run(src, 2, |s| s.memory.set_input("X0".parse().unwrap(), true));
    assert_eq!(word(&sim, "D20") as u16, 0b1000_1101);
    assert_eq!(
        outputs(&sim),
        [true, false, true, true, false, false, false, true]
    );
}

#[test]
fn bcd_conversions() {
    assert_eq!(int_expr("BCD_TO_INT(16#1234)"), 1234);
    assert_eq!(int_expr("BCD_TO_INT(16#9999)"), 9999);
    assert_eq!(int_expr("BCD_TO_INT(16#0000)"), 0);
    assert_eq!(int_expr("BCD_TO_INT(16#0907)"), 907);
    assert_eq!(int_expr("BCD_TO_INT(16#12A4)"), -1);
    assert_eq!(int_expr("BCD_TO_INT(16#F000)"), -1);
    assert_eq!(word_expr("INT_TO_BCD(1234)"), 0x1234);
    assert_eq!(word_expr("INT_TO_BCD(9999)"), 0x9999);
    assert_eq!(word_expr("INT_TO_BCD(905)"), 0x0905);
    assert_eq!(word_expr("INT_TO_BCD(0)"), 0);
    assert_eq!(word_expr("INT_TO_BCD(-5)"), 0);
    assert_eq!(word_expr("INT_TO_BCD(12345)"), 0x9999);
    for v in [1, 42, 808, 5070, 8191] {
        assert_eq!(int_expr(&format!("BCD_TO_INT(INT_TO_BCD({v}))")), v);
    }
}

#[test]
fn shift_words_left_and_right() {
    for n in 0..16 {
        for v in [0x0001u16, 0x00FF, 0x8001, 0xA5A5, 0xFFFF, 0x4000] {
            assert_eq!(
                word_expr(&format!("SHL_W(16#{v:04X}, {n})")),
                v << n,
                "SHL {v:04X} {n}"
            );
            assert_eq!(
                word_expr(&format!("SHR_W(16#{v:04X}, {n})")),
                v >> n,
                "SHR {v:04X} {n}"
            );
        }
    }
    // Outside 0..15: N <= 0 unchanged, N >= 16 gives 0.
    assert_eq!(word_expr("SHL_W(16#1234, -1)"), 0x1234);
    assert_eq!(word_expr("SHR_W(16#1234, -32768)"), 0x1234);
    assert_eq!(word_expr("SHL_W(16#FFFF, 16)"), 0);
    assert_eq!(word_expr("SHR_W(16#FFFF, 32767)"), 0);
    // Doc examples.
    assert_eq!(word_expr("SHL_W(16#00FF, 4)"), 0x0FF0);
    assert_eq!(word_expr("SHR_W(16#8000, 15)"), 0x0001);
}

#[test]
fn rotate_words_left_and_right() {
    for n in -20i32..36 {
        let k = n.rem_euclid(16) as u32;
        for v in [0x0001u16, 0x8001, 0xA5A5, 0xFFFF, 0x1234, 0x0000] {
            assert_eq!(
                word_expr(&format!("ROL_W(16#{v:04X}, {n})")),
                v.rotate_left(k),
                "ROL {v:04X} {n}"
            );
            assert_eq!(
                word_expr(&format!("ROR_W(16#{v:04X}, {n})")),
                v.rotate_right(k),
                "ROR {v:04X} {n}"
            );
        }
    }
    assert_eq!(word_expr("ROL_W(16#8001, 1)"), 0x0003);
    assert_eq!(word_expr("ROR_W(16#8001, 1)"), 0xC000);
    assert_eq!(word_expr("ROL_W(16#1234, -32768)"), 0x1234);
    // Rotating a variable step by step through a whole turn restores it.
    let src = "PROGRAM P\nVAR w AT D20 : WORD; END_VAR\nw := ROL_W(w, 5);\nEND_PROGRAM";
    let start = |s: &mut Simulator| s.memory.set_word("D20".parse().unwrap(), 3);
    assert_eq!(word(&run(src, 16, start), "D20") as u16, 0x0003);
    assert_eq!(word(&run(src, 1, start), "D20") as u16, 0x0060);
    assert_eq!(word(&run(src, 3, start), "D20") as u16, 0x8001);
}

#[test]
fn mux4_selects_and_clamps_the_index() {
    for (k, want) in [
        (-32768, 10),
        (-1, 10),
        (0, 10),
        (1, -20),
        (2, 30),
        (3, 40),
        (4, 40),
        (32767, 40),
    ] {
        assert_eq!(
            int_expr(&format!("MUX4({k}, 10, -20, 30, 40)")),
            want,
            "K = {k}"
        );
    }
    assert_real!("MUX4_R(-5, 1.5, 2.5, 3.5, 4.5)", 1.5);
    assert_real!("MUX4_R(0, 1.5, 2.5, 3.5, 4.5)", 1.5);
    assert_real!("MUX4_R(1, 1.5, 2.5, 3.5, 4.5)", 2.5);
    assert_real!("MUX4_R(2, 1.5, 2.5, 3.5, 4.5)", 3.5);
    assert_real!("MUX4_R(3, 1.5, 2.5, 3.5, -4.5)", -4.5);
    assert_real!("MUX4_R(9, 1.5, 2.5, 3.5, -4.5)", -4.5);
}

#[test]
fn demux4_routes_to_one_output() {
    let src = "PROGRAM P\nVAR d : DEMUX4; k AT D10 : INT; o0 AT D20 : INT; o1 AT D21 : INT;\n\
o2 AT D22 : INT; o3 AT D23 : INT; END_VAR\n\
d(IN := -77, K := k); o0 := d.OUT0; o1 := d.OUT1; o2 := d.OUT2; o3 := d.OUT3;\nEND_PROGRAM";
    let program = plc::compile_st(src).unwrap();
    let mut sim = Simulator::default();
    let outs = |sim: &Simulator| {
        (0..4)
            .map(|i| word(sim, &format!("D2{i}")))
            .collect::<Vec<_>>()
    };
    for (k, want) in [
        (0, [-77, 0, 0, 0]),
        (2, [0, 0, -77, 0]),
        (1, [0, -77, 0, 0]),
        (3, [0, 0, 0, -77]),
        (4, [0, 0, 0, 0]),
        (-1, [0, 0, 0, 0]),
        (0, [-77, 0, 0, 0]),
    ] {
        sim.memory.set_word("D10".parse().unwrap(), k);
        sim.scan(&program, 100);
        assert_eq!(outs(&sim), want, "K = {k}");
    }
}

#[test]
fn curve8_interpolates_and_clamps() {
    // Points (0, 0), (50, 400), (100, 1000); the rest unused but non-zero to prove N works.
    let call = |x: &str, n: i32| {
        format!(
            "CURVE8({x}, {n}, 0.0, 50.0, 100.0, 7.0, 7.0, 7.0, 7.0, 7.0, \
             0.0, 400.0, 1000.0, 9.0, 9.0, 9.0, 9.0, 9.0)"
        )
    };
    assert_real!(&call("25.0", 3), 200.0);
    assert_real!(&call("50.0", 3), 400.0);
    assert_real!(&call("75.0", 3), 700.0);
    assert_real!(&call("0.0", 3), 0.0);
    assert_real!(&call("100.0", 3), 1000.0);
    // Clamped outside X1..XN.
    assert_real!(&call("-10.0", 3), 0.0);
    assert_real!(&call("150.0", 3), 1000.0);
    // N = 2 uses only the first segment, N = 1 is constant Y1, N is clamped to 1..8.
    assert_real!(&call("75.0", 2), 400.0);
    assert_real!(&call("25.0", 2), 200.0);
    assert_real!(&call("25.0", 1), 0.0);
    assert_real!(&call("-25.0", 1), 0.0);
    assert_real!(&call("25.0", 0), 0.0);
    assert_real!(&call("25.0", -5), 0.0);
    // All 8 points: y = 2x on 0..7, N = 99 is clamped to 8.
    let full = |x: f32, n: i32| {
        format!(
            "CURVE8({x:?}, {n}, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, \
             0.0, 2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0)"
        )
    };
    for (x, want) in [
        (0.5, 1.0),
        (3.25, 6.5),
        (6.5, 13.0),
        (6.999, 13.998),
        (7.0, 14.0),
        (9.0, 14.0),
    ] {
        assert_real!(&full(x, 8), want);
        assert_real!(&full(x, 99), want);
    }
    // Decreasing Y works; equal neighbouring X (a step) never divides by zero.
    assert_real!(
        "CURVE8(2.0, 3, 0.0, 4.0, 8.0, 0.0, 0.0, 0.0, 0.0, 0.0, 100.0, 60.0, -20.0, 0.0, 0.0, 0.0, 0.0, 0.0)",
        80.0
    );
    assert_real!(
        "CURVE8(5.0, 3, 0.0, 5.0, 5.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 0.0, 0.0)",
        3.0
    );
    assert_real!(
        "CURVE8(4.0, 3, 0.0, 5.0, 5.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 0.0, 0.0)",
        1.8
    );
    // Non-ascending X does not fault and later statements still run.
    let src = "PROGRAM P\nVAR r AT D20 : REAL; b AT D22 : INT; END_VAR\n\
r := CURVE8(3.0, 4, 0.0, 10.0, 2.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 10.0, 20.0, 30.0, 0.0, 0.0, 0.0, 0.0); b := 5;\nEND_PROGRAM";
    let sim = run(src, 1, |_| {});
    assert!(close(real(&sim, "D20"), 3.0), "{}", real(&sim, "D20"));
    assert_eq!(word(&sim, "D22"), 5);
    // The doc example (tank table).
    assert_real!(
        "CURVE8(75.0, 3, 0.0, 50.0, 100.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 400.0, 1000.0, 0.0, 0.0, 0.0, 0.0, 0.0)",
        700.0
    );
}

/// FIFO16 / LIFO16 test bench: `X0` push, `X1` pop, `X2` reset, value from `D10`;
/// outputs OUT -> D20, COUNT -> D21, EMPTY -> Y0, FULL -> Y1.
fn buffer_bench(block: &str) -> plc::Program {
    let src = format!(
        "PROGRAM P\nVAR b : {block}; v AT D10 : INT; o AT D20 : INT; c AT D21 : INT; e AT Y0 : BOOL; f AT Y1 : BOOL; END_VAR\n\
b(IN := v, PUSH := X0, POP := X1, RST := X2); o := b.OUT; c := b.COUNT; e := b.EMPTY; f := b.FULL;\nEND_PROGRAM"
    );
    plc::compile_st(&src).unwrap_or_else(|e| panic!("{e:?}\n{src}"))
}

struct Bench {
    program: plc::Program,
    sim: Simulator,
}

impl Bench {
    fn new(block: &str) -> Self {
        let program = buffer_bench(block);
        let mut sim = Simulator::default();
        sim.scan(&program, 100);
        Bench { program, sim }
    }
    fn set(&mut self, x: &str, on: bool) {
        self.sim.memory.set_input(x.parse().unwrap(), on);
    }
    fn value(&mut self, v: i16) {
        self.sim.memory.set_word("D10".parse().unwrap(), v);
    }
    fn scan(&mut self) {
        self.sim.scan(&self.program, 100);
    }
    /// One rising and falling edge on `x` (two scans).
    fn pulse(&mut self, x: &str) {
        self.set(x, true);
        self.scan();
        self.set(x, false);
        self.scan();
    }
    fn push(&mut self, v: i16) {
        self.value(v);
        self.pulse("X0");
    }
    fn pop(&mut self) -> i16 {
        self.pulse("X1");
        self.out()
    }
    fn out(&self) -> i16 {
        word(&self.sim, "D20")
    }
    /// (COUNT, EMPTY, FULL)
    fn state(&self) -> (i16, bool, bool) {
        (
            word(&self.sim, "D21"),
            bit(&self.sim, "Y0"),
            bit(&self.sim, "Y1"),
        )
    }
}

#[test]
fn fifo16_is_first_in_first_out() {
    let mut b = Bench::new("FIFO16");
    assert_eq!(b.state(), (0, true, false));
    assert_eq!(b.out(), 0);
    // Pop on an empty buffer: nothing happens.
    assert_eq!(b.pop(), 0);
    assert_eq!(b.state(), (0, true, false));
    for v in 1..=16 {
        b.push(v * 10);
        assert_eq!(b.state(), (v, false, v == 16));
    }
    b.push(999); // ignored while full
    assert_eq!(b.state(), (16, false, true));
    for v in 1..=5 {
        assert_eq!(b.pop(), v * 10);
    }
    // Wrap around the ring: push 5 more, then everything comes out in order.
    for v in 17..=21 {
        b.push(v * 10);
    }
    assert_eq!(b.state(), (16, false, true));
    for v in 6..=21 {
        assert_eq!(b.pop(), v * 10);
    }
    assert_eq!(b.state(), (0, true, false));
    assert_eq!(b.pop(), 210); // empty: OUT keeps its value
    // A held PUSH stores once only.
    b.value(-5);
    b.set("X0", true);
    for _ in 0..5 {
        b.scan();
    }
    assert_eq!(b.state(), (1, false, false));
    b.set("X0", false);
    b.scan();
    // PUSH and POP edges in the same scan: POP first, then PUSH.
    b.value(42);
    b.set("X0", true);
    b.set("X1", true);
    b.scan();
    assert_eq!(b.out(), -5);
    assert_eq!(b.state(), (1, false, false));
    b.set("X0", false);
    b.set("X1", false);
    b.scan();
    assert_eq!(b.pop(), 42);
    // Both edges on an empty buffer: the value is stored, OUT unchanged.
    b.value(43);
    b.set("X0", true);
    b.set("X1", true);
    b.scan();
    assert_eq!((b.out(), b.state()), (42, (1, false, false)));
    b.set("X0", false);
    b.set("X1", false);
    b.scan();
    assert_eq!(b.pop(), 43);
    // RST empties, clears OUT and is dominant over edges.
    b.push(1);
    b.push(2);
    b.set("X2", true);
    b.set("X0", true);
    b.scan();
    assert_eq!(b.state(), (0, true, false));
    assert_eq!(b.out(), 0);
    b.set("X2", false);
    b.scan(); // PUSH still held: no new edge after RST
    assert_eq!(b.state(), (0, true, false));
    b.set("X0", false);
    b.scan();
    b.push(7);
    b.push(8);
    assert_eq!(b.pop(), 7);
    assert_eq!(b.pop(), 8);
}

#[test]
fn lifo16_is_last_in_first_out() {
    let mut b = Bench::new("LIFO16");
    assert_eq!(b.state(), (0, true, false));
    assert_eq!(b.pop(), 0);
    for v in 1..=16 {
        b.push(v);
        assert_eq!(b.state(), (v, false, v == 16));
    }
    b.push(99); // ignored while full
    assert_eq!(b.state(), (16, false, true));
    for v in (13..=16).rev() {
        assert_eq!(b.pop(), v);
    }
    b.push(-3);
    assert_eq!(b.state(), (13, false, false));
    assert_eq!(b.pop(), -3);
    for v in (1..=12).rev() {
        assert_eq!(b.pop(), v);
    }
    assert_eq!(b.state(), (0, true, false));
    assert_eq!(b.pop(), 1); // empty: OUT keeps its value
    // PUSH and POP in the same scan replace the top value.
    b.push(5);
    b.value(6);
    b.set("X0", true);
    b.set("X1", true);
    b.scan();
    assert_eq!(b.out(), 5);
    assert_eq!(b.state(), (1, false, false));
    b.set("X0", false);
    b.set("X1", false);
    b.scan();
    assert_eq!(b.pop(), 6);
    // RST.
    b.push(1);
    b.push(2);
    b.set("X2", true);
    b.scan();
    assert_eq!(b.state(), (0, true, false));
    assert_eq!(b.out(), 0);
    b.set("X2", false);
    b.scan();
    b.push(8);
    assert_eq!(b.pop(), 8);
}

#[test]
fn buffers_take_a_push_on_the_first_scan() {
    for block in ["FIFO16", "LIFO16"] {
        let program = buffer_bench(block);
        let mut sim = Simulator::default();
        sim.memory.set_word("D10".parse().unwrap(), 11);
        sim.memory.set_input("X0".parse().unwrap(), true);
        sim.scan(&program, 100);
        sim.scan(&program, 100);
        assert_eq!(word(&sim, "D21"), 1, "{block}");
    }
}

/// MIN_MAX_AVG bench: IN from D10/D11 (REAL), SAMPLE X0, CONT X1, RST X2;
/// MINIMUM -> D20, MAXIMUM -> D22, AVERAGE -> D24, COUNT -> D26.
fn stats_bench() -> plc::Program {
    let src = "PROGRAM P\nVAR s : MIN_MAX_AVG; v AT D10 : REAL; lo AT D20 : REAL; hi AT D22 : REAL;\n\
av AT D24 : REAL; n AT D26 : DINT; END_VAR\n\
s(IN := v, SAMPLE := X0, CONT := X1, RST := X2); lo := s.MINIMUM; hi := s.MAXIMUM; av := s.AVERAGE; n := s.COUNT;\nEND_PROGRAM";
    plc::compile_st(src).unwrap_or_else(|e| panic!("{e:?}"))
}

fn set_real(sim: &mut Simulator, device: &str, v: f32) {
    let d: plc::Device = device.parse().unwrap();
    let bits = v.to_bits();
    let next: plc::Device = format!("D{}", d.index + 1).parse().unwrap();
    sim.memory.set_word(d, bits as u16 as i16);
    sim.memory.set_word(next, (bits >> 16) as u16 as i16);
}

/// (MINIMUM, MAXIMUM, AVERAGE, COUNT)
fn stats(sim: &Simulator) -> (f32, f32, f32, i32) {
    let n = (word(sim, "D26") as u16 as i32) | ((word(sim, "D27") as i32) << 16);
    (real(sim, "D20"), real(sim, "D22"), real(sim, "D24"), n)
}

#[test]
fn min_max_avg_samples_on_edges() {
    let program = stats_bench();
    let mut sim = Simulator::default();
    let input =
        |sim: &mut Simulator, x: &str, on: bool| sim.memory.set_input(x.parse().unwrap(), on);
    sim.scan(&program, 100);
    assert_eq!(stats(&sim), (0.0, 0.0, 0.0, 0));
    let sample = |sim: &mut Simulator, v: f32| {
        set_real(sim, "D10", v);
        input(sim, "X0", true);
        sim.scan(&program, 100);
        sim.scan(&program, 100); // held: no second sample
        input(sim, "X0", false);
        sim.scan(&program, 100);
    };
    sample(&mut sim, 5.0);
    assert_eq!(stats(&sim), (5.0, 5.0, 5.0, 1));
    sample(&mut sim, -3.0);
    sample(&mut sim, 10.0);
    sample(&mut sim, 4.0);
    let (lo, hi, av, n) = stats(&sim);
    assert_eq!((lo, hi, n), (-3.0, 10.0, 4));
    assert!(close(av, 4.0), "{av}");
    // Without an edge, a changing input is not sampled.
    set_real(&mut sim, "D10", 1000.0);
    sim.scan(&program, 100);
    assert_eq!(stats(&sim).1, 10.0);
    // RST clears everything and is dominant over SAMPLE and CONT.
    input(&mut sim, "X2", true);
    input(&mut sim, "X1", true);
    input(&mut sim, "X0", true);
    sim.scan(&program, 100);
    assert_eq!(stats(&sim), (0.0, 0.0, 0.0, 0));
    sim.scan(&program, 100);
    assert_eq!(stats(&sim), (0.0, 0.0, 0.0, 0));
    input(&mut sim, "X1", false);
    input(&mut sim, "X2", false);
    sim.scan(&program, 100); // SAMPLE still held from during RST: no edge
    assert_eq!(stats(&sim).3, 0);
    input(&mut sim, "X0", false);
    sim.scan(&program, 100);
    sample(&mut sim, -2.5);
    assert_eq!(stats(&sim), (-2.5, -2.5, -2.5, 1));
}

#[test]
fn min_max_avg_samples_every_scan_in_cont_mode() {
    let program = stats_bench();
    let mut sim = Simulator::default();
    sim.memory.set_input("X1".parse().unwrap(), true);
    for v in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0] {
        set_real(&mut sim, "D10", v);
        sim.scan(&program, 100);
    }
    let (lo, hi, av, n) = stats(&sim);
    assert_eq!((lo, hi, n), (1.0, 6.0, 6));
    assert!(close(av, 3.5), "{av}");
    // A constant input for many scans keeps the average where it is.
    set_real(&mut sim, "D10", 3.5);
    for _ in 0..200 {
        sim.scan(&program, 100);
    }
    let (_, _, av, n) = stats(&sim);
    assert_eq!(n, 206);
    assert!(close(av, 3.5), "{av}");
    // CONT off: no more samples without SAMPLE edges.
    sim.memory.set_input("X1".parse().unwrap(), false);
    set_real(&mut sim, "D10", 100.0);
    sim.scan(&program, 100);
    assert_eq!(stats(&sim).3, 206);
    // SAMPLE already TRUE on the first scan counts as an edge.
    let mut sim = Simulator::default();
    set_real(&mut sim, "D10", 7.0);
    sim.memory.set_input("X0".parse().unwrap(), true);
    sim.scan(&program, 100);
    assert_eq!(stats(&sim), (7.0, 7.0, 7.0, 1));
}

/// Every `Example:` line of a math block compiles (with its variables declared), and the
/// Turkish `Örnek:` line is the same code.
#[test]
fn math_doc_examples_compile() {
    const TYPES: &[(&str, &str)] = &[
        (
            "INT",
            "raw pct ao speed dir err n avg total recipe speed_sp zone sp_zone2 hi lo sens_a sens_b sens_c t code tray last active preset count",
        ),
        (
            "REAL",
            "level speed_pct f temp ma vol lvl x v joy flow whole hours minutes p i_amp r_ohm dist dx dy rate volume seconds fill level_m sp sp_day sp_night p_a p_b p_c p_hi p_lo t_max t_f t_c p_psi p_bar",
        ),
        ("BOOL", "ok same clk_1s"),
        (
            "WORD",
            "m status cmd leds w hi_nibble pattern alarms raw_cnt reg thumb disp",
        ),
        ("DINT", "pulses"),
        ("DEMUX4", "d1"),
        ("MIN_MAX_AVG", "st1"),
        ("FIFO16", "q1"),
        ("LIFO16", "s1"),
        ("PACK8", "pk1"),
        ("UNPACK8", "u1"),
    ];
    let example = |doc: &str, tag: &str| -> String {
        let at = doc
            .find(tag)
            .unwrap_or_else(|| panic!("no {tag} in\n{doc}"));
        doc[at + tag.len()..]
            .lines()
            .next()
            .unwrap()
            .trim()
            .to_string()
    };
    let mut failures = Vec::new();
    for e in crate::st::library_entries() {
        if e.category != "math" {
            continue;
        }
        let code = example(&e.doc, "Example:");
        assert_eq!(code, example(&e.doc_tr, "Örnek:"), "{}", e.name);
        if e.kind == "FUNCTION" {
            assert!(
                code.contains(&format!("{}(", e.name)),
                "{}: example does not call the block",
                e.name
            );
        }
        let mut decls = String::new();
        for word in code.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
            if let Some((ty, _)) = TYPES
                .iter()
                .find(|(_, names)| names.split(' ').any(|n| n == word))
                && !decls.contains(&format!(" {word} :"))
            {
                decls.push_str(&format!(" {word} : {ty};"));
            }
        }
        // `mode` selects in MUX4_R; the REAL setpoint variables use other names.
        if code.contains("mode,") {
            decls.push_str(" mode : INT;");
        }
        let src = format!("PROGRAM P\nVAR{decls} END_VAR\n{code}\nEND_PROGRAM");
        if let Err(err) = plc::compile_st(&src) {
            failures.push(format!("{}: {err:?}\n{src}", e.name));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// The compiled division ends the scan when the divisor is zero OR denormal (exponent 0),
/// so every guarded division must treat a denormal divisor like zero. `d` is made denormal
/// at run time (1.0E-20 * 1.0E-20 = 1.0E-40); the statement after the call must still run.
#[test]
fn guarded_divisions_also_reject_denormal_divisors() {
    for (expr, want) in [
        ("SAFE_DIV_R(1.0, d, 99.0)", 99.0),
        ("SAFE_DIV_R(1.0, -d, 99.0)", 99.0),
        ("PERCENT(1.0, d)", 0.0),
        ("NORM(1.0, 0.0, d)", 0.0),
        ("NORM(1.0, d, 0.0)", 0.0),
        ("MAP(1.0, 0.0, d, 5.0, 6.0)", 5.0),
        ("INT_TO_REAL(UNSCALE(1.0, 0.0, d, 7, 100))", 7.0),
    ] {
        let src = format!(
            "PROGRAM P\nVAR d : REAL; r AT D20 : REAL; b AT D22 : INT; END_VAR\n\
             d := 1.0E-20 * 1.0E-20; r := {expr}; b := 9;\nEND_PROGRAM"
        );
        let sim = run(&src, 1, |_| {});
        assert_eq!(word(&sim, "D22"), 9, "{expr}: the scan was aborted");
        assert!(
            close(real(&sim, "D20"), want),
            "{expr} = {}",
            real(&sim, "D20")
        );
    }
    // The smallest normal REAL still divides.
    assert_real!(
        "SAFE_DIV_R(1.0E-30, 1.17549435E-38, 0.0)",
        1.0e-30 / 1.175_494_4e-38
    );
}

/// A step at the first point behaves like any other step: at that X the later Y wins.
#[test]
fn curve8_step_at_the_first_point_returns_the_later_y() {
    assert_real!(
        "CURVE8(0.0, 3, 0.0, 0.0, 5.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 0.0, 0.0)",
        2.0
    );
    assert_real!(
        "CURVE8(-0.1, 3, 0.0, 0.0, 5.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 0.0, 0.0)",
        1.0
    );
    assert_real!(
        "CURVE8(2.5, 3, 0.0, 0.0, 5.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 0.0, 0.0)",
        2.5
    );
    // Without a step, IN = X_1 still gives Y_1 exactly.
    assert_real!(
        "CURVE8(1.0, 2, 1.0, 3.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -4.0, 4.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)",
        -4.0
    );
}

/// DEMUX4 has no memory: skipping the call keeps the outputs, the next call recomputes them.
#[test]
fn demux4_may_be_called_conditionally() {
    let src = "PROGRAM P\nVAR d : DEMUX4; o1 AT D21 : INT; o2 AT D22 : INT; END_VAR\n\
IF NOT X0 THEN d(IN := 5, K := 1); END_IF; IF X0 THEN d(IN := 6, K := 2); END_IF;\n\
o1 := d.OUT1; o2 := d.OUT2;\nEND_PROGRAM";
    let program = plc::compile_st(src).unwrap();
    let mut sim = Simulator::default();
    sim.scan(&program, 100);
    assert_eq!((word(&sim, "D21"), word(&sim, "D22")), (5, 0));
    sim.memory.set_input("X0".parse().unwrap(), true);
    sim.scan(&program, 100);
    assert_eq!((word(&sim, "D21"), word(&sim, "D22")), (0, 6));
}

/// CURVE8's doc states its program-memory cost per call site and its jump labels
/// (P0..P255 are shared by the whole program). Keep both numbers honest: the words within
/// 10 % (or 10 words) of the measurement, the labels exact.
#[test]
fn curve8_size_note_matches_measurement() {
    let call = "r := CURVE8(x, 3, 0.0, 50.0, 100.0, 0.0, 0.0, 0.0, 0.0, 0.0, \
                0.0, 400.0, 1000.0, 0.0, 0.0, 0.0, 0.0, 0.0);\n";
    let src = |n: usize| {
        format!(
            "PROGRAM P\nVAR r : REAL; x : REAL; END_VAR\n{}END_PROGRAM",
            call.repeat(n)
        )
    };
    let words = |n: usize| {
        let program = plc::compile_st_with(&src(n), &crate::st::LowerOptions { share: crate::st::Share::Never, ..Default::default() }).unwrap();
        crate::es2_encoder::encode(&program).unwrap().words.len()
    };
    let per_call = words(2) - words(1);
    // Distinct `P<n>` labels. Every program has the shared end label P0, so the cost of
    // one call site is the difference between two calls and one call.
    let label_count = |n: usize| {
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
    let labels = label_count(2) - label_count(1);
    assert_eq!(
        label_count(3) - label_count(2),
        labels,
        "labels per call site are not constant"
    );
    println!("CURVE8: {per_call} words per call, {labels} labels");
    let number_after = |doc: &str, marker: &str| -> usize {
        let at = doc
            .find(marker)
            .unwrap_or_else(|| panic!("no {marker:?} in\n{doc}"));
        doc[at + marker.len()..]
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .unwrap()
            .parse()
            .unwrap_or_else(|_| panic!("no number after {marker:?}"))
    };
    let e = crate::st::library_entries()
        .into_iter()
        .find(|e| e.name == "CURVE8")
        .unwrap();
    let tolerance = (per_call / 10).max(10);
    for (doc, words_marker, labels_marker) in [
        (&e.doc, "Size: about ", " words per call site and "),
        (&e.doc_tr, "Boyut: her çağrı yeri yaklaşık ", " word ve "),
    ] {
        let stated = number_after(doc, words_marker);
        assert!(
            stated.abs_diff(per_call) <= tolerance,
            "doc says {stated} words, measured {per_call}"
        );
        assert_eq!(number_after(doc, labels_marker), labels, "jump labels");
    }
}

/// PACK8 / UNPACK8 keep no memory either: a skipped call keeps the outputs, the next call
/// recomputes them from the current inputs.
#[test]
fn pack8_and_unpack8_may_be_called_conditionally() {
    let src = "PROGRAM P\nVAR p : PACK8; u : UNPACK8; w AT D20 : WORD; q AT Y0 : BOOL; END_VAR\n\
IF NOT X1 THEN p(IN0 := X0, IN1 := FALSE, IN2 := FALSE, IN3 := FALSE, IN4 := FALSE, IN5 := FALSE, IN6 := FALSE, IN7 := X0);\n\
u(IN := p.OUT); END_IF;\nw := p.OUT; q := u.Q7;\nEND_PROGRAM";
    let program = plc::compile_st(src).unwrap();
    let mut sim = Simulator::default();
    let set = |sim: &mut Simulator, x: &str, on: bool| sim.memory.set_input(x.parse().unwrap(), on);
    set(&mut sim, "X0", true);
    sim.scan(&program, 100);
    assert_eq!((word(&sim, "D20") as u16, bit(&sim, "Y0")), (0x0081, true));
    // Calls skipped: the outputs keep their values although X0 is now FALSE.
    set(&mut sim, "X1", true);
    set(&mut sim, "X0", false);
    sim.scan(&program, 100);
    assert_eq!((word(&sim, "D20") as u16, bit(&sim, "Y0")), (0x0081, true));
    // Called again: recomputed at once.
    set(&mut sim, "X1", false);
    sim.scan(&program, 100);
    assert_eq!((word(&sim, "D20") as u16, bit(&sim, "Y0")), (0x0000, false));
}
