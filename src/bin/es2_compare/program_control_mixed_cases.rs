//! Cross-command flow probes. Counts/spans are small and every backward jump
//! strictly decrements a D counter. No RTC, wall-clock delay or C200 binary ALU.
use super::{Case, case};
use opendvp::es2_bridge::Target;

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (
            vec![("X1", true), ("X0", false), ("X2", false), ("X3", true)],
            1,
        ),
        (vec![("X1", false), ("X0", true)], 2),
        (vec![("X2", true)], 1),
        (vec![("X0", false), ("X2", false)], 1),
        (vec![("X0", true)], 2),
    ]
}

fn fixture(name: &str, init: &str, body: &str) -> Case {
    let seed = if init.is_empty() {
        String::new()
    } else {
        format!("LD X1\n{init}\n")
    };
    case(
        &format!("program_control_mixed_{name}"),
        &format!("{seed}{body}"),
        stages(),
    )
}

pub fn cases(target: Target) -> Vec<Case> {
    let mut out = Vec::new();
    // Same real action site revisited three times, either directly or via CALL.
    // Table sizes are2, scratch counters do not alias flags or loop operands.
    let kernels = [
        ("word_alias", "MOV K7 D10", "ADD D10 K3 D10\nSUB D10 K1 D11"),
        (
            "wide_alias",
            "DMOV K-65536 D20",
            "DADD D20 K65536 D20\nDSUB D20 K1 D24",
        ),
        (
            "matrix_logic",
            "MOV H55AA D40\nMOV H0FF0 D41\nMOV H3333 D50\nMOV H00FF D51",
            "MAND D40 D50 D60 K2\nMXOR D60 D40 D60 K2",
        ),
        (
            "matrix_shift",
            "MOV H8001 D40\nMOV K1 D41\nRST M1097\nRST M1098\nRST M1099",
            "MBS D40 D40 K2\nMBR D40 D60 K2",
        ),
        (
            "drum_word",
            "MOV K-3 D100\nMOV K7 D101\nMOV K0 D110",
            "ABSD D100 D110 M200 K1\nINC D110",
        ),
        (
            "drum_wide",
            "DMOV K-65536 D100\nDMOV K65536 D102\nDMOV K0 D110",
            "DABSD D100 D110 M200 K1\nDADD D110 K32768 D110",
        ),
        (
            "drum_increment",
            "MOV K2 D100\nMOV K3 D101\nMOV K0 D600\nMOV K0 D601\nBMOV D600 C10 K2",
            "INCD D100 C10 M200 K2\nADD C10 K1 D600\nBMOV D600 C10 K1",
        ),
        (
            "float_finite",
            "DMOVR F1.5 D120",
            "DADDR D120 F0.25 D120\nDMULR D120 F2 D124",
        ),
    ];
    for (name, init, action) in kernels {
        out.push(fixture(
            &format!("FOR_{name}"),
            init,
            &format!("FOR K3\nLD X0\n{action}\nLD X3\nINC D800\nNEXT\nEND"),
        ));
        out.push(fixture(&format!("CALL_{name}"), init, &format!(
            "FOR K3\nLD X0\nCALL P10\nINC D801\nNEXT\nFEND\nP10\nLD X0\n{action}\nLD X3\nINC D800\nSRET\nEND")));
    }

    // Contacts alter enable state within a loop AND a subroutine, then OUT and
    // INC expose both false and true visits. All arithmetic is finite/bounded.
    let contacts = [
        (
            "word_bitwise",
            "MOV H8001 D10",
            "LD& D10 H8000\nAND^ D10 K1",
        ),
        (
            "wide_bitwise",
            "DMOV H80000001 D20",
            "DLD& D20 H80000000\nDAND| D20 K0",
        ),
        (
            "word_zone",
            "MOV K-7 D10\nMOV K3 D11",
            "LDZ= D10 D11 K10\nANDZ<= D10 D11 K10",
        ),
        (
            "wide_zone",
            "DMOV K-65536 D20\nDMOV K65536 D24",
            "DLDZ= D20 D24 K131072\nDANDZ> D20 D24 K1",
        ),
        (
            "indexed_bit",
            "MOV H8001 D10\nMOV K15 D11\nDMOV H80000001 D20",
            "BLD D10 D11\nDBAND D20 K31",
        ),
        (
            "float_compare",
            "DMOVR F-1.5 D120\nDMOVR F2.25 D124",
            "FLD< D120 D124\nFAND>= D124 F2.25",
        ),
    ];
    for (name, init, contact) in contacts {
        out.push(fixture(&format!("contacts_{name}"), init, &format!(
            "FOR K2\nLD X0\nCALL P20\nNEXT\nFEND\nP20\n{contact}\nAND X0\nMPS\nOUT Y0\nINC D800\nMPP\nOUT Y1\nSRET\nEND")));
    }

    // Pulse instructions share one source site over loop visits. A false visit
    // rearms it; an unvisited site retains its edge memory across a skipped scan.
    for (name, init, action) in [
        ("word", "MOV K0 D10", "ADDP D10 K1 D10"),
        ("wide", "DMOV K0 D20", "DADDP D20 K65536 D20"),
        ("matrix", "MOV H55AA D40\nMOV H00FF D41", "MINVP D40 D40 K2"),
        ("float", "DMOVR F1.5 D120", "DADDRP D120 F0.25 D120"),
    ] {
        let mut probe = fixture(
            &format!("pulse_{name}_skipped_false_rearm"),
            init,
            &format!("LD X2\nCJ P30\nFOR K3\nLD X0\n{action}\nNEXT\nP30\nLD X3\nINC D800\nEND"),
        );
        // Return from the skipped scan with power still true BEFORE the false
        // visit, distinguishing retained pulse state from rearming on a skip.
        probe.stages.insert(3, (vec![("X2", false)], 1));
        out.push(probe);
    }

    for (name, edge) in [("rising", "LDP"), ("falling", "LDF")] {
        out.push(fixture(&format!("edge_{name}_per_visit_CALL"), "", &format!(
            "FOR K3\nLD X3\nCALL P40\nNEXT\nFEND\nP40\n{edge} X0\nOUT Y0\nINC D800\nLD X2\nNP\nOUT Y1\nLD X0\nPN\nOUT Y2\nSRET\nEND")));
        out.push(fixture(&format!("edge_{name}_alternating_within_scan"), "RST M64", &format!(
            "LD X3\nMOV K0 D700\nFOR K4\nLD= D700 K0\nOR= D700 K2\nAND X0\nOUT M64\n{edge} M64\nOUT Y0\nINC D800\nLD X3\nINC D700\nNEXT\nEND")));
    }

    // MC is lexically outside FOR: no forbidden MC nesting inside a loop.
    // MPS/MRD/MPP remain balanced, including when the master gate is false.
    out.push(fixture("MC_false_timer_counter_MPS", "SET Y0\nSET M50", 
        "LD X0\nMC N0\nLD X3\nMPS\nOUT Y0\nTMR T0 K3\nMRD\nCNT C0 K5\nMPP\nOUT Y1\nMCR N0\nLD X3\nINC D800\nEND"));
    out.push(fixture("MC_FOR_CALL_timer_counter_MPS", "MOV K1 D10",
        "LD X0\nMC N0\nFOR K2\nLD X3\nMPS\nTMR T1 K4\nMRD\nCNT C1 K3\nMPP\nCALL P50\nNEXT\nMCR N0\nFEND\nP50\nLD X3\nMPS\nADD D10 K1 D10\nMPP\nOUT Y2\nSRET\nEND"));

    // Counter initialization executes each scan before the backward label;
    // neither branch path can evade decrement or reload the counter.
    for (name, tail) in [
        ("word", "ADD D10 K2 D10"),
        ("wide", "DADD D20 K65536 D20"),
        ("float", "DADDR D120 F0.25 D120"),
    ] {
        out.push(fixture(&format!("CJ_backward_terminated_{name}"),
            "MOV K0 D10\nDMOV K0 D20\nDMOVR F1.5 D120", &format!(
            "LD X3\nMOV K3 D700\nP60\nLD X0\n{tail}\nLD X3\nDEC D700\nLD> D700 K0\nCJ P60\nLD X3\nINC D800\nEND")));
    }

    for (name, count, init, mutate) in [
        ("D", "D30", "MOV K3 D30", "MOV K1 D30"),
        ("E1", "E1", "MOV K3 D1182", "MOV K1 D1182"),
        (
            "packed",
            "K1M64",
            "SET M64\nSET M65\nRST M66\nRST M67",
            "RST M65",
        ),
    ] {
        out.push(fixture(&format!("FOR_captured_{name}_mutated_by_CALL"), init, &format!(
            "FOR {count}\nLD X3\nCALL P70\nINC D800\nNEXT\nFEND\nP70\nLD X0\n{mutate}\nLD X3\nINC D801\nSRET\nEND")));
    }

    // C200 is deliberately used ONLY as a scalar FOR count: the installed
    // reader reportsE1A and returns0, causing the safe one-iteration clamp.
    // No binary arithmetic or timer/counter data mutation touches C200 here.
    out.push(fixture(
        "FOR_C200_scalar_error_one_iteration",
        "",
        "FOR C200\nLD X3\nINC D800\nNEXT\nLD X0\nADD D800 K1 D801\nEND",
    ));

    let last = if target == Target::Ss2 { 4999 } else { 9999 };
    out.push(fixture(
        "FOR_source_model_end_T_C_guarded",
        &format!("MOV K2 D{last}\nMOV K2 D600\nBMOV D600 T255 K1\nBMOV D600 C199 K1"),
        &format!(
            "FOR D{last}\nLD X0\nADD T255 C199 D10\nLD X3\nINC D800\nNEXT\nLD X3\nINC D801\nEND"
        ),
    ));
    out
}
