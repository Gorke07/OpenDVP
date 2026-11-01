//! IEEE754 binary32 operands and the first floating-point instruction group.
use crate::additional_processing::AdditionalValue;
use crate::plc::{Area, Device, Memory, Relation, comparison_destinations, word_range};
use crate::wide::{WideOperand, double_devices};
use std::collections::BTreeSet;
use std::fmt;

/// Keep literal bits rather than an f32 field: signed zero and payload bits
/// must survive cloning, equality checks and native operand encoding.
#[derive(Debug, Clone, PartialEq)]
pub enum FloatOperand {
    Literal {
        bits: u32,
        text: String,
    },
    /// DECMP/DEZCP K/H operands convert signed integers numerically to float.
    Integer(i32),
    Register(Device),
}
impl FloatOperand {
    /// Contact operands: F literals or T/C/D floating-point devices.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let upper = raw.to_ascii_uppercase();
        if let Some(value) = upper.strip_prefix('F') {
            let parsed = value
                .parse::<f32>()
                .map_err(|_| format!("Geçersiz F sabiti: {raw}"))?;
            if !parsed.is_finite() {
                return Err("F sabiti sonlu 32 bit kayan nokta değeri olmalı".into());
            }
            return Ok(Self::Literal {
                bits: parsed.to_bits(),
                text: upper,
            });
        }
        let d: Device = upper.parse()?;
        double_devices(d)?;
        Ok(Self::Register(d))
    }
    /// Compare actions have a different documented operand matrix from contacts.
    pub fn parse_comparison(raw: &str) -> Result<Self, String> {
        let upper = raw.to_ascii_uppercase();
        if upper.starts_with('K') || upper.starts_with('H') {
            let WideOperand::Constant(value) = WideOperand::parse(&upper)? else {
                unreachable!()
            };
            return Ok(Self::Integer(value));
        }
        let d: Device = upper.parse()?;
        if d.area != Area::D {
            return Err("Kayan nokta karşılaştırma kaynağı K/H/D olmalı".into());
        }
        double_devices(d)?;
        Ok(Self::Register(d))
    }
    pub fn read_bits(&self, m: &Memory) -> u32 {
        match self {
            Self::Literal { bits, .. } => *bits,
            Self::Integer(value) => (*value as f32).to_bits(),
            Self::Register(d) => m.double_word(*d) as u32,
        }
    }
    pub fn read(&self, m: &Memory) -> f32 {
        f32::from_bits(self.read_bits(m))
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        if let Self::Register(d) = self {
            out.extend(double_devices(*d).expect("validated float device"));
        }
    }
}
impl fmt::Display for FloatOperand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Literal { text, .. } => f.write_str(text),
            Self::Integer(n) => write!(f, "K{n}"),
            Self::Register(d) => write!(f, "{d}"),
        }
    }
}

/// Comparison actions retain numeric K/H conversion; contacts already carry
/// raw binary32. Keep the actual source snapshots outside the pure relations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FloatComparisonInput {
    Integer(i32),
    RawBinary32(u32),
}
impl FloatComparisonInput {
    fn capture(source: &FloatOperand, memory: &Memory) -> Self {
        match source {
            FloatOperand::Integer(value) => Self::Integer(*value),
            _ => Self::RawBinary32(source.read_bits(memory)),
        }
    }
    fn value(self) -> f32 {
        match self {
            Self::Integer(value) => value as f32,
            Self::RawBinary32(raw) => f32::from_bits(raw),
        }
    }
}

pub(crate) fn evaluate_float_compare(
    left: FloatComparisonInput,
    right: FloatComparisonInput,
) -> Result<[bool; 3], crate::float_math::FloatBinaryError> {
    let left = left.value();
    let right = right.value();
    if left.is_nan() || right.is_nan() {
        return Err(crate::float_math::FloatBinaryError {
            code: 0xe1a,
            offset: 5,
        });
    }
    Ok([left > right, left == right, left < right])
}

pub(crate) fn evaluate_float_zone(
    lower: FloatComparisonInput,
    upper: FloatComparisonInput,
    value: FloatComparisonInput,
) -> [bool; 3] {
    let lower = lower.value();
    let upper = upper.value();
    let value = value.value();
    // Current handlers collapse reversed ordered bounds to the lower bound;
    // unordered x87 comparisons select the below-range bit without PLC error.
    if lower.is_nan() || upper.is_nan() || value.is_nan() {
        [true, false, false]
    } else if lower > upper {
        [lower > value, lower == value, lower < value]
    } else {
        [
            lower > value,
            lower <= value && value <= upper,
            value > upper,
        ]
    }
}

