//! Differential checks against a digest-verified, isolated original ES2 process.
#[path = "es2_compare/additional_operand_cases.rs"]
mod additional_operand_cases;
#[path = "es2_compare/alarm_cases.rs"]
mod alarm_cases;
#[path = "es2_compare/arithmetic_operand_cases.rs"]
mod arithmetic_operand_cases;
#[path = "es2_compare/bcd_operand_cases.rs"]
mod bcd_operand_cases;
#[path = "es2_compare/bit_operand_cases.rs"]
mod bit_operand_cases;
#[path = "es2_compare/bit_target_width_cases.rs"]
mod bit_target_width_cases;
#[path = "es2_compare/block_operand_cases.rs"]
mod block_operand_cases;
#[path = "es2_compare/calendar_cases.rs"]
mod calendar_cases;
#[path = "es2_compare/cmpt_profile_cases.rs"]
mod cmpt_profile_cases;
#[path = "es2_compare/comparison_operand_cases.rs"]
mod comparison_operand_cases;
#[path = "es2_compare/data_operand_cases.rs"]
mod data_operand_cases;
#[path = "es2_compare/data_transform_cases.rs"]
mod data_transform_cases;
#[path = "es2_compare/dcnt_cases.rs"]
mod dcnt_cases;
#[path = "es2_compare/dcnt_reset_cases.rs"]
mod dcnt_reset_cases;
#[path = "es2_compare/decimal_scratch_profile_cases.rs"]
mod decimal_scratch_profile_cases;
#[path = "es2_compare/drum_cases.rs"]
mod drum_cases;
#[path = "es2_compare/dtm_modern_cases.rs"]
mod dtm_modern_cases;
#[path = "es2_compare/dtm_solar_cases.rs"]
mod dtm_solar_cases;
#[path = "es2_compare/exchange_operand_cases.rs"]
mod exchange_operand_cases;
#[path = "es2_compare/float_move_operand_cases.rs"]
mod float_move_operand_cases;
#[path = "es2_compare/indexed_abs_gray_cases.rs"]
mod indexed_abs_gray_cases;
#[path = "es2_compare/indexed_angle_cases.rs"]
mod indexed_angle_cases;
#[path = "es2_compare/indexed_bcd_cases.rs"]
mod indexed_bcd_cases;
#[path = "es2_compare/indexed_bit_cases.rs"]
mod indexed_bit_cases;
#[path = "es2_compare/indexed_bit_number_cases.rs"]
mod indexed_bit_number_cases;
#[path = "es2_compare/indexed_block_exchange_cases.rs"]
mod indexed_block_exchange_cases;
#[path = "es2_compare/indexed_calendar_time_cases.rs"]
mod indexed_calendar_time_cases;
#[path = "es2_compare/indexed_checksum_cases.rs"]
mod indexed_checksum_cases;
#[path = "es2_compare/indexed_clock_read_cases.rs"]
mod indexed_clock_read_cases;
#[path = "es2_compare/indexed_comparison_cases.rs"]
mod indexed_comparison_cases;
#[path = "es2_compare/indexed_data_processing_cases.rs"]
mod indexed_data_processing_cases;
#[path = "es2_compare/indexed_decimal_conversion_cases.rs"]
mod indexed_decimal_conversion_cases;
#[path = "es2_compare/indexed_desqr_cases.rs"]
mod indexed_desqr_cases;
#[path = "es2_compare/indexed_exp_ln_cases.rs"]
mod indexed_exp_ln_cases;
#[path = "es2_compare/indexed_float_binary_cases.rs"]
mod indexed_float_binary_cases;
#[path = "es2_compare/indexed_float_cases.rs"]
mod indexed_float_cases;
#[path = "es2_compare/indexed_float_comparison_cases.rs"]
mod indexed_float_comparison_cases;
#[path = "es2_compare/indexed_literal_move_cases.rs"]
mod indexed_literal_move_cases;
#[path = "es2_compare/indexed_log_pow_cases.rs"]
mod indexed_log_pow_cases;
#[path = "es2_compare/indexed_multiply_cases.rs"]
mod indexed_multiply_cases;
#[path = "es2_compare/indexed_random_cases.rs"]
mod indexed_random_cases;
#[path = "es2_compare/indexed_real_cases.rs"]
mod indexed_real_cases;
#[path = "es2_compare/indexed_root_cases.rs"]
mod indexed_root_cases;
#[path = "es2_compare/indexed_rotation_cases.rs"]
mod indexed_rotation_cases;
#[path = "es2_compare/indexed_rtc_cases.rs"]
mod indexed_rtc_cases;
#[path = "es2_compare/indexed_scalar_cases.rs"]
mod indexed_scalar_cases;
#[path = "es2_compare/indexed_search_toggle_cases.rs"]
mod indexed_search_toggle_cases;
#[path = "es2_compare/indexed_segment_cases.rs"]
mod indexed_segment_cases;
#[path = "es2_compare/indexed_shift_fifo_cases.rs"]
mod indexed_shift_fifo_cases;
#[path = "es2_compare/indexed_swap_mask_cases.rs"]
mod indexed_swap_mask_cases;
#[path = "es2_compare/indexed_text_cases.rs"]
mod indexed_text_cases;
#[path = "es2_compare/indexed_trig_cases.rs"]
mod indexed_trig_cases;
#[path = "es2_compare/indexed_truncated_cases.rs"]
mod indexed_truncated_cases;
#[path = "es2_compare/m1019_cases.rs"]
mod m1019_cases;
#[path = "es2_compare/matrix_bulk_cases.rs"]
mod matrix_bulk_cases;
#[path = "es2_compare/matrix_cases.rs"]
mod matrix_cases;
#[path = "es2_compare/matrix_counter_cases.rs"]
mod matrix_counter_cases;
#[path = "es2_compare/matrix_input_cases.rs"]
mod matrix_input_cases;
#[path = "es2_compare/mmov_cases.rs"]
mod mmov_cases;
#[path = "es2_compare/move_logic_operand_cases.rs"]
mod move_logic_operand_cases;
#[path = "es2_compare/multiply_operand_cases.rs"]
mod multiply_operand_cases;
#[path = "es2_compare/program_control_cases.rs"]
mod program_control_cases;
#[path = "es2_compare/program_control_mixed_cases.rs"]
mod program_control_mixed_cases;
#[path = "es2_compare/prun_cases.rs"]
mod prun_cases;
#[path = "es2_compare/reff_cases.rs"]
mod reff_cases;
#[path = "es2_compare/reset_counter_operand_cases.rs"]
mod reset_counter_operand_cases;
#[path = "es2_compare/search_operand_cases.rs"]
mod search_operand_cases;
#[path = "es2_compare/search_state_cases.rs"]
mod search_state_cases;
#[path = "es2_compare/search_value_cases.rs"]
mod search_value_cases;
#[path = "es2_compare/sfc_cases.rs"]
mod sfc_cases;
#[path = "es2_compare/sfc_extra_cases.rs"]
mod sfc_extra_cases;
#[path = "es2_compare/shift_operand_cases.rs"]
mod shift_operand_cases;
#[path = "es2_compare/solar_cases.rs"]
mod solar_cases;
#[path = "es2_compare/special_timer_cases.rs"]
mod special_timer_cases;
#[path = "es2_compare/st_cases.rs"]
mod st_cases;
#[path = "es2_compare/system_clock_cases.rs"]
mod system_clock_cases;
#[path = "es2_compare/text_operand_cases.rs"]
mod text_operand_cases;
use opendvp::{
    es2_bridge::{DEFAULT_SIMULATOR, Reading, Session, Target},
    plc::{self, Area, Device},
    project::Project,
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

struct Case {
    rtc: Option<indexed_rtc_cases::NativeFixture>,
    name: String,
    source: String,
    stages: Vec<(Vec<(&'static str, bool)>, usize)>,
}
fn case(name: &str, source: &str, stages: Vec<(Vec<(&'static str, bool)>, usize)>) -> Case {
    Case {
        rtc: None,
        name: name.into(),
        source: source.into(),
        stages,
    }
}
fn suite(target: Target) -> Vec<Case> {
    let mut cases = vec![
        case(
            "branch_manual_example",
            "LD X0\nMPS\nAND X1\nOUT Y1\nMRD\nAND X2\nOUT M0\nMPP\nOUT Y2\nEND",
            (0..8)
                .map(|m| {
                    (
                        vec![("X0", m & 1 != 0), ("X1", m & 2 != 0), ("X2", m & 4 != 0)],
                        1,
                    )
                })
                .collect(),
        ),
        case(
            "branch_saved_before_internal_writes",
            "LD X0\nOUT M0\nLD M0\nMPS\nRST M0\nMRD\nOUT Y0\nMRD\nOUT Y1\nMPP\nOUT Y2\nEND",
            vec![
                (vec![("X0", true)], 2),
                (vec![("X0", false)], 2),
                (vec![("X0", true)], 1),
            ],
        ),
        case(
            "branch_mps_after_output_uses_previous_result",
            "LD X0\nOUT M0\nLD M0\nRST M0\nMPS\nAND X1\nOUT Y0\nMPP\nOUT Y1\nEND",
            vec![
                (vec![("X0", true), ("X1", true)], 2),
                (vec![("X1", false)], 1),
                (vec![("X0", false)], 1),
            ],
        ),
        case(
            "branch_nested_lifo",
            "LD X0\nMPS\nAND X1\nMPS\nAND X2\nOUT Y0\nMRD\nANI X3\nOUT Y1\nMPP\nOUT Y2\nMRD\nOUT Y3\nMPP\nOUT Y4\nEND",
            (0..16)
                .map(|m| {
                    (
                        vec![
                            ("X0", m & 1 != 0),
                            ("X1", m & 2 != 0),
                            ("X2", m & 4 != 0),
                            ("X3", m & 8 != 0),
                        ],
                        1,
                    )
                })
                .collect(),
        ),
        case(
            "branch_edge_prefix_is_sampled_once",
            "LDP X0\nMPS\nAND X1\nOUT Y0\nMRD\nINC D0\nMPP\nOUT Y1\nEND",
            vec![
                (vec![("X0", true), ("X1", true)], 3),
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 2),
            ],
        ),
        case(
            "branch_timer_counter_pulse_actions",
            "LD X0\nMPS\nTMR T0 K2\nMRD\nCNT C0 K2\nMRD\nINCP D0\nMPP\nOUT Y0\nEND",
            vec![
                (vec![("X0", true)], 3),
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 3),
            ],
        ),
        case(
            "edge_contacts_load_hold_rearm",
            "LDP X0\nOUT Y0\nINC D0\nLDF X0\nOUT Y1\nINC D1\nEND",
            vec![
                (vec![], 1),
                (vec![("X0", true)], 3),
                (vec![("X0", false)], 2),
                (vec![("X0", true)], 1),
                (vec![("X0", false)], 1),
            ],
        ),
        case(
            "edge_contacts_first_scan_on",
            "LDP X0\nOUT Y0\nLDF X0\nOUT Y1\nEND",
            vec![
                (vec![("X0", true)], 2),
                (vec![("X0", false)], 2),
                (vec![("X0", true)], 2),
            ],
        ),
        case(
            "edge_series_parallel_gate_history",
            "LD X0\nANDP X1\nOUT Y0\nLD X0\nANDF X1\nOUT Y1\nLD X0\nORP X1\nOUT Y2\nLD X0\nORF X1\nOUT Y3\nEND",
            vec![
                (vec![("X1", true)], 2),
                (vec![("X0", true)], 2),
                (vec![("X1", false)], 2),
                (vec![("X0", false), ("X1", true)], 1),
                (vec![("X1", false)], 2),
            ],
        ),
        case(
            "edge_internal_writes_and_occurrences",
            "LDP M0\nOUT Y0\nLD X0\nOUT M0\nLDP M0\nOUT Y1\nLDF M0\nOUT Y2\nLD X1\nRST M0\nLDP M0\nOUT Y3\nLDF M0\nOUT Y4\nEND",
            vec![
                (vec![("X0", true)], 2),
                (vec![("X1", true)], 2),
                (vec![("X0", false)], 2),
                (vec![("X1", false), ("X0", true)], 2),
            ],
        ),
        case(
            "edge_extended_memory_operands",
            "LD X0\nOUT M2000\nLDP M2000\nOUT Y0\nLDF M2000\nOUT Y1\nLD X1\nANDP M2000\nOUT Y2\nLD X1\nORF M2000\nOUT Y3\nEND",
            vec![
                (vec![("X0", true)], 2),
                (vec![("X1", true)], 1),
                (vec![("X0", false)], 2),
                (vec![("X0", true)], 2),
            ],
        ),
        case(
            "edge_timer_counter_contacts",
            "LD X0\nCNT C0 K2\nTMR T0 K1\nLDP C0\nOUT Y0\nLDF C0\nOUT Y1\nLDP T0\nOUT Y2\nLDF T0\nOUT Y3\nLD X1\nRST C0\nEND",
            vec![
                (vec![("X0", true)], 3),
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 2),
                (vec![("X1", true)], 1),
                (vec![("X0", false)], 2),
            ],
        ),
        case(
            "result_edges_np_pn_vs_contact_edges",
            "LD X0\nAND X1\nNP\nOUT Y0\nLD X0\nAND X1\nPN\nOUT Y1\nLD X0\nANDP X1\nOUT Y2\nEND",
            vec![
                (vec![("X1", true)], 2),
                (vec![("X0", true)], 2),
                (vec![("X0", false)], 2),
                (vec![("X0", true)], 1),
                (vec![("X1", false)], 2),
            ],
        ),
        case(
            "word_logic_and_aliases",
            "LD X0\nWAND HA55A H0FF0 D0\nWOR HA55A H0FF0 D1\nWXOR HA55A H0FF0 D2\nCML HA55A D3\nWAND D0 D1 D4\nWOR D4 H8000 D4\nWXOR D4 HFFFF D4\nCML D4 D4\nEND",
            vec![
                (vec![], 1),
                (vec![("X0", true)], 2),
                (vec![("X0", false)], 1),
            ],
        ),
        case(
            "inc_dec_neg_boundaries",
            "LD X1\nMOV H7FFF D0\nMOV H8000 D1\nMOV H8000 D2\nMOV K-123 D3\nLD X0\nINC D0\nDEC D1\nNEG D2\nNEG D3\nEND",
            vec![
                (vec![("X1", true)], 1),
                (vec![("X1", false), ("X0", true)], 3),
                (vec![("X0", false)], 2),
            ],
        ),
        case(
            "pulse_numeric_forms",
            "LD X1\nMOV H7FFF D13\nMOV H8000 D14\nMOV HAAAA D15\nMOV K-123 D16\nMOV HFFFF D17\nLD X0\nINC D10\nMOVP D10 D11\nADDP D12 K1 D12\nSUBP D20 K1 D20\nINCP D13\nDECP D14\nCMLP D15 D15\nNEGP D16\nWANDP D17 HF0F0 D17\nWORP D18 H0F0F D18\nWXORP D19 HFFFF D19\nEND",
            vec![
                (vec![("X1", true)], 1),
                (vec![("X1", false), ("X0", true)], 4),
                (vec![("X0", false)], 2),
                (vec![("X0", true)], 3),
            ],
        ),
        case(
            "pulse_first_scan_and_occurrences",
            "LD X0\nINCP D0\nINCP D0\nADDP D0 K1 D1\nLD X1\nINCP D0\nEND",
            vec![
                (vec![("X0", true), ("X1", true)], 3),
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 2),
                (vec![("X1", false)], 1),
                (vec![("X1", true)], 1),
            ],
        ),
        case(
            "pulse_extended_destinations_and_counter_operand",
            "LD X0\nINCP D4096\nCMLP D4096 D4999\nWXORP D4999 C0 D4998\nEND",
            vec![
                (vec![("X0", true)], 3),
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 2),
            ],
        ),
        case(
            "contact_and_inversion",
            "LD X0\nOUT Y0\nLDI X0\nOUT Y1\nEND",
            vec![
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 2),
                (vec![("X0", false)], 1),
            ],
        ),
        case(
            "series_and_parallel",
            "LD X0\nAND X1\nOR X2\nANI X3\nOUT Y0\nEND",
            (0..16)
                .map(|mask| {
                    (
                        vec![
                            ("X0", mask & 1 != 0),
                            ("X1", mask & 2 != 0),
                            ("X2", mask & 4 != 0),
                            ("X3", mask & 8 != 0),
                        ],
                        1,
                    )
                })
                .collect(),
        ),
        case(
            "seal_in_stop_priority",
            "LD X0\nOR Y0\nANI X1\nOUT Y0\nEND",
            vec![
                (vec![], 1),
                (vec![("X0", true)], 1),
                (vec![("X0", false)], 2),
                (vec![("X1", true)], 1),
                (vec![("X0", true)], 1),
            ],
        ),
        case(
            "set_reset_order",
            "LD X0\nSET M0\nLD X1\nRST M0\nLD M0\nOUT Y0\nEND",
            vec![
                (vec![("X0", true)], 1),
                (vec![("X0", false)], 1),
                (vec![("X1", true)], 1),
                (vec![("X0", true)], 1),
            ],
        ),
        case(
            "mov_add_sub",
            "LD X0\nMOV K125 D0\nADD D0 K7 D1\nSUB D1 K40 D2\nEND",
            vec![(vec![], 1), (vec![("X0", true)], 2)],
        ),
        case(
            "signed_overflow",
            "LD X0\nMOV H7FFF D0\nADD D0 K1 D1\nSUB D1 K1 D2\nADD H8000 K-1 D3\nSUB K-32768 K1 D4\nEND",
            vec![(vec![("X0", true)], 1)],
        ),
        case(
            "extended_m_and_d",
            if target == Target::Ss2 {
                "LD X10\nOUT M2000\nLD M2000\nMOV K321 D4096\nADD D4096 K1 D4999\nOUT Y10\nEND"
            } else {
                "LD X10\nOUT M2000\nLD M2000\nMOV K321 D4096\nADD D4096 K1 D9999\nOUT Y10\nEND"
            },
            vec![(vec![("X10", true)], 1), (vec![("X10", false)], 1)],
        ),
        case(
            "counter_edges_and_reset",
            "LD X0\nCNT C0 K2\nLD X1\nRST C0\nLD C0\nOUT Y0\nEND",
            vec![
                (vec![("X0", true)], 3),
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 2),
                (vec![("X1", true)], 1),
                (vec![("X1", false)], 2),
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 1),
            ],
        ),
        case(
            "timer_fixed_scan",
            "LD X0\nTMR T0 K3\nLD T0\nOUT Y0\nEND",
            vec![
                (vec![("X0", true)], 20),
                (vec![("X0", false)], 2),
                (vec![("X0", true)], 6),
            ],
        ),
        case(
            "timer_1ms_base",
            "LD X0\nTMR T127 K150\nLD T127\nOUT Y0\nEND",
            vec![
                (vec![("X0", true)], 4),
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 3),
            ],
        ),
        case(
            "reset_before_counter",
            "LD X1\nRST C0\nLD X0\nCNT C0 K3\nEND",
            vec![
                (vec![("X0", true), ("X1", true)], 1),
                (vec![("X1", false)], 1),
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 1),
            ],
        ),
        case(
            "reset_before_timer",
            "LD X1\nRST T0\nLD X0\nTMR T0 K3\nEND",
            vec![
                (vec![("X0", true), ("X1", true)], 2),
                (vec![("X1", false)], 4),
            ],
        ),
        case(
            "word_operands_and_aliases",
            "LD X0\nMOV HFFFF D0\nADD D0 K1 D0\nSUB D0 K1 D0\nMOV T0 D1\nMOV C0 D2\nEND",
            vec![(vec![("X0", true)], 2)],
        ),
        case(
            "motor_demo",
            &Project::default().source,
            vec![
                (vec![("X0", true)], 1),
                (vec![("X0", false)], 155),
                (vec![("X1", true)], 1),
                (vec![("X2", true)], 3),
                (vec![("X2", false)], 1),
                (vec![("X2", true)], 1),
                (vec![("X3", true)], 1),
            ],
        ),
    ];
    let mut depth = "LD X0\n".to_owned();
    for _ in 0..8 {
        depth.push_str("MPS\nAND X1\n");
    }
    depth.push_str("OUT Y0\n");
    for index in 1..=8 {
        depth.push_str(&format!("MPP\nOUT M{}\n", 2000 + index));
    }
    depth.push_str("END");
    cases.push(case(
        "branch_depth_eight",
        &depth,
        vec![
            (vec![("X0", true), ("X1", true)], 1),
            (vec![("X1", false)], 1),
            (vec![("X0", false)], 1),
        ],
    ));
    let mut sequential = String::new();
    for _ in 0..10 {
        sequential.push_str("LD X0\nMPS\nINC D0\nMPP\nINC D1\n");
    }
    sequential.push_str("END");
    cases.push(case(
        "branch_ten_sequential_pairs",
        &sequential,
        vec![(vec![("X0", true)], 2), (vec![("X0", false)], 1)],
    ));
    let comparison_start = cases.len();
    for (index, (a, b)) in [
        (-32768, -32768),
        (-32768, 32767),
        (-1, 0),
        (0, -1),
        (1, 1),
        (32767, -32768),
    ]
    .into_iter()
    .enumerate()
    {
        let mut source = format!("LD X0\nMOV K{a} D0\nMOV K{b} D1\nCMP D0 D1 M0\n");
        let mut output = 0;
        for prefix in ["LD", "AND", "OR"] {
            for relation in ["=", ">", "<", "<>", "<=", ">="] {
                if prefix != "LD" {
                    source.push_str("LD X1\n");
                }
                source.push_str(&format!("{prefix}{relation} D0 D1\nOUT Y{output:o}\n"));
                output += 1;
            }
        }
        source.push_str("END");
        cases.push(case(
            &format!("signed_comparison_contacts_{index}"),
            &source,
            vec![
                (vec![("X0", true), ("X1", true)], 1),
                (vec![("X0", false), ("X1", false)], 1),
            ],
        ));
    }
    cases.push(case("cmp_enabled_hold_and_cmpp_edges",
        "LD X1\nINC D0\nLD X0\nCMP D0 K2 M0\nLD X2\nCMPP D0 K2 M10\nLD M0\nOUT Y0\nLD M1\nOUT Y1\nLD M2\nOUT Y2\nEND",
        vec![(vec![("X0",true),("X1",true),("X2",true)],3),(vec![("X0",false),("X2",false)],1),(vec![("X0",true),("X2",true)],1)]));
    cases.push(case(
        "cmp_three_bit_destination_boundaries",
        "LD X0\nCMP K1 K2 Y375\nCMP H8000 K0 S1021\nCMP K7 K7 M4093\nEND",
        vec![(vec![("X0", true)], 1), (vec![("X0", false)], 1)],
    ));
    cases.push(case("comparison_inside_saved_branches_and_blocks",
        "LD>= D0 K0\nMPS\nAND< D0 K1\nOUT Y0\nMRD\nOR<> D0 K0\nOUT Y1\nMPP\nOUT Y2\nLD X0\nLD= D0 K0\nORB\nOUT Y3\nEND",
        vec![(vec![],1),(vec![("X0",true)],1)]));
    cases.push(case("comparison_timer_counter_and_extended_register_sources",
        "LD X0\nTMR T0 K2\nCNT C0 K2\nCMP T0 C0 M2000\nLD>= T0 K1\nOUT Y0\nLD= C0 K2\nOUT Y1\nLD X0\nAND<> T0 C0\nOUT Y2\nLD X0\nOR<= D4999 K0\nOUT Y3\nLD X1\nMOV K-2 D4999\nEND",
        vec![(vec![("X0",true)],3),(vec![("X0",false)],1),(vec![("X0",true)],2),(vec![("X1",true)],1)]));
    cases.rotate_left(comparison_start);
    let mut math = vec![
        case(
            "mul_signed_full_product",
            "LD X0\nMUL K-32768 K-32768 D0\nMUL K32767 K32767 D2\nMUL K-32768 K32767 D4\nMUL K-7 K3 D6\nMUL K0 K-32768 D8\nMUL K-1 K-1 D10\nEND",
            vec![(vec![("X0", true)], 2), (vec![("X0", false)], 1)],
        ),
        case(
            "div_signed_quotient_remainder",
            "LD X0\nDIV K-7 K3 D0\nDIV K7 K-3 D2\nDIV K-7 K-3 D4\nDIV K32767 K1 D6\nDIV K-32768 K1 D8\nDIV K-32768 K-1 D10\nDIV K0 K1 D12\nDIV K1 K32767 D14\nEND",
            vec![(vec![("X0", true)], 2), (vec![("X0", false)], 1)],
        ),
        case(
            "mul_div_source_destination_aliases",
            "LD X1\nMOV K-7 D0\nMOV K3 D1\nMOV K-123 D2\nMOV K-5 D3\nLD X0\nDIV D0 D1 D0\nMUL D2 D3 D2\nEND",
            vec![
                (vec![("X1", true)], 1),
                (vec![("X1", false), ("X0", true)], 2),
            ],
        ),
        case(
            "div_zero_preserves_previous_output",
            "LD X1\nMOV K123 D10\nMOV K-7 D11\nLD X0\nDIV K17 D0 D10\nEND",
            vec![
                (vec![("X1", true)], 1),
                (vec![("X1", false), ("X0", true)], 2),
            ],
        ),
        case(
            "mul_div_pulse_forms",
            "LD X1\nINC D0\nLD X0\nMULP D0 K3 D10\nDIVP D0 K2 D20\nEND",
            vec![
                (vec![("X0", true), ("X1", true)], 3),
                (vec![("X0", false)], 2),
                (vec![("X0", true)], 2),
            ],
        ),
    ];
    let maximum = if target == Target::Ss2 { 4998 } else { 9998 };
    math.push(case(
        "mul_div_extended_destination_pairs",
        &format!("LD X0\nMUL H8000 H8000 D{maximum}\nDIV K-123 K7 D4096\nEND"),
        vec![(vec![("X0", true)], 1), (vec![("X0", false)], 1)],
    ));
    math.extend(cases);
    let mut zones = Vec::new();
    for (index, (low, high)) in [
        (-5i16, 5i16),
        (7, -3),
        (-2, -2),
        (-32768, 32767),
        (32767, -32768),
    ]
    .into_iter()
    .enumerate()
    {
        let mut source = "LD X0\n".to_string();
        for (slot, value) in [-32768i16, -6, -5, -3, -2, 0, 5, 6, 7, 8, 32767]
            .into_iter()
            .enumerate()
        {
            source.push_str(&format!("ZCP K{low} K{high} K{value} M{}\n", slot * 3));
        }
        source.push_str("END");
        zones.push(case(
            &format!("zcp_signed_boundaries_{index}"),
            &source,
            vec![(vec![("X0", true)], 1), (vec![("X0", false)], 1)],
        ));
    }
    zones.push(case("zcp_hold_clear_and_zcpp_independent_edges",
        "LD X2\nSET M0\nSET M1\nSET M2\nSET M10\nSET M11\nSET M12\nLD X1\nINC D0\nLD X0\nZCP K2 K3 D0 M0\nZCPP K2 K3 D0 M10\nLD X3\nZCPP K2 K3 D0 M20\nEND",
        vec![(vec![("X2",true)],1), (vec![("X2",false),("X0",true),("X1",true)],3),
            (vec![("X0",false),("X3",true)],1), (vec![("X0",true)],2)]));
    zones.push(case("zcp_extended_y_m_s_destinations",
        "LD X1\nMOV K-2 D4999\nLD X0\nZCP K-2 K2 D4999 Y375\nZCPP H8000 H7FFF D4999 M4093\nZCP K3 K-3 D4999 S1021\nEND",
        vec![(vec![("X0",true),("X1",true)],2),(vec![("X0",false)],1)]));
    zones.push(case("zcp_timer_counter_sources_and_saved_branch",
        "LD X0\nMPS\nTMR T0 K2\nMRD\nCNT C0 K2\nMRD\nZCP C0 K3 T0 M2000\nMPP\nZCPP K0 T0 C0 M2010\nEND",
        vec![(vec![("X0",true)],4),(vec![("X0",false)],1),(vec![("X0",true)],3)]));
    zones.extend(math);
    let mut bulk = vec![
        case(
            "bulk_bmov_overlap_forward_cascade",
            "LD X1\nMOV K1 D0\nMOV K2 D1\nMOV K3 D2\nMOV K4 D3\nLD X0\nBMOV D0 D1 K3\nEND",
            vec![
                (vec![("X1", true)], 1),
                (vec![("X1", false), ("X0", true)], 1),
                (vec![("X0", false)], 1),
            ],
        ),
        case(
            "bulk_bmov_overlap_downward_and_self",
            "LD X1\nMOV K1 D0\nMOV K2 D1\nMOV K3 D2\nMOV K4 D3\nLD X0\nBMOV D1 D0 K3\nBMOV D0 D0 K4\nEND",
            vec![
                (vec![("X1", true)], 1),
                (vec![("X1", false), ("X0", true)], 1),
                (vec![("X0", false)], 1),
            ],
        ),
        case(
            "bulk_bmov_dynamic_count_and_alias",
            "LD X1\nINC D0\nLD X0\nBMOV D0 D10 D0\nBMOV D0 D0 D0\nEND",
            vec![
                (vec![("X0", true), ("X1", true)], 4),
                (vec![("X0", false)], 2),
                (vec![("X0", true)], 1),
            ],
        ),
        case(
            "bulk_bmov_invalid_counts_preserve",
            "LD X1\nMOV K-1 D0\nMOV K513 D1\nFMOV K77 D20 K3\nLD X0\nBMOV D10 D20 D0\nBMOV D10 D20 D1\nBMOV D10 D20 D2\nEND",
            vec![
                (vec![("X1", true)], 1),
                (vec![("X1", false), ("X0", true)], 2),
            ],
        ),
        case(
            "bulk_fill_snapshots_aliased_source_and_pulse",
            "LD X1\nINC D0\nLD X0\nFMOV D0 D0 K4\nFMOVP D0 D10 K4\nEND",
            vec![
                (vec![("X0", true), ("X1", true)], 3),
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 2),
            ],
        ),
        case(
            "bulk_exchange_self_alias_and_pulse",
            "LD X1\nINC D0\nLD X0\nXCH D0 D1\nXCH D1 D1\nXCHP D2 D0\nEND",
            vec![
                (vec![("X0", true), ("X1", true)], 3),
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 2),
            ],
        ),
        case(
            "bulk_bmovp_and_timer_counter_words",
            "LD X1\nINC D0\nLD X0\nBMOVP D0 D10 K3\nFMOV K123 T10 K2\nBMOV T10 C10 K2\nXCH T10 C11\nEND",
            vec![
                (vec![("X0", true), ("X1", true)], 3),
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 2),
            ],
        ),
        case(
            "bulk_bcd_bin_roundtrip_signed_packed_and_invalid",
            "LD X1\nMOV K9999 D0\nMOV K1234 D1\nMOV K-1 D2\nMOV HFA12 D3\nFMOV K77 D20 K4\nLD X0\nBCD D0 D10\nBIN D10 D11\nBCD D1 D12\nBIN D12 D13\nBCD D2 D20\nBIN D3 D21\nBCD D10 D22\nBIN D2 D23\nEND",
            vec![
                (vec![("X1", true)], 1),
                (vec![("X1", false), ("X0", true)], 2),
                (vec![("X0", false)], 1),
            ],
        ),
        case(
            "bulk_bcd_bin_pulse_and_inplace",
            "LD X1\nINC D0\nLD X0\nBCDP D0 D10\nBINP D10 D11\nBCD D0 D20\nBIN D20 D20\nEND",
            vec![
                (vec![("X0", true), ("X1", true)], 12),
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 2),
            ],
        ),
        case(
            "bulk_bmov_maximum_count",
            "LD X0\nFMOV K-123 D2000 K512\nBMOV D2000 D3000 K512\nEND",
            vec![(vec![("X0", true)], 1)],
        ),
    ];
    let base = if target == Target::Ss2 { 4996 } else { 9996 };
    bulk.push(case("bulk_last_register_and_dynamic_count",
        &format!("LD X1\nMOV K4 D0\nLD X0\nFMOV K-7 D{base} K4\nBMOV D{base} D10 D0\nXCH D{} D20\nBMOV D{} D21 K1\nEND",base+3,base+3),
        vec![(vec![("X0",true),("X1",true)],1),(vec![("X1",false)],1),(vec![("X0",false)],1)]));
    bulk.extend(zones);
    let mut wide = vec![
        case(
            "wide_move_constants_pairs_and_alias",
            "LD X0\nDMOV HFFFFFFFF D0\nDMOV H80000000 D2\nDMOV H7FFFFFFF D4\nDMOV K123456789 D6\nDMOV D6 D7\nDCML D2 D10\nEND",
            vec![(vec![("X0", true)], 1), (vec![("X0", false)], 1)],
        ),
        case(
            "wide_signed_arithmetic_overflow",
            "LD X0\nDADD H7FFFFFFF K1 D0\nDADD H80000000 K-1 D2\nDADD K-100 K-200 D4\nDSUB H80000000 K1 D6\nDSUB H7FFFFFFF K-1 D8\nDSUB K0 H80000000 D10\nDADD H80000000 H80000000 D12\nEND",
            vec![(vec![("X0", true)], 1), (vec![("X0", false)], 1)],
        ),
        case(
            "wide_multiply_64bit_and_source_alias",
            "LD X0\nDMUL H80000000 H80000000 D0\nDMUL H7FFFFFFF H7FFFFFFF D4\nDMUL H80000000 H7FFFFFFF D8\nDMOV K-123456 D12\nDMUL D12 K321 D12\nEND",
            vec![(vec![("X0", true)], 1), (vec![("X0", false)], 1)],
        ),
        case(
            "wide_divide_signed_quotient_remainder_and_zero",
            "LD X0\nDDIV K-123456789 K7 D0\nDDIV K123456789 K-7 D4\nDDIV K-123456789 K-7 D8\nDDIV H80000000 K1 D12\nDMOV K77 D16\nDMOV K-88 D18\nDDIV K100 K0 D16\nEND",
            vec![(vec![("X0", true)], 1), (vec![("X0", false)], 1)],
        ),
        case(
            "wide_logic_unary_wrap_and_pulse",
            "LD X1\nDMOV H7FFFFFFF D0\nDMOV H80000000 D2\nDMOV H80000000 D4\nLD X0\nDINC D0\nDDEC D2\nDNEG D4\nDAND H12345678 HFFFF0000 D6\nDOR H80000000 K1 D8\nDXOR HFFFFFFFF H7FFFFFFF D10\nEND",
            vec![
                (vec![("X1", true)], 1),
                (vec![("X1", false), ("X0", true)], 2),
                (vec![("X0", false)], 1),
            ],
        ),
        case(
            "wide_fill_exchange_conversion",
            "LD X0\nDFMOV K12345678 D0 K3\nDXCH D0 D10\nDBCD D10 D12\nDBIN D12 D14\nDMOV H99999999 D16\nDBIN D16 D18\nDMOV K-1 D20\nDFMOV K77 D22 K2\nDBCD D20 D22\nDBIN D20 D24\nEND",
            vec![(vec![("X0", true)], 1), (vec![("X0", false)], 1)],
        ),
        case(
            "wide_cmp_zone_signed_and_reversed",
            "LD X1\nDINC D0\nLD X0\nDCMP D0 K2 M0\nDZCP K2 K3 D0 M10\nDZCP K3 K-3 D0 M20\nLD X2\nDCMPP D0 K2 M30\nDZCPP K2 K3 D0 M40\nEND",
            vec![
                (vec![("X0", true), ("X1", true), ("X2", true)], 4),
                (vec![("X0", false), ("X2", false)], 1),
                (vec![("X0", true), ("X2", true)], 1),
            ],
        ),
        case(
            "wide_timer_counter_pairs_and_native_counters",
            "LD X0\nDMOV K12345678 T10\nDMOV K-12345678 C10\nDMOV H87654321 C200\nDMOV K2147483647 C201\nDMOV C200 D0\nDMOV C201 D2\nDMOV T10 D4\nDMOV C10 D6\nDADD C200 K1 C202\nEND",
            vec![(vec![("X0", true)], 1), (vec![("X0", false)], 1)],
        ),
        case(
            "wide_native_counter_c255_contact_boundary",
            "LD X0\nDMOV K12345678 C255\nDLD= C255 K12345678\nOUT Y0\nEND",
            vec![(vec![("X0", true)], 1), (vec![("X0", false)], 1)],
        ),
    ];
    let names = [
        "DMOV", "DCML", "DFMOV", "DXCH", "DBCD", "DBIN", "DADD", "DSUB", "DMUL", "DDIV", "DINC",
        "DDEC", "DAND", "DOR", "DXOR", "DNEG",
    ];
    for (index, name) in names.into_iter().enumerate() {
        let base = 10;
        let args = match name {
            "DMOV" | "DCML" | "DBCD" | "DBIN" | "DXCH" => format!("D0 D{base}"),
            "DFMOV" => format!("D0 D{base} K3"),
            "DINC" | "DDEC" | "DNEG" => format!("D{base}"),
            _ => format!("D0 K3 D{base}"),
        };
        wide.push(case(
            &format!("wide_pulse_{index}_{name}"),
            &format!("LD X1\nDINC D0\nLD X0\n{name}P {args}\nEND"),
            vec![
                (vec![("X0", true), ("X1", true)], 3),
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 2),
            ],
        ));
    }
    for (index, (a, b)) in [
        (-2147483648i32, 2147483647i32),
        (2147483647, -2147483648),
        (-1, 0),
        (0, -1),
        (12345678, 12345678),
    ]
    .into_iter()
    .enumerate()
    {
        let mut source = format!("LD X0\nDMOV K{a} D0\nDMOV K{b} D2\n");
        let mut output = 0;
        for prefix in ["DLD", "DAND", "DOR"] {
            for relation in ["=", ">", "<", "<>", "<=", ">="] {
                if prefix != "DLD" {
                    source.push_str("LD X1\n");
                }
                source.push_str(&format!("{prefix}{relation} D0 D2\nOUT Y{output:o}\n"));
                output += 1;
            }
        }
        source.push_str("END");
        wide.push(case(
            &format!("wide_signed_comparison_contacts_{index}"),
            &source,
            vec![
                (vec![("X0", true), ("X1", true)], 1),
                (vec![("X0", false), ("X1", false)], 1),
            ],
        ));
    }
    let maximum = if target == Target::Ss2 { 4996 } else { 9996 };
    wide.push(case(
        "wide_implicit_four_word_destination_boundary",
        &format!(
            "LD X0\nDMUL H7FFFFFFF K7 D{maximum}\nDMOV D{} D0\nEND",
            maximum + 2
        ),
        vec![(vec![("X0", true)], 1)],
    ));
    wide.push(case(
        "wide_fill_maximum_512_double_words",
        "LD X0\nDFMOV H87654321 D2000 K512\nEND",
        vec![(vec![("X0", true)], 1)],
    ));
    wide.push(case("wide_native_counter_fill_exchange_multiply_reset",
        "LD X0\nDFMOV K12345678 C200 K3\nDXCH C200 C203\nDMUL K-123456 K321 C204\nDDIV K-123456789 K7 C206\nLD X1\nRST C200\nRST C204\nEND",
        vec![(vec![("X0",true)],1),(vec![("X0",false),("X1",true)],1)]));
    wide.push(case("wide_counter_c199_zero_extend_and_truncate",
        "LD X0\nFMOV K-1 C199 K1\nDMOV C199 D0\nDLD= C199 K65535\nOUT Y0\nLD X0\nDMOV H87654321 C199\nDMOV C199 D2\nEND",
        vec![(vec![("X0",true)],1),(vec![("X0",false)],1)]));
    wide.extend(bulk);
    let mut shifts = Vec::new();
    for (index, body) in [
        "ADD K32767 K1 D0",
        "SUB H8000 K1 D0",
        "MUL K7 K3 D0",
        "DIV K-7 K3 D0",
        "DADD H7FFFFFFF K1 D0",
        "DSUB H80000000 K1 D0",
        "DMUL H7FFFFFFF K2 D0",
        "DDIV K-7 K3 D0",
        "INC D0\nNEG D0\nWAND HFFFF K1 D1",
    ]
    .into_iter()
    .enumerate()
    {
        shifts.push(case(
            &format!("shift_arithmetic_flags_{index}"),
            &format!("LD X1\nOUT M1020\nOUT M1021\nOUT M1022\nLD X0\n{body}\nEND"),
            vec![
                (vec![("X1", true)], 1),
                (vec![("X1", false), ("X0", true)], 1),
                (vec![("X0", false)], 1),
                (vec![("X1", true), ("X0", true)], 1),
            ],
        ));
    }
    shifts.push(case("shift_error_flags_independent_sticky_and_reset",
        "LD X2\nRST M1067\nLD X3\nRST M1068\nLD X0\nMOV K-1 D0\nBCD D0 D1\nLD X1\nDIV K7 K0 D2\nLDI X0\nMOV D1069 D20\nEND",
        vec![(vec![("X0",true)],1),(vec![("X0",false),("X1",true)],1),(vec![("X2",true)],1),(vec![("X2",false),("X3",true)],1),(vec![("X1",false)],1)]));
    shifts.push(case("shift_swap_system_control_16_32",
        "LD X2\nOUT M1303\nLD X1\nMOV H1234 D0\nDMOV H12345678 D2\nLD X0\nXCH D0 D0\nDXCH D2 D2\nEND",
        vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],1),(vec![("X2",true)],2),(vec![("X0",false)],1)]));
    for name in ["ROR", "ROL", "RCR", "RCL", "DROR", "DROL", "DRCR", "DRCL"] {
        for pulse in [false, true] {
            let wide = name.starts_with('D');
            let initial = if wide {
                "DMOV H87654321 D0"
            } else {
                "MOV H8123 D0"
            };
            let n = if wide { 17 } else { 7 };
            shifts.push(case(&format!("shift_rotate_{name}{}",if pulse{"P"}else{""}),&format!("LD X1\n{initial}\nSET M1022\nSET M1020\nSET M1021\nLD X0\n{name}{} D0 K{n}\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nEND",if pulse{"P"}else{""}),vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],3),(vec![("X0",false)],1),(vec![("X0",true)],1)]));
        }
    }
    for name in ["ROR", "ROL", "RCR", "RCL", "DROR", "DROL", "DRCR", "DRCL"] {
        let wide = name.starts_with('D');
        let initial = if wide {
            "DMOV H87654321 D0"
        } else {
            "MOV H8123 D0"
        };
        shifts.push(case(
            &format!("shift_full_width_{name}"),
            &format!(
                "LD X0\n{initial}\n{name} D0 K{}\nEND",
                if wide { 32 } else { 16 }
            ),
            vec![(vec![("X0", true)], 1)],
        ));
    }
    for name in ["SFTR", "SFTL", "WSFR", "WSFL"] {
        for offset in [0, 2] {
            for n in [1, 2, 3, 5] {
                let words = name.starts_with('W');
                let mut source = "LD X1\n".to_string();
                if words {
                    for i in 0..8 {
                        source.push_str(&format!("MOV K{} D{i}\n", 10 * (i + 1)));
                    }
                } else {
                    for i in [0, 2, 4, 6] {
                        source.push_str(&format!("SET M{i}\n"));
                    }
                }
                source.push_str(&format!(
                    "LD X0\n{name} {}{offset} {}0 K5 K{n}\nEND",
                    if words { "D" } else { "M" },
                    if words { "D" } else { "M" }
                ));
                shifts.push(case(
                    &format!("shift_ranges_{name}_{offset}_{n}"),
                    &source,
                    vec![
                        (vec![("X1", true)], 1),
                        (vec![("X1", false), ("X0", true)], 2),
                        (vec![("X0", false)], 1),
                    ],
                ));
            }
        }
    }
    for name in ["SFTRP", "SFTLP", "WSFRP", "WSFLP"] {
        let words = name.starts_with('W');
        let init = if words {
            "FMOV K17 D0 K7"
        } else {
            "SET M0\nSET M2\nSET M4"
        };
        shifts.push(case(
            &format!("shift_pulse_ranges_{name}"),
            &format!(
                "LD X1\n{init}\nLD X0\n{name} {}0 {}0 K5 K2\nEND",
                if words { "D" } else { "M" },
                if words { "D" } else { "M" }
            ),
            vec![
                (vec![("X1", true)], 1),
                (vec![("X1", false), ("X0", true)], 3),
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 1),
            ],
        ));
    }
    shifts.push(case(
        "shift_fifo_full_empty_zero_and_carry",
        "LD X2\nINC D0\nLD X0\nSFWR D0 D10 K4\nLD X1\nSFRD D10 D20 K4\nEND",
        vec![
            (vec![("X1", true)], 1),
            (vec![("X1", false), ("X0", true), ("X2", true)], 4),
            (vec![("X0", false), ("X1", true)], 4),
        ],
    ));
    for dest in ["D10", "D11", "D12", "D20"] {
        shifts.push(case(&format!("shift_fifo_pop_alias_{dest}"),&format!("LD X0\nSFWR K11 D10 K4\nSFWR K22 D10 K4\nSFWR K33 D10 K4\nLD X1\nSFRD D10 {dest} K4\nEND"),vec![(vec![("X0",true)],1),(vec![("X0",false),("X1",true)],2)]));
    }
    shifts.push(case(
        "shift_fifo_pulse_forms_and_source_pointer_alias",
        "LD X0\nSFWRP D10 D10 K4\nLD X1\nSFRDP D10 D20 K4\nEND",
        vec![
            (vec![("X0", true)], 3),
            (vec![("X0", false)], 1),
            (vec![("X0", true)], 2),
            (vec![("X1", true)], 2),
        ],
    ));
    shifts.push(case("shift_smOV_decimal_hex_and_alias","LD X2\nOUT M1168\nLD X1\nMOV K1234 D0\nMOV K5678 D1\nLD X0\nSMOV D0 K4 K2 D1 K3\nSMOVP D0 K3 K2 D0 K2\nEND",vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2),(vec![("X0",false),("X2",true)],1),(vec![("X0",true)],2)]));
    shifts.push(case("shift_zrst_bits_words_reversed_and_timers",
        "LD X1\nSET Y0\nSET Y1\nSET Y2\nFMOV K17 D0 K4\nDMOV K1234567 C200\nLD X0\nTMR T0 K5\nCNT C0 K3\nZRST Y2 Y1\nZRST D0 D2\nZRST T0 T0\nZRST C0 C0\nZRST C200 C200\nZRSTP M0 M3\nEND",
        vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2),(vec![("X0",false)],1)]));
    shifts.push(case(
        "shift_inv_and_pulse_coils_save_condition",
        "LD X0\nINV\nINV\nPLS Y0\nOUT Y1\nPLF M0\nOUT Y2\nLD X1\nINV\nOUT Y3\nEND",
        vec![
            (vec![], 1),
            (vec![("X0", true)], 3),
            (vec![("X0", false), ("X1", true)], 2),
            (vec![("X0", true)], 1),
        ],
    ));
    shifts.push(case("shift_multiply_after_error_carry_and_wide_preservation",
        "LD X1\nDIV K1 K0 D10\nLD X0\nMUL K32767 K32767 D0\nLD M1022\nOUT Y0\nLD X0\nSET M1022\nDMUL H7FFFFFFF K2 D2\nLD M1022\nOUT Y1\nEND",
        vec![(vec![("X0",true)],1),(vec![("X1",true)],1),(vec![("X1",false)],1)]));
    shifts.push(case(
        "shift_smOV_invalid_decimal_error_pc_and_hex_override",
        "LD X2\nOUT M1168\nLD X1\nMOV HFA12 D0\nMOV K5678 D1\nLD X0\nSMOV D0 K4 K2 D1 K3\nEND",
        vec![
            (vec![("X1", true)], 1),
            (vec![("X1", false), ("X0", true)], 1),
            (vec![("X2", true)], 1),
        ],
    ));
    for instruction in ["SFWR K7 D10 K4", "SFRD D10 D20 K4"] {
        shifts.push(case(
            &format!(
                "shift_fifo_negative_{}",
                instruction.split_whitespace().next().unwrap()
            ),
            &format!("LD X1\nMOV K-1 D10\nSET M1022\nLD X0\n{instruction}\nEND"),
            vec![
                (vec![("X1", true)], 1),
                (vec![("X1", false), ("X0", true)], 1),
            ],
        ));
    }
    shifts.push(case(
        "shift_zrst_before_counter_does_not_inhibit_edge",
        "LD X1\nZRST C0 C0\nLD X0\nCNT C0 K3\nEND",
        vec![
            (vec![("X0", true), ("X1", true)], 1),
            (vec![("X1", false)], 1),
            (vec![("X0", false)], 1),
            (vec![("X0", true), ("X1", true)], 1),
        ],
    ));
    shifts.push(case(
        "shift_bit_boundaries_y_s_and_input_source",
        "LD X2\nSET S1022\nSET S1023\nLD X3\nSFTR X0 Y375 K3 K2\nSFTL S1022 S1021 K3 K2\nEND",
        vec![
            (vec![("X2", true), ("X0", true), ("X3", true)], 1),
            (vec![("X2", false), ("X1", true)], 1),
        ],
    ));
    shifts.push(case("shift_bit_maximum_1024",
        "LD X0\nSET M3024\nSET M3535\nSET M1022\nSFTR M3024 M2000 K1024 K512\nLD M1022\nOUT Y0\nEND",vec![(vec![("X0",true)],1)]));
    shifts.push(case(
        "shift_word_maximum_512",
        "LD X0\nFMOV K-17 D3000 K256\nWSFR D3000 D2000 K512 K256\nEND",
        vec![(vec![("X0", true)], 1)],
    ));
    let end = if target == Target::Ss2 { 4995 } else { 9995 };
    shifts.push(case("shift_last_model_words",
        &format!("LD X1\nFMOV K77 D{end} K5\nMOV K11 D0\nMOV K22 D1\nLD X0\nWSFR D0 D{end} K5 K2\nWSFLP D0 D{end} K5 K2\nEND"),vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2)]));
    shifts.extend(float_foundation_cases(target));
    shifts.extend(float_math_cases(target));
    shifts.extend(float_math_extended_cases(target));
    shifts.extend(float_advanced_cases(target));
    shifts.extend(float_advanced_precision_cases(target));
    shifts.extend(text_encoding_cases(target));
    shifts.extend(bit_tool_cases(target));
    shifts.extend(data_processing_cases(target));
    shifts.extend(additional_processing_cases(target));
    shifts.extend(search_value_cases::cases(target));
    shifts.extend(search_state_cases::cases(target));
    shifts.extend(data_transform_cases::cases(target));
    shifts.extend(mmov_cases::cases(target));
    shifts.extend(solar_cases::cases(target));
    shifts.extend(calendar_cases::cases(target));
    shifts.extend(drum_cases::cases(target));
    shifts.extend(matrix_cases::cases(target));
    shifts.extend(matrix_counter_cases::cases(target));
    shifts.extend(matrix_input_cases::cases());
    shifts.extend(matrix_bulk_cases::cases(target));
    shifts.extend(program_control_cases::cases(target));
    shifts.extend(program_control_mixed_cases::cases(target));
    shifts.extend(prun_cases::cases(target));
    shifts.extend(special_timer_cases::cases(target));
    shifts.extend(sfc_cases::cases(target));
    shifts.extend(sfc_extra_cases::cases(target));
    shifts.extend(st_cases::cases(target));
    shifts.extend(dcnt_cases::cases(target));
    shifts.extend(reff_cases::cases(target));
    shifts.extend(system_clock_cases::cases(target));
    shifts.extend(alarm_cases::cases(target));
    shifts.push(case(
        "program_control_demo",
        include_str!("../../examples/program-control.il"),
        vec![
            (vec![("X1", true), ("X0", false)], 1),
            (vec![("X1", false), ("X0", true)], 2),
            (vec![("X2", true)], 1),
            (vec![("X0", false)], 1),
            (vec![("X0", true)], 1),
        ],
    ));
    shifts.push(case(
        "program_control_signed_group_negative_once",
        &format!(
            "LDI X0\n{}FOR K4M64\nINC D0\nNEXT\nEND",
            (64..80).map(|n| format!("SET M{n}\n")).collect::<String>()
        ),
        vec![(vec![], 1)],
    ));
    shifts.push(case(
        "program_control_limit_FOR_signed32767_one_scan",
        "LDI X0\nFOR K32767\nINC D0\nNEXT\nMOV D0 D1\nEND",
        vec![(vec![], 1)],
    ));
    shifts.extend(numeric_contact_cases(target));
    shifts.extend(arithmetic_operand_cases::cases());
    shifts.extend(move_logic_operand_cases::cases());
    shifts.extend(comparison_operand_cases::cases());
    shifts.extend(bcd_operand_cases::cases());
    shifts.extend(exchange_operand_cases::cases());
    shifts.extend(block_operand_cases::cases());
    shifts.extend(bit_operand_cases::cases());
    shifts.extend(multiply_operand_cases::cases());
    shifts.extend(multiply_operand_cases::diagnostic_cases());
    shifts.extend(shift_operand_cases::cases());
    shifts.extend(data_operand_cases::cases());
    shifts.extend(additional_operand_cases::cases());
    shifts.extend(cmpt_profile_cases::cases(target));
    shifts.extend(search_operand_cases::cases());
    shifts.extend(float_move_operand_cases::cases());
    shifts.extend(text_operand_cases::cases());
    shifts.extend(indexed_scalar_cases::cases());
    shifts.extend(indexed_multiply_cases::cases());
    shifts.extend(indexed_truncated_cases::cases());
    shifts.extend(m1019_cases::cases());
    shifts.extend(indexed_bcd_cases::cases());
    shifts.extend(indexed_float_cases::cases());
    shifts.extend(indexed_float_cases::model_cases(target));
    shifts.extend(indexed_root_cases::cases());
    shifts.extend(indexed_root_cases::model_cases(target));
    shifts.extend(indexed_angle_cases::cases());
    shifts.extend(indexed_angle_cases::model_cases(target));
    shifts.extend(indexed_decimal_conversion_cases::cases());
    shifts.extend(indexed_decimal_conversion_cases::model_cases(target));
    shifts.extend(indexed_exp_ln_cases::cases());
    shifts.extend(indexed_exp_ln_cases::model_cases(target));
    shifts.extend(indexed_log_pow_cases::cases());
    shifts.extend(indexed_log_pow_cases::model_cases(target));
    shifts.extend(indexed_desqr_cases::cases());
    shifts.extend(indexed_desqr_cases::model_cases(target));
    shifts.extend(indexed_trig_cases::cases());
    shifts.extend(indexed_trig_cases::model_cases(target));
    shifts.extend(indexed_float_comparison_cases::cases());
    shifts.extend(indexed_float_comparison_cases::model_cases(target));
    shifts.extend(indexed_literal_move_cases::cases());
    shifts.extend(indexed_literal_move_cases::model_cases(target));
    shifts.extend(indexed_swap_mask_cases::common_cases());
    shifts.extend(indexed_swap_mask_cases::model_cases(target));
    shifts.extend(indexed_random_cases::common_cases());
    shifts.extend(indexed_random_cases::model_cases(target));
    shifts.extend(indexed_clock_read_cases::cases());
    shifts.extend(indexed_bit_number_cases::common_cases());
    shifts.extend(indexed_bit_number_cases::model_cases(target));
    shifts.extend(indexed_abs_gray_cases::common_cases());
    shifts.extend(indexed_abs_gray_cases::model_cases(target));
    shifts.extend(indexed_calendar_time_cases::common_cases());
    shifts.extend(indexed_calendar_time_cases::model_cases(target));
    shifts.extend(indexed_rtc_cases::native_cases(target));
    shifts.extend(indexed_segment_cases::common_cases());
    shifts.extend(indexed_segment_cases::model_cases(target));
    shifts.extend(indexed_text_cases::common_cases());
    shifts.extend(indexed_text_cases::model_cases(target));
    shifts.extend(indexed_checksum_cases::common_cases());
    shifts.extend(indexed_checksum_cases::model_cases(target));
    shifts.extend(indexed_search_toggle_cases::common_cases());
    shifts.extend(indexed_search_toggle_cases::model_cases(target));
    shifts.extend(indexed_data_processing_cases::common_cases());
    shifts.extend(indexed_data_processing_cases::model_cases(target));
    shifts.extend(indexed_shift_fifo_cases::common_cases());
    shifts.extend(indexed_shift_fifo_cases::model_cases(target));
    shifts.extend(indexed_block_exchange_cases::common_cases());
    shifts.extend(indexed_block_exchange_cases::model_cases(target));
    shifts.extend(indexed_rotation_cases::common_cases());
    shifts.extend(indexed_rotation_cases::model_cases(target));
    shifts.extend(indexed_real_cases::cases());
    shifts.extend(indexed_real_cases::model_cases(target));
    shifts.extend(indexed_float_binary_cases::cases());
    shifts.extend(indexed_float_binary_cases::model_cases(target));
    shifts.extend(decimal_scratch_profile_cases::cases(target));
    shifts.extend(indexed_comparison_cases::cases());
    shifts.extend(indexed_bit_cases::cases());
    shifts.extend(reset_counter_operand_cases::cases());
    shifts.extend(dcnt_reset_cases::cases());
    shifts.extend(bit_target_width_cases::cases());
    shifts.extend(truncated_cases(target));
    shifts.extend(wide);
    shifts
}

