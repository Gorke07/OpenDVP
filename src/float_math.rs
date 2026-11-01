//! Binary32 arithmetic and decimal coefficient/exponent conversion.
use crate::float_ops::FloatOperand;
use crate::plc::{Area, Device, Memory};
use crate::wide::double_devices;
use std::collections::BTreeSet;

/// Visibility of DEBCD's intermediate exponent/coefficient storage in the
/// installed reference artifact. This is not a general firmware capability.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DecimalScratchProfile {
    /// Current COMMGR ES2 removes the public pre-clamp scratch stores.
    #[default]
    CurrentPrivate,
    /// WPL artifacts and current COMMGR SS2 retain the two public stores.
    /// SS2's D5000/D5001 are outside its public model range; visibility and
    /// bridge observation are qualified separately from this write behavior.
    LegacyVisible,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MathOp {
    Radians,
    Degrees,
    Decimal,
    Binary,
    Add,
    Subtract,
    Multiply,
    Divide,
    AddReal,
    SubtractReal,
    MultiplyReal,
    DivideReal,
}
impl MathOp {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "DRAD" => Self::Radians,
            "DDEG" => Self::Degrees,
            "DEBCD" => Self::Decimal,
            "DEBIN" => Self::Binary,
            "DEADD" => Self::Add,
            "DESUB" => Self::Subtract,
            "DEMUL" => Self::Multiply,
            "DEDIV" => Self::Divide,
            "DADDR" => Self::AddReal,
            "DSUBR" => Self::SubtractReal,
            "DMULR" => Self::MultiplyReal,
            "DDIVR" => Self::DivideReal,
            _ => return None,
        })
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Radians => "DRAD",
            Self::Degrees => "DDEG",
            Self::Decimal => "DEBCD",
            Self::Binary => "DEBIN",
            Self::Add => "DEADD",
            Self::Subtract => "DESUB",
            Self::Multiply => "DEMUL",
            Self::Divide => "DEDIV",
            Self::AddReal => "DADDR",
            Self::SubtractReal => "DSUBR",
            Self::MultiplyReal => "DMULR",
            Self::DivideReal => "DDIVR",
        }
    }
    pub fn header(self) -> u16 {
        match self {
            Self::Radians => 0xf900,
            Self::Degrees => 0xfb00,
            Self::Decimal => 0xfd00,
            Self::Binary => 0xff00,
            Self::Add => 0x1101,
            Self::Subtract => 0x1301,
            Self::Multiply => 0x1501,
            Self::Divide => 0x1701,
            Self::AddReal => 0x7701,
            Self::SubtractReal => 0x7901,
            Self::MultiplyReal => 0x7b01,
            Self::DivideReal => 0x7d01,
        }
    }
    fn binary(self) -> bool {
        matches!(
            self,
            Self::Add
                | Self::Subtract
                | Self::Multiply
                | Self::Divide
                | Self::AddReal
                | Self::SubtractReal
                | Self::MultiplyReal
                | Self::DivideReal
        )
    }
    fn real(self) -> bool {
        matches!(
            self,
            Self::AddReal | Self::SubtractReal | Self::MultiplyReal | Self::DivideReal
        )
    }
}

/// Preserve the descriptor's interpretation after a source is snapshotted.
/// A K/H integer and a D word-pair with the same bits are not interchangeable,
/// especially on DEDIV's raw MIN32 and register-denormal paths. Literal is
/// retained for manually constructed ordinary FloatMathAction values; the
/// documented DE* parser does not admit F literals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FloatBinaryInput {
    Integer(i32),
    RawBinary32(u32),
    LiteralBinary32(u32),
}

impl FloatBinaryInput {
    fn capture(operand: &FloatOperand, memory: &Memory) -> Self {
        match operand {
            FloatOperand::Integer(value) => Self::Integer(*value),
            FloatOperand::Register(device) => Self::RawBinary32(memory.double_word(*device) as u32),
            FloatOperand::Literal { bits, .. } => Self::LiteralBinary32(*bits),
        }
    }

    fn raw(self) -> u32 {
        match self {
            Self::Integer(value) => value as u32,
            Self::RawBinary32(bits) | Self::LiteralBinary32(bits) => bits,
        }
    }

