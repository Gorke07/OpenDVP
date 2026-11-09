#[cfg(test)]
#[macro_use]
mod test_fixtures;
#[cfg(test)]
mod additional_operand_integration_tests;
pub mod additional_processing;
pub mod alarm;
pub mod analog_knob;
#[cfg(test)]
mod arithmetic_operand_integration_tests;
#[cfg(test)]
mod basic_operand_tests;
#[cfg(test)]
mod bcd_operand_integration_tests;
pub mod bit_index_operand;
#[cfg(test)]
mod bit_operand_integration_tests;
pub mod bit_tools;
pub mod block_operand;
#[cfg(test)]
mod block_operand_integration_tests;
pub mod calendar_operand;
pub mod canopen;
pub mod canopen_exchange;
#[cfg(test)]
mod canopen_exchange_integration_tests;
pub mod commgr;
#[cfg(test)]
mod comparison_operand_integration_tests;
pub mod coordinated_motion;
#[cfg(test)]
mod coordinated_motion_integration_tests;
pub mod counter;
#[cfg(test)]
mod counter_integration_tests;
#[cfg(test)]
mod counter_interrupt_integration_tests;
#[cfg(test)]
mod data_operand_integration_tests;
pub mod data_processing;
pub mod data_transform;
#[cfg(test)]
mod dcnt_native_replay_tests;
#[cfg(test)]
mod dcnt_reset_integration_tests;
pub mod devices;
pub mod dmv_comm;
pub mod drive_comm;
#[cfg(test)]
mod drive_knob_integration_tests;
pub mod drum;
pub mod dvp_decode;
pub mod dvp_file;
pub mod dvz;
pub mod es2_bridge;
pub mod es2_encoder;
pub mod es2_program_decode;
pub mod es2_protocol;
pub mod ethernet;
pub mod ethernet_config;
#[cfg(test)]
mod ethernet_config_integration_tests;
#[cfg(test)]
mod ethernet_integration_tests;
pub mod ethernet_raw;
#[cfg(test)]
mod ethernet_raw_integration_tests;
#[cfg(test)]
mod exchange_operand_integration_tests;
pub mod execution_plan;
pub mod external_io;
#[cfg(test)]
mod external_io_integration_tests;
pub mod file_memory;
pub mod float_advanced;
pub mod float_math;
#[cfg(test)]
mod float_move_operand_integration_tests;
pub mod float_ops;
pub mod ftc;
pub mod gps;
pub mod hardware;
pub mod recipes;
pub mod high_speed;
#[cfg(test)]
mod high_speed_integration_tests;
pub mod hour_meter;
mod il_lex;
pub mod indexed_additional;
#[cfg(test)]
mod indexed_additional_count_low_tests;
#[cfg(test)]
mod indexed_additional_legacy_tag_tests;
#[cfg(test)]
mod indexed_additional_sclp_order_tests;
#[cfg(test)]
mod indexed_additional_vm_tests;
pub mod indexed_address;
pub mod indexed_asc;
#[cfg(test)]
mod indexed_asc_vm_tests;
pub mod indexed_bit;
#[cfg(test)]
mod indexed_bit_tests;
pub mod indexed_checksum;
#[cfg(test)]
mod indexed_checksum_vm_tests;
pub mod indexed_comparison;
#[cfg(test)]
mod indexed_comparison_tests;
#[cfg(test)]
mod indexed_comparison_vm_tests;
pub mod indexed_multiply;
#[cfg(test)]
mod indexed_multiply_vm_tests;
pub mod indexed_operand;
pub mod indexed_rtc;
#[cfg(test)]
mod indexed_rtc_vm_tests;
pub mod indexed_scalar;
#[cfg(test)]
mod indexed_scalar_integration_tests;
#[cfg(test)]
mod indexed_scalar_vm_tests;
pub mod indexed_segment;
#[cfg(test)]
mod indexed_segment_vm_tests;
pub mod indexed_text;
#[cfg(test)]
mod indexed_text_vm_tests;
pub mod indexed_truncated;
#[cfg(test)]
mod indexed_truncated_vm_tests;
pub mod interrupt;
#[cfg(test)]
mod interrupt_integration_tests;
pub mod isp_file;
#[cfg(test)]
mod m1019_integration_tests;
pub mod mask_random_operand;
pub mod matrix_ops;
pub mod module_io;
#[cfg(test)]
mod module_io_integration_tests;
pub mod motion_basic;
#[cfg(test)]
mod motion_basic_integration_tests;
#[cfg(test)]
mod motion_basic_irq_tests;
pub mod motion_extended;
#[cfg(test)]
mod motion_extended_integration_tests;
#[cfg(test)]
mod move_logic_operand_integration_tests;
pub mod multiply_destination;
#[cfg(test)]
mod multiply_operand_integration_tests;
#[cfg(test)]
mod network_commands_integration_tests;
pub mod numeric_contacts;
pub mod parallel_run;
#[cfg(test)]
mod parallel_run_tests;
pub mod pid;
#[cfg(test)]
mod pid_integration_tests;
pub mod plc;
pub mod plc_download;
pub mod i18n;
pub mod settings;
pub mod i18n_en;
pub mod alloc;
pub mod plc_control;
pub mod program_pack;
pub mod plc_force;
pub mod plc_monitor;
pub mod plc_modules;
pub mod plc_program_transfer;
pub mod power_monitor;
pub mod program_control;
pub mod project;
pub mod pu_module;
#[cfg(test)]
mod pu_module_integration_tests;
pub mod pulse_output;
#[cfg(test)]
mod pulse_output_integration_tests;
pub mod ramp_sort;
pub mod random_ops;
pub mod refresh_io;
#[cfg(test)]
mod refresh_io_integration_tests;
#[cfg(test)]
mod rstd_integration_tests;
pub mod rtc_admission;
pub mod rtc_clock;
#[cfg(test)]
mod rtc_service_vm_tests;
#[cfg(test)]
mod search_operand_integration_tests;
pub mod search_ops;
pub mod serial_io;
#[cfg(test)]
mod serial_io_integration_tests;
pub mod serial_protocol;
pub mod servo_comm;
#[cfg(test)]
mod servo_comm_integration_tests;
#[cfg(test)]
mod servo_next_integration_tests;
pub mod sfc;
pub mod sfc_validation;
pub mod shift;
#[cfg(test)]
mod shift_operand_integration_tests;
pub mod solar_position;
pub mod special_registers;
pub mod special_timers;
#[cfg(all(test, windows))]
mod ss2_live_tests;
pub mod st;
#[cfg(test)]
mod st_integration_tests;
pub mod station_compare;
#[cfg(test)]
mod station_string_integration_tests;
pub mod string_move;
#[cfg(test)]
mod system_relay_integration_tests;
pub mod system_relays;
pub mod text_encoding;
#[cfg(test)]
mod text_operand_integration_tests;
pub mod time_ops;
pub mod timing_io;
pub mod wide;
pub mod word_counter;