fn float_math_extended_cases(target: Target) -> Vec<Case> {
    let mut cases = vec![
        case(
            "float_math_hard_div_constant_min_positive",
            "LDI X0\nSET M1020\nSET M1021\nSET M1022\nDMOV H12345678 D10\nDEDIV K1 H80000000 D10\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLDI X0\nMOV D1067 D20\nMOV D1068 D21\nEND",
            vec![(vec![], 1)],
        ),
        case(
            "float_math_hard_div_constant_min_negative",
            "LDI X0\nSET M1020\nSET M1021\nSET M1022\nDMOV H12345678 D10\nDEDIV K-1 K-2147483648 D10\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLDI X0\nMOV D1067 D20\nMOV D1068 D21\nEND",
            vec![(vec![], 1)],
        ),
        case(
            "float_math_hard_div_constant_min_zero",
            "LDI X0\nSET M1020\nSET M1021\nSET M1022\nDMOV H12345678 D10\nDEDIV K0 H80000000 D10\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLDI X0\nMOV D1067 D20\nMOV D1068 D21\nEND",
            vec![(vec![], 1)],
        ),
        case(
            "float_math_hard_div_constant_min_negative_zero",
            "LDI X0\nSET M1020\nSET M1021\nSET M1022\nDMOV H12345678 D10\nDMOV H80000000 D0\nDEDIV D0 H80000000 D10\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLDI X0\nMOV D1067 D20\nMOV D1068 D21\nEND",
            vec![(vec![], 1)],
        ),
        case(
            "float_math_hard_div_constant_min_min",
            "LDI X0\nSET M1020\nSET M1021\nSET M1022\nDMOV H12345678 D10\nDEDIV H80000000 H80000000 D10\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLDI X0\nMOV D1067 D20\nMOV D1068 D21\nEND",
            vec![(vec![], 1)],
        ),
        case(
            "float_math_hard_div_constant_min_register_same_numeric",
            "LDI X0\nSET M1020\nSET M1021\nSET M1022\nDMOV H12345678 D10\nDMOV HCF000000 D2\nDEDIV K1 D2 D10\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLDI X0\nMOV D1067 D20\nMOV D1068 D21\nEND",
            vec![(vec![], 1)],
        ),
        case(
            "float_math_hard_div_nan_positive_zero_priority",
            "LDI X0\nSET M1020\nSET M1021\nSET M1022\nDMOV H12345678 D10\nDMOV H7FC12345 D0\nDMOV H00000000 D2\nDEDIV D0 D2 D10\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLDI X0\nMOV D1067 D20\nMOV D1068 D21\nEND",
            vec![(vec![], 1)],
        ),
        case(
            "float_math_hard_div_nan_negative_zero_priority",
            "LDI X0\nSET M1020\nSET M1021\nSET M1022\nDMOV H12345678 D10\nDMOV H7FC12345 D0\nDMOV H80000000 D2\nDEDIV D0 D2 D10\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLDI X0\nMOV D1067 D20\nMOV D1068 D21\nEND",
            vec![(vec![], 1)],
        ),
        case(
            "float_math_hard_div_nan_subnormal_priority",
            "LDI X0\nSET M1020\nSET M1021\nSET M1022\nDMOV H12345678 D10\nDMOV HFFC12345 D0\nDMOV H00000001 D2\nDEDIV D0 D2 D10\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLDI X0\nMOV D1067 D20\nMOV D1068 D21\nEND",
            vec![(vec![], 1)],
        ),
        case(
            "float_math_hard_div_positive_min_subnormal",
            "LDI X0\nSET M1020\nSET M1021\nSET M1022\nDMOV H12345678 D10\nDMOV H00000001 D2\nDEDIV K1 D2 D10\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLDI X0\nMOV D1067 D20\nMOV D1068 D21\nEND",
            vec![(vec![], 1)],
        ),
        case(
            "float_math_hard_div_negative_max_subnormal",
            "LDI X0\nSET M1020\nSET M1021\nSET M1022\nDMOV H12345678 D10\nDMOV H807FFFFF D2\nDEDIV K-1 D2 D10\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLDI X0\nMOV D1067 D20\nMOV D1068 D21\nEND",
            vec![(vec![], 1)],
        ),
        case(
            "float_math_hard_div_min_normal_denominator",
            "LDI X0\nSET M1020\nSET M1021\nSET M1022\nDMOV H12345678 D10\nDMOV H00800000 D2\nDEDIV K1 D2 D10\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLDI X0\nMOV D1067 D20\nMOV D1068 D21\nEND",
            vec![(vec![], 1)],
        ),
        case(
            "float_math_hard_div_infinity_infinity_and_cancel",
            "LDI X0\nSET M1020\nSET M1021\nSET M1022\nDMOV H12345678 D10\nDMOV H7F800000 D0\nDMOV HFF800000 D2\nDEDIV D0 D0 D10\nDEADD D0 D2 D12\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLDI X0\nMOV D1067 D20\nMOV D1068 D21\nEND",
            vec![(vec![], 1)],
        ),
        case(
            "float_math_hard_div_negative_finite_positive_infinity",
            "LDI X0\nSET M1020\nSET M1021\nSET M1022\nDMOV H12345678 D10\nDMOV H7F800000 D2\nDEDIV K-1 D2 D10\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLDI X0\nMOV D1067 D20\nMOV D1068 D21\nEND",
            vec![(vec![], 1)],
        ),
        case(
            "float_math_hard_div_max_finite_overflow",
            "LDI X0\nSET M1020\nSET M1021\nSET M1022\nDMOV H12345678 D10\nDMOV H7F7FFFFF D0\nDMOV H00800000 D2\nDEDIV D0 D2 D10\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLDI X0\nMOV D1067 D20\nMOV D1068 D21\nEND",
            vec![(vec![], 1)],
        ),
        case(
            "float_math_hard_div_underflow_to_zero",
            "LDI X0\nSET M1020\nSET M1021\nSET M1022\nDMOV H12345678 D10\nDMOV H80000001 D0\nDEDIV D0 K2 D10\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLDI X0\nMOV D1067 D20\nMOV D1068 D21\nEND",
            vec![(vec![], 1)],
        ),
        case(
            "float_math_hard_div_subnormal_result_preserved",
            "LDI X0\nSET M1020\nSET M1021\nSET M1022\nDMOV H12345678 D10\nDMOV H007FFFFF D0\nDEDIV D0 K1 D10\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLDI X0\nMOV D1067 D20\nMOV D1068 D21\nEND",
            vec![(vec![], 1)],
        ),
        case(
            "float_math_hard_div_nan_finite_error",
            "LDI X0\nSET M1020\nSET M1021\nSET M1022\nDMOV H12345678 D10\nDMOV H7FC12345 D0\nDEDIV D0 K1 D10\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLDI X0\nMOV D1067 D20\nMOV D1068 D21\nEND",
            vec![(vec![], 1)],
        ),
        case(
            "float_math_hard_add_nan_payload_negative_larger",
            "LDI X0\nSET M1020\nSET M1021\nSET M1022\nDMOV H12345678 D10\nDMOV H7FC00001 D0\nDMOV HFFC00002 D2\nDEADD D0 D2 D10\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLDI X0\nMOV D1067 D20\nMOV D1068 D21\nEND",
            vec![(vec![], 1)],
        ),
        case(
            "float_math_hard_add_nan_payload_positive_larger",
            "LDI X0\nSET M1020\nSET M1021\nSET M1022\nDMOV H12345678 D10\nDMOV HFFC00001 D0\nDMOV H7FC00002 D2\nDEADD D0 D2 D10\nLD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLDI X0\nMOV D1067 D20\nMOV D1068 D21\nEND",
            vec![(vec![], 1)],
        ),
    ];
    for exponent in -41..=35 {
        let mut source = String::new();
        for (i, coefficient) in [
            -9999i16, -3402, -1176, -314, -1, 0, 1, 314, 1176, 3402, 9999,
        ]
        .into_iter()
        .enumerate()
        {
            source.push_str(&format!("LD X0\nMOV K{coefficient} D0\nMOV K{exponent} D1\nDEBIN D0 D{}\nLD M1020\nOUT M{}\nLD M1021\nOUT M{}\nLD M1022\nOUT M{}\n", 20+i*2, 200+i*3, 201+i*3, 202+i*3));
        }
        source.push_str("END");
        cases.push(case(
            &format!("float_math_decimal_exponent_{exponent}"),
            &source,
            vec![(vec![("X0", true)], 1)],
        ));
    }
    for exponent in -37..=37 {
        let v = 10.0f32.powi(exponent);
        let mut source = "LD X1\nMOV K77 D50\nMOV K88 D51\n".to_owned();
        for (i, bits) in [
            v.to_bits() - 1,
            v.to_bits(),
            v.to_bits() + 1,
            (-v).to_bits() - 1,
            (-v).to_bits(),
            (-v).to_bits() + 1,
        ]
        .into_iter()
        .enumerate()
        {
            source.push_str(&format!("LD X0\nDMOV H{bits:08X} D0\nDEBCD D0 D{}\nLD M1020\nOUT M{}\nLD M1021\nOUT M{}\nLD M1022\nOUT M{}\n", 20+i*2, 200+i*3, 201+i*3, 202+i*3));
        }
        source.push_str("END");
        cases.push(case(
            &format!("float_math_decimal_power_neighbors_{exponent}"),
            &source,
            vec![(vec![("X0", true)], 1)],
        ));
    }
    if target == Target::Es2 {
        for dest in ["D4999", "D5000", "D5001"] {
            cases.push(case(
                &format!("float_math_decimal_scratch_alias_{dest}"),
                &format!("LD X0\nDMOV H3F800000 D0\nDEBCD D0 {dest}\nEND"),
                vec![(vec![("X0", true)], 1)],
            ));
        }
    }
    for op in ["DEADD", "DESUB", "DEMUL"] {
        for (i, (left, right)) in [
            (0x7fc00001u32, 0xffc00002u32),
            (0x7fc00002, 0xffc00001),
            (0xffc00001, 0x7fc00002),
            (0xffc00002, 0x7fc00001),
            (0xffc00001, 0x7fc00001),
            (0x7fc00001, 0xffc00001),
            (0x7f800001, 0xffc00002),
            (0xff800002, 0x7fc00001),
            (0x7fc00001, 0xff800002),
            (0xffc00001, 0x7f800002),
            (0xff800002, 0x7f800001),
            (0x7f800001, 0xff800002),
        ]
        .into_iter()
        .enumerate()
        {
            cases.push(case(
                &format!("float_math_nan_selection_{op}_{i}"),
                &format!("LD X0\nDMOV H{left:08X} D0\nDMOV H{right:08X} D2\n{op} D0 D2 D20\nEND"),
                vec![(vec![("X0", true)], 1)],
            ));
        }
    }
    cases
}