    fn converted(self) -> f32 {
        match self {
            Self::Integer(value) => value as f32,
            Self::RawBinary32(bits) | Self::LiteralBinary32(bits) => f32::from_bits(bits),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FloatBinaryError {
    pub code: i16,
    pub offset: u16,
}

/// Pure result normalization shared by ordinary and indexed arithmetic. Flags
/// are published only after semantic validation, before the destination write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FloatBinaryResult {
    bits: u32,
    flags: [bool; 3],
}

impl FloatBinaryResult {
    fn normalize(bits: u32) -> Self {
        if bits & 0x7fff_ffff == 0 {
            Self {
                bits: 0,
                flags: [true, false, false],
            }
        } else if bits & 0x7f80_0000 == 0x7f80_0000 {
            let negative = bits & 0x8000_0000 != 0;
            Self {
                bits: if negative { 0xff80_0000 } else { 0x7f80_0000 },
                flags: [false, negative, !negative],
            }
        } else {
            Self {
                bits,
                flags: [false; 3],
            }
        }
    }

    pub(crate) fn publish_flags(self, memory: &mut Memory) {
        for (flag, value) in [1020, 1021, 1022].into_iter().zip(self.flags) {
            memory.set_system(flag, value);
        }
    }

    pub(crate) fn write_value(self, memory: &mut Memory, target: Device) {
        memory.set_double_word(target, self.bits as i32);
    }
}

/// The four DE* binary operations only. Access/observation and pulse gates are
/// caller-owned; this seam has no scratch registers or Memory side effects.
pub(crate) fn evaluate_binary(
    op: MathOp,
    left: FloatBinaryInput,
    right: FloatBinaryInput,
) -> Result<FloatBinaryResult, FloatBinaryError> {
    let dividend = left.converted();
    let divisor = right.converted();
    let result = match op {
        MathOp::Add | MathOp::Subtract | MathOp::Multiply => {
            if let Some(bits) = x87_nan_result(dividend.to_bits(), divisor.to_bits()) {
                f32::from_bits(bits)
            } else {
                match op {
                    MathOp::Add => dividend + divisor,
                    MathOp::Subtract => dividend - divisor,
                    _ => dividend * divisor,
                }
            }
        }
        MathOp::Divide => {
            let raw = right.raw();
            if raw == 0 {
                return Err(FloatBinaryError {
                    code: 0xe19,
                    offset: 5,
                });
            }
            if divisor.is_nan() || dividend.is_nan() {
                return Err(FloatBinaryError {
                    code: 0xe1a,
                    offset: 5,
                });
            }
            if matches!(right, FloatBinaryInput::RawBinary32(_)) && raw & 0x7f80_0000 == 0 {
                return Err(FloatBinaryError {
                    code: 0xe19,
                    offset: 5,
                });
            }
            if raw == 0x8000_0000 {
                if left.raw() & 0x8000_0000 != 0 {
                    f32::INFINITY
                } else {
                    f32::NEG_INFINITY
                }
            } else {
                dividend / divisor
            }
        }
        _ => unreachable!("only DEADD/DESUB/DEMUL/DEDIV use binary evaluation"),
    };
    Ok(FloatBinaryResult::normalize(result.to_bits()))
}

/// DRAD/DDEG convert an integer descriptor to binary32 before multiplying;
/// D pairs already contain binary32. There is no R-style NaN rejection.
pub(crate) fn evaluate_angle(op: MathOp, source: FloatBinaryInput) -> FloatBinaryResult {
    let factor = match op {
        MathOp::Radians => f32::from_bits(0x3c8e_fa35),
        MathOp::Degrees => f32::from_bits(0x4265_2ee1),
        _ => unreachable!("only DRAD/DDEG use angle evaluation"),
    };
    FloatBinaryResult::normalize((source.converted() * factor).to_bits())
}

/// The coefficient/exponent converters have their own flags and optional
/// legacy diagnostics. They do not use the arithmetic result normalizer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DecimalConversionResult {
    bits: i32,
    flags: [bool; 3],
    diagnostics: Option<(i16, i16)>, // Pre-clamp exponent, coefficient.
}

impl DecimalConversionResult {
    pub(crate) fn begin(memory: &mut Memory) {
        clear_decimal_flags(memory);
    }

    pub(crate) fn validate(op: MathOp, raw: u32) -> Result<(), FloatBinaryError> {
        match op {
            MathOp::Decimal => Ok(()),
            MathOp::Binary => {
                let coefficient = raw as i16;
                let exponent = (raw >> 16) as i16;
                // Validation precedes zero handling, including 0e-41.
                if !(-9999..=9999).contains(&coefficient)
                    || exponent > 35
                    || (exponent == 35 && !(-3402..=3402).contains(&coefficient))
                    || exponent < -41
                    || (exponent == -41 && (-1175..=1175).contains(&coefficient))
                {
                    Err(FloatBinaryError {
                        code: 0xe1a,
                        offset: 1,
                    })
                } else {
                    Ok(())
                }
            }
            _ => unreachable!("only DEBCD/DEBIN use coefficient conversion"),
        }
    }

    pub(crate) fn evaluate(op: MathOp, raw: u32) -> Result<Self, FloatBinaryError> {
        Self::validate(op, raw)?;
        Ok(match op {
            MathOp::Decimal => Self::to_decimal(raw),
            MathOp::Binary => Self::to_binary(raw),
            _ => unreachable!("validated conversion operation"),
        })
    }

    fn to_decimal(raw: u32) -> Self {
        let negative = raw & 0x8000_0000 != 0;
        let magnitude = f32::from_bits(raw & 0x7fff_ffff);
        // FCOMP's unordered path also bypasses the legacy diagnostics.
        if magnitude == 0.0 || magnitude.is_nan() {
            return Self {
                bits: 0,
                flags: [true, false, false],
                diagnostics: None,
            };
        }
        let (mut coefficient, mut exponent) = if magnitude.is_infinite() {
            // Retain the observed CRT/FISTP low-word outcome, not Rust's
            // saturating float-to-int conversion.
            (0i16, -4i16)
        } else {
            let logarithm = f64::from(magnitude).log10() as f32;
            let e = logarithm.floor() as i16;
            let mut exponent = e - 3;
            let coefficient = if e <= -42 {
                let scaled = (f64::from(magnitude) / decimal_power(e)) as f32;
                decimal_truncate(f64::from(scaled * 1000.0))
            } else {
                match exponent {
                    -1 => decimal_truncate(f64::from(magnitude) * 10.0),
                    -2 => decimal_truncate(f64::from(magnitude) * 100.0),
                    -3 => {
                        let first = decimal_truncate(f64::from(magnitude) * 1000.0);
                        if first < 1000 {
                            exponent -= 1;
                            decimal_truncate(f64::from(magnitude) * 10000.0)
                        } else {
                            first
                        }
                    }
                    _ => {
                        let first = (f64::from(magnitude) / decimal_power(exponent)) as f32;
                        let first = decimal_truncate(f64::from(first));
                        if first < 1000 {
                            exponent -= 1;
                            let second = (f64::from(magnitude) / decimal_power(exponent)) as f32;
                            decimal_truncate(f64::from(second))
                        } else {
                            first
                        }
                    }
                }
            };
            (coefficient, exponent)
        };
        if negative {
            coefficient = coefficient.wrapping_neg();
        }
        let diagnostics = Some((exponent, coefficient));
        let mut flags = [false; 3];
        if exponent > 35 || (exponent == 35 && !(-3402..=3402).contains(&coefficient)) {
            exponent = 35;
            coefficient = if negative { -3402 } else { 3402 };
            flags[2] = true;
        } else if exponent < -41 || (exponent == -41 && (-1175..=1175).contains(&coefficient)) {
            exponent = -41;
            coefficient = if negative { -1176 } else { 1176 };
            flags[1] = true;
        }
        Self {
            bits: decimal_pair(coefficient, exponent),
            flags,
            diagnostics,
        }
    }

