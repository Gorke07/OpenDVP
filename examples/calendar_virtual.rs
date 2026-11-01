//! Explicit virtual calendar and enabled-time demonstration, no host clock.
use opendvp::plc::{Device, Simulator, compile};
use serde_json::json;
fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn main() {
    let program = compile(&format!(
        "LD X0\nHOUR K1 D100 M0\n{}TRD D20\nTCMP K0 K0 K0 D24 M10\nEND",
        "DELAY K1000\n".repeat(10)
    ))
    .unwrap();
    let mut simulator = Simulator::default();
    simulator
        .memory
        .rtc_clock
        .configure_virtual([2024, 3, 2, 28, 23, 59, 58])
        .unwrap();
    simulator.memory.set_input(d("X0"), true);
    let mut scans = Vec::new();
    for _ in 0..3 {
        simulator.scan(&program, 0);
        assert!(simulator.memory.execution_limitations.is_empty());
        scans.push(json!({"scan":simulator.scans,"time_us":simulator.memory.timing_io.now_us,
            "calendar":(20..27).map(|i|simulator.memory.word(d(&format!("D{i}")))).collect::<Vec<_>>(),
            "enabled_hours":simulator.memory.word(d("D100")),"enabled_seconds":simulator.memory.word(d("D101")),
            "midnight_equal":simulator.memory.bit(d("M11"))}));
    }
    assert_eq!(scans[0]["calendar"], json!([24, 3, 2, 28, 23, 59, 59]));
    assert_eq!(scans[1]["calendar"], json!([24, 4, 2, 29, 0, 0, 0]));
    assert_eq!(scans[2]["calendar"], json!([24, 4, 2, 29, 0, 0, 1]));
    assert_eq!(scans[2]["enabled_seconds"], 2);
    assert!(scans[1]["midnight_equal"].as_bool().unwrap());
    let result = json!({"profile":"Explicit virtual Gregorian calendar and independent enabled-time HOUR; not native/hardware clock parity","scans":scans});
    let output = serde_json::to_string_pretty(&result).unwrap();
    if let Some(path) = std::env::args().nth(1) {
        std::fs::write(path, output + "\n").unwrap();
    } else {
        println!("{output}");
    }
}