fn float_advanced_precision_cases(_target: Target) -> Vec<Case> {
    struct Probe {
        op: String,
        left: u32,
        right: Option<u32>,
        degrees: bool,
        literals: u8,
    }
    impl Probe {
        fn unary(op: &str, value: f32) -> Self {
            Self {
                op: op.into(),
                left: value.to_bits(),
                right: None,
                degrees: false,
                literals: 0,
            }
        }
        fn binary(op: &str, left: f32, right: f32) -> Self {
            Self {
                op: op.into(),
                left: left.to_bits(),
                right: Some(right.to_bits()),
                degrees: false,
                literals: 0,
            }
        }
    }
    fn batch(name: &str, probes: Vec<Probe>, flags: bool, pulse: bool) -> Case {
        assert_eq!(probes.len(), 8);
        let flag = if flags { "SET" } else { "RST" };
        let mut source = format!("LD X1\n{flag} M1020\n{flag} M1021\n{flag} M1022\nRST M1081\n");
        for (slot, probe) in probes.iter().enumerate() {
            source.push_str(&format!("DMOV H{:08X} D{}\nDMOV H{:08X} D{}\nDMOV H5555AAAA D{}\nMOV K-123 D{}\nMOV K0 D{}\n",
                probe.left, slot*4, probe.right.unwrap_or(0), slot*4+2,100+slot*2,200+slot,240+slot));
        }
        for (slot, probe) in probes.iter().enumerate() {
            let first = if probe.literals & 1 != 0 {
                let value = f32::from_bits(probe.left);
                assert!(value.is_finite());
                format!("F{value}")
            } else {
                format!("D{}", slot * 4)
            };
            let args = if let Some(bits) = probe.right {
                let second = if probe.literals & 2 != 0 {
                    let value = f32::from_bits(bits);
                    assert!(value.is_finite());
                    format!("F{value}")
                } else {
                    format!("D{}", slot * 4 + 2)
                };
                format!("{first} {second}")
            } else {
                first
            };
            source.push_str(&format!("LD X0\n{} M1018\n{} {args} D{}\nMOV K{} D{}\nINC D{}\nMOV D1067 D{}\nMOV D1068 D{}\nMOV D1069 D{}\n",
                if probe.degrees { "SET" } else { "RST" },probe.op,100+slot*2,1000+slot,200+slot,240+slot,
                300+slot*3,301+slot*3,302+slot*3));
            // Save every result's flags and sticky error bits before the next
            // math instruction can overwrite the shared system fields.
            for (index, relay) in ["M1020", "M1021", "M1022", "M1067", "M1068"]
                .into_iter()
                .enumerate()
            {
                source.push_str(&format!("LD {relay}\nOUT M{}\n", 2000 + slot * 8 + index));
            }
        }
        source.push_str("END");
        case(
            &format!("float_advanced_precision_{name}"),
            &source,
            if pulse {
                vec![
                    (vec![("X1", true)], 1),
                    (vec![("X1", false), ("X0", true)], 3),
                    (vec![("X0", false)], 1),
                    (vec![("X0", true)], 2),
                ]
            } else {
                vec![
                    (vec![("X1", true)], 1),
                    (vec![("X1", false), ("X0", true)], 1),
                    (vec![("X0", false)], 1),
                ]
            },
        )
    }
    let mut cases = Vec::new();
    let mut seed = 0x71b3_92d5u32;
    let mut random = || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        seed
    };
    for group in 0..12 {
        let probes = (0..8)
            .map(|slot| {
                let index = group * 8 + slot;
                let value = if index < 16 {
                    [
                        -100.0f32,
                        -88.0,
                        -20.0,
                        -10.0,
                        -2.0,
                        -1.0,
                        -f32::EPSILON,
                        -0.0,
                        0.0,
                        f32::EPSILON,
                        0.5,
                        1.0,
                        std::f32::consts::LN_2,
                        10.0,
                        40.0,
                        80.0,
                    ][index]
                } else {
                    -100.0 + (random() as f64 / u32::MAX as f64 * 180.0) as f32
                };
                Probe::unary("DEXP", value)
            })
            .collect();
        cases.push(batch(
            &format!("exp_{group}"),
            probes,
            group % 2 != 0,
            false,
        ));
    }
    for group in 0..12 {
        let probes = (0..8)
            .map(|slot| {
                let word = random();
                let exponent = if slot % 4 == 0 {
                    0
                } else {
                    1 + (word >> 24) % 254
                };
                let bits = (exponent << 23) | ((word & 0x007fffff).max(1));
                Probe::unary("DLN", f32::from_bits(bits))
            })
            .collect();
        cases.push(batch(&format!("ln_{group}"), probes, group % 2 != 0, false));
    }
    for group in 0..10 {
        let probes = (0..8)
            .map(|slot| {
                let offset = 1 + (random() % 128);
                let base = f32::from_bits(if slot % 2 == 0 {
                    0x3f800000 + offset
                } else {
                    0x3f800000 - offset
                });
                let word = random();
                let argument =
                    f32::from_bits((((word >> 24) % 254) << 23) | (word & 0x007fffff).max(1));
                Probe::binary("DLOG", base, argument)
            })
            .collect();
        cases.push(batch(
            &format!("log_near_one_{group}"),
            probes,
            group % 2 != 0,
            false,
        ));
    }
    let exponents = [
        -10.0f32,
        -9.0,
        -8.0,
        -7.0,
        -6.0,
        -5.0,
        -4.0,
        -3.0,
        -2.0,
        -1.0,
        -0.5,
        -0.25,
        0.0,
        0.25,
        0.5,
        1.0,
        2.0,
        3.0,
        4.0,
        5.0,
        6.0,
        7.0,
        8.0,
        9.0,
        10.0,
        1.25,
        2.75,
        -1.25,
        -2.75,
        7.5,
        -7.5,
        1.0 / 3.0,
    ];
    for group in 0..14 {
        let probes = (0..8)
            .map(|slot| {
                let base = 0.25 + (random() as f64 / u32::MAX as f64 * 3.75) as f32;
                Probe::binary(
                    "DPOW",
                    base,
                    exponents[(group * 8 + slot) % exponents.len()],
                )
            })
            .collect();
        cases.push(batch(
            &format!("pow_{group}"),
            probes,
            group % 2 != 0,
            false,
        ));
    }
    for op in ["DASIN", "DACOS", "DATAN"] {
        for group in 0..4 {
            let probes = (0..8)
                .map(|slot| {
                    let index = group * 8 + slot;
                    let value = if op == "DATAN" && group >= 2 {
                        [
                            1e10f32,
                            -1e10,
                            1e20,
                            -1e20,
                            f32::MAX,
                            -f32::MAX,
                            1e30,
                            -1e30,
                        ][slot]
                    } else if index < 16 {
                        [
                            0.0f32,
                            -0.0,
                            f32::from_bits(1),
                            -f32::from_bits(1),
                            1e-38,
                            -1e-38,
                            1e-20,
                            -1e-20,
                            0.5,
                            -0.5,
                            std::f32::consts::FRAC_1_SQRT_2,
                            -std::f32::consts::FRAC_1_SQRT_2,
                            f32::from_bits(0x3f7fffff),
                            f32::from_bits(0xbf7fffff),
                            1.0,
                            -1.0,
                        ][index]
                    } else {
                        let offset = 1 + (random() % 1024);
                        f32::from_bits(if slot % 2 == 0 {
                            0x3f800000 - offset
                        } else {
                            0xbf800000 - offset
                        })
                    };
                    Probe::unary(op, value)
                })
                .collect();
            cases.push(batch(
                &format!("inverse_{op}_{group}"),
                probes,
                group % 2 != 0,
                false,
            ));
        }
    }
    for op in ["DSIN", "DCOS", "DTAN"] {
        for degrees in [false, true] {
            for group in 0..2 {
                let probes = (0..8)
                    .map(|slot| {
                        let value = if group == 0 {
                            [30.0f32, 33.25, 37.5, 42.125, 48.75, 55.5, 66.25, 75.0][slot]
                        } else {
                            [
                                f32::from_bits(0x49800000),
                                -f32::from_bits(0x49800000),
                                f32::from_bits(0x4e800000),
                                -f32::from_bits(0x4e800000),
                                f32::from_bits(0x53800000),
                                -f32::from_bits(0x53800000),
                                f32::from_bits(0x59ffffff),
                                -f32::from_bits(0x59ffffff),
                            ][slot]
                        };
                        let mut probe = Probe::unary(op, value);
                        probe.degrees = degrees;
                        probe
                    })
                    .collect();
                cases.push(batch(
                    &format!("trig_{op}_{degrees}_{group}"),
                    probes,
                    group % 2 != 0,
                    false,
                ));
            }
        }
    }
    for op in ["DADDR", "DSUBR", "DMULR", "DDIVR"] {
        for pulse in [false, true] {
            let probes = [
                (1.2f32.to_bits(), 2.2f32.to_bits(), 3),
                (0x7fc12345, 0, 0),
                (0x7f800000, 0x80000000, 0),
                (1.0f32.to_bits(), 0, 2),
                (1.0f32.to_bits(), 0x80000000, 1),
                (0xff812345, 1.0f32.to_bits(), 0),
                (0xff800000, 0x7f800000, 0),
                (0x80000000, 1.0f32.to_bits(), 2),
            ]
            .into_iter()
            .map(|(left, right, literals)| Probe {
                op: format!("{op}{}", if pulse { "P" } else { "" }),
                left,
                right: Some(right),
                degrees: false,
                literals,
            })
            .collect();
            cases.push(batch(
                &format!("r_padding_{op}_{pulse}"),
                probes,
                true,
                pulse,
            ));
        }
    }
    cases
}

fn float_advanced_cases(target: Target) -> Vec<Case> {
    fn seeded(name: &str, setup: &str, instruction: &str, flags: bool, degrees: bool) -> Case {
        let flag = if flags { "SET" } else { "RST" };
        let mode = if degrees { "SET" } else { "RST" };
        case(
            &format!("float_advanced_{name}"),
            &format!(
                "LD X1\nDMOV H5555AAAA D20\n{setup}\n{flag} M1020\n{flag} M1021\n{flag} M1022\n{flag} M1081\n{mode} M1018\nLD X0\n{instruction}\nOUT Y0\nEND"
            ),
            vec![
                (vec![("X1", true)], 1),
                (vec![("X1", false), ("X0", true)], 1),
                (vec![("X0", false)], 1),
            ],
        )
    }
    let mut cases = Vec::new();
    // Finite, bounded overflow/underflow probes come first, before the larger
    // domain matrix. None is silently omitted when the native is inconclusive.
    for (index, (setup, instruction)) in [
        ("DMOV H42B00000 D0", "DEXP D0 D20"),
        ("DMOV H42B20000 D0", "DEXP D0 D20"),
        ("DMOV HC2D00000 D0", "DEXP D0 D20"),
        ("DMOV H60AD78EC D0\nDMOV H40000000 D4", "DPOW D0 D4 D20"),
        ("", "DADDR F3.4028234e38 F3.4028234e38 D20"),
        ("", "DDIVR F1.17549435e-38 F2 D20"),
    ]
    .into_iter()
    .enumerate()
    {
        cases.push(seeded(
            &format!("bounded_result_{index}"),
            setup,
            instruction,
            index % 2 != 0,
            false,
        ));
    }
    let special = [
        0x00000000u32,
        0x80000000,
        0x00000001,
        0x80000001,
        0x00800000,
        0x80800000,
        0x7f7fffff,
        0xff7fffff,
        0x7f800000,
        0xff800000,
        0x7fc12345,
        0xffc12345,
        0x7f812345,
        0xff812345,
    ];
    for op in ["DEXP", "DLN", "DESQR"] {
        for bits in special {
            for flags in [false, true] {
                cases.push(seeded(
                    &format!("domain_{op}_{bits:08x}_{flags}"),
                    &format!("DMOV H{bits:08X} D0"),
                    &format!("{op} D0 D20"),
                    flags,
                    false,
                ));
            }
        }
    }
    for op in ["DSIN", "DCOS", "DTAN"] {
        for degrees in [false, true] {
            let values = if degrees {
                [
                    0x00000000u32,
                    0x80000000,
                    0x41f00000,
                    0x42b40000,
                    0x43870000,
                    0x43b40000,
                    0xc2b40000,
                    0x60ad78ec,
                    0x5a000000,
                    0x7f800000,
                    0xff800000,
                    0x7f812345,
                ]
            } else {
                [
                    0x00000000u32,
                    0x80000000,
                    0x00000001,
                    0x80000001,
                    0x3f800000,
                    0xbf800000,
                    0x3fc90fdb,
                    0xbfc90fdb,
                    0x59ffffff,
                    0x5a000000,
                    0x7f800000,
                    0x7fc12345,
                ]
            };
            for (index, bits) in values.into_iter().enumerate() {
                cases.push(seeded(
                    &format!("forward_{op}_{degrees}_{bits:08x}"),
                    &format!("DMOV H{bits:08X} D0"),
                    &format!("{op} D0 D20"),
                    index % 2 != 0,
                    degrees,
                ));
            }
        }
    }
    for op in ["DASIN", "DACOS", "DATAN"] {
        for degrees in [false, true] {
            for (index, bits) in [
                0xbf800000u32,
                0xbf800001,
                0x00000000,
                0x80000000,
                0x3f800000,
                0x3f800001,
                0x7f800000,
                0x7fc12345,
            ]
            .into_iter()
            .enumerate()
            {
                cases.push(seeded(
                    &format!("inverse_{op}_{degrees}_{bits:08x}"),
                    &format!("DMOV H{bits:08X} D0"),
                    &format!("{op} D0 D20"),
                    index % 2 != 0,
                    degrees,
                ));
            }
        }
    }
    for (index, (base, argument)) in [
        (0x40a00000u32, 0x42fa0000u32), // log base5 of125=3
        (0x40000000, 0x41000000),
        (0x3f000000, 0x40000000),
        (0x3f800001, 0x40000000),
        (0x3f7fffff, 0x3f000000),
        (0x3f800000, 0),
        (0, 0),
        (0xbf800000, 0xbf800000),
        (0x7fc12345, 0),
        (0x40000000, 0),
        (0x40000000, 0xbf800000),
        (0x7f800000, 0x7f800000),
    ]
    .into_iter()
    .enumerate()
    {
        for flags in [false, true] {
            cases.push(seeded(
                &format!("log_base_argument_{index}_{flags}"),
                &format!("DMOV H{base:08X} D0\nDMOV H{argument:08X} D4"),
                "DLOG D0 D4 D20",
                flags,
                false,
            ));
        }
    }
    for (index, (base, exponent)) in [
        (0x40a00000u32, 0x40400000u32),
        (0x40000000, 0xc0400000),
        (0x40000000, 0),
        (0x40000000, 0x80000000),
        (0xbf800000, 0x40000000),
        (0xbf800000, 0x3f000000),
        (0, 0),
        (0x80000000, 0xbf800000),
        (0x7fc12345, 0x7f812345),
        (0, 0x7fc12345),
        (0x40000000, 0x7fc12345),
        (0x40000000, 0xff812345),
        (0x40000000, 0x7f800000),
        (0x3f000000, 0xff800000),
        (0x7f800000, 0xbf800000),
        (0x7f800000, 0),
    ]
    .into_iter()
    .enumerate()
    {
        cases.push(seeded(
            &format!("pow_base_exponent_{index}"),
            &format!("DMOV H{base:08X} D0\nDMOV H{exponent:08X} D4"),
            "DPOW D0 D4 D20",
            index % 2 != 0,
            false,
        ));
    }
    for op in ["DADDR", "DSUBR", "DMULR", "DDIVR"] {
        for (index, args) in ["F1.2 F2.2", "D0 F-2.2", "F-1.2 D4", "D0 D4"]
            .into_iter()
            .enumerate()
        {
            cases.push(seeded(
                &format!("literal_mix_{op}_{index}"),
                "DMOV H3F99999A D0\nDMOV H400CCCCD D4",
                &format!("{op} {args} D20"),
                index % 2 != 0,
                false,
            ));
        }
        for (index, (left, right)) in [(0x7fc12345u32, 0x7f800000u32), (0x3f800000, 0x80000000)]
            .into_iter()
            .enumerate()
        {
            cases.push(seeded(
                &format!("literal_special_{op}_{index}"),
                &format!("DMOV H{left:08X} D0\nDMOV H{right:08X} D4"),
                &format!("{op} D0 D4 D20"),
                index % 2 != 0,
                false,
            ));
        }
    }
    for op in [
        "DEXP", "DLN", "DLOG", "DESQR", "DPOW", "DSIN", "DCOS", "DTAN", "DASIN", "DACOS", "DATAN",
        "DADDR", "DSUBR", "DMULR", "DDIVR",
    ] {
        let binary = matches!(op, "DLOG" | "DPOW" | "DADDR" | "DSUBR" | "DMULR" | "DDIVR");
        let setup = if op == "DLOG" {
            "DMOV H40A00000 D0\nDMOV H42FA0000 D4"
        } else if op == "DPOW" {
            "DMOV H40000000 D0\nDMOV H40400000 D4"
        } else {
            "DMOV H3F000000 D0\nDMOV H40000000 D4"
        };
        for pulse in [false, true] {
            let name = format!("{op}{}", if pulse { "P" } else { "" });
            let args = if binary { "D0 D4 D20" } else { "D0 D20" };
            cases.push(case(&format!("float_advanced_pulse_{name}"),
                &format!("LD X1\n{setup}\nDMOV H5555AAAA D20\nSET M1020\nSET M1021\nSET M1022\nRST M1018\nLD X2\nDMOV HC0600000 D0\nLD X0\n{name} {args}\nEND"),
                vec![(vec![("X1",true)],1), (vec![("X1",false),("X0",true)],1),
                    (vec![("X2",true)],2), (vec![("X0",false),("X2",false)],1), (vec![("X0",true)],2)]));
        }
        let args = if binary { "D0 D4 D1" } else { "D0 D1" };
        cases.push(seeded(
            &format!("partial_alias_{op}"),
            setup,
            &format!("{op} {args}"),
            true,
            false,
        ));
        let end = if target == Target::Ss2 { 4998 } else { 9998 };
        let end_setup = setup.replace("D0", &format!("D{end}"));
        let args = if binary {
            format!("D{end} D4 D{end}")
        } else {
            format!("D{end} D{end}")
        };
        cases.push(seeded(
            &format!("last_pair_{op}"),
            &end_setup,
            &format!("{op} {args}"),
            false,
            false,
        ));
    }
    for (index, instruction) in [
        "DEXP K-1 D20",
        "DLN H7FFFFFFF D20",
        "DLOG K5 K125 D20",
        "DESQR K2 D20",
        "DPOW K5 K3 D20",
        "DSIN K16777217 D20",
        "DCOS H0000005A D20",
        "DTAN K-90 D20",
        "DASIN HFFFFFFFF D20",
        "DACOS K1 D20",
        "DATAN K-7 D20",
    ]
    .into_iter()
    .enumerate()
    {
        cases.push(seeded(
            &format!("numeric_constant_{index}"),
            "",
            instruction,
            index % 2 != 0,
            index % 2 != 0,
        ));
    }
    cases
}