    fn to_binary(raw: u32) -> Self {
        let coefficient = raw as i16;
        let exponent = (raw >> 16) as i16;
        let magnitude = f64::from(coefficient.unsigned_abs());
        // Exponent -4 uses the native extended constant, with one binary32
        // rounding after multiplication; retain the existing f64 treatment.
        let scale = if exponent == -4 {
            0.0001
        } else {
            decimal_power(exponent)
        };
        let result = (magnitude * scale) as f32;
        let zero = result == 0.0;
        let bits = if zero {
            0
        } else {
            result.to_bits() | if coefficient < 0 { 0x8000_0000 } else { 0 }
        };
        Self {
            bits: bits as i32,
            flags: [zero, false, false],
            diagnostics: None,
        }
    }

    pub(crate) fn publish_before_target(self, memory: &mut Memory) {
        // WPL and current SS2 retain pre-clamp stores; current ES2 omits
        // them. Explicit final destinations are unaffected by this profile.
        if memory.decimal_scratch_profile == DecimalScratchProfile::LegacyVisible
            && let Some((exponent, coefficient)) = self.diagnostics
        {
            memory.set_word(
                Device {
                    area: Area::D,
                    index: 5000,
                },
                exponent,
            );
            memory.set_word(
                Device {
                    area: Area::D,
                    index: 5001,
                },
                coefficient,
            );
        }
        for (flag, value) in [1020, 1021, 1022].into_iter().zip(self.flags) {
            memory.set_system(flag, value);
        }
    }

    pub(crate) fn observed_effects(self, memory: &Memory, out: &mut BTreeSet<Device>) {
        if self.diagnostics.is_some()
            && memory.decimal_scratch_profile == DecimalScratchProfile::LegacyVisible
            && memory.data_register_limit.unwrap_or(9999) >= 5001
        {
            out.extend([
                Device {
                    area: Area::D,
                    index: 5000,
                },
                Device {
                    area: Area::D,
                    index: 5001,
                },
            ]);
        }
    }

