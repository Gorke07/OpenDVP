//! RTC fixed-input corpus admission. Never flatten into Case without retaining its initial
//! input, environmental admission, local-only classification and fixed oracles.
//! Parent owns the real RTC input protocol and all native execution.
use super::{Case, Target, case};
use opendvp::es2_bridge::Reading;
use opendvp::plc::Device;
use std::collections::BTreeSet;

const FORMS: [&str; 4] = ["TRD", "TRDP", "TWR", "TWRP"];
const SEED: [i16; 7] = [24, 4, 2, 29, 12, 30, 7];
const WRITE: [i16; 7] = [4, 2, 12, 15, 15, 27, 30];
const BLANK: [i16; 7] = [333; 7];

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(super) enum PreviousSampleSource {
    /// One declared environmental input, captured immediately before the body.
    /// This is independent of the fixture's private seconds byte (which is7).
    DeclaredEnvironmentalCaptureBeforeBody,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(super) enum ObservationBoundary {
    /// Explicit requirement, not an assertion about stock engine scheduling:
    /// every selected node endpoint has completed public RTC publication and
    /// its monitored body window had no elapsed sample/correction event.
    SettledPerNodeNoDeltaWindow,
    /// Direct local/controlled handler semantics, before background service.
    #[cfg(test)]
    LocalHandlerOnly,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(super) struct RtcInitialInputSpec {
    pub private_shadow: [u8; 7],
    pub previous_sample_source: PreviousSampleSource,
    pub initial_m1016: bool,
    pub initial_m1017: bool,
    pub boundary: ObservationBoundary,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(super) enum Admission {
    /// Candidate only after parent implements and verifies environmental input
    /// and settled per-node admission. No current runtime claim.
    NativeCandidate,
    #[cfg(test)]
    LocalHandlerOnly(&'static str),
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(super) struct FixedOracle {
    pub device: String,
    pub kind: &'static str,
    pub raw_after_scans: [u32; 5],
}
pub(super) struct RtcCorpusCase {
    pub comparison: Case,
    pub initial: RtcInitialInputSpec,
    pub admission: Admission,
    /// Absolute error PC is derived at admission from the original primary RTC
    /// header, then +offset. It is not learned from a returned native error PC.
    pub primary_error_offset: Option<u16>,
    pub fixed_oracles: Vec<FixedOracle>,
}
#[derive(Debug, Clone, serde::Serialize)]
pub(super) struct NativeFixture {
    pub initial: RtcInitialInputSpec,
    pub primary_error_offset: Option<u16>,
    pub fixed_oracles: Vec<FixedOracle>,
}
impl NativeFixture {
    pub(super) fn validate_fixed_oracles(
        &self,
        name: &str,
        scan: u64,
        rows: &[Reading],
    ) -> Result<(), String> {
        validate_oracles(
            name,
            Admission::NativeCandidate,
            &self.fixed_oracles,
            scan,
            rows,
        )
    }
    pub(super) fn initial_input(&self) -> Result<opendvp::rtc_admission::RtcInitialInput, String> {
        if self.initial.boundary != ObservationBoundary::SettledPerNodeNoDeltaWindow {
            return Err("RTC local-handler fixture cannot enter native suite".into());
        }
        Ok(opendvp::rtc_admission::RtcInitialInput {
            wire_seed: self.initial.private_shadow,
            controls: opendvp::rtc_admission::RtcControlFlags {
                m1016: self.initial.initial_m1016,
                m1017: self.initial.initial_m1017,
                m1082: false,
            },
        })
    }
    pub(super) fn primary_error_pc(
        &self,
        p: &opendvp::plc::Program,
    ) -> Result<Option<u16>, String> {
        let Some(offset) = self.primary_error_offset else {
            return Ok(None);
        };
        let index = p
            .networks
            .iter()
            .flat_map(|n| &n.actions)
            .position(|a| match a {
                opendvp::plc::Action::IndexedRtc(_) => true,
                opendvp::plc::Action::Pulse(inner) => {
                    matches!(inner.as_ref(), opendvp::plc::Action::IndexedRtc(_))
                }
                _ => false,
            })
            .ok_or("RTC primary instruction is missing")?;
        let encoded = opendvp::es2_encoder::encode(p)?;
        Ok(Some(
            *encoded
                .action_pc
                .get(index)
                .ok_or("RTC source header PC missing")?
                + offset,
        ))
    }
}
impl RtcCorpusCase {
    #[cfg(test)]
    pub(super) fn validate_fixed_oracles(&self, scan: u64, rows: &[Reading]) -> Result<(), String> {
        validate_oracles(
            &self.comparison.name,
            self.admission,
            &self.fixed_oracles,
            scan,
            rows,
        )
    }
    fn into_native_case(self) -> Case {
        assert_eq!(self.admission, Admission::NativeCandidate);
        assert_eq!(
            self.initial.boundary,
            ObservationBoundary::SettledPerNodeNoDeltaWindow
        );
        let mut comparison = self.comparison;
        comparison.rtc = Some(NativeFixture {
            initial: self.initial,
            primary_error_offset: self.primary_error_offset,
            fixed_oracles: self.fixed_oracles,
        });
        comparison
    }
}
fn validate_oracles(
    name: &str,
    admission: Admission,
    oracles: &[FixedOracle],
    scan: u64,
    rows: &[Reading],
) -> Result<(), String> {
    let slot = usize::try_from(scan.checked_sub(1).ok_or("RTC scan starts at1")?)
        .map_err(|_| "RTC scan overflow")?;
    if slot >= 5 {
        return Err("RTC fixed fixture has exactly five scans".into());
    }
    if !matches!(admission, Admission::NativeCandidate) {
        return Err("LocalHandlerOnly fixture cannot claim native fixed-oracle parity".into());
    }
    for oracle in oracles {
        let device = oracle.device.parse::<Device>()?;
        let row = rows
            .iter()
            .find(|r| r.device == device && r.kind == oracle.kind)
            .ok_or_else(|| format!("{}: missing {} {}", name, oracle.kind, oracle.device))?;
        let expected = oracle.raw_after_scans[slot];
        if row.local != expected || row.original != expected {
            return Err(format!(
                "{} scan{scan} {}: fixed expected{expected}, local{}, native{}",
                name, oracle.device, row.local, row.original
            ));
        }
    }
    Ok(())
}
pub(super) fn native_cases(target: Target) -> Vec<Case> {
    common_cases()
        .into_iter()
        .chain(model_cases(target))
        .map(RtcCorpusCase::into_native_case)
        .collect()
}

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (
            vec![("X1", true), ("X0", false), ("X2", false), ("X7", false)],
            1,
        ),
        (vec![("X1", false), ("X0", true)], 2),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}
fn changing_stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (
            vec![("X1", true), ("X0", false), ("X2", false), ("X7", false)],
            1,
        ),
        (vec![("X1", false), ("X0", true)], 1),
        (vec![("X2", true)], 1),
        (vec![("X0", false)], 1),
        (vec![("X0", true)], 1),
    ]
}
fn is_read(op: &str) -> bool {
    op.starts_with("TRD")
}
fn pulse(op: &str) -> bool {
    matches!(op, "TRDP" | "TWRP")
}
fn word(area: &str, index: u16) -> String {
    format!("{area}{index}")
}
fn block_seed(area: &str, base: u16, values: [i16; 7]) -> String {
    values
        .into_iter()
        .enumerate()
        .map(|(i, v)| format!("MOV K{v} {area}{}", base + i as u16))
        .collect::<Vec<_>>()
        .join("\n")
}
fn observe_block(area: &str, base: u16, out: u16, count: u16) -> String {
    (0..count)
        .map(|i| format!("MOV {area}{} D{}", base + i, out + i))
        .collect::<Vec<_>>()
        .join("\n")
}
fn phase(initial: [i16; 7], later: [i16; 7]) -> [[i16; 7]; 5] {
    [initial, later, later, later, later]
}
fn add_words(c: &mut RtcCorpusCase, out: u16, values: [[i16; 7]; 5], count: usize) {
    for field in 0..count {
        c.fixed_oracles.push(FixedOracle {
            device: format!("D{}", out + field as u16),
            kind: "word",
            raw_after_scans: values.map(|v| u32::from(v[field] as u16)),
        });
    }
}
fn add_word(c: &mut RtcCorpusCase, device: &str, values: [i16; 5]) {
    c.fixed_oracles.push(FixedOracle {
        device: device.into(),
        kind: "word",
        raw_after_scans: values.map(|v| u32::from(v as u16)),
    });
}
fn add_bit(c: &mut RtcCorpusCase, device: &str, values: [bool; 5]) {
    c.fixed_oracles.push(FixedOracle {
        device: device.into(),
        kind: "bit",
        raw_after_scans: values.map(u32::from),
    });
}
fn clock(c: &mut RtcCorpusCase, values: [[i16; 7]; 5]) {
    add_words(c, 600, values, 7);
}
fn no_error(c: &mut RtcCorpusCase) {
    add_bit(c, "Y24", [false; 5]);
    add_bit(c, "Y25", [false; 5]);
    add_word(c, "D900", [0x1234; 5]);
}
fn plc_error(c: &mut RtcCorpusCase, offset: u16) {
    c.primary_error_offset = Some(offset);
    add_bit(c, "Y24", [false, true, true, true, true]);
    add_bit(c, "Y25", [false, true, true, true, true]);
    add_word(c, "D900", [0x1234, 0xe1a, 0xe1a, 0xe1a, 0xe1a]);
}
fn fixture(name: &str, seed: &str, body: &str, observe: &str) -> RtcCorpusCase {
    let gate = if body.starts_with("LD") || body.starts_with("STL") {
        ""
    } else {
        "LD X0"
    };
    let source=[
        "LD X1\nMOV K0 E0\nMOV K0 F0\nRST M1016\nRST M1017\nRST M1082\nRST M1067\nRST M1068\nMOV H1234 D1067\nMOV K77 D1068",seed,
        "LDI X7\nSET M1020\nRST M1021\nSET M1022",gate,body,
        "LD M1020\nOUT Y20\nLD M1021\nOUT Y21\nLD M1022\nOUT Y22\nLD M1067\nOUT Y24\nLD M1068\nOUT Y25\nLD M1082\nOUT Y26\nLD M1016\nOUT Y27\nLDI X7\nMOV D1067 D900\nMOV D1068 D901",
        observe,
        // Independent low-D clock readback. Its expected values are fixed from
        // the declared seed and fixture inputs, never from native shadow/output.
        "LDI X7\nTRD D600\nMOV K1234 D906\nEND",
    ].into_iter().filter(|s|!s.is_empty()).collect::<Vec<_>>().join("\n");
    assert!(source.lines().count() <= 512, "oversized {name}");
    assert!(source.lines().count() * 17 < 0x1ef0, "WORD budget {name}");
    assert!(
        body.lines().any(|s| s
            .split_whitespace()
            .next()
            .is_some_and(|op| FORMS.contains(&op))),
        "no RTC site {name}"
    );
    let mut c = RtcCorpusCase {
        comparison: case(&format!("indexed_rtc_{name}"), &source, stages()),
        initial: RtcInitialInputSpec {
            private_shadow: SEED.map(|v| v as u8),
            previous_sample_source: PreviousSampleSource::DeclaredEnvironmentalCaptureBeforeBody,
            initial_m1016: false,
            initial_m1017: false,
            boundary: ObservationBoundary::SettledPerNodeNoDeltaWindow,
        },
        admission: Admission::NativeCandidate,
        primary_error_offset: None,
        fixed_oracles: vec![],
    };
    add_bit(&mut c, "Y20", [true; 5]);
    add_bit(&mut c, "Y21", [false; 5]);
    add_bit(&mut c, "Y22", [true; 5]);
    add_word(&mut c, "D906", [1234; 5]);
    c
}
#[cfg(test)]
fn local_only(c: &mut RtcCorpusCase, why: &'static str) {
    c.admission = Admission::LocalHandlerOnly(why);
    c.initial.boundary = ObservationBoundary::LocalHandlerOnly;
}
fn unique(cs: &[RtcCorpusCase], n: usize) {
    assert_eq!(cs.len(), n);
    assert_eq!(
        cs.iter()
            .map(|c| &c.comparison.name)
            .collect::<BTreeSet<_>>()
            .len(),
        n
    );
    for c in cs {
        assert_eq!(c.comparison.stages.iter().map(|(_, n)| n).sum::<usize>(), 5);
    }
}

fn regular(
    name: &str,
    op: &str,
    area: &str,
    base: u16,
    operand: &str,
    modifier_seed: &str,
) -> RtcCorpusCase {
    let input = if is_read(op) { BLANK } else { WRITE };
    let seed = format!(
        "{modifier_seed}\n{}\nMOV K777 {}\nMOV K888 {}",
        block_seed(area, base, input),
        word(area, base - 1),
        word(area, base + 7)
    );
    let observe = format!(
        "LDI X7\n{}\nMOV {} D708\nMOV {} D709",
        observe_block(area, base, 700, 7),
        word(area, base - 1),
        word(area, base + 7)
    );
    let mut c = fixture(name, &seed, &format!("{op} {operand}"), &observe);
    add_words(
        &mut c,
        700,
        if is_read(op) {
            phase(BLANK, SEED)
        } else {
            [WRITE; 5]
        },
        7,
    );
    add_word(&mut c, "D708", [777; 5]);
    add_word(&mut c, "D709", [888; 5]);
    clock(
        &mut c,
        if is_read(op) {
            [SEED; 5]
        } else {
            phase(SEED, WRITE)
        },
    );
    add_bit(
        &mut c,
        "Y26",
        if is_read(op) {
            [false; 5]
        } else {
            [false, true, true, true, true]
        },
    );
    no_error(&mut c);
    c
}

pub(super) fn common_cases() -> Vec<RtcCorpusCase> {
    let mut cs = Vec::new();
    // All sixteen modifier aliases in both gray roles and every spelling.
    // Areas rotate here; separate ordinary cases cover each form x T/C/D.
    for (op_index, op) in FORMS.into_iter().enumerate() {
        for bank in 0..8u16 {
            for prefix in ["E", "F"] {
                let delta = if prefix == "E" { 1i16 } else { -1 };
                let suffix = format!("{prefix}{bank}");
                let area = ["T", "C", "D"]
                    [(usize::from(bank) + op_index + usize::from(prefix == "F")) % 3];
                let effective = (40i16 + delta) as u16;
                cs.push(regular(
                    &format!("bank_{op}_{suffix}_{area}"),
                    op,
                    area,
                    effective,
                    &format!("{area}40{suffix}"),
                    &format!("MOV K{delta} {suffix}"),
                ));
            }
        }
    }
    assert_eq!(cs.len(), 64);
    for op in FORMS {
        for area in ["T", "C", "D"] {
            cs.push(regular(
                &format!("ordinary_{op}_{area}"),
                op,
                area,
                40,
                &format!("{area}40"),
                "",
            ));
        }
    }
    assert_eq!(cs.len(), 76);
    for op in FORMS {
        let mut values = if is_read(op) { BLANK } else { WRITE };
        if !is_read(op) {
            values[0] = 2004;
        }
        let seed = format!("SET M1016\n{}", block_seed("D", 40, values));
        let mut c = fixture(
            &format!("four_digit_year_{op}"),
            &seed,
            &format!("{op} D40E0"),
            &format!("LDI X7\n{}", observe_block("D", 40, 700, 7)),
        );
        let mut seed_display = SEED;
        seed_display[0] = 2024;
        let mut written_display = WRITE;
        written_display[0] = 2004;
        add_words(
            &mut c,
            700,
            if is_read(op) {
                phase(BLANK, seed_display)
            } else {
                [values; 5]
            },
            7,
        );
        clock(
            &mut c,
            if is_read(op) {
                [seed_display; 5]
            } else {
                phase(seed_display, written_display)
            },
        );
        add_bit(&mut c, "Y27", [true; 5]);
        add_bit(
            &mut c,
            "Y26",
            if is_read(op) {
                [false; 5]
            } else {
                [false, true, true, true, true]
            },
        );
        no_error(&mut c);
        cs.push(c);
    }
    assert_eq!(cs.len(), 80);
    for op in ["TWR", "TWRP"] {
        for (tag, input, year) in [
            ("all_bad", [100, 0, 13, 0, 24, -1, 60], false),
            ("bad_year", [-1, 4, 2, 29, 12, 30, 7], false),
            ("bad_day_four_digit", [2004, 2, 12, 0, 15, 27, 30], true),
            ("late_second", [24, 4, 2, 29, 12, 30, 60], false),
        ] {
            let seed = format!(
                "{}\n{}",
                if year { "SET M1016" } else { "RST M1016" },
                block_seed("D", 40, input)
            );
            let mut c = fixture(
                &format!("field_error_{op}_{tag}"),
                &seed,
                &format!("{op} D40E0"),
                &format!("LDI X7\n{}", observe_block("D", 40, 700, 7)),
            );
            add_words(&mut c, 700, [input; 5], 7);
            let mut expected = SEED;
            if year {
                expected[0] = 2024;
            }
            clock(&mut c, [expected; 5]);
            add_bit(&mut c, "Y26", [false, true, true, true, true]);
            plc_error(&mut c, 3);
            cs.push(c);
        }
    }
    assert_eq!(cs.len(), 88);
    for op in ["TWR", "TWRP"] {
        for (tag, operand, extra) in [
            ("T_extent", "T250E0", ""),
            ("C_extent", "C194E0", ""),
            ("C200_effective", "C199E1", "MOV K1 E1"),
            ("C255_effective", "C199E1", "MOV K56 E1"),
        ] {
            let mut c = fixture(
                &format!("extent_{op}_{tag}"),
                extra,
                &format!("{op} {operand}"),
                "",
            );
            clock(&mut c, [SEED; 5]);
            add_bit(&mut c, "Y26", [false; 5]);
            plc_error(&mut c, 3);
            cs.push(c);
        }
    }
    assert_eq!(cs.len(), 96);
    for op in FORMS {
        let seed = block_seed("D", 40, if is_read(op) { BLANK } else { WRITE });
        let body = if is_read(op) {
            format!("LD X2\nMOV K777 D40\nLD X0\n{op} D40E0")
        } else {
            format!("LD X2\nMOV K16 D44\nLD X0\n{op} D40F0")
        };
        let mut c = fixture(
            &format!("held_change_rearm_{op}"),
            &seed,
            &body,
            &format!("LDI X7\n{}", observe_block("D", 40, 700, 7)),
        );
        c.comparison.stages = changing_stages();
        if is_read(op) {
            let mut overwritten = SEED;
            overwritten[0] = 777;
            add_words(
                &mut c,
                700,
                [
                    BLANK,
                    SEED,
                    if pulse(op) { overwritten } else { SEED },
                    overwritten,
                    SEED,
                ],
                7,
            );
            clock(&mut c, [SEED; 5]);
            add_bit(&mut c, "Y26", [false; 5]);
        } else {
            let mut changed = WRITE;
            changed[4] = 16;
            add_words(&mut c, 700, [WRITE, WRITE, changed, changed, changed], 7);
            clock(
                &mut c,
                [
                    SEED,
                    WRITE,
                    if pulse(op) { WRITE } else { changed },
                    if pulse(op) { WRITE } else { changed },
                    changed,
                ],
            );
            add_bit(&mut c, "Y26", [false, true, true, true, true]);
        }
        no_error(&mut c);
        cs.push(c);
    }
    assert_eq!(cs.len(), 100);
    for op in ["TRD", "TRDP"] {
        let seed = format!(
            "{}\nMOV K777 D1181\nMOV K888 D1189",
            block_seed("D", 1182, BLANK)
        );
        let body = format!("LDI X7\nMOV K0 E1\nLD X0\n{op} D1182E1");
        let mut c = fixture(
            &format!("self_index_cached_{op}"),
            &seed,
            &body,
            &format!(
                "LDI X7\n{}\nMOV D1181 D708\nMOV D1189 D709",
                observe_block("D", 1182, 700, 7)
            ),
        );
        let mut first = BLANK;
        first[0] = 0;
        let mut cleared = SEED;
        cleared[0] = 0;
        add_words(
            &mut c,
            700,
            [
                first,
                SEED,
                if pulse(op) { cleared } else { SEED },
                cleared,
                SEED,
            ],
            7,
        );
        add_word(&mut c, "D708", [777; 5]);
        add_word(&mut c, "D709", [888; 5]);
        clock(&mut c, [SEED; 5]);
        no_error(&mut c);
        cs.push(c);
    }
    for op in ["TWR", "TWRP"] {
        let input = [0, 7, 12, 31, 23, 59, 58];
        let mut c = fixture(
            &format!("source_is_index_alias_{op}"),
            &block_seed("D", 1182, input),
            &format!("{op} D1182E1"),
            &format!("LDI X7\n{}", observe_block("D", 1182, 700, 7)),
        );
        add_words(&mut c, 700, [input; 5], 7);
        clock(&mut c, phase(SEED, input));
        add_bit(&mut c, "Y26", [false, true, true, true, true]);
        no_error(&mut c);
        cs.push(c);
    }
    assert_eq!(cs.len(), 104);
    for op in FORMS {
        for mode in ["master", "jump", "for"] {
            let seed = block_seed("D", 40, if is_read(op) { BLANK } else { WRITE });
            let instruction = format!("{op} D40E0");
            let body = match mode {
                "master" => format!("LD X2\nMC N0\nLD X0\n{instruction}\nMCR N0"),
                "jump" => format!("LD X2\nCJ P10\nLD X0\n{instruction}\nP10"),
                _ => format!("LDI X7\nFOR K3\nLD X0\n{instruction}\nNEXT"),
            };
            let mut c = fixture(
                &format!("control_{op}_{mode}"),
                &seed,
                &body,
                &format!("LDI X7\n{}", observe_block("D", 40, 700, 7)),
            );
            if mode != "for" {
                c.comparison.stages = changing_stages();
            }
            let active = if mode == "master" {
                [false, false, true, false, true]
            } else if mode == "jump" {
                [false, true, false, false, false]
            } else {
                [false, true, true, false, true]
            };
            let mut done = false;
            let later = if is_read(op) { SEED } else { WRITE };
            let after = active.map(|power| {
                done |= power;
                if done {
                    later
                } else if is_read(op) {
                    BLANK
                } else {
                    WRITE
                }
            });
            add_words(&mut c, 700, after, 7);
            done = false;
            clock(
                &mut c,
                active.map(|power| {
                    done |= power;
                    if !is_read(op) && done { WRITE } else { SEED }
                }),
            );
            no_error(&mut c);
            cs.push(c);
        }
    }
    assert_eq!(cs.len(), 116);
    for op in FORMS {
        for (area, lexical, base, suffix) in [("T", 255, 249, "E7"), ("C", 199, 193, "F7")] {
            let input = if is_read(op) { BLANK } else { WRITE };
            let seed = format!(
                "MOV K-6 {suffix}\n{}\nMOV K777 {area}{}",
                block_seed(area, base, input),
                base - 1
            );
            let mut c = fixture(
                &format!("complete_{area}_negative_rescue_{op}"),
                &seed,
                &format!("{op} {area}{lexical}{suffix}"),
                &format!(
                    "LDI X7\n{}\nMOV {area}{} D708",
                    observe_block(area, base, 700, 7),
                    base - 1
                ),
            );
            add_words(
                &mut c,
                700,
                if is_read(op) {
                    phase(BLANK, SEED)
                } else {
                    [WRITE; 5]
                },
                7,
            );
            add_word(&mut c, "D708", [777; 5]);
            clock(
                &mut c,
                if is_read(op) {
                    [SEED; 5]
                } else {
                    phase(SEED, WRITE)
                },
            );
            add_bit(
                &mut c,
                "Y26",
                if is_read(op) {
                    [false; 5]
                } else {
                    [false, true, true, true, true]
                },
            );
            no_error(&mut c);
            cs.push(c);
        }
    }
    unique(&cs, 124);
    cs
}

pub(super) fn model_cases(target: Target) -> Vec<RtcCorpusCase> {
    let last = if target == Target::Ss2 { 4999 } else { 9999 };
    let complete = last - 6;
    let mut cs = Vec::new();
    for op in FORMS {
        for modified in [false, true] {
            let operand = if modified {
                format!("D{last}F7")
            } else {
                format!("D{complete}")
            };
            let init = if is_read(op) { BLANK } else { WRITE };
            let seed = format!(
                "MOV K-6 F7\n{}\nMOV K777 D{}",
                block_seed("D", complete, init),
                complete - 1
            );
            let mut c = fixture(
                &format!(
                    "model_{target:?}_{op}_complete_{}",
                    if modified {
                        "negative_rescue"
                    } else {
                        "ordinary"
                    }
                ),
                &seed,
                &format!("{op} {operand}"),
                &format!(
                    "LDI X7\n{}\nMOV D{} D708",
                    observe_block("D", complete, 700, 7),
                    complete - 1
                ),
            );
            add_words(
                &mut c,
                700,
                if is_read(op) {
                    phase(BLANK, SEED)
                } else {
                    [WRITE; 5]
                },
                7,
            );
            add_word(&mut c, "D708", [777; 5]);
            clock(
                &mut c,
                if is_read(op) {
                    [SEED; 5]
                } else {
                    phase(SEED, WRITE)
                },
            );
            no_error(&mut c);
            cs.push(c);
        }
    }
    for op in ["TWR", "TWRP"] {
        for modified in [false, true] {
            let operand = if modified {
                format!("D{last}E0")
            } else {
                format!("D{last}")
            };
            let mut c = fixture(
                &format!(
                    "model_{target:?}_{op}_extent_{}",
                    if modified { "indexed" } else { "ordinary" }
                ),
                "",
                &format!("{op} {operand}"),
                "",
            );
            clock(&mut c, [SEED; 5]);
            add_bit(&mut c, "Y26", [false; 5]);
            plc_error(&mut c, 3);
            cs.push(c);
        }
    }
    unique(&cs, 12);
    cs
}

#[cfg(test)]
pub(super) fn local_handler_cases() -> Vec<RtcCorpusCase> {
    let mut cs = Vec::new();
    let ascending_public = [7, 30, 12, 29, 2, 4, 24];
    let alias_result = [24, 4, 2, 29, 2, 4, 24];
    for op in ["TRD", "TRDP"] {
        let mut c = fixture(
            &format!("local_only_public_overlap_{op}"),
            "",
            &format!("{op} D1313E0"),
            &format!("LDI X7\n{}", observe_block("D", 1313, 700, 7)),
        );
        add_words(
            &mut c,
            700,
            [
                ascending_public,
                alias_result,
                if pulse(op) {
                    ascending_public
                } else {
                    alias_result
                },
                ascending_public,
                alias_result,
            ],
            7,
        );
        clock(&mut c, [SEED; 5]);
        local_only(
            &mut c,
            "TRD transient public overlap is observed before background refresh; settled native endpoint may overwrite it",
        );
        cs.push(c);
    }
    for op in ["TWR", "TWRP"] {
        // Intended input exists only in a local handler boundary. A stock engine
        // may overwrite each MOV to these public cells before the next IL node.
        let body = format!(
            "LD X0\n{}\nLD X0\n{op} D1313E0",
            block_seed("D", 1313, [24, 4, 2, 29, 12, 30, 0])
        );
        let mut c = fixture(
            &format!("local_only_public_source_{op}"),
            "",
            &body,
            &format!("LDI X7\n{}", observe_block("D", 1313, 700, 7)),
        );
        let injected = [24, 4, 2, 29, 12, 30, 0];
        add_words(
            &mut c,
            700,
            [
                ascending_public,
                alias_result,
                if pulse(op) { injected } else { alias_result },
                alias_result,
                alias_result,
            ],
            7,
        );
        clock(&mut c, phase(SEED, alias_result));
        local_only(
            &mut c,
            "TWR deliberately injected public-source words require local/controlled handler boundary; stock per-node publish invalidates injection",
        );
        cs.push(c);
    }
    for op in ["TWR", "TWRP"] {
        let seed = format!(
            "SET M1016\n{}",
            block_seed("D", 40, [2004, 2, 12, 0, 15, 27, 30])
        );
        let mut c = fixture(
            &format!("local_only_clamped_public_{op}"),
            &seed,
            &format!("{op} D40E0"),
            &format!("LDI X7\n{}", observe_block("D", 1313, 700, 7)),
        );
        let refreshed = [7, 30, 12, 29, 2, 4, 2024];
        let clamped = [30, 27, 15, 1, 12, 2, 4];
        add_words(
            &mut c,
            700,
            [
                ascending_public,
                clamped,
                if pulse(op) { refreshed } else { clamped },
                refreshed,
                clamped,
            ],
            7,
        );
        let mut display = SEED;
        display[0] = 2024;
        clock(&mut c, [display; 5]);
        plc_error(&mut c, 3);
        local_only(
            &mut c,
            "All-field clamp public image is transient; subsequent public refresh replaces it with unchanged private clock",
        );
        cs.push(c);
    }
    unique(&cs, 6);
    cs
}

#[cfg(test)]
pub(super) fn local_model_cases(target: Target) -> Vec<RtcCorpusCase> {
    let last = if target == Target::Ss2 { 4999 } else { 9999 };
    let mut cs = Vec::new();
    for op in ["TRD", "TRDP"] {
        for count in [1u16, 2] {
            let base = last + 1 - count;
            let seed = (0..count)
                .map(|n| format!("MOV K333 D{}", base + n))
                .collect::<Vec<_>>()
                .join("\n");
            let mut c = fixture(
                &format!("local_only_model_{target:?}_{op}_partial{count}"),
                &seed,
                &format!("{op} D{base}E0"),
                &format!("LDI X7\n{}", observe_block("D", base, 700, count)),
            );
            add_words(&mut c, 700, phase(BLANK, SEED), usize::from(count));
            clock(&mut c, [SEED; 5]);
            no_error(&mut c);
            local_only(
                &mut c,
                "Bounded TRD partial stores are locally exact; native internal decoder status/continuation is not main-suite qualified",
            );
            cs.push(c);
        }
    }
    unique(&cs, 4);
    cs
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendvp::{
        plc::{self, Area, Simulator},
        rtc_clock::{self, RtcServiceSchedule},
    };

    fn check_local(c: &RtcCorpusCase, target: Target) {
        assert_eq!(
            c.initial.previous_sample_source,
            PreviousSampleSource::DeclaredEnvironmentalCaptureBeforeBody
        );
        let p = plc::compile_il(&c.comparison.source)
            .unwrap_or_else(|e| panic!("{}: {e}", c.comparison.name));
        let encoded = opendvp::es2_encoder::encode(&p).unwrap();
        assert!(p.requires_vm());
        assert!(encoded.execution_plan.operand_tail_owners.is_empty());
        let primary = p
            .networks
            .iter()
            .flat_map(|n| &n.actions)
            .position(|a| match a {
                plc::Action::Pulse(inner) => matches!(inner.as_ref(), plc::Action::IndexedRtc(_)),
                plc::Action::IndexedRtc(_) => true,
                _ => false,
            })
            .unwrap();
        let expected_error_pc = c
            .primary_error_offset
            .map(|offset| encoded.action_pc[primary] + offset);
        let mut sim = Simulator::default();
        sim.control.profile = opendvp::program_control::ControlProfile::InstalledLegacy;
        sim.memory.sfc.profile = opendvp::sfc::SfcProfile::InstalledLegacy;
        sim.memory.data_register_limit = Some(if target == Target::Ss2 { 4999 } else { 9999 });
        sim.memory.set_system(1016, c.initial.initial_m1016);
        sim.memory.set_system(1017, c.initial.initial_m1017);
        match c.initial.boundary {
            ObservationBoundary::SettledPerNodeNoDeltaWindow => {
                assert_eq!(c.admission, Admission::NativeCandidate);
                sim.memory
                    .rtc_clock
                    .configure_installed_shadow(c.initial.private_shadow, 0)
                    .unwrap();
                sim.memory.rtc_service_schedule = RtcServiceSchedule::ExplicitBoundaries;
            }
            ObservationBoundary::LocalHandlerOnly => {
                assert!(matches!(c.admission, Admission::LocalHandlerOnly(_)));
                assert!(c.validate_fixed_oracles(1, &[]).is_err());
                sim.memory
                    .rtc_clock
                    .configure_frozen(c.initial.private_shadow);
            }
        }
        rtc_clock::refresh_public(&mut sim.memory).unwrap();
        let mut scan = 0;
        for (inputs, count) in &c.comparison.stages {
            for (input, value) in inputs {
                sim.memory.set_input(input.parse().unwrap(), *value);
            }
            for _ in 0..*count {
                scan += 1;
                if c.initial.boundary == ObservationBoundary::SettledPerNodeNoDeltaWindow {
                    rtc_clock::scan_with_installed_sample(&mut sim, &p, 0, 0).unwrap();
                } else {
                    sim.scan(&p, 0);
                }
                assert_eq!(
                    sim.last_scan,
                    plc::ScanStatus::Complete,
                    "{}",
                    c.comparison.name
                );
                assert!(
                    sim.memory.execution_limitations.is_empty(),
                    "{} {:?}",
                    c.comparison.name,
                    sim.memory.execution_limitations
                );
                for oracle in &c.fixed_oracles {
                    let device = oracle.device.parse::<Device>().unwrap();
                    let actual = match oracle.kind {
                        "word" => u32::from(sim.memory.word(device) as u16),
                        "bit" => u32::from(sim.memory.bit(device)),
                        other => panic!("Unexpected oracle kind {other}"),
                    };
                    assert_eq!(
                        actual,
                        oracle.raw_after_scans[scan - 1],
                        "{} scan{scan} {} {}",
                        c.comparison.name,
                        oracle.device,
                        oracle.kind
                    );
                    if oracle.kind == "word" {
                        assert_ne!(device.area, Area::M);
                    }
                }
                if let Some(pc) = expected_error_pc
                    && sim.memory.system_bit(1067)
                {
                    assert_eq!(
                        sim.memory.word("D1068".parse().unwrap()) as u16,
                        pc,
                        "{} independent error PC",
                        c.comparison.name
                    );
                }
            }
        }
        assert_eq!(scan, 5);
    }
    #[test]
    fn rtc_native_candidate_fixed_input_sources_and_all_literal_oracles() {
        for target in [Target::Es2, Target::Ss2] {
            let cases = common_cases()
                .into_iter()
                .chain(model_cases(target))
                .collect::<Vec<_>>();
            assert_eq!(cases.len(), 136);
            for c in &cases {
                check_local(c, target);
            }
        }
    }
    #[test]
    fn rtc_transient_handler_fixtures_stay_separate_from_settled_native_certificates() {
        for target in [Target::Es2, Target::Ss2] {
            let cases = local_handler_cases()
                .into_iter()
                .chain(local_model_cases(target))
                .collect::<Vec<_>>();
            assert_eq!(cases.len(), 10);
            for c in &cases {
                check_local(c, target);
            }
        }
    }
}
