//! Parent-owned private-desktop diagnostic; canonical invalid D guards only.
use opendvp::{
    es2_bridge::{Session, Target},
    plc::{Device, ScanStatus, Simulator, compile_il},
};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};
const ES2: &str = "5570751341e73b46846f36d99ab476c0f40b9636a8d784d9a3781377e9eb0ea7";
const SS2: &str = "915c96a9ad252cb64773b539290e66af9dcbb4a04af3c860b6d654f7aaad3525";
#[derive(Clone, Serialize)]
struct Literal {
    device: String,
    kind: &'static str,
    value: u32,
}
#[derive(Serialize)]
struct Fixture {
    name: String,
    source: String,
    header: u16,
    error_offset: u16,
    low_only: bool,
    expected: Vec<Literal>,
}
fn d(raw: &str) -> Device {
    raw.parse().expect("authored fixture address")
}
fn fixtures(target: Target) -> Vec<Fixture> {
    let last = if target == Target::Es2 { 9999 } else { 4999 };
    let mut out = Vec::new();
    for (form, name) in ["LRC", "LRCP", "CRC", "CRCP"].into_iter().enumerate() {
        for (role_index, (role, offset)) in [("count", 3), ("source", 1), ("target", 5)]
            .into_iter()
            .enumerate()
        {
            let low = (form + role_index) % 2 == 0;
            let negative = (form + role_index / 2) % 2 == 1;
            let bad = if negative { 0 } else { last };
            let delta = if negative { -1i16 } else { 1 };
            let lrc = name.starts_with("LRC");
            let seeds = vec![
                ("D0".into(), 0x1357),
                (format!("D{last}"), 0x2468),
                ("D9".into(), 0x1111),
                (
                    "D10".into(),
                    match (lrc, low) {
                        (true, false) => 0x3130,
                        (true, true) => 0x30,
                        (false, false) => 0x0201,
                        (false, true) => 1,
                    },
                ),
                (
                    "D11".into(),
                    if low {
                        if lrc { 0x31 } else { 2 }
                    } else {
                        0xaaaa
                    },
                ),
                ("D12".into(), 0x2222),
                ("D19".into(), 0x3333),
                ("D20".into(), 0x4444),
                ("D21".into(), 0x5555),
                ("D22".into(), 0x6666),
                ("D39".into(), 0x7777),
                ("D40".into(), 2),
                ("D41".into(), 0x8888),
                (
                    "D1182".into(),
                    if role == "source" { delta as u16 } else { 0 },
                ),
                (
                    "D1029".into(),
                    if role == "count" { delta as u16 } else { 0 },
                ),
                (
                    "D1194".into(),
                    if role == "target" { delta as u16 } else { 0 },
                ),
            ];
            let mut prefix: Vec<(String, u16)> = vec![("LDI X7".into(), 1)];
            prefix.extend(
                seeds
                    .iter()
                    .map(|(dev, v)| (format!("MOV H{v:04X} {dev}"), 5)),
            );
            prefix.extend([
                ("MOV H2222 D1067".into(), 5),
                ("MOV K77 D1068".into(), 5),
                ("RST M1067".into(), 3),
                ("RST M1068".into(), 3),
                ("SET M1020".into(), 1),
                ("RST M1021".into(), 3),
                ("SET M1022".into(), 1),
                (
                    format!("{} M1161", if low { "SET" } else { "RST" }),
                    if low { 1 } else { 3 },
                ),
            ]);
            // Independent authored sizes: MOV5, SET1, RST3, LDI1; no encoder oracle.
            let header = prefix.iter().map(|(_, words)| *words).sum::<u16>();
            let source = format!(
                "{}\n{name} {} {} {}\nMOV K1234 D300\nEND\n",
                prefix
                    .into_iter()
                    .map(|(line, _)| line)
                    .collect::<Vec<_>>()
                    .join("\n"),
                if role == "source" {
                    format!("D{bad}E1")
                } else {
                    "D10E1".into()
                },
                if role == "count" {
                    format!("D{bad}F0")
                } else {
                    "D40F0".into()
                },
                if role == "target" {
                    format!("D{bad}E7")
                } else {
                    "D20E7".into()
                }
            );
            let mut expected = seeds
                .into_iter()
                .map(|(device, v)| Literal {
                    device,
                    kind: "word",
                    value: u32::from(v),
                })
                .collect::<Vec<_>>();
            for (device, v) in [("D1067", 0xe1a), ("D1068", header + offset), ("D300", 1234)] {
                expected.push(Literal {
                    device: device.into(),
                    kind: "word",
                    value: u32::from(v),
                });
            }
            for (index, on) in [
                (1020, true),
                (1021, false),
                (1022, true),
                (1067, true),
                (1068, true),
                (1161, low),
            ] {
                expected.push(Literal {
                    device: format!("M{index}"),
                    kind: "bit",
                    value: u32::from(on),
                });
            }
            out.push(Fixture {
                name: format!(
                    "checksum_guard_{name}_{role}_{}_{}",
                    if negative { "negative" } else { "high" },
                    if low { "low" } else { "packed" }
                ),
                source,
                header,
                error_offset: offset,
                low_only: low,
                expected,
            });
        }
    }
    out
}
fn local_admission(f: &Fixture, target: Target) -> Result<(), String> {
    let p = compile_il(&f.source).map_err(|e| e.to_string())?;
    let e = opendvp::es2_encoder::encode(&p)?;
    if !p.requires_vm() || e.words.len() >= 0x1ef0 || e.action_pc.last() != Some(&(f.header + 7)) {
        return Err("Fixture layout/marker budget failed".into());
    }
    let header = e.words[usize::from(f.header)];
    if ![0xd000, 0xd010, 0xd200, 0xd210].contains(&header) {
        return Err("Authored header PC disagrees with encoding".into());
    }
    let mut sim = Simulator::default();
    sim.memory.data_register_limit = Some(if target == Target::Es2 { 9999 } else { 4999 });
    sim.control.profile = opendvp::program_control::ControlProfile::InstalledLegacy;
    sim.memory.set_input(d("X7"), false);
    sim.scan(&p, 0);
    if sim.last_scan != ScanStatus::Complete || !sim.memory.execution_limitations.is_empty() {
        return Err("Independent local scan did not complete".into());
    }
    for literal in &f.expected {
        let value = if literal.kind == "bit" {
            u32::from(sim.memory.bit(d(&literal.device)))
        } else {
            u32::from(sim.memory.word(d(&literal.device)) as u16)
        };
        if value != literal.value {
            return Err(format!(
                "{} literal expected{}, local{}",
                literal.device, literal.value, value
            ));
        }
    }
    Ok(())
}
#[cfg(windows)]
fn private_desktop() -> Result<String, String> {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThreadId() -> u32;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetThreadDesktop(id: u32) -> *mut c_void;
        fn GetUserObjectInformationW(
            object: *mut c_void,
            index: i32,
            buffer: *mut c_void,
            length: u32,
            needed: *mut u32,
        ) -> i32;
    }
    let mut buf = [0u16; 512];
    let mut needed = 0;
    let ok = unsafe {
        GetUserObjectInformationW(
            GetThreadDesktop(GetCurrentThreadId()),
            2,
            buf.as_mut_ptr().cast(),
            (buf.len() * 2) as u32,
            &mut needed,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let end = buf
        .iter()
        .position(|v| *v == 0)
        .ok_or("Desktop name unterminated")?;
    let name = String::from_utf16(&buf[..end]).map_err(|e| e.to_string())?;
    if name.is_empty()
        || ["default", "winlogon", "disconnect"].contains(&name.to_ascii_lowercase().as_str())
    {
        return Err("Parent must launch on its owned private desktop".into());
    }
    Ok(name)
}
#[cfg(not(windows))]
fn private_desktop() -> Result<String, String> {
    Err("Windows owned private desktop required".into())
}
fn hash(path: &Path) -> Result<String, std::io::Error> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}
fn append(file: &mut fs::File, value: &Value) -> Result<(), Box<dyn std::error::Error>> {
    serde_json::to_writer(&mut *file, value)?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 || args[2] != "--owned-private-desktop" {
        return Err(
            "usage: checksum_guard_probe SIMULATOR OUTPUT_JSON --owned-private-desktop".into(),
        );
    }
    let desktop = private_desktop()?;
    let simulator = PathBuf::from(&args[0]).canonicalize()?;
    let target_hash = hash(&simulator)?;
    let target = match target_hash.as_str() {
        ES2 => Target::Es2,
        SS2 => Target::Ss2,
        _ => return Err("Only two closed current simulator digests are admitted".into()),
    };
    let output = PathBuf::from(&args[1]);
    let journal_path = output.with_extension("jsonl");
    if output.exists() || journal_path.exists() {
        return Err("Use fresh output and journal paths".into());
    }
    let mut report = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    let mut journal = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(journal_path)?;
    let exe = std::env::current_exe()?;
    let header = json!({"event":"start","target":simulator,"target_sha256":target_hash,"desktop":desktop,
        "producer":exe,"producer_sha256":hash(&exe)?,"probe_sha256":format!("{:x}",Sha256::digest(include_bytes!("checksum_guard_probe.rs"))),
        "scope":"Canonical invalid signed-D resolver guards; one fresh powered scan per case; no invalid payload reads; no simulator modification"});
    append(&mut journal, &header)?;
    let fixtures = fixtures(target);
    let mut results = Vec::new();
    for fixture in &fixtures {
        if let Err(error) = local_admission(fixture, target) {
            results.push(
                json!({"name":fixture.name,"status":"local_admission_failure","error":error}),
            );
        }
    }
    if !results.is_empty() {
        serde_json::to_writer_pretty(
            &mut report,
            &json!({"header":header,"native_started":false,"results":results}),
        )?;
        report.flush()?;
        return Err("Local literal admission failed; native not launched".into());
    }
    for fixture in fixtures {
        let mut stage = "launch";
        let mut snapshot = Value::Null;
        let mut gates = 0;
        let mut steps = 0;
        let mut trace = Value::Null;
        let result = (|| -> Result<(), String> {
            let mut session = Session::launch(&simulator, Arc::new(AtomicBool::new(false)))?;
            stage = "upload";
            session.upload(&fixture.source)?;
            stage = "step";
            let stepped = session.step(&[(d("X7"), false)]);
            gates = session.verified_checksum_gates();
            steps = session.verified_instruction_steps();
            trace = json!(session.trace());
            let s = stepped?;
            snapshot = json!({"scan":s.scan,"rows":s.rows,"mismatches":s.mismatches});
            stage = "comparison";
            if s.mismatches != 0 {
                return Err("Native/local mismatch".into());
            }
            stage = "literal";
            for literal in &fixture.expected {
                let row = s
                    .rows
                    .iter()
                    .find(|r| r.device == d(&literal.device) && r.kind == literal.kind)
                    .ok_or_else(|| format!("Missing literal row {}", literal.device))?;
                if row.local != literal.value || row.original != literal.value {
                    return Err(format!(
                        "{} expected{}, local{}, native{}",
                        literal.device, literal.value, row.local, row.original
                    ));
                }
            }
            stage = "gate";
            if gates != 1 || steps == 0 {
                return Err(format!(
                    "Expected one qualified checksum gate, got{gates}; steps{steps}"
                ));
            }
            Ok(())
        })();
        let status = if result.is_ok() {
            "match"
        } else if ["comparison", "literal"].contains(&stage) {
            "failure"
        } else {
            "inconclusive"
        };
        let record = json!({"name":fixture.name,"status":status,"stage":stage,"error":result.err(),"fixture":fixture,
            "snapshot":snapshot,"verified_checksum_gates":gates,"verified_instruction_steps":steps,"trace":trace});
        println!("{}: {status}", record["name"].as_str().unwrap_or("guard"));
        append(&mut journal, &record)?;
        results.push(record);
    }
    let matched = results.iter().filter(|r| r["status"] == "match").count();
    let failures = results.iter().filter(|r| r["status"] == "failure").count();
    let summary = json!({"event":"summary","cases":results.len(),"matched":matched,"failures":failures,"inconclusive":results.len()-matched-failures});
    append(&mut journal, &summary)?;
    serde_json::to_writer_pretty(
        &mut report,
        &json!({"header":header,"summary":summary,"results":results}),
    )?;
    report.flush()?;
    if matched != 12 {
        return Err("Guard diagnostics did not all match; preserved report/journal".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_twenty_four_local_literals_and_authored_pcs_hold() {
        for model in [Target::Es2, Target::Ss2] {
            let cases = fixtures(model);
            assert_eq!(cases.len(), 12);
            for case in cases {
                local_admission(&case, model).unwrap_or_else(|e| panic!("{}: {e}", case.name));
            }
        }
    }
}
