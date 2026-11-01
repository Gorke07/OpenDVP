//! Signed double-word instruction semantics. Native encoding is in es2_encoder.
use crate::additional_processing::AdditionalValue;
use crate::block_operand::BlockDestination;
use crate::calendar_operand::{CalendarDestination, CalendarValue};
use crate::multiply_destination::MultiplyDestination;
use crate::plc::{Area, Device, Memory, comparison_destinations, word_range};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub enum WideOperand {
    Constant(i32),
    Register(Device),
}
impl WideOperand {
    pub fn parse(raw: &str) -> Result<Self, String> {
        if let Some(n) = raw.strip_prefix('K') {
            return n
                .parse::<i32>()
                .map(Self::Constant)
                .map_err(|_| format!("32 bit sabit geçersiz: {raw}"));
        }
        if let Some(n) = raw.strip_prefix('H') {
            return u32::from_str_radix(n, 16)
                .map(|n| Self::Constant(n as i32))
                .map_err(|_| format!("32 bit hex sabit geçersiz: {raw}"));
        }
        let d: Device = raw.parse()?;
        double_devices(d)?;
        Ok(Self::Register(d))
    }
    pub fn read(&self, m: &Memory) -> i32 {
        match self {
            Self::Constant(v) => *v,
            Self::Register(d) => m.double_word(*d),
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        if let Self::Register(d) = self {
            out.extend(double_devices(*d).expect("validated double word"));
        }
    }
}
impl std::fmt::Display for WideOperand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Constant(v) => write!(f, "K{v}"),
            Self::Register(d) => write!(f, "{d}"),
        }
    }
}
pub fn double_devices(d: Device) -> Result<Vec<Device>, String> {
    // Native legacy boundary: C199 zero-extends one word; it does not
    // consume the independent 32-bit counter C200 as its upper word.
    if d.area == Area::C && d.index == 199 {
        return Ok(vec![d]);
    }
    if d.area == Area::C && d.index >= 200 && d.index <= 255 {
        Ok(vec![d])
    } else {
        word_range(d, 2)
    }
}
fn slot(d: Device, n: u16) -> Result<Device, String> {
    let stride = if d.area == Area::C && d.index >= 200 {
        1
    } else {
        2
    };
    let index = d
        .index
        .checked_add(n.checked_mul(stride).ok_or("32 bit aralık taşıyor")?)
        .ok_or("32 bit aralık taşıyor")?;
    let next = Device {
        area: d.area,
        index,
    };
    if d.area == Area::C && d.index < 200 && next.index >= 200 {
        return Err("32 bit aralık C199/C200 alan sınırını aşamaz".into());
    }
    double_devices(next)?;
    Ok(next)
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WideOp {
    Move,
    Complement,
    Fill,
    Exchange,
    Bcd,
    Bin,
    Add,
    Sub,
    Multiply,
    Divide,
    MultiplyTruncated,
    DivideQuotient,
    Inc,
    Dec,
    And,
    Or,
    Xor,
    Neg,
    Compare,
    Zone,
}
impl WideOp {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "DMOV" => Self::Move,
            "DCML" => Self::Complement,
            "DFMOV" => Self::Fill,
            "DXCH" => Self::Exchange,
            "DBCD" => Self::Bcd,
            "DBIN" => Self::Bin,
            "DADD" => Self::Add,
            "DSUB" => Self::Sub,
            "DMUL" => Self::Multiply,
            "DDIV" => Self::Divide,
            "MUL32" => Self::MultiplyTruncated,
            "DIV32" => Self::DivideQuotient,
            "DINC" => Self::Inc,
            "DDEC" => Self::Dec,
            "DAND" => Self::And,
            "DOR" => Self::Or,
            "DXOR" => Self::Xor,
            "DNEG" => Self::Neg,
            "DCMP" => Self::Compare,
            "DZCP" => Self::Zone,
            _ => return None,
        })
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Move => "DMOV",
            Self::Complement => "DCML",
            Self::Fill => "DFMOV",
            Self::Exchange => "DXCH",
            Self::Bcd => "DBCD",
            Self::Bin => "DBIN",
            Self::Add => "DADD",
            Self::Sub => "DSUB",
            Self::Multiply => "DMUL",
            Self::Divide => "DDIV",
            Self::MultiplyTruncated => "MUL32",
            Self::DivideQuotient => "DIV32",
            Self::Inc => "DINC",
            Self::Dec => "DDEC",
            Self::And => "DAND",
            Self::Or => "DOR",
            Self::Xor => "DXOR",
            Self::Neg => "DNEG",
            Self::Compare => "DCMP",
            Self::Zone => "DZCP",
        }
    }
    pub fn header(self) -> u16 {
        match self {
            Self::Compare => 0x2500,
            Self::Zone => 0x2700,
            Self::Move => 0x2900,
            Self::Complement => 0x2d00,
            Self::Fill => 0x3100,
            Self::Exchange => 0x3300,
            Self::Bcd => 0x3500,
            Self::Bin => 0x3700,
            Self::Add => 0x3900,
            Self::Sub => 0x3b00,
            Self::Multiply => 0x3d00,
            Self::Divide => 0x3f00,
            Self::MultiplyTruncated => 0xf500,
            Self::DivideQuotient => 0xf700,
            Self::Inc => 0x4100,
            Self::Dec => 0x4300,
            Self::And => 0x4500,
            Self::Or => 0x4700,
            Self::Xor => 0x4900,
            Self::Neg => 0x4b00,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum WideDestination {
    Register(Device),
    Numeric(CalendarDestination),
    Block(BlockDestination),
    Product(MultiplyDestination),
}
impl WideDestination {
    fn base(&self) -> Device {
        match self {
            Self::Register(d) => *d,
            Self::Numeric(value) => value.base(),
            Self::Block(value) => value.base(),
            Self::Product(value) => value.base(),
        }
    }
    fn read(&self, m: &Memory) -> i32 {
        match self {
            Self::Register(d) => m.double_word(*d),
            Self::Numeric(value) => value.read(m),
            Self::Block(value) => value.read(m),
            Self::Product(value) => value.read_pair(m)[0],
        }
    }
    fn write(&self, m: &mut Memory, value: i32) {
        match self {
            Self::Register(d) => m.set_double_word(*d, value),
            Self::Numeric(target) => target.write(m, value),
            Self::Block(target) => target.write(m, value),
            Self::Product(_) => unreachable!("full results use write_pair"),
        }
    }
}
impl std::fmt::Display for WideDestination {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Register(d) => d.fmt(f),
            Self::Numeric(value) => value.fmt(f),
            Self::Block(value) => value.fmt(f),
            Self::Product(value) => value.fmt(f),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WideSource {
    Read(CalendarValue),
    Writable(CalendarDestination),
}
impl WideSource {
    pub(crate) fn value(&self) -> &CalendarValue {
        match self {
            Self::Read(value) => value,
            Self::Writable(target) => target.value(),
        }
    }
    fn read(&self, m: &Memory) -> i32 {
        self.value().read(m)
    }
    fn devices(&self, out: &mut BTreeSet<Device>) {
        self.value().devices(out);
    }
}
impl std::fmt::Display for WideSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value().fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WideAction {
    pub op: WideOp,
    pub sources: Vec<WideSource>,
    pub dest: WideDestination,
    pub count: u16,
}
impl WideAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        let op = WideOp::parse(name).ok_or("Bilinmeyen 32 bit komut")?;
        let source_count = match op {
            WideOp::Inc | WideOp::Dec | WideOp::Neg => 0,
            WideOp::Zone => 3,
            WideOp::Compare
            | WideOp::Add
            | WideOp::Sub
            | WideOp::Multiply
            | WideOp::Divide
            | WideOp::MultiplyTruncated
            | WideOp::DivideQuotient
            | WideOp::And
            | WideOp::Or
            | WideOp::Xor => 2,
            _ => 1,
        };
        let expected = source_count + 1 + usize::from(op == WideOp::Fill);
        if args.len() != expected {
            return Err(format!("{name}: {expected} operand gerekli"));
        }
        let dest = if matches!(op, WideOp::Multiply | WideOp::Divide) {
            WideDestination::Product(MultiplyDestination::parse(&args[source_count], true)?)
        } else if op == WideOp::Fill {
            WideDestination::Block(BlockDestination::parse(&args[source_count], true)?)
        } else if matches!(
            op,
            WideOp::Add
                | WideOp::Sub
                | WideOp::Inc
                | WideOp::Dec
                | WideOp::Move
                | WideOp::Complement
                | WideOp::And
                | WideOp::Or
                | WideOp::Xor
                | WideOp::Neg
                | WideOp::Bcd
                | WideOp::Bin
                | WideOp::Exchange
                | WideOp::MultiplyTruncated
                | WideOp::DivideQuotient
        ) {
            WideDestination::Numeric(CalendarDestination::parse(&args[source_count], true)?)
        } else {
            WideDestination::Register(args[source_count].parse::<Device>()?)
        };
        let sources = args[..source_count]
            .iter()
            .map(|s| {
                if op == WideOp::Exchange {
                    return CalendarDestination::parse(s, true).map(WideSource::Writable);
                }
                if matches!(
                    op,
                    WideOp::Compare
                        | WideOp::Fill
                        | WideOp::Zone
                        | WideOp::Add
                        | WideOp::Sub
                        | WideOp::Move
                        | WideOp::Complement
                        | WideOp::And
                        | WideOp::Or
                        | WideOp::Xor
                        | WideOp::Bcd
                        | WideOp::Bin
                        | WideOp::MultiplyTruncated
                        | WideOp::DivideQuotient
                        | WideOp::Multiply
                        | WideOp::Divide
                ) {
                    CalendarValue::parse(s, true).map(WideSource::Read)
                } else {
                    // Keep instruction-specific matrices for the remaining
                    // families; accepting a numeric type is not a blanket grant.
                    WideOperand::parse(s).map(|v| {
                        WideSource::Read(CalendarValue::Additional(AdditionalValue::Double(v)))
                    })
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        if matches!(op, WideOp::Bcd | WideOp::Bin) && sources[0].value().constant().is_some() {
            return Err("DBCD/DBIN kaynağı Kn grubu, T/C/D veya E olmalı".into());
        }
        if matches!(op, WideOp::MultiplyTruncated | WideOp::DivideQuotient)
            && (sources.iter().any(|source| {
                matches!(
                    source.value(),
                    CalendarValue::Index {
                        high_bank: true,
                        ..
                    }
                )
            }) || matches!(&dest, WideDestination::Numeric(value) if matches!(value.value(), CalendarValue::Index { high_bank: true, .. })))
        {
            return Err("MUL32/DIV32: F operandı desteklenmez".into());
        }
        let count = if op == WideOp::Fill {
            args[2]
                .strip_prefix('K')
                .and_then(|n| n.parse::<u16>().ok())
                .or_else(|| {
                    args[2]
                        .strip_prefix('H')
                        .and_then(|n| u16::from_str_radix(n, 16).ok())
                })
                .filter(|n| (1..=512).contains(n))
                .ok_or("DFMOV sayısı K/H1..512 olmalı")?
        } else {
            1
        };
        let result = Self {
            op,
            sources,
            dest,
            count,
        };
        result.destination_devices()?;
        Ok(result)
    }
    pub fn destination_devices(&self) -> Result<Vec<Device>, String> {
        if let WideDestination::Product(value) = &self.dest {
            let mut devices = BTreeSet::new();
            value.devices(&mut devices);
            return Ok(devices.into_iter().collect());
        }
        if let WideDestination::Block(value) = &self.dest {
            return value.devices(self.count);
        }
        if let WideDestination::Numeric(value) = &self.dest {
            let mut devices = BTreeSet::new();
            value.devices(&mut devices);
            return Ok(devices.into_iter().collect());
        }
        let dest = self.dest();
        if matches!(self.op, WideOp::Compare | WideOp::Zone) {
            return Ok(comparison_destinations(dest)?.to_vec());
        }
        let slots = match self.op {
            WideOp::Multiply | WideOp::Divide => 2,
            WideOp::Fill => self.count,
            _ => 1,
        };
        let mut out = Vec::new();
        for n in 0..slots {
            out.extend(double_devices(slot(dest, n)?)?);
        }
        Ok(out)
    }
    pub fn dest(&self) -> Device {
        self.dest.base()
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        match self.op {
            WideOp::MultiplyTruncated | WideOp::DivideQuotient => {
                out.extend((1020..=1022).map(Memory::system_device));
                Memory::error_devices(out);
            }
            WideOp::Add | WideOp::Sub => out.extend((1020..=1022).map(Memory::system_device)),
            WideOp::Multiply => {
                out.insert(Memory::system_device(1022));
            }
            WideOp::Divide | WideOp::Bcd | WideOp::Bin => Memory::error_devices(out),
            WideOp::Exchange => {
                out.insert(Memory::system_device(1303));
            }
            _ => {}
        }
        out.extend(
            self.destination_devices()
                .expect("validated 32 bit destination"),
        );
        for source in &self.sources {
            source.devices(out);
        }
    }
    pub fn label(&self) -> String {
        let mut s = self.op.name().to_string();
        for arg in &self.sources {
            s.push_str(&format!(" {arg}"));
        }
        s.push_str(&format!(" {}", self.dest));
        if self.op == WideOp::Fill {
            s.push_str(&format!(" K{}", self.count));
        }
        s
    }
    pub fn execute(&self, m: &mut Memory) {
        let dest = self.dest();
        if self.op == WideOp::Fill
            && let WideDestination::Block(target) = &self.dest
            && let Some(kind) = target.numeric_kind()
        {
            // DFMOV stores the destination kind in the HIGH byte; FMOV uses
            // the LOW byte. Preserve the other half of shared SORT scratch.
            m.native_numeric_scratch = m
                .native_numeric_scratch
                .map(|old| ((old as u16 & 0xff) | (u16::from(kind) << 8)) as i16);
        }
        // Snapshot every input before any writes, including aliases.
        let values: Vec<i32> = self.sources.iter().map(|s| s.read(m)).collect();
        let a = values.first().copied().unwrap_or_else(|| self.dest.read(m));
        let b = values.get(1).copied().unwrap_or(0);
        let result = match self.op {
            WideOp::Move => Some(a),
            WideOp::Complement => Some(!a),
            WideOp::Add => {
                let result = add(a, b);
                m.arithmetic_flags(a, b, a.wrapping_add(b), false, result);
                Some(result)
            }
            WideOp::Sub => {
                let result = sub(a, b);
                m.arithmetic_flags(a, b, a.wrapping_sub(b), true, result);
                Some(result)
            }
            WideOp::And => Some(a & b),
            WideOp::Or => Some(a | b),
            WideOp::Xor => Some(a ^ b),
            WideOp::Inc => Some(a.wrapping_add(1)),
            WideOp::Dec => Some(a.wrapping_sub(1)),
            WideOp::Neg => Some(a.wrapping_neg()),
            WideOp::Bcd => {
                if (0..=99_999_999).contains(&a) {
                    let mut n = a as u32;
                    let mut packed = 0u32;
                    for shift in 0..8 {
                        packed |= (n % 10) << (4 * shift);
                        n /= 10;
                    }
                    Some(packed as i32)
                } else {
                    m.operation_error(0xe18, 1);
                    None
                }
            }
            WideOp::Bin => {
                let mut n = 0u32;
                let mut valid = true;
                for shift in (0..8).rev() {
                    let digit = ((a as u32) >> (4 * shift)) & 15;
                    if digit > 9 {
                        valid = false;
                        break;
                    }
                    n = n * 10 + digit;
                }
                if !valid {
                    m.operation_error(0xe18, 1);
                }
                valid.then_some(n as i32)
            }
            WideOp::MultiplyTruncated => {
                let product = i64::from(a) * i64::from(b);
                // MUL32 updates carry even when an operation error is latched;
                // numeric MUL16 instead gates setting carry on M1067/68.
                m.set_system(1022, false);
                self.dest.write(m, product as i32);
                if i32::try_from(product).is_err() {
                    m.set_system(1022, true);
                }
                None
            }
            WideOp::DivideQuotient => {
                if b == 0 {
                    m.operation_error(0xe19, 9);
                    None
                } else {
                    // Widen before division so MIN/-1 remains deterministic
                    // locally; native equivalence of that edge needs a probe.
                    Some((i64::from(a) / i64::from(b)) as i32)
                }
            }
            WideOp::Multiply => {
                let product = i64::from(a) * i64::from(b);
                // Unlike MUL16, this native DMUL handler preserves M1022.
                let WideDestination::Product(target) = &self.dest else {
                    unreachable!("validated full multiplication target");
                };
                target.write_pair(m, [product as i32, (product >> 32) as i32]);
                None
            }
            WideOp::Divide => {
                if b != 0 {
                    let WideDestination::Product(target) = &self.dest else {
                        unreachable!("validated full division target");
                    };
                    target.write_pair(
                        m,
                        [
                            (i64::from(a) / i64::from(b)) as i32,
                            (i64::from(a) % i64::from(b)) as i32,
                        ],
                    );
                } else {
                    m.operation_error(0xe19, 9);
                }
                None
            }
            WideOp::Exchange => {
                let WideSource::Writable(source) = &self.sources[0] else {
                    unreachable!("validated writable DXCH source");
                };
                let old = self.dest.read(m);
                let same = matches!(&self.dest, WideDestination::Numeric(target) if source.same_exchange_address(target));
                if same && m.system_bit(1303) {
                    self.dest.write(m, (old as u32).rotate_left(16) as i32);
                } else {
                    // Both snapshots precede either write; native stores the
                    // second operand first, then the first operand.
                    self.dest.write(m, a);
                    source.write(m, old);
                }
                None
            }
            WideOp::Fill => {
                if let WideDestination::Block(target) = &self.dest {
                    for n in 0..self.count {
                        target.element(n).expect("validated fill range").write(m, a);
                    }
                }
                None
            }
            WideOp::Compare | WideOp::Zone => {
                let states = if self.op == WideOp::Compare {
                    [a > b, a == b, a < b]
                } else {
                    let high = b.max(a);
                    let v = values[2];
                    [v < a, v >= a && v <= high, v > high]
                };
                for (d, state) in comparison_destinations(dest)
                    .unwrap()
                    .into_iter()
                    .zip(states)
                {
                    m.set_internal(d, state);
                }
                None
            }
        };
        if let Some(value) = result {
            // Flags above describe the full 32-bit result. Packed destinations
            // truncate only the stored value and cannot change those flags.
            self.dest.write(m, value);
        }
    }
    pub fn display_value(&self, m: &Memory) -> String {
        let dest = self.dest();
        match self.op {
            WideOp::MultiplyTruncated => {
                format!("{} C:{}", self.dest.read(m), u8::from(m.system_bit(1022)))
            }
            WideOp::DivideQuotient => format!("Q:{}", self.dest.read(m)),
            WideOp::Compare | WideOp::Zone => {
                let bits = comparison_destinations(dest).unwrap();
                let (a, b, c) = if self.op == WideOp::Compare {
                    (">", "=", "<")
                } else {
                    ("<", "IN", ">")
                };
                format!(
                    "{a}{} {b}{} {c}{}",
                    u8::from(m.bit(bits[0])),
                    u8::from(m.bit(bits[1])),
                    u8::from(m.bit(bits[2]))
                )
            }
            WideOp::Multiply => {
                let WideDestination::Product(target) = &self.dest else {
                    unreachable!("validated full multiplication target");
                };
                let pair = target.read_pair(m);
                if matches!(
                    target.value(),
                    CalendarValue::Additional(AdditionalValue::Group { .. })
                ) {
                    format!("L:{} H:{}", pair[0], pair[1])
                } else {
                    let value = i64::from(pair[0] as u32) | (i64::from(pair[1]) << 32);
                    value.to_string()
                }
            }
            WideOp::Divide => {
                let WideDestination::Product(target) = &self.dest else {
                    unreachable!("validated full division target");
                };
                let pair = target.read_pair(m);
                format!("Q:{} R:{}", pair[0], pair[1])
            }
            _ => self.dest.read(m).to_string(),
        }
    }
}
// Exact native ES2 sign-conditioned overflow correction; not conventional
// signed wrap. INC/DEC instead use full two's-complement wrap.
fn add(a: i32, b: i32) -> i32 {
    let value = a.wrapping_add(b);
    if a > 0 && b > 0 && value < 0 {
        (value as u32 & 0x7fff_ffff) as i32
    } else if a < 0 && b < 0 && value >= 0 {
        value.wrapping_add(0x8000_0001u32 as i32)
    } else {
        value
    }
}
fn sub(a: i32, b: i32) -> i32 {
    let value = a.wrapping_sub(b);
    if a > 0 && b < 0 && value < 0 {
        (value as u32 & 0x7fff_ffff) as i32
    } else if a < 0 && b > 0 && value >= 0 {
        value.wrapping_add(0x8000_0001u32 as i32)
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plc::{Simulator, compile};
    fn d(s: &str) -> Device {
        s.parse().unwrap()
    }
    #[test]
    fn truncated_product_carry_ignores_error_latches_and_preserves_tail() {
        let p = compile("LD X0\nMUL32 D0 D2 D0\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.memory.set_system(1067, true);
        s.memory.set_system(1068, true);
        s.memory.set_system(1020, true);
        s.memory.set_system(1021, true);
        for (a, b, expected, carry) in [
            (i32::MAX, 1, i32::MAX, false),
            (i32::MIN, 1, i32::MIN, false),
            (i32::MAX, 2, -2, true),
            (i32::MIN, -1, i32::MIN, true),
            (i32::MIN, 2, 0, true),
        ] {
            s.memory.set_double_word(d("D0"), a);
            s.memory.set_double_word(d("D2"), b);
            s.scan(&p, 100);
            assert_eq!(s.memory.double_word(d("D0")), expected);
            assert_eq!(s.memory.double_word(d("D2")), b, "adjacent source/sentinel");
            assert_eq!(s.memory.system_bit(1022), carry);
            assert!(s.memory.system_bit(1020));
            assert!(s.memory.system_bit(1021));
        }
    }
    #[test]
    fn quotient_only_double_division_snapshots_aliases_and_holds_on_zero() {
        let p = compile("LD X0\nDIV32 D0 D2 D0\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.memory.set_double_word(d("D0"), -123456789);
        s.memory.set_double_word(d("D2"), 7);
        for flag in 1020..=1022 {
            s.memory.set_system(flag, true);
        }
        s.scan(&p, 100);
        assert_eq!(s.memory.double_word(d("D0")), -17636684);
        assert_eq!(s.memory.double_word(d("D2")), 7);
        assert!((1020..=1022).all(|flag| s.memory.system_bit(flag)));
        s.memory.set_double_word(d("D2"), 0);
        s.scan(&p, 100);
        assert_eq!(s.memory.double_word(d("D0")), -17636684);
        assert_eq!(s.memory.word(d("D1067")), 0xe19);
        assert_eq!(
            s.memory.word(d("D1068")),
            (p.native_action_pc[0] + 9) as i16
        );
    }
    #[test]
    fn truncated_double_pulses_use_one_result_slot_and_model_boundaries() {
        let p = compile("LD X0\nMUL32P D0 K2 D0\nDIV32P D2 K3 D2\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_double_word(d("D0"), 40000);
        s.memory.set_double_word(d("D2"), 81000);
        for (input, product, quotient) in [
            (true, 80000, 27000),
            (true, 80000, 27000),
            (false, 80000, 27000),
            (true, 160000, 9000),
        ] {
            s.memory.set_input(d("X0"), input);
            s.scan(&p, 100);
            assert_eq!(s.memory.double_word(d("D0")), product);
            assert_eq!(s.memory.double_word(d("D2")), quotient);
        }
        for name in ["MUL32", "DIV32"] {
            for dest in ["D9998", "T254", "C198", "C199", "C255"] {
                assert!(compile(&format!("LD X0\n{name} K7 K2 {dest}\nEND")).is_ok());
            }
            for dest in ["D9999", "T255", "C256"] {
                assert!(compile(&format!("LD X0\n{name} K7 K2 {dest}\nEND")).is_err());
            }
        }
    }
    #[test]
    fn constants_pairs_and_native_counter_storage_are_distinct() {
        for (text, value) in [
            ("K-2147483648", i32::MIN),
            ("K2147483647", i32::MAX),
            ("HFFFFFFFF", -1),
            ("H80000000", i32::MIN),
        ] {
            assert_eq!(
                WideOperand::parse(text).unwrap().read(&Memory::default()),
                value
            );
        }
        for text in [
            "K2147483648",
            "K-2147483649",
            "H100000000",
            "D9999",
            "T255",
            "C256",
            "X0",
        ] {
            assert!(WideOperand::parse(text).is_err(), "{text}");
        }
        let mut m = Memory::default();
        m.set_double_word(d("D0"), 0x12345678);
        m.set_double_word(d("C200"), i32::MIN);
        assert_eq!(m.word(d("D0")) as u16, 0x5678);
        assert_eq!(m.word(d("D1")) as u16, 0x1234);
        assert_eq!(m.double_word(d("C200")), i32::MIN);
        assert_eq!(m.double_word(d("C201")), 0);
    }
    #[test]
    fn signed_overflow_vectors_match_original_and_unary_wrap_is_different() {
        assert_eq!(add(i32::MAX, 1), 0);
        assert_eq!(add(i32::MIN, -1), 0);
        assert_eq!(add(i32::MIN, i32::MIN), i32::MIN + 1);
        assert_eq!(sub(i32::MIN, 1), 0);
        assert_eq!(sub(i32::MAX, -1), 0);
        assert_eq!(sub(0, i32::MIN), i32::MIN);
        let p = compile("LD X0\nDINC D0\nDDEC D2\nDNEG D4\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.memory.set_double_word(d("D0"), i32::MAX);
        s.memory.set_double_word(d("D2"), i32::MIN);
        s.memory.set_double_word(d("D4"), i32::MIN);
        s.scan(&p, 100);
        assert_eq!(s.memory.double_word(d("D0")), i32::MIN);
        assert_eq!(s.memory.double_word(d("D2")), i32::MAX);
        assert_eq!(s.memory.double_word(d("D4")), i32::MIN);
    }
    #[test]
    fn multiply_divide_alias_read_before_write_and_zero_retention() {
        let p = compile("LD X0\nDMUL D0 D2 D0\nDDIV D10 D12 D10\nDDIV K1 K0 D20\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        for (name, value) in [
            ("D0", i32::MIN),
            ("D2", i32::MIN),
            ("D10", -123456789),
            ("D12", 7),
            ("D20", 77),
            ("D22", -88),
        ] {
            s.memory.set_double_word(d(name), value);
        }
        s.scan(&p, 100);
        let product = i64::from(s.memory.double_word(d("D0")) as u32)
            | (i64::from(s.memory.double_word(d("D2"))) << 32);
        assert_eq!(product, 4611686018427387904);
        assert_eq!(s.memory.double_word(d("D10")), -17636684);
        assert_eq!(s.memory.double_word(d("D12")), -1);
        assert_eq!(s.memory.double_word(d("D20")), 77);
        assert_eq!(s.memory.double_word(d("D22")), -88);
    }
    #[test]
    fn wide_pulse_occurrences_and_rearm_track_full_result() {
        let p = compile("LD X0\nDMOVP D0 D10\nLD X1\nDMOVP D0 D20\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_double_word(d("D0"), 12345678);
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        s.memory.set_double_word(d("D0"), -12345678);
        s.memory.set_input(d("X1"), true);
        s.scan(&p, 100);
        assert_eq!(s.memory.double_word(d("D10")), 12345678);
        assert_eq!(s.memory.double_word(d("D20")), -12345678);
        s.memory.set_input(d("X0"), false);
        s.scan(&p, 100);
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert_eq!(s.memory.double_word(d("D10")), -12345678);
    }
    #[test]
    fn full_destinations_and_operand_arity_are_validated() {
        let p =
            compile("LD X0\nDMUL D0 C200 D9996\nDCMP T254 C255 Y375\nDZCP K0 K1 C200 M4093\nEND")
                .unwrap();
        for name in [
            "D0", "D1", "C200", "D9996", "D9997", "D9998", "D9999", "T254", "T255", "C255", "Y375",
            "Y376", "Y377", "M4093", "M4094", "M4095",
        ] {
            assert!(p.devices.contains(&d(name)), "{name}");
        }
        for body in [
            "DMUL K1 K2 D9997",
            "DDIV K1 K2 C255",
            "DFMOV K1 T255 K1",
            "DFMOV K1 D9999 K1",
            "DBCD K1 D0",
            "DAND K1 K2 D9999",
            "DMOV K1",
            "DFMOV K1 D0 D2",
            "DZCP K0 K1 K2 M998",
            "DLD=P K1 K2",
        ] {
            let source = if body.starts_with("DLD") {
                format!("{body}\nOUT Y0\nEND")
            } else {
                format!("LD X0\n{body}\nEND")
            };
            assert!(compile(&source).is_err(), "{body}");
        }
        assert!(WideAction::parse("DMOV", &[]).is_err());
    }
    #[test]
    fn fill_stride_conversion_and_packed_negative_bitpattern() {
        let p =
            compile("LD X0\nDFMOV K99999999 D0 K3\nDBCD D0 D10\nDBIN D10 D12\nDXCH D0 D20\nEND")
                .unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert_eq!(s.memory.double_word(d("D2")), 99999999);
        assert_eq!(s.memory.double_word(d("D4")), 99999999);
        assert_eq!(s.memory.double_word(d("D10")) as u32, 0x99999999);
        assert_eq!(s.memory.double_word(d("D12")), 99999999);
        assert_eq!(s.memory.double_word(d("D20")), 99999999);
        let bad = compile("LD X0\nDBCD D10 D12\nDBIN D20 D22\nEND").unwrap();
        s.memory.set_double_word(d("D20"), 0xa1234567u32 as i32);
        s.memory.set_double_word(d("D22"), 77);
        s.scan(&bad, 100);
        assert_eq!(s.memory.double_word(d("D12")), 99999999);
        assert_eq!(s.memory.double_word(d("D22")), 77);
    }
    #[test]
    fn signed_comparison_and_reversed_zone_use_full_32_bits() {
        let p=compile("DLD> C200 K65535\nOUT Y0\nLD X0\nDCMP H80000000 H7FFFFFFF M0\nDZCP K100000 K-100000 C200 M10\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.memory.set_double_word(d("C200"), 100000);
        s.scan(&p, 100);
        assert!(s.memory.bit(d("Y0")));
        assert!(s.memory.bit(d("M2")));
        assert!(s.memory.bit(d("M11")));
        s.memory.set_double_word(d("C200"), 100001);
        s.scan(&p, 100);
        assert!(s.memory.bit(d("M12")));
        assert!(!s.memory.bit(d("M11")));
    }
}
