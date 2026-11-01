//! `opendvp --mcp`: an MCP (stdio) server that lets an AI assistant write, check, simulate and
//! (with explicit confirmation) download Delta DVP programs through OpenDVP's own library.
//!
//! State lives in the server process: one open project and one offline simulator.
//! Tools that change a real PLC (`plc_download`, `plc_set_run`) refuse unless `confirm` is true.
#![allow(dead_code)]
use async_trait::async_trait;
use opendvp::plc::{Area, Device, Simulator};
use opendvp::project::{Project, SourceLanguage, Symbol, UnitKind};
use rust_mcp_sdk::{
    McpServer, RequestContext, ServerDetails, StdioTransport, ToMcpServerHandler, TransportOptions,
    error::SdkResult,
    macros,
    mcp_server::{McpServerOptions, ServerHandler, server_runtime},
    schema::*,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::{instruction_help, plc_connection};

type Res<T> = std::result::Result<T, String>;

// ---------------------------------------------------------------- tool definitions

macro_rules! tool {
    ($name:tt, $desc:tt, $hint:ident, $ty:ident { $($(#[$m:meta])* $f:ident : $t:ty),* $(,)? }) => {
        #[macros::mcp_tool(name = $name, description = $desc, $hint = true)]
        #[derive(Debug, Default, ::serde::Deserialize, ::serde::Serialize, macros::JsonSchema)]
        pub struct $ty { $($(#[$m])* pub $f: $t),* }
    };
}

// Reference
tool!("search_instructions", "Search OpenDVP's instruction guide (Turkish text) by mnemonic or keyword, e.g. 'MOV', 'PLSY', 'timer'. Use before writing unfamiliar instructions.", read_only_hint, SearchInstructions {
    /// Mnemonic or keyword.
    query: String,
});
tool!("list_models", "List the PLC models OpenDVP knows (id, family, program steps, I/O).", read_only_hint, ListModels {});
tool!("list_serial_ports", "List serial/USB ports on this machine (for the PLC tools).", read_only_hint, ListSerialPorts {});

// Stateless compile / codec
tool!("compile_il", "Compile DVP instruction-list (IL) source and report success or the first diagnostic (line + message). Does not touch the open project.", read_only_hint, CompileIl {
    /// IL program text (max 128 KiB), END-terminated.
    source: String,
});
tool!("compile_st", "Compile Structured Text (ST) source and report success or the first diagnostic. Does not touch the open project.", read_only_hint, CompileSt {
    /// ST program text (max 128 KiB).
    source: String,
});
tool!("il_to_words", "Compile IL and return the ES2 program words OpenDVP would download (hex, END included).", read_only_hint, IlToWords {
    /// IL program text.
    source: String,
});
tool!("words_to_il", "Decode an ES2 program image (hex words separated by spaces/commas) back to IL.", read_only_hint, WordsToIl {
    /// 16-bit words in hex, e.g. "0024 00C5 0F00".
    words: String,
});
tool!("read_dvz", "Read a local .dvz package: model, memory section sizes and the program decoded to IL.", read_only_hint, ReadDvz {
    /// Path of the .dvz file.
    path: String,
});

// Project
tool!("project_new", "Create a new empty project and make it the open project (replaces the current one, unsaved changes are lost).", idempotent_hint, ProjectNew {
    /// Project name.
    name: Option<String>,
    /// "il" (default) or "st".
    language: Option<String>,
    /// Model id from list_models (default ES2).
    device: Option<String>,
});
tool!("project_open", "Open an OpenDVP project (.odvp), .dvp (Delta editor), .dvz, .il or .st file as the open project.", idempotent_hint, ProjectOpen {
    /// File path.
    path: String,
});
tool!("project_save", "Save the open project as .odvp (overwrites the file).", idempotent_hint, ProjectSave {
    /// Target path; defaults to the path it was opened/saved from.
    path: Option<String>,
});
tool!("window_project_pull", "Copy the project that is open in the OpenDVP window into this session (replaces the session's project). Only works while the OpenDVP window is running.", idempotent_hint, WindowProjectPull {});
tool!("window_project_push", "Send this session's project to the OpenDVP window, which replaces its open project with it (the window keeps a recovery copy of what it had). Only works while the OpenDVP window is running and not busy.", destructive_hint, WindowProjectPush {});
tool!("project_info", "Describe the open project: name, model, language, programs, symbols, comments, compile status.", read_only_hint, ProjectInfo {});
tool!("project_export_dvp", "Export the open project as a Delta editor .dvp file (ES2 only).", idempotent_hint, ProjectExportDvp {
    /// Target path.
    path: String,
});
tool!("get_program", "Return the source of one program of the open project (0 = Main, 1.. = units).", read_only_hint, GetProgram {
    /// Program index, default 0.
    index: Option<u32>,
});
tool!("set_program", "Replace the source of one program of the open project, then report whether the whole project still compiles. IL sources must end with END (units: SRET/IRET before END as appropriate).", idempotent_hint, SetProgram {
    /// Program index, default 0 (Main).
    index: Option<u32>,
    /// New source text.
    source: String,
});
tool!("add_unit", "Add a program unit: 'subroutine' (CALL P<n>), 'interrupt' (I<nnn>) or 'cyclic' (extra cyclic program).", destructive_hint, AddUnit {
    /// subroutine | interrupt | cyclic
    kind: String,
});
tool!("remove_unit", "Remove a program unit by program index (1..).", destructive_hint, RemoveUnit {
    /// Program index of the unit (>= 1).
    index: u32,
});
tool!("set_symbol", "Create or replace a named device (symbol) usable in IL operands and ST code.", idempotent_hint, SetSymbol {
    /// Symbol name (letters, digits, underscore).
    name: String,
    /// bool | int | dint | word | dword | real
    data_type: String,
    /// Device such as Y0 or D100; omit to let the allocator pick an address.
    device: Option<String>,
    /// Comment text.
    comment: Option<String>,
    /// Initial value (written to the PLC only if requested at download).
    init: Option<String>,
    /// Keep value through power-off (latched area).
    retain: Option<bool>,
});
tool!("remove_symbol", "Remove a symbol by name.", destructive_hint, RemoveSymbol {
    /// Symbol name.
    name: String,
});
tool!("set_comment", "Set (or with empty text remove) the comment of a device such as X0 or D100.", idempotent_hint, SetComment {
    /// Device.
    device: String,
    /// Comment text.
    text: String,
});
tool!("project_compile", "Compile the whole open project. Returns instruction count and devices used, or the diagnostic with program index and line.", read_only_hint, ProjectCompile {});
tool!("project_words", "Compile the open project and return the ES2 words (hex) plus the generated IL listing.", read_only_hint, ProjectWords {});

// Simulator
tool!("sim_reset", "Recompile the open project and start a fresh offline simulator (all devices zero).", idempotent_hint, SimReset {});
tool!("sim_set", "Set simulator devices before/between scans. Bits: X (input), Y/M/S; words: T/C/D. Example {\"X0\":true,\"D0\":100}.", idempotent_hint, SimSet {
    /// Map device -> value (true/false/0/1 or integer).
    values: serde_json::Map<String, Value>,
});
tool!("sim_scan", "Run PLC scans in the simulator (each scan advances time by scan_ms) and return the requested devices.", idempotent_hint, SimScan {
    /// Number of scans, default 1, max 100000.
    count: Option<u32>,
    /// Milliseconds per scan, default 20.
    scan_ms: Option<u64>,
    /// Devices to read afterwards, e.g. ["Y0","T0","D0"].
    read: Option<Vec<String>>,
});
tool!("sim_read", "Read simulator devices (bits as true/false; T/C/D as values).", read_only_hint, SimRead {
    /// Devices, e.g. ["X0","Y0","D0"].
    devices: Vec<String>,
});

// PLC
tool!("plc_probe", "Read-only: identify the PLC (model, capacity), RUN state, password and PLC-ID status.", read_only_hint, PlcProbe { connection: Connection });
tool!("plc_status", "Read-only: RUN state, error flag/code/step, scan times, clock.", read_only_hint, PlcStatus { connection: Connection });
tool!("plc_read_program", "Read-only: upload the program from the PLC and decode it to IL (does not change the open project).", read_only_hint, PlcReadProgram { connection: Connection });
tool!("plc_scan_modules", "Read-only: list the extension modules the PLC reports.", read_only_hint, PlcScanModules { connection: Connection });
tool!("plc_download", "WRITES TO A REAL PLC: stops it, writes the open project's compiled program, verifies by read-back, optionally restarts it. Refuses unless confirm=true; ask the user first and run plc_probe before.", destructive_hint, PlcDownload {
    connection: Connection,
    /// Must be true; set only after the user explicitly approved this download.
    confirm: bool,
    /// Switch the PLC to RUN after a verified download (default false).
    run_after: Option<bool>,
    /// 4-character PLC password if the PLC is locked (sent once, never retried).
    password: Option<String>,
    /// PLC ID the project expects.
    plc_id: Option<String>,
    /// Also write the symbols' initial values (default false).
    write_initial: Option<bool>,
});
tool!("plc_set_run", "WRITES TO A REAL PLC: switch RUN/STOP (M1072). Refuses unless confirm=true; this can start machinery.", destructive_hint, PlcSetRun {
    connection: Connection,
    /// true = RUN, false = STOP.
    run: bool,
    /// Must be true; set only after the user explicitly approved.
    confirm: bool,
});

// Ladder networks (IL programs; a network is one rung: contacts + outputs)
tool!("list_networks", "List the ladder networks (rungs) of a program: index, source lines, comment and the IL text of each. Network indexes are 0-based.", read_only_hint, ListNetworks {
    /// Program index, default 0 (Main).
    program: Option<u32>,
});
tool!("insert_network", "Insert a ladder network written as IL (e.g. LD X0 / OR Y0 / ANI X1 / OUT Y0, one instruction per line) after network `after` (omit = append before END). Optional comment becomes the rung title. Rejected, project unchanged, if the program would not compile.", idempotent_hint, InsertNetwork {
    /// IL of the new network(s), without END.
    il: String,
    /// Insert after this network index; omit to append at the end.
    after: Option<u32>,
    /// Rung comment (first line = title).
    comment: Option<String>,
    /// Program index, default 0.
    program: Option<u32>,
});
tool!("replace_network", "Replace network `index` with new IL (keeps its comment unless `comment` is given).", idempotent_hint, ReplaceNetwork {
    /// Network index.
    index: u32,
    /// New IL for that network, without END.
    il: String,
    /// New comment (empty string removes it); omit to keep.
    comment: Option<String>,
    /// Program index, default 0.
    program: Option<u32>,
});
tool!("delete_network", "Delete network `index` (with its comment).", destructive_hint, DeleteNetwork {
    /// Network index.
    index: u32,
    /// Program index, default 0.
    program: Option<u32>,
});
tool!("move_network", "Move network `index` one position up or down.", idempotent_hint, MoveNetwork {
    /// Network index.
    index: u32,
    /// "up" or "down".
    direction: String,
    /// Program index, default 0.
    program: Option<u32>,
});
tool!("set_network_comment", "Set (or with empty text remove) the comment of network `index` (first line = title).", idempotent_hint, SetNetworkComment {
    /// Network index.
    index: u32,
    /// Comment text.
    comment: String,
    /// Program index, default 0.
    program: Option<u32>,
});

// PLC forcing (real PLC)
tool!("plc_force", "WRITES TO A REAL PLC: force an X or Y device ON/OFF (the PLC holds the state until released) or release it. Refuses ON/OFF unless confirm=true; forcing outputs can start machinery. Protocol is unit-tested only, not verified on hardware.", destructive_hint, PlcForce {
    connection: Connection,
    /// Device, X0..X255 or Y0..Y255.
    device: String,
    /// "on", "off" or "release".
    mode: String,
    /// Must be true for on/off; set only after the user explicitly approved.
    confirm: Option<bool>,
});
tool!("plc_release_forces", "Release every forced X/Y device in the PLC.", destructive_hint, PlcReleaseForces { connection: Connection });

/// How to reach the PLC. TCP: set host (+ tcp_port, default 502). Serial: set port_name.
#[derive(Debug, Default, ::serde::Deserialize, ::serde::Serialize, macros::JsonSchema)]
pub struct Connection {
    /// "tcp" or "serial" (default: tcp if host is set, else serial).
    pub transport: Option<String>,
    /// IPv4/IPv6 address for TCP.
    pub host: Option<String>,
    pub tcp_port: Option<u16>,
    /// Serial port name from list_serial_ports, e.g. COM3.
    pub port_name: Option<String>,
    /// Default 9600.
    pub baud: Option<u32>,
    /// "ascii" (default for serial) or "rtu".
    pub protocol: Option<String>,
    /// Modbus station, default 1.
    pub station: Option<u8>,
    /// Timeout in ms (250-5000), default 1000.
    pub timeout_ms: Option<u64>,
}

impl Connection {
    fn config(&self) -> Res<plc_connection::Config> {
        use plc_connection::{Config, Parity, Protocol, Transport};
        let tcp = match self.transport.as_deref().map(str::to_ascii_lowercase).as_deref() {
            Some("tcp") => true,
            Some("serial") | Some("usb") => false,
            None => self.host.is_some(),
            Some(other) => return Err(format!("unknown transport: {other}")),
        };
        let mut config = Config::default();
        if let Some(t) = self.timeout_ms {
            config.timeout_ms = t;
        }
        if let Some(s) = self.station {
            config.station = s;
        }
        if tcp {
            config.transport = Transport::Tcp;
            config.protocol = Protocol::Tcp;
            config.host = self.host.clone().ok_or("host is required for tcp")?;
            if let Some(p) = self.tcp_port {
                config.tcp_port = p;
            }
        } else {
            config.transport = Transport::Serial;
            config.port_name = self.port_name.clone().ok_or("port_name is required for serial")?;
            if let Some(b) = self.baud {
                config.baud = b;
            }
            match self.protocol.as_deref().map(str::to_ascii_lowercase).as_deref() {
                None | Some("ascii") => {}
                Some("rtu") => {
                    config.protocol = Protocol::Rtu;
                    config.data_bits = 8;
                    config.parity = Parity::None;
                }
                Some(other) => return Err(format!("unknown protocol: {other}")),
            }
        }
        config.validate()?;
        Ok(config)
    }
}

// ---------------------------------------------------------------- server state

struct State {
    project: Project,
    path: Option<PathBuf>,
    sim: Option<(Simulator, opendvp::plc::Program)>,
}

impl State {
    fn compile(&mut self) -> Res<opendvp::plc::Program> {
        self.project.assign_symbol_addresses()?;
        self.project.compile_located().map_err(|l| {
            format!(
                "program {} ({}), line {}: {}",
                l.program,
                self.project.program_name(l.program),
                l.line,
                l.message
            )
        })
    }
    fn words(&mut self) -> Res<Vec<u16>> {
        let program = self.compile()?;
        Ok(opendvp::es2_encoder::encode(&program)?.words)
    }
}

fn args<T: for<'de> Deserialize<'de>>(params: &CallToolRequestParams) -> Res<T> {
    let map = params.arguments.clone().unwrap_or_default();
    serde_json::from_value(Value::Object(map)).map_err(|e| format!("invalid arguments: {e}"))
}

fn hex(words: &[u16]) -> String {
    words.iter().map(|w| format!("{w:04X}")).collect::<Vec<_>>().join(" ")
}

fn parse_words(text: &str) -> Res<Vec<u16>> {
    text.split(|c: char| c.is_whitespace() || c == ',')
        .filter(|t| !t.is_empty())
        .map(|t| {
            let t = t.trim_start_matches("0x").trim_start_matches("0X");
            u16::from_str_radix(t, 16).map_err(|_| format!("not a 16-bit hex word: {t}"))
        })
        .collect()
}

fn diag(d: opendvp::plc::Diagnostic) -> String {
    format!("line {}: {}", d.line, d.message)
}

fn device(text: &str) -> Res<Device> {
    text.parse::<Device>()
}

fn language(text: &str) -> Res<SourceLanguage> {
    match text.to_ascii_lowercase().as_str() {
        "il" => Ok(SourceLanguage::Il),
        "st" => Ok(SourceLanguage::St),
        other => Err(format!("language must be il or st, got {other}")),
    }
}

fn data_type(text: &str) -> Res<opendvp::st::DataType> {
    use opendvp::st::DataType::*;
    Ok(match text.to_ascii_lowercase().as_str() {
        "bool" => Bool,
        "int" => Int,
        "dint" => Dint,
        "word" => Word,
        "dword" => Dword,
        "real" => Real,
        other => return Err(format!("unknown data type: {other}")),
    })
}

fn json_text(value: Value) -> String {
    serde_json::to_string_pretty(&value).unwrap_or_default()
}

fn device_value(sim: &Simulator, text: &str) -> Res<Value> {
    let d = device(text)?;
    Ok(match d.area {
        Area::T | Area::C => json!({"value": sim.memory.word(d), "contact": sim.memory.bit(d)}),
        Area::D => json!(sim.memory.word(d)),
        _ => json!(sim.memory.bit(d)),
    })
}

fn truthy(v: &Value) -> Res<bool> {
    match v {
        Value::Bool(b) => Ok(*b),
        Value::Number(n) => Ok(n.as_i64().unwrap_or(0) != 0),
        Value::String(s) => match s.to_ascii_lowercase().as_str() {
            "1" | "on" | "true" => Ok(true),
            "0" | "off" | "false" => Ok(false),
            other => Err(format!("not a bit value: {other}")),
        },
        _ => Err("not a bit value".into()),
    }
}

fn project_summary(state: &mut State) -> Value {
    let compile = match state.compile() {
        Ok(program) => json!({"ok": true, "instructions": program.instructions}),
        Err(e) => json!({"ok": false, "error": e}),
    };
    let p = &state.project;
    let programs: Vec<Value> = (0..p.program_count())
        .map(|i| {
            json!({
                "index": i,
                "name": p.program_name(i),
                "language": p.program_language(i).label(),
                "lines": p.program_source(i).lines().count(),
            })
        })
        .collect();
    let symbols: Vec<Value> = p
        .symbols
        .iter()
        .map(|(n, s)| json!({"name": n, "type": s.ty.name(), "device": s.device, "comment": s.comment, "init": s.init, "retain": s.retain}))
        .collect();
    json!({
        "name": p.name, "device": p.device, "language": p.language.label(),
        "path": state.path.as_ref().map(|p| p.display().to_string()),
        "programs": programs, "symbols": symbols, "comments": p.comments, "compile": compile,
    })
}

// ---------------------------------------------------------------- dispatch

async fn run(state: &Mutex<State>, name: &str, params: &CallToolRequestParams) -> Res<String> {
    macro_rules! st {
        () => {
            state.lock().map_err(|_| "state poisoned".to_string())?
        };
    }
    match name {
        // ---- reference
        "search_instructions" => {
            let a: SearchInstructions = args(params)?;
            if a.query.trim().is_empty() {
                return Err("query is empty".into());
            }
            let q = instruction_help::query_for_mnemonic(a.query.trim()).to_ascii_lowercase();
            let hits: Vec<String> = instruction_help::GUIDE
                .iter()
                .filter(|(h, t)| h.to_ascii_lowercase().contains(&q) || t.to_ascii_lowercase().contains(&q))
                .take(8)
                .map(|(h, t)| format!("## {h}\n{t}"))
                .collect();
            if hits.is_empty() {
                Err(format!("no guide entry for '{}'", a.query))
            } else {
                Ok(hits.join("\n\n"))
            }
        }
        "list_models" => {
            let models: Vec<Value> = opendvp::devices::MODELS
                .iter()
                .map(|m| json!({"id": m.id, "title": m.title, "family": m.family, "steps": m.steps, "io": m.io, "comm": m.comm}))
                .collect();
            Ok(json_text(json!(models)))
        }
        "list_serial_ports" => {
            let ports = plc_connection::available_ports().await?;
            Ok(json_text(json!(ports.iter().map(|p| json!({"name": p.name, "label": p.label, "usb": p.usb})).collect::<Vec<_>>())))
        }

        // ---- stateless
        "compile_il" => {
            let a: CompileIl = args(params)?;
            opendvp::plc::compile_il(&a.source).map(|p| format!("OK: {} instructions", p.instructions)).map_err(diag)
        }
        "compile_st" => {
            let a: CompileSt = args(params)?;
            opendvp::plc::compile_st(&a.source).map(|p| format!("OK: {} instructions", p.instructions)).map_err(diag)
        }
        "il_to_words" => {
            let a: IlToWords = args(params)?;
            let p = opendvp::plc::compile_il(&a.source).map_err(diag)?;
            Ok(hex(&opendvp::es2_encoder::encode(&p)?.words))
        }
        "words_to_il" => {
            let a: WordsToIl = args(params)?;
            let w = parse_words(&a.words)?;
            opendvp::es2_program_decode::decode(&w).map_err(|e| format!("cannot decode: {e:?}"))
        }
        "read_dvz" => {
            let a: ReadDvz = args(params)?;
            let data = std::fs::read(&a.path).map_err(|e| e.to_string())?;
            let p = opendvp::dvz::read(&data)?;
            let il = opendvp::es2_program_decode::decode(&p.program)
                .unwrap_or_else(|e| format!("(IL could not be decoded: {e:?})"));
            Ok(format!(
                "model={:?} program={} words D={} C16={} C32={} T={} file_registers={}\n{il}",
                p.model_name(), p.program.len(), p.d.len(), p.c16.len(), p.c32.len(), p.t.len(), p.file_registers.len()
            ))
        }

        // ---- project
        "project_new" => {
            let a: ProjectNew = args(params)?;
            let mut p = Project::default();
            p.name = a.name.unwrap_or_else(|| "AI project".into());
            p.language = language(a.language.as_deref().unwrap_or("il"))?;
            if let Some(d) = a.device {
                if !opendvp::devices::MODELS.iter().any(|m| m.id == d) {
                    return Err(format!("unknown device model: {d}"));
                }
                p.device = d;
            }
            p.source = match p.language {
                SourceLanguage::Il => "END\n".into(),
                SourceLanguage::St => opendvp::project::ST_TEMPLATE.into(),
            };
            p.watch.clear();
            let mut s = st!();
            *s = State { project: p, path: None, sim: None };
            Ok(json_text(project_summary(&mut s)))
        }
        "project_open" => {
            let a: ProjectOpen = args(params)?;
            let path = PathBuf::from(&a.path);
            let p = Project::load(&path)?;
            let mut s = st!();
            *s = State { project: p, path: Some(path), sim: None };
            Ok(json_text(project_summary(&mut s)))
        }
        "window_project_pull" => {
            let json = WINDOW_PROJECT
                .lock()
                .ok()
                .and_then(|slot| slot.clone())
                .ok_or("no OpenDVP window is running (headless server): use project_open instead")?;
            let p: Project = serde_json::from_str(&json).map_err(|e| e.to_string())?;
            let path = WINDOW_PATH.lock().ok().and_then(|slot| slot.clone());
            let mut s = st!();
            *s = State { project: p, path, sim: None };
            Ok(json_text(project_summary(&mut s)))
        }
        "window_project_push" => {
            if WINDOW_PROJECT.lock().ok().is_none_or(|slot| slot.is_none()) {
                return Err("no OpenDVP window is running (headless server): use project_save instead".into());
            }
            let json = serde_json::to_string(&st!().project).map_err(|e| e.to_string())?;
            if let Ok(mut inbox) = WINDOW_INBOX.lock() {
                *inbox = Some(json);
            }
            Ok("sent: the window adopts the project within a second (it shows as unsaved there; save it from the window)".into())
        }
        "project_save" => {
            let a: ProjectSave = args(params)?;
            let mut s = st!();
            let path = a.path.map(PathBuf::from).or_else(|| s.path.clone()).ok_or("no path: pass `path`")?;
            s.project.save(&path)?;
            s.path = Some(path.clone());
            Ok(format!("saved {}", path.display()))
        }
        "project_info" => {
            let mut s = st!();
            Ok(json_text(project_summary(&mut s)))
        }
        "project_export_dvp" => {
            let a: ProjectExportDvp = args(params)?;
            let mut s = st!();
            s.project.assign_symbol_addresses()?;
            let skipped = s.project.export_dvp(Path::new(&a.path))?;
            Ok(format!("exported {} ({skipped} comment entries skipped)", a.path))
        }
        "get_program" => {
            let a: GetProgram = args(params)?;
            let s = st!();
            let i = a.index.unwrap_or(0) as usize;
            if i >= s.project.program_count() {
                return Err(format!("no program {i}; project has {}", s.project.program_count()));
            }
            Ok(s.project.program_source(i).to_string())
        }
        "set_program" => {
            let a: SetProgram = args(params)?;
            let mut s = st!();
            let i = a.index.unwrap_or(0) as usize;
            if i >= s.project.program_count() {
                return Err(format!("no program {i}; project has {}", s.project.program_count()));
            }
            if a.source.len() > 128 * 1024 {
                return Err("source exceeds 128 KiB".into());
            }
            s.project.set_program_source(i, a.source);
            s.sim = None;
            match s.compile() {
                Ok(p) => Ok(format!("program {i} updated; project compiles ({} instructions)", p.instructions)),
                Err(e) => Ok(format!("program {i} updated, but the project does not compile yet: {e}")),
            }
        }
        "add_unit" => {
            let a: AddUnit = args(params)?;
            let kind = match a.kind.to_ascii_lowercase().as_str() {
                "subroutine" => UnitKind::Subroutine,
                "interrupt" => UnitKind::Interrupt,
                "cyclic" => UnitKind::Cyclic,
                other => return Err(format!("kind must be subroutine, interrupt or cyclic, got {other}")),
            };
            let mut s = st!();
            let i = s.project.add_unit(kind)?;
            s.sim = None;
            Ok(format!("added program {i}: {}", s.project.program_name(i)))
        }
        "remove_unit" => {
            let a: RemoveUnit = args(params)?;
            let mut s = st!();
            let index = a.index as usize;
            if index == 0 || index >= s.project.program_count() {
                return Err("index must be an existing unit (>= 1)".into());
            }
            s.project.remove_unit(index - 1);
            s.sim = None;
            Ok("removed".into())
        }
        "set_symbol" => {
            let a: SetSymbol = args(params)?;
            let mut s = st!();
            let dev = match a.device {
                Some(d) => Some(device(&d)?.canonical()),
                None => None,
            };
            let auto = dev.is_none();
            s.project.set_symbol(
                &a.name,
                Symbol {
                    ty: data_type(&a.data_type)?,
                    device: dev,
                    auto,
                    retain: a.retain.unwrap_or(false),
                    comment: a.comment.unwrap_or_default(),
                    init: a.init,
                },
            )?;
            s.sim = None;
            Ok(format!("symbol {} set", a.name))
        }
        "remove_symbol" => {
            let a: RemoveSymbol = args(params)?;
            let mut s = st!();
            s.project.remove_symbol(&a.name);
            s.sim = None;
            Ok("removed".into())
        }
        "set_comment" => {
            let a: SetComment = args(params)?;
            st!().project.set_comment(device(&a.device)?, &a.text);
            Ok("ok".into())
        }
        "project_compile" => {
            let mut s = st!();
            let p = s.compile()?;
            Ok(json_text(json!({"ok": true, "instructions": p.instructions, "devices": p.devices.iter().map(|d| d.canonical()).collect::<Vec<_>>()})))
        }
        "project_words" => {
            let mut s = st!();
            let words = s.words()?;
            let il = s.project.instruction_list().map_err(diag)?;
            Ok(format!("{} words\n{}\n\n{il}", words.len(), hex(&words)))
        }

        // ---- simulator
        "sim_reset" => {
            let mut s = st!();
            let program = s.compile()?;
            s.sim = Some((Simulator::default(), program));
            Ok("simulator reset".into())
        }
        "sim_set" => {
            let a: SimSet = args(params)?;
            let mut s = st!();
            ensure_sim(&mut s)?;
            let (sim, _) = s.sim.as_mut().unwrap();
            for (key, value) in &a.values {
                let d = device(key)?;
                match d.area {
                    Area::X => sim.memory.set_input(d, truthy(value)?),
                    Area::Y | Area::M | Area::S => sim.memory.write_bit(d, truthy(value)?),
                    Area::T | Area::C | Area::D => {
                        let n = value.as_i64().ok_or_else(|| format!("{key}: integer expected"))?;
                        let n = i16::try_from(n)
                            .or_else(|_| u16::try_from(n).map(|u| u as i16))
                            .map_err(|_| format!("{key}: value out of 16-bit range"))?;
                        sim.memory.set_word(d, n)
                    }
                }
            }
            Ok("ok".into())
        }
        "sim_scan" => {
            let a: SimScan = args(params)?;
            let mut s = st!();
            ensure_sim(&mut s)?;
            let (sim, program) = s.sim.as_mut().unwrap();
            let count = a.count.unwrap_or(1).clamp(1, 100_000);
            let ms = a.scan_ms.unwrap_or(20).clamp(1, 1000);
            for _ in 0..count {
                sim.scan(program, ms);
            }
            let mut out = serde_json::Map::new();
            for t in a.read.unwrap_or_default() {
                out.insert(t.clone(), device_value(sim, &t)?);
            }
            Ok(json_text(json!({"scans": sim.scans, "elapsed_ms": sim.elapsed_ms, "status": format!("{:?}", sim.last_scan), "devices": out})))
        }
        "sim_read" => {
            let a: SimRead = args(params)?;
            let mut s = st!();
            ensure_sim(&mut s)?;
            let (sim, _) = s.sim.as_ref().unwrap();
            let mut out = serde_json::Map::new();
            for t in a.devices {
                out.insert(t.clone(), device_value(sim, &t)?);
            }
            Ok(json_text(Value::Object(out)))
        }

        // ---- PLC
        "plc_probe" => {
            let a: PlcProbe = args(params)?;
            let p = plc_connection::probe_plc(a.connection.config()?).await?;
            Ok(format!("{p:#?}"))
        }
        "plc_status" => {
            let a: PlcStatus = args(params)?;
            let p = plc_connection::plc_status(a.connection.config()?).await?;
            Ok(format!("{p:#?}"))
        }
        "plc_read_program" => {
            let a: PlcReadProgram = args(params)?;
            let (il, words) = plc_connection::read_plc_program(a.connection.config()?).await?;
            Ok(format!("{words} words\n{il}"))
        }
        "plc_scan_modules" => {
            let a: PlcScanModules = args(params)?;
            let m = plc_connection::scan_modules(a.connection.config()?).await?;
            Ok(format!("{m:#?}"))
        }
        "plc_download" => {
            let a: PlcDownload = args(params)?;
            if opendvp::settings::get().plc_read_only {
                return Err("refused: PLC writes are disabled (read-only mode in the OpenDVP settings)".into());
            }
            if !a.confirm {
                return Err("refused: plc_download writes to a real PLC. Ask the user, then call again with confirm=true.".into());
            }
            let config = a.connection.config()?;
            let (words, initial) = {
                let mut s = st!();
                let words = s.words()?;
                let initial = if a.write_initial.unwrap_or(false) {
                    s.project.initial_values()?.iter().map(|v| v.to_modbus()).collect::<Res<Vec<_>>>()?
                } else {
                    Vec::new()
                };
                (words, initial)
            };
            let report = plc_connection::download_program(config, words, a.run_after.unwrap_or(false), a.password, a.plc_id, initial).await?;
            Ok(format!("{report:#?}"))
        }
        "plc_set_run" => {
            let a: PlcSetRun = args(params)?;
            if opendvp::settings::get().plc_read_only {
                return Err("refused: PLC writes are disabled (read-only mode in the OpenDVP settings)".into());
            }
            if !a.confirm {
                return Err("refused: plc_set_run changes a real PLC. Ask the user, then call again with confirm=true.".into());
            }
            plc_connection::plc_set_run(a.connection.config()?, a.run).await?;
            Ok(if a.run { "PLC set to RUN" } else { "PLC set to STOP" }.into())
        }
        "plc_force" => {
            let a: PlcForce = args(params)?;
            let mode = match a.mode.to_ascii_lowercase().as_str() {
                "on" => opendvp::plc_force::Force::On,
                "off" => opendvp::plc_force::Force::Off,
                "release" => opendvp::plc_force::Force::Release,
                other => return Err(format!("mode must be on, off or release, got {other}")),
            };
            if mode != opendvp::plc_force::Force::Release && a.confirm != Some(true) {
                return Err("refused: forcing changes a real PLC. Ask the user, then call again with confirm=true.".into());
            }
            let d = device(&a.device)?;
            opendvp::plc_force::address(d)?;
            if mode != opendvp::plc_force::Force::Release && opendvp::settings::get().plc_read_only {
                return Err("refused: PLC writes are disabled (read-only mode in the OpenDVP settings)".into());
            }
            plc_connection::plc_force_device(a.connection.config()?, d, mode).await?;
            Ok(format!("{} {}", a.device.to_ascii_uppercase(), a.mode))
        }
        "plc_release_forces" => {
            let a: PlcReleaseForces = args(params)?;
            plc_connection::plc_release_forces(a.connection.config()?).await?;
            Ok("all forces released".into())
        }

        // ---- ladder networks
        "list_networks" => {
            let a: ListNetworks = args(params)?;
            let s = st!();
            let i = a.program.unwrap_or(0) as usize;
            let source = program_source(&s, i)?;
            let program = editable(&source)?;
            let lines: Vec<&str> = source.lines().collect();
            let items: Vec<Value> = program
                .networks
                .iter()
                .enumerate()
                .map(|(n, net)| {
                    let (first, last) = block(&lines, net);
                    json!({"index": n, "comment": net.comment, "il": lines[net.line - 1..net.end_line].join("\n"), "source_lines": [first + 1, last]})
                })
                .collect();
            Ok(json_text(json!(items)))
        }
        "insert_network" => {
            let a: InsertNetwork = args(params)?;
            edit_networks(state, a.program, |source, program| {
                let lines: Vec<&str> = source.lines().collect();
                let at = match a.after {
                    Some(n) => {
                        let net = program.networks.get(n as usize).ok_or("no such network")?;
                        block(&lines, net).1
                    }
                    None => lines.iter().position(|l| l.trim().eq_ignore_ascii_case("END")).ok_or("END not found")?,
                };
                let mut insert = vec![String::new()];
                insert.extend(comment_lines(a.comment.as_deref().unwrap_or("")));
                insert.extend(strip_end(&a.il));
                let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
                out.splice(at..at, insert);
                Ok(out.join("\n") + "\n")
            })
        }
        "replace_network" => {
            let a: ReplaceNetwork = args(params)?;
            edit_networks(state, a.program, |source, program| {
                let lines: Vec<&str> = source.lines().collect();
                let net = program.networks.get(a.index as usize).ok_or("no such network")?;
                let (start, end) = block(&lines, net);
                let mut insert = Vec::new();
                match &a.comment {
                    Some(c) => insert.extend(comment_lines(c)),
                    None => insert.extend(lines[start..net.line - 1].iter().map(|l| l.to_string())),
                }
                insert.extend(strip_end(&a.il));
                let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
                out.splice(start..end, insert);
                Ok(out.join("\n") + "\n")
            })
        }
        "delete_network" => {
            let a: DeleteNetwork = args(params)?;
            edit_networks(state, a.program, |source, program| {
                let lines: Vec<&str> = source.lines().collect();
                let net = program.networks.get(a.index as usize).ok_or("no such network")?;
                let (start, end) = block(&lines, net);
                let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
                out.drain(start..end);
                Ok(out.join("\n") + "\n")
            })
        }
        "move_network" => {
            let a: MoveNetwork = args(params)?;
            edit_networks(state, a.program, |source, program| {
                let index = a.index as usize;
                let other = match a.direction.to_ascii_lowercase().as_str() {
                    "up" => index.checked_sub(1).ok_or("network is already first")?,
                    "down" => Some(index + 1).filter(|&i| i < program.networks.len()).ok_or("network is already last")?,
                    d => return Err(format!("direction must be up or down, got {d}")),
                };
                let (first, second) = (index.min(other), index.max(other));
                let lines: Vec<&str> = source.lines().collect();
                let (a_start, a_end) = block(&lines, &program.networks[first]);
                let (b_start, b_end) = block(&lines, &program.networks[second]);
                let mut out: Vec<&str> = lines[..a_start].to_vec();
                out.extend(&lines[b_start..b_end]);
                out.extend(&lines[a_end..b_start]);
                out.extend(&lines[a_start..a_end]);
                out.extend(&lines[b_end..]);
                Ok(out.join("\n") + "\n")
            })
        }
        "set_network_comment" => {
            let a: SetNetworkComment = args(params)?;
            edit_networks(state, a.program, |source, program| {
                let lines: Vec<&str> = source.lines().collect();
                let net = program.networks.get(a.index as usize).ok_or("no such network")?;
                let (start, _) = block(&lines, net);
                let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
                out.splice(start..net.line - 1, comment_lines(&a.comment));
                Ok(out.join("\n") + "\n")
            })
        }
        other => Err(format!("unknown tool: {other}")),
    }
}

fn program_source(s: &State, i: usize) -> Res<String> {
    if i >= s.project.program_count() {
        return Err(format!("no program {i}; project has {}", s.project.program_count()));
    }
    Ok(s.project.program_source(i).to_string())
}

/// IL programs without flow control can be edited rung by rung.
fn editable(source: &str) -> Res<opendvp::plc::Program> {
    let program = opendvp::plc::compile(source).map_err(diag)?;
    if !program.graphically_editable() {
        return Err("this program uses flow control or ST; edit it with set_program".into());
    }
    Ok(program)
}

/// Source lines (0-based, end exclusive) of a network including the comment block above it.
fn block(lines: &[&str], net: &opendvp::plc::Network) -> (usize, usize) {
    let mut start = net.line - 1;
    while start > 0 {
        let l = lines[start - 1].trim();
        if l.starts_with(';') || l.starts_with("//") {
            start -= 1;
        } else {
            break;
        }
    }
    (start, net.end_line)
}

fn comment_lines(text: &str) -> Vec<String> {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).map(|l| format!("; {l}")).collect()
}

fn strip_end(il: &str) -> Vec<String> {
    il.lines().filter(|l| !l.trim().eq_ignore_ascii_case("END")).map(str::to_owned).collect()
}

/// Apply a text edit to one program; the result must compile, otherwise nothing changes.
fn edit_networks(
    state: &Mutex<State>,
    program_index: Option<u32>,
    edit: impl FnOnce(&str, &opendvp::plc::Program) -> Res<String>,
) -> Res<String> {
    let mut s = state.lock().map_err(|_| "state poisoned".to_string())?;
    let i = program_index.unwrap_or(0) as usize;
    let source = program_source(&s, i)?;
    let before = editable(&source)?;
    let candidate = edit(&source, &before)?;
    let after = editable(&candidate).map_err(|e| format!("edit rejected, project unchanged: {e}"))?;
    s.project.set_program_source(i, candidate);
    s.sim = None;
    Ok(format!("ok: program {i} now has {} networks (was {})", after.networks.len(), before.networks.len()))
}

fn ensure_sim(s: &mut State) -> Res<()> {
    if s.sim.is_none() {
        let program = s.compile()?;
        s.sim = Some((Simulator::default(), program));
    }
    Ok(())
}

trait Canonical {
    fn canonical(&self) -> String;
}
impl Canonical for Device {
    fn canonical(&self) -> String {
        format!("{:?}{}", self.area, self.index)
    }
}

// ---------------------------------------------------------------- MCP glue

struct Handler {
    state: Mutex<State>,
}

#[async_trait]
impl ServerHandler for Handler {
    async fn handle_list_tools_request(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: &RequestContext,
        _runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<ListToolsResult, RpcError> {
        Ok(ListToolsResult {
            tools: vec![
                SearchInstructions::tool(), ListModels::tool(), ListSerialPorts::tool(),
                CompileIl::tool(), CompileSt::tool(), IlToWords::tool(), WordsToIl::tool(), ReadDvz::tool(),
                ProjectNew::tool(), ProjectOpen::tool(), WindowProjectPull::tool(), WindowProjectPush::tool(), ProjectSave::tool(), ProjectInfo::tool(), ProjectExportDvp::tool(),
                GetProgram::tool(), SetProgram::tool(), AddUnit::tool(), RemoveUnit::tool(),
                SetSymbol::tool(), RemoveSymbol::tool(), SetComment::tool(), ProjectCompile::tool(), ProjectWords::tool(),
                SimReset::tool(), SimSet::tool(), SimScan::tool(), SimRead::tool(),
                PlcProbe::tool(), PlcStatus::tool(), PlcReadProgram::tool(), PlcScanModules::tool(),
                PlcDownload::tool(), PlcSetRun::tool(),
                ListNetworks::tool(), InsertNetwork::tool(), ReplaceNetwork::tool(), DeleteNetwork::tool(), MoveNetwork::tool(), SetNetworkComment::tool(),
                PlcForce::tool(), PlcReleaseForces::tool(),
            ],
            meta: None,
            next_cursor: None,
            cache_scope: ListToolsResultCacheScope::Public,
            result_type: "complete".to_string(),
            ttl_ms: 0,
        })
    }

    async fn handle_call_tool_request(
        &self,
        params: CallToolRequestParams,
        _context: &RequestContext,
        _runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<ServerResult, CallToolError> {
        // Domain failures are tool errors (is_error), not protocol errors.
        let (text, is_error) = match run(&self.state, &params.name, &params).await {
            Ok(text) => (text, None),
            Err(text) => (text, Some(true)),
        };
        Ok(ServerResult::CallToolResult(CallToolResult {
            content: vec![ContentBlock::TextContent(TextContent::new(text, None, None))],
            structured_content: None,
            is_error,
            meta: None,
            result_type: "complete".to_string(),
        }))
    }
}

/// Default port of `opendvp --mcp-http`.
pub const DEFAULT_HTTP_PORT: u16 = 8765;

fn runtime() -> SdkResult<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| rust_mcp_sdk::error::McpSdkError::Internal { description: e.to_string() })
}

fn server_details() -> ServerDetails {
    ServerDetails {
        server_info: Implementation {
            name: "opendvp".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            title: Some("OpenDVP".into()),
            description: Some("Write, check, simulate and download Delta DVP PLC programs".into()),
            icons: vec![],
            website_url: None,
        },
        capabilities: ServerCapabilities {
            tools: Some(ServerCapabilitiesTools { list_changed: None }),
            ..Default::default()
        },
        instructions: Some(
            "Workflow: project_new or project_open -> search_instructions -> set_program -> project_compile \
             -> sim_set/sim_scan to test offline. Only after the user approves, use plc_probe and then plc_download \
             (confirm=true). Never start or download to a PLC without explicit user approval."
                .into(),
        ),
        meta: None,
    }
}

fn handler() -> Handler {
    Handler {
        state: Mutex::new(State { project: Project::default(), path: None, sim: None }),
    }
}

/// Serve MCP over stdin/stdout until the client disconnects (`opendvp --mcp`).
pub fn serve_stdio() -> SdkResult<()> {
    runtime()?.block_on(async {
        let transport = StdioTransport::new(TransportOptions::default())?;
        let server = server_runtime::create_server(McpServerOptions {
            server_details: server_details(),
            transport,
            handler: handler().to_mcp_server_handler(),
            message_observer: None,
        });
        server.start().await
    })
}

/// Serve MCP over streamable HTTP at `http://<host>:<port>/mcp` (`opendvp --mcp-http`).
/// Listens on every interface (0.0.0.0). There is no authentication: anyone who can reach the
/// port can use the tools, including the confirm-gated PLC writes, so use it on a trusted network.
/// Network settings of the HTTP server, taken from the saved preferences.
#[derive(Clone, Debug)]
struct HttpConfig {
    port: u16,
    lan: bool,
    token: Option<String>,
}

impl HttpConfig {
    fn from_settings() -> Self {
        let s = opendvp::settings::get();
        Self {
            port: s.mcp_port,
            lan: s.mcp_lan,
            token: Some(s.mcp_token).filter(|t| !t.is_empty()),
        }
    }
}

/// Accepts exactly one static bearer token (`Authorization: Bearer <token>`).
struct TokenAuth {
    token: String,
}

#[async_trait]
impl rust_mcp_sdk::auth::AuthProvider for TokenAuth {
    async fn verify_token(
        &self,
        access_token: String,
    ) -> std::result::Result<rust_mcp_sdk::auth::AuthInfo, rust_mcp_sdk::auth::AuthenticationError> {
        // Compare without an early exit so the time does not reveal the matching prefix.
        let (a, b) = (access_token.as_bytes(), self.token.as_bytes());
        let same = a.len() == b.len() && a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0;
        if !same {
            return Err(rust_mcp_sdk::auth::AuthenticationError::InvalidToken {
                description: "invalid access token",
            });
        }
        Ok(rust_mcp_sdk::auth::AuthInfo {
            token_unique_id: "opendvp-token".into(),
            client_id: None,
            user_id: None,
            scopes: None,
            // The SDK rejects tokens without an expiry; a static token is valid "from now" each time.
            expires_at: Some(std::time::SystemTime::now() + std::time::Duration::from_secs(3600)),
            audience: None,
            extra: None,
        })
    }

    fn auth_endpoints(
        &self,
    ) -> Option<&std::collections::HashMap<String, rust_mcp_sdk::auth::OauthEndpoint>> {
        None
    }

    async fn handle_request(
        &self,
        _request: http::Request<&str>,
        _state: Arc<rust_mcp_sdk::mcp_http::McpAppState>,
    ) -> std::result::Result<
        http::Response<rust_mcp_sdk::mcp_http::GenericBody>,
        rust_mcp_sdk::mcp_http::McpHttpError,
    > {
        Err(rust_mcp_sdk::mcp_http::McpHttpError::HttpError("not found".into()))
    }

    fn protected_resource_metadata_url(&self) -> Option<&str> {
        None
    }
}

/// Serve MCP over streamable HTTP at `http://<host>:<port>/mcp` (`opendvp --mcp-http`), with the
/// listening scope and access token from the preferences.
pub fn serve_http(port: u16) -> SdkResult<()> {
    serve_http_until(HttpConfig { port, ..HttpConfig::from_settings() }, None)
}

/// Like `serve_http`, but shuts down gracefully once `stop` is notified.
fn serve_http_until(
    config: HttpConfig,
    stop: Option<std::sync::Arc<tokio::sync::Notify>>,
) -> SdkResult<()> {
    runtime()?.block_on(async {
        let host = if config.lan { "0.0.0.0" } else { "127.0.0.1" };
        let auth: Option<Arc<dyn rust_mcp_sdk::auth::AuthProvider>> = config
            .token
            .clone()
            .map(|token| Arc::new(TokenAuth { token }) as Arc<dyn rust_mcp_sdk::auth::AuthProvider>);
        let server = rust_mcp_axum::create_axum_server(
            server_details(),
            handler().to_mcp_server_handler(),
            rust_mcp_axum::AxumServerOptions {
                host: host.into(),
                port: config.port,
                sse_support: false,
                auth,
                ..Default::default()
            },
        );
        if let Some(stop) = stop {
            let handle = server.server_handle();
            tokio::spawn(async move {
                stop.notified().await;
                handle.graceful_shutdown(Some(std::time::Duration::from_secs(1)));
            });
        }
        eprintln!(
            "OpenDVP MCP: listening on {host}:{} ({})",
            config.port,
            if config.token.is_some() { "access token required" } else { "no authentication" }
        );
        server.start().await
    })
}

/// A fresh random access token (128 bits from the OS-seeded hasher keys), hex encoded.
pub fn new_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    (0..2)
        .map(|_| format!("{:016x}", std::collections::hash_map::RandomState::new().build_hasher().finish()))
        .collect()
}

/// State of the background HTTP server started from the window.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Off,
    Running(u16),
    Failed(String),
}

static STATUS: Mutex<Status> = Mutex::new(Status::Off);

/// The window's open project as JSON, refreshed by the window while the server runs.
static WINDOW_PROJECT: Mutex<Option<String>> = Mutex::new(None);
static WINDOW_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);
/// A project sent by `window_project_push`, waiting for the window to adopt it.
static WINDOW_INBOX: Mutex<Option<String>> = Mutex::new(None);

/// Window side: publish the open project so `window_project_pull` can read it.
pub fn publish_window_project(project: &Project, path: Option<&Path>) {
    let Ok(json) = serde_json::to_string(project) else { return };
    if let Ok(mut slot) = WINDOW_PROJECT.lock() {
        *slot = Some(json);
    }
    if let Ok(mut slot) = WINDOW_PATH.lock() {
        *slot = path.map(Path::to_path_buf);
    }
}

/// Window side: a project pushed by an AI client, if any.
pub fn take_pushed_project() -> Option<Project> {
    let json = WINDOW_INBOX.lock().ok()?.take()?;
    serde_json::from_str(&json).ok()
}

/// Window side: forget the published project (server stopped).
pub fn clear_window_project() {
    if let Ok(mut slot) = WINDOW_PROJECT.lock() {
        *slot = None;
    }
    if let Ok(mut slot) = WINDOW_INBOX.lock() {
        *slot = None;
    }
}
/// Stop signal of the running background server; a new start replaces it.
static STOP: Mutex<Option<std::sync::Arc<tokio::sync::Notify>>> = Mutex::new(None);
/// Identifies the current background run so a stopped run cannot overwrite a newer status.
static RUN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn status() -> Status {
    STATUS.lock().map(|s| s.clone()).unwrap_or(Status::Off)
}

fn set_status(run: u64, status: Status) {
    if RUN.load(std::sync::atomic::Ordering::SeqCst) == run {
        if let Ok(mut state) = STATUS.lock() {
            *state = status;
        }
    }
}

/// Previous background run, joined by the next one so a restart can reuse the port.
static LAST_THREAD: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);

/// Start the HTTP server on a background thread with the saved port / scope / token (no-op when
/// it already runs). The server keeps its own project and simulator; it does not share the
/// project open in the window.
pub fn start_background() {
    let config = HttpConfig::from_settings();
    let stop = std::sync::Arc::new(tokio::sync::Notify::new());
    let run = {
        let Ok(mut state) = STATUS.lock() else { return };
        if matches!(*state, Status::Running(_)) {
            return;
        }
        *state = Status::Running(config.port);
        if let Ok(mut slot) = STOP.lock() {
            *slot = Some(stop.clone());
        }
        RUN.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
    };
    let previous = LAST_THREAD.lock().ok().and_then(|mut slot| slot.take());
    let port = config.port;
    let spawned = std::thread::Builder::new().name("mcp-http".into()).spawn(move || {
        if let Some(previous) = previous {
            let _ = previous.join();
        }
        let result = if RUN.load(std::sync::atomic::Ordering::SeqCst) == run {
            serve_http_until(config, Some(stop))
        } else {
            Ok(()) // stopped before it ever started
        };
        let _ = port;
        set_status(
            run,
            match result {
                Ok(()) => Status::Off,
                Err(e) => Status::Failed(e.to_string()),
            },
        );
    });
    match spawned {
        Ok(handle) => {
            if let Ok(mut slot) = LAST_THREAD.lock() {
                *slot = Some(handle);
            }
        }
        Err(e) => set_status(run, Status::Failed(e.to_string())),
    }
}

/// Apply changed network settings: restart the server if it is meant to run.
pub fn restart_background() {
    stop_background();
    if opendvp::settings::get().mcp_server {
        start_background();
    }
}

/// Stop the background server (graceful: open requests get a second to finish).
pub fn stop_background() {
    let Ok(mut state) = STATUS.lock() else { return };
    RUN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    if let Some(stop) = STOP.lock().ok().and_then(|mut slot| slot.take()) {
        stop.notify_one();
    }
    *state = Status::Off;
}
