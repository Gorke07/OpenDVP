//! Independent addressed RAND stream invariant harness.
//! Each stream is tested against its own observed, fixed bounds. RNG states and
//! random results are never synchronized or compared for equality across streams.
use opendvp::{
    es2_bridge::{
        COMMGR_ES2_SHA256, COMMGR_SS2_SHA256, DEFAULT_SIMULATOR, Reading, SCAN_MS, SS2_SHA256,
        Session, TARGET_SHA256,
    },
    execution_plan::PlanOp,
    plc::{Area, Device},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

const POWERS: [bool; 5] = [false, true, true, false, true];
const INDEX_ALIASES: [(&str, &str); 16] = [
    ("E0", "D1028"),
    ("F0", "D1029"),
    ("E1", "D1182"),
    ("F1", "D1183"),
    ("E2", "D1184"),
    ("F2", "D1185"),
    ("E3", "D1186"),
    ("F3", "D1187"),
    ("E4", "D1188"),
    ("F4", "D1189"),
    ("E5", "D1190"),
    ("F5", "D1191"),
    ("E6", "D1192"),
    ("F6", "D1193"),
    ("E7", "D1194"),
    ("F7", "D1195"),
];

fn device(raw: &str) -> Device {
    raw.parse()
        .unwrap_or_else(|e| panic!("Invalid fixture device {raw}: {e}"))
}
fn next(base: Device, offset: u16) -> Device {
    Device {
        area: base.area,
        index: base.index + offset,
    }
}
fn observed(rows: &[Reading], d: Device, kind: &str, original: bool) -> Result<u32, String> {
    rows.iter()
        .find(|r| r.device == d && r.kind == kind)
        .map(|r| if original { r.original } else { r.local })
        .ok_or_else(|| format!("Missing observation {d}/{kind}"))
}

/// Explicit fixture oracle, independent of the instruction's label/parser.
/// The effective address is fixed because every offset is initialized once,
/// observed on every scan, and disjoint from every random destination.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Storage {
    Numeric { base: String, wide: bool },
    Group { base: String, bits: u16, wide: bool },
}
impl Storage {
    fn numeric(base: &str, wide: bool) -> Self {
        assert!(matches!(device(base).area, Area::D | Area::T | Area::C));
        Self::Numeric {
            base: base.into(),
            wide,
        }
    }
    fn group(base: &str, bits: u16, wide: bool) -> Self {
        assert!(matches!(
            device(base).area,
            Area::X | Area::Y | Area::M | Area::S
        ));
        assert!(bits > 0 && bits & 3 == 0 && bits <= if wide { 32 } else { 16 });
        Self::Group {
            base: base.into(),
            bits,
            wide,
        }
    }
    fn base(&self) -> Device {
        device(match self {
            Self::Numeric { base, .. } | Self::Group { base, .. } => base,
        })
    }
    fn wide(&self) -> bool {
        match self {
            Self::Numeric { wide, .. } | Self::Group { wide, .. } => *wide,
        }
    }
    fn slots(&self) -> Vec<(Device, &'static str)> {
        let base = self.base();
        match self {
            Self::Group { bits, .. } => (0..*bits).map(|i| (next(base, i), "bit")).collect(),
            Self::Numeric { wide, .. } => {
                if base.area == Area::C && base.index >= 200 {
                    assert!(*wide, "word natural-counter fixture is forbidden");
                    vec![(base, "dword")]
                } else if *wide && !(base.area == Area::C && base.index == 199) {
                    vec![(base, "word"), (next(base, 1), "word")]
                } else {
                    vec![(base, "word")]
                }
            }
        }
    }
    fn contains_row(&self, row: &Reading) -> bool {
        self.slots()
            .into_iter()
            .any(|(d, kind)| row.device == d && row.kind == kind)
    }
    fn devices(&self) -> BTreeSet<Device> {
        self.slots().into_iter().map(|(d, _)| d).collect()
    }
    fn value(&self, rows: &[Reading], original: bool) -> Result<i32, String> {
        let mut packed = 0_u32;
        for (i, (d, kind)) in self.slots().into_iter().enumerate() {
            let value = observed(rows, d, kind, original)?;
            packed |= value << if kind == "bit" { i } else { i * 16 };
        }
        Ok(if self.wide() {
            packed as i32
        } else {
            i32::from(packed as i16)
        })
    }
    fn initialization(&self, value: i32) -> Vec<String> {
        match self {
            Self::Group { bits, .. } => (0..*bits)
                .map(|i| {
                    format!(
                        "{} {}",
                        if value as u32 & (1 << i) != 0 {
                            "SET"
                        } else {
                            "RST"
                        },
                        next(self.base(), i)
                    )
                })
                .collect(),
            Self::Numeric { wide, .. } => {
                let c199 = self.base().area == Area::C && self.base().index == 199;
                vec![format!(
                    "{} K{value} {}",
                    if *wide && !c199 { "DMOV" } else { "MOV" },
                    self.base()
                )]
            }
        }
    }
    fn initial_value(&self) -> i32 {
        match self {
            Self::Group { bits, wide, .. } => {
                let bits = if *bits == 32 {
                    u32::MAX
                } else {
                    (1 << *bits) - 1
                };
                if *wide {
                    bits as i32
                } else {
                    i32::from(bits as i16)
                }
            }
            Self::Numeric { wide: true, .. }
                if self.base().area == Area::C && self.base().index == 199 =>
            {
                53191
            }
            Self::Numeric { .. } => -12345,
        }
    }
    fn seed(&self) -> Vec<String> {
        self.initialization(if matches!(self, Self::Group { .. }) {
            -1
        } else {
            -12345
        })
    }
}