    pub(crate) fn write_value(self, memory: &mut Memory, target: Device) {
        memory.set_double_word(target, self.bits);
    }
}

/// First-stage validation for R arithmetic. ADDR/SUBR/MULR must call this
/// before reading the right operand; DIVR deliberately reads both sources.
pub(crate) fn validate_real_left(op: MathOp, left: u32) -> Result<(), FloatBinaryError> {
    match op {
        MathOp::AddReal | MathOp::SubtractReal | MathOp::MultiplyReal => {
            if f32::from_bits(left).is_nan() {
                return Err(FloatBinaryError {
                    code: 0xe1a,
                    offset: 1,
                });
            }
        }
        MathOp::DivideReal => {}
        _ => unreachable!("only R arithmetic has staged real-source validation"),
    }
    Ok(())
}

/// Pure validation after the required sources have been read. Error offsets
/// are semantic PCs; DIVR's extra four-word NaN skip is a fresh execution
/// outcome, with the corresponding padding supplied by the encoder.
pub(crate) fn validate_real_pair(
    op: MathOp,
    left: u32,
    right: u32,
) -> Result<(), FloatBinaryError> {
    validate_real_left(op, left)?;
    if op == MathOp::DivideReal {
        if right == 0 || right & 0x7f80_0000 == 0 {
            return Err(FloatBinaryError {
                code: 0xe19,
                offset: 9,
            });
        }
        if f32::from_bits(left).is_nan() || f32::from_bits(right).is_nan() {
            return Err(FloatBinaryError {
                code: 0xe1a,
                offset: 9,
            });
        }
    } else if f32::from_bits(right).is_nan() {
        return Err(FloatBinaryError {
            code: 0xe1a,
            offset: 5,
        });
    }
    Ok(())
}

/// Both literals and register inputs are raw binary32 for R instructions;
/// there is no K/H conversion or DEDIV MIN32 exception in this operation set.
pub(crate) fn evaluate_real_binary(
    op: MathOp,
    left: u32,
    right: u32,
) -> Result<FloatBinaryResult, FloatBinaryError> {
    validate_real_pair(op, left, right)?;
    let left = f32::from_bits(left);
    let right = f32::from_bits(right);
    let result = match op {
        MathOp::AddReal => left + right,
        MathOp::SubtractReal => left - right,
        MathOp::MultiplyReal => left * right,
        MathOp::DivideReal => left / right,
        _ => unreachable!("only R arithmetic uses raw real evaluation"),
    };
    Ok(FloatBinaryResult::normalize(result.to_bits()))
}

#[derive(Debug, Clone, PartialEq)]
pub struct FloatMathAction {
    pub op: MathOp,
    pub source: FloatOperand,
    pub right: Option<FloatOperand>,
    pub dest: Device,
}
impl FloatMathAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let op = MathOp::parse(name).ok_or("Bilinmeyen kayan nokta işlemi")?;
        let arity = if op.binary() { 3 } else { 2 };
        if args.len() != arity {
            return Err(format!("{name}: {arity} operand gerekli"));
        }
        let source = if op.real() {
            real_operand(&args[0])?
        } else {
            FloatOperand::parse_comparison(&args[0])?
        };
        if matches!(op, MathOp::Decimal | MathOp::Binary)
            && !matches!(source, FloatOperand::Register(_))
        {
            return Err(format!("{name} kaynağı D yazmacı olmalı"));
        }
        let dest: Device = args[arity - 1].parse()?;
        if dest.area != Area::D {
            return Err(format!("{name} hedefi D yazmacı olmalı"));
        }
        double_devices(dest)?;
        Ok(Self {
            op,
            source,
            right: if op.binary() {
                Some(if op.real() {
                    real_operand(&args[1])?
                } else {
                    FloatOperand::parse_comparison(&args[1])?
                })
            } else {
                None
            },
            dest,
        })
    }
    pub fn label(&self) -> String {
        let name = self.op.name();
        match &self.right {
            Some(right) => format!("{name} {} {right} {}", self.source, self.dest),
            None => format!("{name} {} {}", self.source, self.dest),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        self.source.devices(out);
        if let Some(right) = &self.right {
            right.devices(out);
        }
        out.extend(double_devices(self.dest).expect("validated math destination"));
    }
    /// Returns the current DDIVR NaN branch's four-word native skip. This
    /// cannot be inferred from sticky operation-error registers after a visit.
    pub fn execute(&self, m: &mut Memory) -> bool {
        if matches!(
            self.op,
            MathOp::Add | MathOp::Subtract | MathOp::Multiply | MathOp::Divide
        ) {
            let left = FloatBinaryInput::capture(&self.source, m);
            let right = FloatBinaryInput::capture(self.right.as_ref().unwrap(), m);
            match evaluate_binary(self.op, left, right) {
                Ok(result) => {
                    result.publish_flags(m);
                    result.write_value(m, self.dest);
                }
                Err(error) => m.operation_error(error.code, error.offset),
            }
            return false;
        }
        if self.op.real() {
            let left = self.source.read_bits(m);
            if let Err(error) = validate_real_left(self.op, left) {
                m.operation_error(error.code, error.offset);
                return false;
            }
            let right = self.right.as_ref().unwrap().read_bits(m);
            match evaluate_real_binary(self.op, left, right) {
                Ok(result) => {
                    result.publish_flags(m);
                    result.write_value(m, self.dest);
                    return false;
                }
                Err(error) => {
                    m.operation_error(error.code, error.offset);
                    return self.op == MathOp::DivideReal
                        && error.code == 0xe1a
                        && error.offset == 9;
                }
            }
        }
        let result = match self.op {
            MathOp::Radians | MathOp::Degrees => {
                evaluate_angle(self.op, FloatBinaryInput::capture(&self.source, m))
            }
            MathOp::Add | MathOp::Subtract | MathOp::Multiply | MathOp::Divide => {
                unreachable!("handled above")
            }
            MathOp::AddReal | MathOp::SubtractReal | MathOp::MultiplyReal | MathOp::DivideReal => {
                unreachable!("handled above")
            }
            MathOp::Decimal => {
                self.to_decimal(m);
                return false;
            }
            MathOp::Binary => {
                self.to_binary(m);
                return false;
            }
        };
        result.publish_flags(m);
        result.write_value(m, self.dest);
        false
    }
    fn to_decimal(&self, m: &mut Memory) {
        self.convert_decimal(m);
    }
    fn to_binary(&self, m: &mut Memory) {
        self.convert_decimal(m);
    }
    fn convert_decimal(&self, m: &mut Memory) {
        DecimalConversionResult::begin(m);
        let raw = self.source.read_bits(m);
        match DecimalConversionResult::evaluate(self.op, raw) {
            Ok(result) => {
                result.publish_before_target(m);
                result.write_value(m, self.dest);
            }
            Err(error) => m.operation_error(error.code, error.offset),
        }
    }
    pub fn display_value(&self, m: &Memory) -> String {
        if self.op == MathOp::Decimal {
            let pair = double_devices(self.dest).unwrap();
            format!("{} × 10^{}", m.word(pair[0]), m.word(pair[1]))
        } else {
            f32::from_bits(m.double_word(self.dest) as u32).to_string()
        }
    }
}

fn real_operand(raw: &str) -> Result<FloatOperand, String> {
    let value = FloatOperand::parse(raw)?;
    match value {
        FloatOperand::Literal { .. } => Ok(value),
        FloatOperand::Register(d) if d.area == Area::D => Ok(value),
        _ => Err("R kayan nokta kaynağı F sabiti veya D yazmacı olmalı".into()),
    }
}

fn clear_decimal_flags(m: &mut Memory) {
    for flag in 1020..=1022 {
        m.set_system(flag, false);
    }
}
fn decimal_pair(coefficient: i16, exponent: i16) -> i32 {
    i32::from(coefficient as u16) | (i32::from(exponent) << 16)
}
fn decimal_truncate(value: f64) -> i16 {
    // FISTP qword returns integer-indefinite for nonfinite/out-of-i64 input;
    // its low16bits are0, unlike Rust's saturating float-to-int cast.
    if !value.is_finite()
        || !(-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&value)
    {
        0
    } else {
        value.trunc() as i64 as i16
    }
}
fn decimal_power(exponent: i16) -> f64 {
    // Caller-side binary32 stores and truncation points match the native
    // algorithm. The CRT pow's last-bit precision needs differential probes.
    10.0f64.powf(f64::from(exponent))
}

/// FLD quiets a signaling left source before the memory-operand operation.
/// Preserve the x87 NaN choice because its sign becomes a visible flag/result.
fn x87_nan_result(left: u32, right: u32) -> Option<u32> {
    let is_nan = |bits: u32| bits & 0x7fff_ffff > 0x7f80_0000;
    match (is_nan(left), is_nan(right)) {
        (false, false) => None,
        (true, false) => Some(left | 0x0040_0000),
        (false, true) => Some(right | 0x0040_0000),
        (true, true) => {
            let left = left | 0x0040_0000;
            let right_quiet = right & 0x0040_0000 != 0;
            let selected = if !right_quiet || left & 0x7fff_ffff > right & 0x7fff_ffff {
                left
            } else if left & 0x7fff_ffff == right & 0x7fff_ffff {
                left & right // Equal significands prefer the positive sign.
            } else {
                right
            };
            Some(selected | 0x0040_0000)
        }
    }
}

