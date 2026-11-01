//! Read-only paused-target capability observations, not equality validation.
//! Each step executes one complete scan; each 500ms wait executes no STEP RPC
//! and never advances local virtual time. This is M1039-fixed/M1170-paused
//! evidence, not physical PLC STOP or an M1039-OFF host-clock experiment.
use opendvp::es2_bridge::{Comparison, DEFAULT_SIMULATOR, Reading, Session};
use opendvp::plc::{Area, Device};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

#[derive(Serialize)]
struct Sample {
    phase: String,
    operation: &'static str,
    wall_elapsed_us: u128,
    requested_wait_ms: u64,
    scan: u64,
    virtual_now_us: u64,
    verified_instruction_steps: u64,
    /// The last independently checked STEP acknowledgment. observe_snapshot
    /// itself reads PLC data, so this is not a new native-PC measurement.
    last_acknowledged_pc: Option<u16>,
    rows: Vec<Reading>,
}
#[derive(Serialize)]
struct Probe {
    seed: i16,
    source: String,
    samples: Vec<Sample>,
    error: Option<String>,
    trace: Vec<opendvp::es2_bridge::Trace>,
}
#[derive(Serialize)]
struct Report {
    schema_version: u32,
    method: &'static str,
    scope: &'static str,
    producer_executable: String,
    producer_sha256: String,
    target_executable: String,
    target_sha256: String,
    target_model: String,
    probes: Vec<Probe>,
}
fn acknowledged_pc(session: &Session) -> Option<u16> {
    session.trace().iter().rev().find_map(|t| {
        let response = t.response.as_ref()?;
        if t.function == 3
            && t.request == [0x14, 0x92, 0, 1]
            && response.len() == 3
            && response[0] == 2
        {
            Some(u16::from_be_bytes([response[1], response[2]]))
        } else {
            None
        }
    })
}
fn sample(
    session: &Session,
    phase: &str,
    operation: &'static str,
    wait: u64,
    origin: Instant,
    c: Comparison,
) -> Sample {
    Sample {
        phase: phase.into(),
        operation,
        wall_elapsed_us: origin.elapsed().as_micros(),
        requested_wait_ms: wait,
        scan: c.scan,
        virtual_now_us: c.local.memory.timing_io.now_us,
        verified_instruction_steps: session.verified_instruction_steps(),
        last_acknowledged_pc: acknowledged_pc(session),
        rows: c.rows,
    }
}
fn main() -> Result<(), String> {
    let mut simulator = PathBuf::from(DEFAULT_SIMULATOR);
    let mut output = PathBuf::from("system-clocks-runtime-observe.json");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--simulator" => {
                simulator = PathBuf::from(args.next().ok_or("--simulator requires path")?)
            }
            "--output" => output = PathBuf::from(args.next().ok_or("--output requires path")?),
            _ => return Err(format!("Unknown argument {arg}")),
        }
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let digest = format!(
        "{:x}",
        Sha256::digest(std::fs::read(&exe).map_err(|e| e.to_string())?)
    );
    let mut report = Report {
        schema_version: 1,
        method: "original_paused_clock_capability_observation",
        scope: "M1039 fixed 100ms; M1170 instruction stepping; host wait is an observation interval, never a local PLC elapsed-time input; no equality/100us support claim",
        producer_executable: exe.display().to_string(),
        producer_sha256: digest,
        target_executable: simulator.display().to_string(),
        target_sha256: String::new(),
        target_model: String::new(),
        probes: Vec::new(),
    };
    for seed in [123, 32760] {
        let source = format!(
            "LD M1002\nMOV K{seed} D1015\nLD X0\nOUT M1015\nLD M1000\nMOV D1015 D0\nINC D10\nEND"
        );
        let mut p = Probe {
            seed,
            source: source.clone(),
            samples: Vec::new(),
            error: None,
            trace: Vec::new(),
        };
        let mut session = Session::launch(Path::new(&simulator), Arc::new(AtomicBool::new(false)))?;
        report.target_sha256 = session.sha256().into();
        report.target_model = session.target().name().into();
        let origin = Instant::now();
        let result = (|| -> Result<(), String> {
            let c = session.upload(&source)?;
            p.samples
                .push(sample(&session, "upload", "upload", 0, origin, c));
            for (phase, enabled) in [("enabled", true), ("disabled", false), ("re-enabled", true)] {
                let c = session.step(&[(
                    Device {
                        area: Area::X,
                        index: 0,
                    },
                    enabled,
                )])?;
                let scan = c.scan;
                let virtual_us = c.local.memory.timing_io.now_us;
                let steps = session.verified_instruction_steps();
                p.samples
                    .push(sample(&session, phase, "step_one_END", 0, origin, c));
                std::thread::sleep(Duration::from_millis(500));
                let c = session.observe_snapshot()?;
                if c.scan != scan
                    || c.local.memory.timing_io.now_us != virtual_us
                    || session.verified_instruction_steps() != steps
                {
                    return Err("Paused observation advanced local scan/time or acknowledged instruction count".into());
                }
                p.samples.push(sample(
                    &session,
                    phase,
                    "observe_without_execution",
                    500,
                    origin,
                    c,
                ));
            }
            Ok(())
        })();
        if let Err(e) = result {
            p.error = Some(e);
        }
        p.trace = session.trace().to_vec();
        report.probes.push(p);
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        &output,
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!("Clock capability observations: {}", output.display());
    if report.probes.iter().any(|p| p.error.is_some()) {
        return Err("One or more capability observations failed; inspect report".into());
    }
    Ok(())
}