pub(crate) fn evaluate_float_relation(relation: &Relation, left: u32, right: u32) -> bool {
    let left = f32::from_bits(left);
    let right = f32::from_bits(right);
    if left.is_nan() || right.is_nan() {
        matches!(relation, Relation::Eq | Relation::Lt | Relation::Le)
    } else {
        relation.test(left, right)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FloatAction {
    Advanced(crate::float_advanced::AdvancedAction),
    Math(crate::float_math::FloatMathAction),
    Float {
        source: Device,
        dest: Device,
        wide: bool,
    },
    Integer {
        source: Device,
        dest: Device,
        wide: bool,
    },
    Move {
        source: FloatOperand,
        dest: AdditionalValue,
    },
    Compare {
        left: FloatOperand,
        right: FloatOperand,
        dest: Device,
    },
    Zone {
        lower: FloatOperand,
        upper: FloatOperand,
        value: FloatOperand,
        dest: Device,
    },
}
fn data_device(raw: &str) -> Result<Device, String> {
    let d: Device = raw.parse()?;
    if d.area != Area::D {
        return Err("FLT kaynağı ve hedefi D yazmacı olmalı".into());
    }
    Ok(d)
}
/// Shared arithmetic seam for concrete and indexed conversions. A mode is
/// selected once per visit; data access width is independent of encoded fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FloatConversionMode {
    Forward { wide: bool },
    Integer { wide: bool, flt_reverse: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FloatConversionResult {
    value: i32,
    wide: bool,
    /// None means forward FLT's zero-only OR, not a replacement flag image.
    flags: Option<[bool; 3]>,
}

impl FloatConversionMode {
    pub(crate) fn source_wide(self) -> bool {
        !matches!(self, Self::Forward { wide: false })
    }

    pub(crate) fn result_wide(self) -> bool {
        !matches!(self, Self::Integer { wide: false, .. })
    }

    pub(crate) fn begin(self, m: &mut Memory) {
        if matches!(self, Self::Integer { .. }) {
            for flag in 1020..=1022 {
                m.set_system(flag, false);
            }
        }
    }

    pub(crate) fn convert(self, source: i32) -> Option<FloatConversionResult> {
        let Self::Integer { wide, flt_reverse } = self else {
            let integer = if self.source_wide() {
                source
            } else {
                i32::from(source as i16)
            };
            return Some(FloatConversionResult {
                value: (integer as f32).to_bits() as i32,
                wide: true,
                flags: None,
            });
        };
        let value = f32::from_bits(source as u32) as f64;
        // Reverse FLT rejects exponent-255; INT rejects NaNs but saturates Inf.
        if value.is_nan() || (flt_reverse && value.is_infinite()) {
            return None;
        }
        let (min, max) = if wide {
            (f64::from(i32::MIN), f64::from(i32::MAX as f32))
        } else {
            (f64::from(i16::MIN), f64::from(i16::MAX))
        };
        let overflow = value < min || value > max;
        let result = if value < min {
            if wide { i32::MIN } else { i32::from(i16::MIN) }
        } else if value > max {
            if wide { i32::MAX } else { i32::from(i16::MAX) }
        } else {
            // Exact +2^31 reaches truncation because the native binary32 upper
            // threshold rounds to2^31; retain the x87 low32 result semantics.
            value.trunc() as i64 as i32
        };
        let fraction = !overflow && result != 0 && f64::from(result as f32) != value;
        Some(FloatConversionResult {
            value: result,
            wide,
            flags: Some([result == 0, fraction, overflow]),
        })
    }
}

impl FloatConversionResult {
    pub(crate) fn publish_flags(self, m: &mut Memory) {
        if let Some(flags) = self.flags {
            for (flag, value) in (1020..=1022).zip(flags) {
                m.set_system(flag, value);
            }
        } else if self.value == 0 {
            m.set_system(1020, true);
        }
    }

    pub(crate) fn write_value(self, m: &mut Memory, dest: Device) {
        if self.wide {
            m.set_double_word(dest, self.value);
        } else {
            m.set_word(dest, self.value as i16);
        }
    }
}

fn convert_to_integer(m: &mut Memory, source: Device, dest: Device, wide: bool, flt_reverse: bool) {
    let mode = FloatConversionMode::Integer { wide, flt_reverse };
    mode.begin(m);
    let Some(result) = mode.convert(m.double_word(source)) else {
        m.operation_error(0xe1a, 1);
        return;
    };
    result.publish_flags(m);
    result.write_value(m, dest);
}
impl FloatAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if crate::float_advanced::AdvancedOp::parse(name).is_some() {
            return crate::float_advanced::AdvancedAction::parse(name, args).map(Self::Advanced);
        }
        if crate::float_math::MathOp::parse(name).is_some() {
            return crate::float_math::FloatMathAction::parse(name, args).map(Self::Math);
        }
        let expected = match name {
            "DECMP" => 3,
            "DEZCP" => 4,
            _ => 2,
        };
        if args.len() != expected {
            return Err(format!("{name}: {expected} operand gerekli"));
        }
        Ok(match name {
            "FLT" | "DFLT" => {
                let source = data_device(&args[0])?;
                let dest = data_device(&args[1])?;
                // M1081 can reverse the direction at runtime: either case needs
                // valid pairs for the source and the floating-point destination.
                double_devices(source)?;
                double_devices(dest)?;
                Self::Float {
                    source,
                    dest,
                    wide: name == "DFLT",
                }
            }
            "INT" | "DINT" => {
                let source: Device = args[0].parse()?;
                double_devices(source)?;
                let dest: Device = args[1].parse()?;
                let wide = name == "DINT";
                if wide {
                    double_devices(dest)?;
                } else {
                    word_range(dest, 1)?;
                }
                Self::Integer { source, dest, wide }
            }
            "DMOVR" => {
                let source = FloatOperand::parse(&args[0])?;
                if !matches!(source, FloatOperand::Literal { .. }) {
                    return Err("DMOVR kaynağı F sabiti olmalı".into());
                }
                let dest = AdditionalValue::parse(&args[1], true, true)?;
                Self::Move { source, dest }
            }
            "DECMP" => {
                let dest: Device = args[2].parse()?;
                comparison_destinations(dest)?;
                Self::Compare {
                    left: FloatOperand::parse_comparison(&args[0])?,
                    right: FloatOperand::parse_comparison(&args[1])?,
                    dest,
                }
            }
            "DEZCP" => {
                let dest: Device = args[3].parse()?;
                comparison_destinations(dest)?;
                Self::Zone {
                    lower: FloatOperand::parse_comparison(&args[0])?,
                    upper: FloatOperand::parse_comparison(&args[1])?,
                    value: FloatOperand::parse_comparison(&args[2])?,
                    dest,
                }
            }
            _ => return Err("Bilinmeyen kayan nokta komutu".into()),
        })
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Math(action) => action.op.name(),
            Self::Advanced(action) => action.op.name(),
            Self::Float { wide: false, .. } => "FLT",
            Self::Float { wide: true, .. } => "DFLT",
            Self::Integer { wide: false, .. } => "INT",
            Self::Integer { wide: true, .. } => "DINT",
            Self::Move { .. } => "DMOVR",
            Self::Compare { .. } => "DECMP",
            Self::Zone { .. } => "DEZCP",
        }
    }
    pub fn dest(&self) -> Device {
        match self {
            Self::Math(action) => action.dest,
            Self::Advanced(action) => action.dest,
            Self::Float { dest, .. }
            | Self::Integer { dest, .. }
            | Self::Compare { dest, .. }
            | Self::Zone { dest, .. } => *dest,
            Self::Move { dest, .. } => dest.base().expect("validated DMOVR destination"),
        }
    }
    pub fn label(&self) -> String {
        let name = self.name();
        match self {
            Self::Math(action) => action.label(),
            Self::Advanced(action) => action.label(),
            Self::Float { source, dest, .. } | Self::Integer { source, dest, .. } => {
                format!("{name} {source} {dest}")
            }
            Self::Move { source, dest } => format!("{name} {source} {dest}"),
            Self::Compare { left, right, dest } => format!("{name} {left} {right} {dest}"),
            Self::Zone {
                lower,
                upper,
                value,
                dest,
            } => format!("{name} {lower} {upper} {value} {dest}"),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        out.extend((1020..=1022).map(Memory::system_device));
        Memory::error_devices(out);
        match self {
            Self::Math(action) => action.devices(out),
            Self::Advanced(action) => action.devices(out),
            Self::Float { source, dest, .. } => {
                out.insert(Memory::system_device(1081));
                out.extend(double_devices(*source).expect("validated FLT source"));
                out.extend(double_devices(*dest).expect("validated FLT destination"));
            }
            Self::Integer { source, dest, wide } => {
                out.extend(double_devices(*source).expect("validated INT source"));
                if *wide {
                    out.extend(double_devices(*dest).expect("validated DINT destination"));
                } else {
                    out.insert(*dest);
                }
            }
            Self::Move { source, dest } => {
                source.devices(out);
                dest.devices(out);
            }
            Self::Compare { left, right, dest } => {
                left.devices(out);
                right.devices(out);
                out.extend(comparison_destinations(*dest).expect("validated DECMP destination"));
            }
            Self::Zone {
                lower,
                upper,
                value,
                dest,
            } => {
                lower.devices(out);
                upper.devices(out);
                value.devices(out);
                out.extend(comparison_destinations(*dest).expect("validated DEZCP destination"));
            }
        }
    }
    pub fn execute(&self, m: &mut Memory) {
        match self {
            Self::Math(action) => {
                action.execute(m);
            }
            Self::Advanced(action) => action.execute(m),
            Self::Float { source, dest, wide } => {
                if m.system_bit(1081) {
                    convert_to_integer(m, *source, *dest, *wide, true);
                } else {
                    let integer = if *wide {
                        m.double_word(*source)
                    } else {
                        i32::from(m.word(*source))
                    };
                    let result = FloatConversionMode::Forward { wide: *wide }
                        .convert(integer)
                        .expect("integer to binary32 always valid");
                    result.publish_flags(m);
                    result.write_value(m, *dest);
                }
            }
            Self::Integer { source, dest, wide } => {
                convert_to_integer(m, *source, *dest, *wide, false)
            }
            Self::Move { source, dest } => {
                let bits = source.read_bits(m);
                dest.write(m, bits as i32);
            }
            Self::Compare { left, right, dest } => {
                let left = FloatComparisonInput::capture(left, m);
                let right = FloatComparisonInput::capture(right, m);
                let results = match evaluate_float_compare(left, right) {
                    Ok(results) => results,
                    Err(error) => {
                        m.operation_error(error.code, error.offset);
                        return;
                    }
                };
                for (d, value) in comparison_destinations(*dest)
                    .unwrap()
                    .into_iter()
                    .zip(results)
                {
                    m.set_internal(d, value);
                }
            }
            Self::Zone {
                lower,
                upper,
                value,
                dest,
            } => {
                let lower = FloatComparisonInput::capture(lower, m);
                let upper = FloatComparisonInput::capture(upper, m);
                let value = FloatComparisonInput::capture(value, m);
                let results = evaluate_float_zone(lower, upper, value);
                for (d, value) in comparison_destinations(*dest)
                    .unwrap()
                    .into_iter()
                    .zip(results)
                {
                    m.set_internal(d, value);
                }
            }
        }
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        if let Self::Math(action) = self
            && action.op == crate::float_math::MathOp::Decimal
            && m.decimal_scratch_profile == crate::float_math::DecimalScratchProfile::LegacyVisible
            && m.data_register_limit.unwrap_or(9999) >= 5001
        {
            // WPL ES2 exposes the legacy scratch pair. Current ES2 removed
            // those stores; both SS2 images keep them beyond their public range.
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
    pub fn display_value(&self, m: &Memory) -> String {
        match self {
            Self::Math(action) => action.display_value(m),
            Self::Advanced(action) => action.display_value(m),
            Self::Float { dest, wide, .. } if m.system_bit(1081) => {
                if *wide {
                    m.double_word(*dest).to_string()
                } else {
                    m.word(*dest).to_string()
                }
            }
            Self::Float { dest, .. } => f32::from_bits(m.double_word(*dest) as u32).to_string(),
            Self::Move { dest, .. } => f32::from_bits(dest.read(m) as u32).to_string(),
            Self::Integer { dest, wide, .. } => {
                if *wide {
                    m.double_word(*dest).to_string()
                } else {
                    m.word(*dest).to_string()
                }
            }
            Self::Compare { dest, .. } | Self::Zone { dest, .. } => comparison_destinations(*dest)
                .unwrap()
                .into_iter()
                .map(|d| if m.bit(d) { "1" } else { "0" })
                .collect::<Vec<_>>()
                .join(" / "),
        }
    }
}

pub fn parse_spec(name: &str) -> Option<(&'static str, Relation)> {
    for (name_prefix, prefix) in [("FLD", "LD"), ("FAND", "AND"), ("FOR", "OR")] {
        if let Some(suffix) = name.strip_prefix(name_prefix) {
            return Some((prefix, Relation::parse(suffix)?));
        }
    }
    None
}
#[derive(Debug, Clone, PartialEq)]
pub struct FloatPredicate {
    pub relation: Relation,
    pub left: FloatOperand,
    pub right: FloatOperand,
}
impl FloatPredicate {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let spec = parse_spec(name).ok_or("Bilinmeyen kayan nokta kontağı")?;
        if args.len() != 2 {
            return Err(format!("{name}: 2 operand gerekli"));
        }
        Ok(Self {
            relation: spec.1,
            left: FloatOperand::parse(&args[0])?,
            right: FloatOperand::parse(&args[1])?,
        })
    }
    pub fn evaluate(&self, m: &Memory) -> bool {
        let left = self.left.read_bits(m);
        let right = self.right.read_bits(m);
        evaluate_float_relation(&self.relation, left, right)
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        self.left.devices(out);
        self.right.devices(out);
    }
    pub fn label(&self, prefix: &str) -> String {
        let prefix = match prefix {
            "LD" | "FLD" => "FLD",
            "AND" | "FAND" => "FAND",
            "OR" | "FOR" => "FOR",
            _ => prefix,
        };
        format!(
            "{prefix}{} {} {}",
            self.relation.symbol(),
            self.left,
            self.right
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d(raw: &str) -> Device {
        raw.parse().unwrap()
    }
    fn args(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|s| s.to_string()).collect()
    }
    fn action(name: &str, raw: &[&str]) -> FloatAction {
        FloatAction::parse(name, &args(raw)).unwrap()
    }
    fn store(m: &mut Memory, raw: &str, value: f32) {
        m.set_double_word(d(raw), value.to_bits() as i32);
    }

    #[test]
    fn native_special_value_errors_threshold_and_flag_transitions() {
        let mut m = Memory::default();
        m.set_double_word(d("D0"), 0x7f800000);
        m.set_word(d("D20"), 1234);
        m.set_system(1081, true);
        for flag in 1020..=1022 {
            m.set_system(flag, true);
        }
        action("FLT", &["D0", "D20"]).execute(&mut m);
        assert_eq!(m.word(d("D20")), 1234);
        assert_eq!(m.word(d("D1067")), 0xe1a);
        assert!((1020..=1022).all(|flag| !m.system_bit(flag)));
        action("INT", &["D0", "D20"]).execute(&mut m);
        assert_eq!(m.word(d("D20")), i16::MAX);
        assert!(m.system_bit(1022));
        m.set_double_word(d("D0"), 0x4f000000);
        action("DINT", &["D0", "D20"]).execute(&mut m);
        assert_eq!(m.double_word(d("D20")), i32::MIN);
        assert!(m.system_bit(1021));
        assert!(!m.system_bit(1022));
        m.set_system(1081, false);
        for flag in 1020..=1022 {
            m.set_system(flag, true);
        }
        m.set_word(d("D0"), 23);
        action("FLT", &["D0", "D20"]).execute(&mut m);
        assert!((1020..=1022).all(|flag| m.system_bit(flag)));
    }
    #[test]
    fn native_unordered_contacts_and_compare_actions_differ() {
        let mut m = Memory::default();
        m.set_double_word(d("D0"), 0x7fc01234);
        for (relation, expected) in [
            ("=", true),
            (">", false),
            ("<", true),
            ("<>", false),
            ("<=", true),
            (">=", false),
        ] {
            let p = FloatPredicate::parse(&format!("FLD{relation}"), &args(&["D0", "F0"])).unwrap();
            assert_eq!(p.evaluate(&m), expected);
        }
        m.set_internal(d("M10"), true);
        m.set_internal(d("M11"), true);
        m.set_internal(d("M12"), true);
        action("DECMP", &["D0", "K0", "M10"]).execute(&mut m);
        assert!(m.bit(d("M10")) && m.bit(d("M11")) && m.bit(d("M12")));
        assert_eq!(m.word(d("D1068")), 5);
        action("DEZCP", &["K0", "K1", "D0", "M10"]).execute(&mut m);
        assert_eq!(
            [m.bit(d("M10")), m.bit(d("M11")), m.bit(d("M12"))],
            [true, false, false]
        );
    }
    #[test]
    fn documented_integer_float_bits_and_dynamic_reverse_mode() {
        let mut m = Memory::default();
        m.set_word(d("D0"), 10);
        action("FLT", &["D0", "D12"]).execute(&mut m);
        assert_eq!(m.double_word(d("D12")) as u32, 0x4120_0000);
        m.set_double_word(d("D0"), 100000);
        action("DFLT", &["D0", "D20"]).execute(&mut m);
        assert_eq!(m.double_word(d("D20")) as u32, 0x47c3_5000);
        m.set_system(1081, true);
        action("FLT", &["D20", "D12"]).execute(&mut m);
        assert_eq!(m.word(d("D12")), 32767);
        assert!(m.system_bit(1022));
        action("DFLT", &["D20", "D30"]).execute(&mut m);
        assert_eq!(m.double_word(d("D30")), 100000);
        assert!(!m.system_bit(1022));
    }

    #[test]
    fn binary32_bit_order_and_literal_negative_zero_are_preserved() {
        let mut m = Memory::default();
        action("DMOVR", &["F1.200E+0", "D10"]).execute(&mut m);
        assert_eq!(m.double_word(d("D10")) as u32, 1.2f32.to_bits());
        assert_eq!(m.word(d("D10")) as u16, 0x999a);
        assert_eq!(m.word(d("D11")) as u16, 0x3f99);
        let literal = FloatOperand::parse("F-0.0").unwrap();
        assert_eq!(literal.read_bits(&m), 0x8000_0000);
        assert_eq!(FloatOperand::parse(&literal.to_string()).unwrap(), literal);
        action("DMOVR", &["F-0.0", "C200"]).execute(&mut m);
        assert_eq!(m.double_word(d("C200")) as u32, 0x8000_0000);
        // Device reads retain payload bits without converting through text.
        m.set_double_word(d("D0"), 0x7fc0_1234);
        assert_eq!(
            FloatOperand::parse("D0").unwrap().read_bits(&m),
            0x7fc0_1234
        );
    }

    #[test]
    fn finite_int_conversion_truncates_saturates_and_reports_fraction() {
        let mut m = Memory::default();
        for (value, expected, borrow, carry) in [
            (12.75, 12, true, false),
            (-12.75, -12, true, false),
            (-0.75, 0, false, false),
            (100000.0, 32767, false, true),
            (-100000.0, -32768, false, true),
            (0.0, 0, false, false),
        ] {
            store(&mut m, "T20", value);
            action("INT", &["T20", "D0"]).execute(&mut m);
            assert_eq!(m.word(d("D0")), expected);
            assert_eq!(m.system_bit(1020), expected == 0);
            assert_eq!(m.system_bit(1021), borrow);
            assert_eq!(m.system_bit(1022), carry);
        }
        store(&mut m, "C200", -123456.75);
        action("DINT", &["C200", "C201"]).execute(&mut m);
        assert_eq!(m.double_word(d("C201")), -123456);
        assert!(m.system_bit(1021));
    }

    #[test]
    fn comparisons_distinguish_integer_constants_from_float_raw_words() {
        let mut m = Memory::default();
        store(&mut m, "D0", 23.0);
        action("DECMP", &["D0", "K23", "M10"]).execute(&mut m);
        assert_eq!(
            [m.bit(d("M10")), m.bit(d("M11")), m.bit(d("M12"))],
            [false, true, false]
        );
        // H41B80000 means integer1102577664 numerically converted tofloat,
        // rather than the rawbitpattern for23.0.
        action("DECMP", &["D0", "H41B80000", "M10"]).execute(&mut m);
        assert!(m.bit(d("M12")));
        assert!(!m.bit(d("M11")));
        action("DECMP", &["K24", "D0", "Y0"]).execute(&mut m);
        assert!(m.bit(d("Y0")));
    }

    #[test]
    fn zone_includes_both_bounds_and_uses_lower_bound_fallback() {
        let mut m = Memory::default();
        let zone = action("DEZCP", &["K10", "K20", "D0", "M10"]);
        for (value, result) in [
            (9.5, [true, false, false]),
            (10.0, [false, true, false]),
            (15.0, [false, true, false]),
            (20.0, [false, true, false]),
            (20.5, [false, false, true]),
        ] {
            store(&mut m, "D0", value);
            zone.execute(&mut m);
            assert_eq!([m.bit(d("M10")), m.bit(d("M11")), m.bit(d("M12"))], result);
        }
        store(&mut m, "D0", 15.0);
        action("DEZCP", &["K20", "K10", "D0", "M10"]).execute(&mut m);
        assert_eq!(
            [m.bit(d("M10")), m.bit(d("M11")), m.bit(d("M12"))],
            [true, false, false]
        );
    }

    #[test]
    fn contact_relations_preserve_prefixes_and_compare_without_epsilon() {
        let mut m = Memory::default();
        store(&mut m, "D0", 1.2);
        for prefix in ["FLD", "FAND", "FOR"] {
            for (relation, expected) in [
                ("=", true),
                (">", false),
                ("<", false),
                ("<>", false),
                ("<=", true),
                (">=", true),
            ] {
                let name = format!("{prefix}{relation}");
                let predicate = FloatPredicate::parse(&name, &args(&["D0", "F1.2"])).unwrap();
                assert_eq!(predicate.evaluate(&m), expected);
                assert_eq!(
                    predicate.label(parse_spec(&name).unwrap().0),
                    format!("{name} D0 F1.2")
                );
            }
        }
        m.set_double_word(d("D0"), (1.2f32.to_bits() + 1) as i32);
        assert!(
            !FloatPredicate::parse("FLD=", &args(&["D0", "F1.2"]))
                .unwrap()
                .evaluate(&m)
        );
        assert!(
            FloatPredicate::parse("FLD>", &args(&["D0", "F1.2"]))
                .unwrap()
                .evaluate(&m)
        );
    }

    #[test]
    fn aliasing_reads_source_before_conversion_overwrites_its_pair() {
        let mut m = Memory::default();
        m.set_word(d("D0"), 23);
        action("FLT", &["D0", "D0"]).execute(&mut m);
        assert_eq!(m.double_word(d("D0")) as u32, 0x41b8_0000);
        m.set_system(1081, true);
        action("DFLT", &["D0", "D0"]).execute(&mut m);
        assert_eq!(m.double_word(d("D0")), 23);
        store(&mut m, "D0", 12345.75);
        action("INT", &["D0", "D1"]).execute(&mut m);
        assert_eq!(m.word(d("D1")), 12345);
    }

    #[test]
    fn documented_operand_matrices_and_pair_boundaries_are_enforced() {
        for (name, raw) in [
            ("FLT", vec!["K1", "D0"]),
            ("DFLT", vec!["T0", "D0"]),
            ("FLT", vec!["D0", "C0"]),
            ("INT", vec!["F1.2", "D0"]),
            ("INT", vec!["D0", "C200"]),
            ("DINT", vec!["T255", "D0"]),
            ("DMOVR", vec!["D0", "D10"]),
            ("DMOVR", vec!["K1", "D0"]),
            ("DMOVR", vec!["F1.2", "D9999"]),
            ("DECMP", vec!["T0", "K1", "M0"]),
            ("DECMP", vec!["F1.2", "D0", "M0"]),
            ("DEZCP", vec!["K0", "K1", "C0", "M0"]),
            ("DECMP", vec!["K1", "D0", "X0"]),
        ] {
            assert!(
                FloatAction::parse(name, &args(&raw)).is_err(),
                "{name} {raw:?}"
            );
        }
        assert!(FloatPredicate::parse("FLD=", &args(&["K1", "D0"])).is_err());
        assert!(FloatOperand::parse("FNaN").is_err());
        assert!(FloatOperand::parse("F1e99").is_err());
        let mut tracked = BTreeSet::new();
        action("DINT", &["C200", "C255"]).devices(&mut tracked);
        assert!(tracked.contains(&d("C200")) && tracked.contains(&d("C255")));
        assert!(!tracked.contains(&d("C201")));
    }
}
