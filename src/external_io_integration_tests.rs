//! Compiled VM tests of the named virtual peripherals. These do not assert
//! hardware scan rates or parity with the installed PC-only native stubs.
use crate::execution_plan::PlanOp;
use crate::external_io::LongHeld;
use crate::plc::{Action, Device, Program, ScanStatus, Simulator, compile_il};
fn d(s: &str) -> Device {
    s.parse().unwrap()
}
fn scan(s: &mut Simulator, p: &Program) {
    s.scan(p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(
        s.memory.execution_limitations.is_empty(),
        "{:?}",
        s.memory.execution_limitations
    );
}
fn site(p: &Program) -> u16 {
    p.execution_plan
        .nodes
        .iter()
        .find_map(|n| match n.op {
            PlanOp::Action {
                network, action, ..
            } if matches!(p.networks[network].actions[action], Action::ExternalIo(_)) => Some(n.pc),
            _ => None,
        })
        .unwrap()
}
fn enabled() -> Simulator {
    let mut s = Simulator::default();
    s.memory.set_input(d("X20"), true);
    s
}
fn visits(s: &mut Simulator, p: &Program, n: usize) {
    for _ in 0..n {
        scan(s, p);
    }
}
fn byte(s: &Simulator, base: u16) -> u8 {
    (0..8).fold(0, |v, n| {
        v | (u8::from(s.memory.physical_output(Device {
            area: crate::plc::Area::Y,
            index: base + n,
        })) << n)
    })
}
fn ascii(s: &mut Simulator, bytes: &[u8]) {
    for (i, pair) in bytes.chunks(2).enumerate() {
        s.memory.set_word(
            Device {
                area: crate::plc::Area::D,
                index: i as u16,
            },
            i16::from_le_bytes([pair[0], *pair.get(1).unwrap_or(&0)]),
        );
    }
}
#[test]
fn all_eight_forms_preserve_genuine_local_headers_and_following_word_pc() {
    for (line, header, len) in [
        ("TKY X0 D0 M100", 0x9c00, 7),
        ("DTKY X0 D0 M100", 0x9d00, 13),
        ("HKY X0 Y0 D0 M100", 0x9e00, 9),
        ("DHKY X0 Y0 D0 M100", 0x9f00, 17),
        ("DSW X0 Y0 D0 K1", 0xa000, 9),
        ("SEGL D0 Y0 K0", 0xa400, 7),
        ("ARWS M0 D0 Y0 K0", 0xa600, 9),
        ("PR D0 Y0", 0xaa00, 5),
    ] {
        let p = compile_il(&format!("LD X20\n{line}\nMOV K7 D100\nEND")).unwrap();
        let encoded = crate::es2_encoder::local_execution_layout(&p).unwrap();
        let pc = site(&p);
        assert_eq!(encoded.known_word_at(pc), Some(header));
        assert_eq!(encoded.action_pc[1], pc + len);
        assert!(
            crate::es2_encoder::encode(&p).is_err(),
            "native stub must stay blocked: {line}"
        );
    }
}
#[test]
fn undocumented_pulse_and_extra_wide_spellings_are_rejected() {
    for line in [
        "TKYP X0 D0 M0",
        "DTKYP X0 D0 M0",
        "HKYP X0 Y0 D0 M0",
        "DHKYP X0 Y0 D0 M0",
        "DDSW X0 Y0 D0 K1",
        "DSEGL D0 Y0 K0",
        "DARWS M0 D0 Y0 K0",
        "DPR D0 Y0",
        "PRP D0 Y0",
    ] {
        assert!(
            compile_il(&format!("LD X20\n{line}\nEND")).is_err(),
            "{line}"
        );
    }
}
#[test]
fn printer_limit_counts_compiler_sites_even_if_contacts_are_false() {
    assert!(compile_il("LD X0\nPR D0 Y0\nPR D10 Y20\nEND").is_ok());
    assert!(compile_il("LD X0\nPR D0 Y0\nPR D10 Y20\nPR D20 Y40\nEND").is_err());
}
#[test]
fn tky_published_5301_held_release_and_disable_hold_value() {
    let p = compile_il("LD X20\nTKY X0 D0 M100\nEND").unwrap();
    let mut s = enabled();
    for k in [5, 3, 0, 1] {
        s.memory.set_input(
            Device {
                area: crate::plc::Area::X,
                index: k,
            },
            true,
        );
        scan(&mut s, &p);
        scan(&mut s, &p);
        s.memory.set_input(
            Device {
                area: crate::plc::Area::X,
                index: k,
            },
            false,
        );
        scan(&mut s, &p);
    }
    assert_eq!(s.memory.word(d("D0")), 5301);
    assert!(s.memory.bit(d("M101")));
    assert!(!s.memory.bit(d("M110")));
    s.memory.set_input(d("X20"), false);
    scan(&mut s, &p);
    assert_eq!(s.memory.word(d("D0")), 5301);
    assert!(!(100..111).any(|i| s.memory.bit(Device {
        area: crate::plc::Area::M,
        index: i
    })));
}
#[test]
fn dtky_wraps_eight_decimal_digits_without_signed_word_truncation() {
    let p = compile_il("LD X20\nDTKY X0 D0 M100\nEND").unwrap();
    let mut s = enabled();
    s.memory.set_double_word(d("D0"), 99_999_999);
    s.memory.set_input(d("X2"), true);
    scan(&mut s, &p);
    assert_eq!(s.memory.double_word(d("D0")), 99_999_992);
}
#[test]
fn hky_queued_earliest_key_wins_and_held_key_does_not_repeat() {
    let p = compile_il("LD X20\nHKY X0 Y0 D0 M100\nEND").unwrap();
    let mut s = enabled();
    let pc = site(&p);
    s.memory.external_io.queue_key(100, pc, 5, true).unwrap();
    s.memory.external_io.queue_key(200, pc, 2, true).unwrap();
    crate::timing_io::advance_by(&mut s.memory, 300);
    visits(&mut s, &p, 8);
    assert_eq!(s.memory.word(d("D0")), 5);
    assert!(s.memory.bit(d("M107")));
    visits(&mut s, &p, 8);
    assert_eq!(s.memory.word(d("D0")), 5);
    assert!(!s.memory.bit(d("M107")));
}
#[test]
fn dhky_m1167_hex_digit_and_function_event_are_distinct() {
    let p = compile_il("LD X20\nDHKY X0 Y0 D0 M100\nEND").unwrap();
    let mut s = enabled();
    let pc = site(&p);
    s.memory.set_system(1167, true);
    s.memory.set_double_word(d("D0"), 0x1234567);
    s.memory.external_io.set_keyboard(pc, &[15]).unwrap();
    visits(&mut s, &p, 8);
    assert_eq!(s.memory.double_word(d("D0")), 0x1234567f);
    assert!(s.memory.bit(d("M105")));
    assert!(s.memory.bit(d("M106")));
}
#[test]
fn dsw_two_groups_commit_only_at_the_actual_400ms_deadline() {
    let p = compile_il("LD X20\nDSW X0 Y0 D0 K2\nEND").unwrap();
    let mut s = enabled();
    let pc = site(&p);
    s.memory.external_io.set_dip(pc, [5301, 9876]).unwrap();
    scan(&mut s, &p);
    crate::timing_io::advance_by(&mut s.memory, 399_999);
    assert_eq!(s.memory.word(d("D0")), 0);
    assert_eq!(s.memory.word(d("D1")), 0);
    crate::timing_io::advance_by(&mut s.memory, 1);
    assert_eq!(s.memory.word(d("D0")), 5301);
    assert_eq!(s.memory.word(d("D1")), 9876);
    assert!(s.memory.system_bit(1029));
}
#[test]
fn dsw_stop_cancels_future_rows_and_rearm_starts_a_new_cycle() {
    let p = compile_il("LD X20\nDSW X0 Y0 D0 K1\nEND").unwrap();
    let mut s = enabled();
    s.memory.external_io.set_dip(site(&p), [1234, 0]).unwrap();
    scan(&mut s, &p);
    crate::timing_io::advance_by(&mut s.memory, 200_000);
    s.memory.set_input(d("X20"), false);
    scan(&mut s, &p);
    crate::timing_io::advance_by(&mut s.memory, 500_000);
    assert_eq!(s.memory.word(d("D0")), 0);
    s.memory.set_input(d("X20"), true);
    scan(&mut s, &p);
    crate::timing_io::advance_by(&mut s.memory, 400_000);
    assert_eq!(s.memory.word(d("D0")), 1234);
}
#[test]
fn segl_two_displays_latch_both_independent_decimal_words() {
    let p = compile_il("LD X20\nSEGL D0 Y0 K4\nEND").unwrap();
    let mut s = enabled();
    s.memory.set_word(d("D0"), 5301);
    s.memory.set_word(d("D1"), 9876);
    visits(&mut s, &p, 12);
    assert_eq!(
        s.memory.external_io.display_latches[&site(&p)],
        [5301, 9876]
    );
    assert!(s.memory.system_bit(1029));
}
#[test]
fn segl_inverted_data_and_latch_polarity_still_observe_correct_number() {
    let p = compile_il("LD X20\nSEGL D0 Y0 K3\nEND").unwrap();
    let mut s = enabled();
    s.memory.set_word(d("D0"), 1234);
    scan(&mut s, &p);
    assert_eq!(byte(&s, 0) & 15, 11);
    scan(&mut s, &p);
    assert!(!s.memory.physical_output(d("Y4")));
    visits(&mut s, &p, 10);
    assert_eq!(s.memory.external_io.display_latches[&site(&p)][0], 1234);
}
#[test]
fn arrows_wrap_selected_digit_without_carry_then_shift_right() {
    let p = compile_il("LD X20\nARWS M0 D0 Y0 K0\nEND").unwrap();
    let mut s = enabled();
    s.memory.set_word(d("D0"), 9999);
    s.memory.set_internal(d("M1"), true);
    scan(&mut s, &p);
    assert_eq!(s.memory.word(d("D0")), 999);
    assert_eq!(s.memory.external_io.selected_digit(site(&p)), Some(3));
    assert!(s.memory.physical_output(d("Y7")));
    s.memory.set_internal(d("M1"), false);
    scan(&mut s, &p);
    s.memory.set_internal(d("M2"), true);
    scan(&mut s, &p);
    assert_eq!(s.memory.external_io.selected_digit(site(&p)), Some(2));
    assert!(s.memory.physical_output(d("Y6")));
}
#[test]
fn pr_eight_packed_ascii_bytes_have_setup_strobe_hold_and_one_frame() {
    let p = compile_il("LD X20\nPR D0 Y0\nEND").unwrap();
    let mut s = enabled();
    ascii(&mut s, b"Ab ;//!? ");
    scan(&mut s, &p);
    assert_eq!(byte(&s, 0), b'A');
    assert!(!s.memory.physical_output(d("Y10")));
    assert!(s.memory.physical_output(d("Y11")));
    scan(&mut s, &p);
    assert!(s.memory.physical_output(d("Y10")));
    scan(&mut s, &p);
    assert!(!s.memory.physical_output(d("Y10")));
    visits(&mut s, &p, 21);
    assert_eq!(
        s.memory.external_io.printer_frames[&site(&p)][0],
        b"Ab ;//!?"
    );
    assert!(s.memory.system_bit(1029));
    visits(&mut s, &p, 6);
    assert_eq!(s.memory.external_io.printer_frames[&site(&p)].len(), 1);
}
#[test]
fn long_pr_finishes_after_false_contact_and_nul_excludes_tail() {
    let p = compile_il("LD X20\nPR D0 Y0\nEND").unwrap();
    let mut s = enabled();
    s.memory.set_system(1027, true);
    ascii(&mut s, b"Ab\0ignored");
    scan(&mut s, &p);
    s.memory.set_input(d("X20"), false);
    visits(&mut s, &p, 5);
    assert_eq!(s.memory.external_io.printer_frames[&site(&p)][0], b"Ab");
    assert!(s.memory.system_bit(1029));
    assert!(!s.memory.physical_output(d("Y11")));
}
#[test]
fn long_pr_explicit_hold_completed_profile_never_reprints_or_pulses_while_held() {
    let p = compile_il("LD X20\nPR D0 Y0\nEND").unwrap();
    let mut s = enabled();
    s.memory.external_io.long_held = LongHeld::HoldCompleted;
    s.memory.set_system(1027, true);
    ascii(&mut s, b"A\0");
    visits(&mut s, &p, 9);
    assert_eq!(s.memory.external_io.printer_frames[&site(&p)].len(), 1);
    assert!(!s.memory.system_bit(1029));
    assert!(s.memory.physical_output(d("Y11")));
    s.memory.set_input(d("X20"), false);
    scan(&mut s, &p);
    assert!(s.memory.system_bit(1029));
}
#[test]
fn pulse_owned_output_is_not_stolen_by_segl() {
    let p = compile_il("LD X20\nPLSY K1000 K0 Y0\nSEGL D0 Y0 K0\nEND").unwrap();
    let mut s = enabled();
    s.scan(&p, 0);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert!(!s.memory.execution_limitations.is_empty());
    assert!(s.memory.pulse_output.is_busy(0));
    assert_eq!(s.memory.external_io.active_count(), 0);
}
#[test]
fn mtr_owned_strobes_are_not_stolen_by_hky() {
    let p = compile_il("LD X20\nMTR X40 Y0 M200 K2\nHKY X0 Y0 D0 M100\nEND").unwrap();
    let mut s = enabled();
    let mtr = p
        .execution_plan
        .nodes
        .iter()
        .find_map(|n| match n.op {
            PlanOp::Action {
                network, action, ..
            } if p.networks[network].actions[action]
                .label()
                .starts_with("MTR ") =>
            {
                Some(n.pc)
            }
            _ => None,
        })
        .unwrap();
    s.memory
        .refresh_io
        .set_matrix_rows(mtr as usize, vec![1, 2])
        .unwrap();
    s.scan(&p, 0);
    assert!(!s.memory.execution_limitations.is_empty());
    assert_eq!(s.memory.external_io.active_count(), 0);
}
#[test]
fn matrix_and_keyboard_shared_inputs_respect_first_owner_in_both_orders() {
    for matrix_first in [false, true] {
        let matrix = "MTR X0 Y20 M200 K2";
        let keyboard = "HKY X0 Y40 D0 M100";
        let body = if matrix_first {
            format!("{matrix}\n{keyboard}")
        } else {
            format!("{keyboard}\n{matrix}")
        };
        let p = compile_il(&format!("LD X20\n{body}\nEND")).unwrap();
        let mtr = p
            .execution_plan
            .nodes
            .iter()
            .find_map(|node| match node.op {
                PlanOp::Action {
                    network, action, ..
                } if matches!(p.networks[network].actions[action], Action::Refresh(_)) => {
                    Some(node.pc)
                }
                _ => None,
            })
            .unwrap();
        let mut s = enabled();
        s.memory
            .refresh_io
            .set_matrix_rows(mtr as usize, vec![1, 2])
            .unwrap();
        s.scan(&p, 0);
        assert!(!s.memory.execution_limitations.is_empty());
        assert_eq!(s.memory.refresh_io.matrix_owner().is_some(), matrix_first);
        assert_eq!(
            s.memory.external_io.active_count(),
            usize::from(!matrix_first)
        );
    }
}
#[test]
fn idle_dip_completion_is_visible_to_exactly_one_following_plc_body() {
    let p = compile_il("LD X20\nDSW X0 Y0 D0 K1\nLD M1029\nINC D100\nEND").unwrap();
    let mut s = enabled();
    s.memory.external_io.set_dip(site(&p), [5301, 0]).unwrap();
    s.scan(&p, 400);
    assert_eq!(s.last_scan, ScanStatus::Complete);
    assert_eq!(s.memory.word(d("D0")), 5301);
    assert_eq!(s.memory.word(d("D100")), 0);
    scan(&mut s, &p);
    assert_eq!(s.memory.word(d("D100")), 1);
    scan(&mut s, &p);
    assert_eq!(s.memory.word(d("D100")), 1);
}
#[test]
fn csfo_reserved_output_is_not_stolen_by_printer() {
    let p = compile_il("LD X20\nCSFO X0 D100 D110\nPR D0 Y0\nEND").unwrap();
    let mut s = enabled();
    s.memory.set_word(d("D100"), 1);
    s.memory.set_word(d("D110"), 100);
    s.scan(&p, 0);
    assert!(!s.memory.execution_limitations.is_empty());
    assert_eq!(s.memory.external_io.active_count(), 0);
    assert!(matches!(
        s.memory.timing_io.output_owners.get(&d("Y0")),
        Some(crate::timing_io::ResourceOwner::Csfo(_))
    ));
}
#[test]
fn m1034_suppresses_actual_printer_bytes_but_retains_command_record() {
    let p = compile_il("LD X20\nSET M1034\nPR D0 Y0\nEND").unwrap();
    let mut s = enabled();
    ascii(&mut s, b"ABCDEFGH");
    visits(&mut s, &p, 24);
    assert_eq!(
        s.memory.external_io.printer_commands[&site(&p)][0],
        b"ABCDEFGH"
    );
    assert!(s.memory.external_io.printer_frames[&site(&p)][0].is_empty());
    assert!((0..10).all(|i| !s.memory.physical_output(Device {
        area: crate::plc::Area::Y,
        index: i
    })));
}
#[test]
fn newer_m1029_write_survives_external_completion_aging() {
    let p = compile_il("LD X20\nSEGL D0 Y0 K0\nLD X21\nSET M1029\nEND").unwrap();
    let mut s = enabled();
    visits(&mut s, &p, 12);
    s.memory.set_input(d("X20"), false);
    s.memory.set_input(d("X21"), true);
    scan(&mut s, &p);
    s.memory.set_input(d("X21"), false);
    scan(&mut s, &p);
    assert!(s.memory.system_bit(1029));
}
#[test]
fn watchdog_abort_cancels_external_deadlines_without_reasserting_outputs() {
    let p = compile_il("LD X20\nDSW X0 Y0 D0 K1\nDELAY K1000\nDELAY K1000\nDELAY K1\nEND").unwrap();
    let mut s = enabled();
    s.scan(&p, 0);
    assert_ne!(s.last_scan, ScanStatus::Complete);
    assert!(s.memory.system_bit(1008));
    assert_eq!(s.memory.external_io.active_count(), 0);
    crate::timing_io::advance_by(&mut s.memory, 1_000_000);
    assert!((0..4).all(|i| !s.memory.physical_output(Device {
        area: crate::plc::Area::Y,
        index: i
    })));
    assert_eq!(s.memory.word(d("D0")), 0);
}
#[test]
fn call_for_revisits_one_printer_pc_and_advances_the_declared_visit_cadence() {
    let p =
        compile_il("LD X20\nFOR K3\nCALL P0\nNEXT\nFEND\nP0\nLD X20\nPR D0 Y0\nSRET\nEND").unwrap();
    let mut s = enabled();
    ascii(&mut s, b"ABCDEFGH");
    visits(&mut s, &p, 8);
    assert_eq!(s.memory.external_io.printer_frames.len(), 1);
    assert_eq!(
        s.memory.external_io.printer_frames[&site(&p)][0],
        b"ABCDEFGH"
    );
    assert_eq!(s.memory.external_io.active_count(), 1);
}
