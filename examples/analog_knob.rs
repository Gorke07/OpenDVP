use opendvp::analog_knob::KnobProfile;
use opendvp::plc::{Simulator, compile_il};

fn main() -> Result<(), String> {
    let program = compile_il("LD M1000\nSET M1178\nVRRD K0 D0\nVRSC K0 D1\nEND")
        .map_err(|e| format!("{e:?}"))?;
    let mut sim = Simulator::default();
    sim.memory.analog_knob.set_profile(KnobProfile::VirtualSa2);
    sim.memory.analog_knob.set_raw(0, 200)?;
    // Independent calibrated graduation; no guessed raw-byte transfer curve.
    sim.memory.analog_knob.set_scaled_milli(0, 3400)?;
    sim.scan(&program, 10);
    assert!(sim.memory.execution_limitations.is_empty());
    let raw = sim.memory.word("D0".parse()?);
    let scaled = sim.memory.word("D1".parse()?);
    let monitor = sim.memory.word("D1178".parse()?);
    assert_eq!((raw, scaled, monitor), (200, 3, 200));
    println!("Sanal VR0: ham={raw}, ölçekli={scaled}, D1178={monitor}");
    Ok(())
}
