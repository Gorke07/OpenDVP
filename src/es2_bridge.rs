//! An isolated original ES2 process and a scan-synchronous differential runner.
//! All simulator writes target a process launched and owned by this module.
use crate::{
    additional_processing::CompareTableProfile,
    es2_encoder::{self, EncodedProgram},
    es2_protocol::Client,
    plc::{self, Area, Device, Program, Simulator},
};
use futures::channel::oneshot;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    net::{Ipv4Addr, TcpListener},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
#[path = "es2_bridge_bits.rs"]
mod bit_snapshots;
#[path = "es2_bridge_random_gate.rs"]
mod random_gate;
#[path = "es2_bridge_rtc_observation.rs"]
mod rtc_observation;
pub use rtc_observation::{RtcDecoderObservation, RtcNativeObservation};
#[path = "es2_bridge_rtc_comparison.rs"]
mod rtc_comparison;
pub use rtc_comparison::{RtcComparisonBudgets, RtcComparisonEvidence};

#[cfg(test)]
#[path = "ss2_program_transfer_tests.rs"]
mod ss2_program_transfer_tests;

#[cfg(test)]
#[path = "ss2_online_tests.rs"]
mod ss2_online_tests;

pub const DEFAULT_SIMULATOR: &str = "C:\\Program Files (x86)\\Delta Industrial Automation\\DIAStudio\\DIATools\\COMMGR 2.11\\Simulator\\DVPSimulator_ES2.exe";
pub const TARGET_SHA256: &str = "1f8974f13caad54334b5ad4c0402d8c1026a4dec29a7f61c19136e76df028515";
pub const DEFAULT_SS2_SIMULATOR: &str = "C:\\Program Files (x86)\\Delta Industrial Automation\\DIAStudio\\DIATools\\COMMGR 2.11\\Simulator\\DVPSimulator_SS2.exe";
pub const SS2_SHA256: &str = "7557a79d80617cba950eef702cd77fda45f9d3138ef82cad682bc7587f5347f4";
pub const COMMGR_ES2_SHA256: &str =
    "5570751341e73b46846f36d99ab476c0f40b9636a8d784d9a3781377e9eb0ea7";
pub const COMMGR_SS2_SHA256: &str =
    "915c96a9ad252cb64773b539290e66af9dcbb4a04af3c860b6d654f7aaad3525";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Es2,
    Ss2,
}
impl Target {
    pub fn name(self) -> &'static str {
        match self {
            Self::Es2 => "ES2",
            Self::Ss2 => "SS2",
        }
    }
    /// Word limits of the installed simulator binaries, not physical PLCs.
    fn program_word_limit(self) -> usize {
        match self {
            Self::Es2 => es2_encoder::MAX_PROGRAM_WORDS,
            Self::Ss2 => 0x1ef0,
        }
    }
    fn validate_program_word_count(self, count: usize) -> Result<(), String> {
        if count == 0 || count > self.program_word_limit() {
            return Err(format!(
                "{} simülatörü en fazla {} program WORD destekler",
                self.name(),
                self.program_word_limit()
            ));
        }
        Ok(())
    }
    fn from_image(digest: &str, length: u64) -> Result<Self, String> {
        match (digest, length) {
            (TARGET_SHA256, 1_715_200) | (COMMGR_ES2_SHA256, 1_775_528) => Ok(Self::Es2),
            (SS2_SHA256, 1_715_200) | (COMMGR_SS2_SHA256, 1_735_080) => Ok(Self::Ss2),
            _ => Err(format!(
                "Unsupported ES2/SS2 binary (size {length}, SHA-256 {digest}); expected a verified Delta editor or COMMGR 2.11 simulator"
            )),
        }
    }
    fn validate_program(self, program: &Program) -> Result<(), String> {
        if self == Self::Ss2
            && let Some(d) = program
                .devices
                .iter()
                .find(|d| d.area == Area::D && d.index >= 5000)
        {
            return Err(format!(
                "SS2 does not support {d}; verified D range is D0..D4999"
            ));
        }
        Ok(())
    }
}

fn compare_table_profile(digest: &str) -> Result<CompareTableProfile, String> {
    match digest {
        COMMGR_ES2_SHA256 => Ok(CompareTableProfile::CurrentEs2),
        COMMGR_SS2_SHA256 => Ok(CompareTableProfile::CurrentSs2),
        TARGET_SHA256 | SS2_SHA256 => Ok(CompareTableProfile::InstalledLegacy),
        _ => Err("CMPT/DCMPT behavior is not verified for this simulator binary".into()),
    }
}

fn decimal_scratch_profile(
    digest: &str,
) -> Result<crate::float_math::DecimalScratchProfile, String> {
    use crate::float_math::DecimalScratchProfile;
    match digest {
        COMMGR_ES2_SHA256 => Ok(DecimalScratchProfile::CurrentPrivate),
        TARGET_SHA256 | SS2_SHA256 | COMMGR_SS2_SHA256 => Ok(DecimalScratchProfile::LegacyVisible),
        _ => Err("DEBCD scratch behavior is not verified for this simulator binary".into()),
    }
}

fn random_tail_profile(digest: &str) -> Result<crate::random_tail::RandomTailProfile, String> {
    use crate::random_tail::RandomTailProfile;
    match digest {
        COMMGR_ES2_SHA256 | COMMGR_SS2_SHA256 => Ok(RandomTailProfile::CurrentCommgr),
        TARGET_SHA256 | SS2_SHA256 => Ok(RandomTailProfile::InstalledLegacyInertOnly),
        _ => Err("DRAND tail behavior is unverified for this simulator image".into()),
    }
}

fn calendar_tail_profile(digest: &str) -> crate::calendar_tail::CalendarTailProfile {
    match digest {
        COMMGR_ES2_SHA256 | COMMGR_SS2_SHA256 => {
            crate::calendar_tail::CalendarTailProfile::CurrentCommgr
        }
        _ => crate::calendar_tail::CalendarTailProfile::UnqualifiedNative,
    }
}

fn validate_segment_native_profile(program: &Program, digest: &str) -> Result<(), String> {
    if !matches!(digest, COMMGR_ES2_SHA256 | COMMGR_SS2_SHA256)
        && program.networks.iter().flat_map(|n| &n.actions).any(|a| {
            matches!(
                if let plc::Action::Pulse(inner) = a {
                    inner.as_ref()
                } else {
                    a
                },
                plc::Action::IndexedSegment(_)
            )
        })
    {
        return Err(
            "Indexed SEGD source/header qualification is unavailable for this native image".into(),
        );
    }
    Ok(())
}

fn validate_additional_native_profile(program: &Program, digest: &str) -> Result<(), String> {
    if !matches!(digest, COMMGR_ES2_SHA256 | COMMGR_SS2_SHA256)
        && program
            .networks
            .iter()
            .flat_map(|n| &n.actions)
            .any(crate::indexed_additional::is_instruction)
    {
        return Err(
            "WSUM/SCAL/SCLP/CMPT ordered qualification unavailable for this native image".into(),
        );
    }
    Ok(())
}
fn validate_text_native_profile(program: &Program, digest: &str) -> Result<(), String> {
    if !matches!(digest, COMMGR_ES2_SHA256 | COMMGR_SS2_SHA256)
        && program
            .networks
            .iter()
            .flat_map(|n| &n.actions)
            .any(crate::indexed_text::is_instruction)
    {
        return Err(
            "ASCI/HEX/CCD ordered cursor qualification is unavailable for this native image".into(),
        );
    }
    Ok(())
}
fn validate_checksum_native_profile(program: &Program, digest: &str) -> Result<(), String> {
    if !matches!(digest, COMMGR_ES2_SHA256 | COMMGR_SS2_SHA256)
        && program.networks.iter().flat_map(|n| &n.actions).any(|a| {
            matches!(
                if let plc::Action::Pulse(inner) = a {
                    inner.as_ref()
                } else {
                    a
                },
                plc::Action::IndexedChecksum(_)
            )
        })
    {
        return Err(
            "LRC/CRC ordered source/header qualification is unavailable for this native image"
                .into(),
        );
    }
    Ok(())
}
fn transform_profile(
    digest: &str,
) -> Result<crate::data_transform::NativeTransformProfile, String> {
    use crate::data_transform::NativeTransformProfile;
    match digest {
        COMMGR_ES2_SHA256 => Ok(NativeTransformProfile::CurrentEs2),
        TARGET_SHA256 | SS2_SHA256 | COMMGR_SS2_SHA256 => {
            Ok(NativeTransformProfile::InstalledLegacy)
        }
        _ => Err("Unknown simulator DTM profile".into()),
    }
}

// These addresses belong to exact older Delta editor binaries, not to a model in
// general. Other digests must never access process memory through this path.
fn teaching_tick_address(digest: &str) -> Result<usize, String> {
    match digest {
        TARGET_SHA256 => Ok(0x53acd8),
        SS2_SHA256 => Ok(0x5347a8),
        _ => Err("Teaching tick replay is not verified for this simulator binary".into()),
    }
}
// The original's fixed-scan remainder code is defective for fractional 100 ms ticks.
// An exact 100 ms quantum avoids that path and is verified by the reference suite.
pub const SCAN_MS: u64 = 100;
const RUN: u16 = 0x0c30;
const COMMIT: u16 = 0x0c31;
const STEP_MODE: u16 = 0x0c92;
const STEP: u16 = 0x0c93;
const PC: u16 = 0x1492;

#[derive(Debug, Clone, Serialize)]
pub struct Reading {
    #[serde(serialize_with = "device_string")]
    pub device: Device,
    pub kind: &'static str,
    pub local: u32,
    pub original: u32,
}
fn device_string<S: serde::Serializer>(device: &Device, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&device.to_string())
}
#[derive(Debug, Clone)]
pub struct Comparison {
    pub local: Simulator,
    pub scan: u64,
    pub rows: Vec<Reading>,
    pub mismatches: usize,
}
/// One value read from the running owned simulator. Reads of different blocks
/// may cross scan boundaries; no local model or single-scan guarantee applies.
#[derive(Debug, Clone)]
pub struct OnlineReading {
    pub device: Device,
    pub kind: &'static str,
    pub value: u32,
}
#[derive(Debug, Clone)]
pub struct OnlineSnapshot {
    /// Number of completed observations in this session, not a PLC scan count.
    pub sequence: u64,
    /// Earliest possible time of a value in this sequential observation.
    /// Captured on the worker before reading any data, so queue or UI delivery
    /// delays cannot make an old sample appear freshly observed.
    pub observed_at: Instant,
    /// RUN coil as read from the owned simulator after the device values.
    pub running: bool,
    pub rows: Vec<OnlineReading>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Trace {
    pub function: u8,
    pub request: Vec<u8>,
    pub response: Option<Vec<u8>>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TeachingTickInput {
    pub owned_pid: u32,
    pub boundary_pc: u16,
    pub address: usize,
    pub before: u8,
    pub supplied: u8,
    pub readback: u8,
}

/// One input observation before any program opcode executes. Native startup
/// clock levels may inherit host phase although fixed END counters start at 0.
#[derive(Debug, Clone, Serialize)]
pub struct ClockStartupInput {
    pub owned_pid: u32,
    pub boundary_pc: u16,
    pub devices: Vec<String>,
    pub protocol_addresses: Vec<u16>,
    pub levels: [bool; 4],
    pub method: String,
}

type Reply = oneshot::Sender<Result<Comparison, String>>;

/// A valid transport can contain an unsupported program; it must not be
/// conflated with a failed or retired simulator connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgramDiscoveryError {
    Transport(String),
    Decode(String),
}
impl std::fmt::Display for ProgramDiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(error) | Self::Decode(error) => f.write_str(error),
        }
    }
}
impl std::error::Error for ProgramDiscoveryError {}

enum Job {
    Upload(String, Reply),
    Step(Vec<(Device, bool)>, Reply),
    OnlineStart(oneshot::Sender<Result<OnlineSnapshot, String>>),
    OnlinePoll(
        Vec<(Device, bool)>,
        oneshot::Sender<Result<OnlineSnapshot, String>>,
    ),
    OnlineStop(oneshot::Sender<Result<OnlineSnapshot, String>>),
    ReadProgram(oneshot::Sender<Result<Vec<u16>, String>>),
    DiscoverProgram(oneshot::Sender<Result<Vec<u16>, ProgramDiscoveryError>>),
    Trace(oneshot::Sender<Vec<Trace>>),
    Shutdown,
}
#[derive(Debug, Clone)]
pub struct Bridge {
    tx: mpsc::Sender<Job>,
    cancel: Arc<AtomicBool>,
    port: u16,
    supports_online: bool,
}
impl Bridge {
    pub fn port(&self) -> u16 {
        self.port
    }
    pub fn supports_online(&self) -> bool {
        self.supports_online
    }
    pub fn disconnect(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        let _ = self.tx.send(Job::Shutdown);
    }
    pub async fn upload(&self, source: String) -> Result<Comparison, String> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Job::Upload(source, tx))
            .map_err(|_| "ES2 worker closed".to_owned())?;
        rx.await.map_err(|_| "ES2 upload interrupted".to_owned())?
    }
    pub async fn step(&self, inputs: Vec<(Device, bool)>) -> Result<Comparison, String> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Job::Step(inputs, tx))
            .map_err(|_| "ES2 worker closed".to_owned())?;
        rx.await.map_err(|_| "ES2 step interrupted".to_owned())?
    }
    pub async fn start_online(&self) -> Result<OnlineSnapshot, String> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Job::OnlineStart(tx))
            .map_err(|_| "SS2 worker closed".to_owned())?;
        rx.await
            .map_err(|_| "SS2 online start interrupted".to_owned())?
    }
    pub async fn poll_online(&self, inputs: Vec<(Device, bool)>) -> Result<OnlineSnapshot, String> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Job::OnlinePoll(inputs, tx))
            .map_err(|_| "SS2 worker closed".to_owned())?;
        rx.await
            .map_err(|_| "SS2 online read interrupted".to_owned())?
    }
    pub async fn stop_online(&self) -> Result<OnlineSnapshot, String> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Job::OnlineStop(tx))
            .map_err(|_| "SS2 worker closed".to_owned())?;
        rx.await
            .map_err(|_| "SS2 online stop interrupted".to_owned())?
    }
    /// Read the exact word count of the last successfully uploaded program
    /// from this owned simulator. This does not discover an arbitrary PLC image.
    pub async fn read_program_words(&self) -> Result<Vec<u16>, String> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Job::ReadProgram(tx))
            .map_err(|_| "ES2 worker closed".to_owned())?;
        rx.await
            .map_err(|_| "ES2 program read interrupted".to_owned())?
    }
    /// Read a supported complete simulator program without assuming a known
    /// word count. The decoder must establish END at an instruction boundary.
    pub async fn discover_program_words(&self) -> Result<Vec<u16>, ProgramDiscoveryError> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Job::DiscoverProgram(tx))
            .map_err(|_| ProgramDiscoveryError::Transport("ES2 worker closed".into()))?;
        rx.await
            .map_err(|_| ProgramDiscoveryError::Transport("ES2 program read interrupted".into()))?
    }
    pub async fn trace(&self) -> Vec<Trace> {
        let (tx, rx) = oneshot::channel();
        if self.tx.send(Job::Trace(tx)).is_err() {
            return Vec::new();
        }
        rx.await.unwrap_or_default()
    }
}

