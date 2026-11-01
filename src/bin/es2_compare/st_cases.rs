//! Structured Text source and independent language-level result witnesses.
//! Native parity alone shares the lowerer; literal expected outputs below also
//! detect a wrong lowering that both execution engines would otherwise share.
use super::{Case, case};
use opendvp::es2_bridge::{Reading, Target};

fn source(declarations: &str, body: &str) -> String {
    format!(
        "PROGRAM NativeWitness\nVAR\n enabled AT X0 : BOOL;\n option AT X1 : BOOL;\n third AT X2 : BOOL;\n first AT M1002 : BOOL;\n{declarations}\nEND_VAR\n{body}\nEND_PROGRAM\n"
    )
}
fn fixed(name: &str, declarations: &str, body: &str) -> Case {
    case(
        &format!("st_{name}"),
        &source(declarations, body),
        vec![(vec![], 4)],
    )
}
fn switched(name: &str, declarations: &str, body: &str) -> Case {
    case(
        &format!("st_{name}"),
        &source(declarations, body),
        vec![
            (vec![("X0", false), ("X1", false)], 1),
            (vec![("X0", true), ("X1", true)], 1),
            (vec![("X0", false), ("X1", true)], 1),
            (vec![("X0", false), ("X1", false)], 1),
        ],
    )
}
pub(super) fn cases(_target: Target) -> Vec<Case> {
    let mut out = vec![
        fixed(
            "constant_assignment",
            "value AT D0 : INT; lamp AT Y0 : BOOL;",
            "value := 123; lamp := TRUE;",
        ),
        fixed(
            "integer_precedence",
            "value AT D0 : INT;",
            "value := 3 + 4 * 5 - 6 / 2;",
        ),
        fixed(
            "integer_parentheses",
            "value AT D0 : INT;",
            "value := (3 + 4) * (5 - 2);",
        ),
        fixed(
            "negative_division_modulo",
            "quotient AT D0 : INT; remainder AT D1 : INT;",
            "quotient := -23 / 7; remainder := -23 MOD 7;",
        ),
        fixed(
            "integer_overflow",
            "value AT D0 : INT;",
            "value := 32767; value := value + 1;",
        ),
        fixed(
            "integer_underflow",
            "value AT D0 : INT;",
            "value := -32768; value := value - 1;",
        ),
        fixed(
            "integer_multiply_low_word",
            "value AT D0 : INT;",
            "value := 300 * 300;",
        ),
        fixed(
            "sequential_assignment_alias",
            "value AT D0 : INT; copy AT D1 : INT;",
            "value := 7; value := value + value; copy := value;",
        ),
        fixed(
            "dint_overflow",
            "value AT D0 : DINT; negative AT Y0 : BOOL;",
            "value := 2147483647; value := value + 1; negative := value < 0;",
        ),
        fixed(
            "dint_large_product",
            "value AT D0 : DINT;",
            "value := 70000; value := value * 30000;",
        ),
        fixed(
            "dint_negative_division",
            "value AT D0 : DINT;",
            "value := -70000; value := value / 3;",
        ),
        fixed(
            "dint_low_carry_distinct_operands",
            "left AT D0 : DINT; right AT D2 : DINT; value AT D4 : DINT;",
            "left := 65535; right := 1; value := left + right;",
        ),
        fixed(
            "dint_low_borrow_alias_left",
            "left AT D0 : DINT; right AT D2 : DINT;",
            "left := 65536; right := 1; left := left - right;",
        ),
        fixed(
            "dint_min_subtract_one",
            "value AT D0 : DINT;",
            "value := -2147483648; value := value - 1;",
        ),
        fixed(
            "dint_min_plus_min_alias_both",
            "value AT D0 : DINT;",
            "value := -2147483648; value := value + value;",
        ),
        fixed(
            "dint_nested_carry_borrow",
            "left AT D0 : DINT; right AT D2 : DINT; value AT D4 : DINT;",
            "left := 2147483647; right := -2147483648; value := (left + 65535) - (right - 1);",
        ),
        fixed(
            "dint_constant_min_plus_min",
            "value AT D0 : DINT;",
            "value := -2147483648 + -2147483648;",
        ),
        fixed(
            "dint_zero_minus_min",
            "value AT D0 : DINT;",
            "value := 0 - (-2147483648);",
        ),
        fixed(
            "dint_negative_carry_to_zero",
            "left AT D0 : DINT; right AT D2 : DINT; value AT D4 : DINT;",
            "left := -1; right := 1; value := left + right;",
        ),
        fixed(
            "dint_low_carry_alias_right",
            "left AT D0 : DINT; right AT D2 : DINT;",
            "left := 65535; right := 1; right := left + right;",
        ),
        fixed(
            "unary_minimum_values",
            "wide AT D0 : DINT; wideResult AT D2 : DINT; narrow AT D4 : INT; narrowResult AT D5 : INT;",
            "wide := -2147483648; wideResult := -wide; narrow := -32768; narrowResult := -narrow;",
        ),
        fixed(
            "real_finite_arithmetic_comparison",
            "value AT D0 : REAL; high AT Y0 : BOOL;",
            "value := 1.5; value := (value + 2.5) * 0.5; high := value > 1.5;",
        ),
        fixed(
            "real_negative_division_comparison",
            "value AT D0 : REAL; negative AT Y0 : BOOL;",
            "value := -7.5; value := value / 2.5; negative := value < 0.0;",
        ),
        fixed(
            "word_unsigned_compare",
            "value AT D0 : WORD; high AT Y0 : BOOL;",
            "value := 16#FFFF; high := value > 16#7FFF;",
        ),
        fixed(
            "word_bitwise",
            "value AT D0 : WORD;",
            "value := (16#F0F0 AND 16#0FF0) XOR 16#00FF;",
        ),
        fixed("word_not", "value AT D0 : WORD;", "value := NOT 16#000F;"),
        fixed(
            "dword_unsigned_compare",
            "value AT D0 : DWORD; high AT Y0 : BOOL;",
            "value := 16#FFFFFFFF; high := value > 16#7FFFFFFF;",
        ),
        fixed(
            "dword_bitwise",
            "value AT D0 : DWORD;",
            "value := (16#F0F0F0F0 AND 16#0FF00FF0) XOR 16#000000FF;",
        ),
        fixed(
            "signed_comparison",
            "low AT Y0 : BOOL; equal AT Y1 : BOOL; diff AT Y2 : BOOL;",
            "low := -32768 < 32767; equal := (4 + 3) = 7; diff := -1 <> 1;",
        ),
        fixed(
            "declaration_initializer_persists",
            "value AT D0 : INT := 7;",
            "value := value + 1;",
        ),
        fixed(
            "first_scan_initialization",
            "value AT D0 : INT;",
            "IF first THEN value := 100; END_IF; value := value + 1;",
        ),
        switched(
            "if_elsif_priority",
            "value AT D0 : INT;",
            "IF enabled THEN value := 10; ELSIF option THEN value := 20; ELSE value := 30; END_IF;",
        ),
        switched(
            "nested_if",
            "value AT D0 : INT;",
            "IF enabled THEN IF option THEN value := 11; ELSE value := 12; END_IF; ELSE IF option THEN value := 21; ELSE value := 22; END_IF; END_IF;",
        ),
        switched(
            "case_labels_ranges",
            "selector AT D0 : INT; value AT D1 : INT;",
            "IF enabled THEN selector := -1; ELSIF option THEN selector := 3; ELSE selector := 9; END_IF; CASE selector OF -1, 0: value := 10; 1..4: value := 20; ELSE value := 30; END_CASE;",
        ),
        fixed(
            "for_inclusive",
            "i AT D0 : INT; total AT D1 : INT;",
            "total := 0; FOR i := 1 TO 5 DO total := total + i; END_FOR;",
        ),
        fixed(
            "for_descending",
            "i AT D0 : INT; total AT D1 : INT;",
            "total := 0; FOR i := 10 TO 2 BY -2 DO total := total + i; END_FOR;",
        ),
        fixed(
            "for_zero_iterations",
            "i AT D0 : INT; total AT D1 : INT;",
            "total := 99; FOR i := 6 TO 2 DO total := 0; END_FOR;",
        ),
        fixed(
            "for_captured_bound",
            "i AT D0 : INT; total AT D1 : INT; limit AT D2 : INT;",
            "total := 0; limit := 5; FOR i := 1 TO limit DO total := total + i; limit := 0; END_FOR;",
        ),
        fixed(
            "for_exit",
            "i AT D0 : INT; total AT D1 : INT;",
            "total := 0; FOR i := 1 TO 8 DO IF i = 4 THEN EXIT; END_IF; total := total + i; END_FOR;",
        ),
        fixed(
            "while_sum",
            "i AT D0 : INT; total AT D1 : INT;",
            "i := 0; total := 0; WHILE i < 5 DO total := total + i; i := i + 1; END_WHILE;",
        ),
        fixed(
            "while_zero_iterations",
            "total AT D0 : INT;",
            "total := 17; WHILE FALSE DO total := 0; END_WHILE;",
        ),
        fixed(
            "while_exit",
            "i AT D0 : INT; total AT D1 : INT;",
            "i := 0; total := 0; WHILE i < 10 DO i := i + 1; IF i = 4 THEN EXIT; END_IF; total := total + i; END_WHILE;",
        ),
        fixed(
            "repeat_runs_once",
            "total AT D0 : INT;",
            "total := 0; REPEAT total := total + 1; UNTIL TRUE END_REPEAT;",
        ),
        fixed(
            "repeat_until",
            "i AT D0 : INT; total AT D1 : INT;",
            "i := 0; total := 0; REPEAT i := i + 1; total := total + i; UNTIL i >= 4 END_REPEAT;",
        ),
        fixed(
            "repeat_exit",
            "i AT D0 : INT; total AT D1 : INT;",
            "i := 0; total := 0; REPEAT i := i + 1; IF i = 3 THEN EXIT; END_IF; total := total + i; UNTIL FALSE END_REPEAT;",
        ),
        fixed(
            "nested_exit_is_inner_only",
            "i AT D0 : INT; j AT D1 : INT; total AT D2 : INT;",
            "total := 0; FOR i := 1 TO 3 DO FOR j := 1 TO 3 DO IF j = 2 THEN EXIT; END_IF; total := total + 1; END_FOR; total := total + 10; END_FOR;",
        ),
        switched(
            "return_skips_later_assignment",
            "before AT D0 : INT := 0; after AT D1 : INT := 0;",
            "before := before + 1; IF enabled THEN RETURN; END_IF; after := after + 1;",
        ),
        fixed(
            "unmapped_persistent_local",
            "counter : INT := 5; value AT D0 : INT;",
            "counter := counter + 1; value := counter;",
        ),
        fixed(
            "boolean_local_initializer",
            "stored : BOOL := TRUE; lamp AT Y0 : BOOL;",
            "lamp := stored; stored := NOT stored;",
        ),
        fixed(
            "iec_mapped_addresses",
            "input AT %IX0.0 : BOOL; output AT %QX0.0 : BOOL; value AT %MW0 : INT;",
            "output := NOT input; value := 42;",
        ),
    ];
    out.push(case(
        "st_conditional_assignment_false_holds",
        &source(
            "value AT D0 : INT := 5; lamp AT Y0 : BOOL;",
            "IF enabled THEN value := value + 1; lamp := TRUE; END_IF;",
        ),
        vec![
            (vec![("X0", false)], 2),
            (vec![("X0", true)], 2),
            (vec![("X0", false)], 2),
            (vec![("X0", true)], 1),
        ],
    ));
    out.push(case("st_boolean_truth_table",&source("a AT Y0 : BOOL; b AT Y1 : BOOL; c AT Y2 : BOOL;", "a := enabled AND NOT option; b := enabled XOR option; c := (enabled OR option) AND third;"),(0..8).map(|n|(vec![("X0",n&1!=0),("X1",n&2!=0),("X2",n&4!=0)],1)).collect()));
    out
}