pub mod indexed_bcd;
#[cfg(test)]
mod indexed_bcd_vm_tests;

pub mod indexed_float;
#[cfg(test)]
mod indexed_float_vm_tests;

pub mod indexed_root;
#[cfg(test)]
mod indexed_root_vm_tests;

pub mod indexed_angle;
#[cfg(test)]
mod indexed_angle_vm_tests;

pub mod indexed_decimal_conversion;
#[cfg(test)]
mod indexed_decimal_conversion_vm_tests;

pub mod indexed_exp_ln;
#[cfg(test)]
mod indexed_exp_ln_vm_tests;

pub mod indexed_log_pow;
#[cfg(test)]
mod indexed_log_pow_vm_tests;

pub mod indexed_desqr;
#[cfg(test)]
mod indexed_desqr_vm_tests;

pub mod indexed_trig;
#[cfg(test)]
mod indexed_trig_vm_tests;

pub mod indexed_float_comparison;
#[cfg(test)]
mod indexed_float_comparison_vm_tests;

pub mod indexed_literal_move;
#[cfg(test)]
mod indexed_literal_move_vm_tests;

#[cfg(test)]
mod drand_vm_visit_tests;

pub mod indexed_swap_mask;
#[cfg(test)]
mod indexed_swap_mask_vm_tests;

pub mod calendar_tail;
#[cfg(test)]
mod calendar_tail_vm_tests;
pub mod indexed_calendar_time;
#[cfg(test)]
mod indexed_calendar_time_vm_tests;
pub mod indexed_random;
#[cfg(test)]
mod indexed_random_vm_tests;
pub mod random_tail;
#[cfg(test)]
mod random_tail_vm_tests;

#[cfg(test)]
mod indexed_clock_read_vm_tests;

pub mod indexed_bit_number;
#[cfg(test)]
mod indexed_bit_number_vm_tests;

pub mod indexed_abs_gray;
#[cfg(test)]
mod indexed_abs_gray_vm_tests;

pub mod indexed_search_toggle;
#[cfg(test)]
mod indexed_search_toggle_vm_tests;

pub mod indexed_data_processing;
#[cfg(test)]
mod indexed_data_processing_vm_tests;

pub mod indexed_shift_fifo;
#[cfg(test)]
mod indexed_shift_fifo_vm_tests;

pub mod indexed_block_exchange;
#[cfg(test)]
mod indexed_block_exchange_vm_tests;

pub mod indexed_rotation;
#[cfg(test)]
mod indexed_rotation_vm_tests;

pub mod indexed_real_binary;
#[cfg(test)]
mod indexed_real_binary_vm_tests;

pub mod indexed_float_binary;
#[cfg(test)]
mod indexed_float_binary_vm_tests;
#[cfg(test)]
mod st_array_tests;
#[cfg(test)]
mod st_blocks_tests;
#[cfg(test)]
mod st_stdlib_logic_tests;
#[cfg(test)]
mod st_stdlib_math_tests;
#[cfg(test)]
mod st_stdlib_control_tests;
#[cfg(test)]
mod st_stdlib_process_tests;
#[cfg(test)]
mod st_share_tests;
#[cfg(test)]
mod st_retain_tests;