fn float_math_cases(target: Target) -> Vec<Case> {
    fn seeded(name: &str, setup: &str, instruction: &str, flags: bool) -> Case {
        let flag = if flags { "SET" } else { "RST" };
        case(
            &format!("float_math_{name}"),
            &format!(
                "LD X1\nDMOV H5555AAAA D20\n{setup}\n{flag} M1020\n{flag} M1021\n{flag} M1022\n{flag} M1081\nLD X0\n{instruction}\nOUT Y0\nEND"
            ),
            vec![
                (vec![("X1", true)], 1),
                (vec![("X1", false), ("X0", true)], 1),
                (vec![("X0", false)], 1),
            ],
        )
    }
    let mut cases = Vec::new();
    // Raw IEEE encodings deliberately preserve signed zero and NaN payloads.
    let special = [
        0x00000000u32,
        0x80000000,
        0x00000001,
        0x80000001,
        0x007fffff,
        0x807fffff,
        0x00800000,
        0x80800000,
        0x7f7fffff,
        0xff7fffff,
        0x7f800000,
        0xff800000,
        0x7fc12345,
        0xffc12345,
        0x7f812345,
        0xff812345,
        0x3f800000,
        0xbf800000,
    ];
    for op in ["DRAD", "DDEG"] {
        for bits in special {
            for flags in [false, true] {
                cases.push(seeded(
                    &format!("angle_{op}_{bits:08x}_{flags}"),
                    &format!("DMOV H{bits:08X} D0"),
                    &format!("{op} D0 D20"),
                    flags,
                ));
            }
        }
    }
    let arithmetic: [(&str, [(u32, u32); 8]); 4] = [
        (
            "DEADD",
            [
                (0, 0x80000000),
                (0x7f7fffff, 0x7f7fffff),
                (0x00800000, 0x807fffff),
                (0x7f7fffff, 1),
                (0x7f800000, 0xff800000),
                (0x7fc12345, 0x3f800000),
                (0xff812345, 0x3f800000),
                (0xbf800000, 0x3f800000),
            ],
        ),
        (
            "DESUB",
            [
                (0, 0x80000000),
                (0x7f7fffff, 0xff7fffff),
                (0x00800000, 0x007fffff),
                (0xff7fffff, 0x00800000),
                (0x7f800000, 0x7f800000),
                (0xffc12345, 0xbf800000),
                (0x7f812345, 0),
                (0x3f800000, 0x3f800000),
            ],
        ),
        (
            "DEMUL",
            [
                (0x80000000, 0x3f800000),
                (0x7f7fffff, 0x40000000),
                (0x00800000, 0x3f000000),
                (1, 0x3f000000),
                (0x7f800000, 0),
                (0x7fc12345, 0x3f800000),
                (0xff812345, 0x3f800000),
                (0xff7fffff, 0xbf800000),
            ],
        ),
        (
            "DEDIV",
            [
                (0x3f800000, 0),
                (0xbf800000, 0x80000000),
                (0, 0),
                (0x7f7fffff, 1),
                (0x00800000, 0x40000000),
                (1, 0x40000000),
                (0x7f800000, 0x7f800000),
                (0x7f812345, 0x3f800000),
            ],
        ),
    ];
    for (op, pairs) in arithmetic {
        for (index, (left, right)) in pairs.into_iter().enumerate() {
            for flags in [false, true] {
                cases.push(seeded(
                    &format!("binary_{op}_{index}_{flags}"),
                    &format!("DMOV H{left:08X} D0\nDMOV H{right:08X} D4"),
                    &format!("{op} D0 D4 D20"),
                    flags,
                ));
            }
        }
    }
    for bits in [
        0x00000000u32,
        0x80000000,
        0x3f800000,
        0xbf800000,
        0x40490fdb,
        0xc0490fdb,
        0x411ffdf4,
        0xc11ffdf4,
        0x00000001,
        0x007fffff,
        0x00800000,
        0x7f7fffff,
        0x7f800000,
        0xff800000,
        0x7fc12345,
        0x7f812345,
    ] {
        for flags in [false, true] {
            cases.push(seeded(
                &format!("decimal_encode_{bits:08x}_{flags}"),
                &format!("DMOV H{bits:08X} D0"),
                "DEBCD D0 D20",
                flags,
            ));
        }
    }
    for (index, (coefficient, exponent)) in [
        (0, 0),
        (0, -41),
        (0, 35),
        (314, -2),
        (-314, -2),
        (1234, 3),
        (-1234, 3),
        (1175, -41),
        (-1175, -41),
        (1, -41),
        (-1, -41),
        (9999, 35),
        (-9999, 35),
        (3402, 35),
        (3403, 35),
        (9999, -41),
        (10000, 0),
        (-10000, 0),
        (1234, -42),
        (1234, 36),
        (32767, 0),
        (-32768, 0),
    ]
    .into_iter()
    .enumerate()
    {
        cases.push(seeded(
            &format!("decimal_decode_{coefficient}_{exponent}"),
            &format!("MOV K{coefficient} D0\nMOV K{exponent} D1"),
            "DEBIN D0 D20",
            index % 2 != 0,
        ));
    }
    for (coefficient, exponent, flags) in [
        (0, 0, true),
        (314, -2, false),
        (10000, 0, true),
        (9999, 35, false),
        (1, -41, false),
    ] {
        cases.push(seeded(
            &format!("decimal_decode_flags_{coefficient}_{exponent}_{flags}"),
            &format!("MOV K{coefficient} D0\nMOV K{exponent} D1"),
            "DEBIN D0 D20",
            flags,
        ));
    }
    for op in [
        "DRAD", "DDEG", "DEBCD", "DEBIN", "DEADD", "DESUB", "DEMUL", "DEDIV",
    ] {
        let binary = matches!(op, "DEADD" | "DESUB" | "DEMUL" | "DEDIV");
        let setup = if op == "DEBIN" {
            "MOV K314 D0\nMOV K-2 D1\nDMOV H40000000 D4"
        } else {
            "DMOV H3FA00000 D0\nDMOV H40000000 D4"
        };
        let change = if op == "DEBIN" {
            "MOV K-314 D0\nMOV K-2 D1"
        } else {
            "DMOV HC0600000 D0"
        };
        for pulse in [false, true] {
            let name = format!("{op}{}", if pulse { "P" } else { "" });
            let args = if binary { "D0 D4 D20" } else { "D0 D20" };
            cases.push(case(
                &format!("float_math_pulse_{name}"),
                &format!("LD X1\n{setup}\nDMOV H5555AAAA D20\nSET M1020\nSET M1021\nSET M1022\nSET M1081\nLD X2\n{change}\nLD X0\n{name} {args}\nEND"),
                vec![
                    (vec![("X1",true)],1),
                    (vec![("X1",false),("X0",true)],1),
                    (vec![("X2",true)],2),
                    (vec![("X0",false),("X2",false)],1),
                    (vec![("X0",true)],2),
                ],
            ));
        }
        for destination in ["D0", "D1"] {
            let args = if binary {
                format!("D0 D4 {destination}")
            } else {
                format!("D0 {destination}")
            };
            cases.push(seeded(
                &format!("alias_{op}_{destination}"),
                setup,
                &format!("{op} {args}"),
                false,
            ));
        }
        let end = if target == Target::Ss2 { 4998 } else { 9998 };
        let end_setup = setup
            .replace("D0", &format!("D{end}"))
            .replace("D1", &format!("D{}", end + 1));
        let args = if binary {
            format!("D{end} D4 D{end}")
        } else {
            format!("D{end} D{end}")
        };
        cases.push(seeded(
            &format!("last_pair_{op}"),
            &end_setup,
            &format!("{op} {args}"),
            true,
        ));
    }
    for (index, instruction) in [
        "DRAD K180 D20",
        "DRAD K-360 D20",
        "DRAD H000000B4 D20",
        "DDEG K180 D20",
        "DDEG K-360 D20",
        "DDEG H000000B4 D20",
        "DEADD K-7 H00000003 D20",
        "DESUB K-7 H00000003 D20",
        "DEMUL K-7 H00000003 D20",
        "DEDIV K-7 H00000003 D20",
        "DEADD K16777217 K-16777216 D20",
        "DESUB H80000000 K1 D20",
    ]
    .into_iter()
    .enumerate()
    {
        cases.push(seeded(
            &format!("constant_{index}"),
            "",
            instruction,
            index % 2 != 0,
        ));
    }
    cases
}

fn float_foundation_cases(target: Target) -> Vec<Case> {
    let mut cases = Vec::new();
    for op in ["FLT", "DFLT", "INT", "DINT"] {
        for dest in ["D0", "D1", "D2", "D20"] {
            for reverse in [false, true] {
                let flag = if reverse { "SET" } else { "RST" };
                cases.push(case(
                    &format!("float_foundation_alias_{op}_{dest}_{reverse}"),
                    &format!("LD X1\n{flag} M1081\nDMOV H414C0000 D0\nLD X0\n{op} D0 {dest}\nEND"),
                    vec![
                        (vec![("X1", true)], 1),
                        (vec![("X1", false), ("X0", true)], 2),
                    ],
                ));
            }
        }
    }
    for area in ["T20", "T254", "C20", "C198", "C199", "C200", "C255"] {
        for op in ["INT", "DINT"] {
            cases.push(case(
                &format!("float_foundation_numeric_source_{op}_{area}"),
                &format!("LD X1\nDMOVR F1.2 {area}\nLD X0\n{op} {area} D20\nEND"),
                vec![
                    (vec![("X1", true)], 1),
                    (vec![("X1", false), ("X0", true)], 2),
                ],
            ));
            if op == "DINT" || !matches!(area, "C200" | "C255") {
                cases.push(case(
                    &format!("float_foundation_numeric_dest_{op}_{area}"),
                    &format!("LD X1\nDMOVR F-12.75 D0\nLD X0\n{op} D0 {area}\nEND"),
                    vec![
                        (vec![("X1", true)], 1),
                        (vec![("X1", false), ("X0", true)], 2),
                    ],
                ));
            }
        }
        cases.push(case(
            &format!("float_foundation_natural_contacts_{area}"),
            &format!("LD X0\nDMOVR F1.2 {area}\nFLD= {area} F1.2\nOUT Y0\nEND"),
            vec![(vec![], 1), (vec![("X0", true)], 2)],
        ));
    }
    for (i, (low, high, value)) in [
        (0x7fc01234u32, 0x41400000u32, 0x3f800000u32),
        (0, 0x7f801234, 0x3f800000),
        (0, 0x41400000, 0x7fc01234),
        (0xff800000, 0x7f800000, 0),
        (0, 0, 0x80000000),
        (1, 1, 1),
    ]
    .into_iter()
    .enumerate()
    {
        cases.push(case(&format!("float_foundation_zone_bits_{i}"),&format!("LD X1\nDMOV H{low:08X} D0\nDMOV H{high:08X} D4\nDMOV H{value:08X} D8\nSET M1020\nSET M1021\nSET M1022\nLD X0\nDEZCP D0 D4 D8 M10\nEND"),vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2)]));
    }
    for (integer, bits) in [
        (-1, 0xbf800000u32),
        (16777217, 0x4b800000),
        (i32::MAX, 0x4f000000),
        (i32::MIN, 0xcf000000),
    ] {
        cases.push(case(&format!("float_foundation_compare_integer_{integer}"),&format!("LD X1\nDMOV H{bits:08X} D0\nLD X0\nDECMP D0 K{integer} M10\nDECMP H{integer:08X} D0 M20\nEND"),vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2)]));
    }
    let bits = [
        0x00000000u32,
        0x80000000,
        0x3f400000,
        0xbf400000,
        0x414c0000,
        0xc14c0000,
        0x46fffe00,
        0x46ffff00,
        0x47000000,
        0xc7000000,
        0x4effffff,
        0x4f000000,
        0x4f000001,
        0xcf000000,
        0xcf000001,
        0x00000001,
        0x80000001,
        0x007fffff,
        0x7f7fffff,
        0x7f800000,
        0xff800000,
        0x7fc01234,
        0x7f801234,
    ];
    for op in ["FLT", "DFLT", "INT", "DINT"] {
        for flags in [false, true] {
            let flag = if flags { "SET" } else { "RST" };
            for value in bits {
                cases.push(case(&format!("float_foundation_convert_{op}_{flags}_{value:08x}"),&format!("LD X1\nSET M1081\nDMOV H{value:08X} D0\nDMOV H55555555 D20\n{flag} M1020\n{flag} M1021\n{flag} M1022\nLD X0\n{op} D0 D20\nOUT Y0\nEND"),vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2)]));
            }
        }
    }
    for op in ["FLT", "DFLT"] {
        for value in [
            0,
            1,
            -1,
            32767,
            -32768,
            16777215,
            16777216,
            16777217,
            16777219,
            i32::MIN,
            i32::MAX,
        ] {
            if op == "FLT" && i16::try_from(value).is_err() {
                continue;
            }
            for flags in [false, true] {
                let flag = if flags { "SET" } else { "RST" };
                let mov = if op == "DFLT" { "DMOV" } else { "MOV" };
                cases.push(case(&format!("float_foundation_forward_{op}_{flags}_{value}"),&format!("LD X1\nRST M1081\n{mov} K{value} D0\n{flag} M1020\n{flag} M1021\n{flag} M1022\nLD X0\n{op} D0 D20\nEND"),vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2)]));
            }
        }
    }
    for relation in ["=", ">", "<", "<>", "<=", ">="] {
        for (i, (left, right)) in [
            (0u32, 0x80000000u32),
            (0x3f99999a, 0x3f99999a),
            (0x3f99999b, 0x3f99999a),
            (0xc1400000, 0x41400000),
            (1, 0),
            (0x7f800000, 0x7f800000),
            (0x7f800000, 0xff800000),
            (0x7fc01234, 0),
            (0, 0x7f801234),
        ]
        .into_iter()
        .enumerate()
        {
            cases.push(case(&format!("float_foundation_contacts_{relation}_{i}"),&format!("LD X1\nDMOV H{left:08X} D0\nDMOV H{right:08X} D4\nSET M1020\nSET M1021\nSET M1022\nFLD{relation} D0 D4\nOUT Y0\nLD X0\nFAND{relation} D0 D4\nOUT Y1\nLD X0\nFOR{relation} D0 D4\nOUT Y2\nEND"),vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],1),(vec![("X0",false)],1)]));
        }
    }
    for (i, literal) in [
        "F0",
        "F-0.0",
        "F1.2",
        "F-23",
        "F1.200E+0",
        "F3.4028235E38",
        "F1E-45",
    ]
    .iter()
    .enumerate()
    {
        cases.push(case(&format!("float_foundation_literals_{i}"),&format!("LD X0\nDMOVR {literal} D0\nFLD= D0 {literal}\nOUT Y0\nLD X0\nFAND>= {literal} D0\nOUT Y1\nLD X0\nFOR< {literal} D0\nOUT Y2\nEND"),vec![(vec![],1),(vec![("X0",true)],2),(vec![("X0",false)],1)]));
    }
    for (i, (left, right)) in [
        (0u32, 0u32),
        (0x41400000, 0xc1400000),
        (0x7f800000, 0xff800000),
        (0x7fc01234, 0),
        (0, 0x7f801234),
    ]
    .into_iter()
    .enumerate()
    {
        cases.push(case(&format!("float_foundation_compare_{i}"),&format!("LD X1\nDMOV H{left:08X} D0\nDMOV H{right:08X} D4\nSET M10\nSET M11\nSET M12\nSET M1020\nSET M1021\nSET M1022\nLD X0\nDECMP D0 D4 M10\nEND"),vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2)]));
    }
    for (i, (low, high, value)) in [
        (10, 20, 9),
        (10, 20, 10),
        (10, 20, 15),
        (10, 20, 20),
        (10, 20, 21),
        (20, 10, 15),
        (20, 10, 20),
        (20, 10, 21),
    ]
    .into_iter()
    .enumerate()
    {
        cases.push(case(
            &format!("float_foundation_zone_{i}"),
            &format!("LD X0\nDEZCP K{low} K{high} K{value} M10\nEND"),
            vec![(vec![], 1), (vec![("X0", true)], 2)],
        ));
    }
    for (name, body) in [
        ("FLT", "FLT D0 D20"),
        ("DFLT", "DFLT D0 D20"),
        ("INT", "INT D0 D20"),
        ("DINT", "DINT D0 D20"),
        ("DMOVR", "DMOVR F1.2 D20"),
        ("DECMP", "DECMP D0 K1 M10"),
        ("DEZCP", "DEZCP K0 K1 D0 M10"),
    ] {
        for pulse in [false, true] {
            let body = if pulse {
                body.replacen(name, &format!("{name}P"), 1)
            } else {
                body.to_string()
            };
            cases.push(case(
                &format!("float_foundation_pulse_{name}_{pulse}"),
                &format!("LD X1\nDMOV H3F000000 D0\nSET M1081\nLD X0\n{body}\nDINC D0\nEND"),
                vec![
                    (vec![("X1", true)], 1),
                    (vec![("X1", false), ("X0", true)], 3),
                    (vec![("X0", false)], 1),
                    (vec![("X0", true)], 2),
                ],
            ));
        }
    }
    let end = if target == Target::Ss2 { 4998 } else { 9998 };
    cases.push(case("float_foundation_model_last_pair",&format!("LD X0\nDMOVR F-12.75 D{end}\nINT D{end} D20\nDINT D{end} D22\nDMOVR F1.2 D{end}\nFLD= D{end} F1.2\nOUT Y0\nEND"),vec![(vec![("X0",true)],2)]));
    cases
}

fn text_encoding_cases(target: Target) -> Vec<Case> {
    let mut cases = Vec::new();
    let end = if target == Target::Ss2 { 4999 } else { 9999 };
    for mode in [false, true] {
        let flag = if mode { "SET" } else { "RST" };
        for op in ["ASCI", "HEX", "CCD", "LRC", "CRC"] {
            for n in [1, 2, 3, 4, 8] {
                let args = match op {
                    "ASCI" | "HEX" | "CCD" => format!("D{end} D20 K{n}"),
                    _ => format!("D{end} K{n} D20"),
                };
                cases.push(case(&format!("text_encoding_boundary_source_{op}_{mode}_{n}"),&format!("LD X1\n{flag} M1161\nMOV H3030 D{end}\nFMOV HAA55 D20 K8\nLD X0\n{op} {args}\nEND"),vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2)]));
            }
            let args = match op {
                "ASCI" | "HEX" | "CCD" => format!("D0 D{end} K8"),
                _ => format!("D0 K8 D{end}"),
            };
            cases.push(case(&format!("text_encoding_boundary_dest_{op}_{mode}"),&format!("LD X1\n{flag} M1161\nFMOV H3030 D0 K8\nMOV HAA55 D{end}\nLD X0\n{op} {args}\nEND"),vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2)]));
        }
        for op in ["SEGD", "ASCI", "HEX", "CCD"] {
            for area in ["T20", "C20"] {
                let args = if op == "SEGD" {
                    format!("{area} D20")
                } else {
                    format!("{area} D20 K2")
                };
                let seed = if area == "C200" || area == "C255" {
                    format!("DMOV H12344443 {area}")
                } else {
                    format!("DMOV H4443 {area}")
                };
                cases.push(case(
                    &format!("text_encoding_numeric_source_{op}_{mode}_{area}"),
                    &format!("LD X1\n{flag} M1161\n{seed}\nLD X0\n{op} {args}\nEND"),
                    vec![
                        (vec![("X1", true)], 1),
                        (vec![("X1", false), ("X0", true)], 2),
                    ],
                ));
                let args = if op == "SEGD" {
                    format!("K7 {area}")
                } else {
                    format!("D0 {area} K2")
                };
                cases.push(case(
                    &format!("text_encoding_numeric_dest_{op}_{mode}_{area}"),
                    &format!("LD X1\n{flag} M1161\n{seed}\nMOV H4443 D0\nLD X0\n{op} {args}\nEND"),
                    vec![
                        (vec![("X1", true)], 1),
                        (vec![("X1", false), ("X0", true)], 2),
                    ],
                ));
            }
        }
    }
    for mode in [false, true] {
        let flag = if mode { "SET" } else { "RST" };
        for op in ["ASCI", "HEX"] {
            for dest in ["D0", "D1", "D2", "D10"] {
                cases.push(case(
                    &format!("text_encoding_overlap_{op}_{mode}_{dest}"),
                    &format!(
                        "LD X1\n{flag} M1161\nFMOV H4443 D0 K16\nLD X0\n{op} D0 {dest} K8\nEND"
                    ),
                    vec![
                        (vec![("X1", true)], 1),
                        (vec![("X1", false), ("X0", true)], 2),
                    ],
                ));
            }
            for n in [1, 3, 5, 8, 16] {
                cases.push(case(
                    &format!("text_encoding_constant_{op}_{mode}_{n}"),
                    &format!(
                        "LD X1\n{flag} M1161\nFMOV HAA55 D10 K16\nLD X0\n{op} H4443 D10 K{n}\nEND"
                    ),
                    vec![
                        (vec![("X1", true)], 1),
                        (vec![("X1", false), ("X0", true)], 2),
                    ],
                ));
            }
        }
        for invalid in [0, 1, 2, 3, 4, 5, 6, 7] {
            let offset = if mode { invalid } else { invalid / 2 };
            let value = if !mode && invalid % 2 == 1 {
                0x6730
            } else {
                0x3067
            };
            cases.push(case(&format!("text_encoding_hex_partial_error_{mode}_{invalid}"),&format!("LD X1\n{flag} M1161\nFMOV H3030 D0 K8\nMOV H{value:04X} D{offset}\nFMOV HAA55 D20 K4\nLD X0\nHEX D0 D20 K8\nEND"),vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2)]));
        }
    }
    for value in 0..16 {
        cases.push(case(&format!("text_encoding_SEGD_{value}"), &format!("LD X1\nMOV HAA55 D20\nSET M1020\nSET M1021\nSET M1022\nLD X0\nSEGD K{value} D20\nEND"), vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2)]));
    }
    for mode in [false, true] {
        let flag = if mode { "SET" } else { "RST" };
        for n in [1, 2, 3, 4, 5, 6, 7, 8, 9, 15, 16, 255, 256] {
            for op in ["ASCI", "HEX"] {
                let init = if op == "ASCI" {
                    "MOV H0123 D0\nMOV H4567 D1\nMOV H89AB D2\nMOV HCDEF D3"
                } else if mode {
                    "MOV H43 D0\nMOV H44 D1\nMOV H45 D2\nMOV H46 D3"
                } else {
                    "MOV H4443 D0\nMOV H4645 D1\nMOV H3938 D2\nMOV H4241 D3"
                };
                cases.push(case(&format!("text_encoding_{op}_mode_{mode}_n_{n}"),&format!("LD X1\n{flag} M1161\nFMOV H3030 D0 K256\nFMOV HAA55 D300 K256\n{init}\nSET M1020\nSET M1021\nSET M1022\nLD X0\n{op} D0 D300 K{n}\nEND"), vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2)]));
            }
        }
        for op in ["SEGD", "ASCI", "ASCII", "HEX", "CCD", "LRC", "CRC"] {
            for pulse in [false, true] {
                let suffix = if pulse { "P" } else { "" };
                let args = match op {
                    "SEGD" => "D0 D20",
                    "ASCI" | "ASCII" | "HEX" => "D0 D20 K6",
                    "CCD" => "D0 D20 D10",
                    _ => "D0 D10 D20",
                };
                cases.push(case(&format!("text_encoding_{op}{suffix}_mode_{mode}_pulse_rearm"),&format!("LD X1\n{flag} M1161\nFMOV H3030 D0 K8\nMOV K6 D10\nFMOV HAA55 D20 K8\nSET M1020\nSET M1021\nSET M1022\nLD X0\n{op}{suffix} {args}\nINC D0\nEND"),vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],3),(vec![("X0",false)],1),(vec![("X0",true)],2)]));
            }
        }
        for op in ["CCD", "LRC", "CRC"] {
            for n in [-1, 0, 1, 2, 3, 6, 255, 256, 257] {
                let args = if op == "CCD" {
                    "D0 D300 D290"
                } else {
                    "D0 D290 D300"
                };
                cases.push(case(&format!("text_encoding_{op}_mode_{mode}_dynamic_{n}"),&format!("LD X1\n{flag} M1161\nFMOV H3030 D0 K256\nMOV K{n} D290\nFMOV HAA55 D300 K4\nSET M1020\nSET M1021\nSET M1022\nLD X0\n{op} {args}\nEND"),vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2)]));
            }
        }
    }
    cases
}

fn bit_tool_cases(target: Target) -> Vec<Case> {
    let mut cases = Vec::new();
    for wide in [false, true] {
        let prefix = if wide { "D" } else { "" };
        let mov = if wide { "DMOV" } else { "MOV" };
        let inc = if wide { "DINC" } else { "INC" };
        let values = if wide {
            vec![0, 1, 6513, 123456789, i32::MAX, -1, i32::MIN]
        } else {
            vec![0, 1, 2, 6513, 32767, -1, -32768]
        };
        for name in ["ABS", "GRY", "GBIN"] {
            for pulse in [false, true] {
                let op = format!("{prefix}{name}{}", if pulse { "P" } else { "" });
                let body = if name == "ABS" {
                    format!("{op} D0")
                } else {
                    format!("{op} D0 D20")
                };
                cases.push(case(&format!("bit_tool_{op}_hold_rearm"),&format!("LD X1\n{mov} K-3 D0\nSET M1020\nSET M1021\nSET M1022\nLD X0\n{body}\n{inc} D0\nOUT Y0\nEND"),
                    vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],5),(vec![("X0",false)],1),(vec![("X0",true)],2)]));
            }
            for (i, v) in values.iter().enumerate() {
                let body = if name == "ABS" {
                    format!("{prefix}{name} D0")
                } else {
                    format!("{prefix}{name} D0 D20")
                };
                cases.push(case(&format!("bit_tool_{prefix}{name}_value_{i}"),&format!("LD X1\n{mov} K{v} D0\nFMOV K77 D20 K3\nSET M1020\nSET M1021\nSET M1022\nLD X0\n{body}\nEND"),
                    vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2)]));
            }
        }
        for name in ["BOUT", "BSET", "BRST"] {
            let nums = if wide {
                vec![-1, 0, 15, 16, 31, 32, 65536, i32::MIN]
            } else {
                vec![-1, 0, 15, 16, 31]
            };
            for n in nums {
                cases.push(case(&format!("bit_tool_{prefix}{name}_index_{n}"),&format!("LD X1\n{mov} K-1 D0\n{mov} K{n} D10\nSET M1020\nSET M1021\nSET M1022\nLD X0\n{prefix}{name} D0 D10\nOUT Y0\nEND"),
                    vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2),(vec![("X0",false)],2)]));
            }
        }
        for name in ["BLD", "BLDI", "BAND", "BANI", "BOR", "BORI"] {
            let load = name.starts_with("BLD");
            let initial = if load { "" } else { "LD X0\n" };
            let gate = if load { "AND X0\n" } else { "" };
            let max = if wide { 31 } else { 15 };
            let seed = if wide {
                "DMOV H80000001 D0\nDMOV K-1 D10"
            } else {
                "MOV H8001 D0\nMOV K-1 D10"
            };
            let mut body = String::new();
            for (i, n) in ["K0".into(), "K1".into(), format!("K{max}"), "D10".into()]
                .iter()
                .enumerate()
            {
                body.push_str(&format!(
                    "{initial}{prefix}{name} D0 {n}\n{gate}OUT Y{i:o}\n"
                ));
            }
            cases.push(case(
                &format!("bit_tool_{prefix}{name}_gates_and_dynamic_wrap"),
                &format!(
                    "LD X1\n{seed}\nSET M1020\nSET M1021\nSET M1022\n{body}LD X2\n{inc} D10\nEND"
                ),
                vec![
                    (vec![("X1", true)], 1),
                    (vec![("X1", false), ("X0", true), ("X2", true)], 3),
                    (vec![("X0", false), ("X2", false)], 1),
                ],
            ));
        }
        for dest in ["D0", "T20", "C20"] {
            cases.push(case(&format!("bit_tool_{prefix}gray_alias_{dest}"),&format!("LD X1\n{mov} K6513 D0\nLD X0\n{prefix}GRY D0 {dest}\n{prefix}GBIN {dest} D30\n{prefix}ABS {dest}\nEND"),vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2)]));
        }
        cases.push(case(&format!("bit_tool_{prefix}indexed_index_destination_alias"),&format!("LD X1\n{mov} K1 D0\nLD X0\n{prefix}BOUT D0 D0\n{prefix}BSET D0 D0\n{prefix}BRST D0 D0\n{prefix}BLD D0 D0\nOUT Y0\nEND"),vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2),(vec![("X0",false)],1)]));
    }
    cases.push(case("bit_tool_natural_counters_and_gray_error_after_index_write",
        "LD X1\nDMOV K6513 C200\nDMOV K-1 C210\nLD X0\nDGRY C200 C201\nDGBIN C201 C202\nDBOUT C200 C210\nDBLD C200 C210\nOUT Y0\nEND",
        vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2),(vec![("X0",false)],1)]));
    let last = if target == Target::Ss2 { 4999 } else { 9999 };
    cases.push(case("bit_tool_model_word_endpoints",
        &format!("LD X0\nDIV16 K-7 K1 D{last}\nABS D{last}\nGRY D{last} T255\nGBIN T255 C199\nBOUT C199 K15\nBLD C199 K15\nOUT Y0\nLD X1\nDMOV K-1234567 D{}\nDABS D{}\nDGRY D{} C255\nDGBIN C255 C200\nDBSET C200 K31\nDBLD C200 K31\nOUT Y1\nEND",last-1,last-1,last-1),vec![(vec![("X0",true),("X1",true)],2),(vec![("X0",false),("X1",false)],1)]));
    cases.push(case("bit_tool_indexed_contacts_nested_blocks_and_mps",
        "LD X1\nMOV H8001 D0\nDMOV H80000001 D10\nLD X0\nBLD D0 K0\nBORI D0 K1\nANB\nMPS\nDBAND D10 K31\nOUT Y0\nMPP\nDBANI D10 K1\nOUT Y1\nEND",
        vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2),(vec![("X0",false)],1)]));
    cases
}