#[cfg(test)]
mod arithmetic_tests {
    use super::*;
    fn d(index: u16) -> Device {
        Device {
            area: Area::D,
            index,
        }
    }
    fn action(op: &str, args: &[&str]) -> FloatMathAction {
        FloatMathAction::parse(
            op,
            &args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(),
        )
        .unwrap()
    }
    #[test]
    fn angle_factors_and_numeric_constants_have_native_binary32_rounding() {
        let mut m = Memory::default();
        action("DRAD", &["K180", "D0"]).execute(&mut m);
        assert_eq!(m.double_word(d(0)) as u32, 0x4049_0fdb);
        action("DDEG", &["D0", "D2"]).execute(&mut m);
        assert_eq!(m.double_word(d(2)) as u32, 180.0f32.to_bits());
        action("DEADD", &["K16777217", "K1", "D4"]).execute(&mut m);
        assert_eq!(m.double_word(d(4)) as u32, 16777216.0f32.to_bits());
        m.set_double_word(d(0), 0x8000_0000u32 as i32);
        action("DRAD", &["D0", "D0"]).execute(&mut m);
        assert_eq!(m.double_word(d(0)), 0);
        assert!(m.system_bit(1020));
    }
    #[test]
    fn division_error_priority_holds_destination_and_all_arithmetic_flags() {
        let mut m = Memory::default();
        for (left, right, code) in [
            (0x7fc0_1234u32, 0u32, 0xe19),
            (0x7fc0_1234, 0x8000_0000, 0xe1a),
            (0x7fc0_1234, 1, 0xe1a),
            (0x3f80_0000, 1, 0xe19),
        ] {
            m.set_internal(Memory::system_device(1067), false);
            m.set_internal(Memory::system_device(1068), false);
            for flag in 1020..=1022 {
                m.set_system(flag, true);
            }
            m.set_double_word(d(0), left as i32);
            m.set_double_word(d(2), right as i32);
            m.set_double_word(d(10), 0x1234_5678);
            action("DEDIV", &["D0", "D2", "D10"]).execute(&mut m);
            assert_eq!(m.double_word(d(10)), 0x1234_5678);
            assert_eq!(m.word(d(1067)), code);
            assert_eq!(m.word(d(1068)), 5);
            assert!((1020..=1022).all(|flag| m.system_bit(flag)));
        }
    }
    #[test]
    fn unordered_payload_selection_matches_observed_x87_sign_and_flags() {
        let mut m = Memory::default();
        for (left, right, negative) in [
            (0x7fc0_0001u32, 0xffc0_0002u32, true),
            (0xffc0_0001, 0x7fc0_0002, false),
            (0xffc0_0001, 0x7fc0_0001, false),
            (0xff80_0002, 0x7fc0_0001, true),
            (0x7fc0_0001, 0xff80_0002, false),
            (0xffc0_0001, 0x7f80_0002, true),
            (0x7f80_0001, 0xff80_0002, false),
        ] {
            for op in ["DEADD", "DESUB", "DEMUL"] {
                m.set_double_word(d(0), left as i32);
                m.set_double_word(d(2), right as i32);
                action(op, &["D0", "D2", "D10"]).execute(&mut m);
                assert_eq!(
                    m.double_word(d(10)) as u32,
                    if negative { 0xff80_0000 } else { 0x7f80_0000 }
                );
                assert_eq!(m.system_bit(1021), negative);
                assert_eq!(m.system_bit(1022), !negative);
                assert!(!m.system_bit(1020));
            }
        }
    }
    #[test]
    fn operand_matrices_reject_non_data_devices_and_float_literals() {
        for (op, args) in [
            ("DRAD", vec!["F180", "D0"]),
            ("DDEG", vec!["T0", "D0"]),
            ("DEBCD", vec!["K1", "D0"]),
            ("DEBIN", vec!["D0", "T0"]),
            ("DEADD", vec!["D0", "D2", "D9999"]),
            ("DESUB", vec!["C200", "K1", "D0"]),
        ] {
            assert!(
                FloatMathAction::parse(
                    op,
                    &args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>()
                )
                .is_err()
            );
        }
    }
}