/// Start a private, digest-verified copy of the installed original, on an ephemeral port.
pub async fn launch(path: String) -> Result<Bridge, String> {
    let (tx, rx) = mpsc::channel();
    let (ready_tx, ready_rx) = oneshot::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = cancel.clone();
    thread::Builder::new()
        .name("opendvp-es2".into())
        .spawn(move || {
            let mut session = match Session::launch(Path::new(&path), worker_cancel) {
                Ok(s) => s,
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            if ready_tx
                .send(Ok((session.port, session.digest == COMMGR_SS2_SHA256)))
                .is_err()
            {
                return;
            }
            while let Ok(job) = rx.recv() {
                if session.cancel.load(Ordering::Relaxed) {
                    break;
                }
                match job {
                    Job::Upload(source, reply) => {
                        let result = session.upload(&source);
                        let _ = reply.send(result);
                    }
                    Job::Step(inputs, reply) => {
                        let result = session.step(&inputs);
                        let _ = reply.send(result);
                    }
                    Job::OnlineStart(reply) => {
                        let _ = reply.send(session.start_online());
                    }
                    Job::OnlinePoll(inputs, reply) => {
                        let _ = reply.send(session.poll_online(&inputs));
                    }
                    Job::OnlineStop(reply) => {
                        let _ = reply.send(session.stop_online());
                    }
                    Job::ReadProgram(reply) => {
                        let result = session.read_program_words();
                        let _ = reply.send(result);
                    }
                    Job::DiscoverProgram(reply) => {
                        let result = session.discover_program_words();
                        let _ = reply.send(result);
                    }
                    Job::Trace(reply) => {
                        let _ = reply.send(session.trace.clone());
                    }
                    Job::Shutdown => break,
                }
            }
        })
        .map_err(|e| e.to_string())?;
    let (port, supports_online) = ready_rx
        .await
        .map_err(|_| "ES2 worker failed during startup".to_owned())??;
    Ok(Bridge {
        tx,
        cancel,
        port,
        supports_online,
    })
}

struct OwnedNative {
    _child: ChildGuard,
    client: Client,
}
struct ChildGuard(Option<Child>);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
impl OwnedNative {
    fn start(path: &Path, port: u16, cancel: &AtomicBool) -> Result<Self, String> {
        let mut command = Command::new(path);
        command
            .arg(format!("OpenDVP;{port};"))
            .current_dir(path.parent().ok_or("Simulator directory unavailable")?)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let child = command
            .spawn()
            .map_err(|e| format!("ES2 simulator could not start: {e}"))?;
        let mut child = ChildGuard(Some(child));
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if cancel.load(Ordering::Relaxed) || Instant::now() > deadline {
                return Err("ES2 startup cancelled or timed out".into());
            }
            if let Some(exit) = child
                .0
                .as_mut()
                .unwrap()
                .try_wait()
                .map_err(|e| e.to_string())?
            {
                return Err(format!("ES2 exited during startup: {exit}"));
            }
            if owns_listener(child.0.as_ref().unwrap().id(), port)?
                && let Ok(client) = Client::connect(port, 1)
            {
                return Ok(Self {
                    _child: child,
                    client,
                });
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
}

#[cfg(windows)]
fn owns_listener(pid: u32, port: u16) -> Result<bool, String> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCPROW_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER,
    };
    // The API writes a DWORD count followed by a variable array of DWORD-aligned rows.
    let mut size = 0u32;
    unsafe {
        GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut size,
            0,
            2,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        );
    }
    if size < 4 {
        return Ok(false);
    }
    let mut storage = vec![0u32; (size as usize).div_ceil(4)];
    let error = unsafe {
        GetExtendedTcpTable(
            storage.as_mut_ptr().cast(),
            &mut size,
            0,
            2,
            TCP_TABLE_OWNER_PID_LISTENER,
            0,
        )
    };
    if error == 122 {
        return Ok(false);
    }
    if error != 0 {
        return Err(format!("Cannot verify ES2 TCP listener owner: {error}"));
    }
    let count = storage[0] as usize;
    let row_size = std::mem::size_of::<MIB_TCPROW_OWNER_PID>();
    if 4 + count * row_size > storage.len() * 4 {
        return Err("Invalid TCP owner table length".into());
    }
    for i in 0..count {
        let row = unsafe {
            std::ptr::read_unaligned(
                storage
                    .as_ptr()
                    .cast::<u8>()
                    .add(4 + i * row_size)
                    .cast::<MIB_TCPROW_OWNER_PID>(),
            )
        };
        if row.dwOwningPid == pid && u16::from_be(row.dwLocalPort as u16) == port {
            return Ok(true);
        }
    }
    Ok(false)
}
#[cfg(not(windows))]
fn owns_listener(_: u32, _: u16) -> Result<bool, String> {
    Err("The original ES2 simulator requires Windows".into())
}

pub struct Session {
    target: Target,
    digest: String,
    path: PathBuf,
    port: u16,
    native: Option<OwnedNative>,
    cancel: Arc<AtomicBool>,
    program: Option<Program>,
    encoded: Option<EncodedProgram>,
    local: Simulator,
    trace: Vec<Trace>,
    faulted: bool,
    online_active: bool,
    online_tainted: bool,
    online_sequence: u64,
    verified_steps: u64,
    verified_random_gates: u64,
    verified_calendar_gates: u64,
    verified_rtc_gates: u64,
    verified_segment_gates: u64,
    verified_checksum_gates: u64,
    verified_text_gates: u64,
    verified_additional_gates: u64,
    teaching_tick_replay: Option<u8>,
    teaching_tick_inputs: Vec<TeachingTickInput>,
    clock_startup_input: Option<ClockStartupInput>,
    rtc_comparison: Option<rtc_comparison::RtcSessionState>,
    rtc_mode_active: bool,
    rtc_decoder_diagnostics: bool,
    rtc_decoder_expected: Option<rtc_comparison::RtcDecoderExpectation>,
}
impl Session {
    /// Synchronous counterpart for CLI/fixtures; UI uses `launch` and its worker.
    pub fn launch(path: &Path, cancel: Arc<AtomicBool>) -> Result<Self, String> {
        let path = path
            .canonicalize()
            .map_err(|e| format!("ES2 executable not found: {e}"))?;
        let length = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
        if ![1_715_200, 1_775_528, 1_735_080].contains(&length) {
            return Err("Unsupported ES2/SS2 binary size; expected a verified Delta editor or COMMGR 2.11 simulator".into());
        }
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let digest = format!("{:x}", Sha256::digest(&bytes));
        let target = Target::from_image(&digest, bytes.len() as u64)?;
        let comparison_profile = compare_table_profile(&digest)?;
        let decimal_profile = decimal_scratch_profile(&digest)?;
        let random_profile = random_tail_profile(&digest)?;
        let calendar_profile = calendar_tail_profile(&digest);
        let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .map_err(|e| e.to_string())?
            .local_addr()
            .map_err(|e| e.to_string())?
            .port();
        let native = OwnedNative::start(&path, port, &cancel)?;
        let mut local = Simulator::default();
        local.memory.compare_table_profile = comparison_profile;
        local.memory.decimal_scratch_profile = decimal_profile;
        local.memory.random_tail_profile = random_profile;
        local.memory.calendar_tail_profile = calendar_profile;
        local.memory.data_register_limit = Some(if target == Target::Ss2 { 4999 } else { 9999 });
        Ok(Self {
            target,
            digest,
            path,
            port,
            native: Some(native),
            cancel,
            program: None,
            encoded: None,
            local,
            trace: Vec::new(),
            faulted: false,
            online_active: false,
            online_tainted: false,
            online_sequence: 0,
            verified_steps: 0,
            verified_random_gates: 0,
            verified_calendar_gates: 0,
            verified_rtc_gates: 0,
            verified_segment_gates: 0,
            verified_checksum_gates: 0,
            verified_text_gates: 0,
            verified_additional_gates: 0,
            teaching_tick_replay: None,
            teaching_tick_inputs: Vec::new(),
            clock_startup_input: None,
            rtc_comparison: None,
            rtc_mode_active: false,
            rtc_decoder_diagnostics: false,
            rtc_decoder_expected: None,
        })
    }
    pub fn port(&self) -> u16 {
        self.port
    }
    pub fn target(&self) -> Target {
        self.target
    }
    /// Exact identity of the executable checked when this session was opened.
    pub fn sha256(&self) -> &str {
        &self.digest
    }
    pub fn trace(&self) -> &[Trace] {
        &self.trace
    }
    /// Acknowledged instructions whose returned PC passed independent checking.
    /// Unlike the bounded trace buffer, this counter covers the entire upload.
    pub fn verified_instruction_steps(&self) -> u64 {
        self.verified_steps
    }
    pub fn verified_calendar_gates(&self) -> u64 {
        self.verified_calendar_gates
    }
    pub fn verified_rtc_gates(&self) -> u64 {
        self.verified_rtc_gates
    }
    pub fn verified_segment_gates(&self) -> u64 {
        self.verified_segment_gates
    }
    pub fn verified_checksum_gates(&self) -> u64 {
        self.verified_checksum_gates
    }
    pub fn verified_text_gates(&self) -> u64 {
        self.verified_text_gates
    }
    pub fn verified_additional_gates(&self) -> u64 {
        self.verified_additional_gates
    }
    pub fn verified_random_gates(&self) -> u64 {
        self.verified_random_gates
    }
    /// Explicit input replay at the teaching-timer scheduler boundary. This
    /// writes only the owned oracle's pending tick input, never its results.
    /// It establishes controlled algorithm parity, not natural host-time parity.
    pub fn set_teaching_tick_replay(&mut self, ticks: Option<u8>) -> Result<(), String> {
        if ticks.is_some() {
            teaching_tick_address(&self.digest)?;
        }
        self.teaching_tick_replay = ticks;
        Ok(())
    }
    pub fn teaching_tick_inputs(&self) -> &[TeachingTickInput] {
        &self.teaching_tick_inputs
    }
    pub fn clock_startup_input(&self) -> Option<&ClockStartupInput> {
        self.clock_startup_input.as_ref()
    }

