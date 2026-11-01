//! Advanced binary32 functions.
//! x86 kernels preserve the caller's x87 control word and leave its stack balanced.
use crate::float_math::FloatBinaryError;
use crate::float_ops::FloatOperand;
use crate::plc::{Area, Device, Memory};
use crate::wide::double_devices;
use std::collections::BTreeSet;

/// Native Delphi math uses extended precision with round-to-nearest. Scope
/// those two fields to one action; exception masks and all other bits survive.
/// Drop restores the exact caller word, including on validation-error returns.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
struct X87Control {
    original: u16,
}
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
impl X87Control {
    fn read() -> u16 {
        let mut word = 0u16;
        unsafe {
            std::arch::asm!("fnstcw word ptr [{word}]", word = in(reg) &mut word,
                options(nostack, preserves_flags));
        }
        word
    }
    fn write(word: u16) {
        unsafe {
            std::arch::asm!("fldcw word ptr [{word}]", word = in(reg) &word,
                options(nostack, preserves_flags));
        }
    }
    fn enter() -> Self {
        let original = Self::read();
        // PC bits8..9=11 (64-bit significand), RC bits10..11=00 (nearest).
        Self::write((original & !0x0f00) | 0x0300);
        Self { original }
    }
}
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
impl Drop for X87Control {
    fn drop(&mut self) {
        Self::write(self.original);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdvancedOp {
    Exp,
    Ln,
    Log,
    Sqrt,
    Pow,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
}
impl AdvancedOp {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "DEXP" => Self::Exp,
            "DLN" => Self::Ln,
            "DLOG" => Self::Log,
            "DESQR" => Self::Sqrt,
            "DPOW" => Self::Pow,
            "DSIN" => Self::Sin,
            "DCOS" => Self::Cos,
            "DTAN" => Self::Tan,
            "DASIN" => Self::Asin,
            "DACOS" => Self::Acos,
            "DATAN" => Self::Atan,
            _ => return None,
        })
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Exp => "DEXP",
            Self::Ln => "DLN",
            Self::Log => "DLOG",
            Self::Sqrt => "DESQR",
            Self::Pow => "DPOW",
            Self::Sin => "DSIN",
            Self::Cos => "DCOS",
            Self::Tan => "DTAN",
            Self::Asin => "DASIN",
            Self::Acos => "DACOS",
            Self::Atan => "DATAN",
        }
    }
    pub fn header(self) -> u16 {
        match self {
            Self::Exp => 0x1901,
            Self::Ln => 0x1b01,
            Self::Log => 0x1d01,
            Self::Sqrt => 0x1f01,
            Self::Pow => 0x2101,
            Self::Sin => 0x2501,
            Self::Cos => 0x2701,
            Self::Tan => 0x2901,
            Self::Asin => 0x2b01,
            Self::Acos => 0x2d01,
            Self::Atan => 0x2f01,
        }
    }
    fn binary(self) -> bool {
        matches!(self, Self::Log | Self::Pow)
    }
}