#[cfg(test)]
mod decimal_tests {
    use super::*;
    fn d(index: u16) -> Device {
        Device {
            area: Area::D,
            index,
        }
    }
    fn conversion(op: MathOp, source: u16, dest: u16) -> FloatMathAction {
        FloatMathAction {
            op,
            source: FloatOperand::Register(d(source)),
            right: None,
            dest: d(dest),
        }
    }
    fn seed_flags(m: &mut Memory) {
        for flag in 1020..=1022 {
            m.set_system(flag, true);
        }
    }
    #[test]
    fn binary_decimal_formats_match_documented_examples_and_normalization() {
        let mut m = Memory::default();
        m.decimal_scratch_profile = DecimalScratchProfile::LegacyVisible;
        m.set_word(d(0), 314);
        m.set_word(d(1), -2);
        seed_flags(&mut m);
        conversion(MathOp::Binary, 0, 2).execute(&mut m);
        assert_eq!(m.double_word(d(2)) as u32, 0x4048_f5c3);
        assert!(!(1020..=1022).any(|flag| m.system_bit(flag)));
        conversion(MathOp::Decimal, 2, 4).execute(&mut m);
        assert_eq!((m.word(d(4)), m.word(d(5))), (3140, -3));
        assert_eq!((m.word(d(5000)), m.word(d(5001))), (-3, 3140));
        for (value, coefficient, exponent) in [
            (1.0f32, 1000, -3),
            (10.0, 1000, -2),
            (100.0, 1000, -1),
            (1000.0, 1000, 0),
            (-23.0, -2300, -2),
        ] {
            m.set_double_word(d(0), value.to_bits() as i32);
            conversion(MathOp::Decimal, 0, 2).execute(&mut m);
            assert_eq!((m.word(d(2)), m.word(d(3))), (coefficient, exponent));
        }
    }
    #[test]
    fn decimal_underflow_clamps_after_observable_unclamped_diagnostics() {
        let mut m = Memory::default();
        m.decimal_scratch_profile = DecimalScratchProfile::LegacyVisible;
        m.set_double_word(d(0), f32::MIN_POSITIVE.to_bits() as i32);
        conversion(MathOp::Decimal, 0, 2).execute(&mut m);
        assert_eq!((m.word(d(2)), m.word(d(3))), (1176, -41));
        assert_eq!((m.word(d(5000)), m.word(d(5001))), (-41, 1175));
        assert!(m.system_bit(1021));
        assert!(!m.system_bit(1020) && !m.system_bit(1022));
        m.set_double_word(d(0), 1);
        conversion(MathOp::Decimal, 0, 2).execute(&mut m);
        assert_eq!((m.word(d(2)), m.word(d(3))), (1176, -41));
        assert_eq!((m.word(d(5000)), m.word(d(5001))), (-48, 1401));
    }
    #[test]
    fn binary_conversion_validates_before_zero_and_preserves_destination_on_error() {
        for (coefficient, exponent) in [
            (0, -41),
            (1175, -41),
            (-1175, -41),
            (3403, 35),
            (-3403, 35),
            (10000, 0),
            (1, -42),
            (1, 36),
        ] {
            let mut m = Memory::default();
            m.set_word(d(0), coefficient);
            m.set_word(d(1), exponent);
            m.set_double_word(d(10), 0x1234_5678);
            seed_flags(&mut m);
            conversion(MathOp::Binary, 0, 10).execute(&mut m);
            assert_eq!(m.double_word(d(10)), 0x1234_5678);
            assert_eq!(m.word(d(1067)), 0xe1a);
            assert_eq!(m.word(d(1068)), 1);
            assert!(!(1020..=1022).any(|flag| m.system_bit(flag)));
        }
        for (coefficient, exponent) in [
            (1176, -41),
            (-1176, -41),
            (3402, 35),
            (-3402, 35),
            (9999, 0),
            (1, -40),
            (0, -40),
            (0, 35),
        ] {
            let mut m = Memory::default();
            m.set_word(d(0), coefficient);
            m.set_word(d(1), exponent);
            conversion(MathOp::Binary, 0, 10).execute(&mut m);
            assert!(!m.system_bit(1067), "{coefficient}e{exponent}");
            assert_eq!(m.system_bit(1020), coefficient == 0);
            assert!(!m.system_bit(1021) && !m.system_bit(1022));
        }
    }
    #[test]
    fn decimal_zero_and_unordered_paths_hold_diagnostics_while_inf_keeps_legacy_pair() {
        for raw in [0u32, 0x8000_0000, 0x7fc0_1234, 0xff80_0001] {
            let mut m = Memory::default();
            m.decimal_scratch_profile = DecimalScratchProfile::LegacyVisible;
            m.set_word(d(5000), 123);
            m.set_word(d(5001), 456);
            m.set_double_word(d(0), raw as i32);
            seed_flags(&mut m);
            conversion(MathOp::Decimal, 0, 2).execute(&mut m);
            assert_eq!(m.double_word(d(2)), 0);
            assert!(m.system_bit(1020));
            assert!(!m.system_bit(1021) && !m.system_bit(1022));
            assert_eq!((m.word(d(5000)), m.word(d(5001))), (123, 456));
        }
        for raw in [0x7f80_0000u32, 0xff80_0000] {
            let mut m = Memory::default();
            m.decimal_scratch_profile = DecimalScratchProfile::LegacyVisible;
            m.set_double_word(d(0), raw as i32);
            conversion(MathOp::Decimal, 0, 2).execute(&mut m);
            assert_eq!((m.word(d(2)), m.word(d(3))), (0, -4));
            assert!(!(1020..=1022).any(|flag| m.system_bit(flag)));
        }
    }
    #[test]
    fn decimal_source_and_diagnostic_aliases_follow_native_write_order() {
        let mut m = Memory::default();
        m.decimal_scratch_profile = DecimalScratchProfile::LegacyVisible;
        m.set_word(d(0), 314);
        m.set_word(d(1), -2);
        conversion(MathOp::Binary, 0, 1).execute(&mut m);
        assert_eq!(m.double_word(d(1)) as u32, 0x4048_f5c3);
        conversion(MathOp::Decimal, 1, 5000).execute(&mut m);
        assert_eq!((m.word(d(5000)), m.word(d(5001))), (3140, -3));
        m.set_double_word(d(0), (-23.0f32).to_bits() as i32);
        conversion(MathOp::Decimal, 0, 0).execute(&mut m);
        assert_eq!((m.word(d(0)), m.word(d(1))), (-2300, -2));
    }

    #[test]
    fn current_private_decimal_scratch_holds_sentinels_across_result_classes() {
        assert_eq!(
            DecimalScratchProfile::default(),
            DecimalScratchProfile::CurrentPrivate
        );
        for (raw, pair, flags) in [
            (0x3f80_0000u32, (1000, -3), [false, false, false]),
            (0x4048_f5c3, (3140, -3), [false, false, false]),
            (0xc1b8_0000, (-2300, -2), [false, false, false]),
            (0, (0, 0), [true, false, false]),
            (0x8000_0000, (0, 0), [true, false, false]),
            (0x7fc0_1234, (0, 0), [true, false, false]),
            (0xff80_0001, (0, 0), [true, false, false]),
            (0x7f80_0000, (0, -4), [false, false, false]),
            (0x0080_0000, (1176, -41), [false, true, false]),
            (0x8080_0000, (-1176, -41), [false, true, false]),
            (1, (1176, -41), [false, true, false]),
            (0x7f7f_ffff, (3402, 35), [false, false, false]),
            (0xff7f_ffff, (-3402, 35), [false, false, false]),
        ] {
            let mut m = Memory::default();
            m.set_word(d(5000), 123);
            m.set_word(d(5001), 456);
            m.set_double_word(d(0), raw as i32);
            seed_flags(&mut m);
            conversion(MathOp::Decimal, 0, 20).execute(&mut m);
            assert_eq!((m.word(d(20)), m.word(d(21))), pair, "{raw:08x}");
            assert_eq!(
                [1020, 1021, 1022].map(|flag| m.system_bit(flag)),
                flags,
                "{raw:08x}"
            );
            assert_eq!((m.word(d(5000)), m.word(d(5001))), (123, 456), "{raw:08x}");
            assert_eq!(m.double_word(d(0)) as u32, raw);
        }
    }