    #[cfg(windows)]
    fn replay_teaching_ticks(&mut self, ticks: u8) -> Result<(), String> {
        let address = teaching_tick_address(&self.digest)?;
        use std::ffi::c_void;
        use std::os::windows::io::AsRawHandle;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn ReadProcessMemory(
                process: *mut c_void,
                address: *const c_void,
                buffer: *mut c_void,
                size: usize,
                read: *mut usize,
            ) -> i32;
            fn WriteProcessMemory(
                process: *mut c_void,
                address: *mut c_void,
                buffer: *const c_void,
                size: usize,
                written: *mut usize,
            ) -> i32;
        }
        if self.word_read(0x140f)? != SCAN_MS as u16
            || !self.bit_read(plc::Memory::system_device(1039))?
        {
            return Err(
                "Teaching replay requires unchanged M1039/D1039 fixed scan controls".into(),
            );
        }
        let boundary_pc = self.word_read(PC)?;
        let native = self.native.as_mut().ok_or("No owned native process")?;
        let child = native
            ._child
            .0
            .as_mut()
            .ok_or("Owned process handle unavailable")?;
        if child.try_wait().map_err(|e| e.to_string())?.is_some()
            || !owns_listener(child.id(), self.port)?
        {
            return Err("Teaching replay requires the live owned verified listener".into());
        }
        let mut before = 0u8;
        let mut transferred = 0usize;
        let handle = child.as_raw_handle();
        // Digest checked on launch; the original is instruction-stepped with
        // M1039 fixed scan, which disables asynchronous teaching tick updates.
        let ok = unsafe {
            ReadProcessMemory(
                handle,
                address as *const c_void,
                (&mut before as *mut u8).cast(),
                1,
                &mut transferred,
            )
        };
        if ok == 0 || transferred != 1 || before != 0 {
            return Err(format!(
                "Teaching tick boundary was not clean: read={ok}, bytes={transferred}, pending={before}"
            ));
        }
        let ok = unsafe {
            WriteProcessMemory(
                handle,
                address as *mut c_void,
                (&ticks as *const u8).cast(),
                1,
                &mut transferred,
            )
        };
        if ok == 0 || transferred != 1 {
            return Err("Owned teaching tick input write failed".into());
        }
        let mut after = 0u8;
        let ok = unsafe {
            ReadProcessMemory(
                handle,
                address as *const c_void,
                (&mut after as *mut u8).cast(),
                1,
                &mut transferred,
            )
        };
        if ok == 0 || transferred != 1 || after != ticks {
            return Err("Owned teaching tick input readback failed".into());
        }
        self.teaching_tick_inputs.push(TeachingTickInput {
            owned_pid: child.id(),
            boundary_pc,
            address,
            before,
            supplied: ticks,
            readback: after,
        });
        self.local
            .memory
            .special_timers
            .supply_teaching_ticks(ticks);
        Ok(())
    }
    #[cfg(not(windows))]
    fn replay_teaching_ticks(&mut self, _: u8) -> Result<(), String> {
        Err("Owned teaching replay requires Windows".into())
    }
    /// External private RTC input. Background native time can advance between
    /// calls even in STOP mode; callers must retain their observation boundary.
    pub fn read_rtc_shadow(&mut self) -> Result<[u8; 7], String> {
        let data = self.request(0x64, &[0, 0x1d, 0])?;
        let result = crate::es2_protocol::decode_rtc_shadow(&data);
        if result.is_err() {
            self.faulted = true;
        }
        result
    }
    /// Seed only this owned simulator, without changing the local model or host
    /// clock. A matching readback proves the write boundary, not a frozen clock.
    pub fn write_rtc_shadow(&mut self, fields: [u8; 7]) -> Result<(), String> {
        if self.rtc_mode_active || self.rtc_comparison.is_some() {
            return Err("RTC comparison forbids reseeding after its one-time initial input".into());
        }
        let data = crate::es2_protocol::rtc_seed_payload(fields);
        if self.request(0x64, &data)? != data {
            self.faulted = true;
            return Err("ES2 RTC seed echo mismatch".into());
        }
        Ok(())
    }
    fn request(&mut self, function: u8, data: &[u8]) -> Result<Vec<u8>, String> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err("ES2 operation cancelled".into());
        }
        let result = self
            .native
            .as_mut()
            .ok_or("ES2 not connected")?
            .client
            .request(function, data);
        // Trace is bounded independently from the lifetime of the process.
        if self.trace.len() >= 20000 {
            self.trace.drain(..10000);
        }
        self.trace.push(Trace {
            function,
            request: data.to_vec(),
            response: result.as_ref().ok().cloned(),
            error: result.as_ref().err().cloned(),
        });
        result
    }
    fn bit_write(&mut self, address: u16, value: bool) -> Result<(), String> {
        let mut data = address.to_be_bytes().to_vec();
        data.extend_from_slice(if value { &[0xff, 0] } else { &[0, 0] });
        let echo = self.request(5, &data)?;
        if echo != data {
            return Err("ES2 bit-write echo mismatch".into());
        }
        Ok(())
    }
    fn word_write(&mut self, address: u16, value: u16) -> Result<(), String> {
        let mut data = address.to_be_bytes().to_vec();
        data.extend_from_slice(&value.to_be_bytes());
        if self.request(6, &data)? != data {
            return Err("ES2 word-write echo mismatch".into());
        }
        Ok(())
    }
    fn word_read(&mut self, address: u16) -> Result<u16, String> {
        Ok(self.words_read(address, 1)?[0])
    }
    fn words_read(&mut self, address: u16, count: u16) -> Result<Vec<u16>, String> {
        if !(1..=100).contains(&count) {
            return Err("Preflight register read count must be 1..100".into());
        }
        let mut data = address.to_be_bytes().to_vec();
        data.extend_from_slice(&count.to_be_bytes());
        let out = self.request(3, &data)?;
        if out.len() != usize::from(count) * 2 + 1 || usize::from(out[0]) != usize::from(count) * 2
        {
            return Err("ES2 register-read byte count mismatch".into());
        }
        Ok(out[1..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|v| u16::from_be_bytes([v[0], v[1]]))
            .collect())
    }
    fn bit_read(&mut self, d: Device) -> Result<bool, String> {
        let address = bit_address(d)?;
        let mut data = address.to_be_bytes().to_vec();
        data.extend_from_slice(&[0, 1]);
        let out = self.request(if d.area == Area::X { 2 } else { 1 }, &data)?;
        if out.len() != 2 || out[0] != 1 {
            return Err("ES2 bit-read byte count mismatch".into());
        }
        Ok(out[1] & 1 != 0)
    }
    pub fn upload(&mut self, source: &str) -> Result<Comparison, String> {
        let program = plc::compile(source).map_err(|e| e.to_string())?;
        self.target.validate_program(&program)?;
        let encoded =
            es2_encoder::encode_for_transform_profile(&program, transform_profile(&self.digest)?)?;
        // Check before discarding the existing session or restarting its owned
        // simulator. The existing connection may already own a running process.
        self.target
            .validate_program_word_count(encoded.words.len())?;
        let comparison_profile = compare_table_profile(&self.digest)?;
        let decimal_profile = decimal_scratch_profile(&self.digest)?;
        let random_profile = random_tail_profile(&self.digest)?;
        let calendar_profile = calendar_tail_profile(&self.digest);
        crate::random_tail::validate_program_profile(&program, random_profile)?;
        crate::calendar_tail::validate_program_profile(&program, calendar_profile)?;
        validate_segment_native_profile(&program, &self.digest)?;
        validate_checksum_native_profile(&program, &self.digest)?;
        validate_text_native_profile(&program, &self.digest)?;
        validate_additional_native_profile(&program, &self.digest)?;
        self.verified_steps = 0;
        self.verified_random_gates = 0;
        self.verified_calendar_gates = 0;
        self.verified_rtc_gates = 0;
        self.verified_segment_gates = 0;
        self.verified_checksum_gates = 0;
        self.verified_text_gates = 0;
        self.verified_additional_gates = 0;
        // Cold restart establishes identical zeroed state, including hidden edge history.
        self.online_active = false;
        self.online_tainted = false;
        self.online_sequence = 0;
        self.native.take();
        self.program = None;
        self.encoded = None;
        self.local = Simulator::default();
        crate::power_monitor::PowerState::set_backend(
            &mut self.local.memory,
            crate::power_monitor::PowerBackend::InstalledStorageOnly,
        );
        self.local.memory.compare_table_profile = comparison_profile;
        self.local.memory.decimal_scratch_profile = decimal_profile;
        self.local.memory.random_tail_profile = random_profile;
        self.local.memory.calendar_tail_profile = calendar_profile;
        self.teaching_tick_inputs.clear();
        self.clock_startup_input = None;
        self.rtc_comparison = None;
        self.rtc_mode_active = false;
        self.rtc_decoder_diagnostics = false;
        self.rtc_decoder_expected = None;
        self.local
            .memory
            .system_relays
            .set_profile(crate::system_relays::ClockProfile::InstalledFixed);
        self.local
            .memory
            .system_relays
            .set_high_speed_backend(crate::system_relays::HighSpeedBackend::InstalledFixedStorage);
        self.local.control.profile = crate::program_control::ControlProfile::InstalledLegacy;
        // The installed C200..C234 DCNT handler counts rung edges; the
        // DocumentedEvents backend instead models model-specific physical
        // counter inputs (including C232..C234). Do not mix those profiles.
        // C235+ native encoding remains blocked until its physical path has
        // independent runtime evidence. Model selection is still explicit.
        self.local.memory.counters.model = match self.target {
            Target::Es2 => crate::counter::CounterModel::Es2,
            Target::Ss2 => crate::counter::CounterModel::Ss2,
        };
        self.local.memory.counters.profile = crate::counter::CounterProfile::InstalledLegacy;
        // M1170 instruction stepping resets the original cost accumulator on
        // every service invocation. Host latency/fixed100ms is not that cost.
        self.local.memory.watchdog.profile = crate::interrupt::WatchdogProfile::Disabled;
        self.local.memory.refresh_io.filter_profile =
            crate::refresh_io::FilterProfile::InstalledRegisters;
        self.local.memory.sfc.profile = crate::sfc::SfcProfile::InstalledLegacy;
        self.local
            .memory
            .special_timers
            .set_teaching_tick_profile(true);
        self.local.memory.special_timers.set_special_hold_profile(
            crate::special_timers::SpecialHoldProfile::InstalledTableCandidate,
        );
        self.local.memory.data_register_limit = Some(if self.target == Target::Ss2 {
            4999
        } else {
            9999
        });
        self.trace.clear();
        self.faulted = true;
        self.native = Some(OwnedNative::start(&self.path, self.port, &self.cancel)?);
        self.bit_write(RUN, false)?;
        match self.target {
            Target::Ss2 => {
                // This bank path and its 16-word blocks are validated against
                // the owned SS2 simulator, not a physical PLC transfer.
                for (chunk, words) in encoded.words.chunks(16).enumerate() {
                    let start = chunk * 16;
                    let pdu =
                        crate::plc_program_transfer::ss2_simulator_bank_write_request(start, words)
                            .map_err(|e| format!("SS2 program-write request invalid: {e:?}"))?;
                    let out = self.request(pdu[0], &pdu[1..])?;
                    let response = std::iter::once(pdu[0]).chain(out).collect::<Vec<_>>();
                    crate::plc_program_transfer::ss2_simulator_bank_write_response(
                        &response,
                        start,
                        words.len(),
                    )
                    .map_err(|_| "SS2 program-write echo mismatch".to_owned())?;
                }
            }
            Target::Es2 => {
                for (chunk, words) in encoded.words.chunks(100).enumerate() {
                    let start = (chunk * 100) as u16;
                    let pdu = crate::plc_program_transfer::simulator_write_request(start, words)
                        .map_err(|e| format!("ES2 program-write request invalid: {e:?}"))?;
                    let out = self.request(pdu[0], &pdu[1..])?;
                    let response = std::iter::once(pdu[0]).chain(out).collect::<Vec<_>>();
                    crate::plc_program_transfer::simulator_write_response(
                        &response,
                        start,
                        words.len(),
                    )
                    .map_err(|_| "ES2 program-write echo mismatch".to_owned())?;
                }
            }
        }
        for (chunk, words) in encoded.words.chunks(100).enumerate() {
            let pdu = crate::plc_program_transfer::simulator_read_request(
                (chunk * 100) as u16,
                words.len(),
            )
            .map_err(|e| format!("ES2 program-read request invalid: {e:?}"))?;
            let out = self.request(pdu[0], &pdu[1..])?;
            if decode_program_chunk(&out, words.len())? != words {
                return Err(format!(
                    "ES2 program readback differs at word {}",
                    chunk * 100
                ));
            }
        }
        // Original function 0x74 uses the shared numeric scratch as its
        // readback cursor; after the final chunk it is the program word count.
        self.local.memory.native_numeric_scratch = Some(encoded.words.len() as i16);
        self.bit_write(COMMIT, true)?;
        // M1039 + D1039 select original fixed scan time; M1170/M1171 expose stepping.
        self.word_write(0x140f, SCAN_MS as u16)?;
        self.bit_write(0x0c0f, true)?;
        self.local.memory.set_word(
            Device {
                area: Area::D,
                index: 1039,
            },
            SCAN_MS as i16,
        );
        self.local.memory.set_system(1039, true);
        self.local.seed_running_flags();
        self.bit_write(STEP_MODE, true)?;
        if self.word_read(PC)? != 0 {
            return Err("ES2 did not start at program counter 0".into());
        }
        let clock_devices: [Device; 4] = std::array::from_fn(|n| Device {
            area: Area::M,
            index: 1011 + n as u16,
        });
        let initial_clocks = self.read_bit_devices(&clock_devices)?;
        let levels = clock_devices.map(|d| initial_clocks[&d]);
        if self.word_read(PC)? != 0 {
            return Err("Native PC changed during initial clock input capture".into());
        }
        crate::system_relays::ClockState::seed_installed_levels(&mut self.local.memory, levels)?;
        let owned_pid = self
            .native
            .as_ref()
            .and_then(|native| native._child.0.as_ref())
            .ok_or("Owned native process missing")?
            .id();
        self.clock_startup_input = Some(ClockStartupInput {
            owned_pid,
            boundary_pc: 0,
            devices: clock_devices.map(|d| d.to_string()).into(),
            protocol_addresses: clock_devices.into_iter().map(bit_address).collect::<Result<_, _>>()?,
            levels,
            method: "One read of M1011..M1014 after M1039+M1170 setup at PC0, before first source instruction; fixed phases remain zero; no later results are copied".into(),
        });
        self.program = Some(program);
        self.encoded = Some(encoded);
        self.faulted = false;
        let result = self.snapshot();
        if result.is_err() {
            self.faulted = true;
        }
        result
    }
    pub fn step(&mut self, inputs: &[(Device, bool)]) -> Result<Comparison, String> {
        if self.faulted {
            return Err("ES2 session faulted; reload program".into());
        }
        if self.online_tainted {
            return Err(
                "Serbest RUN sonrasında karşılaştırma için programı yeniden yükleyin".into(),
            );
        }
        let result = if self.rtc_comparison.is_some() {
            self.step_rtc_inner(inputs)
        } else {
            self.step_inner(inputs)
        };
        if result.is_err() {
            self.faulted = true;
        }
        result
    }
    /// Read exactly the known uploaded image, without inferring length from
    /// END words (which can also appear in instruction operands). Function
    /// 0x74 updates the native numeric scratch cursor; mirror that observed
    /// side effect locally so subsequent differential scans remain consistent.
    pub fn read_program_words(&mut self) -> Result<Vec<u16>, String> {
        if self.faulted {
            return Err("ES2 session faulted; reload program".into());
        }
        if self.online_active {
            return Err("Programı okumadan önce online RUN durdurulmalı".into());
        }
        let length = self.encoded.as_ref().ok_or("Önce bu simülatör oturumuna program yükleyin; bağımsız program uzunluğu algılanamıyor")?.words.len();
        if self.target.validate_program_word_count(length).is_err() {
            return Err("Simülatör program uzunluğu geçersiz".into());
        }
        let result = self.read_program_words_inner(length);
        match result {
            Ok(words) => {
                self.local.memory.native_numeric_scratch = Some(length as i16);
                Ok(words)
            }
            Err(error) => {
                self.faulted = true;
                Err(error)
            }
        }
    }
    fn read_program_words_inner(&mut self, length: usize) -> Result<Vec<u16>, String> {
        let mut words = Vec::with_capacity(length);
        while words.len() < length {
            let count = (length - words.len()).min(100);
            let request =
                crate::plc_program_transfer::simulator_read_request(words.len() as u16, count)
                    .map_err(|e| format!("ES2 program-read request invalid: {e:?}"))?;
            let response = self.request(request[0], &request[1..])?;
            words.extend(decode_program_chunk(&response, count)?);
        }
        Ok(words)
    }

    /// Read without local program-length metadata. Each response is scanned
    /// only at decoded instruction boundaries; unsupported or noncanonical
    /// instructions fail closed. This protocol is verified only against the
    /// owned ES2/SS2 simulator, not a physical PLC.
    pub fn discover_program_words(&mut self) -> Result<Vec<u16>, ProgramDiscoveryError> {
        if self.faulted {
            return Err(ProgramDiscoveryError::Transport(
                "ES2 session faulted; reload program".into(),
            ));
        }
        if self.online_active {
            return Err(ProgramDiscoveryError::Decode(
                "Programı okumadan önce online RUN durdurulmalı".into(),
            ));
        }
        let mut words = Vec::new();
        let limit = self.target.program_word_limit();
        while words.len() < limit {
            let count = (limit - words.len()).min(100);
            let request =
                crate::plc_program_transfer::simulator_read_request(words.len() as u16, count)
                    .map_err(|e| {
                        ProgramDiscoveryError::Transport(format!(
                            "ES2 program-read request invalid: {e:?}"
                        ))
                    })?;
            let response = match self.request(request[0], &request[1..]) {
                Ok(response) => response,
                Err(error) => {
                    self.faulted = true;
                    return Err(ProgramDiscoveryError::Transport(error));
                }
            };
            let chunk = match decode_program_chunk(&response, count) {
                Ok(chunk) => chunk,
                Err(error) => {
                    self.faulted = true;
                    return Err(ProgramDiscoveryError::Transport(error));
                }
            };
            words.extend(chunk);
            // 0x74 advances native scratch through every requested word,
            // including the tail read after END. Keep local state in sync even
            // if later decoding reports an unsupported image.
            self.local.memory.native_numeric_scratch = Some(words.len() as i16);
            match crate::es2_program_decode::decode_prefix(&words) {
                Ok(crate::es2_program_decode::PrefixDecode::Complete {
                    words_consumed, ..
                }) => {
                    words.truncate(words_consumed);
                    return Ok(words);
                }
                Ok(crate::es2_program_decode::PrefixDecode::Incomplete { .. }) => {}
                Err(error) => return Err(ProgramDiscoveryError::Decode(error.to_string())),
            }
        }
        Err(ProgramDiscoveryError::Decode(
            "Desteklenen program sınırında END bulunamadı veya program eksik".into(),
        ))
    }
    /// Observe the owned, stepped target without executing a PLC instruction or
    /// advancing local virtual time. Host-driven native inputs may still change.
    pub fn observe_snapshot(&mut self) -> Result<Comparison, String> {
        if self.faulted || self.program.is_none() || self.online_tainted {
            return Err("Snapshot için hazır ve yüklenmiş simülatör oturumu gerekiyor".into());
        }
        self.snapshot()
    }

    /// Free RUN is verified for the exact COMMGR SS2 simulator only. Values
    /// are sequential observations, not an atomic PLC scan or comparison.
    pub fn start_online(&mut self) -> Result<OnlineSnapshot, String> {
        if self.rtc_mode_active {
            return Err("RTC settled comparison requires paused instruction windows; reload before free RUN".into());
        }
        if self.faulted || self.program.is_none() {
            return Err("Önce SS2 simülatörüne program yükleyin".into());
        }
        if self.digest != COMMGR_SS2_SHA256 {
            return Err(
                "Online RUN yalnız doğrulanmış COMMGR 2.11 SS2 simülatöründe kullanılabilir".into(),
            );
        }
        if self.online_active {
            return Err("SS2 online RUN zaten çalışıyor".into());
        }
        // Once normal RUN is entered, the local step model is no longer in
        // lockstep. Only a fresh upload can establish a comparison again.
        self.online_tainted = true;
        let observed_at = Instant::now();
        let result = (|| {
            self.bit_write(RUN, false)?;
            self.bit_write(STEP_MODE, false)?;
            self.bit_write(0x0c0f, false)?; // M1039: turn off fixed 100 ms scans.
            self.bit_write(RUN, true)?;
            self.online_active = true;
            let snapshot = self.online_snapshot(observed_at)?;
            if !snapshot.running {
                return Err("SS2 RUN durumu doğrulanamadı".into());
            }
            Ok(snapshot)
        })();
        self.finish_online_operation(result)
    }

    pub fn poll_online(&mut self, inputs: &[(Device, bool)]) -> Result<OnlineSnapshot, String> {
        if self.faulted || !self.online_active {
            return Err("SS2 online RUN etkin değil".into());
        }
        if inputs.len() > 256
            || inputs
                .iter()
                .any(|(d, _)| d.area != Area::X || d.index > 255)
        {
            return Err("Online girişleri X0..X377 ile sınırlıdır".into());
        }
        let observed_at = Instant::now();
        let result = (|| {
            for (device, value) in inputs {
                self.bit_write(bit_address(*device)?, *value)?;
            }
            self.online_snapshot(observed_at)
        })();
        self.finish_online_operation(result)
    }

    pub fn stop_online(&mut self) -> Result<OnlineSnapshot, String> {
        if self.faulted || !self.online_active {
            return Err("SS2 online RUN etkin değil".into());
        }
        let observed_at = Instant::now();
        let result = (|| {
            self.bit_write(RUN, false)?;
            self.online_active = false;
            let snapshot = self.online_snapshot(observed_at)?;
            if snapshot.running {
                return Err("SS2 STOP durumu doğrulanamadı".into());
            }
            Ok(snapshot)
        })();
        self.finish_online_operation(result)
    }

    fn finish_online_operation(
        &mut self,
        result: Result<OnlineSnapshot, String>,
    ) -> Result<OnlineSnapshot, String> {
        if result.is_err() {
            // A broken transport cannot safely issue STOP: close our private
            // simulator process rather than leaving an unobserved RUN alive.
            self.online_active = false;
            self.faulted = true;
            self.native.take();
        }
        result
    }

    fn online_snapshot(&mut self, observed_at: Instant) -> Result<OnlineSnapshot, String> {
        let devices = self
            .program
            .as_ref()
            .ok_or("No program")?
            .devices
            .iter()
            .copied()
            .collect::<Vec<_>>();
        // One batched read per area range: D, T and 16-bit C values together.
        let data = self.read_data_devices(
            &devices
                .iter()
                .copied()
                .filter(|d| {
                    matches!(d.area, Area::D | Area::T) || (d.area == Area::C && d.index < 200)
                })
                .collect::<Vec<_>>(),
        )?;
        let bits = self.read_bit_devices(
            &devices
                .iter()
                .copied()
                .filter(|d| d.bit())
                .collect::<Vec<_>>(),
        )?;
        let mut rows = Vec::new();
        for device in devices {
            if device.bit() {
                rows.push(OnlineReading {
                    device,
                    kind: "bit",
                    value: u32::from(*bits.get(&device).ok_or("Missing online bit")?),
                });
            }
            if device.area == Area::C && device.index >= 200 {
                let mut request = word_address(device)?.to_be_bytes().to_vec();
                request.extend_from_slice(&1u16.to_be_bytes());
                let response = self.request(3, &request)?;
                if response.len() != 5 || response[0] != 4 {
                    return Err("SS2 online counter response length mismatch".into());
                }
                rows.push(OnlineReading {
                    device,
                    kind: "dword",
                    value: u32::from_be_bytes([response[1], response[2], response[3], response[4]]),
                });
            } else if matches!(device.area, Area::D | Area::T | Area::C) {
                let value = *data.get(&device).ok_or("Missing online register")?;
                rows.push(OnlineReading {
                    device,
                    kind: "word",
                    value: u32::from(value),
                });
            }
        }
        let status = self.request(1, &[0x0c, 0x30, 0, 1])?;
        if status.len() != 2 || status[0] != 1 {
            return Err("SS2 RUN durum yanıtı geçersiz".into());
        }
        let running = status[1] & 1 != 0;
        self.online_active = running;
        self.online_sequence = self.online_sequence.saturating_add(1);
        Ok(OnlineSnapshot {
            sequence: self.online_sequence,
            observed_at,
            running,
            rows,
        })
    }
    fn step_inner(&mut self, inputs: &[(Device, bool)]) -> Result<Comparison, String> {
        if self.program.is_none() {
            return Err("Upload a program before stepping".into());
        }
        if inputs.len() > 256
            || inputs
                .iter()
                .any(|(d, _)| d.area != Area::X || d.index > 255)
        {
            return Err("ES2 step accepts at most 256 valid X inputs".into());
        }
        for (d, value) in inputs {
            self.bit_write(bit_address(*d)?, *value)?;
            self.local.memory.set_input(*d, *value);
        }
        let encoded = self.encoded.as_ref().ok_or("No encoded program")?.clone();
        let program = self.program.as_ref().ok_or("No program")?.clone();
        if program.requires_vm() {
            self.local.begin_plan_scan(&program, SCAN_MS, true)?;
            while let Some(pc) = self.local.current_plan_pc() {
                let index = *encoded
                    .execution_plan
                    .by_pc
                    .get(&pc)
                    .ok_or("Local control PC is outside the encoded plan")?;
                if let Some(check) = encoded.preflight.get(&index) {
                    self.preflight(*check)?;
                }
                if let Some(ticks) = self.teaching_tick_replay {
                    use crate::{execution_plan::PlanOp, program_control::ControlOp};
                    if matches!(
                        &encoded.execution_plan.nodes[index].op,
                        PlanOp::End
                            | PlanOp::Control {
                                op: ControlOp::MainEnd,
                                ..
                            }
                    ) {
                        self.replay_teaching_ticks(ticks)?;
                    }
                }
                // Execute the source VM first. Actual native PC never chooses a
                // local branch, call target, return, or loop iteration.
                let expected = self.local.execute_plan_step(&program)?;
                self.bit_write(STEP, true)?;
                let actual = self.word_read(PC)?;
                if actual != expected.next_pc {
                    return Err(format!(
                        "Control PC mismatch at {}: independently predicted {}, original {}",
                        expected.pc, expected.next_pc, actual
                    ));
                }
                self.verified_steps += 1;
                if expected.scan_boundary {
                    break;
                }
            }
        } else {
            let mut index = 0;
            while index < encoded.next_pc.len() {
                if let Some(check) = encoded.preflight.get(&index) {
                    self.preflight(*check)?;
                }
                self.bit_write(STEP, true)?;
                let actual = self.word_read(PC)?;
                index = checked_next_instruction(&encoded, index, actual)?;
                self.verified_steps += 1;
            }
        }
        // Program data must not silently replace the externally selected
        // fixed scan quantum. Reading the control is supported; changing it
        // invalidates this comparison method rather than forcing equality.
        if self.word_read(0x140f)? != SCAN_MS as u16
            || !self.bit_read(plc::Memory::system_device(1039))?
        {
            return Err("Program changed M1039/D1039 fixed scan control; the 100 ms comparison is inconclusive".into());
        }
        if !program.requires_vm() {
            self.local.scan(&program, SCAN_MS);
        }
        if !self.local.memory.execution_limitations.is_empty() {
            return Err(self
                .local
                .memory
                .execution_limitations
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join("; "));
        }
        self.snapshot()
    }
    fn native_value(&mut self, value: &crate::data_processing::DataValue) -> Result<i32, String> {
        use crate::{data_processing::DataValue, plc::Operand, wide::WideOperand};
        match value {
            DataValue::Word(Operand::Constant(v)) => Ok(i32::from(*v)),
            DataValue::Double(WideOperand::Constant(v)) => Ok(*v),
            DataValue::Word(Operand::Register(d)) => {
                Ok(i32::from(self.word_read(word_address(*d)?)? as i16))
            }
            DataValue::Double(WideOperand::Register(d)) => {
                let low = self.word_read(word_address(*d)?)?;
                let high = self.word_read(word_address(Device {
                    area: d.area,
                    index: d.index + 1,
                })?)?;
                Ok((u32::from(low) | (u32::from(high) << 16)) as i32)
            }
        }
    }
    fn preflight_calendar_action(
        &mut self,
        action: &crate::indexed_calendar_time::IndexedCalendarTimeAction,
    ) -> Result<(), String> {
        let memory = self.local.memory.clone();
        let native = action.qualify_native_with(&memory, |captured, devices| {
            self.capture_operand_storage(captured, devices)
        });
        let mut local = memory;
        let outcome = action.execute(&mut local, true, &mut std::collections::BTreeSet::new());
        crate::calendar_tail::qualify_outcomes(&outcome, &native.outcome)
    }
    fn preflight(&mut self, check: es2_encoder::PreflightCheck) -> Result<(), String> {
        use crate::{
            data_processing::DataValue,
            plc::{Action, Memory, Operand},
            wide::WideOperand,
        };
        if let es2_encoder::PreflightCheck::IndexedBitNumberPredicate(index) = check {
            let predicate = match self
                .encoded
                .as_ref()
                .and_then(|e| e.execution_plan.nodes.get(index))
                .map(|n| &n.op)
            {
                Some(crate::execution_plan::PlanOp::Predicate {
                    expr: plc::Expr::IndexedBitNumber(p),
                    ..
                }) => p.clone(),
                _ => return Err("Indexed bit-number predicate preflight site is invalid".into()),
            };
            let mut memory = self.local.memory.clone();
            // Conservative even on a local deciding AND/OR input. An earlier
            // RAND can make native control diverge; no local skip proves safety.
            for index in 0..predicate.source_count() {
                for (device, value) in
                    self.read_data_devices(&predicate.source_modifier_devices(index))?
                {
                    memory.set_word(device, value as i16);
                }
                let source = predicate.resolved_source(index, &memory)?;
                let mut devices = std::collections::BTreeSet::new();
                source.devices(&mut devices);
                self.capture_operand_storage(&mut memory, &devices)?;
            }
            return Ok(());
        }
        if let es2_encoder::PreflightCheck::IndexedBitContact(index) = check {
            let contact = match self
                .encoded
                .as_ref()
                .and_then(|e| e.execution_plan.nodes.get(index))
                .map(|n| &n.op)
            {
                Some(crate::execution_plan::PlanOp::Predicate {
                    expr: plc::Expr::IndexedBit(contact),
                    ..
                }) => contact.clone(),
                _ => return Err("Indexed bit preflight site is invalid".into()),
            };
            let mut memory = self.local.memory.clone();
            for (device, value) in self.read_data_devices(&contact.modifier_devices())? {
                memory.set_word(device, value as i16);
            }
            contact.resolve(&memory)?;
            return Ok(());
        }
        if let es2_encoder::PreflightCheck::IndexedFloatPredicate(index) = check {
            let predicate = match self
                .encoded
                .as_ref()
                .and_then(|e| e.execution_plan.nodes.get(index))
                .map(|n| &n.op)
            {
                Some(crate::execution_plan::PlanOp::Predicate {
                    expr: plc::Expr::IndexedFloatComparison(predicate),
                    ..
                }) => predicate.clone(),
                _ => return Err("Indexed float predicate preflight site is invalid".into()),
            };
            let mut memory = self.local.memory.clone();
            for index in 0..2 {
                for (device, value) in
                    self.read_data_devices(&predicate.source_modifier_devices(index))?
                {
                    memory.set_word(device, value as i16);
                }
                if let Some(source) = predicate.resolved_source(index, &memory)? {
                    let mut devices = std::collections::BTreeSet::new();
                    source.devices(&mut devices);
                    self.capture_operand_storage(&mut memory, &devices)?;
                }
            }
            // Native accumulator state is not inferred from local predictions:
            // conservative qualification is distinct from gated local visits.
            return Ok(());
        }
        if let es2_encoder::PreflightCheck::IndexedPredicate(index) = check {
            let predicate = match self
                .encoded
                .as_ref()
                .and_then(|e| e.execution_plan.nodes.get(index))
                .map(|n| &n.op)
            {
                Some(crate::execution_plan::PlanOp::Predicate {
                    expr: plc::Expr::IndexedComparison(predicate),
                    ..
                }) => predicate.clone(),
                _ => return Err("Indexed predicate preflight site is invalid".into()),
            };
            let mut memory = self.local.memory.clone();
            for (device, value) in self.read_data_devices(&predicate.modifier_devices())? {
                memory.set_word(device, value as i16);
            }
            // As with scalar preflight, bound every native access candidate;
            // the native accumulator is not inferred from local predictions.
            // Offline execution separately preserves actual shortcut behavior.
            predicate.resolve(&memory)?;
            return Ok(());
        }
        let source_index = match check {
            es2_encoder::PreflightCheck::IndexedFloatPredicate(_) => unreachable!("handled above"),
            es2_encoder::PreflightCheck::IndexedPredicate(_) => unreachable!("handled above"),
            es2_encoder::PreflightCheck::IndexedBitNumberPredicate(_) => {
                unreachable!("handled above")
            }
            es2_encoder::PreflightCheck::IndexedBitContact(_) => unreachable!("handled above"),
            es2_encoder::PreflightCheck::Matrix(i) => i,
            es2_encoder::PreflightCheck::Drum(i) => i,
            es2_encoder::PreflightCheck::CalendarClock(i) => i,
            es2_encoder::PreflightCheck::Search(i)
            | es2_encoder::PreflightCheck::Solar(i)
            | es2_encoder::PreflightCheck::Transform(i)
            | es2_encoder::PreflightCheck::Text(i)
            | es2_encoder::PreflightCheck::IndexedScalar(i)
            | es2_encoder::PreflightCheck::IndexedMultiply(i)
            | es2_encoder::PreflightCheck::IndexedTruncated(i)
            | es2_encoder::PreflightCheck::IndexedBcd(i)
            | es2_encoder::PreflightCheck::IndexedFloat(i)
            | es2_encoder::PreflightCheck::IndexedRoot(i)
            | es2_encoder::PreflightCheck::IndexedAngle(i)
            | es2_encoder::PreflightCheck::IndexedDecimalConversion(i)
            | es2_encoder::PreflightCheck::IndexedExpLn(i)
            | es2_encoder::PreflightCheck::IndexedLogPow(i)
            | es2_encoder::PreflightCheck::IndexedDesqr(i)
            | es2_encoder::PreflightCheck::IndexedTrig(i)
            | es2_encoder::PreflightCheck::IndexedFloatComparison(i)
            | es2_encoder::PreflightCheck::IndexedLiteralMove(i)
            | es2_encoder::PreflightCheck::IndexedSwapMask(i)
            | es2_encoder::PreflightCheck::IndexedRandom(i)
            | es2_encoder::PreflightCheck::IndexedBitNumberAction(i)
            | es2_encoder::PreflightCheck::IndexedAbsGray(i)
            | es2_encoder::PreflightCheck::IndexedSearchToggle(i)
            | es2_encoder::PreflightCheck::IndexedCalendarTime(i)
            | es2_encoder::PreflightCheck::IndexedRtc(i)
            | es2_encoder::PreflightCheck::IndexedSegment(i)
            | es2_encoder::PreflightCheck::IndexedChecksum(i)
            | es2_encoder::PreflightCheck::IndexedText(i)
            | es2_encoder::PreflightCheck::IndexedAdditional(i)
            | es2_encoder::PreflightCheck::IndexedDataProcessing(i)
            | es2_encoder::PreflightCheck::IndexedShiftFifo(i)
            | es2_encoder::PreflightCheck::IndexedBlockExchange(i)
            | es2_encoder::PreflightCheck::IndexedRotation(i)
            | es2_encoder::PreflightCheck::IndexedRealBinary(i)
            | es2_encoder::PreflightCheck::IndexedFloatBinary(i)
            | es2_encoder::PreflightCheck::IndexedComparison(i)
            | es2_encoder::PreflightCheck::IndexedBitAction(i)
            | es2_encoder::PreflightCheck::WordCounter(i)
            | es2_encoder::PreflightCheck::NumericReset(i)
            | es2_encoder::PreflightCheck::DoubleCounter(i)
            | es2_encoder::PreflightCheck::Sort(i) => i,
        };
        let mut action = self
            .program
            .as_ref()
            .ok_or("No program")?
            .networks
            .iter()
            .flat_map(|n| n.actions.iter())
            .nth(source_index)
            .ok_or("Preflight action index out of range")?
            .clone();
        if matches!(check, es2_encoder::PreflightCheck::IndexedCalendarTime(_)) {
            let pulse = matches!(action, Action::Pulse(_));
            let pc = *self
                .encoded
                .as_ref()
                .and_then(|e| e.action_pc.get(source_index))
                .ok_or("Calendar header PC metadata missing")?;
            let expected = self.local.random_header_gate(pulse, source_index, pc);
            let active = self.qualify_calendar_native_gate(expected, pulse)?;
            self.verified_calendar_gates += 1;
            if !active {
                return Ok(());
            }
        }
        if matches!(check, es2_encoder::PreflightCheck::IndexedAdditional(_)) {
            let pulse = matches!(action, Action::Pulse(_));
            let pc = *self
                .encoded
                .as_ref()
                .and_then(|e| e.action_pc.get(source_index))
                .ok_or("Additional header PC metadata missing")?;
            let expected = self.local.random_header_gate(pulse, source_index, pc);
            let active = self.qualify_additional_native_gate(expected, pulse)?;
            self.verified_additional_gates += 1;
            if !active {
                return Ok(());
            }
        }
        if matches!(check, es2_encoder::PreflightCheck::IndexedText(_)) {
            let pulse = matches!(action, Action::Pulse(_));
            let pc = *self
                .encoded
                .as_ref()
                .and_then(|e| e.action_pc.get(source_index))
                .ok_or("Text header PC metadata missing")?;
            let expected = self.local.random_header_gate(pulse, source_index, pc);
            let active = self.qualify_text_native_gate(expected, pulse)?;
            self.verified_text_gates += 1;
            if !active {
                return Ok(());
            }
        }
        if matches!(check, es2_encoder::PreflightCheck::IndexedChecksum(_)) {
            let pulse = matches!(action, Action::Pulse(_));
            let pc = *self
                .encoded
                .as_ref()
                .and_then(|e| e.action_pc.get(source_index))
                .ok_or("Checksum header PC metadata missing")?;
            let expected = self.local.random_header_gate(pulse, source_index, pc);
            let active = self.qualify_checksum_native_gate(expected, pulse)?;
            self.verified_checksum_gates += 1;
            if !active {
                return Ok(());
            }
        }
        if matches!(check, es2_encoder::PreflightCheck::IndexedSegment(_)) {
            let pulse = matches!(action, Action::Pulse(_));
            let pc = *self
                .encoded
                .as_ref()
                .and_then(|e| e.action_pc.get(source_index))
                .ok_or("SEGD header PC metadata missing")?;
            let expected = self.local.random_header_gate(pulse, source_index, pc);
            let active = self.qualify_segment_native_gate(expected, pulse)?;
            self.verified_segment_gates += 1;
            if !active {
                return Ok(());
            }
        }
        if matches!(check, es2_encoder::PreflightCheck::IndexedRtc(_)) && self.rtc_mode_active {
            let pulse = matches!(action, Action::Pulse(_));
            let pc = *self
                .encoded
                .as_ref()
                .and_then(|e| e.action_pc.get(source_index))
                .ok_or("RTC header PC metadata missing")?;
            let expected = self.local.random_header_gate(pulse, source_index, pc);
            let active = self.qualify_rtc_native_gate(expected, pulse)?;
            self.verified_rtc_gates += 1;
            if self.rtc_decoder_diagnostics && !active {
                self.rtc_decoder_expected =
                    Some(rtc_comparison::RtcDecoderExpectation::Skipped { header_pc: pc });
            }
            if !active {
                return Ok(());
            }
        }
        if matches!(check, es2_encoder::PreflightCheck::IndexedRandom(_))
            && self.local.memory.random_tail_profile
                == crate::random_tail::RandomTailProfile::CurrentCommgr
        {
            let pulse = matches!(action, Action::Pulse(_));
            let pc = *self
                .encoded
                .as_ref()
                .and_then(|e| e.action_pc.get(source_index))
                .ok_or("RAND header PC metadata is missing")?;
            let expected = self.local.random_header_gate(pulse, source_index, pc);
            let active = self.qualify_random_native_gate(expected, pulse)?;
            self.verified_random_gates += 1;
            if !active {
                return Ok(());
            }
        }
        if let Action::Pulse(inner) = action {
            action = *inner;
        }
        match action {
            Action::WordCounter(action) => action.validate_span(&self.local.memory)?,
            Action::NumericReset(action) => action.validate_span(&self.local.memory)?,
            Action::DoubleCounter(action) => action.validate_preset_span(&self.local.memory)?,
            Action::IndexedBit(action) => {
                let mut memory = self.local.memory.clone();
                for (device, value) in self.read_data_devices(&action.modifier_devices())? {
                    memory.set_word(device, value as i16);
                }
                action.resolve(&memory)?;
            }
            Action::IndexedComparison(action) => {
                let mut memory = self.local.memory.clone();
                for (device, value) in self.read_data_devices(&action.modifier_devices())? {
                    memory.set_word(device, value as i16);
                }
                action.resolve(&memory)?;
            }
            Action::IndexedAngle(action) => {
                let mut memory = self.local.memory.clone();
                for (device, value) in self.read_data_devices(&action.modifier_devices())? {
                    memory.set_word(device, value as i16);
                }
                let source = action.resolved_source(&memory)?;
                let mut devices = std::collections::BTreeSet::new();
                source.devices(&mut devices);
                self.capture_operand_storage(&mut memory, &devices)?;
                // The clone only qualifies actual paused input addresses.
                // Native results never feed the independent local conversion.
                action.prepare(&memory, &mut Default::default())?;
            }
            Action::IndexedDecimalConversion(action) => {
                let mut memory = self.local.memory.clone();
                for (device, value) in self.read_data_devices(&action.source_modifier_devices())? {
                    memory.set_word(device, value as i16);
                }
                let source = action.resolved_source(&memory)?;
                let mut devices = std::collections::BTreeSet::new();
                source.devices(&mut devices);
                self.capture_operand_storage(&mut memory, &devices)?;
                if action.semantic_error(&memory)?.is_none() {
                    for (device, value) in
                        self.read_data_devices(&action.target_modifier_devices())?
                    {
                        memory.set_word(device, value as i16);
                    }
                }
                // Qualification reads paused inputs only; arithmetic remains local.
                action.prepare(&memory, &mut Default::default())?;
            }
            Action::IndexedExpLn(action) => {
                let mut memory = self.local.memory.clone();
                for (device, value) in self.read_data_devices(&action.source_modifier_devices())? {
                    memory.set_word(device, value as i16);
                }
                let source = action.resolved_source(&memory)?;
                let mut devices = std::collections::BTreeSet::new();
                source.devices(&mut devices);
                self.capture_operand_storage(&mut memory, &devices)?;
                if action.semantic_error(&memory)?.is_none() {
                    for (device, value) in
                        self.read_data_devices(&action.target_modifier_devices())?
                    {
                        memory.set_word(device, value as i16);
                    }
                }
                // Qualification reads paused inputs only; arithmetic remains local.
                action.prepare(&memory, &mut Default::default())?;
            }
            Action::IndexedLogPow(action) => {
                let mut memory = self.local.memory.clone();
                for index in 0..2 {
                    if index == 1 && !action.needs_right_source(&memory)? {
                        break;
                    }
                    for (device, value) in
                        self.read_data_devices(&action.source_modifier_devices(index))?
                    {
                        memory.set_word(device, value as i16);
                    }
                    let source = action.resolved_source(index, &memory)?;
                    let mut devices = std::collections::BTreeSet::new();
                    source.devices(&mut devices);
                    self.capture_operand_storage(&mut memory, &devices)?;
                }
                if action.semantic_error(&memory)?.is_none() {
                    for (device, value) in
                        self.read_data_devices(&action.target_modifier_devices())?
                    {
                        memory.set_word(device, value as i16);
                    }
                }
                // Qualify actual paused inputs without using native result values.
                action.prepare(&memory, &mut Default::default())?;
            }
            Action::IndexedDesqr(action) => {
                let mut memory = self.local.memory.clone();
                for (device, value) in self.read_data_devices(&action.source_modifier_devices())? {
                    memory.set_word(device, value as i16);
                }
                let source = action.resolved_source(&memory)?;
                let mut devices = std::collections::BTreeSet::new();
                source.devices(&mut devices);
                self.capture_operand_storage(&mut memory, &devices)?;
                if action.semantic_error(&memory)?.is_none() {
                    for (device, value) in
                        self.read_data_devices(&action.target_modifier_devices())?
                    {
                        memory.set_word(device, value as i16);
                    }
                }
                // Qualification reads paused inputs only; arithmetic remains local.
                action.prepare(&memory, &mut Default::default())?;
            }
            Action::IndexedTrig(action) => {
                let mut memory = self.local.memory.clone();
                for (device, value) in self.read_data_devices(&action.source_modifier_devices())? {
                    memory.set_word(device, value as i16);
                }
                let source = action.resolved_source(&memory)?;
                let mut devices = std::collections::BTreeSet::new();
                source.devices(&mut devices);
                self.capture_operand_storage(&mut memory, &devices)?;
                if let Some(mode) = action.mode_device() {
                    memory.set_bit(mode, self.bit_read(mode)?);
                }
                if action.semantic_error(&memory)?.is_none() {
                    for (device, value) in
                        self.read_data_devices(&action.target_modifier_devices())?
                    {
                        memory.set_word(device, value as i16);
                    }
                }
                // Qualification reads paused inputs only; arithmetic remains local.
                action.prepare(&memory, &mut Default::default())?;
            }
            Action::IndexedFloatComparison(action) => {
                let mut memory = self.local.memory.clone();
                for index in 0..action.sources().len() {
                    for (device, value) in
                        self.read_data_devices(&action.source_modifier_devices(index))?
                    {
                        memory.set_word(device, value as i16);
                    }
                    let source = action.resolved_source(index, &memory)?;
                    let mut devices = std::collections::BTreeSet::new();
                    source.devices(&mut devices);
                    self.capture_operand_storage(&mut memory, &devices)?;
                }
                if action.semantic_error(&memory)?.is_none() {
                    for (device, value) in
                        self.read_data_devices(&action.target_modifier_devices())?
                    {
                        memory.set_word(device, value as i16);
                    }
                }
                // Qualification reads paused inputs only; arithmetic remains local.
                action.prepare(&memory, &mut Default::default())?;
            }
            Action::IndexedLiteralMove(action) => {
                let mut memory = self.local.memory.clone();
                for (device, value) in self.read_data_devices(&action.modifier_devices())? {
                    memory.set_word(device, value as i16);
                }
                // Source bits are a fixed validated literal; only the target
                // index comes from the paused native process for qualification.
                action.prepare(&memory, &mut Default::default())?;
            }
            Action::IndexedSwapMask(action) => {
                let mut memory = self.local.memory.clone();
                for index in 0..action.source_count() {
                    for (device, value) in
                        self.read_data_devices(&action.source_modifier_devices(index))?
                    {
                        memory.set_word(device, value as i16);
                    }
                    let source = action.resolved_source(index, &memory)?;
                    let mut devices = std::collections::BTreeSet::new();
                    source.devices(&mut devices);
                    self.capture_operand_storage(&mut memory, &devices)?;
                }
                for (device, value) in self.read_data_devices(&action.target_modifier_devices())? {
                    memory.set_word(device, value as i16);
                }
                // Qualification preserves source, mask, old-target read order.
                // Captured values stay in this clone and never replace local results.
                action.prepare(&memory, &mut Default::default())?;
            }
            Action::IndexedRandom(action) => {
                let mut memory = self.local.memory.clone();
                for index in 0..action.source_count() {
                    for (device, value) in
                        self.read_data_devices(&action.source_modifier_devices(index))?
                    {
                        memory.set_word(device, value as i16);
                    }
                    let source = action.resolved_source(index, &memory)?;
                    let mut devices = std::collections::BTreeSet::new();
                    source.devices(&mut devices);
                    self.capture_operand_storage(&mut memory, &devices)?;
                }
                if action.semantic_error(&memory)?.is_none() {
                    for (device, value) in
                        self.read_data_devices(&action.target_modifier_devices())?
                    {
                        memory.set_word(device, value as i16);
                    }
                }
                // Both bounds precede semantic domain checks; failed domains skip
                // the target. Paused inputs never replace local random results.
                action.prepare(&memory, &mut Default::default())?;
            }
            Action::IndexedBitNumber(action) => {
                let mut memory = self.local.memory.clone();
                for index in 0..action.source_count() {
                    for (device, value) in
                        self.read_data_devices(&action.source_modifier_devices(index))?
                    {
                        memory.set_word(device, value as i16);
                    }
                    let source = action.resolved_source(index, &memory)?;
                    let mut devices = std::collections::BTreeSet::new();
                    source.devices(&mut devices);
                    self.capture_operand_storage(&mut memory, &devices)?;
                }
                for (device, value) in self.read_data_devices(&action.target_modifier_devices())? {
                    memory.set_word(device, value as i16);
                }
                // Clone-only address qualification; never inject native scalar
                // results into local RMW computation. OFF visits remain bounded.
                action.prepare(&memory, &mut Default::default())?;
            }
            Action::IndexedAbsGray(action) => {
                let mut memory = self.local.memory.clone();
                for index in 0..action.source_count() {
                    for (device, value) in
                        self.read_data_devices(&action.source_modifier_devices(index))?
                    {
                        memory.set_word(device, value as i16);
                    }
                    let source = action.resolved_source(index, &memory)?;
                    let mut devices = std::collections::BTreeSet::new();
                    source.devices(&mut devices);
                    self.capture_operand_storage(&mut memory, &devices)?;
                }
                if action.semantic_error(&memory)?.is_none() {
                    for (device, value) in
                        self.read_data_devices(&action.target_modifier_devices())?
                    {
                        memory.set_word(device, value as i16);
                    }
                }
                // Source values are actual paused inputs for domain qualification.
                // A negative Gray source skips target resolution; captured values
                // stay in this clone and never replace local integer computation.
                action.prepare(&memory, &mut Default::default())?;
            }
            Action::IndexedSearchToggle(action) => {
                let memory = self.local.memory.clone();
                // Capture actual paused search inputs in the handler read order.
                // Qualification never replaces independent local arithmetic.
                action.qualify_native_with(&memory, |captured, devices| {
                    self.capture_operand_storage(captured, devices)
                })?;
            }
            Action::IndexedCalendarTime(action) => {
                self.preflight_calendar_action(&action)?;
            }
            Action::Time(action @ crate::time_ops::TimeAction::Arithmetic { .. }) => {
                let binding =
                    crate::indexed_calendar_time::IndexedCalendarTimeAction::from_arithmetic(
                        &action,
                    )
                    .expect("arithmetic adapter");
                self.preflight_calendar_action(&binding)?;
            }
            Action::IndexedDataProcessing(action) => {
                let memory = self.local.memory.clone();
                // Hydrate actual paused inputs at each replay read. The action
                // retains clone-written zero/error/target overlays and follows
                // late modifier lookup. Nothing replaces independent local math.
                action.qualify_native_with(&memory, |captured, devices| {
                    self.capture_operand_storage(captured, devices)
                })?;
            }
            Action::IndexedShiftFifo(action) => {
                let memory = self.local.memory.clone();
                // Hydrate actual paused inputs at each replay read. The action
                // retains clone-written overlays through FIFO header/output
                // stores and late modifier lookup. Nothing replaces local math.
                action.qualify_native_with(&memory, |captured, devices| {
                    self.capture_operand_storage(captured, devices)
                })?;
            }
            Action::IndexedBlockExchange(action) => {
                let mut memory = self.local.memory.clone();
                // Use the paused target's mode/count/index inputs, even when
                // local power is OFF or a P site is held after random control.
                for (device, value) in self.read_bit_devices(&action.control_devices())? {
                    memory.set_system(device.index, value);
                }
                for (device, value) in self.read_data_devices(&action.modifier_devices())? {
                    memory.set_word(device, value as i16);
                }
                if let Some(count) = action.count_device() {
                    self.capture_operand_storage(
                        &mut memory,
                        &std::collections::BTreeSet::from([count]),
                    )?;
                }
                let devices = action.capture_devices(&memory)?;
                self.capture_operand_storage(&mut memory, &devices)?;
                // XCH can overwrite the FIRST index while storing SECOND.
                // Its late destination is qualified by clone-only replay.
                // Captured native inputs never replace local calculations.
                action.qualify_native(&memory)?;
            }
            Action::IndexedRotation(action) => {
                let mut memory = self.local.memory.clone();
                for index in 0..action.source_count() {
                    for (device, value) in
                        self.read_data_devices(&action.source_modifier_devices(index))?
                    {
                        memory.set_word(device, value as i16);
                    }
                    let source = action.resolved_source(index, &memory)?;
                    let mut devices = std::collections::BTreeSet::new();
                    source.devices(&mut devices);
                    self.capture_operand_storage(&mut memory, &devices)?;
                }
                for (device, value) in self.read_data_devices(&action.target_modifier_devices())? {
                    memory.set_word(device, value as i16);
                }
                // Rotations accept every scalar bit pattern. Carry does not
                // change D-backed address modifiers, so preflight only checks
                // the actual source and late write span; no native carry/result
                // is injected into the local rotation calculation. Even local
                // OFF/held visits qualify both candidate spans conservatively.
                action.qualify_target(&memory)?;
            }
            Action::IndexedRealBinary(action) => {
                let mut memory = self.local.memory.clone();
                // Capture only the inputs actually visited by this semantic
                // path. A left NaN in ADDR/SUBR/MULR skips the right descriptor
                // and its modifier; DIVR must read both before validation.
                for index in 0..2 {
                    if index == 1 && !action.needs_right_source(&memory)? {
                        break;
                    }
                    for (device, value) in
                        self.read_data_devices(&action.source_modifier_devices(index))?
                    {
                        memory.set_word(device, value as i16);
                    }
                    if let Some(source) = action.resolved_source(index, &memory)? {
                        let mut devices = std::collections::BTreeSet::new();
                        source.devices(&mut devices);
                        self.capture_operand_storage(&mut memory, &devices)?;
                    }
                }
                if action.semantic_error(&memory)?.is_none() {
                    for (device, value) in
                        self.read_data_devices(&action.target_modifier_devices())?
                    {
                        memory.set_word(device, value as i16);
                    }
                }
                // Qualification never feeds native outputs into local math.
                // OFF/held candidates remain conservatively address-checked.
                action.prepare(&memory, &mut Default::default())?;
            }
            Action::IndexedFloatBinary(action) => {
                let mut memory = self.local.memory.clone();
                for (device, value) in self.read_data_devices(&action.modifier_devices())? {
                    memory.set_word(device, value as i16);
                }
                // Capture both actual native inputs in operand order. DIV
                // errors preserve flags and can bypass an invalid target.
                for index in 0..2 {
                    let source = action.resolved_source(index, &memory)?;
                    let mut devices = std::collections::BTreeSet::new();
                    source.devices(&mut devices);
                    self.capture_operand_storage(&mut memory, &devices)?;
                }
                action.prepare(&memory, &mut Default::default())?;
            }
            Action::IndexedRoot(action) => {
                let mut memory = self.local.memory.clone();
                for (device, value) in self.read_data_devices(&action.modifier_devices())? {
                    memory.set_word(device, value as i16);
                }
                // A negative source returns before the original resolves its
                // destination. Qualification uses the actual paused source.
                let source = action.resolved_source(&memory)?;
                let mut devices = std::collections::BTreeSet::new();
                source.devices(&mut devices);
                self.capture_operand_storage(&mut memory, &devices)?;
                action.prepare(&memory, &mut Default::default())?;
            }
            Action::IndexedFloat(action) => {
                let mut memory = self.local.memory.clone();
                // The conversion direction controls real operand widths. The
                // paused original supplies mode and source, not previous RAM.
                if let Some(mode) = action.mode_device() {
                    memory.set_bit(mode, self.bit_read(mode)?);
                }
                for (device, value) in self.read_data_devices(&action.modifier_devices())? {
                    memory.set_word(device, value as i16);
                }
                let source = action.resolved_source(&memory)?;
                let mut devices = std::collections::BTreeSet::new();
                source.devices(&mut devices);
                self.capture_operand_storage(&mut memory, &devices)?;
                action.prepare(&memory, &mut Default::default())?;
            }
            Action::IndexedBcd(action) => {
                let mut memory = self.local.memory.clone();
                for (device, value) in self.read_data_devices(&action.modifier_devices())? {
                    memory.set_word(device, value as i16);
                }
                // Source validity controls whether the native handler reaches
                // the destination. Read the paused original, not stale local RAM.
                let source = action.resolved_source(&memory)?;
                let mut devices = std::collections::BTreeSet::new();
                source.devices(&mut devices);
                self.capture_operand_storage(&mut memory, &devices)?;
                action.prepare(&memory, &mut Default::default())?;
            }
            Action::IndexedTruncated(action) => {
                let mut memory = self.local.memory.clone();
                for (device, value) in self.read_data_devices(&action.modifier_devices())? {
                    memory.set_word(device, value as i16);
                }
                for index in 0..action.sources().len() {
                    let source = action.resolved_source(index, &memory)?;
                    let mut devices = std::collections::BTreeSet::new();
                    source.devices(&mut devices);
                    self.capture_operand_storage(&mut memory, &devices)?;
                }
                let prepared = action.prepare(&memory, &mut Default::default())?;
                qualify_indexed_truncated_result(&prepared, &memory)?;
            }
            Action::IndexedMultiply(action) => {
                let mut memory = self.local.memory.clone();
                for (device, value) in self.read_data_devices(&action.modifier_devices())? {
                    memory.set_word(device, value as i16);
                }
                // A divisor can be changed by an earlier native instruction.
                // Capture actual source storage before deciding whether the
                // destination is accessed; previous local values cannot decide.
                for index in 0..action.sources().len() {
                    let source = action.resolved_source(index, &memory)?;
                    let mut devices = std::collections::BTreeSet::new();
                    source.devices(&mut devices);
                    self.capture_operand_storage(&mut memory, &devices)?;
                }
                let prepared = action.prepare(&memory, &mut Default::default())?;
                if let crate::indexed_multiply::PreparedMultiply::Ready(action) = prepared
                    && let Action::Wide(wide) = action.as_ref()
                    && wide.op == crate::wide::WideOp::Divide
                    && wide.sources[0].value().read(&memory) == i32::MIN
                    && wide.sources[1].value().read(&memory) == -1
                {
                    return Err("DDIV MIN32/-1 is excluded: the installed native handler does not return reliably".into());
                }
            }
            Action::IndexedScalar(action) => {
                // Offset words can change earlier in this very scan. Native
                // input at this PC, never previous local state, guards replay.
                let mut memory = self.local.memory.clone();
                for (device, value) in self.read_data_devices(&action.modifier_devices())? {
                    memory.set_word(device, value as i16);
                }
                action.resolve(&memory)?;
            }
            Action::Text(
                legacy @ (crate::text_encoding::TextAction::Lrc { .. }
                | crate::text_encoding::TextAction::Crc { .. }),
            ) => {
                let action = crate::indexed_checksum::IndexedChecksumAction::from_legacy(&legacy)?;
                let memory = self.local.memory.clone();
                let visit = action.qualify_native_with(&memory, |captured, devices| {
                    self.capture_operand_storage(captured, devices)
                });
                if let crate::indexed_checksum::ChecksumOutcome::Limitation(reason) = visit.outcome
                {
                    return Err(reason);
                }
                self.local.observed_devices.extend(visit.observed);
            }
            Action::IndexedChecksum(action) => {
                let memory = self.local.memory.clone();
                let visit = action.qualify_native_with(&memory, |captured, devices| {
                    self.capture_operand_storage(captured, devices)
                });
                if let crate::indexed_checksum::ChecksumOutcome::Limitation(reason) = visit.outcome
                {
                    return Err(reason);
                }
                self.local.observed_devices.extend(visit.observed);
            }
            Action::Text(
                legacy @ (crate::text_encoding::TextAction::Ascii { .. }
                | crate::text_encoding::TextAction::Hex { .. }
                | crate::text_encoding::TextAction::Check { .. }),
            ) => {
                let action = crate::indexed_text::IndexedTextAction::from_legacy(&legacy)?;
                let memory = self.local.memory.clone();
                let visit = action.qualify_native_with(&memory, |captured, devices| {
                    self.capture_operand_storage(captured, devices)
                });
                if let crate::indexed_text::TextOutcome::Limitation(reason) = visit.outcome {
                    return Err(reason);
                }
                self.local.observed_devices.extend(visit.observed);
            }
            Action::Additional(
                legacy @ (crate::additional_processing::AdditionalAction::WordSum { .. }
                | crate::additional_processing::AdditionalAction::Scale { .. }
                | crate::additional_processing::AdditionalAction::ScalePoints { .. }
                | crate::additional_processing::AdditionalAction::CompareTable { .. }),
            ) => {
                let action =
                    crate::indexed_additional::IndexedAdditionalAction::from_legacy(&legacy)?;
                let memory = self.local.memory.clone();
                let visit = action.qualify_native_with(&memory, |captured, devices| {
                    self.capture_operand_storage(captured, devices)
                });
                if let crate::indexed_additional::AdditionalOutcome::Limitation(reason) =
                    visit.outcome
                {
                    return Err(reason);
                }
                self.local.observed_devices.extend(visit.observed);
            }
            Action::IndexedAdditional(action) => {
                let memory = self.local.memory.clone();
                let visit = action.qualify_native_with(&memory, |captured, devices| {
                    self.capture_operand_storage(captured, devices)
                });
                if let crate::indexed_additional::AdditionalOutcome::Limitation(reason) =
                    visit.outcome
                {
                    return Err(reason);
                }
                self.local.observed_devices.extend(visit.observed);
            }
            Action::IndexedText(action) => {
                let memory = self.local.memory.clone();
                let visit = action.qualify_native_with(&memory, |captured, devices| {
                    self.capture_operand_storage(captured, devices)
                });
                if let crate::indexed_text::TextOutcome::Limitation(reason) = visit.outcome {
                    return Err(reason);
                }
                self.local.observed_devices.extend(visit.observed);
            }
            Action::Text(action) => {
                let mut memory = self.local.memory.clone();
                let mode = Memory::system_device(1161);
                memory.set_internal(mode, self.bit_read(mode)?);
                action.validate_execution(&memory)?;
            }
            Action::Matrix(action) => {
                // Earlier instructions in this same scan may have changed n.
                // Local execution occurs after native scan completion, so its
                // previous-scan count is not an oracle for target buffer safety.
                let mut memory = self.local.memory.clone();
                if let DataValue::Word(Operand::Register(d)) = action.count() {
                    let values = self.read_data_devices(&[*d])?;
                    for (device, value) in values {
                        memory.set_word(device, value as i16);
                    }
                }
                action.validate_native_span(&memory)?;
            }
            Action::Drum(action) => action.validate_native_span(&self.local.memory)?,
            Action::IndexedSegment(action) => {
                let memory = self.local.memory.clone();
                let visited = action.qualify_native_with(&memory, |captured, devices| {
                    self.capture_operand_storage(captured, devices)
                })?;
                self.local.observed_devices.extend(visited);
            }
            Action::IndexedRtc(action) if self.rtc_mode_active => {
                self.preflight_rtc_action(&action)?;
            }
            Action::Rtc(action) if self.rtc_mode_active => {
                self.preflight_rtc_action(&crate::indexed_rtc::IndexedRtcAction::from_legacy(
                    &action,
                ))?;
            }
            Action::IndexedRtc(_) | Action::Rtc(_) | Action::Hour(_) => {
                return Err("TRD/TWR/HOUR/DHOUR: kurulu simülatörün arka plan RTC/tick girdileri bu normal oturumda sabitlenmedi; native karşılaştırma belirsiz. Yerel açık saat profili veya ayrıca kaydedilmiş kontrollü handler tekrarı kullanılmalı".into());
            }
            Action::Solar(a) => {
                let mut memory = Memory::default();
                memory.data_register_limit = Some(if self.target == Target::Ss2 {
                    4999
                } else {
                    9999
                });
                let inputs = (0..26)
                    .map(|offset| Device {
                        area: Area::D,
                        index: a.source.index + offset,
                    })
                    .collect::<Vec<_>>();
                for (d, value) in self.read_data_devices(&inputs)? {
                    memory.set_word(d, value as i16);
                }
                a.validate_installed_execution(&memory)?;
            }
            Action::Transform(a) => {
                let mode = self.native_value(&a.mode)?;
                if mode as u8 == 15 && self.local.memory.transform_context.solar_backend.is_some() {
                    return Err("DTM mode 15 native comparison requires the built-in solar calculation; a custom callback is configured".into());
                }
                let profile = transform_profile(&self.digest)?;
                if !profile.supports(mode) {
                    return Err(format!(
                        "DTM mode {} has local/new-firmware semantics not implemented by this installed target; comparison is inconclusive",
                        mode as u8
                    ));
                }
                let count = self.native_value(&a.count)?;
                let mut memory = Memory::default();
                memory.data_register_limit = Some(if self.target == Target::Ss2 {
                    4999
                } else {
                    9999
                });
                for (v, actual) in [(&a.mode, mode), (&a.count, count)] {
                    if let DataValue::Word(Operand::Register(d)) = v {
                        memory.set_word(*d, actual as i16);
                    }
                }
                let mut footprint = std::collections::BTreeSet::new();
                a.observed_devices(&memory, &mut footprint);
                let words = footprint
                    .into_iter()
                    .filter(|d| d.area == Area::D)
                    .collect::<Vec<_>>();
                for (d, v) in self.read_data_devices(&words)? {
                    memory.set_word(d, v as i16);
                }
                a.validate_for_profile(&memory, profile)?;
            }
            Action::Search(a) => {
                if let crate::search_ops::SearchAction::Search { ref count, .. } = a {
                    let actual = self.native_value(count)?;
                    let mut memory = Memory::default();
                    memory.data_register_limit = Some(if self.target == Target::Ss2 {
                        4999
                    } else {
                        9999
                    });
                    match count {
                        DataValue::Word(Operand::Register(d)) => memory.set_word(*d, actual as i16),
                        DataValue::Double(WideOperand::Register(d)) => {
                            memory.set_double_word(*d, actual)
                        }
                        _ => {}
                    }
                    a.validate_runtime_span(&memory)?;
                }
            }
            Action::RampSort(crate::ramp_sort::RampSortAction::Sort {
                wide,
                source,
                rows: 1,
                columns: 1,
                dest,
                count,
            }) => {
                let n = self.native_value(&count)? as u32;
                if n <= 32 && self.target == Target::Ss2 {
                    let words = n * if wide { 2 } else { 1 };
                    for d in [source, dest] {
                        let end = u32::from(d.index) + words;
                        if end > 5000 && end <= 10000 {
                            return Err(
                                "SORT dynamic span exceeds verified SS2 D0..D4999 range".into()
                            );
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn capture_operand_storage(
        &mut self,
        memory: &mut plc::Memory,
        devices: &std::collections::BTreeSet<Device>,
    ) -> Result<(), String> {
        for &device in devices {
            match device.area {
                Area::X | Area::Y | Area::M | Area::S => {
                    // X and read-only system relays are valid *sources*.
                    // Capture their image without the writable-output filter.
                    memory.set_bit(device, self.bit_read(device)?);
                }
                Area::C if device.index >= 200 => {
                    let mut request = word_address(device)?.to_be_bytes().to_vec();
                    request.extend_from_slice(&1u16.to_be_bytes());
                    let result = self.request(3, &request)?;
                    if result.len() != 5 || result[0] != 4 {
                        return Err("32 bit counter operand read byte count mismatch".into());
                    }
                    memory.set_double_word_raw(
                        device,
                        i32::from_be_bytes([result[1], result[2], result[3], result[4]]),
                    );
                }
                _ => {
                    if device.area == Area::D
                        && device.index > memory.data_register_limit.unwrap_or(9999)
                    {
                        return Err(
                            "Indexed multiply source exceeds the selected model's D range".into(),
                        );
                    }
                    memory.set_word(device, self.word_read(word_address(device)?)? as i16);
                }
            }
        }
        Ok(())
    }
    fn read_data_devices(
        &mut self,
        devices: &[Device],
    ) -> Result<std::collections::BTreeMap<Device, u16>, String> {
        let mut result = std::collections::BTreeMap::new();
        for block in data_read_blocks(devices)? {
            let first = word_address(block[0])?;
            let span = word_address(*block.last().unwrap())? - first + 1;
            let values = self.words_read(first, span)?;
            for device in block {
                let offset = usize::from(word_address(device)? - first);
                result.insert(device, values[offset]);
            }
        }
        Ok(result)
    }
    fn snapshot(&mut self) -> Result<Comparison, String> {
        self.snapshot_for(&self.local.clone())
    }
    fn snapshot_for(&mut self, local: &Simulator) -> Result<Comparison, String> {
        let devices: Vec<Device> = self
            .program
            .as_ref()
            .ok_or("No program")?
            .devices
            .iter()
            .chain(local.observed_devices.iter())
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        // Paused owned target: contiguous D words can be fetched together.
        // Address gaps and the D4095/D4096 protocol window remain separate.
        let data = devices
            .iter()
            .copied()
            .filter(|d| d.area == Area::D)
            .collect::<Vec<_>>();
        let data = self.read_data_devices(&data)?;
        let bits = self.read_bit_devices(
            &devices
                .iter()
                .copied()
                .filter(|d| d.bit())
                .collect::<Vec<_>>(),
        )?;
        let mut rows = Vec::new();
        for d in devices {
            if d.bit() {
                let original = u32::from(*bits.get(&d).ok_or("Missing prefetched bit")?);
                rows.push(Reading {
                    device: d,
                    kind: "bit",
                    local: u32::from(local.memory.bit(d)),
                    original,
                });
            }
            if d.area == Area::C && d.index >= 200 {
                let mut request = word_address(d)?.to_be_bytes().to_vec();
                request.extend_from_slice(&1u16.to_be_bytes());
                let result = self.request(3, &request)?;
                if result.len() != 5 || result[0] != 4 {
                    return Err("32 bit counter read byte count mismatch".into());
                }
                let original = u32::from_be_bytes([result[1], result[2], result[3], result[4]]);
                rows.push(Reading {
                    device: d,
                    kind: "dword",
                    local: local.memory.double_word(d) as u32,
                    original,
                });
            } else if matches!(d.area, Area::D | Area::T | Area::C) {
                let original = if d.area == Area::D {
                    *data.get(&d).ok_or("Missing prefetched D register")?
                } else {
                    self.word_read(word_address(d)?)?
                };
                rows.push(Reading {
                    device: d,
                    kind: "word",
                    local: u32::from(local.memory.word(d) as u16),
                    original: u32::from(original),
                });
            }
        }
        let mismatches = rows.iter().filter(|r| r.local != r.original).count();
        Ok(Comparison {
            local: local.clone(),
            scan: local.scans,
            rows,
            mismatches,
        })
    }
}

fn qualify_indexed_truncated_result(
    prepared: &crate::indexed_multiply::PreparedMultiply,
    memory: &plc::Memory,
) -> Result<(), String> {
    use crate::{
        calendar_operand::CalendarValue,
        indexed_multiply::PreparedMultiply,
        plc::Action,
        wide::{WideDestination, WideOp},
    };
    if let PreparedMultiply::Ready(action) = prepared
        && let Action::Wide(wide) = action.as_ref()
    {
        if let WideDestination::Numeric(target) = &wide.dest
            && matches!(target.value(), CalendarValue::Index { wide: true, .. })
        {
            return Err("MUL32/DIV32 direct E result uses noncanonical D storage in this installed simulator; native comparison is unqualified".into());
        }
        if wide.op == WideOp::DivideQuotient
            && wide.sources[0].value().read(memory) == i32::MIN
            && wide.sources[1].value().read(memory) == -1
        {
            return Err(
                "DIV32 MIN32/-1 is excluded: the installed native handler does not return reliably"
                    .into(),
            );
        }
    }
    Ok(())
}

fn decode_program_chunk(response: &[u8], count: usize) -> Result<Vec<u16>, String> {
    let pdu = std::iter::once(0x74)
        .chain(response.iter().copied())
        .collect::<Vec<_>>();
    crate::plc_program_transfer::simulator_read_response(&pdu, count)
        .map_err(|_| "ES2 program read byte count mismatch".to_owned())
}

const MAX_WORDS: u16 = 100;
/// Unused words a block may span to save a request (each request has a fixed
/// turnaround, about 15.6 ms on the simulators).
const MAX_WORD_GAP: u16 = 16;

/// 16-bit registers (D, T, C0..C199) grouped into FC03 reads. A block may skip a few
/// unused words of the same area where the wire map is continuous; the D4095/D4096
/// protocol window always splits.
fn data_read_blocks(devices: &[Device]) -> Result<Vec<Vec<Device>>, String> {
    let devices = devices
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let mut result: Vec<Vec<Device>> = Vec::new();
    for d in devices {
        if !matches!(d.area, Area::D | Area::T) && !(d.area == Area::C && d.index < 200) {
            return Err("Word batching accepts only D, T and 16-bit C registers".into());
        }
        let address = word_address(d)?;
        if let Some(block) = result.last_mut()
            && block[0].area == d.area
        {
            let first = word_address(block[0])?;
            let last = *block.last().unwrap();
            let step = address.checked_sub(word_address(last)?);
            if step == d.index.checked_sub(last.index)
                && step.is_some_and(|s| s <= MAX_WORD_GAP)
                && address - first < MAX_WORDS
            {
                block.push(d);
                continue;
            }
        }
        result.push(vec![d]);
    }
    Ok(result)
}

fn checked_next_instruction(
    encoded: &crate::es2_encoder::EncodedProgram,
    index: usize,
    actual: u16,
) -> Result<usize, String> {
    let expected = *encoded
        .next_pc
        .get(index)
        .ok_or("ES2 instruction index out of range")?;
    if actual == expected {
        return Ok(index + 1);
    }
    // Only exact compiler-declared padding permits a non-linear step.
    // Never accept an arbitrary later PC: it could hide a skipped action.
    if encoded.padded_short_circuit.get(&index) == Some(&actual)
        && encoded.next_pc.get(index + 6) == Some(&actual)
    {
        return Ok(index + 7);
    }
    if encoded.padded_error_skip.get(&index) == Some(&actual)
        && encoded.next_pc.get(index + 4) == Some(&actual)
    {
        return Ok(index + 5);
    }
    Err(format!(
        "ES2 PC mismatch: expected {expected}, received {actual}; scan is inconclusive"
    ))
}

pub fn bit_address(d: Device) -> Result<u16, String> {
    let limit = match d.area {
        Area::X | Area::Y | Area::T | Area::C => 255,
        Area::S => 1023,
        Area::M => 4095,
        Area::D => return Err("D is not a bit device".into()),
    };
    if d.index > limit {
        return Err("Bit device out of range".into());
    }
    Ok(match d.area {
        Area::S => d.index,
        Area::X => 0x400 + d.index,
        Area::Y => 0x500 + d.index,
        Area::T => 0x600 + d.index,
        Area::C => 0xe00 + d.index,
        Area::M if d.index >= 1536 => 0xb000 + d.index - 1536,
        Area::M => 0x800 + d.index,
        Area::D => unreachable!(),
    })
}
pub fn word_address(d: Device) -> Result<u16, String> {
    match d.area {
        Area::T if d.index < 256 => Ok(0x600 + d.index),
        Area::C if d.index < 256 => Ok(0xe00 + d.index),
        Area::D if d.index < 4096 => Ok(0x1000 + d.index),
        Area::D if d.index < 10000 => Ok(0x9000 + d.index - 4096),
        _ => Err("Unsupported ES2 word address".into()),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires digest-verified current COMMGR owned processes; run on private desktop"]
    fn indexed_random_gate_native_rejects_divergent_input_before_bad_target_and_preserves_off_skip()
    {
        use super::*;
        for path in [
            Path::new(DEFAULT_SIMULATOR),
            Path::new(DEFAULT_SS2_SIMULATOR),
        ] {
            let mut session = Session::launch(path, Arc::new(AtomicBool::new(false))).unwrap();
            assert!(matches!(
                session.digest.as_str(),
                COMMGR_ES2_SHA256 | COMMGR_SS2_SHA256
            ));
            let source = "LDI X1\nMOV K-1 E0\nLD X0\nRAND K7 K7 D0E0\nEND";
            session.upload(source).unwrap();
            let header = session.encoded.as_ref().unwrap().action_pc[1];
            // Deliberately diverge only an external input in this diagnostic.
            // No random value, seed, result register or math callback is written.
            session
                .bit_write(bit_address("X0".parse().unwrap()).unwrap(), true)
                .unwrap();
            let error = session.step(&[]).unwrap_err();
            assert!(
                error.to_ascii_lowercase().contains("gate")
                    || error.to_ascii_lowercase().contains("control"),
                "{error}"
            );
            assert_eq!(session.word_read(PC).unwrap(), header, "{error}");
            assert_eq!(
                session
                    .word_read(word_address("D1067".parse().unwrap()).unwrap())
                    .unwrap(),
                0
            );
            assert_eq!(session.verified_random_gates(), 0);
            println!(
                "{} divergent input rejected at header{} before invalid target: {}",
                session.target.name(),
                header,
                error
            );

            session.upload(source).unwrap();
            let off = session.step(&[("X0".parse().unwrap(), false)]).unwrap();
            assert!(off.rows.iter().all(|r| r.local == r.original));
            assert_eq!(session.verified_random_gates(), 1);
            assert_eq!(
                session
                    .word_read(word_address("D1067".parse().unwrap()).unwrap())
                    .unwrap(),
                0
            );
            println!(
                "{} matched OFF skips invalid target with actual gate checked",
                session.target.name()
            );

            session.upload("LD X0\nDRANDP K7 K7 D20E0\nEND").unwrap();
            for power in [false, true, true, false, false, true] {
                let s = session.step(&[("X0".parse().unwrap(), power)]).unwrap();
                assert!(s.rows.iter().all(|r| r.local == r.original));
            }
            assert_eq!(session.verified_random_gates(), 6);
            println!(
                "{} full current DRANDP gate/history lifecycle checked",
                session.target.name()
            );
        }
    }
    #[test]
    fn random_tail_digest_policy_preserves_current_and_legacy_qualification() {
        use crate::random_tail::{self, RandomTailProfile};
        for digest in [COMMGR_ES2_SHA256, COMMGR_SS2_SHA256] {
            assert_eq!(
                random_tail_profile(digest).unwrap(),
                RandomTailProfile::CurrentCommgr
            );
        }
        for digest in [TARGET_SHA256, SS2_SHA256] {
            assert_eq!(
                random_tail_profile(digest).unwrap(),
                RandomTailProfile::InstalledLegacyInertOnly
            );
        }
        assert!(random_tail_profile("unknown-image").is_err());
        for dest in ["D20E0", "D122F0", "D125F0", "K1M253F7"] {
            let p = plc::compile(&format!("LD X0\nDRAND K7 K7 {dest}\nEND")).unwrap();
            assert!(
                random_tail::validate_program_profile(&p, RandomTailProfile::CurrentCommgr).is_ok()
            );
            assert!(
                random_tail::validate_program_profile(
                    &p,
                    RandomTailProfile::InstalledLegacyInertOnly
                )
                .is_err()
            );
        }
        let p = plc::compile("LD X0\nDRAND D0E0 K7 F0\nEND").unwrap();
        assert!(
            random_tail::validate_program_profile(&p, RandomTailProfile::InstalledLegacyInertOnly)
                .is_ok()
        );
    }
    #[test]
    #[ignore = "requires digest-verified COMMGR 2.11 ES2 and SS2 owned processes"]
    fn m1019_native_cold_stop_stepped_run_storage_lifecycle() {
        use super::*;
        use crate::plc::Memory;
        fn wait_running(session: &mut Session, expected: bool) {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                let running = session.bit_read(Memory::system_device(1000)).unwrap();
                let stopped = session.bit_read(Memory::system_device(1001)).unwrap();
                if running == expected && stopped != expected {
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "RUN/STOP transition was not observed"
                );
                thread::sleep(Duration::from_millis(10));
            }
        }
        let es2 = Path::new(DEFAULT_SS2_SIMULATOR).with_file_name("DVPSimulator_ES2.exe");
        for path in [es2.as_path(), Path::new(DEFAULT_SS2_SIMULATOR)] {
            let mut session = Session::launch(path, Arc::new(AtomicBool::new(false))).unwrap();
            assert!(matches!(
                session.digest.as_str(),
                COMMGR_ES2_SHA256 | COMMGR_SS2_SHA256
            ));
            let source = "LD M1019\nOUT Y0\nEND";
            session.upload(source).unwrap();
            let control = Memory::system_device(1019);
            assert!(!session.bit_read(control).unwrap());
            assert_eq!(
                session.local.memory.power_monitor.backend(),
                crate::power_monitor::PowerBackend::InstalledStorageOnly
            );
            wait_running(&mut session, true);
            session
                .bit_write(bit_address(control).unwrap(), true)
                .unwrap();
            assert!(session.bit_read(control).unwrap());
            // STEP_MODE must be disabled for the real STOP cleanup path.
            // RUN stays false throughout: no freely running program is used.
            session.bit_write(RUN, false).unwrap();
            session.bit_write(STEP_MODE, false).unwrap();
            wait_running(&mut session, false);
            assert!(session.bit_read(control).unwrap());
            session.bit_write(STEP_MODE, true).unwrap();
            wait_running(&mut session, true);
            assert_eq!(session.word_read(PC).unwrap(), 0);
            assert!(session.bit_read(control).unwrap());
            session.upload(source).unwrap();
            assert!(!session.bit_read(control).unwrap());
            println!(
                "M1019 {} {}: cold OFF; explicit ON; real STOP retained; stepped RUN initialization PC0 retained; cold re-upload OFF; voltage/LED not tested",
                session.target.name(),
                session.digest
            );
        }
    }
    #[test]
    fn indexed_truncated_native_qualification_distinguishes_direct_e_from_its_d_alias() {
        use crate::{indexed_truncated::IndexedTruncatedAction, plc::Memory};
        let memory = Memory::default();
        for (source, eligible) in [
            ("MUL32 D5E0 K3 E1", false),
            ("DIV32 D5E0 K1 E1", false),
            ("DIV32 D5E0 K0 E1", true),
            ("MUL32 D5E0 K3 D1182E1", true),
            ("MUL16 D5E0 K3 E1", true),
            ("DIV32 K-2147483648 K-1 D20E0", false),
            ("DIV32 K-2147483648 K2 D20E0", true),
        ] {
            let parts = source.split_whitespace().collect::<Vec<_>>();
            let action = IndexedTruncatedAction::parse(
                parts[0],
                &parts[1..]
                    .iter()
                    .map(|s| (*s).to_owned())
                    .collect::<Vec<_>>(),
            )
            .unwrap();
            let prepared = action.prepare(&memory, &mut Default::default()).unwrap();
            assert_eq!(
                super::qualify_indexed_truncated_result(&prepared, &memory).is_ok(),
                eligible,
                "{source}"
            );
        }
    }
    #[test]
    #[ignore = "requires installed digest-verified SS2 simulator"]
    fn ss2_discovers_raw_program_in_fresh_owned_session() {
        let mut session = Session::launch(
            Path::new(DEFAULT_SS2_SIMULATOR),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let image = es2_encoder::encode(&plc::compile("LD X0\nOUT Y0\nEND").unwrap())
            .unwrap()
            .words;
        let mut request = vec![0, 0, 0, image.len() as u8, (image.len() * 2) as u8];
        request.extend(image.iter().flat_map(|word| word.to_be_bytes()));
        assert_eq!(
            session.request(0x75, &request).unwrap(),
            [0, 0, 0, image.len() as u8]
        );
        assert!(session.program.is_none() && session.encoded.is_none());
        assert_eq!(session.discover_program_words().unwrap(), image);
        assert_eq!(session.local.memory.native_numeric_scratch, Some(100));
    }
    #[test]
    #[ignore = "requires installed digest-verified SS2 simulator"]
    fn ss2_discovers_length_from_instruction_boundaries_without_upload_metadata() {
        let mut session = Session::launch(
            Path::new(DEFAULT_SS2_SIMULATOR),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let source = format!("LD X0\n{}OUT Y0\nEND\n", "MOV K123 D0\n".repeat(30));
        let encoded = es2_encoder::encode(&plc::compile(&source).unwrap()).unwrap();
        assert!(encoded.words.len() > 100 && encoded.words.len() < 200);
        assert_ne!(encoded.words.len() % 16, 0);
        session.upload(&source).unwrap();
        let writes: Vec<_> = session
            .trace()
            .iter()
            .filter(|e| e.function == 0x10)
            .collect();
        assert_eq!(writes.len(), encoded.words.len().div_ceil(16));
        for (chunk, (trace, words)) in writes.iter().zip(encoded.words.chunks(16)).enumerate() {
            let pdu =
                crate::plc_program_transfer::ss2_simulator_bank_write_request(chunk * 16, words)
                    .unwrap();
            assert_eq!(trace.request, pdu[1..]);
            assert_eq!(trace.response.as_deref(), Some(&pdu[1..5]));
        }
        assert!(!session.trace().iter().any(|e| e.function == 0x75));
        let original_program = session.program.take();
        let original_encoded = session.encoded.take();
        let before = session.trace().len();
        let words = session.discover_program_words().unwrap();
        assert_eq!(words, encoded.words);
        let reads = session.trace()[before..]
            .iter()
            .filter(|e| e.function == 0x74)
            .collect::<Vec<_>>();
        assert_eq!(reads.len(), 2);
        assert_eq!(reads[0].request, [0, 0, 0, 100]);
        assert_eq!(reads[1].request, [0, 100, 0, 100]);
        assert_eq!(session.local.memory.native_numeric_scratch, Some(200));
        session.program = original_program;
        session.encoded = original_encoded;
        let x0: Device = "X0".parse().unwrap();
        let y0: Device = "Y0".parse().unwrap();
        for input in [true, false] {
            let snapshot = session.step(&[(x0, input)]).unwrap();
            assert_eq!(snapshot.mismatches, 0);
            let output = snapshot
                .rows
                .iter()
                .find(|r| r.device == y0 && r.kind == "bit")
                .unwrap();
            assert_eq!(output.original, u32::from(input));
            assert_eq!(output.local, u32::from(input));
        }
    }

    #[test]
    #[ignore = "requires installed digest-verified SS2 simulator"]
    fn ss2_rejects_end_word_inside_malformed_operand() {
        let mut session = Session::launch(
            Path::new(DEFAULT_SS2_SIMULATOR),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        // A malformed MOV operand contains END's numeric value. The genuine
        // SS2 holds these bytes, but no execution or commit is attempted.
        let words = [0x0024u16, 0x2800, 0x0f00, 0x0080, 0x0086, 0x0086, 0x0f00];
        let mut request = vec![0, 0, 0, words.len() as u8, (words.len() * 2) as u8];
        request.extend(words.iter().flat_map(|word| word.to_be_bytes()));
        assert_eq!(
            session.request(0x75, &request).unwrap(),
            [0, 0, 0, words.len() as u8]
        );
        assert!(matches!(
            session.discover_program_words(),
            Err(ProgramDiscoveryError::Decode(_))
        ));
        assert!(!session.faulted, "decoder rejection is not an I/O failure");
        assert_eq!(session.local.memory.native_numeric_scratch, Some(100));
    }
    #[test]
    fn program_read_chunk_requires_exact_count_and_decodes_big_endian() {
        assert_eq!(
            decode_program_chunk(&[4, 0x12, 0x34, 0xab, 0xcd], 2).unwrap(),
            [0x1234, 0xabcd]
        );
        for invalid in [
            &[][..],
            &[0][..],
            &[2, 0x12][..],
            &[4, 0x12, 0x34][..],
            &[2, 0x12, 0x34, 0, 0][..],
        ] {
            assert!(decode_program_chunk(invalid, 2).is_err());
        }
        assert!(decode_program_chunk(&[], 0).is_err());
        assert!(decode_program_chunk(&[], 101).is_err());
    }

    #[test]
    #[ignore = "requires installed digest-verified SS2 simulator"]
    fn ss2_program_readback_uses_known_uploaded_length_and_roundtrips() {
        let mut session = Session::launch(
            Path::new(DEFAULT_SS2_SIMULATOR),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert!(session.read_program_words().is_err());
        assert!(!session.faulted);
        let source = format!("LD X0\n{}END\n", "MOV K123 D0\n".repeat(30));
        let encoded = es2_encoder::encode(&plc::compile(&source).unwrap()).unwrap();
        assert!(encoded.words.len() > 100);
        session.upload(&source).unwrap();
        let trace_start = session.trace().len();
        let words = session.read_program_words().unwrap();
        assert_eq!(words, encoded.words);
        let reads = session.trace()[trace_start..]
            .iter()
            .filter(|e| e.function == 0x74)
            .collect::<Vec<_>>();
        assert_eq!(reads.len(), words.len().div_ceil(100));
        assert_eq!(reads[0].request, [0, 0, 0, 100]);
        let decoded = crate::es2_program_decode::decode(&words).unwrap();
        assert_eq!(
            es2_encoder::encode(&plc::compile(&decoded).unwrap())
                .unwrap()
                .words,
            words
        );
        assert_eq!(
            session
                .step(&[("X0".parse().unwrap(), true)])
                .unwrap()
                .mismatches,
            0
        );
    }
    #[test]
    fn data_batches_preserve_gaps_window_boundary_and_exact_device_set() {
        let words = (3990..4200)
            .map(|index| Device {
                area: Area::D,
                index,
            })
            .chain([Device {
                area: Area::D,
                index: 4999,
            }])
            .collect::<Vec<_>>();
        let blocks = data_read_blocks(&words).unwrap();
        assert_eq!(
            blocks.iter().map(Vec::len).collect::<Vec<_>>(),
            [100, 6, 100, 4, 1]
        );
        assert_eq!(blocks.into_iter().flatten().collect::<Vec<_>>(), words);
        assert!(
            data_read_blocks(&[Device {
                area: Area::C,
                index: 200
            }])
            .is_err()
        );
    }
    #[test]
    #[ignore = "requires the installed digest-verified ES2 and SS2 processes"]
    fn native_rtc_wire_and_uncontrolled_clock_comparison_boundary() {
        for path in [DEFAULT_SIMULATOR, DEFAULT_SS2_SIMULATOR] {
            let mut session =
                Session::launch(Path::new(path), Arc::new(AtomicBool::new(false))).unwrap();
            let initial = session.read_rtc_shadow().unwrap();
            let seed = [24, 4, 2, 29, 12, 30, 0];
            session.write_rtc_shadow(seed).unwrap();
            let observed = session.read_rtc_shadow().unwrap();
            // A host-second sample can advance 0..59 seconds at this boundary.
            // Seeding second0 keeps that bounded update in the same minute.
            assert_eq!(observed[..6], seed[..6]);
            assert!(observed[6] <= 59);
            println!(
                "{} RTC: initial={initial:?}, explicit seed={seed:?}, observed={observed:?}; no frozen-clock claim",
                session.target.name()
            );
            for source in ["LD X0\nTRD D0\nEND", "LD X0\nHOUR K1 D0 M0\nEND"] {
                session.upload(source).unwrap();
                let error = session.step(&[("X0".parse().unwrap(), true)]).unwrap_err();
                assert!(error.contains("RTC/tick"), "{error}");
            }
        }
    }
    #[test]
    #[ignore = "requires the installed digest-verified ES2 and SS2 processes"]
    fn native_batched_data_reads_match_single_reads() {
        for path in [DEFAULT_SIMULATOR, DEFAULT_SS2_SIMULATOR] {
            let mut session =
                Session::launch(Path::new(path), Arc::new(AtomicBool::new(false))).unwrap();
            let mut source = String::from("LD X0\n");
            let words = (0..103)
                .chain(4090..4103)
                .map(|index| Device {
                    area: Area::D,
                    index,
                })
                .collect::<Vec<_>>();
            for d in &words {
                source.push_str(&format!(
                    "MOV H{:04X} {d}\n",
                    d.index.wrapping_mul(317).wrapping_add(0x8023)
                ));
            }
            source.push_str("END");
            session.upload(&source).unwrap();
            assert_eq!(
                session
                    .step(&[("X0".parse().unwrap(), true)])
                    .unwrap()
                    .mismatches,
                0
            );
            let batched = session.read_data_devices(&words).unwrap();
            for d in &words {
                assert_eq!(
                    batched[d],
                    session.word_read(word_address(*d).unwrap()).unwrap(),
                    "{d}"
                );
            }
            println!(
                "{}: {} D values match single reads across count100 and protocol window",
                session.target().name(),
                words.len()
            );
            let uploaded = session.upload("LD X0\nMOV D1039 D0\nEND").unwrap();
            assert_eq!(uploaded.mismatches, 0);
            assert_eq!(
                session
                    .step(&[("X0".parse().unwrap(), true)])
                    .unwrap()
                    .mismatches,
                0
            );
            session.upload("LD X0\nMOV K5 D1039\nEND").unwrap();
            assert!(
                session
                    .step(&[("X0".parse().unwrap(), true)])
                    .unwrap_err()
                    .contains("fixed scan control")
            );
            println!(
                "{}: fixed scan control read agrees; program override explicitly rejected as inconclusive",
                session.target().name()
            );
        }
    }
    #[test]
    fn drand_checks_each_disabled_tail_and_only_the_declared_enabled_skip() {
        let p = crate::plc::compile("LD X0\nDRAND K0 H7FFFFFFF D20\nMOV K123 D30\nEND").unwrap();
        let e = crate::es2_encoder::encode(&p).unwrap();
        assert_eq!(super::checked_next_instruction(&e, 1, 8).unwrap(), 2);
        assert_eq!(super::checked_next_instruction(&e, 1, 14).unwrap(), 8);
        assert!(super::checked_next_instruction(&e, 1, 9).is_err());
        for i in 2..8 {
            assert_eq!(
                super::checked_next_instruction(&e, i, e.next_pc[i]).unwrap(),
                i + 1
            );
        }
    }
    use super::*;
    #[test]
    fn ddivr_padding_accepts_only_its_declared_four_word_error_skip() {
        let e = crate::es2_encoder::encode(
            &plc::compile("LD X0\nDDIVRP D0 F2 D20\nMOV K7 D30\nOUT Y0\nEND").unwrap(),
        )
        .unwrap();
        assert_eq!(e.action_pc, [1, 18, 23]);
        assert_eq!(e.words[1], 0x7d11);
        assert_eq!(&e.words[14..18], &[0xfdf0; 4]);
        assert_eq!(checked_next_instruction(&e, 1, 14).unwrap(), 2);
        assert_eq!(checked_next_instruction(&e, 1, 18).unwrap(), 6);
        for actual in [15, 16, 17, 19, 23] {
            assert!(checked_next_instruction(&e, 1, actual).is_err());
        }
        assert!(checked_next_instruction(&e, 0, 18).is_err());
        assert!(checked_next_instruction(&e, 2, 18).is_err());
    }
    #[test]
    fn indexed_ddivr_padding_accepts_only_its_declared_four_word_error_skip() {
        for name in ["DDIVR", "DDIVRP"] {
            let e = crate::es2_encoder::encode(
                &plc::compile(&format!(
                    "LD X0\n{name} D0E0 F2 D20F7\nMOV K7 D30\nOUT Y0\nEND"
                ))
                .unwrap(),
            )
            .unwrap();
            assert_eq!(e.action_pc, [1, 18, 23]);
            assert_eq!(&e.words[14..18], &[0xfdf0; 4]);
            assert_eq!(checked_next_instruction(&e, 1, 14).unwrap(), 2);
            assert_eq!(checked_next_instruction(&e, 1, 18).unwrap(), 6);
            for actual in [15, 16, 17, 19, 23] {
                assert!(
                    checked_next_instruction(&e, 1, actual).is_err(),
                    "{name} PC {actual}"
                );
            }
            assert!(checked_next_instruction(&e, 0, 18).is_err());
            assert!(checked_next_instruction(&e, 2, 18).is_err());
        }
    }
    #[test]
    fn orz_padding_allows_only_the_exact_compiler_declared_shortcut() {
        let e = crate::es2_encoder::encode(
            &plc::compile("LD X0\nORZ>= K1 K2 K0\nOUT Y0\nEND").unwrap(),
        )
        .unwrap();
        assert_eq!(e.action_pc, [14]);
        assert_eq!(e.words[1], 0x8a02);
        assert_eq!(e.words[8..14], [0xfdf0; 6]);
        assert_eq!(checked_next_instruction(&e, 1, 8).unwrap(), 2);
        assert_eq!(checked_next_instruction(&e, 1, 14).unwrap(), 8);
        assert!(checked_next_instruction(&e, 1, 15).is_err());
        assert!(checked_next_instruction(&e, 0, 14).is_err());
        assert!(checked_next_instruction(&e, 2, 14).is_err());
        // Padding expansion beyond the old 512-instruction cap still encodes; the real
        // budget is the program region.
        let large = format!("{}END", "LD X0\nORZ= K1 K1 K0\nOUT Y0\n".repeat(60));
        let encoded = crate::es2_encoder::encode(&plc::compile(&large).unwrap()).unwrap();
        assert!(encoded.next_pc.len() > 512);
    }
    #[test]
    fn ss2_validates_all_wide_source_and_result_words() {
        for body in ["DMOV D4999 D0", "DMUL K1 K2 D4997", "DFMOV K1 D4998 K2"] {
            assert!(
                plc::compile(&format!("LD X0\n{body}\nEND")).is_err() || {
                    let p = plc::compile(&format!("LD X0\n{body}\nEND")).unwrap();
                    Target::Ss2.validate_program(&p).is_err()
                }
            );
        }
        let p = plc::compile("LD X0\nDMUL K1 K2 D4996\nDLD= C255 K0\nOUT Y0\nEND").unwrap();
        assert!(Target::Ss2.validate_program(&p).is_ok());
        assert_eq!(word_address("C200".parse().unwrap()).unwrap(), 0xec8);
        assert_eq!(word_address("C255".parse().unwrap()).unwrap(), 0xeff);
    }
    #[test]
    fn ss2_rejects_bulk_implicit_word_crossing_before_upload() {
        for command in [
            "BMOV D4998 D0 K3",
            "BMOV D0 D4998 K3",
            "FMOV K1 D4998 K3",
            "BMOV D4998 D20E0 K3",
            "BMOV D0E0 D4998 K3",
            "WSFR D4998 D20E0 K3 K3",
            "WSFL D0E0 D4998 K3 K1",
            "SFWR D0E0 D4998 K3",
            "SFRD D4998 D20F0 K3",
        ] {
            let p = plc::compile(&format!("LD X0\n{command}\nEND")).unwrap();
            assert!(
                Target::Ss2
                    .validate_program(&p)
                    .unwrap_err()
                    .contains("D5000")
            );
            assert!(Target::Es2.validate_program(&p).is_ok());
        }
    }
    #[test]
    fn address_boundaries() {
        assert_eq!(bit_address("X10".parse().unwrap()).unwrap(), 0x408);
        assert_eq!(bit_address("M2000".parse().unwrap()).unwrap(), 0xb1d0);
        assert_eq!(word_address("D4096".parse().unwrap()).unwrap(), 0x9000);
        assert_eq!(word_address("D9999".parse().unwrap()).unwrap(), 0xa70f);
        assert!(word_address("X0".parse().unwrap()).is_err());
    }
    #[test]
    fn unrelated_executable_is_rejected() {
        let cancel = Arc::new(AtomicBool::new(false));
        let path = std::env::current_exe().unwrap();
        assert!(
            Session::launch(&path, cancel)
                .err()
                .unwrap()
                .contains("Unsupported ES2/SS2 binary")
        );
    }
    #[test]
    fn target_identity_uses_exact_digest() {
        assert_eq!(
            Target::from_image(TARGET_SHA256, 1_715_200),
            Ok(Target::Es2)
        );
        assert_eq!(Target::from_image(SS2_SHA256, 1_715_200), Ok(Target::Ss2));
        assert_eq!(
            Target::from_image(COMMGR_ES2_SHA256, 1_775_528),
            Ok(Target::Es2)
        );
        assert_eq!(
            Target::from_image(COMMGR_SS2_SHA256, 1_735_080),
            Ok(Target::Ss2)
        );
        assert!(Target::from_image(COMMGR_SS2_SHA256, 1_715_200).is_err());
        assert!(Target::from_image(COMMGR_ES2_SHA256, 1_735_080).is_err());
        assert!(Target::from_image(&"0".repeat(64), 1_715_200).is_err());
    }
    #[test]
    fn decimal_scratch_behavior_is_bound_to_exact_binary_digest() {
        use crate::float_math::DecimalScratchProfile;
        assert_eq!(
            decimal_scratch_profile(COMMGR_ES2_SHA256),
            Ok(DecimalScratchProfile::CurrentPrivate)
        );
        assert_eq!(
            compare_table_profile(COMMGR_SS2_SHA256),
            Ok(CompareTableProfile::CurrentSs2)
        );
        for digest in [TARGET_SHA256, SS2_SHA256] {
            assert_eq!(
                decimal_scratch_profile(digest),
                Ok(DecimalScratchProfile::LegacyVisible)
            );
        }
        assert!(decimal_scratch_profile(&"0".repeat(64)).is_err());
    }
    #[test]
    fn comparison_behavior_is_bound_to_exact_binary_digest() {
        assert_eq!(
            compare_table_profile(COMMGR_ES2_SHA256),
            Ok(CompareTableProfile::CurrentEs2)
        );
        // The current COMMGR SS2 image has its own indexed dispatch (additional_processing).
        assert_eq!(
            compare_table_profile(COMMGR_SS2_SHA256),
            Ok(CompareTableProfile::CurrentSs2)
        );
        for digest in [TARGET_SHA256, SS2_SHA256] {
            assert_eq!(
                compare_table_profile(digest),
                Ok(CompareTableProfile::InstalledLegacy)
            );
        }
        assert!(compare_table_profile(&"0".repeat(64)).is_err());
    }
    #[test]
    fn teaching_tick_addresses_are_bound_to_exact_legacy_binaries() {
        assert_eq!(teaching_tick_address(TARGET_SHA256), Ok(0x53acd8));
        assert_eq!(teaching_tick_address(SS2_SHA256), Ok(0x5347a8));
        assert!(teaching_tick_address(COMMGR_ES2_SHA256).is_err());
        assert!(teaching_tick_address(COMMGR_SS2_SHA256).is_err());
        assert!(teaching_tick_address(&"0".repeat(64)).is_err());
    }
    #[test]
    fn transform_capabilities_follow_verified_digest_not_model_name() {
        for digest in [TARGET_SHA256, SS2_SHA256, COMMGR_SS2_SHA256] {
            let profile = transform_profile(digest).unwrap();
            assert!((30..=36).all(|mode| !profile.supports(mode)));
            assert!(!profile.supports(15));
            assert!(profile.supports(7));
        }
        let current = transform_profile(COMMGR_ES2_SHA256).unwrap();
        assert!((30..=36).all(|mode| current.supports(mode) && current.supports(0x0100 | mode)));
        assert!(current.supports(15) && current.supports(0x010f));
        assert!([10, 39, 52].into_iter().all(|mode| !current.supports(mode)));
        assert!(transform_profile(&"0".repeat(64)).is_err());
    }
    #[test]
    #[ignore = "requires both installed Delta editor and COMMGR simulator binaries"]
    fn installed_binary_identity_and_replay_capability_match_exact_digest() {
        let cases = [
            (
                r"C:\Program Files (x86)\Delta Industrial Automation\WPLSoft 2.53\Simulator\DVPSimulator_ES2.exe",
                Target::Es2,
                TARGET_SHA256,
                true,
            ),
            (
                r"C:\Program Files (x86)\Delta Industrial Automation\WPLSoft 2.53\Simulator\DVPSimulator_SS2.exe",
                Target::Ss2,
                SS2_SHA256,
                true,
            ),
            (DEFAULT_SIMULATOR, Target::Es2, COMMGR_ES2_SHA256, false),
            (DEFAULT_SS2_SIMULATOR, Target::Ss2, COMMGR_SS2_SHA256, false),
        ];
        for (path, target, digest, replay_supported) in cases {
            let mut session =
                Session::launch(Path::new(path), Arc::new(AtomicBool::new(false))).unwrap();
            assert_eq!(session.target(), target);
            assert_eq!(session.sha256(), digest);
            assert_eq!(
                session.set_teaching_tick_replay(Some(1)).is_ok(),
                replay_supported
            );
            assert!(session.set_teaching_tick_replay(None).is_ok());
        }
    }
    #[test]
    fn ss2_simulator_word_limit_is_target_specific() {
        assert!(Target::Ss2.validate_program_word_count(0x1ef0).is_ok());
        assert!(Target::Ss2.validate_program_word_count(0x1ef1).is_err());
        assert!(Target::Ss2.validate_program_word_count(0).is_err());
        assert!(Target::Es2.validate_program_word_count(0x1ef1).is_ok());
        assert!(
            Target::Es2
                .validate_program_word_count(es2_encoder::MAX_PROGRAM_WORDS)
                .is_ok()
        );
        assert!(
            Target::Es2
                .validate_program_word_count(es2_encoder::MAX_PROGRAM_WORDS + 1)
                .is_err()
        );
        assert_eq!(Target::Ss2.program_word_limit() % 100, 20);
    }
    #[test]
    fn ss2_validates_implicit_second_data_word_before_upload() {
        let invalid = plc::compile("LD X0\nMUL K1 K2 D4999\nEND").unwrap();
        assert!(
            Target::Ss2
                .validate_program(&invalid)
                .unwrap_err()
                .contains("D5000")
        );
        assert!(Target::Es2.validate_program(&invalid).is_ok());
        let valid = plc::compile("LD X0\nDIV K1 K2 D4998\nEND").unwrap();
        assert!(Target::Ss2.validate_program(&valid).is_ok());
    }
    #[test]
    fn ss2_rejects_out_of_range_d_before_upload() {
        let invalid = plc::compile("LD X0\nMOV K1 D5000\nEND").unwrap();
        assert!(
            Target::Ss2
                .validate_program(&invalid)
                .unwrap_err()
                .contains("D5000")
        );
        assert!(Target::Es2.validate_program(&invalid).is_ok());
        let valid = plc::compile("LD X0\nMOV K1 D4999\nEND").unwrap();
        assert!(Target::Ss2.validate_program(&valid).is_ok());
    }
    #[test]
    fn ss2_validates_each_zone_comparison_source_before_upload() {
        for args in ["D5000 K1 K0", "K0 D5000 K1", "K0 K1 D5000"] {
            let p = plc::compile(&format!("LD X0\nZCPP {args} M0\nEND")).unwrap();
            assert!(
                Target::Ss2
                    .validate_program(&p)
                    .unwrap_err()
                    .contains("D5000")
            );
            assert!(Target::Es2.validate_program(&p).is_ok());
        }
        let p = plc::compile("LD X0\nZCP D4999 D4999 D4999 M4093\nEND").unwrap();
        assert!(Target::Ss2.validate_program(&p).is_ok());
    }
}