#[derive(Clone, Debug, Serialize)]
struct Bound {
    expression: String,
    expected: i32,
    storage: Storage,
}
impl Bound {
    fn numeric(expression: &str, base: &str, wide: bool, expected: i32) -> Self {
        Self {
            expression: expression.into(),
            expected,
            storage: Storage::numeric(base, wide),
        }
    }
    fn group(expression: &str, base: &str, bits: u16, wide: bool, expected: i32) -> Self {
        Self {
            expression: expression.into(),
            expected,
            storage: Storage::group(base, bits, wide),
        }
    }
}
#[derive(Clone, Debug, Serialize)]
struct FixedIndex {
    name: String,
    alias: String,
    value: i16,
}
fn indices(values: &[(&str, i16)]) -> Vec<FixedIndex> {
    values
        .iter()
        .map(|(name, value)| FixedIndex {
            name: (*name).into(),
            alias: INDEX_ALIASES
                .iter()
                .find(|(n, _)| n == name)
                .expect("literal bank table")
                .1
                .into(),
            value: *value,
        })
        .collect()
}
#[derive(Clone, Debug, Serialize)]
struct FixedInput {
    device: String,
    value: bool,
}
#[derive(Clone, Debug, Serialize)]
struct Guard {
    device: String,
    kind: &'static str,
    expected: u32,
}
impl Guard {
    fn numeric(d: Device, expected: u32) -> Self {
        Self {
            device: d.to_string(),
            kind: if d.area == Area::C && d.index >= 200 {
                "dword"
            } else {
                "word"
            },
            expected,
        }
    }
    fn initialization(&self) -> String {
        match self.kind {
            "bit" => format!(
                "{} {}",
                if self.expected != 0 { "SET" } else { "RST" },
                self.device
            ),
            "dword" => format!("DMOV K{} {}", self.expected, self.device),
            _ => format!("MOV K{} {}", self.expected, self.device),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct Case {
    name: String,
    op: String,
    source: String,
    wide: bool,
    pulse: bool,
    bounds: [Bound; 2],
    target_expression: String,
    effective_target: Storage,
    fixed_indices: Vec<FixedIndex>,
    fixed_inputs: Vec<FixedInput>,
    guards: Vec<Guard>,
}
impl Case {
    fn instruction(&self) -> String {
        format!(
            "{} {} {} {}",
            self.op, self.bounds[0].expression, self.bounds[1].expression, self.target_expression
        )
    }
    fn validate_metadata(&self) -> Result<(), String> {
        let target = self.effective_target.devices();
        let aliases: BTreeSet<_> = self
            .fixed_indices
            .iter()
            .map(|i| device(&i.alias))
            .collect();
        if aliases.len() != self.fixed_indices.len() {
            return Err("Duplicate modifier initialization".into());
        }
        let sources: BTreeSet<_> = self
            .bounds
            .iter()
            .flat_map(|b| b.storage.devices())
            .collect();
        if !target.is_disjoint(&sources)
            || !target.is_disjoint(&aliases)
            || !sources.is_disjoint(&aliases)
        {
            return Err("Random output, fixed bounds and index storage must be disjoint".into());
        }
        for guard in &self.guards {
            let d = device(&guard.device);
            if target.contains(&d) || sources.contains(&d) || aliases.contains(&d) {
                return Err(format!("Guard {} aliases a fixture role", guard.device));
            }
        }
        for d in [device("D400"), device("D401"), device("Y0")] {
            if target.contains(&d) || sources.contains(&d) || aliases.contains(&d) {
                return Err("Post marker aliases fixture role".into());
            }
        }
        Ok(())
    }
}

fn fixture(
    name: &str,
    op: &str,
    bounds: [Bound; 2],
    target: &str,
    effective_target: Storage,
    fixed_indices: Vec<FixedIndex>,
    label_base_guard: Option<&str>,
) -> Case {
    let wide = op.starts_with('D');
    let pulse = op.ends_with('P');
    assert_eq!(effective_target.wide(), wide);
    let base = effective_target.base();
    let slots = effective_target.slots();
    let last = slots.last().unwrap().0;
    let mut guards = Vec::new();
    if matches!(effective_target, Storage::Group { .. }) {
        for index in [base.index - 1, last.index + 1] {
            guards.push(Guard {
                device: Device {
                    area: base.area,
                    index,
                }
                .to_string(),
                kind: "bit",
                expected: 1,
            });
        }
    } else {
        if base.index > 0 {
            guards.push(Guard::numeric(
                Device {
                    area: base.area,
                    index: base.index - 1,
                },
                1234,
            ));
        }
        if last.index < if base.area == Area::D { 9999 } else { 255 } {
            guards.push(Guard::numeric(next(last, 1), 5678));
        }
    }
    if let Some(label) = label_base_guard {
        let d = device(label);
        if !effective_target.devices().contains(&d)
            && !guards.iter().any(|g| device(&g.device) == d)
        {
            guards.push(Guard::numeric(d, 777));
        }
    }
    let mut fixed_inputs = Vec::new();
    let mut lines = vec!["LDP X1".into()];
    lines.extend(effective_target.seed());
    lines.extend(guards.iter().map(Guard::initialization));
    for bound in &bounds {
        if bound.storage.base().area == Area::X {
            for (i, (d, _)) in bound.storage.slots().into_iter().enumerate() {
                fixed_inputs.push(FixedInput {
                    device: d.to_string(),
                    value: bound.expected as u32 & (1 << i) != 0,
                });
            }
        } else {
            lines.extend(bound.storage.initialization(bound.expected));
        }
    }
    lines.extend(
        fixed_indices
            .iter()
            .map(|i| format!("MOV K{} {}", i.value, i.alias)),
    );
    lines.extend(
        [
            "SET M1020",
            "SET M1021",
            "SET M1022",
            "RST M1067",
            "RST M1068",
            "MOV K0 D1067",
            "MOV K0 D1068",
            "LD X0",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    let mut case = Case {
        name: format!("indexed_random_invariant_{name}"),
        op: op.into(),
        source: String::new(),
        wide,
        pulse,
        bounds,
        target_expression: target.into(),
        effective_target,
        fixed_indices,
        fixed_inputs,
        guards,
    };
    lines.push(case.instruction());
    lines.extend(
        ["MOV K123 D400", "INC D401", "OUT Y0"]
            .into_iter()
            .map(str::to_owned),
    );
    // Observe fixed external bounds independently even when the numerical
    // body is OFF/held. These are separate fixture reads, not RAND visits.
    for (i, bound) in case.bounds.iter().enumerate() {
        if bound.storage.base().area == Area::X {
            lines.push("LDI X7".into());
            lines.push(format!(
                "{} {} D{}",
                if bound.storage.wide() { "DMOV" } else { "MOV" },
                bound.expression,
                390 + i * 2
            ));
        }
    }
    lines.push("END".into());
    case.source = lines.join("\n");
    case.validate_metadata()
        .unwrap_or_else(|e| panic!("{}: {e}", case.name));
    case
}

fn suite() -> Vec<Case> {
    let mut cases = Vec::new();
    for (i, (suffix, _)) in INDEX_ALIASES.into_iter().enumerate() {
        let wide = (i / 2) % 2 == 1;
        let pulse = i % 2 == 1;
        let offset = if i % 2 == 0 { -2_i16 } else { 2 };
        let op = format!(
            "{}{}",
            if wide { "DRAND" } else { "RAND" },
            if pulse { "P" } else { "" }
        );
        let lower = if wide { 65536 } else { 11 };
        let upper = if wide { 131073 } else { 1001 };
        cases.push(fixture(
            &format!("bank_{suffix}"),
            &op,
            [
                Bound::numeric(
                    &format!("D100{suffix}"),
                    &format!("D{}", 100 + offset),
                    wide,
                    lower,
                ),
                Bound::numeric(
                    &format!("D104{suffix}"),
                    &format!("D{}", 104 + offset),
                    wide,
                    upper,
                ),
            ],
            &format!("D300{suffix}"),
            Storage::numeric(&format!("D{}", 300 + offset), wide),
            indices(&[(suffix, offset)]),
            Some("D300"),
        ));
    }
    cases.extend([
        fixture(
            "short_y_x_source",
            "RAND",
            [
                Bound::group("K1X20F1", "X20", 4, false, 3),
                Bound::group("K1M80E1", "M80", 4, false, 12),
            ],
            "K1Y20E0",
            Storage::group("Y21", 4, false),
            indices(&[("E0", 1), ("E1", 0), ("F1", 0)]),
            None,
        ),
        fixture(
            "short_m_pulse",
            "RANDP",
            [
                Bound::numeric("D100E0", "D100", false, 10),
                Bound::numeric("D104F0", "D104", false, 100),
            ],
            "K2M200E1",
            Storage::group("M201", 8, false),
            indices(&[("E0", 0), ("F0", 0), ("E1", 1)]),
            None,
        ),
        fixture(
            "wide_short_s_direct_sources",
            "DRAND",
            [
                Bound::numeric("E1", "D1182", true, 32768),
                Bound::numeric("F3", "D1187", true, 60000),
            ],
            "K4S128F7",
            Storage::group("S126", 16, true),
            indices(&[("F7", -2)]),
            None,
        ),
        fixture(
            "wide_full_m_pulse",
            "DRANDP",
            [
                Bound::group("K8M80E0", "M80", 32, true, 65536),
                Bound::numeric("F1", "D1183", true, 196607),
            ],
            "K8M256E7",
            Storage::group("M257", 32, true),
            indices(&[("E0", 0), ("E7", 1)]),
            None,
        ),
        fixture(
            "timer_pair",
            "DRAND",
            [
                Bound::numeric("D100E0", "D100", true, 10),
                Bound::numeric("D104E0", "D104", true, 1000),
            ],
            "T20F0",
            Storage::numeric("T22", true),
            indices(&[("E0", 0), ("F0", 2)]),
            None,
        ),
        fixture(
            "counter_199_low_only",
            "DRANDP",
            [
                Bound::numeric("D100E1", "D100", true, 500),
                Bound::numeric("D104E1", "D104", true, 1000),
            ],
            "C200E0",
            Storage::numeric("C199", true),
            indices(&[("E0", -1), ("E1", 0)]),
            None,
        ),
        fixture(
            "counter_255_natural",
            "DRAND",
            [
                Bound::numeric("C200E1", "C200", true, 65536),
                Bound::numeric("C201E1", "C201", true, 196607),
            ],
            "C254F7",
            Storage::numeric("C255", true),
            indices(&[("E1", 0), ("F7", 1)]),
            None,
        ),
        fixture(
            "direct_f_word_target",
            "RANDP",
            [
                Bound::numeric("D100E0", "D100", false, 10),
                Bound::numeric("D104E0", "D104", false, 100),
            ],
            "F1",
            Storage::numeric("D1183", false),
            indices(&[("E0", 0)]),
            None,
        ),
        fixture(
            "direct_f_wide_target",
            "DRANDP",
            [
                Bound::numeric("D100E0", "D100", true, 65536),
                Bound::numeric("D104E0", "D104", true, 131073),
            ],
            "F7",
            Storage::numeric("D1195", true),
            indices(&[("E0", 0)]),
            None,
        ),
        fixture(
            "negative_domain_hold",
            "RAND",
            [
                Bound::numeric("D100E0", "D100", false, -1),
                Bound::numeric("D104F0", "D104", false, 7),
            ],
            "D300E1",
            Storage::numeric("D302", false),
            indices(&[("E0", 0), ("F0", 0), ("E1", 2)]),
            Some("D300"),
        ),
        fixture(
            "reversed_domain_pulse_hold",
            "DRANDP",
            [
                Bound::numeric("D100E0", "D100", true, 10),
                Bound::numeric("D104F0", "D104", true, 1),
            ],
            "D300E1",
            Storage::numeric("D302", true),
            indices(&[("E0", 0), ("F0", 0), ("E1", 2)]),
            Some("D300"),
        ),
        fixture(
            "equal_domain",
            "DRAND",
            [
                Bound::numeric("D100E0", "D100", true, 7),
                Bound::numeric("D104E0", "D104", true, 7),
            ],
            "D300F0",
            Storage::numeric("D298", true),
            indices(&[("E0", 0), ("F0", -2)]),
            Some("D300"),
        ),
    ]);
    cases
}

/// Admission check, not an alternate tail dispatcher. These fixtures need only
/// inert operand WORDs; deliberate control collisions belong to another corpus.
fn inert_tail_word(word: u16) -> bool {
    match word as u8 & 0xf0 {
        0x80 | 0x90 | 0xe0 => true,
        0xf0 => !(0xf6..=0xfd).contains(&(word >> 8)),
        _ => false,
    }
}
#[derive(Clone, Debug, Serialize)]
struct Encoding {
    header_pc: u16,
    tested_words: Vec<u16>,
    program_word_count: usize,
    program_words_sha256: String,
}
fn encoding(case: &Case) -> Result<Encoding, String> {
    case.validate_metadata()?;
    let program = opendvp::plc::compile_il(&case.source).map_err(|e| e.to_string())?;
    let encoded = opendvp::es2_encoder::encode(&program)?;
    let nodes: Vec<_> = encoded
        .execution_plan
        .nodes
        .iter()
        .filter(|node| {
            if let PlanOp::Action {
                network, action, ..
            } = node.op
            {
                program.networks[network].actions[action].label() == case.instruction()
            } else {
                false
            }
        })
        .collect();
    if nodes.len() != 1 {
        return Err(format!(
            "Expected exactly one tested action, got {}",
            nodes.len()
        ));
    }
    let header_pc = nodes[0].pc;
    let length = if case.wide { 13 } else { 7 };
    let tested_words = encoded
        .words
        .get(usize::from(header_pc)..usize::from(header_pc) + length)
        .ok_or("Truncated native instruction")?
        .to_vec();
    let expected_header =
        (if case.wide { 0x5501 } else { 0x5401 }) | if case.pulse { 0x10 } else { 0 };
    if tested_words[0] != expected_header {
        return Err("Tested action header mismatch".into());
    }
    if case.wide && !tested_words[7..].iter().all(|w| inert_tail_word(*w)) {
        return Err(
            "This independent-stream suite excludes DRAND control-colliding operand tails".into(),
        );
    }
    let bytes: Vec<_> = encoded.words.iter().flat_map(|w| w.to_le_bytes()).collect();
    Ok(Encoding {
        header_pc,
        tested_words,
        program_word_count: encoded.words.len(),
        program_words_sha256: format!("{:x}", Sha256::digest(bytes)),
    })
}

#[derive(Serialize)]
struct Failure {
    scan: u64,
    property: String,
    side: String,
    detail: String,
}
#[derive(Serialize)]
struct Sample {
    scan: u64,
    power: bool,
    instruction_enabled: bool,
    local_bounds: [i32; 2],
    original_bounds: [i32; 2],
    local_value: i32,
    original_value: i32,
    readings: Vec<Reading>,
}
#[derive(Serialize)]
struct Report {
    fixture: Case,
    encoding: Option<Encoding>,
    status: &'static str,
    property_checks: usize,
    paired_nonrandom_checks: usize,
    verified_instruction_steps: u64,
    verified_random_gates: u64,
    samples: Vec<Sample>,
    failures: Vec<Failure>,
    error: Option<String>,
}
impl Report {
    fn check(&mut self, scan: u64, property: &str, side: &str, passed: bool, detail: String) {
        self.property_checks += 1;
        if !passed {
            self.failures.push(Failure {
                scan,
                property: property.into(),
                side: side.into(),
                detail,
            });
        }
    }
    fn paired(&mut self, scan: u64, rows: &[Reading]) {
        for row in rows
            .iter()
            .filter(|r| !self.fixture.effective_target.contains_row(r))
        {
            self.paired_nonrandom_checks += 1;
            if row.local != row.original {
                self.failures.push(Failure {
                    scan,
                    property: "nonrandom_row_equality".into(),
                    side: "paired".into(),
                    detail: format!(
                        "{} {}: local={}, original={}",
                        row.device, row.kind, row.local, row.original
                    ),
                });
            }
        }
    }
    fn fixed_reading(
        &mut self,
        scan: u64,
        rows: &[Reading],
        original: bool,
        expected_reading: (Device, &str, u32),
        property: &str,
    ) -> Result<(), String> {
        let (d, kind, expected) = expected_reading;
        let value = observed(rows, d, kind, original)?;
        self.check(
            scan,
            property,
            if original { "original" } else { "local" },
            value == expected,
            format!("{d}/{kind}={value}, expected={expected}"),
        );
        Ok(())
    }
}

fn run(path: &Path, case: Case) -> Report {
    let mut report = Report {
        fixture: case.clone(),
        encoding: None,
        status: "error",
        property_checks: 0,
        paired_nonrandom_checks: 0,
        verified_instruction_steps: 0,
        verified_random_gates: 0,
        samples: Vec::new(),
        failures: Vec::new(),
        error: None,
    };
    let result = (|| -> Result<(), String> {
        let wire = encoding(&case)?;
        let error_pc = u32::from(wire.header_pc + if case.wide { 5 } else { 3 });
        report.encoding = Some(wire);
        let mut session = Session::launch(path, Arc::new(AtomicBool::new(false)))?;
        let initial = session.upload(&case.source)?;
        report.paired(initial.scan, &initial.rows);
        let mut previous_values = [case.effective_target.initial_value(); 2];
        let mut previous_power = false;
        let mut post_counter = 0_u32;
        let mut error_seen = [false; 2];
        for power in POWERS {
            let mut inputs = vec![(device("X0"), power), (device("X1"), true)];
            inputs.extend(
                case.fixed_inputs
                    .iter()
                    .map(|i| (device(&i.device), i.value)),
            );
            let current = session.step(&inputs)?;
            report.verified_instruction_steps = session.verified_instruction_steps();
            report.verified_random_gates = session.verified_random_gates();
            let enabled = power && (!case.pulse || !previous_power);
            let mut values = [0_i32; 2];
            let mut side_bounds = [[0_i32; 2]; 2];
            if power {
                post_counter += 1;
            }
            for (side_index, original) in [false, true].into_iter().enumerate() {
                let side = if original { "original" } else { "local" };
                for (bound_index, bound) in case.bounds.iter().enumerate() {
                    let value = bound.storage.value(&current.rows, original)?;
                    side_bounds[side_index][bound_index] = value;
                    report.check(
                        current.scan,
                        "fixed_source_observation",
                        side,
                        value == bound.expected,
                        format!(
                            "{}: observed={value}, fixture={}",
                            bound.expression, bound.expected
                        ),
                    );
                }
                let [lower, upper] = side_bounds[side_index];
                let domain_error = lower < 0 || upper < 0 || lower > upper;
                error_seen[side_index] |= enabled && domain_error;
                let value = case.effective_target.value(&current.rows, original)?;
                values[side_index] = value;
                if enabled && !domain_error {
                    report.check(
                        current.scan,
                        if lower == upper {
                            "equal_bound_exact_result"
                        } else {
                            "inclusive_own_stream_bounds"
                        },
                        side,
                        (lower..=upper).contains(&value),
                        format!("own bounds {lower} <= {value} <= {upper}"),
                    );
                } else {
                    let property = if enabled {
                        "domain_error_target_hold"
                    } else if power {
                        "held_pulse_target_hold"
                    } else {
                        "disabled_target_hold"
                    };
                    report.check(
                        current.scan,
                        property,
                        side,
                        value == previous_values[side_index],
                        format!(
                            "value={value}, own previous={}",
                            previous_values[side_index]
                        ),
                    );
                }
                for index in &case.fixed_indices {
                    report.fixed_reading(
                        current.scan,
                        &current.rows,
                        original,
                        (device(&index.alias), "word", u32::from(index.value as u16)),
                        "offset_stays_fixed",
                    )?;
                }
                for guard in &case.guards {
                    report.fixed_reading(
                        current.scan,
                        &current.rows,
                        original,
                        (device(&guard.device), guard.kind, guard.expected),
                        "neighbor_or_label_base_guard",
                    )?;
                }
                let error = error_seen[side_index];
                for (raw, kind, expected, property) in [
                    ("M1020", "bit", 1, "zero_flag_preserved"),
                    ("M1021", "bit", 1, "borrow_flag_preserved"),
                    ("M1022", "bit", 1, "carry_flag_preserved"),
                    ("M1067", "bit", u32::from(error), "operation_error_latch"),
                    ("M1068", "bit", u32::from(error), "operation_pc_latch"),
                    (
                        "D1067",
                        "word",
                        if error { 0x0e1a } else { 0 },
                        "domain_error_code",
                    ),
                    (
                        "D1068",
                        "word",
                        if error { error_pc } else { 0 },
                        "domain_error_pc",
                    ),
                    (
                        "D400",
                        "word",
                        if post_counter > 0 { 123 } else { 0 },
                        "post_instruction_marker",
                    ),
                    ("D401", "word", post_counter, "post_instruction_counter"),
                    ("Y0", "bit", u32::from(power), "post_instruction_power"),
                ] {
                    report.fixed_reading(
                        current.scan,
                        &current.rows,
                        original,
                        (device(raw), kind, expected),
                        property,
                    )?;
                }
            }
            report.paired(current.scan, &current.rows);
            report.samples.push(Sample {
                scan: current.scan,
                power,
                instruction_enabled: enabled,
                local_bounds: side_bounds[0],
                original_bounds: side_bounds[1],
                local_value: values[0],
                original_value: values[1],
                readings: current.rows,
            });
            previous_values = values;
            previous_power = power;
        }
        Ok(())
    })();
    report.error = result.err();
    if report.error.is_none() {
        report.status = if report.failures.is_empty() {
            "invariants_pass"
        } else {
            "invariants_fail"
        };
    }
    report
}

fn file_sha256(path: &Path) -> Result<String, String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(std::fs::read(path).map_err(|e| e.to_string())?)
    ))
}
fn main() -> Result<(), String> {
    let mut simulator = PathBuf::from(DEFAULT_SIMULATOR);
    let mut output = PathBuf::from("indexed-random-invariants.json");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--simulator" => {
                simulator = PathBuf::from(args.next().ok_or("--simulator needs a path")?)
            }
            "--output" => output = PathBuf::from(args.next().ok_or("--output needs a path")?),
            "--help" => {
                println!(
                    "indexed_random_invariants [--simulator EXE] [--output JSON]\nIndependent addressed RAND stream invariants; no random-value equality or seed synchronization."
                );
                return Ok(());
            }
            _ => return Err(format!("Unknown argument: {arg}")),
        }
    }
    let simulator = std::fs::canonicalize(simulator).map_err(|e| e.to_string())?;
    let target_sha256 = file_sha256(&simulator)?;
    let model = match target_sha256.as_str() {
        TARGET_SHA256 | COMMGR_ES2_SHA256 => "ES2",
        SS2_SHA256 | COMMGR_SS2_SHA256 => "SS2",
        _ => return Err("Unrecognized simulator SHA-256".into()),
    };
    let producer = std::env::current_exe().map_err(|e| e.to_string())?;
    let producer_sha256 = file_sha256(&producer)?;
    let mut reports = Vec::new();
    for case in suite() {
        let report = run(&simulator, case);
        println!(
            "{}: {} ({} properties, {} nonrandom comparisons, {} verified steps, {} failures){}",
            report.fixture.name,
            report.status,
            report.property_checks,
            report.paired_nonrandom_checks,
            report.verified_instruction_steps,
            report.failures.len(),
            report
                .error
                .as_ref()
                .map(|e| format!(" — {e}"))
                .unwrap_or_default()
        );
        reports.push(report);
    }
    let bad = reports.iter().any(|r| r.status != "invariants_pass");
    let data = serde_json::json!({
        "schema_version": 1, "evidence_kind": "independent_addressed_random_invariants",
        "target_model": model, "target_executable": simulator, "target_sha256": target_sha256,
        "producer_executable": producer, "producer_sha256": producer_sha256,
        "harness_source_sha256": format!("{:x}", Sha256::digest(include_bytes!("indexed_random_invariants.rs"))),
        "scan_ms": SCAN_MS, "stage_x0": POWERS, "fixture_count": reports.len(),
        "method": "New owned Session per fixture; verified upload and every native instruction PC. Each stream uses its own observed fixed bounds and previous result. No RNG state transfer, random-value equality assertion, or distribution claim.",
        "random_row_exclusion": "Only explicitly constructed effective destination storage rows, including row kind; label-base and C/T contact rows remain checked.",
        "limits": [
            "Finite 28-fixture invariant suite, not an exhaustive randomness or distribution proof",
            "Offsets are explicitly initialized once and observed every scan; targets cannot feed future bounds or indices",
            "Non-equal domains fit the complete stored target width; truncation parity is covered separately",
            "Only inert DRAND operand tails admitted here; deliberate control collisions belong to the separate parity corpus",
            "Host-timed native and virtual local random streams remain independent; neither equal nor unequal samples prove stream equivalence"
        ],
        "cases": reports,
    });
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        &output,
        serde_json::to_vec_pretty(&data).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!("Report: {}", output.display());
    if bad {
        Err("Addressed random invariants failed or remain inconclusive; inspect report".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_external_bound_fixture_observes_inputs_without_sampling_rng() {
        let case = suite()
            .into_iter()
            .find(|c| c.name.ends_with("short_y_x_source"))
            .unwrap();
        let program = opendvp::plc::compile_il(&case.source).unwrap();
        let mut sim = opendvp::plc::Simulator::default();
        sim.memory.random_tail_profile = opendvp::random_tail::RandomTailProfile::CurrentCommgr;
        sim.memory.set_input(device("X1"), true);
        for input in &case.fixed_inputs {
            sim.memory.set_input(device(&input.device), input.value);
        }
        sim.scan(&program, 0);
        assert_eq!(sim.last_scan, opendvp::plc::ScanStatus::Complete);
        assert!(sim.memory.execution_limitations.is_empty());
        assert_eq!(sim.memory.word(device("D390")), 3);
        assert!(sim.memory.random_state.is_none());
        for index in 16..20 {
            assert!(sim.observed_devices.contains(&Device {
                area: Area::X,
                index
            }));
        }
    }

    #[test]
    fn all_four_forms_and_sixteen_banks_have_fixed_nonaliased_fixtures() {
        let cases = suite();
        assert_eq!(cases.len(), 28);
        assert_eq!(
            cases
                .iter()
                .map(|c| c.name.clone())
                .collect::<BTreeSet<_>>()
                .len(),
            28
        );
        assert_eq!(
            cases.iter().map(|c| c.op.as_str()).collect::<BTreeSet<_>>(),
            BTreeSet::from(["RAND", "RANDP", "DRAND", "DRANDP"])
        );
        for (name, alias) in INDEX_ALIASES {
            let case = cases
                .iter()
                .find(|c| c.name == format!("indexed_random_invariant_bank_{name}"))
                .unwrap();
            assert_eq!(case.fixed_indices[0].alias, alias);
        }
        for case in cases {
            case.validate_metadata().unwrap();
        }
    }

    #[test]
    fn each_fixture_compiles_with_exact_header_length_and_inert_tail() {
        for case in suite() {
            let wire = encoding(&case).unwrap_or_else(|e| panic!("{}: {e}", case.name));
            assert_eq!(wire.tested_words.len(), if case.wide { 13 } else { 7 });
            assert!(wire.header_pc > 0);
            assert_eq!(wire.program_words_sha256.len(), 64);
        }
    }

    #[test]
    fn exclusion_is_actual_storage_and_row_kind_not_original_label_base() {
        let moved = Storage::numeric("D302", true);
        let row = |raw: &str, kind| Reading {
            device: device(raw),
            kind,
            local: 1,
            original: 2,
        };
        assert!(!moved.contains_row(&row("D300", "word")));
        assert!(moved.contains_row(&row("D302", "word")));
        assert!(moved.contains_row(&row("D303", "word")));
        assert!(!moved.contains_row(&row("D304", "word")));
        let natural = Storage::numeric("C255", true);
        assert!(natural.contains_row(&row("C255", "dword")));
        assert!(!natural.contains_row(&row("C255", "bit")));
        let boundary = Storage::numeric("C199", true);
        assert!(boundary.contains_row(&row("C199", "word")));
        assert!(!boundary.contains_row(&row("C199", "bit")));
        assert!(!boundary.contains_row(&row("C200", "dword")));
    }

    #[test]
    fn independent_literal_storage_oracles_preserve_counter_and_group_width() {
        let rows = [
            Reading {
                device: device("C199"),
                kind: "word",
                local: 0xabcd,
                original: 0x1234,
            },
            Reading {
                device: device("C200"),
                kind: "dword",
                local: 0x76543210,
                original: 0x12345678,
            },
        ];
        assert_eq!(
            Storage::numeric("C199", true).value(&rows, false).unwrap(),
            43981
        );
        assert_eq!(
            Storage::numeric("C199", true).value(&rows, true).unwrap(),
            4660
        );
        assert_eq!(
            Storage::numeric("C200", true).value(&rows, false).unwrap(),
            0x76543210
        );
        assert_eq!(Storage::group("M201", 8, false).initial_value(), 255);
        assert_eq!(Storage::group("S126", 16, true).initial_value(), 65535);
        assert_eq!(Storage::group("M257", 32, true).initial_value(), -1);
    }

    #[test]
    fn safe_tail_admission_distinguishes_e_from_control_colliding_f() {
        for word in [0x0080, 0x0090, 0xfae6, 0x58f6, 0x80f4, 0xf4f6, 0xfef6] {
            assert!(inert_tail_word(word));
        }
        for word in [0xf6f6, 0xf8f6, 0xfaf6, 0xfcf6, 0xf6f4, 0xfdf4, 0x0000] {
            assert!(!inert_tail_word(word));
        }
    }
}
