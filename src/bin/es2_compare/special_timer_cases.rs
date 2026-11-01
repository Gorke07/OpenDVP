//! Bounded timer/flow differential fixtures. Default comparison uses fixed100ms.
//! TTMR has no elapsed ticks in that profile; the SAME sources can be replayed
//! with explicitly injected --teaching-ticks inputs, never copied result values.
use super::{Case, case};
use opendvp::es2_bridge::Target;

fn stages() -> Vec<(Vec<(&'static str, bool)>, usize)> {
    vec![
        (
            vec![("X0", false), ("X1", true), ("X2", false), ("X3", true)],
            1,
        ),
        (vec![("X0", true), ("X1", false)], 1),
        (vec![], 3),
        (vec![("X2", true)], 1),
        (vec![("X0", false), ("X2", false)], 2),
        (vec![("X0", true)], 3),
    ]
}
fn fixture(name: &str, init: &str, body: &str) -> Case {
    let init = if init.is_empty() {
        String::new()
    } else {
        format!("LD X1\n{init}\n")
    };
    case(
        &format!("special_timers_{name}"),
        &format!("{init}{body}"),
        stages(),
    )
}
fn flags(fast100: bool, fast10: bool) -> String {
    format!(
        "{} M1028\n{} M1038",
        if fast100 { "SET" } else { "RST" },
        if fast10 { "SET" } else { "RST" }
    )
}
fn observe(timer: u16) -> String {
    format!("LD T{timer}\nOUT Y0\nLDI T{timer}\nOUT Y1\nLD X3\nMOV T{timer} D800\nINC D900\nEND")
}
fn timer_case(op: &str, timer: u16, preset: &str, fast100: bool, fast10: bool) -> Case {
    fixture(
        &format!(
            "{op}_T{timer}_{preset}_flags{}{}",
            u8::from(fast100),
            u8::from(fast10)
        ),
        &format!("MOV K3 D50\n{}", flags(fast100, fast10)),
        &format!(
            "LD X2\nMOV K1 D50\nLD X0\n{op} T{timer} {preset}\nOUT Y2\n{}",
            observe(timer)
        ),
    )
}
fn bit_span_seed(raw: &str) -> String {
    let area = raw.as_bytes()[0] as char;
    let n = if area == 'Y' {
        usize::from_str_radix(&raw[1..], 8).unwrap()
    } else {
        raw[1..].parse().unwrap()
    };
    (n..n + 4)
        .map(|index| {
            if area == 'Y' {
                format!("SET Y{index:o}\n")
            } else {
                format!("SET {area}{index}\n")
            }
        })
        .collect()
}
fn teaching_seed(dest: usize) -> String {
    format!("MOV K77 D{dest}\nMOV K99 D{}", dest + 1)
}

pub fn cases(target: Target) -> Vec<Case> {
    let mut out = Vec::new();
    let endpoints = [
        0, 63, 64, 126, 127, 128, 183, 184, 191, 192, 199, 200, 239, 240, 245, 246, 249, 250, 255,
    ];
    // Every legal timer index, plus full boundary/preset/control combinations.
    for timer in 0..=255 {
        out.push(timer_case("TMR", timer, "K3", false, false));
    }
    for timer in endpoints {
        for fast100 in [false, true] {
            for fast10 in [false, true] {
                for preset in ["K0", "K1", "K3", "D50"] {
                    if preset == "K3" && !fast100 && !fast10 {
                        continue;
                    }
                    out.push(timer_case("TMR", timer, preset, fast100, fast10));
                }
            }
        }
    }
    // Skip is not a false visit. Contact/current are read after the branch.
    for timer in [
        0, 64, 127, 128, 183, 184, 191, 192, 199, 200, 239, 240, 245, 246, 249, 250, 255,
    ] {
        let mut probe = fixture(
            &format!("TMR_T{timer}_CJ_skipped_then_false"),
            &flags(false, false),
            &format!(
                "LD X2\nCJ P10\nLD X0\nTMR T{timer} K3\nP10\n{}",
                observe(timer)
            ),
        );
        probe.stages = vec![
            (
                vec![("X0", false), ("X1", true), ("X2", false), ("X3", true)],
                1,
            ),
            (vec![("X0", true), ("X1", false)], 1),
            (vec![("X2", true), ("X0", false)], 4),
            (vec![("X2", false)], 1),
            (vec![("X0", true)], 4),
        ];
        out.push(probe);
        out.push(fixture(
            &format!("TMR_T{timer}_MC_false"),
            &flags(false, false),
            &format!(
                "LD X0\nMC N0\nLD X3\nTMR T{timer} K3\nOUT Y2\nMCR N0\n{}",
                observe(timer)
            ),
        ));
    }
    // Main FEND services enabled subroutine timers even on an uncalled scan.
    for timer in [
        183, 184, 185, 186, 187, 188, 189, 190, 191, 192, 193, 194, 195, 196, 197, 198, 199, 200,
    ] {
        let mut probe = fixture(
            &format!("TMR_T{timer}_CALL_FEND_uncalled_continuation"),
            &flags(false, false),
            &format!(
                "LD X0\nCALL P20\nLD T{timer}\nOUT Y0\nLD X3\nMOV T{timer} D800\nINC D900\nFEND\nP20\nLD X3\nTMR T{timer} K3\nSRET\nEND"
            ),
        );
        probe.stages = vec![
            (vec![("X0", false), ("X1", true), ("X3", true)], 1),
            (vec![("X0", true), ("X1", false)], 1),
            (vec![("X0", false)], 4),
            (vec![("X0", true)], 2),
            (vec![("X0", false)], 1),
        ];
        out.push(probe);
    }
    for timer in [0, 127, 184, 240, 255] {
        out.push(fixture(
            &format!("TMR_T{timer}_same_site_FOR_revisit"),
            &flags(false, false),
            &format!(
                "FOR K3\nLD X0\nTMR T{timer} K3\nMOV T{timer} D810\nNEXT\n{}",
                observe(timer)
            ),
        ));
        out.push(fixture(
            &format!("TMR_T{timer}_duplicate_site_last_false"),
            &flags(false, false),
            &format!(
                "LD X0\nTMR T{timer} K3\nLD X2\nTMR T{timer} K1\n{}",
                observe(timer)
            ),
        ));
    }
    for (timer, flag) in [(64, 1028), (200, 1038)] {
        out.push(fixture(
            &format!("TMR_T{timer}_live_resolution_toggle"),
            &flags(false, false),
            &format!(
                "LD X2\nOUT M{flag}\nLD X0\nTMR T{timer} K32767\n{}",
                observe(timer)
            ),
        ));
    }
    for after in [false, true] {
        let action = if after {
            "TMR T240 K3\nLD X2\nRST T240"
        } else {
            "RST T240\nLD X0\nTMR T240 K3"
        };
        out.push(fixture(
            &format!("TMR_RST_accumulative_order_after{after}"),
            &flags(false, false),
            &format!("LD X0\n{action}\n{}", observe(240)),
        ));
    }

    // ATMR is a logic gate; OUT and MOV remain in the SAME network.
    for timer in endpoints {
        for preset in ["K0", "K1", "K3", "D50"] {
            out.push(timer_case("ATMR", timer, preset, false, false));
        }
    }
    for (timer, fast100, fast10) in [
        (64, true, false),
        (126, true, true),
        (200, false, true),
        (245, true, true),
    ] {
        out.push(timer_case("ATMR", timer, "H3", fast100, fast10));
    }
    out.push(fixture(
        "ATMR_following_MOV_OUT",
        "MOV K-1 D20",
        "LD X0\nATMR T0 K3\nMOV K123 D20\nOUT Y0\nLD X3\nMOV T0 D800\nEND",
    ));
    out.push(fixture(
        "ATMR_following_CJ",
        "MOV K0 D20",
        "LD X0\nATMR T0 K3\nCJ P30\nLD X3\nINC D20\nP30\nLD X3\nINC D21\nEND",
    ));
    out.push(fixture(
        "ATMR_MPS_interleaving",
        "",
        "LD X0\nMPS\nATMR T0 K3\nOUT Y0\nMRD\nMOV K11 D20\nMPP\nOUT Y1\nLD X3\nMOV T0 D800\nEND",
    ));
    out.push(fixture("ATMR_MC_false_following_logic", "",
        "LD X0\nMC N0\nLD X3\nATMR T240 K3\nMOV K123 D20\nOUT Y0\nMCR N0\nLD X3\nMOV T240 D800\nEND"));
    out.push(fixture(
        "ATMR_FOR_revisited_gate",
        "",
        "FOR K3\nLD X0\nATMR T184 K3\nINC D20\nOUT Y0\nNEXT\nLD X3\nMOV T184 D800\nEND",
    ));
    out.push(fixture(
        "ATMR_suppresses_following_FOR_body",
        "",
        "LD X0\nATMR T0 K3\nFOR K2\nINC D20\nNEXT\nLD X3\nMOV T0 D800\nEND",
    ));
    out.push(fixture(
        "ATMR_following_MPS_captures_gated_logic",
        "",
        "LD X0\nATMR T0 K3\nMPS\nOUT Y0\nMRD\nMOV K123 D20\nMPP\nOUT Y1\nLD X3\nMOV T0 D800\nEND",
    ));
    out.push(fixture(
        "ATMR_following_MC_uses_gated_logic",
        "",
        "LD X0\nATMR T0 K3\nMC N0\nLD X3\nOUT Y0\nINC D20\nMCR N0\nLD X3\nMOV T0 D800\nEND",
    ));

    // Four-bit spans include byte/octal rollover, high M and the final S/Y bits.
    // Six named boundary timers plus T64 for its switchable resolution.
    for timer in [0, 7, 8, 63, 64, 127, 183] {
        for dest in ["Y7", "Y374", "M1535", "S1020"] {
            for preset in ["K1", "H3"] {
                let mut probe = fixture(
                    &format!("STMR_T{timer}_{preset}_{dest}_four_phases"),
                    &(bit_span_seed(dest) + &flags(false, false)),
                    &format!(
                        "LD X0\nSTMR T{timer} {preset} {dest}\nLD X3\nMOV T{timer} D800\nINC D900\nEND"
                    ),
                );
                probe.stages = vec![
                    (
                        vec![("X0", false), ("X1", true), ("X2", false), ("X3", true)],
                        2,
                    ),
                    (vec![("X0", true), ("X1", false)], 5),
                    (vec![("X0", false)], 5),
                    (vec![("X0", true)], 2),
                    (vec![("X0", false)], 4),
                ];
                out.push(probe);
            }
        }
    }
    for timer in [64, 126] {
        out.push(fixture(
            &format!("STMR_T{timer}_fast_M1028"),
            &flags(true, false),
            &format!("LD X0\nSTMR T{timer} K3 M200\nLD X3\nMOV T{timer} D800\nEND"),
        ));
    }
    out.push(fixture(
        "STMR_four_output_feedback",
        "",
        "LD X0\nANI Y3\nSTMR T10 K1 Y0\nLD X3\nMOV T10 D800\nEND",
    ));
    out.push(fixture(
        "STMR_output_alias_input",
        "SET M200",
        "LD X0\nOR M200\nSTMR T7 K1 M200\nLD X3\nMOV T7 D800\nEND",
    ));
    out.push(fixture(
        "STMR_MC_false_continues_off_phase",
        "",
        "LD X0\nMC N0\nLD X3\nSTMR T8 K1 M200\nMCR N0\nLD X3\nMOV T8 D800\nEND",
    ));
    out.push(fixture(
        "STMR_CJ_skip_then_false",
        "",
        "LD X2\nCJ P40\nLD X0\nSTMR T63 K1 M200\nP40\nLD X3\nMOV T63 D800\nEND",
    ));

    // TTMR: zero ticks under default fixed-scan; explicit boundary tick input
    // may be supplied by the comparator in an independently labelled replay.
    let last_pair = if target == Target::Ss2 { 4998 } else { 9998 };
    for dest in [100, last_pair] {
        for scale in [0, 1, 2] {
            out.push(fixture(
                &format!("TTMR_D{dest}_scale{scale}_clock_input"),
                &teaching_seed(dest),
                &format!("LD X0\nTTMR D{dest} K{scale}\nLD X3\nINC D900\nEND"),
            ));
        }
    }
    for scale in [0, 1, 2] {
        let mut probe = fixture(
            &format!("TTMR_scale{scale}_wrap_130_scans"),
            &teaching_seed(100),
            &format!("LD X0\nTTMR D100 K{scale}\nLD X3\nINC D900\nEND"),
        );
        probe.stages = vec![
            (vec![("X0", false), ("X1", true), ("X3", true)], 1),
            (vec![("X0", true), ("X1", false)], 130),
            (vec![("X0", false)], 1),
            (vec![("X0", true)], 2),
        ];
        out.push(probe);
    }
    let mut seed = String::new();
    let mut body = String::new();
    for i in 0..9 {
        seed.push_str(&teaching_seed(100 + 2 * i));
        seed.push('\n');
        let gate = if i == 0 {
            "LD X0\nAND X3"
        } else if i == 8 {
            "LD X2"
        } else {
            "LD X0"
        };
        body.push_str(&format!("{gate}\nTTMR D{} K{}\n", 100 + 2 * i, i % 3));
    }
    body.push_str("LD X0\nINC D900\nEND");
    let mut owners = fixture("TTMR_installed_eight_ninth_free_reuse", &seed, &body);
    owners.stages = vec![
        (
            vec![("X0", false), ("X1", true), ("X2", false), ("X3", true)],
            1,
        ),
        (vec![("X0", true), ("X1", false)], 2),
        (vec![("X2", true)], 2),
        (vec![("X3", false)], 2),
        (vec![("X2", false)], 1),
        (vec![("X3", true)], 2),
        (vec![("X0", false)], 1),
        (vec![("X0", true), ("X2", true)], 2),
    ];
    out.push(owners);
    for dest in [100, 101] {
        out.push(fixture(
            &format!("TTMR_live_pair_alias_D{dest}"),
            "MOV K77 D100\nMOV K88 D101\nMOV K99 D102",
            &format!("LD X0\nTTMR D100 K1\nLD X0\nTTMR D{dest} K2\nLD X3\nINC D900\nEND"),
        ));
    }
    out.push(fixture(
        "TTMR_same_PC_FOR_revisit",
        &teaching_seed(100),
        "FOR K3\nLD X0\nTTMR D100 K1\nNEXT\nLD X3\nINC D900\nEND",
    ));
    out.push(fixture(
        "TTMR_same_PC_CALL_revisit",
        &teaching_seed(100),
        "FOR K3\nLD X0\nCALL P50\nNEXT\nFEND\nP50\nLD X3\nTTMR D100 K2\nSRET\nEND",
    ));
    out.push(fixture(
        "TTMR_MC_false_releases_owner",
        &teaching_seed(100),
        "LD X0\nMC N0\nLD X3\nTTMR D100 K1\nMCR N0\nLD X3\nINC D900\nEND",
    ));
    out.push(fixture(
        "TTMR_CJ_skips_without_releasing_owner",
        &teaching_seed(100),
        "LD X2\nCJ P60\nLD X0\nTTMR D100 K1\nP60\nLD X3\nINC D900\nEND",
    ));
    out.push(fixture(
        "TTMR_first_owner_pending_reset_staggered",
        &teaching_seed(100),
        "LD X0\nTTMR D100 K0\nLD X2\nTTMR D110 K2\nLD X3\nINC D900\nEND",
    ));
    out.extend(supplemental_cases());
    out
}

// Appended independently: the original774 source/stage fixtures remain intact.
fn supplemental_cases() -> Vec<Case> {
    let mut out = Vec::new();
    for timer in [0, 8] {
        for preset in ["K1", "K3"] {
            let mut probe = fixture(
                &format!("STMR_physical_M1535_C_contacts_T{timer}_{preset}"),
                "CNT C0 K1\nCNT C1 K1\nCNT C2 K1\nDIV16 K11 K1 C0\nDIV16 K22 K1 C1\nDIV16 K33 K1 C2\nSET M1535\nSET M1536\nSET M1537\nSET M1538",
                &format!(
                    "LD X0\nSTMR T{timer} {preset} M1535\nLD C0\nOUT Y20\nLD C1\nOUT Y21\nLD C2\nOUT Y22\nLD M1535\nOUT Y23\nLD M1536\nOUT Y24\nLD M1537\nOUT Y25\nLD M1538\nOUT Y26\nLD X3\nMOV C0 D700\nMOV C1 D701\nMOV C2 D702\nMOV T{timer} D800\nEND"
                ),
            );
            probe.stages = vec![
                (vec![("X0", false), ("X1", true), ("X3", true)], 1),
                (vec![("X0", true), ("X1", false)], 5),
                (vec![("X0", false)], 5),
                (vec![("X0", true)], 2),
            ];
            out.push(probe);
        }
    }

    // Native STMR's raw mask-table access may inhibit OTHER timers in its
    // physical byte. Every neighbour is independently driven and observed.
    for timer in [7u16, 8, 63, 127, 183] {
        let base = timer / 8 * 8;
        let mut body = String::from("LD X3\n");
        for neighbour in base..base + 8 {
            if neighbour != timer {
                body.push_str(&format!("TMR T{neighbour} K32767\n"));
            }
        }
        body.push_str(&format!("LD X0\nSTMR T{timer} K1 M200\nLD X3\n"));
        for neighbour in base..base + 8 {
            body.push_str(&format!("MOV T{neighbour} D{}\n", 700 + neighbour - base));
        }
        body.push_str("END");
        let mut probe = fixture(
            &format!("STMR_T{timer}_shared_hold_neighbour_byte"),
            &flags(false, false),
            &body,
        );
        probe.stages = vec![
            (vec![("X0", false), ("X1", true), ("X3", true)], 1),
            (vec![("X0", true), ("X1", false)], 6),
            (vec![("X0", false)], 5),
            (vec![("X0", true)], 3),
        ];
        out.push(probe);
    }

    // SORT continuation exposes shared numeric scratch without reading private
    // state. Timer powerX2 and SORT powerX0 are independent, including false
    // cleanup while SORT continues. First/held/reactivated visits are retained.
    let table = "MOV K30 D400\nMOV K10 D401\nMOV K20 D402\nMOV K3 D403\nMOV K1 D404\nMOV K2 D405\nMOV K-777 D500\nMOV K-777 D501\nMOV K-777 D502\nMOV K-777 D503\nMOV K-777 D504\nMOV K-777 D505\nRST M1029";
    for (name, init, predecessor) in [
        ("TTMR_D0", teaching_seed(0), "TTMR D0 K1".to_owned()),
        ("TTMR_D1", teaching_seed(1), "TTMR D1 K1".to_owned()),
        ("TTMR_D3", teaching_seed(3), "TTMR D3 K1".to_owned()),
        ("TTMR_D200", teaching_seed(200), "TTMR D200 K1".to_owned()),
        ("STMR_T0", String::new(), "STMR T0 K1 M200".to_owned()),
        ("STMR_T1", String::new(), "STMR T1 K1 M200".to_owned()),
        ("STMR_T3", String::new(), "STMR T3 K1 M200".to_owned()),
        ("STMR_T8", String::new(), "STMR T8 K1 M200".to_owned()),
        ("STMR_T63", String::new(), "STMR T63 K1 M200".to_owned()),
        ("STMR_T183", String::new(), "STMR T183 K1 M200".to_owned()),
    ] {
        let mut probe = fixture(
            &format!("scratch_{name}_SORT_continuation"),
            &format!("{table}\n{init}"),
            &format!(
                "LD X2\n{predecessor}\nLD X0\nSORT D400 K3 K2 D500 K2\nOUT Y0\nLD X3\nMOV K123 D900\nEND"
            ),
        );
        probe.stages = vec![
            (
                vec![("X0", false), ("X1", true), ("X2", false), ("X3", true)],
                1,
            ),
            (vec![("X0", true), ("X1", false), ("X2", true)], 6),
            (vec![("X2", false)], 3),
            (vec![("X0", false)], 1),
            (vec![("X0", true)], 2),
            (vec![("X2", true)], 4),
        ];
        out.push(probe);
    }

    for reset in ["RST T8", "ZRST T8 T15"] {
        for after in [false, true] {
            let first = if after {
                String::new()
            } else {
                format!("LD X2\n{reset}\n")
            };
            let last = if after {
                format!("LD X2\n{reset}\n")
            } else {
                String::new()
            };
            let mut probe = fixture(
                &format!("STMR_held_{}_after{after}", reset.replace(' ', "_")),
                &flags(false, false),
                &format!(
                    "LD X3\nTMR T9 K32767\n{first}LD X0\nSTMR T8 K1 M200\n{last}LD X3\nMOV T8 D800\nMOV T9 D801\nEND"
                ),
            );
            probe.stages = vec![
                (
                    vec![("X0", false), ("X1", true), ("X2", false), ("X3", true)],
                    1,
                ),
                (vec![("X0", true), ("X1", false)], 4),
                (vec![("X2", true)], 1),
                (vec![("X2", false)], 3),
                (vec![("X0", false)], 5),
            ];
            out.push(probe);
        }
    }

    // Raw negative runtime presets are reached ONLY through a valid D operand;
    // source K presets retain the documented nonnegative parser contract.
    for timer in [0, 127, 184, 240, 255] {
        for preset in [0, -1, -32768] {
            out.push(fixture(
                &format!("TMR_T{timer}_live_D_preset_{preset}"),
                &format!("MOV K3 D50\n{}", flags(false, false)),
                &format!(
                    "LD X2\nMOV K{preset} D50\nLD X0\nTMR T{timer} D50\nOUT Y2\n{}",
                    observe(timer)
                ),
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn timer_corpus_compiles_encodes_and_completes_bounded_local_scans() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let mut names = std::collections::BTreeSet::new();
            let mut errors = Vec::new();
            for c in super::cases(target) {
                assert!(names.insert(c.name.clone()), "{}", c.name);
                let program = match opendvp::plc::compile(&c.source) {
                    Ok(p) => p,
                    Err(e) => {
                        errors.push(format!("{}: {e}", c.name));
                        continue;
                    }
                };
                match opendvp::es2_encoder::encode(&program) {
                    Ok(e) => assert!(e.next_pc.len() <= 512, "{}", c.name),
                    Err(e) => {
                        errors.push(format!("{}: {e}", c.name));
                        continue;
                    }
                }
                let mut sim = opendvp::plc::Simulator::default();
                for (inputs, scans) in c.stages {
                    for (name, value) in inputs {
                        sim.memory.set_input(name.parse().unwrap(), value);
                    }
                    for _ in 0..scans {
                        sim.scan(&program, 100);
                        if let opendvp::plc::ScanStatus::Fault(e) = &sim.last_scan {
                            errors.push(format!("{}: {e}", c.name));
                            break;
                        }
                    }
                }
            }
            for timer in 0..=255 {
                assert!(
                    names.contains(&format!("special_timers_TMR_T{timer}_K3_flags00")),
                    "missing timer-class address T{timer}"
                );
            }
            assert!(errors.is_empty(), "{}", errors.join("\n"));
        }
    }
}