/// Preserve K/H integer conversion before the unary kernel. Literal raw bits
/// remain available to manually constructed ordinary AdvancedAction values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExpLnInput {
    Integer(i32),
    RawBinary32(u32),
}
impl ExpLnInput {
    fn capture(source: &FloatOperand, memory: &Memory) -> Self {
        match source {
            FloatOperand::Integer(value) => Self::Integer(*value),
            _ => Self::RawBinary32(source.read_bits(memory)),
        }
    }
    fn converted(self) -> f32 {
        match self {
            Self::Integer(value) => value as f32,
            Self::RawBinary32(raw) => f32::from_bits(raw),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AdvancedResult {
    bits: u32,
    flags: [bool; 3],
}
impl AdvancedResult {
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

fn validate_exp_ln_value(op: AdvancedOp, value: f32) -> Result<(), FloatBinaryError> {
    let code = match op {
        AdvancedOp::Exp if value.is_nan() => Some(0xe1a),
        AdvancedOp::Ln if value.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) => {
            Some(0xe19)
        }
        AdvancedOp::Exp | AdvancedOp::Ln => None,
        _ => unreachable!("only DEXP/DLN use this source contract"),
    };
    match code {
        Some(code) => Err(FloatBinaryError { code, offset: 1 }),
        None => Ok(()),
    }
}

/// Pure validation for staged native preflight, with no kernel evaluation.
pub(crate) fn validate_exp_ln(op: AdvancedOp, source: ExpLnInput) -> Result<(), FloatBinaryError> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    let _control = X87Control::enter();
    validate_exp_ln_value(op, source.converted())
}

/// Keep the existing kernels, FSTP boundaries and scoped caller-CW restoration.
/// Semantic errors do not normalize flags or visit a result destination.
pub(crate) fn evaluate_exp_ln(
    op: AdvancedOp,
    source: ExpLnInput,
) -> Result<AdvancedResult, FloatBinaryError> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    let _control = X87Control::enter();
    let value = source.converted();
    validate_exp_ln_value(op, value)?;
    Ok(AdvancedResult::normalize(kernel(op, value).to_bits()))
}

/// Binary advanced instructions use the same signed-integer/raw-pair
/// interpretation as Exp/Ln, while retaining their own staged domains.
pub(crate) type LogPowInput = ExpLnInput;
pub(crate) type DesqrInput = ExpLnInput;
pub(crate) type TrigInput = ExpLnInput;

pub(crate) fn trig_uses_degrees(op: AdvancedOp) -> bool {
    matches!(op, AdvancedOp::Sin | AdvancedOp::Cos | AdvancedOp::Tan)
}

fn validate_trig_value(op: AdvancedOp, value: f32) -> Result<(), FloatBinaryError> {
    let code = match op {
        AdvancedOp::Asin | AdvancedOp::Acos | AdvancedOp::Atan if value.is_nan() => Some(0xe1a),
        AdvancedOp::Asin | AdvancedOp::Acos if !(-1.0..=1.0).contains(&value) => Some(0xe19),
        AdvancedOp::Sin
        | AdvancedOp::Cos
        | AdvancedOp::Tan
        | AdvancedOp::Asin
        | AdvancedOp::Acos
        | AdvancedOp::Atan => None,
        _ => unreachable!("only trig operations use trig validation"),
    };
    match code {
        Some(code) => Err(FloatBinaryError { code, offset: 1 }),
        None => Ok(()),
    }
}
pub(crate) fn validate_trig(op: AdvancedOp, source: TrigInput) -> Result<(), FloatBinaryError> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    let _control = X87Control::enter();
    validate_trig_value(op, source.converted())
}
/// Forward instructions alone consult the sampled degree mode. Retain the
/// two-rounding degree conversion, existing x87 kernels and RTL cutoff model.
pub(crate) fn evaluate_trig(
    op: AdvancedOp,
    source: TrigInput,
    degrees: bool,
) -> Result<AdvancedResult, FloatBinaryError> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    let _control = X87Control::enter();
    let value = source.converted();
    validate_trig_value(op, value)?;
    let result = if trig_uses_degrees(op) {
        let angle = if degrees { degree_angle(value) } else { value };
        if !angle.is_finite() || angle.abs() >= 9_007_199_254_740_992.0 {
            f32::INFINITY
        } else {
            kernel(op, angle)
        }
    } else {
        kernel(op, value)
    };
    Ok(AdvancedResult::normalize(result.to_bits()))
}

/// Floating square root preserves borrow/carry on its dedicated zero path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DesqrResult {
    Zero,
    Normalized(AdvancedResult),
}
impl DesqrResult {
    pub(crate) fn publish_flags(self, memory: &mut Memory) {
        match self {
            Self::Zero => memory.set_system(1020, true),
            Self::Normalized(result) => result.publish_flags(memory),
        }
    }
    pub(crate) fn write_value(self, memory: &mut Memory, target: Device) {
        match self {
            Self::Zero => memory.set_double_word(target, 0),
            Self::Normalized(result) => result.write_value(memory, target),
        }
    }
}

fn validate_desqr_value(value: f32) -> Result<(), FloatBinaryError> {
    if value.is_nan() || value < 0.0 {
        Err(FloatBinaryError {
            code: 0xe1b,
            offset: 1,
        })
    } else {
        Ok(())
    }
}
pub(crate) fn validate_desqr(source: DesqrInput) -> Result<(), FloatBinaryError> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    let _control = X87Control::enter();
    validate_desqr_value(source.converted())
}
pub(crate) fn evaluate_desqr(source: DesqrInput) -> Result<DesqrResult, FloatBinaryError> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    let _control = X87Control::enter();
    let value = source.converted();
    validate_desqr_value(value)?;
    if value == 0.0 {
        Ok(DesqrResult::Zero)
    } else {
        Ok(DesqrResult::Normalized(AdvancedResult::normalize(
            kernel(AdvancedOp::Sqrt, value).to_bits(),
        )))
    }
}