    #[test]
    fn decimal_profiles_keep_arithmetic_and_flags_identical_with_explicit_legacy_diagnostics() {
        for (raw, pair, flags, diagnostics) in [
            (
                0x4048_f5c3u32,
                (3140, -3),
                [false, false, false],
                (-3, 3140),
            ),
            (0xc1b8_0000, (-2300, -2), [false, false, false], (-2, -2300)),
            (0x0080_0000, (1176, -41), [false, true, false], (-41, 1175)),
            (
                0x8080_0000,
                (-1176, -41),
                [false, true, false],
                (-41, -1175),
            ),
            (1, (1176, -41), [false, true, false], (-48, 1401)),
            (0x7f80_0000, (0, -4), [false, false, false], (-4, 0)),
            (0x7f7f_ffff, (3402, 35), [false, false, false], (35, 3402)),
            (0, (0, 0), [true, false, false], (123, 456)),
            (0x7fc0_1234, (0, 0), [true, false, false], (123, 456)),
        ] {
            let mut current = Memory::default();
            current.set_word(d(5000), 123);
            current.set_word(d(5001), 456);
            current.set_double_word(d(0), raw as i32);
            seed_flags(&mut current);
            let mut legacy = current.clone();
            legacy.decimal_scratch_profile = DecimalScratchProfile::LegacyVisible;
            conversion(MathOp::Decimal, 0, 20).execute(&mut current);
            conversion(MathOp::Decimal, 0, 20).execute(&mut legacy);
            assert_eq!(
                (current.word(d(20)), current.word(d(21))),
                pair,
                "{raw:08x}"
            );
            assert_eq!((legacy.word(d(20)), legacy.word(d(21))), pair, "{raw:08x}");
            assert_eq!(
                [1020, 1021, 1022].map(|flag| current.system_bit(flag)),
                flags
            );
            assert_eq!(
                [1020, 1021, 1022].map(|flag| legacy.system_bit(flag)),
                flags
            );
            assert_eq!((current.word(d(5000)), current.word(d(5001))), (123, 456));
            assert_eq!(
                (legacy.word(d(5000)), legacy.word(d(5001))),
                diagnostics,
                "{raw:08x}"
            );
        }
    }

    #[test]
    fn private_scratch_does_not_hide_explicit_result_writes_into_d5000_or_d5001() {
        for (profile, target, expected) in [
            (
                DecimalScratchProfile::CurrentPrivate,
                4999,
                [-2300, -2, 456, 789],
            ),
            (
                DecimalScratchProfile::CurrentPrivate,
                5000,
                [111, -2300, -2, 789],
            ),
            (
                DecimalScratchProfile::CurrentPrivate,
                5001,
                [111, 123, -2300, -2],
            ),
            (
                DecimalScratchProfile::LegacyVisible,
                4999,
                [-2300, -2, -2300, 789],
            ),
            (
                DecimalScratchProfile::LegacyVisible,
                5000,
                [111, -2300, -2, 789],
            ),
            (
                DecimalScratchProfile::LegacyVisible,
                5001,
                [111, -2, -2300, -2],
            ),
        ] {
            let mut m = Memory::default();
            m.decimal_scratch_profile = profile;
            m.set_double_word(d(0), 0xc1b8_0000u32 as i32);
            for (index, value) in [
                (4998, 777),
                (4999, 111),
                (5000, 123),
                (5001, 456),
                (5002, 789),
                (5003, 888),
            ] {
                m.set_word(d(index), value);
            }
            conversion(MathOp::Decimal, 0, target).execute(&mut m);
            assert_eq!(
                [4999, 5000, 5001, 5002].map(|index| m.word(d(index))),
                expected,
                "{profile:?} D{target}"
            );
            assert_eq!((m.word(d(4998)), m.word(d(5003))), (777, 888));
            assert!(!(1020..=1022).any(|flag| m.system_bit(flag)));
        }
    }

    #[test]
    fn decimal_source_is_snapshotted_before_legacy_scratch_and_explicit_alias_writes() {
        for profile in [
            DecimalScratchProfile::CurrentPrivate,
            DecimalScratchProfile::LegacyVisible,
        ] {
            for target in [20, 5000] {
                let mut m = Memory::default();
                m.decimal_scratch_profile = profile;
                m.set_double_word(d(5000), 0xc1b8_0000u32 as i32);
                m.set_word(d(5002), 77);
                conversion(MathOp::Decimal, 5000, target).execute(&mut m);
                assert_eq!((m.word(d(target)), m.word(d(target + 1))), (-2300, -2));
                assert_eq!(m.word(d(5002)), 77);
                if target == 20 {
                    let expected = match profile {
                        DecimalScratchProfile::CurrentPrivate => (0, 0xc1b8u16 as i16),
                        DecimalScratchProfile::LegacyVisible => (-2, -2300),
                    };
                    assert_eq!((m.word(d(5000)), m.word(d(5001))), expected);
                }
            }
        }
    }
}

