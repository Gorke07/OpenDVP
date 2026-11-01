//! Draft standalone example. Parent must copy/admit/build it and launch it on
//! an owned private desktop. No native execution is implied by this source.
use opendvp::{
    es2_bridge::{Comparison, RtcComparisonBudgets, Session, Target},
    plc::{Action, Device, Memory, ScanStatus, Simulator, compile_il},
    rtc_admission::{AdmissionError, RtcControlFlags, RtcInitialInput},
    rtc_clock::{RtcOutcome, RtcServiceSchedule},
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

const SEED: [u8; 7] = [24, 4, 2, 29, 12, 30, 7];
const WRITE: [u8; 7] = [25, 5, 3, 4, 6, 7, 8];
const ES2: &str = "5570751341e73b46846f36d99ab476c0f40b9636a8d784d9a3781377e9eb0ea7";
const SS2: &str = "915c96a9ad252cb64773b539290e66af9dcbb4a04af3c860b6d654f7aaad3525";

#[derive(Debug, Clone, Copy, Serialize)]
enum Gate {
    Active,
    Off,
    MasterOff,
    HeldLoop,
}
#[derive(Debug, Clone, Copy, Serialize)]
enum Latch {
    Clean,
    Code,
    Pc,
}
#[derive(Debug, Clone, Serialize)]
struct Literal {
    device: String,
    kind: &'static str,
    value: u32,
    /// Seconds may change under explicitly admitted environmental events.
    /// Full Session comparisons still check them against independent state.
    native_literal: bool,
}
#[derive(Debug, Clone, Serialize)]
struct Fixture {
    name: String,
    source: String,
    operation: String,
    outcome: RtcOutcome,
    gate: Gate,
    latch: Latch,
    literals: Vec<Literal>,
    expected_shadow_without_elapsed_input: [u8; 7],
}
#[derive(Debug, Clone, Serialize)]
struct SitePlan {
    header: u16,
    words: Vec<u16>,
    expected_visits: Vec<Value>,
    clear_header: u16,
    clear_words: Vec<u16>,
    encoded_words: usize,
}
fn d(raw: &str) -> Device {
    raw.parse().expect("fixture device")
}
fn word(device: impl Into<String>, value: u16) -> Literal {
    Literal {
        device: device.into(),
        kind: "word",
        value: u32::from(value),
        native_literal: true,
    }
}
fn bit(index: u16, on: bool) -> Literal {
    Literal {
        device: format!("M{index}"),
        kind: "bit",
        value: u32::from(on),
        native_literal: true,
    }
}
fn dword(device: impl Into<String>, value: u32) -> Literal {
    Literal {
        device: device.into(),
        kind: "dword",
        value,
        native_literal: true,
    }
}
fn add_word(seed: &mut Vec<String>, checks: &mut Vec<Literal>, address: String, value: u16) {
    seed.push(format!("MOV H{value:04X} {address}"));
    checks.push(word(address, value));
}
fn add_counter_guard(seed: &mut Vec<String>, checks: &mut Vec<Literal>, index: u16) {
    seed.push(format!("DMOV H12345678 C{index}"));
    checks.push(dword(format!("C{index}"), 0x1234_5678));
}
fn fixture(
    name: &str,
    operation: String,
    setup: Vec<String>,
    literals: Vec<Literal>,
    outcome: RtcOutcome,
    gate: Gate,
    latch: Latch,
    commit: bool,
) -> Fixture {
    let mut setup = [
        vec![
            "MOV K0 E0".into(),
            "MOV K0 F7".into(),
            "MOV K0 E7".into(),
            "RST M1067".into(),
            "RST M1068".into(),
            "MOV H2222 D1067".into(),
            "MOV K123 D1068".into(),
            "SET M1020".into(),
            "RST M1021".into(),
            "SET M1022".into(),
            "RST M1082".into(),
        ],
        setup,
    ]
    .concat();
    match latch {
        Latch::Clean => {}
        Latch::Code => setup.push("SET M1067".into()),
        Latch::Pc => setup.push("SET M1068".into()),
    }
    let body = match gate {
        // Consecutive RTC actions: no intervening operand decoder before the
        // valid clearing control; each exit is captured inside Session first.
        Gate::Active => format!("LDI X7\n{operation}\nTRD D300E7"),
        Gate::Off => format!("LD X0\n{operation}\nLDI X7\nTRD D300E7"),
        Gate::MasterOff => format!("LD X0\nMC N0\nLDI X7\n{operation}\nMCR N0\nLDI X7\nTRD D300E7"),
        Gate::HeldLoop => format!("LDI X7\nFOR K2\nLDI X7\n{operation}\nNEXT\nLDI X7\nTRD D300E7"),
    };
    let source = format!("LDI X7\n{}\n{body}\nMOV K321 D350\nEND", setup.join("\n"));
    assert!(source.lines().count() <= 96);
    Fixture {
        name: format!("rtc_decoder_diagnostic_{name}"),
        source,
        operation,
        outcome,
        gate,
        latch,
        literals,
        expected_shadow_without_elapsed_input: if commit { WRITE } else { SEED },
    }
}

fn fixtures(target: Target) -> Vec<Fixture> {
    let last = if target == Target::Es2 {
        9999u16
    } else {
        4999u16
    };
    let mut cases = Vec::new();
    // Six complete controls, including both P spellings and all T/C/D areas.
    for (area, end) in [("T", 255), ("C", 199), ("D", last)] {
        cases.push(read_case(area, end, 7, "TRD", false, Gate::Active));
    }
    cases.push(write_fields("complete_TWR", "TWR", false, Latch::Clean));
    cases.push(read_case("D", last, 7, "TRDP", true, Gate::Active));
    cases.push(write_fields("complete_TWRP", "TWRP", false, Latch::Clean));
    // Twelve partial-store cases: one/six stores in each area, normal and P.
    for (area, end) in [("T", 255), ("C", 199), ("D", last)] {
        for written in [1, 6] {
            for op in ["TRD", "TRDP"] {
                cases.push(read_case(
                    area,
                    end,
                    written,
                    op,
                    op.ends_with('P'),
                    Gate::Active,
                ));
            }
        }
    }
    for (effective, modifier, op) in [(200, "E0", "TRD"), (255, "F7", "TRDP")] {
        let mut setup = vec![format!("MOV K{} {modifier}", effective - 199)];
        let mut checks = Vec::new();
        add_word(&mut setup, &mut checks, "C199".into(), 0x4a4a);
        add_counter_guard(&mut setup, &mut checks, 200);
        if effective == 255 {
            add_counter_guard(&mut setup, &mut checks, 255);
        }
        cases.push(fixture(
            &format!("zero_C{effective}_{op}"),
            format!("{op} C199{modifier}"),
            setup,
            checks,
            RtcOutcome::PartialRead { written: 0 },
            Gate::Active,
            Latch::Clean,
            false,
        ));
    }
    // Guarded resolver failures; no invalid effective payload or stale pointer
    // is read. Each lexical base/descriptor itself is valid.
    for (name, operation, modifier, offset, guard) in [
        (
            "resolver_T256_TRD",
            "TRD T255E0".into(),
            "E0",
            1,
            "T255".into(),
        ),
        (
            "resolver_C256_TRDP",
            "TRDP C199F7".into(),
            "F7",
            57,
            "C199".into(),
        ),
        (
            "resolver_Dhigh_TWR",
            format!("TWR D{last}E0"),
            "E0",
            1,
            format!("D{last}"),
        ),
        (
            "resolver_Dnegative_TWRP",
            "TWRP D10F7".into(),
            "F7",
            -11,
            "D0".into(),
        ),
    ] {
        let mut setup = vec![format!("MOV K{offset} {modifier}")];
        let mut checks = Vec::new();
        add_word(&mut setup, &mut checks, guard, 0x4a4a);
        cases.push(fixture(
            name,
            operation,
            setup,
            checks,
            RtcOutcome::PlcError { offset: 1 },
            Gate::Active,
            Latch::Clean,
            false,
        ));
    }
    for (area, end, op) in [("T", 255, "TWR"), ("C", 199, "TWRP"), ("D", last, "TWR")] {
        cases.push(extent_case(area, end, op, Gate::Active));
    }
    let mut setup = vec!["MOV K1 F7".into()];
    let mut checks = Vec::new();
    add_counter_guard(&mut setup, &mut checks, 200);
    cases.push(fixture(
        "extent_C200_TWRP",
        "TWRP C199F7".into(),
        setup,
        checks,
        RtcOutcome::PlcError { offset: 3 },
        Gate::Active,
        Latch::Clean,
        false,
    ));
    cases.push(write_fields(
        "field_day_zero_TWR",
        "TWR",
        true,
        Latch::Clean,
    ));
    cases.push(write_fields(
        "field_day_zero_TWRP",
        "TWRP",
        true,
        Latch::Clean,
    ));
    cases.push(write_fields(
        "field_code_latch_retained",
        "TWR",
        true,
        Latch::Code,
    ));
    cases.push(write_fields(
        "field_pc_latch_retained",
        "TWRP",
        true,
        Latch::Pc,
    ));
    cases.push(read_case("D", last, 1, "TRDP", true, Gate::HeldLoop));
    cases.push(extent_case("D", last, "TWRP", Gate::HeldLoop));
    cases.push(read_case("D", last, 1, "TRDP", true, Gate::Off));
    cases.push(extent_case("D", last, "TWRP", Gate::MasterOff));
    assert_eq!(cases.len(), 36);
    let names: std::collections::BTreeSet<_> = cases.iter().map(|c| &c.name).collect();
    assert_eq!(names.len(), 36);
    cases
}

fn read_case(area: &str, end: u16, written: u16, op: &str, indexed: bool, gate: Gate) -> Fixture {
    let base = end + 1 - written;
    let mut setup = vec![];
    let mut checks = vec![];
    add_word(
        &mut setup,
        &mut checks,
        format!("{area}{}", base - 1),
        0x4a4a,
    );
    for n in 0..written {
        let address = format!("{area}{}", base + n);
        setup.push(format!("MOV H5555 {address}"));
        let skipped = matches!(gate, Gate::Off | Gate::MasterOff);
        let mut literal = word(
            address,
            if skipped {
                0x5555
            } else {
                u16::from(SEED[usize::from(n)])
            },
        );
        literal.native_literal = skipped || n != 6;
        checks.push(literal);
    }
    if area == "C" {
        add_counter_guard(&mut setup, &mut checks, 200);
    }
    let operand = if indexed {
        setup.push("MOV K-1 F7".into());
        // C199+1 is not a legal original WORD base. Use a positive offset for
        // the one-word endpoint and negative rescue for the other cases.
        if base == end {
            setup.pop();
            setup.push("MOV K1 F7".into());
            format!("{area}{}F7", base - 1)
        } else {
            format!("{area}{}F7", base + 1)
        }
    } else {
        format!("{area}{base}")
    };
    let outcome = if matches!(gate, Gate::Off | Gate::MasterOff) {
        RtcOutcome::Nominal
    } else if written == 7 {
        RtcOutcome::Complete
    } else {
        RtcOutcome::PartialRead {
            written: written as u8,
        }
    };
    fixture(
        &format!("read_{area}_{written}_{op}_{gate:?}"),
        format!("{op} {operand}"),
        setup,
        checks,
        outcome,
        gate,
        Latch::Clean,
        false,
    )
}
fn extent_case(area: &str, end: u16, op: &str, gate: Gate) -> Fixture {
    let base = end - 5;
    let mut setup = vec![];
    let mut checks = vec![];
    for n in 0..6 {
        add_word(
            &mut setup,
            &mut checks,
            format!("{area}{}", base + n),
            0x5000 + n,
        );
    }
    add_word(
        &mut setup,
        &mut checks,
        format!("{area}{}", base - 1),
        0x4a4a,
    );
    let outcome = if matches!(gate, Gate::Off | Gate::MasterOff) {
        RtcOutcome::Nominal
    } else {
        RtcOutcome::PlcError { offset: 3 }
    };
    fixture(
        &format!("extent_{area}_{op}_{gate:?}"),
        format!("{op} {area}{base}E0"),
        setup,
        checks,
        outcome,
        gate,
        Latch::Clean,
        false,
    )
}
fn write_fields(name: &str, op: &str, bad: bool, latch: Latch) -> Fixture {
    let mut fields = WRITE.map(u16::from);
    if bad {
        fields[3] = 0;
    }
    let mut setup = vec![];
    let mut checks = vec![];
    for (n, value) in fields.into_iter().enumerate() {
        add_word(&mut setup, &mut checks, format!("D{}", 10 + n), value);
    }
    add_word(&mut setup, &mut checks, "D9".into(), 0x4a4a);
    add_word(&mut setup, &mut checks, "D17".into(), 0x6b6b);
    fixture(
        name,
        format!("{op} D10E0"),
        setup,
        checks,
        if bad {
            RtcOutcome::PlcError { offset: 3 }
        } else {
            RtcOutcome::Complete
        },
        Gate::Active,
        latch,
        !bad,
    )
}

fn is_rtc(action: &Action) -> bool {
    match action {
        Action::Pulse(inner) => is_rtc(inner),
        Action::IndexedRtc(_) | Action::Rtc(_) => true,
        _ => false,
    }
}
fn active_expectation(header: u16, outcome: &RtcOutcome) -> Value {
    json!({"Active":{"header_pc":header,"outcome":outcome}})
}
fn read_literal(memory: &Memory, literal: &Literal) -> u32 {
    let at = d(&literal.device);
    match literal.kind {
        "bit" => u32::from(memory.bit(at)),
        "dword" => memory.double_word(at) as u32,
        _ => u32::from(memory.word(at) as u16),
    }
}
fn local_admission(f: &Fixture, target: Target) -> Result<(SitePlan, Vec<Literal>, Value), String> {
    let p = compile_il(&f.source).map_err(|e| e.to_string())?;
    if !p.requires_vm() {
        return Err("Diagnostic must use the source VM".into());
    }
    let encoded = opendvp::es2_encoder::encode(&p)?;
    if encoded.words.len() >= 0x1ef0 {
        return Err("Diagnostic program exceeds installed budget".into());
    }
    let pcs: Vec<_> = p
        .networks
        .iter()
        .flat_map(|n| &n.actions)
        .enumerate()
        .filter(|(_, a)| is_rtc(a))
        .map(|(i, _)| encoded.action_pc[i])
        .collect();
    if pcs.len() != 2 {
        return Err(format!(
            "Expected diagnostic+clearing RTC sites, found {}",
            pcs.len()
        ));
    }
    let header = pcs[0];
    let clear_header = pcs[1];
    for pc in &pcs {
        let node = encoded
            .execution_plan
            .by_pc
            .get(pc)
            .and_then(|i| encoded.execution_plan.nodes.get(*i))
            .ok_or("RTC plan header missing")?;
        if node.fallthrough != pc + 3 {
            return Err(format!("RTC must end at H+3: {pc}"));
        }
    }
    let mut visits = vec![if matches!(f.gate, Gate::Off | Gate::MasterOff) {
        json!({"Skipped":{"header_pc":header}})
    } else {
        active_expectation(header, &f.outcome)
    }];
    if matches!(f.gate, Gate::HeldLoop) {
        visits.push(json!({"Skipped":{"header_pc":header}}));
    }
    visits.push(active_expectation(clear_header, &RtcOutcome::Complete));
    let plan = SitePlan {
        header,
        words: encoded.words[usize::from(header)..usize::from(header) + 3].to_vec(),
        expected_visits: visits,
        clear_header,
        clear_words: encoded.words[usize::from(clear_header)..usize::from(clear_header) + 3]
            .to_vec(),
        encoded_words: encoded.words.len(),
    };
    let error_offset = if let RtcOutcome::PlcError { offset } = f.outcome {
        Some(offset)
    } else {
        None
    };
    let mut literals = f.literals.clone();
    let code = if error_offset.is_some() && !matches!(f.latch, Latch::Code) {
        0xe1a
    } else {
        0x2222
    };
    let error_pc = if let Some(offset) = error_offset.filter(|_| !matches!(f.latch, Latch::Pc)) {
        header + offset
    } else {
        123
    };
    literals.extend([
        word("D1067", code),
        word("D1068", error_pc),
        word("D350", 321),
        bit(
            1067,
            error_offset.is_some() || matches!(f.latch, Latch::Code),
        ),
        bit(1068, error_offset.is_some() || matches!(f.latch, Latch::Pc)),
        bit(1020, true),
        bit(1021, false),
        bit(1022, true),
        bit(
            1082,
            f.operation.starts_with("TWR") && matches!(f.outcome, RtcOutcome::Complete)
                || f.name.contains("field_"),
        ),
    ]);
    for (n, value) in f
        .expected_shadow_without_elapsed_input
        .into_iter()
        .enumerate()
    {
        let mut literal = word(format!("D{}", 300 + n), u16::from(value));
        literal.native_literal = n != 6;
        literals.push(literal);
    }
    let mut sim = Simulator::default();
    sim.memory.data_register_limit = Some(if target == Target::Es2 { 9999 } else { 4999 });
    sim.memory.rtc_clock.configure_frozen(SEED);
    sim.memory.rtc_service_schedule = RtcServiceSchedule::ExplicitBoundaries;
    sim.memory.set_input(d("X0"), false);
    sim.memory.set_input(d("X7"), false);
    opendvp::rtc_clock::refresh_public(&mut sim.memory)?;
    sim.scan(&p, 0);
    if sim.last_scan != ScanStatus::Complete || !sim.memory.execution_limitations.is_empty() {
        return Err(format!(
            "Independent source VM: {:?}; {:?}",
            sim.last_scan, sim.memory.execution_limitations
        ));
    }
    for literal in &literals {
        let actual = read_literal(&sim.memory, literal);
        if actual != literal.value {
            return Err(format!(
                "Independent literal {} {}: expected {}, got {actual}",
                literal.device, literal.kind, literal.value
            ));
        }
    }
    if sim.memory.rtc_clock.shadow() != Some(f.expected_shadow_without_elapsed_input) {
        return Err("Independent private RTC literal failed".into());
    }
    Ok((
        plan,
        literals,
        json!({"status":"passed","method":"Independent public source-VM FrozenSeed, zero elapsed input; all seven literal fields checked","scans":sim.scans,"shadow":sim.memory.rtc_clock.shadow()}),
    ))
}

fn compare_literals(snapshot: &Comparison, literals: &[Literal]) -> Result<Vec<Value>, String> {
    let mut rows = vec![];
    for literal in literals.iter().filter(|c| c.native_literal) {
        let at = d(&literal.device);
        let row = snapshot
            .rows
            .iter()
            .find(|r| r.device == at && r.kind == literal.kind)
            .ok_or_else(|| {
                format!(
                    "Missing qualified literal row {} {}",
                    literal.device, literal.kind
                )
            })?;
        rows.push(json!({"expected":literal,"local":row.local,"native":row.original}));
        if row.local != literal.value || row.original != literal.value {
            return Err(format!(
                "Qualified literal mismatch {} {}: expected{}, local{}, native{}",
                literal.device, literal.kind, literal.value, row.local, row.original
            ));
        }
    }
    Ok(rows)
}
fn decoder_coverage(
    session: &Session,
    plan: &SitePlan,
    gate: Gate,
    outcome: &RtcOutcome,
) -> Result<Value, String> {
    let evidence = session
        .rtc_comparison_evidence()
        .ok_or("Missing RTC evidence")?;
    if evidence.decoder_diagnostics.len() != plan.expected_visits.len() {
        return Err("RTC diagnostic visit count differs from independent source plan".into());
    }
    for (observed, want) in evidence
        .decoder_diagnostics
        .iter()
        .zip(&plan.expected_visits)
    {
        if serde_json::to_value(&observed.expectation).map_err(|e| e.to_string())? != *want {
            return Err("RTC visit/outcome differs from literal source plan".into());
        }
    }
    let clear = evidence
        .decoder_diagnostics
        .last()
        .ok_or("Missing clearing control")?;
    let clear_from_set =
        clear.before.status_before & 0x10 != 0 && clear.before.status_after & 0x10 != 0;
    let held_set = evidence
        .decoder_diagnostics
        .iter()
        .enumerate()
        .any(|(i, e)| {
            matches!(gate, Gate::HeldLoop)
                && i == 1
                && e.before.status_before & 0x10 != 0
                && e.before.status_after & 0x10 != 0
        });
    let clear_required = matches!(
        outcome,
        RtcOutcome::PartialRead { .. } | RtcOutcome::PlcError { .. }
    );
    Ok(
        json!({"fresh_visit_outcomes_match":true,"valid_clear_from_observed_set_bit":clear_from_set,
        "held_p_preserved_observed_set_bit":held_set,"held_set_required":matches!(gate,Gate::HeldLoop),
        "clear_from_set_required":clear_required,
        "coverage_complete":(!clear_required||clear_from_set)&&(!matches!(gate,Gate::HeldLoop)||held_set)}),
    )
}

#[cfg(windows)]
fn private_desktop_name() -> Result<String, String> {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThreadId() -> u32;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetThreadDesktop(thread: u32) -> *mut c_void;
        fn GetUserObjectInformationW(
            object: *mut c_void,
            index: i32,
            buffer: *mut c_void,
            length: u32,
            needed: *mut u32,
        ) -> i32;
    }
    let mut buffer = [0u16; 512];
    let mut needed = 0;
    let ok = unsafe {
        GetUserObjectInformationW(
            GetThreadDesktop(GetCurrentThreadId()),
            2,
            buffer.as_mut_ptr().cast(),
            (buffer.len() * 2) as u32,
            &mut needed,
        )
    };
    if ok == 0 {
        return Err(format!(
            "Cannot read launch desktop: {}",
            std::io::Error::last_os_error()
        ));
    }
    let end = buffer
        .iter()
        .position(|c| *c == 0)
        .ok_or("Desktop name not terminated")?;
    let name = String::from_utf16(&buffer[..end]).map_err(|e| e.to_string())?;
    if name.is_empty()
        || ["default", "winlogon", "disconnect"].contains(&name.to_ascii_lowercase().as_str())
    {
        return Err("Root must launch this diagnostic on its owned private desktop".into());
    }
    Ok(name)
}
#[cfg(not(windows))]
fn private_desktop_name() -> Result<String, String> {
    Err("Owned native RTC diagnostic requires Windows".into())
}
fn digest(path: &Path) -> Result<String, std::io::Error> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}
fn append_journal(file: &mut fs::File, value: &Value) -> Result<(), Box<dyn std::error::Error>> {
    serde_json::to_writer(&mut *file, value)?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 || args[2] != "--owned-private-desktop" {
        return Err(
            "usage: rtc_decoder_diagnostic_probe SIMULATOR OUTPUT_JSON --owned-private-desktop"
                .into(),
        );
    }
    let desktop = private_desktop_name()?;
    let target_path = PathBuf::from(&args[0]).canonicalize()?;
    let target_hash = digest(&target_path)?;
    let target = match target_hash.as_str() {
        ES2 => Target::Es2,
        SS2 => Target::Ss2,
        _ => return Err("Decoder diagnostics require exact current COMMGR ES2/SS2 image".into()),
    };
    let output = PathBuf::from(&args[1]);
    let journal_path = output.with_extension("jsonl");
    if output == journal_path || output.exists() || journal_path.exists() {
        return Err(
            "Use fresh .json and .jsonl report paths; existing attempts are immutable".into(),
        );
    }
    let mut report_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    let mut journal = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&journal_path)?;
    let executable = std::env::current_exe()?;
    let declared = RtcInitialInput {
        wire_seed: SEED,
        controls: RtcControlFlags {
            m1016: false,
            m1017: false,
            m1082: false,
        },
    };
    let budgets = RtcComparisonBudgets {
        max_capture_us: 200_000,
        max_observation_us: 100_000,
        max_window_us: 250_000,
        max_between_us: 950_000,
        latest_start_phase_us: 600_000,
        continuous_host_clock_assumed: true,
    };
    let header = json!({"event":"start","schema_version":1,"target":target_path,"target_sha256":target_hash,
        "producer_executable":executable,"producer_sha256":digest(&executable)?,
        "probe_source_sha256":format!("{:x}",Sha256::digest(include_bytes!("rtc_decoder_diagnostic_probe.rs"))),
        "desktop":desktop,"root_private_desktop_attestation":true,"declared":declared,"budgets":budgets,
        "method":"Owned unchanged image; diagnostic RPM before/after/idle; independent source VM and declared environmental inputs; no result resync"});
    append_journal(&mut journal, &header)?;
    let cases = fixtures(target);
    let mut ready = Vec::new();
    let mut results = Vec::new();
    // Finish every independent fixed-input admission before opening a simulator.
    for fixture in cases {
        match local_admission(&fixture,target) {
            Ok((plan,literals,local))=>ready.push((fixture,plan,literals,local)),
            Err(error)=>results.push(json!({"name":fixture.name,"status":"local_admission_failure","error":error,"fixture":fixture})),
        }
    }
    if !results.is_empty() {
        for result in &results {
            append_journal(&mut journal, result)?;
        }
        serde_json::to_writer_pretty(
            &mut report_file,
            &json!({"header":header,"native_started":false,"results":results}),
        )?;
        report_file.flush()?;
        return Err(
            "Independent source/literal admission failed; no native process started".into(),
        );
    }
    let mut session = match Session::launch(&target_path, Arc::new(AtomicBool::new(false))) {
        Ok(session) => session,
        Err(error) => {
            let result = json!({"status":"launch_inconclusive","error":error});
            append_journal(&mut journal, &result)?;
            serde_json::to_writer_pretty(
                &mut report_file,
                &json!({"header":header,"results":[result]}),
            )?;
            report_file.flush()?;
            return Err("Owned simulator launch failed".into());
        }
    };
    for (fixture, plan, literals, local) in ready {
        let mut snapshot_record = Value::Null;
        let mut literal_record = Value::Null;
        let mut coverage = Value::Null;
        let mut stage = "upload";
        let result = (|| -> Result<(), String> {
            session.upload(&fixture.source)?;
            stage = "configure";
            session.configure_rtc_decoder_diagnostics(declared, budgets)?;
            stage = "step";
            let snapshot = session.step(&[(d("X0"), false), (d("X7"), false)])?;
            snapshot_record =
                json!({"scan":snapshot.scan,"rows":snapshot.rows,"mismatches":snapshot.mismatches});
            stage = "snapshot";
            if snapshot.mismatches != 0 {
                return Err("Accepted RTC snapshot has durable mismatches".into());
            }
            stage = "literal";
            literal_record = json!(compare_literals(&snapshot, &literals)?);
            stage = "coverage";
            coverage = decoder_coverage(&session, &plan, fixture.gate, &fixture.outcome)?;
            Ok(())
        })();
        let evidence = session.rtc_comparison_evidence();
        let reference_failure = evidence
            .and_then(|e| e.error.as_ref())
            .is_some_and(|e| matches!(e, AdmissionError::ConformanceFailure { .. }));
        let status = if reference_failure || (result.is_err() && stage == "snapshot") {
            "conformance_failure"
        } else if result.is_err() {
            if stage == "literal" || stage == "coverage" {
                "independent_oracle_failure"
            } else {
                "inconclusive"
            }
        } else if coverage["coverage_complete"] != true {
            "inconclusive_status_transition_coverage"
        } else {
            "diagnostic_match"
        };
        let record = json!({"name":fixture.name,"status":status,"stage":stage,"error":result.err(),
            "fixture":fixture,"independent_site_plan":plan,"fixed_local_admission":local,
            "literal_expectations":literals,"literal_checks":literal_record,"status_coverage":coverage,"snapshot":snapshot_record,
            "rtc":evidence,"verified_instruction_steps":session.verified_instruction_steps(),
            "verified_rtc_gates":session.verified_rtc_gates(),"trace":session.trace()});
        println!(
            "{}: {status}",
            record["name"].as_str().unwrap_or("diagnostic")
        );
        append_journal(&mut journal, &record)?;
        results.push(record);
        // No retry or result-dependent reseed. Next upload cold-restarts only
        // this owned child and declares a new independent fixture input.
    }
    let matched = results
        .iter()
        .filter(|r| r["status"] == "diagnostic_match")
        .count();
    let failures = results
        .iter()
        .filter(|r| {
            matches!(
                r["status"].as_str(),
                Some("conformance_failure" | "independent_oracle_failure")
            )
        })
        .count();
    let summary = json!({"event":"summary","cases":results.len(),"matched":matched,"failures":failures,"inconclusive":results.len()-matched-failures,
        "qualification":"Separate decoder diagnostics only; normal qualifier/domain is unchanged; physical PLC and transitive worker persistence are not claimed"});
    append_journal(&mut journal, &summary)?;
    serde_json::to_writer_pretty(
        &mut report_file,
        &json!({"header":header,"summary":summary,"results":results}),
    )?;
    report_file.flush()?;
    drop(session);
    if failures != 0 {
        std::process::exit(2);
    }
    if matched != 36 {
        std::process::exit(3);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parent can run this example's test harness before launching any target.
    /// It never calls Session, reads a host clock, or opens a desktop.
    #[test]
    fn both_model_fixture_sets_pass_independent_local_literal_admission() {
        for target in [Target::Es2, Target::Ss2] {
            let cases = fixtures(target);
            assert_eq!(cases.len(), 36);
            for fixture in cases {
                let (plan, literals, _) = local_admission(&fixture, target)
                    .unwrap_or_else(|reason| panic!("{}: {reason}", fixture.name));
                assert_eq!(plan.words.len(), 3);
                assert_eq!(plan.clear_words.len(), 3);
                assert!(literals.iter().any(|c| c.device == "D1068"));
                assert!(literals.iter().any(|c| c.device == "D350"));
                assert!(plan.encoded_words < 0x1ef0);
            }
        }
    }
}
