//! 2026-10-28: older bulk-kernel scratch effects discovered by matrix fixtures.
//! These execute genuine bulk predecessors and word SORT; no matrix command
//! is needed to expose the previously missing shared scratch byte writes.
//! Expected results, including dynamic BMOV n=0 error state, come from target.
use super::{Case, case};
use opendvp::es2_bridge::Target;

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        // Set ordinary PLC words only, then independently seed scratch with SER.
        (
            vec![
                ("X1", true),
                ("X4", true),
                ("X2", false),
                ("X0", false),
                ("X3", true),
            ],
            1,
        ),
        // Predecessor disabled: establish its held-scratch SORT baseline.
        (vec![("X1", false), ("X4", false), ("X0", true)], 1),
        // Re-seed before each predecessor evaluation. On held P scans, SER's
        // seed survives because the bulk instruction no longer qualifies.
        (vec![("X4", true), ("X2", true)], 3),
        (vec![("X2", false)], 1),
        // Drop SORT power to release its owner, then rearm both operations.
        (vec![("X0", false), ("X4", false)], 1),
        (vec![("X0", true), ("X2", true), ("X4", true)], 3),
        (vec![("X2", false)], 1),
    ]
}

fn fixture(
    name: &str,
    bulk: &str,
    seed: i16,
    source_area: char,
    dest_area: char,
    wide: bool,
    zero: bool,
) -> Case {
    let mut source = String::from("LD X1\n");
    // T/C initialization needs a block transfer because MOV destinations are
    // D-only here. X4 seeds scratch with SER after all initialization, before
    // the first disabled/active predecessor observation.
    for (offset, value) in [30, 10, 20, 3, 1, 2].into_iter().enumerate() {
        source.push_str(&format!(
            "MOV K{value} D{offset}\nMOV K-777 D{}\n",
            100 + offset
        ));
    }
    let source_base = if source_area == 'D' { 200 } else { 20 };
    let dest_base = if dest_area == 'D' { 300 } else { 100 };
    for (offset, value) in [0x1234u16, 0x5678, 0x9abc, 0xdef0].into_iter().enumerate() {
        let d = if source_area == 'D' {
            source_base + offset
        } else {
            820 + offset
        };
        source.push_str(&format!("MOV H{value:04X} D{d}\n"));
    }
    if source_area != 'D' {
        source.push_str(&format!("BMOV D820 {source_area}{source_base} K4\n"));
    }
    for offset in 0..if wide { 4 } else { 2 } {
        let d = if dest_area == 'D' {
            dest_base + offset
        } else {
            830 + offset
        };
        source.push_str(&format!("MOV K-7 D{d}\n"));
    }
    if dest_area != 'D' {
        source.push_str(&format!(
            "BMOV D830 {dest_area}{dest_base} K{}\n",
            if wide { 4 } else { 2 }
        ));
    }
    source.push_str(&format!("MOV K{} D799\n", if zero { 0 } else { 2 }));
    source.push_str("MOV K7 D750\nSET M1020\nSET M1021\nSET M1022\nRST M1029\nRST M1067\nRST M1068\nMOV K0 D1067\nMOV K0 D1068\n");
    source.push_str(&format!("LD X4\nSER D750 K{seed} D760 K1\nLD X2\n{bulk}\nMOV K321 D800\nLD X0\nSORT D0 K3 K2 D100 K2\nMOV K123 D801\nLD X3\n"));
    for offset in 0..if wide { 4 } else { 2 } {
        source.push_str(&format!(
            "MOV {dest_area}{} D{}\n",
            dest_base + offset,
            810 + offset
        ));
    }
    source.push_str("LD M1020\nOUT M120\nLD M1021\nOUT M121\nLD M1022\nOUT M122\nLD M1029\nOUT M123\nLD M1067\nOUT M124\nLD M1068\nOUT M125\nEND");
    case(&format!("matrix_bulk_{name}"), &source, stages())
}

pub(super) fn cases(_target: Target) -> Vec<Case> {
    let mut cases = Vec::new();
    for pulse in [false, true] {
        let suffix = if pulse { "P" } else { "" };
        // 18 BMOV cases establish independent HIGH(source)/LOW(destination)
        // effects across every raw T/C/D pairing and ordinary/P qualification.
        for source_area in ['T', 'C', 'D'] {
            for dest_area in ['T', 'C', 'D'] {
                let source_base = if source_area == 'D' { 200 } else { 20 };
                let dest_base = if dest_area == 'D' { 300 } else { 100 };
                cases.push(fixture(
                    &format!("BMOV{suffix}_{source_area}_{dest_area}"),
                    &format!("BMOV{suffix} {source_area}{source_base} {dest_area}{dest_base} K2"),
                    3,
                    source_area,
                    dest_area,
                    false,
                    false,
                ));
            }
        }
        // FMOV changes LOW only: SER0 prevents selection until FMOV installs
        // positive tag82/84/86. DFMOV changes HIGH only: SER3 initially permits
        // selection, then a raw destination tag makes the shared word negative.
        for dest_area in ['T', 'C', 'D'] {
            let dest_base = if dest_area == 'D' { 300 } else { 100 };
            cases.push(fixture(
                &format!("FMOV{suffix}_{dest_area}"),
                &format!("FMOV{suffix} K-123 {dest_area}{dest_base} K2"),
                0,
                'D',
                dest_area,
                false,
                false,
            ));
            cases.push(fixture(
                &format!("DFMOV{suffix}_{dest_area}"),
                &format!("DFMOV{suffix} H89ABCDEF {dest_area}{dest_base} K2"),
                3,
                'D',
                dest_area,
                true,
                false,
            ));
        }
        // The native byte stores precede BMOV's count validity check. Safe n0
        // performs no element loop; neither native error absence nor presence
        // is hardcoded here. No grouped or overflowing count is executed.
        cases.push(fixture(
            &format!("BMOV{suffix}_dynamic_zero_count_scratch"),
            &format!("BMOV{suffix} D200 C100 D799"),
            3,
            'D',
            'C',
            false,
            true,
        ));
    }
    cases
}
