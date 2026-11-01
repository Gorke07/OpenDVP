//! Property checks for independent native/local random streams, never equality.
use opendvp::{
    es2_bridge::{
        COMMGR_ES2_SHA256, COMMGR_SS2_SHA256, DEFAULT_SIMULATOR, Reading, SCAN_MS, SS2_SHA256,
        Session, TARGET_SHA256,
    },
    plc::{Area, Device},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Destination {
    Words {
        first: u16,
        wide: bool,
    },
    Alias {
        name: String,
        first: u16,
        wide: bool,
    },
    Group {
        first: u16,
        bits: u16,
        wide: bool,
    },
}
impl Destination {
    fn source(&self) -> String {
        match self {
            Self::Words { first, .. } => format!("D{first}"),
            Self::Alias { name, .. } => name.clone(),
            Self::Group { first, bits, .. } => format!("K{}M{first}", bits / 4),
        }
    }
    fn contains(&self, d: Device) -> bool {
        match self {
            Self::Words { first, wide } | Self::Alias { first, wide, .. } => {
                d.area == Area::D && (d.index == *first || (*wide && d.index == first + 1))
            }
            Self::Group { first, bits, .. } => {
                d.area == Area::M && (*first..first + bits).contains(&d.index)
            }
        }
    }
    fn value(&self, rows: &[Reading], original: bool) -> Result<i32, String> {
        let get = |area, index, kind| {
            rows.iter()
                .find(|r| r.device == Device { area, index } && r.kind == kind)
                .map(|r| if original { r.original } else { r.local })
                .ok_or_else(|| format!("Missing observation {area:?}{index}/{kind}"))
        };
        let (value, wide) = match self {
            Self::Words { first, wide } | Self::Alias { first, wide, .. } => {
                let low = get(Area::D, *first, "word")?;
                let value = if *wide {
                    low | (get(Area::D, first + 1, "word")? << 16)
                } else {
                    low
                };
                (value, *wide)
            }
            Self::Group { first, bits, wide } => {
                let mut value = 0;
                for bit in 0..*bits {
                    value |= get(Area::M, first + bit, "bit")? << bit;
                }
                (value, *wide)
            }
        };
        Ok(if wide {
            value as i32
        } else {
            i32::from(value as i16)
        })
    }
}

struct Case {
    name: String,
    source: String,
    lower: i32,
    upper: i32,
    pulse: bool,
    destination: Destination,
}

fn suite() -> Vec<Case> {
    let mut cases = Vec::new();
    for wide in [false, true] {
        for pulse in [false, true] {
            let mut bounds = vec![
                (0, 1),
                (1, 2),
                (2, 3),
                (3, 4),
                (0, 32767),
                (32766, 32767),
                (101, 1001),
                (100, 1000),
            ];
            if wide {
                bounds.extend([
                    (0, i32::MAX),
                    (i32::MAX - 1, i32::MAX),
                    (65536, 131073),
                    (1_000_000_000, 2_000_000_000),
                ]);
            }
            for (index, (lower, upper)) in bounds.into_iter().enumerate() {
                cases.push(make_case(
                    wide,
                    pulse,
                    lower,
                    upper,
                    Destination::Words { first: 20, wide },
                    index % 2 == 0,
                ));
            }
            cases.push(make_case(
                wide,
                pulse,
                10,
                100,
                Destination::Group {
                    first: 2000,
                    bits: if wide { 32 } else { 16 },
                    wide,
                },
                false,
            ));
        }
    }
    // Direct index destinations are checked against literal expected D aliases,
    // independently of the parser's alias helper. Both streams must satisfy
    // the same bounds and hold rules; random values are never paired for equality.
    for wide in [false, true] {
        for pulse in [false, true] {
            for (index, first) in [1028, 1182, 1184, 1186, 1188, 1190, 1192, 1194]
                .into_iter()
                .enumerate()
            {
                for high in [false, true] {
                    cases.push(make_case(
                        wide,
                        pulse,
                        if wide { 65536 } else { 10 },
                        if wide { 131073 } else { 100 },
                        Destination::Alias {
                            name: format!("{}{index}", if high { "F" } else { "E" }),
                            first: first + u16::from(high),
                            wide,
                        },
                        false,
                    ));
                }
            }
            for (left, right, left_alias, right_alias) in [
                ("E0", "F1", 1028, 1183),
                ("F0", "E7", 1029, 1194),
                ("E1", "F3", 1182, 1187),
            ] {
                let lower = if wide { 65536 } else { 11 };
                let upper = if wide { 262144 } else { 1000 };
                let mut c = make_case(
                    wide,
                    pulse,
                    lower,
                    upper,
                    Destination::Words { first: 20, wide },
                    false,
                );
                let mov = if wide { "DMOV" } else { "MOV" };
                c.source = c.source.replace(
                    "LD X0\n",
                    &format!(
                        "{mov} K{lower} D{left_alias}\n{mov} K{upper} D{right_alias}\nLD X0\n"
                    ),
                );
                c.source = c
                    .source
                    .replace(" D0 D4 D20", &format!(" {left} {right} D20"));
                c.name.push_str(&format!("_bounds_{left}_{right}"));
                cases.push(c);
            }
        }
    }
    cases
}

fn make_case(
    wide: bool,
    pulse: bool,
    lower: i32,
    upper: i32,
    destination: Destination,
    constants: bool,
) -> Case {
    let op = format!(
        "{}{}",
        if wide { "DRAND" } else { "RAND" },
        if pulse { "P" } else { "" }
    );
    let mov = if wide { "DMOV" } else { "MOV" };
    let seed_dest = match destination {
        Destination::Words { first, .. } | Destination::Alias { first, .. } => {
            format!("{mov} K-12345 D{first}")
        }
        Destination::Group { first, bits, .. } => (first..first + bits)
            .map(|i| format!("SET M{i}"))
            .collect::<Vec<_>>()
            .join("\n"),
    };
    let bounds = if constants {
        format!("K{lower} K{upper}")
    } else {
        "D0 D4".into()
    };
    let source = format!(
        "LDP X1\n{seed_dest}\n{mov} K{lower} D0\n{mov} K{upper} D4\nSET M1020\nSET M1021\nSET M1022\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\nLD X0\n{op} {bounds} {}\nMOV K123 D30\nINC D31\nOUT Y0\nEND",
        destination.source()
    );
    Case {
        name: format!(
            "{op}_{lower}_{upper}_{}_{}",
            destination.source(),
            if constants { "constants" } else { "registers" }
        ),
        source,
        lower,
        upper,
        pulse,
        destination,
    }
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
    x0: bool,
    instruction_enabled: bool,
    local_value: i32,
    original_value: i32,
    readings: Vec<Reading>,
}
#[derive(Serialize)]
struct Report {
    name: String,
    source: String,
    lower: i32,
    upper: i32,
    pulse: bool,
    destination: Destination,
    status: &'static str,
    property_checks: usize,
    paired_nonrandom_checks: usize,
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
        for row in rows.iter().filter(|r| !self.destination.contains(r.device)) {
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
}

fn run(path: &Path, case: Case) -> Report {
    let mut report = Report {
        name: case.name,
        source: case.source,
        lower: case.lower,
        upper: case.upper,
        pulse: case.pulse,
        destination: case.destination,
        status: "error",
        property_checks: 0,
        paired_nonrandom_checks: 0,
        samples: Vec::new(),
        failures: Vec::new(),
        error: None,
    };
    let result = (|| -> Result<(), String> {
        // Each case owns a new process; upload also verifies the exact bytes.
        let mut session = Session::launch(path, Arc::new(AtomicBool::new(false)))?;
        let initial = session.upload(&report.source)?;
        report.paired(initial.scan, &initial.rows);
        let mut previous = [0, 0];
        let mut previous_power = false;
        let mut counter = 0;
        let x0: Device = "X0".parse()?;
        let x1: Device = "X1".parse()?;
        for (stage, power) in [false, true, true, false, true].into_iter().enumerate() {
            let current = session.step(&[(x0, power), (x1, true)])?;
            let enabled = power && (!report.pulse || !previous_power);
            let values = [
                report.destination.value(&current.rows, false)?,
                report.destination.value(&current.rows, true)?,
            ];
            if power {
                counter += 1;
            }
            for (side_index, side) in ["local", "original"].into_iter().enumerate() {
                let value = values[side_index];
                if enabled {
                    report.check(
                        current.scan,
                        "inclusive_bounds",
                        side,
                        (report.lower..=report.upper).contains(&value),
                        format!("{} <= {value} <= {}", report.lower, report.upper),
                    );
                } else {
                    let expected = if stage == 0 {
                        if matches!(report.destination, Destination::Group { .. }) {
                            -1
                        } else {
                            -12345
                        }
                    } else {
                        previous[side_index]
                    };
                    report.check(
                        current.scan,
                        if power { "pulse_hold" } else { "disabled_hold" },
                        side,
                        value == expected,
                        format!("value={value}, previous={expected}"),
                    );
                }
                for (area, index, kind, expected, property) in [
                    (Area::M, 1020, "bit", 1, "zero_flag_preserved"),
                    (Area::M, 1021, "bit", 1, "borrow_flag_preserved"),
                    (Area::M, 1022, "bit", 1, "carry_flag_preserved"),
                    (Area::M, 1067, "bit", 0, "no_operation_error"),
                    (Area::M, 1068, "bit", 0, "no_error_pc"),
                    (Area::D, 1067, "word", 0, "error_code_held"),
                    (Area::D, 1068, "word", 0, "error_pc_held"),
                    (
                        Area::D,
                        30,
                        "word",
                        if counter > 0 { 123 } else { 0 },
                        "post_instruction_marker",
                    ),
                    (Area::D, 31, "word", counter, "post_instruction_counter"),
                    (
                        Area::Y,
                        0,
                        "bit",
                        u32::from(power),
                        "post_instruction_power",
                    ),
                ] {
                    let row = current
                        .rows
                        .iter()
                        .find(|r| r.device == Device { area, index } && r.kind == kind)
                        .ok_or_else(|| {
                            format!("Missing invariant reading {area:?}{index}/{kind}")
                        })?;
                    let observed = if side_index == 0 {
                        row.local
                    } else {
                        row.original
                    };
                    report.check(
                        current.scan,
                        property,
                        side,
                        observed == expected,
                        format!("observed={observed}, expected={expected}"),
                    );
                }
            }
            report.paired(current.scan, &current.rows);
            report.samples.push(Sample {
                scan: current.scan,
                x0: power,
                instruction_enabled: enabled,
                local_value: values[0],
                original_value: values[1],
                readings: current.rows,
            });
            previous = values;
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

fn main() -> Result<(), String> {
    let mut path = PathBuf::from(DEFAULT_SIMULATOR);
    let mut output = PathBuf::from("es2-random-invariants.json");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--simulator" => path = PathBuf::from(args.next().ok_or("--simulator needs a path")?),
            "--output" => output = PathBuf::from(args.next().ok_or("--output needs a path")?),
            "--help" => {
                println!(
                    "random_invariants [--simulator EXE] [--output JSON]\nIndependent stream properties, not numerical random-stream equality."
                );
                return Ok(());
            }
            _ => return Err(format!("Unknown argument: {arg}")),
        }
    }
    let digest = format!(
        "{:x}",
        Sha256::digest(std::fs::read(&path).map_err(|e| e.to_string())?)
    );
    let model = match digest.as_str() {
        TARGET_SHA256 | COMMGR_ES2_SHA256 => "ES2",
        SS2_SHA256 | COMMGR_SS2_SHA256 => "SS2",
        _ => return Err("Unrecognized simulator SHA-256".into()),
    };
    let mut reports = Vec::new();
    for case in suite() {
        let report = run(&path, case);
        println!(
            "{}: {} ({} property checks, {} paired nonrandom checks, {} failures){}",
            report.name,
            report.status,
            report.property_checks,
            report.paired_nonrandom_checks,
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
    let producer = std::env::current_exe().map_err(|e| e.to_string())?;
    let producer_sha256 = format!(
        "{:x}",
        Sha256::digest(std::fs::read(&producer).map_err(|e| e.to_string())?)
    );
    let data = serde_json::json!({
        "schema_version": 1, "evidence_kind": "invariants", "target_model": model, "target_sha256": digest,
        "scan_ms": SCAN_MS,
        "producer_executable": producer, "producer_sha256": producer_sha256,
        "method": "Independent owned native/local streams; verified upload and every native PC step; no numerical equality assertion for random destinations",
        "stage_x0": [false, true, true, false, true],
        "limits": ["Finite bounds/state/operand corpus only; no distribution or exhaustive randomness proof", "Local seed1 and virtual state differ from native host-timer state", "Matching random samples across streams is neither required nor evidence of whole-stream equivalence"],
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
        Err("Random invariants failed or remain inconclusive; inspect report".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn index_properties_compile_and_track_each_complete_destination() {
        let cases = super::suite();
        assert_eq!(cases.len(), 120);
        let mut names = std::collections::BTreeSet::new();
        for case in cases {
            assert!(names.insert(case.name.clone()), "{}", case.name);
            let p = opendvp::plc::compile_il(&case.source)
                .unwrap_or_else(|e| panic!("{}: {e}", case.name));
            let encoded =
                opendvp::es2_encoder::encode(&p).unwrap_or_else(|e| panic!("{}: {e}", case.name));
            assert!(encoded.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS);
            if let super::Destination::Alias { first, wide, .. } = case.destination {
                for index in first..=first + u16::from(wide) {
                    assert!(
                        p.devices.contains(&opendvp::plc::Device {
                            area: opendvp::plc::Area::D,
                            index
                        }),
                        "{} D{index}",
                        case.name
                    );
                }
            }
        }
    }
}