fn data_processing_cases(target: Target) -> Vec<Case> {
    let mut cases = Vec::new();
    for op in ["SUM", "DSUM"] {
        cases.push(case(
            &format!("data_processing_{op}_zero_set_hold_and_explicit_reset"),
            &format!("LD X1\nRST M1020\nLD X0\n{op} D0 D20\nLD X2\nMOV K1 D0\nEND"),
            vec![
                (vec![("X1", true)], 1),
                (vec![("X1", false), ("X0", true)], 1),
                (vec![("X2", true)], 2),
                (vec![("X1", true), ("X2", false)], 1),
            ],
        ));
    }
    for n in 1..8 {
        cases.push(case(
            &format!("data_processing_decode_encode_bit_width_{n}"),
            &format!("LD X7\nDECO X0 M0 K{n}\nENCO M0 D0 K{n}\nEND"),
            vec![
                (vec![("X7", true)], 1),
                (
                    vec![("X0", true), ("X1", true), ("X3", true), ("X5", true)],
                    1,
                ),
            ],
        ));
    }
    for (name, body, seed, increment) in [
        ("DECO", "DECO D0 M0 K4", "MOV K3 D0", "INC D0"),
        ("ENCO", "ENCO D0 D20 K4", "MOV K16 D0", "INC D0"),
        ("SUM", "SUM D0 D20", "MOV K-1 D0", "INC D0"),
        ("DSUM", "DSUM D0 D20", "DMOV K-1 D0", "DINC D0"),
        ("BON", "BON D0 Y0 K3", "MOV K7 D0", "INC D0"),
        ("DBON", "DBON D0 Y0 K31", "DMOV H7FFFFFFF D0", "DINC D0"),
        ("MEAN", "MEAN D0 D20 K2", "MOV K4 D0\nMOV K-1 D1", "INC D0"),
        (
            "DMEAN",
            "DMEAN D0 D20 K2",
            "DMOV K4 D0\nDMOV K-1 D2",
            "DINC D0",
        ),
        ("SQR", "SQR D0 D20", "MOV K4 D0", "INC D0"),
        ("DSQR", "DSQR D0 D20", "DMOV K4 D0", "DINC D0"),
    ] {
        for pulse in [false, true] {
            let body = if pulse {
                body.replacen(name, &format!("{name}P"), 1)
            } else {
                body.into()
            };
            cases.push(case(&format!("data_processing_{name}_{}",if pulse {"pulse"}else{"held"}),
                &format!("LD X1\n{seed}\nSET M1020\nSET M1021\nSET M1022\nLD X0\n{body}\n{increment}\nEND"),
                vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],7),(vec![("X0",false)],1),(vec![("X0",true)],2)]));
        }
    }
    for (op, values) in [
        ("SUM", vec!["K0", "K-1", "H8000", "H5555", "H7FFF"]),
        (
            "DSUM",
            vec!["K0", "K-1", "H80000000", "H55555555", "H7FFFFFFF"],
        ),
        ("SQR", vec!["K0", "K1", "K2", "K-1", "K32761", "K32767"]),
        (
            "DSQR",
            vec!["K0", "K1", "K2", "K-1", "K2147395600", "K2147483647"],
        ),
    ] {
        for (index, value) in values.into_iter().enumerate() {
            cases.push(case(&format!("data_processing_{op}_value_{index}"),&format!("LD X1\nFMOV K12345 D20 K3\nSET M1020\nSET M1021\nSET M1022\nLD X0\n{op} {value} D20\nEND"),
                vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2)]));
        }
    }
    for (op, seed, index_values) in [
        ("BON", "MOV H8008 D0", vec![-1, 0, 3, 15, 16]),
        (
            "DBON",
            "DMOV H80000008 D0",
            vec![-1, 0, 3, 16, 31, 32, 65536],
        ),
    ] {
        for n in index_values {
            let index_seed = if op == "DBON" {
                format!("DMOV K{n} D10")
            } else {
                format!("MOV K{n} D10")
            };
            cases.push(case(
                &format!("data_processing_{op}_dynamic_{n}"),
                &format!("LD X1\n{seed}\n{index_seed}\nSET Y0\nLD X0\n{op} D0 Y0 D10\nEND"),
                vec![
                    (vec![("X1", true)], 1),
                    (vec![("X1", false), ("X0", true)], 2),
                    (vec![("X0", false)], 1),
                ],
            ));
        }
    }
    for wide in [false, true] {
        let (op, mov) = if wide {
            ("DMEAN", "DMOV")
        } else {
            ("MEAN", "MOV")
        };
        for n in [-1, 0, 1, 2, 3, 64, 65] {
            let seed = if wide {
                "DFMOV K-7 D0 K64"
            } else {
                "FMOV K-7 D0 K64"
            };
            cases.push(case(
                &format!("data_processing_{op}_dynamic_{n}"),
                &format!("LD X1\n{seed}\n{mov} K{n} D150\nLD X0\n{op} D0 D200 D150\nEND"),
                vec![
                    (vec![("X1", true)], 1),
                    (vec![("X1", false), ("X0", true)], 2),
                ],
            ));
        }
        for dest in ["D0", "D1", "T20", "C20"] {
            let seed = if wide {
                "DMOV K4 D0\nDMOV K-1 D2\nDMOV K11 D4"
            } else {
                "MOV K4 D0\nMOV K-1 D1\nMOV K11 D2"
            };
            cases.push(case(
                &format!("data_processing_{op}_alias_{dest}"),
                &format!("LD X1\n{seed}\nLD X0\n{op} D0 {dest} K3\nEND"),
                vec![
                    (vec![("X1", true)], 1),
                    (vec![("X1", false), ("X0", true)], 2),
                ],
            ));
        }
    }
    cases.push(case(
        "data_processing_decode_input_bits_maximum_and_highest_encode",
        "LD X7\nDECO X0 M0 K8\nENCO M0 D0 K8\nEND",
        vec![
            (vec![("X7", true)], 1),
            (
                vec![
                    ("X0", true),
                    ("X1", true),
                    ("X2", true),
                    ("X3", true),
                    ("X4", true),
                    ("X5", true),
                    ("X6", true),
                ],
                1,
            ),
        ],
    ));
    cases.push(case("data_processing_decode_alias_and_bit_area_ends",
        "LD X1\nDECO K3 M0 K3\nLD X0\nDECO M0 M0 K3\nENCO M0 D0 K3\nDECO HFFFF Y370 K3\nENCO Y370 T255 K3\nDECO K5 S1016 K3\nENCO S1016 C199 K3\nEND",
        vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2)]));
    for value in ["K0", "K1", "H8001", "HFFFF"] {
        cases.push(case(
            &format!("data_processing_encode_word_{value}"),
            &format!("LD X1\nMOV {value} D0\nMOV K77 D20\nLD X0\nENCO D0 D20 K4\nEND"),
            vec![
                (vec![("X1", true)], 1),
                (vec![("X1", false), ("X0", true)], 2),
            ],
        ));
    }
    for dest in ["D20", "T255", "C199"] {
        cases.push(case(
            &format!("data_processing_decode_word_{dest}"),
            &format!("LD X0\nDECO HFFFF {dest} K4\nENCO {dest} D0 K4\nEND"),
            vec![(vec![("X0", true)], 2)],
        ));
    }
    let last = if target == Target::Ss2 { 4999 } else { 9999 };
    for wide in [false, true] {
        let op = if wide { "DMEAN" } else { "MEAN" };
        let seed = if wide {
            "DMOV K4 D0\nDMOV K-1 D2\nDMOV K11 D4\nDMOV K3 D150"
        } else {
            "MOV K4 D0\nMOV K-1 D1\nMOV K11 D2\nMOV K3 D150"
        };
        cases.push(case(
            &format!("data_processing_{op}_count_destination_alias"),
            &format!("LD X1\n{seed}\nLD X0\n{op} D0 D150 D150\nEND"),
            vec![
                (vec![("X1", true)], 1),
                (vec![("X1", false), ("X0", true)], 3),
            ],
        ));
        cases.push(case(
            &format!("data_processing_{op}_partial_model_range"),
            &format!("LD X1\nDIV16 K7 K1 D{last}\nLD X0\n{op} D{last} D0 K2\nEND"),
            vec![
                (vec![("X1", true)], 1),
                (vec![("X1", false), ("X0", true)], 1),
            ],
        ));
        cases.push(case(
            &format!("data_processing_{op}_partial_negative_word"),
            &format!("LD X1\nDIV16 K-7 K1 D{last}\nLD X0\n{op} D{last} D0 K2\nEND"),
            vec![
                (vec![("X1", true)], 1),
                (vec![("X1", false), ("X0", true)], 1),
            ],
        ));
        cases.push(case(
            &format!("data_processing_{op}_partial_timer_word"),
            &format!("LD X1\nDIV16 K7 K1 T255\nLD X0\n{op} T255 D0 K2\nEND"),
            vec![
                (vec![("X1", true)], 1),
                (vec![("X1", false), ("X0", true)], 1),
            ],
        ));
    }
    cases.push(case("data_processing_natural_counters_and_mixed_counter_range",
        "LD X1\nDMOV HFFFF0001 C200\nDMOV K31 C201\nDIV16 K7 K1 C199\nLD X0\nDSUM C200 C202\nDBON C200 Y0 C201\nDMEAN C199 C203 K2\nEND",
        vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2)]));
    cases
}

/// Deterministic differential corpus for additional data processing. RAND
/// deliberately uses equal bounds/errors: independently timed RNG streams are
/// not an equality oracle, even when their algorithm has been reconstructed.
fn additional_processing_cases(target: Target) -> Vec<Case> {
    let mut cases = Vec::new();
    let stages = || {
        vec![
            (vec![("X1", true)], 1),
            (vec![("X1", false), ("X0", true)], 2),
            (vec![("X0", false)], 1),
            (vec![("X0", true)], 1),
        ]
    };
    let flags = "SET M1020\nSET M1021\nSET M1022\nMOV H1234 D1067\nMOV K77 D1068";
    let observed = "LD M1020\nOUT Y0\nLD M1021\nOUT Y1\nLD M1022\nOUT Y2\nLD M1067\nOUT Y3\nLD M1068\nOUT Y4\nLDI X7\nMOV D1067 D300\nMOV D1068 D301";
    let mut add = |name: &str, seed: &str, body: &str| {
        cases.push(case(
            &format!("additional_processing_{name}"),
            &format!(
                "LD X1\n{seed}\n{flags}\nLD X0\n{body}\nMOV K123 D302\nINC D303\n{observed}\nEND"
            ),
            stages(),
        ));
    };
    for (op, seed) in [
        ("SWAP", "MOV H12AB D0"),
        ("DSWAP", "DMOV H12AB34CD D0"),
        ("MVM", "MOV HAA55 D0\nMOV HFF00 D4\nMOV H1234 D20"),
        (
            "DMVM",
            "DMOV HAA55CC33 D0\nDMOV HFF0000FF D4\nDMOV H12345678 D20",
        ),
        ("WSUM", "MOV K-7 D0\nMOV K11 D1\nMOV K-3 D2"),
        ("DWSUM", "DMOV K-1234567 D0\nDMOV K2000000 D2\nDMOV K-3 D4"),
        ("SCAL", "MOV K501 D0\nMOV K-168 D4\nMOV K534 D8"),
        (
            "SCLP",
            "MOV K1 D0\nMOV K2 D4\nMOV K0 D5\nMOV K3 D6\nMOV K0 D7",
        ),
        (
            "DSCLP",
            "DMOV K1 D0\nDMOV K2 D4\nDMOV K0 D6\nDMOV K3 D8\nDMOV K0 D10",
        ),
        (
            "CMPT",
            "MOV K-3 D0\nMOV K1 D1\nMOV K7 D2\nMOV K-2 D10\nMOV K1 D11\nMOV K6 D12",
        ),
        (
            "DCMPT",
            "DMOV K-3 D0\nDMOV K1 D2\nDMOV K7 D4\nDMOV K-2 D10\nDMOV K1 D12\nDMOV K6 D14",
        ),
    ] {
        let args = match op {
            "SWAP" | "DSWAP" => "D0",
            "MVM" | "DMVM" => "D0 D4 D20",
            "WSUM" | "DWSUM" => "D0 K3 D20",
            "SCAL" => "D0 D4 D8 D20",
            "SCLP" | "DSCLP" => "D0 D4 D20",
            _ => "D0 D10 H0103 D20",
        };
        for pulse in [false, true] {
            let full = format!("{op}{}", if pulse { "P" } else { "" });
            add(
                &format!("lifecycle_{full}"),
                seed,
                &format!(
                    "{full} {args}\n{} D0",
                    if op.starts_with('D') { "DINC" } else { "INC" }
                ),
            );
        }
    }
    for (op, mov, value) in [("SWAP", "MOV", "HAB12"), ("DSWAP", "DMOV", "HAB12CD34")] {
        for dest in ["D20", "T20", "C20", "C200"] {
            if op == "SWAP" && dest == "C200" {
                continue;
            }
            add(
                &format!("swap_{op}_{dest}"),
                &if op == "SWAP" && !dest.starts_with('D') {
                    format!("DIV16 {value} K1 {dest}")
                } else {
                    format!("{mov} {value} {dest}")
                },
                &format!("{op} {dest}"),
            );
        }
    }
    for (op, group, width) in [
        ("SWAP", "K1M100", 4),
        ("SWAP", "K4M100", 16),
        ("DSWAP", "K5M100", 20),
        ("DSWAP", "K8M100", 32),
    ] {
        let mut seed = String::new();
        for bit in 0..width {
            if 0x89ABCDEFu32 & (1 << bit) != 0 {
                seed.push_str(&format!("\nSET M{}", 100 + bit));
            }
        }
        add(
            &format!("swap_group_{op}_{group}"),
            &seed,
            &format!("{op} {group}"),
        );
    }
    for wide in [false, true] {
        let (op, mov, value, mask, old) = if wide {
            ("DMVM", "DMOV", "H89ABCDEF", "HFF00F0F0", "H12345678")
        } else {
            ("MVM", "MOV", "H89AB", "HFF00", "H1234")
        };
        for mask in ["K0", "K-1", mask] {
            for dest in ["D0", "D1", "D4", "D20", "T20", "C20"] {
                add(
                    &format!("mask_{op}_{mask}_{dest}"),
                    &format!("{mov} {value} D0\n{mov} {old} D4\n{mov} {old} D20"),
                    &format!("{op} D0 {mask} {dest}"),
                );
            }
        }
        let (group, raw) = if wide {
            ("K8M100", 0x89ABCDEFu32)
        } else {
            ("K4M100", 0x89ABu32)
        };
        let mut seed = format!("{mov} {old} D20");
        for bit in 0..if wide { 32 } else { 16 } {
            if raw & (1 << bit) != 0 {
                seed.push_str(&format!("\nSET M{}", 100 + bit));
            }
        }
        add(
            &format!("group_{op}"),
            &seed,
            &format!("{op} {group} K-1 D20\n{op} D20 {mask} {group}"),
        );
        add(
            &format!("group_input_{op}"),
            &format!("{mov} K-1 D20"),
            &format!("{op} K1X0 K-1 D20\n{op} D20 K-1 K1M100"),
        );
    }
    for wide in [false, true] {
        let (op, mov, seed) = if wide {
            (
                "DWSUM",
                "DMOV",
                "DMOV H7FFFFFFF D0\nDMOV K1 D2\nDMOV K-7 D4",
            )
        } else {
            ("WSUM", "MOV", "MOV K-32768 D0\nMOV K32767 D1\nMOV K-7 D2")
        };
        for count in [-1, 0, 1, 2, 3, 64, 65] {
            add(
                &format!("sum_{op}_count_{count}"),
                &format!("{seed}\n{mov} K{count} D150\nDMOV H12345678 D20"),
                &format!("{op} D0 D150 D20"),
            );
        }
        for dest in ["D0", "D1", "D2", "D150", "T20", "C20", "C200"] {
            add(
                &format!("sum_{op}_alias_{dest}"),
                &format!("{seed}\n{mov} K3 D150"),
                &format!("{op} D0 D150 {dest}"),
            );
        }
        let last = if target == Target::Ss2 { 4999 } else { 9999 };
        for source in [format!("D{last}"), "T255".into(), "C199".into()] {
            add(
                &format!("sum_{op}_partial_source_{source}"),
                &format!("DIV16 K-7 K1 {source}"),
                &format!("{op} {source} K3 D20"),
            );
        }
        for dest in [format!("D{last}"), "T255".into(), "C199".into()] {
            add(
                &format!("sum_{op}_partial_destination_{dest}"),
                seed,
                &format!("{op} D0 K3 {dest}"),
            );
        }
    }
    add(
        "sum_natural_counter_sequence",
        "DIV16 K-7 K1 C199\nDMOV K100000 C200\nDMOV K-300000 C201",
        "DWSUM C199 K3 D20\nDWSUM C200 K2 C202",
    );
    for (index, (source, slope, offset)) in [
        (500, 168, -4),
        (500, -168, 534),
        (1, 500, 0),
        (-1, 500, 0),
        (32767, 32767, 32767),
        (-32768, 32767, -32768),
        (-32768, -32768, 32767),
        (32767, -32768, -32768),
    ]
    .into_iter()
    .enumerate()
    {
        add(
            &format!("scal_signed_{index}"),
            "MOV K555 D20",
            &format!("SCAL K{source} K{slope} K{offset} D20"),
        );
    }
    for (index, [source, max_source, min_source, max_dest, min_dest]) in [
        [500, 3000, 200, 500, 30],
        [500, 3000, 200, 30, 500],
        [1, 2, 0, 3, 0],
        [-1, 0, -2, 0, -3],
        [-5, 10, 0, 100, 0],
        [15, 10, 0, 100, 0],
        [5, 0, 10, 100, 0],
        [5, 5, 5, 100, 0],
        [32767, 32767, -32768, 32767, -32768],
        [0, 32767, -32768, -32768, 32767],
    ]
    .into_iter()
    .enumerate()
    {
        for dest in ["D20", "D4", "D5"] {
            add(
                &format!("sclp_word_{index}_{dest}"),
                &format!(
                    "MOV K{source} D0\nMOV K{max_source} D4\nMOV K{min_source} D5\nMOV K{max_dest} D6\nMOV K{min_dest} D7\nMOV K555 D20"
                ),
                &format!("SCLP D0 D4 {dest}"),
            );
        }
    }
    for (index, [source, max_source, min_source, max_dest, min_dest]) in [
        [500, 3000, 200, 500, 30],
        [500, 3000, 200, 30, 500],
        [1, 2, 0, 3, 0],
        [-1, 0, -2, 0, -3],
        [-5, 10, 0, 100, 0],
        [15, 10, 0, 100, 0],
        [5, 0, 10, 100, 0],
        [5, 5, 5, 100, 0],
        [2147483647, 2147483647, -2147483648, 2147483647, -2147483648],
        [123456789, 2000000000, 0, 2000000000, 0],
        [16777217, 33554432, 0, 33554431, 0],
    ]
    .into_iter()
    .enumerate()
    {
        add(
            &format!("sclp_double_integer_{index}"),
            &format!(
                "RST M1162\nDMOV K{source} D0\nDMOV K{max_source} D4\nDMOV K{min_source} D6\nDMOV K{max_dest} D8\nDMOV K{min_dest} D10\nDMOV H12345678 D20"
            ),
            "DSCLP D0 D4 D20",
        );
    }
    for (index, [source, max_source, min_source, max_dest, min_dest]) in [
        [500.0f32, 3000.0, 200.0, 500.0, 30.0],
        [500.0, 3000.0, 200.0, 30.0, 500.0],
        [1.0, 2.0, 0.0, 3.0, 0.0],
        [-1.0, 0.0, -2.0, 0.0, -3.0],
        [-5.0, 10.0, 0.0, 100.0, 0.0],
        [15.0, 10.0, 0.0, 100.0, 0.0],
        [5.0, 0.0, 10.0, 100.0, 0.0],
        [5.0, 5.0, 5.0, 100.0, 0.0],
        [0.1, 0.3, -0.7, 2.2, -1.3],
        [1e20, 3e20, 0.0, 7e20, -2e20],
    ]
    .into_iter()
    .enumerate()
    {
        let mut seed = "SET M1162\nDMOV H12345678 D20".to_string();
        for (value, dest) in [
            (source, 0),
            (max_source, 4),
            (min_source, 6),
            (max_dest, 8),
            (min_dest, 10),
        ] {
            seed.push_str(&format!("\nDMOV H{:08X} D{dest}", value.to_bits()));
        }
        add(
            &format!("sclp_double_float_{index}"),
            &seed,
            "DSCLP D0 D4 D20",
        );
    }
    for (op, block) in [("SCLP", 9997), ("DSCLP", 9993)] {
        // The decoder's fixed block bound is 9996/9992 even on SS2, whose
        // usable data range is smaller; use target-local endpoints separately.
        if target != Target::Ss2 {
            add(
                &format!("sclp_invalid_block_{op}"),
                "DMOV H12345678 D20",
                &format!("{op} K1 D{block} D20"),
            );
        }
    }
    for wide in [false, true] {
        let (op, mov, stride, max_count) = if wide {
            ("DCMPT", "DMOV", 2, 32)
        } else {
            ("CMPT", "MOV", 1, 16)
        };
        let mut seed = "DMOV HFFFFFFFF D100".to_string();
        for (index, (a, b)) in [(-7, -8), (-1, -1), (0, 1), (1, 0), (32767, -32768), (7, 7)]
            .into_iter()
            .enumerate()
        {
            seed.push_str(&format!(
                "\n{mov} K{a} D{}\n{mov} K{b} D{}",
                index * stride,
                40 + index * stride
            ));
        }
        for mode in [0, 1, 2, 3, 4, 5, 255] {
            for count in [0, 1, 6, max_count, 255] {
                add(
                    &format!("cmpt_{op}_mode_{mode}_count_{count}"),
                    &seed,
                    &format!("{op} D0 D40 H{:04X} D100", (mode << 8) | count),
                );
            }
        }
        for dest in ["D0", "D1", "D40", "T20", "C20", "K4M100"] {
            add(
                &format!("cmpt_{op}_alias_{dest}"),
                &seed,
                &format!("{op} D0 D40 H0306 {dest}"),
            );
        }
        let last = if target == Target::Ss2 { 4999 } else { 9999 };
        add(
            &format!("cmpt_{op}_partial_source"),
            &format!("DIV16 K-7 K1 D{last}\nDIV16 K-7 K1 T255\nDMOV HFFFFFFFF D100"),
            &format!("{op} D{last} T255 K3 D100"),
        );
    }
    for enabled in [false, true] {
        add(
            &format!("cmpt_double_mode1162_{enabled}"),
            &format!(
                "{} M1162\nDMOV HBF800000 D0\nDMOV H40000000 D2\nDMOV H00000000 D4\nDMOV HC0000000 D40\nDMOV H3F800000 D42\nDMOV H80000000 D44",
                if enabled { "SET" } else { "RST" }
            ),
            "DCMPT D0 D40 H0103 D100\nDCMPT D0 D40 H12340303 D102",
        );
    }
    for wide in [false, true] {
        let (op, mov, maximum) = if wide {
            ("DRAND", "DMOV", 2147483647)
        } else {
            ("RAND", "MOV", 32767)
        };
        for (index, (low, high)) in [(0, 0), (1, 1), (maximum, maximum), (-1, 1), (0, -1), (2, 1)]
            .into_iter()
            .enumerate()
        {
            for pulse in [false, true] {
                let full = format!("{op}{}", if pulse { "P" } else { "" });
                add(
                    &format!("random_{full}_{index}"),
                    &format!("{mov} K{low} D0\n{mov} K{high} D4\nDMOV H12345678 D20"),
                    &format!("{full} D0 D4 D20"),
                );
            }
        }
    }
    cases
}

fn numeric_contact_cases(target: Target) -> Vec<Case> {
    let mut cases = Vec::new();
    let stages = vec![
        (vec![("X1", true)], 1),
        (vec![("X1", false), ("X0", true)], 2),
        (vec![("X0", false)], 1),
    ];
    for wide in [false, true] {
        let d = if wide { "D" } else { "" };
        let (min, max, full) = if wide {
            ("K-2147483648", "K2147483647", "HFFFFFFFF")
        } else {
            ("K-32768", "K32767", "HFFFF")
        };
        for prefix in ["LD", "AND", "OR"] {
            for symbol in ["&", "|", "^"] {
                let mut source = "LD X1\nSET M1020\nSET M1021\nSET M1022\n".to_owned();
                for (index, (a, b)) in [
                    ("K0", "K0"),
                    ("K1", "K2"),
                    ("K1", "K1"),
                    (min, min),
                    (full, "K0"),
                    (min, full),
                    (full, full),
                    ("K0", full),
                ]
                .into_iter()
                .enumerate()
                {
                    if prefix != "LD" {
                        source.push_str("LD X0\n");
                    }
                    source.push_str(&format!("{d}{prefix}{symbol} {a} {b}\n"));
                    if prefix == "LD" {
                        source.push_str("AND X0\n");
                    }
                    source.push_str(&format!("OUT Y{index:o}\n"));
                }
                source.push_str("END");
                cases.push(case(
                    &format!(
                        "numeric_contact_{d}{prefix}_logic_{}",
                        match symbol {
                            "&" => "and",
                            "|" => "or",
                            _ => "xor",
                        }
                    ),
                    &source,
                    stages.clone(),
                ));
            }
            for (relation_index, symbol) in
                [">", ">=", "<", "<=", "=", "<>"].into_iter().enumerate()
            {
                let mut source = "LD X1\nSET M1020\nSET M1021\nSET M1022\n".to_owned();
                for (index, (a, b, c)) in [
                    ("K170", "K200", "K-30"),
                    ("K231", "K200", "K30"),
                    ("K400", "K200", "K200"),
                    ("K0", "K0", "K0"),
                    (max, min, "K1"),
                    (min, "K0", "K0"),
                    ("K0", "K0", min),
                    (max, "K0", max),
                    (min, max, "K-1"),
                    ("K-200", "K400", "K600"),
                ]
                .into_iter()
                .enumerate()
                {
                    if prefix != "LD" {
                        source.push_str("LD X0\n");
                    }
                    source.push_str(&format!("{d}{prefix}Z{symbol} {a} {b} {c}\n"));
                    if prefix == "LD" {
                        source.push_str("AND X0\n");
                    }
                    source.push_str(&format!("OUT Y{index:o}\n"));
                }
                source.push_str("END");
                cases.push(case(
                    &format!("numeric_contact_{d}{prefix}_difference_{relation_index}"),
                    &source,
                    stages.clone(),
                ));
            }
        }
    }
    cases.push(case("numeric_contact_register_values_mps_edges_and_mutation",
        "LD X1\nMOV K170 D0\nMOV K200 D1\nDIV16 K-30 K1 T255\nDMOV K-1234567 D10\nDMOV K-1234500 C200\nDMOV K-67 C201\nLD X0\nMPS\nANDZ<= D0 D1 T255\nOUT Y0\nMPP\nDANDZ= D10 C200 C201\nOUT Y1\nLD& D0 D1\nMPS\nAND X0\nNP\nINC D20\nMPP\nORZ>= D0 D1 K30\nOUT Y2\nLD X2\nINC D0\nDINC D10\nEND",
        vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2),(vec![("X2",true)],2),(vec![("X0",false),("X2",false)],1)]));
    cases.push(case("numeric_contact_load_stack_and_branch_result",
        "LD X0\nLD| K0 K1\nLDZ= K400 K200 K-200\nANB\nORB\nOUT Y0\nLD X0\nDLD& H80000000 H80000000\nDORZ< K-2147483648 K0 K1\nANB\nINV\nOUT Y1\nEND",
        vec![(vec![],1),(vec![("X0",true)],2),(vec![("X0",false)],1)]));
    cases.push(case("numeric_contact_orz_padding_error_step_and_following_actions",
        "LD X0\nORZ>= K1 K2 K0\nDIV16 K1 K0 D0\nINC D2\nLD X1\nORZ= K1 K2 K1\nDIV32 K1 K0 D4\nINC D6\nEND",
        vec![(vec![],1),(vec![("X0",true),("X1",true)],2),(vec![("X0",false),("X1",false)],1)]));
    let last = if target == Target::Ss2 { 4999 } else { 9999 };
    cases.push(case("numeric_contact_model_word_and_counter_endpoints",
        &format!("LD X0\nDIV16 K-7 K1 D{last}\nDIV16 K-7 K1 C199\nDMOV K-2147483648 C255\nDMOV K-2147483648 D{}\nLD& D{last} C199\nOUT Y0\nLDZ= D{last} C199 K0\nOUT Y1\nDLD^ C255 D{}\nOUT Y2\nDLDZ= C255 D{} K0\nOUT Y3\nEND",last-1,last-1,last-1),
        vec![(vec![("X0",true)],2),(vec![("X0",false)],1)]));
    cases
}

fn truncated_cases(target: Target) -> Vec<Case> {
    let mut cases = Vec::new();
    for (op, pairs) in [
        (
            "MUL16",
            vec![
                (32767, 32767),
                (-32768, -1),
                (-32768, 1),
                (-32768, 32767),
                (-17, 21),
                (0, -1),
                (181, 181),
            ],
        ),
        (
            "DIV16",
            vec![
                (-32768, -1),
                (-32768, 1),
                (32767, -7),
                (-17, 3),
                (17, -3),
                (-17, -3),
                (0, 1),
                (7, 0),
            ],
        ),
        (
            "MUL32",
            vec![
                (i32::MAX, i32::MAX),
                (i32::MIN, -1),
                (i32::MIN, 1),
                (65536, 65536),
                (-17, 21),
                (0, -1),
                (46340, 46340),
            ],
        ),
        (
            "DIV32",
            vec![
                (i32::MIN, 1),
                (i32::MAX, -7),
                (-123456789, 31),
                (123456789, -31),
                (-17, -3),
                (0, 1),
                (7, 0),
            ],
        ),
    ] {
        for (index, (a, b)) in pairs.into_iter().enumerate() {
            cases.push(case(
                &format!("truncated_{op}_signed_{index}"),
                &format!("LD X1\nFMOV K12345 D10 K4\nSET M1020\nSET M1021\nSET M1022\nLD X0\n{op} K{a} K{b} D10\nEND"),
                vec![(vec![("X1", true)], 1), (vec![("X1", false), ("X0", true)], 2), (vec![("X0", false)], 1)],
            ));
        }
    }
    for op in ["MUL16", "MUL32"] {
        for mask in 0..4 {
            let errors = format!(
                "{} M1067\n{} M1068",
                if mask & 1 != 0 { "SET" } else { "RST" },
                if mask & 2 != 0 { "SET" } else { "RST" }
            );
            let factors = if op == "MUL32" {
                "K2147483647 K2"
            } else {
                "K32767 K32767"
            };
            cases.push(case(
                &format!("truncated_{op}_error_flags_{mask}"),
                &format!("LD X1\nSET M1020\nSET M1021\nSET M1022\n{errors}\nLD X0\n{op} {factors} D0\nLD M1022\nOUT Y0\nEND"),
                vec![(vec![("X1", true)], 1), (vec![("X1", false), ("X0", true)], 2)],
            ));
        }
    }
    for op in ["MUL16", "DIV16", "MUL32", "DIV32"] {
        let wide = op.ends_with("32");
        let seed = if wide {
            "DMOV K-1234567 D0\nDMOV K31 D4"
        } else {
            "MOV K-12345 D0\nMOV K31 D4"
        };
        cases.push(case(
            &format!("truncated_{op}_pulse_and_alias"),
            &format!(
                "LD X1\n{seed}\nFMOV K555 D10 K4\nLD X0\n{op}P D0 D4 D10\n{} D0\nEND",
                if wide { "DINC" } else { "INC" }
            ),
            vec![
                (vec![("X1", true)], 1),
                (vec![("X1", false), ("X0", true)], 3),
                (vec![("X0", false)], 1),
                (vec![("X0", true)], 2),
            ],
        ));
        for dest in ["D0", "D1", "D4", "T20", "C20"] {
            cases.push(case(
                &format!("truncated_{op}_destination_{dest}"),
                &format!("LD X1\n{seed}\nDIV16 K77 K1 T22\nDIV16 K88 K1 C22\nLD X0\n{op} D0 D4 {dest}\nEND"),
                vec![
                    (vec![("X1", true)], 1),
                    (vec![("X1", false), ("X0", true)], 2),
                ],
            ));
        }
    }
    cases.push(case(
        "truncated_zero_divisor_error_latches_and_reset",
        "LD X0\nDIV16 K1 K0 D0\nLD X1\nDIV32 K2 K0 D2\nLD X2\nRST M1067\nLD X3\nRST M1068\nEND",
        vec![
            (vec![("X0", true)], 1),
            (vec![("X0", false), ("X1", true)], 1),
            (vec![("X2", true), ("X3", true)], 1),
            (vec![("X2", false), ("X3", false)], 1),
        ],
    ));
    cases.push(case("truncated_counter_width_boundary",
        "LD X1\nDMOV K12345678 C200\nDIV16 K77 K1 C199\nLD X0\nMUL16 K123 K234 C199\nDIV16 C199 K-7 T255\nMUL32 C200 K-31 C201\nDIV32 C201 K7 C255\nMUL32 C199 K-13 C199\nDIV32 C199 K3 D0\nEND",
        vec![(vec![("X1",true)],1),(vec![("X1",false),("X0",true)],2)]));
    let last = if target == Target::Ss2 { 4999 } else { 9999 };
    cases.push(case("truncated_model_word_endpoints",
        &format!("LD X0\nMUL16 K7 K9 D{last}\nDIV16 D{last} K-2 T255\nMUL16P T255 K3 C199\nMUL32 K-123456 K31 D{}\nDIV32P D{} K-3 C255\nEND",last-1,last-1),
        vec![(vec![("X0",true)],2),(vec![("X0",false)],1),(vec![("X0",true)],1)]));
    cases
}