fn validate_log_pow_left_value(op: AdvancedOp, value: f32) -> Result<(), FloatBinaryError> {
    assert!(matches!(op, AdvancedOp::Log | AdvancedOp::Pow));
    if value.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)
        || (op == AdvancedOp::Log && value == 1.0)
    {
        Err(FloatBinaryError {
            code: 0xe19,
            offset: 1,
        })
    } else {
        Ok(())
    }
}

pub(crate) fn validate_log_pow_left(
    op: AdvancedOp,
    source: LogPowInput,
) -> Result<(), FloatBinaryError> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    let _control = X87Control::enter();
    validate_log_pow_left_value(op, source.converted())
}

fn validate_log_pow_values(op: AdvancedOp, left: f32, right: f32) -> Result<(), FloatBinaryError> {
    validate_log_pow_left_value(op, left)?;
    if op == AdvancedOp::Log && right.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        Err(FloatBinaryError {
            code: 0xe19,
            offset: 5,
        })
    } else {
        Ok(())
    }
}

pub(crate) fn validate_log_pow_pair(
    op: AdvancedOp,
    left: LogPowInput,
    right: LogPowInput,
) -> Result<(), FloatBinaryError> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    let _control = X87Control::enter();
    validate_log_pow_values(op, left.converted(), right.converted())
}

/// Existing log10 rounding boundaries and power branches remain unchanged.
/// The caller must validate the left source before observing the right one.
pub(crate) fn evaluate_log_pow(
    op: AdvancedOp,
    left: LogPowInput,
    right: LogPowInput,
) -> Result<AdvancedResult, FloatBinaryError> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    let _control = X87Control::enter();
    let left = left.converted();
    let right = right.converted();
    validate_log_pow_values(op, left, right)?;
    let result = match op {
        AdvancedOp::Log => {
            let base = kernel(AdvancedOp::Log, left);
            let argument = kernel(AdvancedOp::Log, right);
            argument / base
        }
        AdvancedOp::Pow => power(left, right),
        _ => unreachable!("only DLOG/DPOW use binary advanced evaluation"),
    };
    Ok(AdvancedResult::normalize(result.to_bits()))
}

#[derive(Debug, Clone, PartialEq)]
pub struct AdvancedAction {
    pub op: AdvancedOp,
    pub source: FloatOperand,
    pub right: Option<FloatOperand>,
    pub dest: Device,
}
impl AdvancedAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let op = AdvancedOp::parse(name).ok_or("Bilinmeyen ileri kayan nokta işlemi")?;
        let arity = if op.binary() { 3 } else { 2 };
        if args.len() != arity {
            return Err(format!("{name}: {arity} operand gerekli"));
        }
        let source = FloatOperand::parse_comparison(&args[0])?;
        let right = if op.binary() {
            Some(FloatOperand::parse_comparison(&args[1])?)
        } else {
            None
        };
        let dest: Device = args[arity - 1].parse()?;
        if dest.area != Area::D {
            return Err(format!("{name} hedefi D yazmacı olmalı"));
        }
        double_devices(dest)?;
        Ok(Self {
            op,
            source,
            right,
            dest,
        })
    }
    pub fn name(&self) -> &'static str {
        self.op.name()
    }
    pub fn label(&self) -> String {
        match &self.right {
            Some(right) => format!("{} {} {right} {}", self.name(), self.source, self.dest),
            None => format!("{} {} {}", self.name(), self.source, self.dest),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        if matches!(self.op, AdvancedOp::Sin | AdvancedOp::Cos | AdvancedOp::Tan) {
            out.insert(Memory::system_device(1018));
        }
        self.source.devices(out);
        if let Some(right) = &self.right {
            right.devices(out);
        }
        out.extend(double_devices(self.dest).expect("validated advanced destination"));
    }
    pub fn display_value(&self, m: &Memory) -> String {
        f32::from_bits(m.double_word(self.dest) as u32).to_string()
    }
    pub fn execute(&self, m: &mut Memory) {
        if self.op == AdvancedOp::Sqrt {
            let source = DesqrInput::capture(&self.source, m);
            match evaluate_desqr(source) {
                Ok(result) => {
                    result.publish_flags(m);
                    result.write_value(m, self.dest);
                }
                Err(error) => m.operation_error(error.code, error.offset),
            }
            return;
        }
        if matches!(self.op, AdvancedOp::Exp | AdvancedOp::Ln) {
            let source = ExpLnInput::capture(&self.source, m);
            match evaluate_exp_ln(self.op, source) {
                Ok(result) => {
                    result.publish_flags(m);
                    result.write_value(m, self.dest);
                }
                Err(error) => m.operation_error(error.code, error.offset),
            }
            return;
        }
        if matches!(self.op, AdvancedOp::Log | AdvancedOp::Pow) {
            let left = LogPowInput::capture(&self.source, m);
            if let Err(error) = validate_log_pow_left(self.op, left) {
                m.operation_error(error.code, error.offset);
                return;
            }
            let right =
                LogPowInput::capture(self.right.as_ref().expect("binary advanced source"), m);
            match evaluate_log_pow(self.op, left, right) {
                Ok(result) => {
                    result.publish_flags(m);
                    result.write_value(m, self.dest);
                }
                Err(error) => m.operation_error(error.code, error.offset),
            }
            return;
        }
        let source = TrigInput::capture(&self.source, m);
        let degrees = trig_uses_degrees(self.op) && m.system_bit(1018);
        match evaluate_trig(self.op, source, degrees) {
            Ok(result) => {
                result.publish_flags(m);
                result.write_value(m, self.dest);
            }
            Err(error) => m.operation_error(error.code, error.offset),
        }
    }
}