#[cfg(test)]
mod real_arithmetic_tests {
    use super::*;
    fn d(index: u16) -> Device {
        Device {
            area: Area::D,
            index,
        }
    }
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }
    fn action(name: &str, values: &[&str]) -> FloatMathAction {
        FloatMathAction::parse(name, &args(values)).unwrap()
    }
    fn raw(m: &mut Memory, index: u16, bits: u32) {
        m.set_double_word(d(index), bits as i32);
    }
    fn seeded_flags(m: &mut Memory) {
        m.set_system(1020, true);
        m.set_system(1021, false);
        m.set_system(1022, true);
    }
    fn check_error(m: &Memory, code: i16, pc: i16) {
        assert_eq!(m.word(d(1067)), code);
        assert_eq!(m.word(d(1068)), pc);
        assert!(m.system_bit(1067) && m.system_bit(1068));
        assert!(m.system_bit(1020) && !m.system_bit(1021) && m.system_bit(1022));
        assert_eq!(m.double_word(d(10)), 0x1234_5678);
    }
    #[test]
    fn literal_examples_keep_scientific_source_and_native_headers() {
        let mut m = Memory::default();
        let add = action("DADDR", &["F1.200E+0", "F2.200E+0", "D10"]);
        assert_eq!(add.label(), "DADDR F1.200E+0 F2.200E+0 D10");
        add.execute(&mut m);
        assert_eq!(m.double_word(d(10)) as u32, 0x4059_999a);
        action("DSUBR", &["F1.2", "F2.2", "D10"]).execute(&mut m);
        assert_eq!(m.double_word(d(10)) as u32, 0xbf80_0000);
        action("DMULR", &["F1.2", "F2.2", "D10"]).execute(&mut m);
        assert_eq!(m.double_word(d(10)) as u32, 0x4028_f5c3);
        action("DDIVR", &["F1.5", "F0.5", "D10"]).execute(&mut m);
        assert_eq!(m.double_word(d(10)) as u32, 0x4040_0000);
        for (name, header) in [
            ("DADDR", 0x7701),
            ("DSUBR", 0x7901),
            ("DMULR", 0x7b01),
            ("DDIVR", 0x7d01),
        ] {
            assert_eq!(MathOp::parse(name).unwrap().header(), header);
        }
    }
    #[test]
    fn r_operand_matrices_keep_numeric_constant_families_distinct() {
        for name in ["DADDR", "DSUBR", "DMULR", "DDIVR"] {
            for values in [
                ["K1", "F2", "D10"],
                ["F1", "H2", "D10"],
                ["T0", "D0", "D10"],
                ["D0", "C200", "D10"],
                ["F1", "F2", "T0"],
                ["F1", "F2", "D9999"],
            ] {
                assert!(
                    FloatMathAction::parse(name, &args(&values)).is_err(),
                    "{name} {values:?}"
                );
            }
            assert!(FloatMathAction::parse(name, &args(&["F1", "D0", "D10"])).is_ok());
            assert!(FloatMathAction::parse(name, &args(&["D0", "F1", "D10"])).is_ok());
        }
        assert!(FloatMathAction::parse("DEADD", &args(&["F1", "F2", "D10"])).is_err());
        assert!(FloatMathAction::parse("DEADD", &args(&["K1", "K2", "D10"])).is_ok());
    }
    #[test]
    fn separate_nan_validation_preserves_flags_and_first_error_pc() {
        for name in ["DADDR", "DSUBR", "DMULR"] {
            for (left, right, pc) in [
                (0x7fc0_1234, 0x3f80_0000, 1),
                (0x3f80_0000, 0xff80_0001, 5),
                (0x7fc0_1234, 0xffc0_5678, 1),
            ] {
                let mut m = Memory::default();
                raw(&mut m, 0, left);
                raw(&mut m, 2, right);
                raw(&mut m, 10, 0x1234_5678);
                seeded_flags(&mut m);
                action(name, &["D0", "D2", "D10"]).execute(&mut m);
                check_error(&m, 0xe1a, pc);
            }
        }
    }
    #[test]
    fn division_validates_destination_field_and_rejects_signed_zeros_and_subnormals() {
        for divisor in [0u32, 0x8000_0000, 1, 0x007f_ffff, 0x8000_0001, 0x807f_ffff] {
            let mut m = Memory::default();
            raw(&mut m, 0, 1.0f32.to_bits());
            raw(&mut m, 2, divisor);
            raw(&mut m, 10, 0x1234_5678);
            seeded_flags(&mut m);
            action("DDIVR", &["D0", "D2", "D10"]).execute(&mut m);
            check_error(&m, 0xe19, 9);
        }
        for (left, right, code) in [
            (0x7fc0_1234, 0u32, 0xe19),
            (0x7fc0_1234, 0x3f80_0000, 0xe1a),
            (0x3f80_0000, 0xffc0_1234, 0xe1a),
        ] {
            let mut m = Memory::default();
            raw(&mut m, 0, left);
            raw(&mut m, 2, right);
            raw(&mut m, 10, 0x1234_5678);
            seeded_flags(&mut m);
            action("DDIVR", &["D0", "D2", "D10"]).execute(&mut m);
            check_error(&m, code, 9);
        }
    }
    #[test]
    fn negative_zero_literal_and_overlapping_destination_follow_float_semantics() {
        let mut m = Memory::default();
        let multiply = action("DMULR", &["F-0.0", "F2.0", "D10"]);
        assert_eq!(multiply.source.read_bits(&m), 0x8000_0000);
        assert_eq!(multiply.label(), "DMULR F-0.0 F2.0 D10");
        multiply.execute(&mut m);
        assert_eq!(m.double_word(d(10)), 0);
        assert!(m.system_bit(1020));
        raw(&mut m, 0, 2.0f32.to_bits());
        raw(&mut m, 2, 4.0f32.to_bits());
        action("DDIVR", &["D0", "D2", "D1"]).execute(&mut m);
        assert_eq!(m.double_word(d(1)) as u32, 0x3f00_0000);
        assert!(!(1020..=1022).any(|flag| m.system_bit(flag)));
    }
}