#[derive(Serialize)]
struct Difference {
    scan: u64,
    reading: Reading,
}
#[derive(Serialize)]
struct RtcLiteralCertificate {
    scope: &'static str,
    checked_scans: Vec<u64>,
    invalidated_at_scan: Option<u64>,
    oracle_count: usize,
    failure: Option<String>,
}
#[derive(Serialize)]
struct CaseReport {
    rtc_fixture: Option<indexed_rtc_cases::NativeFixture>,
    rtc: Option<opendvp::es2_bridge::RtcComparisonEvidence>,
    rtc_literal_certificate: Option<RtcLiteralCertificate>,
    verified_rtc_gates: u64,
    verified_segment_gates: u64,
    verified_checksum_gates: u64,
    verified_text_gates: u64,
    verified_additional_gates: u64,
    name: String,
    source: String,
    status: String,
    scans: u64,
    checks: usize,
    differences: Vec<Difference>,
    last: Vec<Reading>,
    error: Option<String>,
    trace: Vec<opendvp::es2_bridge::Trace>,
    verified_instruction_steps: u64,
    verified_random_gates: u64,
    verified_calendar_gates: u64,
    predicted_control_steps: u64,
    teaching_tick_inputs: Vec<opendvp::es2_bridge::TeachingTickInput>,
    clock_startup_input: Option<opendvp::es2_bridge::ClockStartupInput>,
}
fn run(session: &mut Session, c: &Case) -> CaseReport {
    let mut report = CaseReport {
        rtc_fixture: c.rtc.clone(),
        rtc: None,
        rtc_literal_certificate: c.rtc.as_ref().map(|f|RtcLiteralCertificate{
            scope:"Fixed fixture literals only while no advancing environmental event has been admitted; independent conditional parity is reported separately",
            checked_scans:Vec::new(),invalidated_at_scan:None,oracle_count:f.fixed_oracles.len(),failure:None,
        }),
        verified_rtc_gates: 0,
        verified_segment_gates: 0,
        verified_checksum_gates: 0,
        verified_text_gates: 0,
        verified_additional_gates: 0,
        name: c.name.clone(),
        source: c.source.clone(),
        status: "error".into(),
        scans: 0,
        checks: 0,
        differences: Vec::new(),
        last: Vec::new(),
        error: None,
        trace: Vec::new(),
        verified_instruction_steps: 0,
        verified_random_gates: 0,
        verified_calendar_gates: 0,
        predicted_control_steps: 0,
        teaching_tick_inputs: Vec::new(),
        clock_startup_input: None,
    };
    let mut uploaded = false;
    let result = (|| -> Result<(), String> {
        let p = plc::compile(&c.source).map_err(|e| e.to_string())?;
        let mut inputs: BTreeMap<Device, bool> = p
            .devices
            .iter()
            .filter(|d| d.area == Area::X)
            .map(|d| (*d, false))
            .collect();
        session.upload(&c.source)?;
        uploaded = true;
        let primary_error_pc = if let Some(fixture) = &c.rtc {
            let pc = fixture.primary_error_pc(&p)?;
            session.configure_rtc_comparison(
                fixture.initial_input()?,
                opendvp::es2_bridge::RtcComparisonBudgets {
                    max_capture_us: 200_000,
                    max_observation_us: 100_000,
                    max_window_us: 250_000,
                    max_between_us: 950_000,
                    latest_start_phase_us: 600_000,
                    continuous_host_clock_assumed: true,
                },
            )?;
            pc
        } else {
            None
        };
        for (changes, scans) in &c.stages {
            for (d, v) in changes {
                inputs.insert(d.parse()?, *v);
            }
            for _ in 0..*scans {
                let snapshot =
                    session.step(&inputs.iter().map(|(d, v)| (*d, *v)).collect::<Vec<_>>())?;
                report.scans = snapshot.scan;
                if p.requires_vm() {
                    report.predicted_control_steps += snapshot.local.control.steps;
                }
                report.checks += snapshot.rows.len();
                st_cases::validate_semantics(&c.name, snapshot.scan, &snapshot.rows)?;
                dcnt_cases::validate_semantics(&c.name, snapshot.scan, &snapshot.rows)?;
                if c.name.starts_with("indexed_segment_") {
                    for index in 840..=847 {
                        let device = Device {
                            area: Area::M,
                            index,
                        };
                        let row = snapshot
                            .rows
                            .iter()
                            .find(|r| r.device == device && r.kind == "bit")
                            .ok_or("SEGD literal marker observation missing")?;
                        if row.local != 1 || row.original != 1 {
                            return Err(format!(
                                "SEGD independent literal/guard {device}: local{},native{}",
                                row.local, row.original
                            ));
                        }
                    }
                    let row = snapshot
                        .rows
                        .iter()
                        .find(|r| r.device == "D308".parse::<Device>().unwrap() && r.kind == "word")
                        .ok_or("SEGD literal marker word missing")?;
                    if row.local != 255 || row.original != 255 {
                        return Err(format!(
                            "SEGD independent marker word: local{},native{}",
                            row.local, row.original
                        ));
                    }
                }

                if c.name.starts_with("indexed_checksum_") && c.name.contains("literal_oracle_") {
                    for index in 840..=847 {
                        let device = Device {
                            area: Area::M,
                            index,
                        };
                        let row = snapshot
                            .rows
                            .iter()
                            .find(|r| r.device == device && r.kind == "bit")
                            .ok_or("Checksum independent marker missing")?;
                        if row.local != 1 || row.original != 1 {
                            return Err(format!(
                                "Checksum literal {device}: local{},native{}",
                                row.local, row.original
                            ));
                        }
                    }
                    let row = snapshot
                        .rows
                        .iter()
                        .find(|r| r.device == "D308".parse::<Device>().unwrap() && r.kind == "word")
                        .ok_or("Checksum literal marker word missing")?;
                    if row.local != 255 || row.original != 255 {
                        return Err(format!(
                            "Checksum marker word: local{},native{}",
                            row.local, row.original
                        ));
                    }
                }

                if c.name.starts_with("indexed_text_") && c.name.contains("literal_oracle_") {
                    for index in 840..=847 {
                        let device = Device {
                            area: Area::M,
                            index,
                        };
                        let row = snapshot
                            .rows
                            .iter()
                            .find(|r| r.device == device && r.kind == "bit")
                            .ok_or("Text independent marker missing")?;
                        if row.local != 1 || row.original != 1 {
                            return Err(format!(
                                "Text literal {device}: local{},native{}",
                                row.local, row.original
                            ));
                        }
                    }
                    let row = snapshot
                        .rows
                        .iter()
                        .find(|r| r.device == "D308".parse::<Device>().unwrap() && r.kind == "word")
                        .ok_or("Text literal marker word missing")?;
                    if row.local != 255 || row.original != 255 {
                        return Err(format!(
                            "Text marker word: local{},native{}",
                            row.local, row.original
                        ));
                    }
                }
                if let Some(fixture) = &c.rtc {
                    let certificate = report
                        .rtc_literal_certificate
                        .as_mut()
                        .expect("RTC certificate metadata");
                    let evidence = session
                        .rtc_comparison_evidence()
                        .ok_or("RTC evidence missing")?;
                    if evidence.input_ledger.len() > 1 {
                        certificate.invalidated_at_scan.get_or_insert(snapshot.scan);
                    }
                    if certificate.invalidated_at_scan.is_none() {
                        if let Err(error) =
                            fixture.validate_fixed_oracles(&c.name, snapshot.scan, &snapshot.rows)
                        {
                            certificate.failure = Some(error.clone());
                            return Err(format!("RTC fixed-literal oracle failure: {error}"));
                        }
                        certificate.checked_scans.push(snapshot.scan);
                    }
                    if let Some(expected) = primary_error_pc
                        && snapshot.local.memory.system_bit(1067)
                    {
                        let pc = snapshot
                            .rows
                            .iter()
                            .find(|r| {
                                r.device == "D1068".parse::<Device>().unwrap() && r.kind == "word"
                            })
                            .ok_or("RTC error PC observation missing")?;
                        if pc.local != u32::from(expected) || pc.original != u32::from(expected) {
                            return Err(format!(
                                "RTC independently encoded error PC expected{expected}, local{}, native{}",
                                pc.local, pc.original
                            ));
                        }
                    }
                }
                for r in &snapshot.rows {
                    if r.local != r.original {
                        report.differences.push(Difference {
                            scan: snapshot.scan,
                            reading: r.clone(),
                        });
                    }
                }
                report.last = snapshot.rows;
            }
        }
        Ok(())
    })();
    if uploaded {
        report.trace = session.trace().to_vec();
        report.verified_instruction_steps = session.verified_instruction_steps();
        report.verified_random_gates = session.verified_random_gates();
        report.verified_calendar_gates = session.verified_calendar_gates();
        report.verified_rtc_gates = session.verified_rtc_gates();
        report.verified_segment_gates = session.verified_segment_gates();
        report.verified_checksum_gates = session.verified_checksum_gates();
        report.verified_text_gates = session.verified_text_gates();
        report.verified_additional_gates = session.verified_additional_gates();
        report.rtc = session.rtc_comparison_evidence().cloned();
        report.teaching_tick_inputs = session.teaching_tick_inputs().to_vec();
        report.clock_startup_input = session.clock_startup_input().cloned();
    }
    match result {
        Ok(()) => {
            report.status = if report.differences.is_empty() {
                "match"
            } else {
                "mismatch"
            }
            .into()
        }
        Err(e) => report.error = Some(e),
    }
    report
}
fn main() -> Result<(), String> {
    let mut path = PathBuf::from(DEFAULT_SIMULATOR);
    let mut output = PathBuf::from("es2-comparison.json");
    let mut project = None;
    let mut scans = 10usize;
    let mut case_prefix = None;
    let mut case_name = None;
    let mut unresponsive_probes = false;
    let mut risky_probes = false;
    let mut teaching_ticks = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--unresponsive-probes" => unresponsive_probes = true,
            "--risky-probes" => risky_probes = true,
            "--teaching-ticks" => {
                teaching_ticks = Some(
                    args.next()
                        .ok_or("--teaching-ticks needs 0..255")?
                        .parse::<u8>()
                        .map_err(|e| e.to_string())?,
                );
            }
            "--simulator" => path = PathBuf::from(args.next().ok_or("--simulator needs a path")?),
            "--output" => output = PathBuf::from(args.next().ok_or("--output needs a path")?),
            "--project" => {
                project = Some(PathBuf::from(args.next().ok_or("--project needs a path")?))
            }
            "--scans" => {
                scans = args
                    .next()
                    .ok_or("--scans needs a number")?
                    .parse::<usize>()
                    .map_err(|e| e.to_string())?
                    .min(1000)
            }
            "--case-prefix" => {
                case_prefix = Some(args.next().ok_or("--case-prefix needs a prefix")?)
            }
            "--case-name" => {
                case_name = Some(args.next().ok_or("--case-name needs an exact name")?)
            }
            "--help" => {
                println!(
                    "es2_compare [--simulator EXE] [--output JSON] [--project ODVP_OR_IL --scans N] [--case-prefix PREFIX] [--case-name EXACT] [--unresponsive-probes] [--risky-probes] [--teaching-ticks 0..255]\nWithout --project, runs the isolated reference suite. Each case cold-restarts the owned original. Known nonresponsive probes are retained separately; --unresponsive-probes includes them. --risky-probes adds the explicit DTM raw NaN/infinity corpus. --teaching-ticks explicitly replays pending 100ms teaching-timer inputs at END/FEND; this is controlled algorithm comparison, not natural host-clock parity."
                );
                return Ok(());
            }
            _ => return Err(format!("Unknown argument: {a}")),
        }
    }
    let mut session = Session::launch(Path::new(&path), Arc::new(AtomicBool::new(false)))?;
    session.set_teaching_tick_replay(teaching_ticks)?;
    let mut cases = if let Some(p) = project {
        let p = Project::load(&p)?;
        // Explicit project language also covers ST bodies beginning with IF
        // or a loop, where source auto-detection deliberately stays conservative.
        let source = p.instruction_list().map_err(|e| e.to_string())?;
        vec![Case {
            rtc: None,
            name: p.name,
            source,
            stages: vec![(vec![], scans)],
        }]
    } else {
        let mut cases = suite(session.target());
        if session.sha256() == opendvp::es2_bridge::COMMGR_ES2_SHA256 {
            cases.extend(dtm_modern_cases::cases());
            cases.extend(dtm_solar_cases::cases());
        }
        cases
    };
    if risky_probes {
        cases.extend(data_transform_cases::risky_cases(session.target()));
    }
    if let Some(prefix) = case_prefix {
        cases.retain(|case| case.name.starts_with(&prefix));
        if cases.is_empty() {
            return Err("No cases matched --case-prefix".into());
        }
    } else {
        // Diagnostic fixtures retain known reference defects. Run them by
        // explicit prefix so they are not mistaken for parity regressions.
        cases.retain(|case| {
            !case.name.starts_with("block_boundary_probe_")
                && !case.name.starts_with("multiply_index_probe_")
        });
    }
    if let Some(name) = case_name {
        cases.retain(|case| case.name == name);
        if cases.is_empty() {
            return Err("No case matched --case-name".into());
        }
    }
    let matrix_unsafe = cases
        .iter()
        .filter(|c| matrix_counter_cases::unsafe_installed_case(&c.name))
        .map(|c| c.name.clone())
        .collect::<Vec<_>>();
    if !matrix_unsafe.is_empty() {
        eprintln!(
            "Withheld {} installed matrix C200+ binary diagnostics after observed hangs/state corruption; sources and first failures retained in matrix-counter reports. Encoder also rejects these operands; no equivalence claimed.",
            matrix_unsafe.len()
        );
        cases.retain(|c| !matrix_counter_cases::unsafe_installed_case(&c.name));
        if cases.is_empty() {
            return Err("Selected matrix operands have a proven unsafe installed handler; native encoding is blocked".into());
        }
    }
    if !unresponsive_probes {
        let before = cases.len();
        cases.retain(|c| !known_nonresponsive_float_probe(&c.name));
        if cases.len() != before {
            eprintln!(
                "Excluded {} known nonresponsive native probes; see retained float probe reports. Use --unresponsive-probes to include them; they are not passing equivalence evidence.",
                before - cases.len()
            );
        }
        if cases.is_empty() {
            return Err("Only known nonresponsive probes selected; use --unresponsive-probes to run them explicitly".into());
        }
    }
    let mut reports = Vec::new();
    for c in cases {
        let r = run(&mut session, &c);
        println!(
            "{}: {} ({} scans, {} checks, {} differences){}",
            r.name,
            r.status,
            r.scans,
            r.checks,
            r.differences.len(),
            r.error
                .as_deref()
                .map(|e| format!(" — {e}"))
                .unwrap_or_default()
        );
        reports.push(r);
    }
    let bad = reports.iter().any(|r| r.status != "match");
    use sha2::{Digest, Sha256};
    let producer_executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let producer_sha256 = format!(
        "{:x}",
        Sha256::digest(std::fs::read(&producer_executable).map_err(|e| e.to_string())?)
    );
    let report = serde_json::json!({"schema_version":1,"producer_executable":producer_executable,"producer_sha256":producer_sha256,"target_model":session.target().name(),"target_sha256":session.sha256(),"scan_ms":opendvp::es2_bridge::SCAN_MS,"method":"owned original process; verified program readback; M1170/M1171 instruction stepping; PC checked after every instruction; M1039/D1039 exact 100 ms quantum","teaching_ticks_per_scan":teaching_ticks,"teaching_clock_scope":if teaching_ticks.is_some(){"Explicit pending tick input replay before END/FEND; owned original memory input only, matching independent local input, no result writes; not natural host-clock parity"}else{"Installed M1039 fixed scan prevents natural teaching ticks; owner/false behavior only"},"cases":reports});
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        &output,
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!("Report: {}", output.display());
    if bad {
        return Err(
            "Reference comparison has mismatches or inconclusive cases; inspect report".into(),
        );
    }
    Ok(())
}