fn degree_angle(a: f32) -> f32 {
    let intermediate = a * f32::from_bits(0x4049_0fdb);
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        // Native literal at41EB8C; preserve its extended precision until FSTP.
        let reciprocal: [u8; 10] = [0x0b, 0xb6, 0x60, 0x0b, 0xb6, 0x60, 0x0b, 0xb6, 0xf7, 0x3f];
        let mut result = 0.0f32;
        unsafe {
            std::arch::asm!(
                "fld tbyte ptr [{reciprocal}]",
                "fmul dword ptr [{input}]",
                "fstp dword ptr [{output}]",
                reciprocal = in(reg) reciprocal.as_ptr(),
                input = in(reg) &intermediate,
                output = in(reg) &mut result,
                options(nostack),
            );
        }
        result
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        (f64::from(intermediate) / 180.0) as f32
    }
}

fn power(a: f32, b: f32) -> f32 {
    // Native RTL sees an infinite base before its exponent special cases.
    // Its default result is +0 for negative exponents and +Inf otherwise.
    if a == f32::INFINITY {
        return if b.is_sign_negative() {
            0.0
        } else {
            f32::INFINITY
        };
    }
    // 518C9C..518CD7 routes every nonfinite exponent through the same
    // positive-overflow default, before the ordinary power algorithm.
    if !b.is_finite() {
        return f32::INFINITY;
    }
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if b == 0.0 {
            return 1.0;
        }
        // RTL518DB8..518E14 chooses repeated multiplication only for a
        // sufficiently small integer exponent and bounded base exponent.
        if b.abs() < 4096.0 && b.fract() == 0.0 {
            let n = b.abs() as u32;
            let exponent = n.ilog2();
            let base_high = (f64::from(a).to_bits() >> 48) as u16;
            let scaled = base_high.wrapping_shl(1).wrapping_sub(0x7fe0) as i16;
            let shift = exponent + 1;
            if scaled.wrapping_shl(shift) >> shift == scaled {
                return integer_power(a, n, b.is_sign_negative());
            }
        }
        let mut product = [0u8; 10];
        unsafe {
            // Native518D50..58: LN via FYL2X, multiply the exponent, then
            // store EXTENDED80, with no binary32/binary64 boundary here.
            std::arch::asm!(
                "fldln2", "fld dword ptr [{base}]", "fyl2x",
                "fmul dword ptr [{exponent}]", "fstp tbyte ptr [{product}]",
                base = in(reg) &a, exponent = in(reg) &b,
                product = in(reg) product.as_mut_ptr(), options(nostack),
            );
        }
        exp_extended(&product)
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        f64::from(a).powf(f64::from(b)) as f32
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn integer_power(a: f32, n: u32, negative: bool) -> f32 {
    let mut mask = if n == 1 { 0 } else { 1u32 << (n.ilog2() - 1) };
    let mut result = 0.0f32;
    unsafe {
        // RTL518E16..45: start at base, square for each remaining binary
        // exponent digit, multiply base on set digits, reciprocal LAST.
        std::arch::asm!(
            "fld dword ptr [{base}]", "fld st(0)",
            "test {mask:e}, {mask:e}", "jz 4f",
            "2:", "fmul st(0), st(0)",
            "test {number:e}, {mask:e}", "jz 3f", "fmul st(0), st(1)",
            "3:", "shr {mask:e}, 1", "jnz 2b",
            "4:", "fstp st(1)",
            "test {negative:e}, {negative:e}", "jz 5f",
            "fld1", "fdivrp st(1), st(0)",
            "5:", "fstp dword ptr [{output}]",
            base = in(reg) &a, number = in(reg) n,
            mask = inout(reg) mask, negative = in(reg) u32::from(negative),
            output = in(reg) &mut result, options(nostack),
        );
    }
    let _ = mask;
    result
}

fn exponential(a: f32) -> f32 {
    if !a.is_finite() {
        return if a.is_sign_negative() {
            0.0
        } else {
            f32::INFINITY
        };
    }
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let mut extended = [0u8; 10];
        unsafe {
            std::arch::asm!("fld dword ptr [{input}]", "fstp tbyte ptr [{output}]",
                input = in(reg) &a, output = in(reg) extended.as_mut_ptr(), options(nostack));
        }
        exp_extended(&extended)
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        f64::from(a).exp() as f32
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn exp_extended(input: &[u8; 10]) -> f32 {
    let sign_exponent = u16::from_le_bytes([input[8], input[9]]);
    let negative = sign_exponent & 0x8000 != 0;
    // Beyond1024 the binary32 outcome is certain. Avoid overflow in the
    // integer reduction scratch; native RTL similarly checks its f64 range.
    if sign_exponent & 0x7fff >= 0x4009 {
        return if negative { 0.0 } else { f32::INFINITY };
    }
    let half = 0.5f32; // Native52A4FC raw0000003F.
    let sqrt_two = [0x85u8, 0x64, 0xde, 0xf9, 0x33, 0xf3, 0x04, 0xb5, 0xff, 0x3f];
    let mut halves = 0i16;
    let mut result = 0.0f32;
    unsafe {
        // Transcription of5186ED..5187AC. FISTTP on positive2*y replaces
        // native FRNDINT under truncate mode, without changing caller CW.
        // The remaining FSCALE/F2XM1 operations and sqrt2 literal match RTL.
        std::arch::asm!(
            "fld tbyte ptr [{input}]", "fldl2e", "fmulp st(1), st(0)", "fabs",
            "fcom dword ptr [{half}]", "fnstsw ax", "test ah, 0x41", "jnz 3f",
            "fld1", "fld st(1)", "fscale", "fisttp word ptr [{halves}]",
            "fild word ptr [{halves}]", "fxch st(1)", "fchs", "fxch st(1)",
            "fscale", "fstp st(1)", "fsubp st(1), st(0)",
            "f2xm1", "fld1", "faddp st(1), st(0)",
            "shr word ptr [{halves}], 1", "jnc 2f",
            "fld tbyte ptr [{sqrt_two}]", "fmulp st(1), st(0)",
            "2:", "fild word ptr [{halves}]", "fxch st(1)",
            "fscale", "fstp st(1)", "jmp 4f",
            "3:", "f2xm1", "fld1", "faddp st(1), st(0)",
            "4:", "test {negative:e}, {negative:e}", "jz 5f",
            "fld1", "fdivrp st(1), st(0)",
            "5:", "fstp dword ptr [{output}]",
            input = in(reg) input.as_ptr(), half = in(reg) &half,
            halves = in(reg) &mut halves, sqrt_two = in(reg) sqrt_two.as_ptr(),
            negative = in(reg) u32::from(negative), output = in(reg) &mut result,
            // FNSTSW overwrites AX before later input registers are consumed.
            // An early clobber prevents optimized builds from reusing AX for
            // the sign flag or a pointer that remains live after that write.
            out("ax") _, options(nostack),
        );
    }
    result
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn inverse(a: f32, acos: bool) -> f32 {
    let magnitude = a.abs();
    let half = 0.5f32;
    let quarter = 0.25f32;
    let mut complement = u32::from(acos);
    let mut result = 0.0f32;
    unsafe {
        // Native518323..5183BB: FPATAN always takes smaller/larger,
        // optionally complemented with pi/2. Equality returns pi/4 directly.
        // Sign correction follows the complement and retains extended precision.
        std::arch::asm!(
            "fld dword ptr [{input}]", "fld1", "fld st(1)",
            "fmul st(0), st(0)", "fsubp st(1), st(0)", "fsqrt",
            "fcomi st(0), st(1)", "ja 3f", "jb 2f",
            "fstp st(0)", "fstp st(0)", "fldpi",
            "fmul dword ptr [{quarter}]", "jmp 5f",
            "2:", "fxch st(1)", "xor {complement:e}, 1",
            "3:", "fpatan", "test {complement:e}, {complement:e}", "jz 5f",
            "fldpi", "fmul dword ptr [{half}]", "fsubrp st(1), st(0)",
            "5:", "test {negative:e}, {negative:e}", "jz 6f",
            "fchs", "test {acos:e}, {acos:e}", "jz 6f", "fldpi", "faddp st(1), st(0)",
            "6:", "fstp dword ptr [{output}]",
            input = in(reg) &magnitude, half = in(reg) &half, quarter = in(reg) &quarter,
            complement = inout(reg) complement, negative = in(reg) u32::from(a.is_sign_negative()),
            acos = in(reg) u32::from(acos), output = in(reg) &mut result, options(nostack),
        );
    }
    let _ = complement;
    result
}

fn kernel(op: AdvancedOp, a: f32) -> f32 {
    if op == AdvancedOp::Exp {
        return exponential(a);
    }
    // Positive infinity log is returned directly by native wrappers. Keeping
    // this outside FYL2X also avoids an unnecessary x87 exceptional operation.
    if matches!(op, AdvancedOp::Ln | AdvancedOp::Log) && a == f32::INFINITY {
        return a;
    }
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let mut result = 0.0f32;
        // Each block pushes and pops exactly its own x87 temporaries. The
        // action's scoped guard owns CW changes; kernels do not use FNINIT.
        macro_rules! unary {
            ($($instruction:literal),+ $(,)?) => {{
                unsafe {
                    std::arch::asm!(
                        "fld dword ptr [{input}]", $($instruction,)+
                        "fstp dword ptr [{output}]",
                        input = in(reg) &a, output = in(reg) &mut result,
                        options(nostack),
                    );
                }
            }};
        }
        match op {
            AdvancedOp::Ln => unary!("fldln2", "fxch st(1)", "fyl2x"),
            AdvancedOp::Log => unary!("fldlg2", "fxch st(1)", "fyl2x"),
            AdvancedOp::Sqrt => unary!("fsqrt"),
            AdvancedOp::Sin => unary!("fsin"),
            AdvancedOp::Cos => unary!("fcos"),
            AdvancedOp::Tan => unary!("fptan", "fstp st(0)"),
            AdvancedOp::Atan => unary!("fld1", "fpatan"),
            AdvancedOp::Asin | AdvancedOp::Acos => {
                return inverse(a, op == AdvancedOp::Acos);
            }
            _ => unreachable!("binary op handled outside kernel"),
        }
        result
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        let a = f64::from(a);
        (match op {
            AdvancedOp::Ln => a.ln(),
            AdvancedOp::Log => a.log10(),
            AdvancedOp::Sqrt => a.sqrt(),
            AdvancedOp::Sin => a.sin(),
            AdvancedOp::Cos => a.cos(),
            AdvancedOp::Tan => a.tan(),
            AdvancedOp::Asin => a.asin(),
            AdvancedOp::Acos => a.acos(),
            AdvancedOp::Atan => a.atan(),
            _ => unreachable!(),
        }) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn optimized_exp_keeps_sign_live_after_fnstsw_overwrites_ax() {
        // Original ES2/SS2 native result at88:7EF882B7. A late AX clobber
        // allowed release register allocation to turn this into exp(-88).
        let mut m = Memory::default();
        m.set_double_word(d(0), std::hint::black_box(0x42b0_0000));
        let op = AdvancedAction::parse("DEXP", &["D0".into(), "D20".into()]).unwrap();
        std::hint::black_box(op).execute(&mut m);
        assert_eq!(m.double_word(d(20)) as u32, 0x7ef8_82b7);
        assert!(!m.system_bit(1020) && !m.system_bit(1021) && !m.system_bit(1022));
    }
    fn d(index: u16) -> Device {
        Device {
            area: Area::D,
            index,
        }
    }
    fn action(op: AdvancedOp, a: f32, b: Option<f32>) -> (Memory, AdvancedAction) {
        let mut m = Memory::default();
        m.set_double_word(d(0), a.to_bits() as i32);
        if let Some(b) = b {
            m.set_double_word(d(2), b.to_bits() as i32);
        }
        (
            m,
            AdvancedAction {
                op,
                source: FloatOperand::Register(d(0)),
                right: b.map(|_| FloatOperand::Register(d(2))),
                dest: d(10),
            },
        )
    }
    fn output(m: &Memory) -> f32 {
        f32::from_bits(m.double_word(d(10)) as u32)
    }
    #[test]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn action_scopes_native_precision_and_restores_caller_control_word() {
        // This guard also restores the test thread's environment on panic.
        let _restore = X87Control {
            original: X87Control::read(),
        };
        let caller = (X87Control::read() & !0x0f00) | 0x0600; // PC53, round down.
        X87Control::write(caller);
        {
            let _native = X87Control::enter();
            assert_eq!(X87Control::read(), (caller & !0x0f00) | 0x0300);
            assert_eq!(X87Control::read() & 0x003f, caller & 0x003f);
        }
        assert_eq!(X87Control::read(), caller);
        let (mut m, act) = action(AdvancedOp::Exp, 1.0, None);
        act.execute(&mut m);
        assert_eq!(output(&m).to_bits(), 0x402d_f854);
        assert_eq!(X87Control::read(), caller);
        let (mut m, error) = action(AdvancedOp::Ln, -1.0, None);
        error.execute(&mut m);
        assert_eq!(m.word(d(1067)), 0xe19);
        assert_eq!(X87Control::read(), caller);
    }
    #[test]
    fn domains_hold_output_flags_and_report_operand_pc() {
        for (op, a, b, code, pc) in [
            (AdvancedOp::Exp, f32::NAN, None, 0xe1a, 1),
            (AdvancedOp::Ln, f32::NAN, None, 0xe19, 1),
            (AdvancedOp::Log, 1.0, Some(f32::NAN), 0xe19, 1),
            (AdvancedOp::Log, 2.0, Some(0.0), 0xe19, 5),
            (AdvancedOp::Pow, -1.0, Some(f32::NAN), 0xe19, 1),
            (AdvancedOp::Sqrt, -1.0, None, 0xe1b, 1),
            (AdvancedOp::Sqrt, f32::NAN, None, 0xe1b, 1),
            (AdvancedOp::Asin, f32::NAN, None, 0xe1a, 1),
            (AdvancedOp::Acos, f32::INFINITY, None, 0xe19, 1),
        ] {
            let (mut m, act) = action(op, a, b);
            m.set_double_word(d(10), 123456);
            for flag in 1020..=1022 {
                m.set_system(flag, true);
            }
            act.execute(&mut m);
            assert_eq!(m.double_word(d(10)), 123456, "{op:?}");
            assert_eq!(m.word(d(1067)), code, "{op:?}");
            assert_eq!(m.word(d(1068)), pc, "{op:?}");
            assert!((1020..=1022).all(|flag| m.system_bit(flag)));
        }
    }
    #[test]
    fn square_root_zero_preserves_borrow_and_carry() {
        for a in [0.0, -0.0] {
            let (mut m, act) = action(AdvancedOp::Sqrt, a, None);
            m.set_system(1021, true);
            m.set_system(1022, true);
            act.execute(&mut m);
            assert_eq!(output(&m).to_bits(), 0);
            assert!((1020..=1022).all(|flag| m.system_bit(flag)));
        }
    }
    #[test]
    fn trig_cutoff_is_write_and_carry_without_plc_error() {
        for op in [AdvancedOp::Sin, AdvancedOp::Cos, AdvancedOp::Tan] {
            for a in [f32::NAN, f32::NEG_INFINITY, 9_007_199_254_740_992.0] {
                let (mut m, act) = action(op, a, None);
                m.set_system(1020, true);
                m.set_system(1021, true);
                act.execute(&mut m);
                assert_eq!(output(&m), f32::INFINITY);
                assert!(m.system_bit(1022));
                assert!(!m.system_bit(1020) && !m.system_bit(1021) && !m.system_bit(1067));
            }
        }
    }
    #[test]
    fn power_rtl_nonfinite_priority_precedes_mathematical_limits() {
        for base in [0.5, 1.0, 2.0] {
            for exponent in [
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::from_bits(0xffc0_1234),
            ] {
                let (mut m, act) = action(AdvancedOp::Pow, base, Some(exponent));
                act.execute(&mut m);
                assert_eq!(output(&m), f32::INFINITY);
                assert!(m.system_bit(1022));
                assert!(!m.system_bit(1067));
            }
        }
        for (exponent, expected) in [(-0.0, 0.0), (-1.0, 0.0), (0.0, f32::INFINITY)] {
            let (mut m, act) = action(AdvancedOp::Pow, f32::INFINITY, Some(exponent));
            act.execute(&mut m);
            assert_eq!(output(&m), expected);
        }
    }
    #[test]
    fn degree_mode_and_inverse_radians() {
        let (mut m, act) = action(AdvancedOp::Sin, 90.0, None);
        m.set_system(1018, true);
        act.execute(&mut m);
        assert_eq!(output(&m), 1.0);
        for op in [AdvancedOp::Asin, AdvancedOp::Acos, AdvancedOp::Atan] {
            let (mut m, act) = action(op, 0.5, None);
            act.execute(&mut m);
            let radians = output(&m);
            m.set_system(1018, true);
            act.execute(&mut m);
            assert_eq!(output(&m).to_bits(), radians.to_bits());
        }
        assert_eq!(kernel(AdvancedOp::Acos, -1.0), std::f32::consts::PI);
        assert_eq!(kernel(AdvancedOp::Asin, 1.0), std::f32::consts::FRAC_PI_2);
    }
    #[test]
    fn extended_exp_reduction_and_integer_power_paths() {
        // Cover small F2XM1, odd/even half-unit reduction, reciprocal,
        // binary32 overflow/subnormal rounding and zero special cases.
        for x in [
            0.0f32, -0.0, 0.125, -0.125, 0.5, -0.5, 1.0, -1.0, 2.0, -2.0, 10.0, -10.0, 88.0, 89.0,
            -90.0, -103.0, -104.0,
        ] {
            assert_eq!(
                exponential(x).to_bits(),
                (f64::from(x).exp() as f32).to_bits(),
                "exp({x})"
            );
        }
        for (a, b, expected) in [
            (2.0, 0.0, 1.0),
            (2.0, 1.0, 2.0),
            (2.0, 3.0, 8.0),
            (2.0, -3.0, 0.125),
            (0.5, -4.0, 16.0),
            (4.0, 0.5, 2.0),
            (9.0, 0.5, 3.0),
            (1.0, 4096.0, 1.0),
            (2.0, 4095.0, f32::INFINITY),
        ] {
            assert_eq!(power(a, b), expected, "pow({a},{b})");
        }
    }
    #[test]
    fn parsing_and_alias_snapshot() {
        for name in [
            "DEXP", "DLN", "DLOG", "DESQR", "DPOW", "DSIN", "DCOS", "DTAN", "DASIN", "DACOS",
            "DATAN",
        ] {
            let op = AdvancedOp::parse(name).unwrap();
            let mut args = vec!["K2".to_owned()];
            if op.binary() {
                args.push("H3".to_owned());
            }
            args.push("D10".to_owned());
            assert_eq!(AdvancedAction::parse(name, &args).unwrap().name(), name);
            args[0] = "F1.5".to_owned();
            assert!(AdvancedAction::parse(name, &args).is_err());
        }
        let (mut m, mut act) = action(AdvancedOp::Log, 2.0, Some(8.0));
        act.dest = d(0);
        act.execute(&mut m);
        assert_eq!(f32::from_bits(m.double_word(d(0)) as u32), 3.0);
    }
}