/// Independent ST semantic oracle: check BOTH engines, not just equality.
/// Values refer to raw signed16 D rows; DINT/DWORD high halves are explicit.
pub(super) fn validate_semantics(name: &str, scan: u64, rows: &[Reading]) -> Result<(), String> {
    let Some(name) = name.strip_prefix("st_") else {
        return Ok(());
    };
    let n = scan as i64;
    let expected: Vec<(&str, i64)> = match name {
        "constant_assignment" => vec![("D0", 123), ("Y0", 1)],
        "integer_precedence" => vec![("D0", 20)],
        "integer_parentheses" => vec![("D0", 21)],
        "negative_division_modulo" => vec![("D0", -3), ("D1", -2)],
        "integer_overflow" => vec![("D0", -32768)],
        "integer_underflow" => vec![("D0", 32767)],
        "integer_multiply_low_word" => vec![("D0", 24464)],
        "sequential_assignment_alias" => vec![("D0", 14), ("D1", 14)],
        "dint_overflow" => vec![("D0", 0), ("D1", -32768), ("Y0", 1)],
        "dint_large_product" => {
            let v = 2_100_000_000u32;
            vec![
                ("D0", v as u16 as i16 as i64),
                ("D1", (v >> 16) as u16 as i16 as i64),
            ]
        }
        "dint_negative_division" => vec![("D0", -23333), ("D1", -1)],
        "dint_low_carry_distinct_operands" => vec![("D4", 0), ("D5", 1)],
        "dint_low_borrow_alias_left" => vec![("D0", -1), ("D1", 0)],
        "dint_min_subtract_one" => vec![("D0", -1), ("D1", 32767)],
        "dint_min_plus_min_alias_both" | "dint_constant_min_plus_min" => vec![("D0", 0), ("D1", 0)],
        "dint_nested_carry_borrow" => vec![("D4", -1), ("D5", 0)],
        "dint_zero_minus_min" => vec![("D0", 0), ("D1", -32768)],
        "dint_negative_carry_to_zero" => vec![("D4", 0), ("D5", 0)],
        "dint_low_carry_alias_right" => vec![("D2", 0), ("D3", 1)],
        "unary_minimum_values" => vec![("D2", 0), ("D3", -32768), ("D5", -32768)],
        "real_finite_arithmetic_comparison" => vec![("D0", 0), ("D1", 0x4000), ("Y0", 1)],
        "real_negative_division_comparison" => vec![("D0", 0), ("D1", -16320), ("Y0", 1)],
        "word_unsigned_compare" => vec![("D0", -1), ("Y0", 1)],
        "word_bitwise" => vec![("D0", 0x0f)],
        "word_not" => vec![("D0", -16)],
        "dword_unsigned_compare" => vec![("D0", -1), ("D1", -1), ("Y0", 1)],
        "dword_bitwise" => vec![("D0", 0x0f), ("D1", 0x00f0)],
        "signed_comparison" => vec![("Y0", 1), ("Y1", 1), ("Y2", 1)],
        "declaration_initializer_persists" => vec![("D0", 7 + n)],
        "first_scan_initialization" => vec![("D0", 100 + n)],
        "if_elsif_priority" => vec![("D0", [30, 10, 20, 30][scan as usize - 1])],
        "nested_if" => vec![("D0", [22, 11, 21, 22][scan as usize - 1])],
        "case_labels_ranges" => vec![("D1", [30, 10, 20, 30][scan as usize - 1])],
        "for_inclusive" | "for_captured_bound" => vec![("D1", 15)],
        "for_descending" => vec![("D1", 30)],
        "for_zero_iterations" => vec![("D1", 99)],
        "for_exit" | "while_exit" => vec![("D1", 6)],
        "while_sum" => vec![("D0", 5), ("D1", 10)],
        "while_zero_iterations" => vec![("D0", 17)],
        "repeat_runs_once" => vec![("D0", 1)],
        "repeat_until" => vec![("D0", 4), ("D1", 10)],
        "repeat_exit" => vec![("D0", 3), ("D1", 3)],
        "nested_exit_is_inner_only" => vec![("D2", 33)],
        "return_skips_later_assignment" => vec![("D0", n), ("D1", [1, 1, 2, 3][scan as usize - 1])],
        "unmapped_persistent_local" => vec![("D0", 5 + n)],
        "boolean_local_initializer" => vec![("Y0", i64::from(scan % 2 == 1))],
        "iec_mapped_addresses" => vec![("Y0", 1), ("D0", 42)],
        "conditional_assignment_false_holds" => vec![
            ("D0", [5, 5, 6, 7, 7, 7, 8][scan as usize - 1]),
            ("Y0", i64::from(scan >= 3)),
        ],
        "boolean_truth_table" => {
            let bits = scan - 1;
            let a = bits & 1 != 0;
            let b = bits & 2 != 0;
            let c = bits & 4 != 0;
            vec![
                ("Y0", i64::from(a && !b)),
                ("Y1", i64::from(a ^ b)),
                ("Y2", i64::from((a || b) && c)),
            ]
        }
        _ => return Err(format!("ST semantic witness missing for {name}")),
    };
    for (device, value) in expected {
        let address = device.parse::<opendvp::plc::Device>()?;
        let row = rows
            .iter()
            .find(|r| r.device == address)
            .ok_or_else(|| format!("ST {name}: {device} semantic reading missing"))?;
        let raw = if address.area == opendvp::plc::Area::D {
            u32::from(value as u16)
        } else {
            value as u32
        };
        if row.local != raw || row.original != raw {
            return Err(format!(
                "ST {name} scan{scan}: {device} expected{value} (raw {raw}), local{}, original{}",
                row.local, row.original
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendvp::plc::{self, Area, Simulator};
    use opendvp::program_control::ControlProfile;

    #[test]
    fn st_corpus_has_independent_semantics_in_both_target_profiles() {
        let mut failures = Vec::new();
        for target in [Target::Es2, Target::Ss2] {
            for profile in [ControlProfile::Documented, ControlProfile::InstalledLegacy] {
                'case_loop: for case in cases(target) {
                    let program = match plc::compile_st(&case.source) {
                        Ok(program) => program,
                        Err(error) => {
                            failures.push(format!("{target:?}/{profile:?}/{}: {error}", case.name));
                            continue;
                        }
                    };
                    if !program.source_is_st {
                        failures.push(format!("{}: ST frontend marker missing", case.name));
                        continue;
                    }
                    let mut sim = Simulator::default();
                    sim.control.profile = profile;
                    sim.memory.data_register_limit =
                        Some(if target == Target::Ss2 { 4999 } else { 9999 });
                    for (changes, scans) in &case.stages {
                        for (name, value) in changes {
                            sim.memory.set_input(name.parse().unwrap(), *value);
                        }
                        for _ in 0..*scans {
                            sim.scan(&program, 100);
                            if !sim.memory.execution_limitations.is_empty() {
                                failures.push(format!(
                                    "{target:?}/{profile:?}/{}: {:?}",
                                    case.name, sim.memory.execution_limitations
                                ));
                                continue 'case_loop;
                            }
                            let rows: Vec<_> = program
                                .devices
                                .iter()
                                .map(|device| {
                                    let value = if device.area == Area::D {
                                        u32::from(sim.memory.word(*device) as u16)
                                    } else {
                                        u32::from(sim.memory.bit(*device))
                                    };
                                    Reading {
                                        device: *device,
                                        kind: if device.area == Area::D {
                                            "word"
                                        } else {
                                            "bit"
                                        },
                                        local: value,
                                        original: value,
                                    }
                                })
                                .collect();
                            if let Err(error) = validate_semantics(&case.name, sim.scans, &rows) {
                                failures.push(format!("{target:?}/{profile:?}: {error}"));
                                continue 'case_loop;
                            }
                        }
                    }
                }
            }
        }
        assert!(
            failures.is_empty(),
            "ST independent semantic failures:\n{}",
            failures.join("\n")
        );
    }
}