fn known_nonresponsive_float_probe(name: &str) -> bool {
    [
        "float_math_hard_div_max_finite_overflow",
        "float_math_angle_DDEG_7f7fffff_",
        "float_math_angle_DDEG_ff7fffff_",
        "float_math_binary_DEADD_1_",
        "float_math_binary_DESUB_1_",
        "float_math_binary_DEMUL_1_",
        "float_math_decimal_encode_7f800000_",
        "float_math_decimal_encode_ff800000_",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
        || matches!(
            name,
            "search_transform_dtm_interpolation_float_values_5"
                | "float_advanced_bounded_result_1"
                | "float_advanced_bounded_result_3"
                | "float_advanced_bounded_result_4"
                | "float_advanced_domain_DEXP_7f7fffff_false"
                | "float_advanced_domain_DEXP_7f7fffff_true"
                | "float_advanced_domain_DEXP_7f800000_false"
                | "float_advanced_domain_DEXP_7f800000_true"
                | "float_advanced_domain_DLN_7f800000_false"
                | "float_advanced_domain_DLN_7f800000_true"
                | "float_advanced_log_base_argument_11_false"
                | "float_advanced_log_base_argument_11_true"
                | "float_advanced_pow_base_exponent_10"
                | "float_advanced_pow_base_exponent_11"
                | "float_advanced_pow_base_exponent_12"
                | "float_advanced_pow_base_exponent_13"
                | "float_advanced_pow_base_exponent_15"
        )
}

#[cfg(test)]
mod case_generation_tests {
    #[test]
    fn program_control_fixtures_link_and_execute_bounded_local_visits() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let cases = super::suite(target)
                .into_iter()
                .filter(|c| c.name.starts_with("program_control_"))
                .collect::<Vec<_>>();
            assert_eq!(cases.len(), 122);
            let mut errors = Vec::new();
            for c in cases {
                let p = match opendvp::plc::compile(&c.source) {
                    Ok(p) => p,
                    Err(e) => {
                        errors.push(format!("{}: {e}", c.name));
                        continue;
                    }
                };
                match opendvp::es2_encoder::encode(&p) {
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
                        sim.scan(&p, 100);
                        if let opendvp::plc::ScanStatus::Fault(e) = &sim.last_scan {
                            errors.push(format!("{}: {e}", c.name));
                            break;
                        }
                    }
                }
            }
            assert!(errors.is_empty(), "{}", errors.join("\n"));
        }
    }
    #[test]
    fn matrix_corpus_preserves_diagnostics_but_blocks_unsafe_installed_operands() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let ordinary = super::matrix_cases::cases(target);
            let counters = super::matrix_counter_cases::cases(target);
            let inputs = super::matrix_input_cases::cases();
            let bulk = super::matrix_bulk_cases::cases(target);
            assert_eq!(bulk.len(), 32);
            assert_eq!(
                (ordinary.len(), counters.len(), inputs.len()),
                (1024, 108, 12)
            );
            let mut names = std::collections::BTreeSet::new();
            let mut rejected = 0;
            for c in ordinary
                .into_iter()
                .chain(counters)
                .chain(inputs)
                .chain(bulk)
            {
                assert!(names.insert(c.name.clone()), "{}", c.name);
                let p =
                    opendvp::plc::compile(&c.source).unwrap_or_else(|e| panic!("{}: {e}", c.name));
                let encoded = opendvp::es2_encoder::encode(&p);
                if super::matrix_counter_cases::unsafe_installed_case(&c.name) {
                    rejected += 1;
                    assert!(encoded.unwrap_err().contains("C200+"), "{}", c.name);
                    assert!(
                        p.native_program_words > 0,
                        "local PC layout must survive native rejection"
                    );
                } else {
                    assert!(
                        encoded
                            .unwrap_or_else(|e| panic!("{}: {e}", c.name))
                            .next_pc
                            .len()
                            <= 512
                    );
                }
            }
            assert_eq!(rejected, 30);
        }
    }
    #[test]
    fn search_scenarios_compile_and_fit_native_budget() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let mut names = std::collections::BTreeSet::new();
            let cases = super::search_value_cases::cases(target)
                .into_iter()
                .chain(super::search_state_cases::cases(target));
            let mut errors = Vec::new();
            for c in cases {
                assert!(names.insert(c.name.clone()), "{}", c.name);
                match opendvp::plc::compile(&c.source)
                    .map_err(|e| e.to_string())
                    .and_then(|p| opendvp::es2_encoder::encode(&p))
                {
                    Ok(e) => assert!(e.next_pc.len() <= 512, "{}", c.name),
                    Err(e) => errors.push(format!("{}: {e}", c.name)),
                }
            }
            assert!(errors.is_empty(), "{}", errors.join("\n"));
        }
    }
    use super::*;

    #[test]
    fn additional_processing_scenarios_compile_and_fit_native_budget() {
        let mut errors = Vec::new();
        for target in [Target::Es2, Target::Ss2] {
            let mut names = std::collections::BTreeSet::new();
            let cases = additional_processing_cases(target);
            println!(
                "{target:?}: {} additional processing scenarios",
                cases.len()
            );
            for case in cases {
                assert!(names.insert(case.name.clone()), "duplicate {}", case.name);
                let program = match plc::compile(&case.source) {
                    Ok(program) => program,
                    Err(error) => {
                        errors.push(format!("{target:?} {}: {error}", case.name));
                        continue;
                    }
                };
                let encoded = match opendvp::es2_encoder::encode(&program) {
                    Ok(encoded) => encoded,
                    Err(error) => {
                        errors.push(format!("{target:?} {}: {error}", case.name));
                        continue;
                    }
                };
                assert!(
                    encoded.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                    "{} exceeds native stepping budget",
                    case.name
                );
            }
        }
        assert!(errors.is_empty(), "{}", errors.join("\n"));
    }

    #[test]
    fn advanced_precision_batches_compile_and_fit_native_budget() {
        for target in [Target::Es2, Target::Ss2] {
            let cases = float_advanced_precision_cases(target);
            assert_eq!(cases.len(), 80);
            let mut names = std::collections::BTreeSet::new();
            for case in cases {
                assert!(names.insert(case.name.clone()), "duplicate {}", case.name);
                let program = plc::compile(&case.source)
                    .unwrap_or_else(|error| panic!("{}: {error}", case.name));
                let encoded = opendvp::es2_encoder::encode(&program)
                    .unwrap_or_else(|error| panic!("{}: {error}", case.name));
                assert!(encoded.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS);
                for index in 100..116 {
                    assert!(
                        program.devices.contains(&Device {
                            area: Area::D,
                            index
                        }),
                        "{} misses D{index}",
                        case.name
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod scalar_operand_corpus_tests {
    #[test]
    fn indexed_decimal_conversion_corpus_has_unique_names_and_encodes_two_roles() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let mut names = std::collections::BTreeSet::new();
            for case in super::indexed_decimal_conversion_cases::cases()
                .into_iter()
                .chain(super::indexed_decimal_conversion_cases::model_cases(target))
            {
                assert!(names.insert(case.name.clone()), "{}", case.name);
                let p = opendvp::plc::compile_il(&case.source)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let e = opendvp::es2_encoder::encode(&p)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                assert!(
                    e.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                    "{}",
                    case.name
                );
                assert!(
                    e.preflight.values().any(|c| matches!(
                        c,
                        opendvp::es2_encoder::PreflightCheck::IndexedDecimalConversion(_)
                    )),
                    "{}",
                    case.name
                );
            }
            assert!(!names.is_empty());
        }
    }
    #[test]
    fn indexed_exp_ln_corpus_has_unique_names_and_encodes_two_roles() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let mut names = std::collections::BTreeSet::new();
            for case in super::indexed_exp_ln_cases::cases()
                .into_iter()
                .chain(super::indexed_exp_ln_cases::model_cases(target))
            {
                assert!(names.insert(case.name.clone()), "{}", case.name);
                let p = opendvp::plc::compile_il(&case.source)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let e = opendvp::es2_encoder::encode(&p)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                assert!(
                    e.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                    "{}",
                    case.name
                );
                assert!(
                    e.preflight.values().any(|c| matches!(
                        c,
                        opendvp::es2_encoder::PreflightCheck::IndexedExpLn(_)
                    )),
                    "{}",
                    case.name
                );
            }
            assert!(!names.is_empty());
        }
    }
    #[test]
    fn indexed_desqr_corpus_has_unique_names_and_encodes_two_roles() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let mut names = std::collections::BTreeSet::new();
            for case in super::indexed_desqr_cases::cases()
                .into_iter()
                .chain(super::indexed_desqr_cases::model_cases(target))
            {
                assert!(names.insert(case.name.clone()), "{}", case.name);
                let p = opendvp::plc::compile_il(&case.source)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let e = opendvp::es2_encoder::encode(&p)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                assert!(
                    e.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                    "{}",
                    case.name
                );
                assert!(
                    e.preflight.values().any(|c| matches!(
                        c,
                        opendvp::es2_encoder::PreflightCheck::IndexedDesqr(_)
                    )),
                    "{}",
                    case.name
                );
            }
            assert!(!names.is_empty());
        }
    }
    #[test]
    fn indexed_float_comparison_corpus_has_unique_names_and_encodes_two_roles() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let mut names = std::collections::BTreeSet::new();
            for case in super::indexed_float_comparison_cases::cases()
                .into_iter()
                .chain(super::indexed_float_comparison_cases::model_cases(target))
            {
                assert!(names.insert(case.name.clone()), "{}", case.name);
                let p = opendvp::plc::compile_il(&case.source)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let e = opendvp::es2_encoder::encode(&p)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                assert!(
                    e.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                    "{}",
                    case.name
                );
                assert!(
                    e.preflight.values().any(|c| matches!(
                        c,
                        opendvp::es2_encoder::PreflightCheck::IndexedFloatComparison(_)
                            | opendvp::es2_encoder::PreflightCheck::IndexedFloatPredicate(_)
                    )),
                    "{}",
                    case.name
                );
            }
            assert!(!names.is_empty());
        }
    }
    #[test]
    fn indexed_random_corpus_qualifies_actual_tail_words_pcs_and_complete_local_visits() {
        use super::indexed_random_cases::TailClass;
        use opendvp::{execution_plan::PlanOp, random_tail::OperandTailControl as Control};
        use sha2::{Digest, Sha256};
        for target in [super::Target::Es2, super::Target::Ss2] {
            let cases = super::indexed_random_cases::common_cases()
                .into_iter()
                .chain(super::indexed_random_cases::model_cases(target))
                .collect::<Vec<_>>();
            let audits = super::indexed_random_cases::tail_inventory(&cases);
            let mut names = std::collections::BTreeSet::new();
            let mut qualified = Vec::new();
            for case in cases {
                assert!(names.insert(case.name.clone()), "{}", case.name);
                let p = opendvp::plc::compile_il(&case.source)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let e = opendvp::es2_encoder::encode(&p)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                assert!(
                    e.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                    "{}",
                    case.name
                );
                assert!(
                    e.preflight.values().any(|c| matches!(
                        c,
                        opendvp::es2_encoder::PreflightCheck::IndexedRandom(_)
                    )),
                    "{}",
                    case.name
                );
                let headers = e
                    .execution_plan
                    .nodes
                    .iter()
                    .filter(|n| {
                        matches!(n.op, PlanOp::Action { .. })
                            && matches!(e.words[usize::from(n.pc)], 0x5501 | 0x5511)
                    })
                    .collect::<Vec<_>>();
                let expected = audits
                    .iter()
                    .filter(|a| a.case_name == case.name)
                    .collect::<Vec<_>>();
                assert_eq!(headers.len(), expected.len(), "{}", case.name);
                let mut tails = Vec::new();
                for (header, audit) in headers.into_iter().zip(expected) {
                    let start = usize::from(header.pc);
                    assert_eq!(
                        &e.words[start + 7..start + 13],
                        audit.words,
                        "{} line{}",
                        case.name,
                        audit.source_line
                    );
                    for (offset, class) in audit.classes.iter().enumerate() {
                        let op = match class {
                            TailClass::Inert => None,
                            TailClass::Np => Some(Control::Np),
                            TailClass::Pn => Some(Control::Pn),
                            TailClass::Anb => Some(Control::AndBlock),
                            TailClass::Orb => Some(Control::OrBlock),
                            TailClass::Mps => Some(Control::Mps),
                            TailClass::Mrd => Some(Control::Mrd),
                            TailClass::Mpp => Some(Control::Mpp),
                            TailClass::Inv => Some(Control::Invert),
                        };
                        let pc = header.pc + 7 + offset as u16;
                        let n = &e.execution_plan.nodes[e.execution_plan.by_pc[&pc]];
                        assert_eq!(n.fallthrough, pc + 1);
                        assert_eq!(
                            n.op,
                            op.map(|op| PlanOp::OperandTailControl {
                                header: header.pc,
                                op
                            })
                            .unwrap_or(PlanOp::Inert)
                        );
                    }
                    tails.push(serde_json::json!({"header_pc":header.pc,"source_line":audit.source_line,
                        "words":audit.words,"classes":audit.classes.iter().map(|c|format!("{c:?}")).collect::<Vec<_>>()}));
                }
                let mut sim = opendvp::plc::Simulator::default();
                sim.memory.random_tail_profile =
                    opendvp::random_tail::RandomTailProfile::CurrentCommgr;
                sim.control.profile = opendvp::program_control::ControlProfile::InstalledLegacy;
                sim.memory.sfc.profile = opendvp::sfc::SfcProfile::InstalledLegacy;
                sim.memory.data_register_limit = Some(if target == super::Target::Ss2 {
                    4999
                } else {
                    9999
                });
                let mut scans = 0;
                for (inputs, count) in &case.stages {
                    for (input, value) in inputs {
                        sim.memory.set_input(input.parse().unwrap(), *value);
                    }
                    for _ in 0..*count {
                        sim.scan(&p, 0);
                        scans += 1;
                        assert_eq!(
                            sim.last_scan,
                            opendvp::plc::ScanStatus::Complete,
                            "{}",
                            case.name
                        );
                        assert!(
                            sim.memory.execution_limitations.is_empty(),
                            "{}: {:?}",
                            case.name,
                            sim.memory.execution_limitations
                        );
                    }
                }
                qualified.push(serde_json::json!({"case":case.name,"source_sha256":format!("{:x}",Sha256::digest(case.source.as_bytes())),
                    "local_completed_scans":scans,"tails":tails}));
            }
            println!(
                "INDEXED_RANDOM_TAIL_QUALIFICATION {}",
                serde_json::json!({"model":target.name(),"cases":qualified})
            );
        }
    }
    fn assert_indexed_search_case_local(case: &super::Case, target: super::Target) -> bool {
        let p =
            opendvp::plc::compile_il(&case.source).unwrap_or_else(|e| panic!("{}: {e}", case.name));
        let encoded =
            opendvp::es2_encoder::encode(&p).unwrap_or_else(|e| panic!("{}: {e}", case.name));
        assert!(encoded.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS);
        assert!(encoded.words.len() <= opendvp::es2_encoder::MAX_PROGRAM_WORDS);
        assert!(
            encoded.preflight.values().any(|c| matches!(
                c,
                opendvp::es2_encoder::PreflightCheck::IndexedSearchToggle(_)
            )),
            "{}",
            case.name
        );
        let oracle = case.name.contains("literal_oracle_");
        let markers = if oracle {
            let markers = case
                .source
                .lines()
                .filter_map(|line| {
                    let fields = line.split_whitespace().collect::<Vec<_>>();
                    if fields.len() != 2 || fields[0] != "OUT" {
                        return None;
                    }
                    let d = fields[1].parse::<opendvp::plc::Device>().ok()?;
                    (d.area == opendvp::plc::Area::M && (840..=847).contains(&d.index)).then_some(d)
                })
                .collect::<std::collections::BTreeSet<_>>();
            assert!(!markers.is_empty(), "{} has no declared oracle", case.name);
            markers
        } else {
            Default::default()
        };
        let mut sim = opendvp::plc::Simulator::default();
        sim.control.profile = opendvp::program_control::ControlProfile::InstalledLegacy;
        sim.memory.sfc.profile = opendvp::sfc::SfcProfile::InstalledLegacy;
        sim.memory.random_tail_profile = opendvp::random_tail::RandomTailProfile::CurrentCommgr;
        sim.memory.data_register_limit = Some(if target == super::Target::Ss2 {
            4999
        } else {
            9999
        });
        for (inputs, count) in &case.stages {
            for (input, value) in inputs {
                sim.memory.set_input(input.parse().unwrap(), *value);
            }
            for _ in 0..*count {
                sim.scan(&p, 0);
                assert_eq!(
                    sim.last_scan,
                    opendvp::plc::ScanStatus::Complete,
                    "{}",
                    case.name
                );
                assert!(
                    sim.memory.execution_limitations.is_empty(),
                    "{}: {:?}",
                    case.name,
                    sim.memory.execution_limitations
                );
                for device in &markers {
                    assert!(
                        sim.memory.bit(*device),
                        "{} independent oracle {device}",
                        case.name
                    );
                }
            }
        }
        oracle
    }
    #[test]
    fn indexed_search_toggle_native_candidates_admit_all_stages_and_literal_oracles() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let cases = super::indexed_search_toggle_cases::common_cases()
                .into_iter()
                .chain(super::indexed_search_toggle_cases::model_cases(target))
                .collect::<Vec<_>>();
            let errors = cases
                .iter()
                .filter_map(|case| {
                    opendvp::plc::compile_il(&case.source)
                        .map_err(|e| e.to_string())
                        .and_then(|p| opendvp::es2_encoder::encode(&p).map(|_| ()))
                        .err()
                        .map(|e| format!("{}: {e}", case.name))
                })
                .collect::<Vec<_>>();
            assert!(errors.is_empty(), "{errors:#?}");
            let mut names = std::collections::BTreeSet::new();
            let mut oracles = 0;
            for case in cases {
                assert!(names.insert(case.name.clone()));
                oracles += usize::from(assert_indexed_search_case_local(&case, target));
            }
            assert_eq!(names.len(), 296);
            assert_eq!(oracles, 44);
        }
    }
    #[test]
    fn indexed_search_toggle_counter_termination_cases_are_local_only_and_preserve_later_counters()
    {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let native = super::indexed_search_toggle_cases::common_cases()
                .into_iter()
                .chain(super::indexed_search_toggle_cases::model_cases(target))
                .map(|case| case.name)
                .collect::<std::collections::BTreeSet<_>>();
            let mut names = std::collections::BTreeSet::new();
            for case in super::indexed_search_toggle_cases::counter_termination_cases(target) {
                assert!(
                    !native.contains(&case.name),
                    "local policy leaked into native list: {}",
                    case.name
                );
                assert!(case.name.contains("LOCAL_ONLY") && names.insert(case.name.clone()));
                assert!(assert_indexed_search_case_local(&case, target));
            }
            assert_eq!(names.len(), 6);
        }
    }
    fn assert_indexed_calendar_case_local(case: &super::Case, target: super::Target) -> bool {
        let p =
            opendvp::plc::compile_il(&case.source).unwrap_or_else(|e| panic!("{}: {e}", case.name));
        let encoded =
            opendvp::es2_encoder::encode(&p).unwrap_or_else(|e| panic!("{}: {e}", case.name));
        assert!(encoded.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS);
        assert!(encoded.words.len() <= opendvp::es2_encoder::MAX_PROGRAM_WORDS);
        assert!(
            encoded.preflight.values().any(|c| matches!(
                c,
                opendvp::es2_encoder::PreflightCheck::IndexedCalendarTime(_)
            )),
            "{}",
            case.name
        );
        let markers = {
            let markers = case
                .source
                .lines()
                .filter_map(|line| {
                    let fields = line.split_whitespace().collect::<Vec<_>>();
                    if fields.len() != 2 || fields[0] != "OUT" {
                        return None;
                    }
                    let d = fields[1].parse::<opendvp::plc::Device>().ok()?;
                    (d.area == opendvp::plc::Area::M && (840..=847).contains(&d.index)).then_some(d)
                })
                .collect::<std::collections::BTreeSet<_>>();
            markers
        };
        let oracle = !markers.is_empty();
        let mut sim = opendvp::plc::Simulator::default();
        sim.control.profile = opendvp::program_control::ControlProfile::InstalledLegacy;
        sim.memory.sfc.profile = opendvp::sfc::SfcProfile::InstalledLegacy;
        sim.memory.random_tail_profile = opendvp::random_tail::RandomTailProfile::CurrentCommgr;
        sim.memory.calendar_tail_profile =
            opendvp::calendar_tail::CalendarTailProfile::CurrentCommgr;
        sim.memory.data_register_limit = Some(if target == super::Target::Ss2 {
            4999
        } else {
            9999
        });
        for (inputs, count) in &case.stages {
            for (input, value) in inputs {
                sim.memory.set_input(input.parse().unwrap(), *value);
            }
            for _ in 0..*count {
                sim.scan(&p, 0);
                assert_eq!(
                    sim.last_scan,
                    opendvp::plc::ScanStatus::Complete,
                    "{}",
                    case.name
                );
                assert!(
                    sim.memory.execution_limitations.is_empty(),
                    "{}: {:?}",
                    case.name,
                    sim.memory.execution_limitations
                );
                for device in &markers {
                    assert!(
                        sim.memory.bit(*device),
                        "{} independent oracle {device}",
                        case.name
                    );
                }
            }
        }
        oracle
    }
    #[test]
    fn indexed_calendar_time_native_candidates_admit_all_stages_and_literal_oracles() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let cases = super::indexed_calendar_time_cases::common_cases()
                .into_iter()
                .chain(super::indexed_calendar_time_cases::model_cases(target))
                .collect::<Vec<_>>();
            let errors = cases
                .iter()
                .filter_map(|case| {
                    opendvp::plc::compile_il(&case.source)
                        .map_err(|e| e.to_string())
                        .and_then(|p| opendvp::es2_encoder::encode(&p).map(|_| ()))
                        .err()
                        .map(|e| format!("{}: {e}", case.name))
                })
                .collect::<Vec<_>>();
            assert!(errors.is_empty(), "{errors:#?}");
            let mut names = std::collections::BTreeSet::new();
            let mut oracles = 0;
            for case in cases {
                assert!(names.insert(case.name.clone()));
                oracles += usize::from(assert_indexed_calendar_case_local(&case, target));
            }
            assert_eq!(names.len(), 402);
            assert_eq!(oracles, 166);
        }
    }
    #[test]
    fn calendar_corpus_keeps_all_original_tail_controls_in_main() {
        use opendvp::random_tail::OperandTailControl as C;
        for target in [super::Target::Es2, super::Target::Ss2] {
            let cases = super::indexed_calendar_time_cases::common_cases()
                .into_iter()
                .chain(super::indexed_calendar_time_cases::model_cases(target));
            let mut found = std::collections::BTreeSet::new();
            for case in cases {
                let p = opendvp::plc::compile_il(&case.source).unwrap();
                let e = opendvp::es2_encoder::encode(&p).unwrap();
                for (&pc, owner) in &e.execution_plan.operand_tail_owners {
                    if *owner != opendvp::random_tail::OperandTailOwner::CalendarTime {
                        continue;
                    }
                    let words = &e.words[usize::from(pc)..usize::from(pc) + 7];
                    for control in opendvp::calendar_tail::decode_time_tail(words)
                        .unwrap()
                        .into_iter()
                        .flatten()
                    {
                        found.insert(match control {
                            C::Np => 0,
                            C::AndBlock => 1,
                            C::Mps => 2,
                            C::Mpp => 3,
                            _ => panic!("unreachable time role control"),
                        });
                    }
                }
            }
            assert_eq!(found, [0, 1, 2, 3].into_iter().collect());
        }
    }
    fn assert_indexed_data_case_local(case: &super::Case, target: super::Target) -> bool {
        let p =
            opendvp::plc::compile_il(&case.source).unwrap_or_else(|e| panic!("{}: {e}", case.name));
        let encoded =
            opendvp::es2_encoder::encode(&p).unwrap_or_else(|e| panic!("{}: {e}", case.name));
        assert!(encoded.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS);
        assert!(encoded.words.len() <= opendvp::es2_encoder::MAX_PROGRAM_WORDS);
        assert!(
            encoded.preflight.values().any(|c| matches!(
                c,
                opendvp::es2_encoder::PreflightCheck::IndexedDataProcessing(_)
            )),
            "{}",
            case.name
        );
        let oracle = case.name.contains("literal_oracle_");
        let markers = if oracle {
            let markers = case
                .source
                .lines()
                .filter_map(|line| {
                    let fields = line.split_whitespace().collect::<Vec<_>>();
                    if fields.len() != 2 || fields[0] != "OUT" {
                        return None;
                    }
                    let d = fields[1].parse::<opendvp::plc::Device>().ok()?;
                    (d.area == opendvp::plc::Area::M && (840..=847).contains(&d.index)).then_some(d)
                })
                .collect::<std::collections::BTreeSet<_>>();
            assert!(!markers.is_empty(), "{} has no declared oracle", case.name);
            markers
        } else {
            Default::default()
        };
        let mut sim = opendvp::plc::Simulator::default();
        sim.control.profile = opendvp::program_control::ControlProfile::InstalledLegacy;
        sim.memory.sfc.profile = opendvp::sfc::SfcProfile::InstalledLegacy;
        sim.memory.random_tail_profile = opendvp::random_tail::RandomTailProfile::CurrentCommgr;
        sim.memory.data_register_limit = Some(if target == super::Target::Ss2 {
            4999
        } else {
            9999
        });
        for (inputs, count) in &case.stages {
            for (input, value) in inputs {
                sim.memory.set_input(input.parse().unwrap(), *value);
            }
            for _ in 0..*count {
                sim.scan(&p, 0);
                assert_eq!(
                    sim.last_scan,
                    opendvp::plc::ScanStatus::Complete,
                    "{}",
                    case.name
                );
                assert!(
                    sim.memory.execution_limitations.is_empty(),
                    "{}: {:?}",
                    case.name,
                    sim.memory.execution_limitations
                );
                for device in &markers {
                    assert!(
                        sim.memory.bit(*device),
                        "{} independent oracle {device}",
                        case.name
                    );
                }
            }
        }
        oracle
    }
    #[test]
    fn indexed_data_processing_native_candidates_admit_all_stages_and_literal_oracles() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let cases = super::indexed_data_processing_cases::common_cases()
                .into_iter()
                .chain(super::indexed_data_processing_cases::model_cases(target))
                .collect::<Vec<_>>();
            let errors = cases
                .iter()
                .filter_map(|case| {
                    opendvp::plc::compile_il(&case.source)
                        .map_err(|e| e.to_string())
                        .and_then(|p| opendvp::es2_encoder::encode(&p).map(|_| ()))
                        .err()
                        .map(|e| format!("{}: {e}", case.name))
                })
                .collect::<Vec<_>>();
            assert!(errors.is_empty(), "{errors:#?}");
            let mut names = std::collections::BTreeSet::new();
            let mut oracles = 0;
            for case in cases {
                assert!(names.insert(case.name.clone()));
                oracles += usize::from(assert_indexed_data_case_local(&case, target));
            }
            assert_eq!(names.len(), 326);
            assert_eq!(oracles, 38);
        }
    }
    #[test]
    fn indexed_data_processing_documented_partials_are_local_only_and_keep_requested_denominator() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let native = super::indexed_data_processing_cases::common_cases()
                .into_iter()
                .chain(super::indexed_data_processing_cases::model_cases(target))
                .map(|case| case.name)
                .collect::<std::collections::BTreeSet<_>>();
            let mut names = std::collections::BTreeSet::new();
            for case in super::indexed_data_processing_cases::documented_partial_cases(target) {
                assert!(
                    !native.contains(&case.name),
                    "local policy leaked into native list: {}",
                    case.name
                );
                assert!(case.name.contains("LOCAL_ONLY") && names.insert(case.name.clone()));
                assert!(assert_indexed_data_case_local(&case, target));
            }
            assert_eq!(names.len(), 20);
        }
    }
    #[test]
    fn indexed_shift_fifo_corpus_admits_all_stages_and_declared_literal_oracles() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let cases = super::indexed_shift_fifo_cases::common_cases()
                .into_iter()
                .chain(super::indexed_shift_fifo_cases::model_cases(target))
                .collect::<Vec<_>>();
            let admission_errors = cases
                .iter()
                .filter_map(|case| {
                    opendvp::plc::compile_il(&case.source)
                        .map_err(|error| error.to_string())
                        .and_then(|p| opendvp::es2_encoder::encode(&p).map(|_| ()))
                        .err()
                        .map(|error| format!("{}: {error}", case.name))
                })
                .collect::<Vec<_>>();
            assert!(admission_errors.is_empty(), "{admission_errors:#?}");
            let mut names = std::collections::BTreeSet::new();
            let mut oracle_cases = 0;
            for case in cases {
                assert!(names.insert(case.name.clone()));
                let p = opendvp::plc::compile_il(&case.source)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let encoded = opendvp::es2_encoder::encode(&p)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                assert!(encoded.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS);
                assert!(encoded.words.len() <= opendvp::es2_encoder::MAX_PROGRAM_WORDS);
                assert!(
                    encoded.preflight.values().any(|c| matches!(
                        c,
                        opendvp::es2_encoder::PreflightCheck::IndexedShiftFifo(_)
                    )),
                    "{}",
                    case.name
                );
                let markers = if case.name.contains("literal_oracle_") {
                    oracle_cases += 1;
                    let markers = case
                        .source
                        .lines()
                        .filter_map(|line| {
                            let f = line.split_whitespace().collect::<Vec<_>>();
                            if f.len() != 2 || f[0] != "OUT" {
                                return None;
                            }
                            let d = f[1].parse::<opendvp::plc::Device>().ok()?;
                            (d.area == opendvp::plc::Area::M && (840..=847).contains(&d.index))
                                .then_some(d)
                        })
                        .collect::<std::collections::BTreeSet<_>>();
                    assert!(!markers.is_empty(), "{} has no declared oracle", case.name);
                    markers
                } else {
                    Default::default()
                };
                let mut sim = opendvp::plc::Simulator::default();
                sim.control.profile = opendvp::program_control::ControlProfile::InstalledLegacy;
                sim.memory.sfc.profile = opendvp::sfc::SfcProfile::InstalledLegacy;
                sim.memory.random_tail_profile =
                    opendvp::random_tail::RandomTailProfile::CurrentCommgr;
                sim.memory.data_register_limit = Some(if target == super::Target::Ss2 {
                    4999
                } else {
                    9999
                });
                for (inputs, count) in &case.stages {
                    for (input, value) in inputs {
                        sim.memory.set_input(input.parse().unwrap(), *value);
                    }
                    for _ in 0..*count {
                        sim.scan(&p, 0);
                        assert_eq!(
                            sim.last_scan,
                            opendvp::plc::ScanStatus::Complete,
                            "{}",
                            case.name
                        );
                        assert!(
                            sim.memory.execution_limitations.is_empty(),
                            "{}: {:?}",
                            case.name,
                            sim.memory.execution_limitations
                        );
                        for device in &markers {
                            assert!(
                                sim.memory.bit(*device),
                                "{} independent oracle {device}",
                                case.name
                            );
                        }
                    }
                }
            }
            assert_eq!(names.len(), 320);
            assert_eq!(oracle_cases, 32);
        }
    }
    #[test]
    fn indexed_block_exchange_corpus_admits_all_stages_and_declared_literal_oracles() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let mut names = std::collections::BTreeSet::new();
            let mut oracle_cases = 0;
            for case in super::indexed_block_exchange_cases::common_cases()
                .into_iter()
                .chain(super::indexed_block_exchange_cases::model_cases(target))
            {
                assert!(names.insert(case.name.clone()));
                let p = opendvp::plc::compile_il(&case.source)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let encoded = opendvp::es2_encoder::encode(&p)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                assert!(encoded.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS);
                assert!(encoded.words.len() <= opendvp::es2_encoder::MAX_PROGRAM_WORDS);
                assert!(
                    encoded.preflight.values().any(|c| matches!(
                        c,
                        opendvp::es2_encoder::PreflightCheck::IndexedBlockExchange(_)
                    )),
                    "{}",
                    case.name
                );
                let markers = if case.name.contains("literal_oracle_") {
                    oracle_cases += 1;
                    let markers = case
                        .source
                        .lines()
                        .filter_map(|line| {
                            let fields = line.split_whitespace().collect::<Vec<_>>();
                            if fields.len() != 2 || fields[0] != "OUT" {
                                return None;
                            }
                            let device = fields[1].parse::<opendvp::plc::Device>().ok()?;
                            (device.area == opendvp::plc::Area::M
                                && (840..=847).contains(&device.index))
                            .then_some(device)
                        })
                        .collect::<std::collections::BTreeSet<_>>();
                    assert!(!markers.is_empty(), "{} has no declared oracle", case.name);
                    markers
                } else {
                    Default::default()
                };
                let mut sim = opendvp::plc::Simulator::default();
                sim.control.profile = opendvp::program_control::ControlProfile::InstalledLegacy;
                sim.memory.sfc.profile = opendvp::sfc::SfcProfile::InstalledLegacy;
                sim.memory.random_tail_profile =
                    opendvp::random_tail::RandomTailProfile::CurrentCommgr;
                sim.memory.data_register_limit = Some(if target == super::Target::Ss2 {
                    4999
                } else {
                    9999
                });
                for (inputs, count) in &case.stages {
                    for (input, value) in inputs {
                        sim.memory.set_input(input.parse().unwrap(), *value);
                    }
                    for _ in 0..*count {
                        sim.scan(&p, 0);
                        assert_eq!(
                            sim.last_scan,
                            opendvp::plc::ScanStatus::Complete,
                            "{}",
                            case.name
                        );
                        assert!(
                            sim.memory.execution_limitations.is_empty(),
                            "{}: {:?}",
                            case.name,
                            sim.memory.execution_limitations
                        );
                        for device in &markers {
                            assert!(
                                sim.memory.bit(*device),
                                "{} independent oracle {device}",
                                case.name
                            );
                        }
                    }
                }
            }
            assert_eq!(names.len(), 126);
            assert_eq!(oracle_cases, 28);
        }
    }
    #[test]
    fn indexed_rotation_corpus_admits_all_stages_and_asserts_literal_oracles() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let mut names = std::collections::BTreeSet::new();
            for case in super::indexed_rotation_cases::common_cases()
                .into_iter()
                .chain(super::indexed_rotation_cases::model_cases(target))
            {
                assert!(names.insert(case.name.clone()));
                let p = opendvp::plc::compile_il(&case.source)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let encoded = opendvp::es2_encoder::encode(&p)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                assert!(encoded.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS);
                assert!(encoded.words.len() <= opendvp::es2_encoder::MAX_PROGRAM_WORDS);
                assert!(
                    encoded.preflight.values().any(|c| matches!(
                        c,
                        opendvp::es2_encoder::PreflightCheck::IndexedRotation(_)
                    )),
                    "{}",
                    case.name
                );
                let mut sim = opendvp::plc::Simulator::default();
                sim.control.profile = opendvp::program_control::ControlProfile::InstalledLegacy;
                sim.memory.sfc.profile = opendvp::sfc::SfcProfile::InstalledLegacy;
                sim.memory.random_tail_profile =
                    opendvp::random_tail::RandomTailProfile::CurrentCommgr;
                sim.memory.data_register_limit = Some(if target == super::Target::Ss2 {
                    4999
                } else {
                    9999
                });
                for (inputs, count) in &case.stages {
                    for (input, value) in inputs {
                        sim.memory.set_input(input.parse().unwrap(), *value);
                    }
                    for _ in 0..*count {
                        sim.scan(&p, 0);
                        assert_eq!(
                            sim.last_scan,
                            opendvp::plc::ScanStatus::Complete,
                            "{}",
                            case.name
                        );
                        assert!(
                            sim.memory.execution_limitations.is_empty(),
                            "{}: {:?}",
                            case.name,
                            sim.memory.execution_limitations
                        );
                        if case.name.contains("literal_oracle_") {
                            for index in 840..=845 {
                                assert!(
                                    sim.memory.bit(opendvp::plc::Device {
                                        area: opendvp::plc::Area::M,
                                        index
                                    }),
                                    "{} independent oracle M{index}",
                                    case.name
                                );
                            }
                        }
                    }
                }
            }
            assert_eq!(names.len(), 100);
        }
    }
    #[test]
    fn indexed_clock_read_corpus_uses_seeded_inputs_and_admits_all_stages() {
        let mut names = std::collections::BTreeSet::new();
        for case in super::indexed_clock_read_cases::cases() {
            assert!(names.insert(case.name.clone()));
            let p = opendvp::plc::compile_il(&case.source)
                .unwrap_or_else(|e| panic!("{}: {e}", case.name));
            let encoded =
                opendvp::es2_encoder::encode(&p).unwrap_or_else(|e| panic!("{}: {e}", case.name));
            assert!(encoded.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS);
            assert!(encoded.words.len() <= opendvp::es2_encoder::MAX_PROGRAM_WORDS);
            for limit in [4999, 9999] {
                let mut sim = opendvp::plc::Simulator::default();
                sim.control.profile = opendvp::program_control::ControlProfile::InstalledLegacy;
                sim.memory.sfc.profile = opendvp::sfc::SfcProfile::InstalledLegacy;
                sim.memory.random_tail_profile =
                    opendvp::random_tail::RandomTailProfile::CurrentCommgr;
                sim.memory.data_register_limit = Some(limit);
                sim.memory
                    .system_relays
                    .set_profile(opendvp::system_relays::ClockProfile::InstalledFixed);
                sim.memory.set_system(1039, true);
                sim.memory.set_word("D1039".parse().unwrap(), 100);
                opendvp::system_relays::ClockState::seed_installed_levels(
                    &mut sim.memory,
                    [true, false, true, false],
                )
                .unwrap();
                for (inputs, count) in &case.stages {
                    for (input, value) in inputs {
                        sim.memory.set_input(input.parse().unwrap(), *value);
                    }
                    for _ in 0..*count {
                        sim.scan(&p, 0);
                        assert_eq!(
                            sim.last_scan,
                            opendvp::plc::ScanStatus::Complete,
                            "{}",
                            case.name
                        );
                        assert!(
                            sim.memory.execution_limitations.is_empty(),
                            "{}: {:?}",
                            case.name,
                            sim.memory.execution_limitations
                        );
                    }
                }
            }
        }
        assert_eq!(names.len(), 66);
    }
    #[test]
    fn indexed_bit_number_corpus_admission_checks_every_stage_and_original_header() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let mut names = std::collections::BTreeSet::new();
            for case in super::indexed_bit_number_cases::common_cases()
                .into_iter()
                .chain(super::indexed_bit_number_cases::model_cases(target))
            {
                assert!(names.insert(case.name.clone()), "{}", case.name);
                let p = opendvp::plc::compile_il(&case.source)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let e = opendvp::es2_encoder::encode(&p)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                assert!(
                    e.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                    "{}",
                    case.name
                );
                assert!(
                    e.preflight.values().any(|c| matches!(
                        c,
                        opendvp::es2_encoder::PreflightCheck::IndexedBitNumberAction(_)
                            | opendvp::es2_encoder::PreflightCheck::IndexedBitNumberPredicate(_)
                    )),
                    "{}",
                    case.name
                );
                let mut sim = opendvp::plc::Simulator::default();
                sim.control.profile = opendvp::program_control::ControlProfile::InstalledLegacy;
                sim.memory.sfc.profile = opendvp::sfc::SfcProfile::InstalledLegacy;
                sim.memory.random_tail_profile =
                    opendvp::random_tail::RandomTailProfile::CurrentCommgr;
                sim.memory.data_register_limit = Some(if target == super::Target::Ss2 {
                    4999
                } else {
                    9999
                });
                for (inputs, count) in &case.stages {
                    for (input, value) in inputs {
                        sim.memory.set_input(input.parse().unwrap(), *value);
                    }
                    for _ in 0..*count {
                        sim.scan(&p, 0);
                        assert_eq!(
                            sim.last_scan,
                            opendvp::plc::ScanStatus::Complete,
                            "{}",
                            case.name
                        );
                        assert!(
                            sim.memory.execution_limitations.is_empty(),
                            "{}: {:?}",
                            case.name,
                            sim.memory.execution_limitations
                        );
                        if case.name.contains("literal_dynamic_modulo_") {
                            for index in 840..=846 {
                                assert!(
                                    sim.memory.bit(opendvp::plc::Device {
                                        area: opendvp::plc::Area::M,
                                        index
                                    }),
                                    "{} independent literal oracle M{index}",
                                    case.name
                                );
                            }
                        }
                    }
                }
            }
            assert!(!names.is_empty());
        }
    }
    #[test]
    fn indexed_abs_gray_corpus_has_unique_names_and_encodes_all_declared_spans() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let mut names = std::collections::BTreeSet::new();
            for case in super::indexed_abs_gray_cases::common_cases()
                .into_iter()
                .chain(super::indexed_abs_gray_cases::model_cases(target))
            {
                assert!(names.insert(case.name.clone()), "{}", case.name);
                let p = opendvp::plc::compile_il(&case.source)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let e = opendvp::es2_encoder::encode(&p)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                assert!(
                    e.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                    "{}",
                    case.name
                );
                assert!(
                    e.preflight.values().any(|c| matches!(
                        c,
                        opendvp::es2_encoder::PreflightCheck::IndexedAbsGray(_)
                    )),
                    "{}",
                    case.name
                );
                let mut sim = opendvp::plc::Simulator::default();
                sim.control.profile = opendvp::program_control::ControlProfile::InstalledLegacy;
                sim.memory.sfc.profile = opendvp::sfc::SfcProfile::InstalledLegacy;
                sim.memory.random_tail_profile =
                    opendvp::random_tail::RandomTailProfile::CurrentCommgr;
                sim.memory.data_register_limit = Some(if target == super::Target::Ss2 {
                    4999
                } else {
                    9999
                });
                for (inputs, count) in &case.stages {
                    for (input, value) in inputs {
                        sim.memory.set_input(input.parse().unwrap(), *value);
                    }
                    for _ in 0..*count {
                        sim.scan(&p, 0);
                        assert_eq!(
                            sim.last_scan,
                            opendvp::plc::ScanStatus::Complete,
                            "{}",
                            case.name
                        );
                        assert!(
                            sim.memory.execution_limitations.is_empty(),
                            "{}: {:?}",
                            case.name,
                            sim.memory.execution_limitations
                        );
                    }
                }
            }
            assert!(!names.is_empty());
        }
    }
    #[test]
    fn indexed_swap_mask_corpus_has_unique_names_and_encodes_all_declared_spans() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let mut names = std::collections::BTreeSet::new();
            for case in super::indexed_swap_mask_cases::common_cases()
                .into_iter()
                .chain(super::indexed_swap_mask_cases::model_cases(target))
            {
                assert!(names.insert(case.name.clone()), "{}", case.name);
                let p = opendvp::plc::compile_il(&case.source)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let e = opendvp::es2_encoder::encode(&p)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                assert!(
                    e.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                    "{}",
                    case.name
                );
                assert!(
                    e.preflight.values().any(|c| matches!(
                        c,
                        opendvp::es2_encoder::PreflightCheck::IndexedSwapMask(_)
                    )),
                    "{}",
                    case.name
                );
            }
            assert!(!names.is_empty());
        }
    }
    #[test]
    fn indexed_literal_move_corpus_has_unique_names_and_encodes_two_roles() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let mut names = std::collections::BTreeSet::new();
            for case in super::indexed_literal_move_cases::cases()
                .into_iter()
                .chain(super::indexed_literal_move_cases::model_cases(target))
            {
                assert!(names.insert(case.name.clone()), "{}", case.name);
                let p = opendvp::plc::compile_il(&case.source)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let e = opendvp::es2_encoder::encode(&p)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                assert!(
                    e.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                    "{}",
                    case.name
                );
                assert!(
                    e.preflight.values().any(|c| matches!(
                        c,
                        opendvp::es2_encoder::PreflightCheck::IndexedLiteralMove(_)
                    )),
                    "{}",
                    case.name
                );
            }
            assert!(!names.is_empty());
        }
    }
    #[test]
    fn indexed_trig_corpus_has_unique_names_and_encodes_two_roles() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let mut names = std::collections::BTreeSet::new();
            for case in super::indexed_trig_cases::cases()
                .into_iter()
                .chain(super::indexed_trig_cases::model_cases(target))
            {
                assert!(names.insert(case.name.clone()), "{}", case.name);
                let p = opendvp::plc::compile_il(&case.source)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let e = opendvp::es2_encoder::encode(&p)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                assert!(
                    e.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                    "{}",
                    case.name
                );
                assert!(
                    e.preflight
                        .values()
                        .any(|c| matches!(c, opendvp::es2_encoder::PreflightCheck::IndexedTrig(_))),
                    "{}",
                    case.name
                );
            }
            assert!(!names.is_empty());
        }
    }
    #[test]
    fn indexed_angle_corpus_has_unique_names_and_encodes_two_roles() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let mut names = std::collections::BTreeSet::new();
            for case in super::indexed_angle_cases::cases()
                .into_iter()
                .chain(super::indexed_angle_cases::model_cases(target))
            {
                assert!(names.insert(case.name.clone()), "{}", case.name);
                let p = opendvp::plc::compile_il(&case.source)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let e = opendvp::es2_encoder::encode(&p)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                assert!(
                    e.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                    "{}",
                    case.name
                );
                assert!(
                    e.preflight.values().any(|c| matches!(
                        c,
                        opendvp::es2_encoder::PreflightCheck::IndexedAngle(_)
                    )),
                    "{}",
                    case.name
                );
            }
            assert!(!names.is_empty());
        }
    }
    #[test]
    fn indexed_log_pow_corpus_has_unique_names_and_encodes_staged_preflight() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let mut names = std::collections::BTreeSet::new();
            for case in super::indexed_log_pow_cases::cases()
                .into_iter()
                .chain(super::indexed_log_pow_cases::model_cases(target))
            {
                assert!(names.insert(case.name.clone()), "{}", case.name);
                let p = opendvp::plc::compile_il(&case.source)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let e = opendvp::es2_encoder::encode(&p)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                assert!(
                    e.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                    "{}",
                    case.name
                );
                assert!(
                    e.preflight.values().any(|c| matches!(
                        c,
                        opendvp::es2_encoder::PreflightCheck::IndexedLogPow(_)
                    )),
                    "{}",
                    case.name
                );
            }
            assert!(!names.is_empty());
        }
    }
    #[test]
    fn indexed_real_corpus_has_unique_names_and_encodes_staged_preflight() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let mut names = std::collections::BTreeSet::new();
            for case in super::indexed_real_cases::cases()
                .into_iter()
                .chain(super::indexed_real_cases::model_cases(target))
            {
                assert!(names.insert(case.name.clone()), "{}", case.name);
                let p = opendvp::plc::compile_il(&case.source)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let e = opendvp::es2_encoder::encode(&p)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                assert!(
                    e.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                    "{}",
                    case.name
                );
                assert!(
                    e.preflight.values().any(|c| matches!(
                        c,
                        opendvp::es2_encoder::PreflightCheck::IndexedRealBinary(_)
                    )),
                    "{}",
                    case.name
                );
            }
            assert!(!names.is_empty());
        }
    }
    #[test]
    fn decimal_scratch_profile_corpus_explicitly_observes_the_public_sentinels() {
        assert!(super::decimal_scratch_profile_cases::cases(super::Target::Ss2).is_empty());
        let mut names = std::collections::BTreeSet::new();
        for case in super::decimal_scratch_profile_cases::cases(super::Target::Es2) {
            assert!(names.insert(case.name.clone()));
            let p = opendvp::plc::compile_il(&case.source)
                .unwrap_or_else(|e| panic!("{}: {e}", case.name));
            assert!(
                p.devices.contains(&"D5000".parse().unwrap())
                    && p.devices.contains(&"D5001".parse().unwrap())
            );
            assert!(opendvp::es2_encoder::encode(&p).is_ok(), "{}", case.name);
        }
        assert!(!names.is_empty());
    }
    #[test]
    fn indexed_float_binary_corpus_encodes_with_preflight_and_unique_names() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let mut names = std::collections::BTreeSet::new();
            for case in super::indexed_float_binary_cases::cases()
                .into_iter()
                .chain(super::indexed_float_binary_cases::model_cases(target))
            {
                assert!(names.insert(case.name.clone()), "{}", case.name);
                let p = opendvp::plc::compile_il(&case.source)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let e = opendvp::es2_encoder::encode(&p)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                assert!(
                    e.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                    "{}",
                    case.name
                );
                assert!(
                    e.preflight.values().any(|c| matches!(
                        c,
                        opendvp::es2_encoder::PreflightCheck::IndexedFloatBinary(_)
                    )),
                    "{}",
                    case.name
                );
            }
            assert!(!names.is_empty());
        }
    }
    #[test]
    fn indexed_root_corpus_has_unique_names_and_encodes_with_preflight() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let mut names = std::collections::BTreeSet::new();
            for case in super::indexed_root_cases::cases()
                .into_iter()
                .chain(super::indexed_root_cases::model_cases(target))
            {
                assert!(names.insert(case.name.clone()), "{}", case.name);
                let p = opendvp::plc::compile_il(&case.source)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let e = opendvp::es2_encoder::encode(&p)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                assert!(
                    e.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                    "{}",
                    case.name
                );
                assert!(
                    e.preflight
                        .values()
                        .any(|c| matches!(c, opendvp::es2_encoder::PreflightCheck::IndexedRoot(_))),
                    "{}",
                    case.name
                );
            }
            assert!(!names.is_empty());
        }
    }
    #[test]
    fn indexed_float_corpus_encodes_real_access_widths_and_model_endpoints() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let mut names = std::collections::BTreeSet::new();
            for case in super::indexed_float_cases::cases()
                .into_iter()
                .chain(super::indexed_float_cases::model_cases(target))
            {
                assert!(names.insert(case.name.clone()), "{}", case.name);
                let p = opendvp::plc::compile_il(&case.source)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let e = opendvp::es2_encoder::encode(&p)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                assert!(
                    e.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                    "{}",
                    case.name
                );
                assert!(
                    e.preflight.values().any(|c| matches!(
                        c,
                        opendvp::es2_encoder::PreflightCheck::IndexedFloat(_)
                    )),
                    "{}",
                    case.name
                );
            }
            assert!(!names.is_empty());
        }
    }
    #[test]
    fn indexed_bcd_corpus_has_unique_names_and_encodes_with_preflight() {
        let mut names = std::collections::BTreeSet::new();
        for case in super::indexed_bcd_cases::cases() {
            assert!(names.insert(case.name.clone()), "{}", case.name);
            let p = opendvp::plc::compile_il(&case.source)
                .unwrap_or_else(|e| panic!("{}: {e}", case.name));
            let e =
                opendvp::es2_encoder::encode(&p).unwrap_or_else(|e| panic!("{}: {e}", case.name));
            assert!(
                e.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                "{}",
                case.name
            );
            assert!(
                e.preflight
                    .values()
                    .any(|c| matches!(c, opendvp::es2_encoder::PreflightCheck::IndexedBcd(_))),
                "{}",
                case.name
            );
        }
        assert!(!names.is_empty());
    }
    #[test]
    fn m1019_corpus_compiles_with_complete_public_spans_and_bounded_steps() {
        let mut names = std::collections::BTreeSet::new();
        for case in super::m1019_cases::cases() {
            assert!(names.insert(case.name.clone()), "{}", case.name);
            let p = opendvp::plc::compile_il(&case.source)
                .unwrap_or_else(|e| panic!("{}: {e}", case.name));
            let e =
                opendvp::es2_encoder::encode(&p).unwrap_or_else(|e| panic!("{}: {e}", case.name));
            assert!(
                e.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                "{}",
                case.name
            );
        }
        assert!(!names.is_empty());
    }
    #[test]
    fn indexed_truncated_corpus_encodes_single_result_actions_with_preflight() {
        let mut names = std::collections::BTreeSet::new();
        for case in super::indexed_truncated_cases::cases() {
            assert!(names.insert(case.name.clone()), "{}", case.name);
            let p = opendvp::plc::compile_il(&case.source)
                .unwrap_or_else(|e| panic!("{}: {e}", case.name));
            let e =
                opendvp::es2_encoder::encode(&p).unwrap_or_else(|e| panic!("{}: {e}", case.name));
            assert!(p.requires_vm(), "{}", case.name);
            assert!(
                e.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                "{}",
                case.name
            );
            assert!(
                e.preflight.values().any(|v| matches!(
                    v,
                    opendvp::es2_encoder::PreflightCheck::IndexedTruncated(_)
                )),
                "{}",
                case.name
            );
        }
        assert!(!names.is_empty());
    }
    #[test]
    fn indexed_multiply_corpus_encodes_each_typed_action_with_preflight() {
        let mut names = std::collections::BTreeSet::new();
        for case in super::indexed_multiply_cases::cases() {
            assert!(names.insert(case.name.clone()), "{}", case.name);
            let p = opendvp::plc::compile_il(&case.source)
                .unwrap_or_else(|e| panic!("{}: {e}", case.name));
            let e =
                opendvp::es2_encoder::encode(&p).unwrap_or_else(|e| panic!("{}: {e}", case.name));
            assert!(p.requires_vm(), "{}", case.name);
            assert!(
                e.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                "{}",
                case.name
            );
            assert!(
                e.preflight
                    .values()
                    .any(|v| matches!(v, opendvp::es2_encoder::PreflightCheck::IndexedMultiply(_))),
                "{}",
                case.name
            );
        }
        assert!(!names.is_empty());
    }
    #[test]
    fn dcnt_reset_corpus_compiles_with_original_tag_and_bounded_steps() {
        let cases = super::dcnt_reset_cases::cases();
        assert_eq!(cases.len(), 64);
        for case in cases {
            let p = opendvp::plc::compile_il(&case.source)
                .unwrap_or_else(|e| panic!("{}: {e}", case.name));
            let e =
                opendvp::es2_encoder::encode(&p).unwrap_or_else(|e| panic!("{}: {e}", case.name));
            assert!(
                e.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                "{}",
                case.name
            );
            let counter_pc: Vec<_> = p
                .networks
                .iter()
                .flat_map(|n| &n.actions)
                .zip(&e.action_pc)
                .filter_map(|(a, pc)| {
                    matches!(a, opendvp::plc::Action::DoubleCounter(_)).then_some(*pc)
                })
                .collect();
            assert_eq!(counter_pc.len(), 1, "{}", case.name);
            assert_eq!(e.words[usize::from(counter_pc[0]) + 1] & 255, 0x83);
        }
    }
    #[test]
    fn indexed_bit_and_counter_reset_corpora_compile_within_native_budget() {
        for (cases, count) in [
            (super::indexed_bit_cases::cases(), 147),
            (super::reset_counter_operand_cases::cases(), 96),
        ] {
            let mut names = std::collections::BTreeSet::new();
            for case in cases {
                assert!(names.insert(case.name.clone()), "{}", case.name);
                let p = opendvp::plc::compile_il(&case.source)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let e = opendvp::es2_encoder::encode(&p)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                assert!(
                    e.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                    "{}",
                    case.name
                );
            }
            assert_eq!(names.len(), count);
        }
    }
    #[test]
    fn indexed_comparison_corpus_compiles_with_original_descriptors_and_vm() {
        let mut names = std::collections::BTreeSet::new();
        for case in super::indexed_comparison_cases::cases() {
            assert!(names.insert(case.name.clone()), "{}", case.name);
            let program = opendvp::plc::compile_il(&case.source)
                .unwrap_or_else(|e| panic!("{}: {e}", case.name));
            assert!(program.requires_vm(), "{}", case.name);
            let encoded = opendvp::es2_encoder::encode(&program)
                .unwrap_or_else(|e| panic!("{}: {e}", case.name));
            assert!(
                encoded.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                "{}",
                case.name
            );
            assert!(
                encoded.preflight.values().any(|v| matches!(
                    v,
                    opendvp::es2_encoder::PreflightCheck::IndexedComparison(_)
                        | opendvp::es2_encoder::PreflightCheck::IndexedPredicate(_)
                )),
                "{}",
                case.name
            );
        }
        assert_eq!(names.len(), 194);
    }
    #[test]
    fn indexed_scalar_corpus_compiles_encodes_and_keeps_original_addresses() {
        let mut names = std::collections::BTreeSet::new();
        for case in super::indexed_scalar_cases::cases() {
            assert!(names.insert(case.name.clone()), "{}", case.name);
            let program = opendvp::plc::compile_il(&case.source)
                .unwrap_or_else(|e| panic!("{}: {e}", case.name));
            let encoded = opendvp::es2_encoder::encode(&program)
                .unwrap_or_else(|e| panic!("{}: {e}", case.name));
            assert!(
                encoded.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                "{}",
                case.name
            );
            assert!(
                encoded
                    .preflight
                    .values()
                    .any(|v| matches!(v, opendvp::es2_encoder::PreflightCheck::IndexedScalar(_))),
                "{}",
                case.name
            );
        }
    }
    #[test]
    fn solar_and_text_operand_cases_compile_with_their_target_capabilities() {
        let mut names = std::collections::BTreeSet::new();
        for (cases, current) in [
            (super::dtm_solar_cases::cases(), true),
            (super::text_operand_cases::cases(), false),
        ] {
            for case in cases {
                assert!(names.insert(case.name.clone()), "{}", case.name);
                let program = opendvp::plc::compile_il(&case.source)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let profile = if current {
                    opendvp::data_transform::NativeTransformProfile::CurrentEs2
                } else {
                    opendvp::data_transform::NativeTransformProfile::InstalledLegacy
                };
                let encoded = opendvp::es2_encoder::encode_for_transform_profile(&program, profile)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                assert!(
                    encoded.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                    "{}",
                    case.name
                );
                if current {
                    assert!(
                        opendvp::es2_encoder::encode(&program).is_err()
                            || case.source.contains("D840 D841")
                    );
                }
            }
        }
    }
    #[test]
    fn modern_transform_cases_encode_only_with_current_native_capability() {
        let mut names = std::collections::BTreeSet::new();
        for case in super::dtm_modern_cases::cases() {
            assert!(names.insert(case.name.clone()));
            let program = opendvp::plc::compile_il(&case.source)
                .unwrap_or_else(|e| panic!("{}: {e}", case.name));
            let encoded = opendvp::es2_encoder::encode_for_transform_profile(
                &program,
                opendvp::data_transform::NativeTransformProfile::CurrentEs2,
            )
            .unwrap_or_else(|e| panic!("{}: {e}", case.name));
            assert!(
                encoded.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                "{}",
                case.name
            );
        }
        assert!(names.len() >= 156);
    }
    #[test]
    fn search_and_float_move_operand_scenarios_compile_and_encode() {
        let mut names = std::collections::BTreeSet::new();
        for case in super::search_operand_cases::cases()
            .into_iter()
            .chain(super::float_move_operand_cases::cases())
        {
            assert!(names.insert(case.name.clone()));
            let program = opendvp::plc::compile_il(&case.source)
                .unwrap_or_else(|e| panic!("{}: {e}", case.name));
            let encoded = opendvp::es2_encoder::encode(&program)
                .unwrap_or_else(|e| panic!("{}: {e}", case.name));
            assert!(
                encoded.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                "{}",
                case.name
            );
        }
    }
    #[test]
    fn cmpt_profile_scenarios_compile_with_all_observed_mode_inputs() {
        for target in [super::Target::Es2, super::Target::Ss2] {
            let mut names = std::collections::BTreeSet::new();
            for case in super::cmpt_profile_cases::cases(target) {
                assert!(names.insert(case.name.clone()));
                let program = opendvp::plc::compile_il(&case.source)
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name));
                let encoded = opendvp::es2_encoder::encode(&program).unwrap();
                assert!(encoded.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS);
            }
        }
    }
    #[test]
    fn generated_additional_scenarios_compile_encode_and_keep_unique_names() {
        let mut names = std::collections::BTreeSet::new();
        let cases = super::additional_operand_cases::cases();
        assert_eq!(cases.len(), 82);
        for case in cases {
            assert!(names.insert(case.name.clone()), "duplicate {}", case.name);
            let program = opendvp::plc::compile_il(&case.source)
                .unwrap_or_else(|error| panic!("{}: {error}", case.name));
            let encoded = opendvp::es2_encoder::encode(&program)
                .unwrap_or_else(|error| panic!("{}: {error}", case.name));
            assert!(encoded.words.len() <= 7920, "{}", case.name);
            assert!(
                encoded.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                "{}",
                case.name
            );
        }
    }
    #[test]
    fn generated_data_scenarios_compile_encode_and_keep_unique_names() {
        let mut names = std::collections::BTreeSet::new();
        for case in super::data_operand_cases::cases() {
            assert!(names.insert(case.name.clone()), "duplicate {}", case.name);
            let program = opendvp::plc::compile_il(&case.source)
                .unwrap_or_else(|error| panic!("{}: {error}", case.name));
            let encoded = opendvp::es2_encoder::encode(&program)
                .unwrap_or_else(|error| panic!("{}: {error}", case.name));
            assert!(encoded.words.len() <= 7920, "{}", case.name);
            assert!(
                encoded.next_pc.len() <= opendvp::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS,
                "{}",
                case.name
            );
        }
        assert!(names.len() >= 52);
    }
    #[test]
    fn generated_shift_scenarios_compile_and_encode() {
        for case in super::shift_operand_cases::cases() {
            let program = opendvp::plc::compile_il(&case.source)
                .unwrap_or_else(|error| panic!("{}: {error}", case.name));
            let encoded = opendvp::es2_encoder::encode(&program)
                .unwrap_or_else(|error| panic!("{}: {error}", case.name));
            assert!(encoded.words.len() <= 7920, "{}", case.name);
        }
    }
    #[test]
    fn generated_multiply_scenarios_compile_and_encode() {
        for case in super::multiply_operand_cases::cases()
            .into_iter()
            .chain(super::multiply_operand_cases::diagnostic_cases())
        {
            let program = opendvp::plc::compile_il(&case.source)
                .unwrap_or_else(|error| panic!("{}: {error}", case.name));
            let encoded = opendvp::es2_encoder::encode(&program)
                .unwrap_or_else(|error| panic!("{}: {error}", case.name));
            assert!(encoded.words.len() <= 7920, "{}", case.name);
        }
    }
    #[test]
    fn generated_bit_scenarios_compile_encode_and_keep_unique_names() {
        let mut names = std::collections::BTreeSet::new();
        for case in super::bit_operand_cases::cases() {
            assert!(names.insert(case.name.clone()), "duplicate {}", case.name);
            assert!(case.name.starts_with("bit_operand_"), "{}", case.name);
            let program = opendvp::plc::compile_il(&case.source)
                .unwrap_or_else(|error| panic!("{}: {error}", case.name));
            let encoded = opendvp::es2_encoder::encode(&program)
                .unwrap_or_else(|error| panic!("{}: {error}", case.name));
            assert!(encoded.words.len() <= 7920, "{}", case.name);
        }
        assert!(names.len() >= 74);
    }
    #[test]
    fn generated_exchange_and_block_scenarios_compile_and_encode() {
        for case in super::exchange_operand_cases::cases()
            .into_iter()
            .chain(super::block_operand_cases::cases())
        {
            let program = opendvp::plc::compile_il(&case.source)
                .unwrap_or_else(|error| panic!("{}: {error}", case.name));
            let encoded = opendvp::es2_encoder::encode(&program)
                .unwrap_or_else(|error| panic!("{}: {error}", case.name));
            assert!(encoded.words.len() <= 7920, "{}", case.name);
        }
    }
}

