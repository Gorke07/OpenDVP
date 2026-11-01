//! Deterministic virtual I/O demonstration, not physical/native parity proof.
use opendvp::{
    gps::GpsState,
    plc::{Device, Simulator, compile},
};
use serde_json::json;

fn d(raw: &str) -> Device {
    raw.parse().unwrap()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let program = compile("LD X1\nMOV H81 D1036\nMOV K2000 D1249\nMOV K1 D0\nMOV K100 D10\nMOV K2 D20\nMOV K5 D22\nLD X10\nCSFO X0 D0 D10\nGPWM D20 D22 M0\nGPS K1 D100\nSET M1148\nDELAY K1\nEND").map_err(|error| error.to_string())?;
    let mut sim = Simulator::default();
    sim.memory.gps = GpsState::configured_virtual();
    sim.memory.set_input(d("X1"), true);
    sim.scan(&program, 0);
    sim.memory.set_input(d("X1"), false);
    sim.memory.set_input(d("X10"), true);
    sim.memory.set_system(1312, true);
    for at_us in [1_000, 2_000, 3_000, 4_000] {
        sim.memory
            .timing_io
            .enqueue_input_edge(at_us, d("X0"), true)?;
        sim.memory
            .timing_io
            .enqueue_input_edge(at_us + 100, d("X0"), false)?;
    }
    let body = "GPRMC,123519,A,4807.038,N,01131.000,E,022.4,084.4,230309,003.1,W";
    let checksum = body.bytes().fold(0u8, |value, byte| value ^ byte);
    let frame = format!("${body}*{checksum:02X}\r\n");
    let split = 25;
    sim.memory
        .gps
        .inject_rx(1_500, &frame.as_bytes()[..split])
        .map_err(|error| error.to_string())?;
    sim.memory
        .gps
        .inject_rx(2_500, &frame.as_bytes()[split..])
        .map_err(|error| error.to_string())?;
    let mut scans = Vec::new();
    for _ in 0..10 {
        sim.scan(&program, 1);
        if !sim.memory.execution_limitations.is_empty() {
            return Err(format!("{:?}", sim.memory.execution_limitations).into());
        }
        scans.push(
            json!({"scan": sim.scans, "time_us": sim.memory.timing_io.now_us,
            "gpwm": sim.memory.bit(d("M0")), "input_image_x0": sim.memory.bit(d("X0")),
            "captured_pulses": sim.memory.double_word(d("D2")),
            "input_hz": sim.memory.word(d("D1")), "output_hz": sim.memory.double_word(d("D11")),
            "gps_completed": sim.memory.system_bit(1314)}),
        );
    }
    assert_eq!(sim.memory.timing_io.now_us, 10_050);
    assert_eq!(sim.memory.double_word(d("D2")), 4);
    assert_eq!(sim.memory.word(d("D1")), 1_000);
    assert_eq!(sim.memory.double_word(d("D11")), 1_000);
    assert!(sim.memory.timing_io.pulse_outputs[&0].accumulated_pulses > 0);
    assert!(sim.memory.system_bit(1314));
    let packet = sim
        .memory
        .gps
        .last_packet
        .as_ref()
        .ok_or("No received GPS packet")?;
    let latitude = packet
        .latitude
        .as_ref()
        .ok_or("No latitude")?
        .decimal_degrees;
    let longitude = packet
        .longitude
        .as_ref()
        .ok_or("No longitude")?
        .decimal_degrees;
    assert!((latitude - 48.1173).abs() < 0.00001);
    assert!((longitude - 11.516667).abs() < 0.00001);
    let report = json!({"scope": "Configured virtual COM1 and timestamped pulse model; no native/hardware parity claim",
        "source": "examples/virtual_timing.rs", "frame": frame, "latitude": latitude, "longitude": longitude,
        "scans": scans, "profile_qualifications": sim.memory.timing_io.notices,
        "generated_pulses": sim.memory.timing_io.pulse_outputs[&0].accumulated_pulses});
    let encoded = serde_json::to_string_pretty(&report)?;
    if let Some(path) = std::env::args().nth(1) {
        std::fs::write(path, encoded)?;
    } else {
        println!("{encoded}");
    }
    Ok(())
}