#[cfg(test)]
mod segment_corpus_admission_tests {
    use super::*;
    #[test]
    fn indexed_segment_all_native_candidates_compile_and_satisfy_every_local_literal_stage() {
        for target in [Target::Es2, Target::Ss2] {
            let cases = indexed_segment_cases::common_cases()
                .into_iter()
                .chain(indexed_segment_cases::model_cases(target))
                .collect::<Vec<_>>();
            assert_eq!(cases.len(), 72);
            let mut names = std::collections::BTreeSet::new();
            for c in cases {
                assert!(c.rtc.is_none());
                assert!(names.insert(c.name.clone()));
                let p = plc::compile_il(&c.source).unwrap_or_else(|e| panic!("{}: {e}", c.name));
                let encoded = opendvp::es2_encoder::encode(&p).unwrap();
                assert!(p.requires_vm());
                assert!(encoded.words.len() < 0x1ef0);
                assert!(encoded.execution_plan.operand_tail_owners.is_empty());
                assert!(
                    encoded.preflight.values().any(|c| matches!(
                        c,
                        opendvp::es2_encoder::PreflightCheck::IndexedSegment(_)
                    ))
                );
                let mut sim = opendvp::plc::Simulator::default();
                sim.control.profile = opendvp::program_control::ControlProfile::InstalledLegacy;
                sim.memory.sfc.profile = opendvp::sfc::SfcProfile::InstalledLegacy;
                sim.memory.data_register_limit =
                    Some(if target == Target::Ss2 { 4999 } else { 9999 });
                let mut scans = 0;
                for (inputs, count) in c.stages {
                    for (input, value) in inputs {
                        sim.memory.set_input(input.parse().unwrap(), value);
                    }
                    for _ in 0..count {
                        sim.scan(&p, 0);
                        scans += 1;
                        assert_eq!(
                            sim.last_scan,
                            opendvp::plc::ScanStatus::Complete,
                            "{}",
                            c.name
                        );
                        assert!(
                            sim.memory.execution_limitations.is_empty(),
                            "{}: {:?}",
                            c.name,
                            sim.memory.execution_limitations
                        );
                        for index in 840..=847 {
                            assert!(
                                sim.memory.bit(Device {
                                    area: Area::M,
                                    index
                                }),
                                "{} scan{scans} independent marker M{index}",
                                c.name
                            );
                        }
                        assert_eq!(
                            sim.memory.word("D308".parse().unwrap()),
                            255,
                            "{} scan{scans}",
                            c.name
                        );
                    }
                }
                assert_eq!(scans, 5);
            }
        }
    }
}

#[cfg(test)]
mod checksum_corpus_admission_tests {
    use super::*;
    #[test]
    fn all_checksum_candidates_compile_and_match_independent_literal_stages() {
        for target in [Target::Es2, Target::Ss2] {
            let cases = indexed_checksum_cases::common_cases()
                .into_iter()
                .chain(indexed_checksum_cases::model_cases(target))
                .collect::<Vec<_>>();
            assert_eq!(cases.len(), 78);
            assert_eq!(
                cases
                    .iter()
                    .filter(|c| c.name.contains("literal_oracle_"))
                    .count(),
                74
            );
            let mut names = std::collections::BTreeSet::new();
            for c in cases {
                assert!(names.insert(c.name.clone()));
                let p = plc::compile_il(&c.source).unwrap_or_else(|e| panic!("{}: {e}", c.name));
                let encoded = opendvp::es2_encoder::encode(&p).unwrap();
                assert!(p.requires_vm());
                assert!(encoded.words.len() < 0x1ef0);
                assert!(encoded.execution_plan.operand_tail_owners.is_empty());
                assert!(encoded.preflight.values().any(|c| matches!(
                    c,
                    opendvp::es2_encoder::PreflightCheck::IndexedChecksum(_)
                )));
                let mut sim = opendvp::plc::Simulator::default();
                sim.control.profile = opendvp::program_control::ControlProfile::InstalledLegacy;
                sim.memory.sfc.profile = opendvp::sfc::SfcProfile::InstalledLegacy;
                sim.memory.data_register_limit =
                    Some(if target == Target::Ss2 { 4999 } else { 9999 });
                let mut scan = 0;
                for (inputs, count) in c.stages {
                    for (input, value) in inputs {
                        sim.memory.set_input(input.parse().unwrap(), value);
                    }
                    for _ in 0..count {
                        sim.scan(&p, 0);
                        scan += 1;
                        assert_eq!(
                            sim.last_scan,
                            opendvp::plc::ScanStatus::Complete,
                            "{}",
                            c.name
                        );
                        assert!(
                            sim.memory.execution_limitations.is_empty(),
                            "{}: {:?}",
                            c.name,
                            sim.memory.execution_limitations
                        );
                        if c.name.contains("literal_oracle_") {
                            for index in 840..=847 {
                                assert!(
                                    sim.memory.bit(Device {
                                        area: Area::M,
                                        index
                                    }),
                                    "{} scan{scan}: M{index}, actual code/PC={}/{}, literal code/PC={}/{}",
                                    c.name,
                                    sim.memory.word("D1067".parse().unwrap()),
                                    sim.memory.word("D1068".parse().unwrap()),
                                    sim.memory.word("D282".parse().unwrap()),
                                    sim.memory.word("D283".parse().unwrap())
                                );
                            }
                            assert_eq!(
                                sim.memory.word("D308".parse().unwrap()),
                                255,
                                "{} scan{scan}",
                                c.name
                            );
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod text_corpus_admission_tests {
    use super::*;
    #[test]
    fn all_text_candidates_compile_and_match_independent_literal_stages() {
        for target in [Target::Es2, Target::Ss2] {
            let cases = indexed_text_cases::common_cases()
                .into_iter()
                .chain(indexed_text_cases::model_cases(target))
                .collect::<Vec<_>>();
            assert_eq!(cases.len(), 224);
            assert_eq!(
                cases
                    .iter()
                    .filter(|c| c.name.contains("literal_oracle_"))
                    .count(),
                218
            );
            let mut names = std::collections::BTreeSet::new();
            for c in cases {
                assert!(names.insert(c.name.clone()));
                let p = plc::compile_il(&c.source).unwrap_or_else(|e| panic!("{}: {e}", c.name));
                let encoded = opendvp::es2_encoder::encode(&p).unwrap();
                assert!(p.requires_vm());
                assert!(encoded.words.len() < 0x1ef0);
                assert!(encoded.execution_plan.operand_tail_owners.is_empty());
                assert!(
                    encoded
                        .preflight
                        .values()
                        .any(|c| matches!(c, opendvp::es2_encoder::PreflightCheck::IndexedText(_)))
                );
                let mut sim = opendvp::plc::Simulator::default();
                sim.control.profile = opendvp::program_control::ControlProfile::InstalledLegacy;
                sim.memory.sfc.profile = opendvp::sfc::SfcProfile::InstalledLegacy;
                sim.memory.data_register_limit =
                    Some(if target == Target::Ss2 { 4999 } else { 9999 });
                let mut scan = 0;
                for (inputs, count) in c.stages {
                    for (input, value) in inputs {
                        sim.memory.set_input(input.parse().unwrap(), value);
                    }
                    for _ in 0..count {
                        sim.scan(&p, 0);
                        scan += 1;
                        assert_eq!(
                            sim.last_scan,
                            opendvp::plc::ScanStatus::Complete,
                            "{}",
                            c.name
                        );
                        assert!(
                            sim.memory.execution_limitations.is_empty(),
                            "{}: {:?}",
                            c.name,
                            sim.memory.execution_limitations
                        );
                        if c.name.contains("literal_oracle_") {
                            for index in 840..=847 {
                                assert!(
                                    sim.memory.bit(Device {
                                        area: Area::M,
                                        index
                                    }),
                                    "{} scan{scan}: M{index}",
                                    c.name
                                );
                            }
                            assert_eq!(
                                sim.memory.word("D308".parse().unwrap()),
                                255,
                                "{} scan{scan}",
                                c.name
                            );
                        }
                    }
                }
            }
        }
    }
}
