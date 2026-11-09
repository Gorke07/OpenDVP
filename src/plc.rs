//! A deliberately bounded offline instruction language, not a Delta bytecode encoder.
#[path = "scan_action.rs"]
mod scan_action;
#[path = "scan_vm.rs"]
mod scan_vm;
#[cfg(test)]
#[path = "sfc_integration_tests.rs"]
mod sfc_integration_tests;
#[cfg(test)]
#[path = "special_timer_integration_tests.rs"]
mod special_timer_integration_tests;
use crate::additional_processing::AdditionalAction;
use crate::bit_tools::BitAction;
use crate::block_operand::{BlockDestination, BlockSource};
use crate::calendar_operand::{CalendarDestination, CalendarValue};
use crate::data_processing::DataAction;
use crate::data_transform::TransformAction;
use crate::drum::DrumAction;
use crate::file_memory::FileAction;
use crate::float_ops::{FloatAction, FloatPredicate};
use crate::gps::GpsAction;
use crate::hour_meter::HourAction;
use crate::matrix_ops::MatrixAction;
use crate::multiply_destination::MultiplyDestination;
use crate::numeric_contacts::NumericPredicate;
use crate::ramp_sort::RampSortAction;
use crate::random_ops::RandomAction;
use crate::rtc_clock::RtcAction;
use crate::search_ops::SearchAction;
use crate::shift::ShiftAction;
use crate::solar_position::SolarAction;
use crate::special_timers::TimerAction;
use crate::text_encoding::TextAction;
use crate::time_ops::TimeAction;
use crate::timing_io::TimingAction;
use crate::wide::WideAction;
pub use scan_vm::ScanStatus;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Area {
    X,
    Y,
    M,
    S,
    T,
    C,
    D,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Device {
    pub area: Area,
    pub index: u16,
}

impl Device {
    pub fn bit(self) -> bool {
        self.area != Area::D
    }
    pub fn writable_bit(self) -> bool {
        matches!(self.area, Area::Y | Area::M | Area::S)
            && !(self.area == Area::M
                && ((1000..=1003).contains(&self.index) || (1011..=1014).contains(&self.index)))
    }
}

impl FromStr for Device {
    type Err = String;
    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let raw = raw.trim().to_ascii_uppercase();
        let (area, digits) = raw.split_at_checked(1).ok_or("Adres boş olamaz")?;
        let area = match area {
            "X" => Area::X,
            "Y" => Area::Y,
            "M" => Area::M,
            "S" => Area::S,
            "T" => Area::T,
            "C" => Area::C,
            "D" => Area::D,
            _ => return Err(format!("Bilinmeyen adres: {raw}")),
        };
        let radix = if matches!(area, Area::X | Area::Y) {
            8
        } else {
            10
        };
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!("Geçersiz adres: {raw}"));
        }
        let index = u16::from_str_radix(digits, radix)
            .map_err(|_| format!("Geçersiz adres: {raw} (X/Y sekizliktir)"))?;
        let max = match area {
            Area::X | Area::Y => 255,
            Area::M => 4095,
            Area::S => 1023,
            Area::T | Area::C => 255,
            Area::D => 9999,
        };
        if index > max {
            return Err(format!("ES2 adres aralığı dışında: {raw}"));
        }
        if area == Area::M
            && (1000..1536).contains(&index)
            && !matches!(
                index,
                1000..=1003 | 1008 | 1011..=1022 | 1026..=1030 | 1034 | 1038..=1057 | 1059 | 1067 | 1068 | 1070..=1072 | 1078 | 1079 | 1081 | 1082 | 1088..=1113 | 1116 | 1117 | 1119..=1131 | 1133 | 1134 | 1136 | 1138..=1143 | 1148 | 1156..=1159 | 1161 | 1162 | 1167..=1169 | 1177..=1179 | 1183 | 1184 | 1190..=1193 | 1196..=1198 | 1200..=1254 | 1257 | 1260 | 1262 | 1263 | 1270..=1277 | 1280 | 1284 | 1286 | 1303 | 1305..=1308 | 1310..=1320 | 1321 | 1322 | 1334 | 1335 | 1346..=1348 | 1520 | 1521 | 1524..=1527 | 1530..=1535
            )
        {
            return Err(format!(
                "{raw}: özel sistem röleleri bu sürümde desteklenmiyor"
            ));
        }
        Ok(Self { area, index })
    }
}

impl fmt::Display for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if matches!(self.area, Area::X | Area::Y) {
            write!(f, "{:?}{:o}", self.area, self.index)
        } else {
            write!(f, "{:?}{}", self.area, self.index)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    Constant(i16),
    Register(Device),
}
impl Operand {
    pub(crate) fn parse(s: &str) -> Result<Self, String> {
        let s = s.to_ascii_uppercase();
        if let Some(n) = s.strip_prefix('K') {
            return n
                .parse::<i16>()
                .map(Self::Constant)
                .map_err(|_| format!("16 bit sabit geçersiz: {s}"));
        }
        if let Some(n) = s.strip_prefix('H') {
            return u16::from_str_radix(n, 16)
                .map(|v| Self::Constant(v as i16))
                .map_err(|_| format!("16 bit hex sabit geçersiz: {s}"));
        }
        let d: Device = s.parse()?;
        if !matches!(d.area, Area::D | Area::T | Area::C) {
            return Err("Sayı operandı D, T, C, K veya H olmalı".into());
        }
        if d.area == Area::C && d.index >= 200 {
            return Err(
                "C200 ve üstü 32 bit sayaçtır; 16 bit operand olarak desteklenmiyor".into(),
            );
        }
        Ok(Self::Register(d))
    }
    pub(crate) fn read(&self, memory: &Memory) -> i16 {
        match self {
            Self::Constant(v) => *v,
            Self::Register(d) => memory.word(*d),
        }
    }
}
impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Constant(v) => write!(f, "K{v}"),
            Self::Register(d) => write!(f, "{d}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Relation {
    Eq,
    Gt,
    Lt,
    Ne,
    Le,
    Ge,
}
impl Relation {
    pub(crate) fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "=" => Self::Eq,
            ">" => Self::Gt,
            "<" => Self::Lt,
            "<>" => Self::Ne,
            "<=" => Self::Le,
            ">=" => Self::Ge,
            _ => return None,
        })
    }
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Eq => "=",
            Self::Gt => ">",
            Self::Lt => "<",
            Self::Ne => "<>",
            Self::Le => "<=",
            Self::Ge => ">=",
        }
    }
    pub fn test<T: PartialOrd>(&self, a: T, b: T) -> bool {
        match self {
            Self::Eq => a == b,
            Self::Gt => a > b,
            Self::Lt => a < b,
            Self::Ne => a != b,
            Self::Le => a <= b,
            Self::Ge => a >= b,
        }
    }
    pub fn opcode_offset(&self) -> u16 {
        match self {
            Self::Eq => 0,
            Self::Gt => 2,
            Self::Lt => 4,
            Self::Ne => 8,
            Self::Le => 10,
            Self::Ge => 12,
        }
    }
}

pub fn comparison_destinations(base: Device) -> Result<[Device; 3], String> {
    if !base.writable_bit() {
        return Err("CMP/ZCP hedefi üç ardışık Y/M/S biti olmalı".into());
    }
    let mut result = [base; 3];
    for (offset, slot) in result.iter_mut().enumerate() {
        let index = base
            .index
            .checked_add(offset as u16)
            .ok_or("CMP/ZCP hedef aralığı taşıyor")?;
        let d = Device {
            area: base.area,
            index,
        };
        if !d.writable_bit() {
            return Err(format!("CMP/ZCP sonuç biti salt okunur: {d}"));
        }
        *slot = d
            .to_string()
            .parse()
            .map_err(|e| format!("CMP/ZCP sonuç aralığı geçersiz: {e}"))?;
    }
    Ok(result)
}

pub fn paired_data_destinations(base: Device) -> Result<[Device; 2], String> {
    if base.area != Area::D {
        return Err("MUL/DIV hedefi iki ardışık D register olmalı".into());
    }
    let next = base
        .index
        .checked_add(1)
        .ok_or("MUL/DIV hedef aralığı taşıyor")?;
    let second = format!("D{next}")
        .parse::<Device>()
        .map_err(|e| format!("MUL/DIV hedef aralığı geçersiz: {e}"))?;
    Ok([base, second])
}

/// Validate a complete implicit word range before any writes occur.
pub fn word_range(base: Device, count: u16) -> Result<Vec<Device>, String> {
    let limit = match base.area {
        Area::D => 9999,
        Area::T => 255,
        Area::C => 199,
        _ => return Err("Kelime aralığı D/T/C0..199 alanında olmalı".into()),
    };
    if count == 0
        || base
            .index
            .checked_add(count - 1)
            .is_none_or(|end| end > limit)
    {
        return Err(format!(
            "Kelime aralığı sınırı aşıyor: {base}, {count} kelime"
        ));
    }
    Ok((0..count)
        .map(|offset| Device {
            area: base.area,
            index: base.index + offset,
        })
        .collect())
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    IndexedBitNumber(crate::indexed_bit_number::IndexedBitNumberPredicate),
    IndexedBit(crate::indexed_bit::IndexedBitContact),
    IndexedComparison(crate::indexed_comparison::IndexedPredicate),
    IndexedFloatComparison(crate::indexed_float_comparison::IndexedFloatPredicate),
    Float(FloatPredicate),
    Numeric(NumericPredicate),
    Invert(Box<Expr>),
    CompareWide(Relation, CalendarValue, CalendarValue),
    Compare(Relation, CalendarValue, CalendarValue),
    Saved(usize),
    Contact(Device, bool),
    Edge(Device, bool, usize),
    Trigger(Box<Expr>, bool, usize),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
}
impl Expr {
    pub fn evaluate(&self, m: &Memory) -> bool {
        self.evaluate_checked(m).unwrap_or(false)
    }
    // Preserve address failures through INV and boolean composition. Converting
    // a failed child to false early could make its inversion appear energized.
    fn evaluate_checked(&self, m: &Memory) -> Result<bool, String> {
        Ok(match self {
            Self::IndexedBitNumber(predicate) => return predicate.evaluate(m),
            Self::IndexedBit(contact) => return contact.evaluate(m),
            Self::IndexedComparison(predicate) => return predicate.evaluate(m),
            Self::IndexedFloatComparison(predicate) => return predicate.evaluate(m),
            Self::Float(predicate) => predicate.evaluate(m),
            Self::Numeric(predicate) => predicate.evaluate(m),
            Self::Invert(inner) => !inner.evaluate_checked(m)?,
            Self::CompareWide(op, a, b) => op.test(a.read(m), b.read(m)),
            Self::Compare(op, a, b) => op.test(a.read(m), b.read(m)),
            Self::Saved(id) => m.branch_values.get(id).copied().unwrap_or(false),
            Self::Contact(d, inverted) => m.bit(*d) ^ inverted,
            Self::Trigger(_, _, id) | Self::Edge(_, _, id) => {
                m.contact_pulses.get(id).copied().unwrap_or(false)
            }
            Self::And(a, b) => a.evaluate_checked(m)? && b.evaluate_checked(m)?,
            Self::Or(a, b) => a.evaluate_checked(m)? || b.evaluate_checked(m)?,
        })
    }
    fn sample(&self, m: &mut Memory) -> Result<bool, String> {
        Ok(match self {
            Self::IndexedBitNumber(predicate) => return predicate.evaluate(m),
            Self::IndexedBit(contact) => return contact.sample(m),
            Self::IndexedComparison(predicate) => return predicate.evaluate(m),
            Self::IndexedFloatComparison(predicate) => return predicate.evaluate(m),
            Self::Float(predicate) => predicate.evaluate(m),
            Self::Numeric(predicate) => predicate.evaluate(m),
            Self::Invert(inner) => !inner.sample(m)?,
            Self::CompareWide(op, a, b) => op.test(a.read(m), b.read(m)),
            Self::Compare(op, a, b) => op.test(a.read(m), b.read(m)),
            Self::Saved(id) => m.branch_values.get(id).copied().unwrap_or(false),
            Self::Contact(d, inverted) => m.bit(*d) ^ inverted,
            Self::Edge(d, rising, id) => {
                let now = m.bit(*d);
                m.sample_edge(*id, *rising, now)
            }
            Self::Trigger(inner, rising, id) => {
                let now = inner.sample(m)?;
                m.sample_edge(*id, *rising, now)
            }
            Self::And(a, b) | Self::Or(a, b) => {
                // Both operands execute even when the left value determines
                // the result: otherwise a gated edge would be delayed.
                let left = a.sample(m)?;
                let right = b.sample(m)?;
                if matches!(self, Self::And(..)) {
                    left && right
                } else {
                    left || right
                }
            }
        })
    }
    fn has_indexed_operands(&self) -> bool {
        match self {
            Self::IndexedBitNumber(_)
            | Self::IndexedComparison(_)
            | Self::IndexedBit(_)
            | Self::IndexedFloatComparison(_) => true,
            Self::Invert(inner) | Self::Trigger(inner, ..) => inner.has_indexed_operands(),
            Self::And(a, b) | Self::Or(a, b) => {
                a.has_indexed_operands() || b.has_indexed_operands()
            }
            _ => false,
        }
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        match self {
            Self::IndexedBitNumber(predicate) => predicate.devices(out),
            Self::IndexedBit(contact) => contact.devices(out),
            Self::IndexedComparison(predicate) => predicate.devices(out),
            Self::IndexedFloatComparison(predicate) => predicate.devices(out),
            Self::Float(predicate) => predicate.devices(out),
            Self::Numeric(predicate) => predicate.devices(out),
            Self::Invert(inner) => inner.devices(out),
            Self::Compare(_, a, b) | Self::CompareWide(_, a, b) => {
                a.devices(out);
                b.devices(out);
            }
            Self::Saved(_) => {}
            Self::Contact(d, _) | Self::Edge(d, ..) => {
                out.insert(*d);
            }
            Self::Trigger(inner, _, _) => inner.devices(out),
            Self::And(a, b) | Self::Or(a, b) => {
                a.devices(out);
                b.devices(out);
            }
        }
    }
    pub fn contacts(&self) -> usize {
        match self {
            Self::Invert(inner) => inner.contacts(),
            Self::Compare(..)
            | Self::CompareWide(..)
            | Self::Numeric(..)
            | Self::Float(..)
            | Self::IndexedComparison(..)
            | Self::IndexedFloatComparison(..)
            | Self::IndexedBitNumber(..) => 1,
            Self::Saved(_) => 0,
            Self::Contact(..) | Self::Edge(..) | Self::IndexedBit(..) => 1,
            Self::Trigger(inner, ..) => inner.contacts(),
            Self::And(a, b) | Self::Or(a, b) => a.contacts() + b.contacts(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WordUnary {
    Inc,
    Dec,
    Neg,
}
impl WordUnary {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Inc => "INC",
            Self::Dec => "DEC",
            Self::Neg => "NEG",
        }
    }
    fn apply(&self, value: i16) -> i16 {
        match self {
            Self::Inc => value.wrapping_add(1),
            Self::Dec => value.wrapping_sub(1),
            Self::Neg => value.wrapping_neg(),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum WordLogic {
    And,
    Or,
    Xor,
}
impl WordLogic {
    pub fn name(&self) -> &'static str {
        match self {
            Self::And => "WAND",
            Self::Or => "WOR",
            Self::Xor => "WXOR",
        }
    }
    fn apply(&self, a: i16, b: i16) -> i16 {
        match self {
            Self::And => a & b,
            Self::Or => a | b,
            Self::Xor => a ^ b,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum WordConversion {
    Bcd,
    Bin,
}
impl WordConversion {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Bcd => "BCD",
            Self::Bin => "BIN",
        }
    }
    pub(crate) fn apply(&self, value: i16) -> Option<i16> {
        match self {
            Self::Bcd if (0..=9999).contains(&value) => {
                let value = value as u16;
                Some(
                    (((value / 1000) << 12)
                        | (((value / 100) % 10) << 8)
                        | (((value / 10) % 10) << 4)
                        | (value % 10)) as i16,
                )
            }
            Self::Bin => {
                let value = value as u16;
                let mut result = 0;
                for shift in (0..4).rev() {
                    let digit = (value >> (shift * 4)) & 15;
                    if digit > 9 {
                        return None;
                    }
                    result = result * 10 + digit;
                }
                Some(result as i16)
            }
            _ => None,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    IndexedScalar(crate::indexed_scalar::IndexedScalarAction),
    IndexedMultiply(crate::indexed_multiply::IndexedMultiplyAction),
    IndexedFloat(crate::indexed_float::IndexedFloatAction),
    IndexedRoot(crate::indexed_root::IndexedRootAction),
    IndexedAngle(crate::indexed_angle::IndexedAngleAction),
    IndexedDecimalConversion(crate::indexed_decimal_conversion::IndexedDecimalConversionAction),
    IndexedExpLn(crate::indexed_exp_ln::IndexedExpLnAction),
    IndexedLogPow(crate::indexed_log_pow::IndexedLogPowAction),
    IndexedDesqr(crate::indexed_desqr::IndexedDesqrAction),
    IndexedTrig(crate::indexed_trig::IndexedTrigAction),
    IndexedFloatComparison(crate::indexed_float_comparison::IndexedFloatComparisonAction),
    IndexedLiteralMove(crate::indexed_literal_move::IndexedLiteralMoveAction),
    IndexedSwapMask(crate::indexed_swap_mask::IndexedSwapMaskAction),
    IndexedRandom(crate::indexed_random::IndexedRandomAction),
    IndexedBitNumber(crate::indexed_bit_number::IndexedBitNumberAction),
    IndexedAbsGray(crate::indexed_abs_gray::IndexedAbsGrayAction),
    IndexedSearchToggle(crate::indexed_search_toggle::IndexedSearchToggleAction),
    IndexedCalendarTime(crate::indexed_calendar_time::IndexedCalendarTimeAction),
    IndexedRtc(crate::indexed_rtc::IndexedRtcAction),
    IndexedSegment(crate::indexed_segment::IndexedSegmentAction),
    IndexedAsc(crate::indexed_asc::IndexedAscAction),
    IndexedChecksum(crate::indexed_checksum::IndexedChecksumAction),
    IndexedText(crate::indexed_text::IndexedTextAction),
    IndexedAdditional(crate::indexed_additional::IndexedAdditionalAction),
    IndexedDataProcessing(crate::indexed_data_processing::IndexedDataProcessingAction),
    IndexedShiftFifo(crate::indexed_shift_fifo::IndexedShiftFifoAction),
    IndexedBlockExchange(crate::indexed_block_exchange::IndexedBlockExchangeAction),
    IndexedRotation(crate::indexed_rotation::IndexedRotationAction),
    IndexedRealBinary(crate::indexed_real_binary::IndexedRealBinaryAction),
    IndexedFloatBinary(crate::indexed_float_binary::IndexedFloatBinaryAction),
    IndexedBcd(crate::indexed_bcd::IndexedBcdAction),
    IndexedTruncated(crate::indexed_truncated::IndexedTruncatedAction),
    IndexedComparison(crate::indexed_comparison::IndexedComparisonAction),
    IndexedBit(crate::indexed_bit::IndexedBitAction),
    WordCounter(crate::word_counter::WordCounterAction),
    NumericReset(crate::word_counter::NumericResetAction),
    ModuleIo(crate::module_io::ModuleIoAction),
    SerialIo(crate::serial_io::SerialIoAction),
    ParallelRun(crate::parallel_run::ParallelRunAction),
    AnalogKnob(crate::analog_knob::KnobAction),
    Drive(crate::drive_comm::DriveAction),
    Servo(crate::servo_comm::ServoAction),
    Dmv(crate::dmv_comm::DmvAction),
    Ethernet(crate::ethernet::EthernetAction),
    RawEthernet(crate::ethernet_raw::RawEthernetAction),
    Canopen(crate::canopen::CanopenAction),
    CanExchange(crate::canopen_exchange::ExchangeAction),
    CoordinatedMotion(crate::coordinated_motion::CoordinatedAction),
    Pu(crate::pu_module::PuAction),
    ExtendedMotion(crate::motion_extended::ExtendedMotionAction),
    ExternalIo(crate::external_io::ExternalIoAction),
    MotionBasic(crate::motion_basic::MotionAction),
    PulseOutput(crate::pulse_output::PulseAction),
    Refresh(crate::refresh_io::RefreshAction),
    HighSpeed(crate::high_speed::HighSpeedAction),
    DoubleCounter(crate::counter::CounterAction),
    Pid(crate::pid::PidAction),
    Ftc(crate::ftc::FtcAction),
    EthernetConfig(crate::ethernet_config::EthernetConfigAction),
    Station(crate::station_compare::StationAction),
    StringMove(crate::string_move::StringMoveAction),
    Sfc(crate::sfc::SfcAction),
    Alarm(crate::alarm::AlarmAction),
    SpecialTimer(TimerAction),
    FileMemory(FileAction),
    Drum(DrumAction),
    Matrix(MatrixAction),
    Hour(HourAction),
    Rtc(RtcAction),
    Time(TimeAction),
    Timing(TimingAction),
    Gps(GpsAction),
    Solar(SolarAction),
    Search(SearchAction),
    RampSort(RampSortAction),
    Transform(TransformAction),
    Additional(AdditionalAction),
    Random(RandomAction),
    Float(FloatAction),
    Bits(BitAction),
    Text(TextAction),
    Data(DataAction),
    Shift(ShiftAction),
    EdgeOut(Device, bool),
    Wide(WideAction),
    Convert(WordConversion, CalendarValue, CalendarDestination),
    BlockMove(BlockSource, BlockDestination, Operand),
    Fill(CalendarValue, BlockDestination, u16),
    Exchange(CalendarDestination, CalendarDestination),
    ZoneCompare(CalendarValue, CalendarValue, CalendarValue, Device),
    Multiply(CalendarValue, CalendarValue, MultiplyDestination),
    Divide(CalendarValue, CalendarValue, MultiplyDestination),
    MultiplyTruncated(CalendarValue, CalendarValue, CalendarDestination),
    DivideQuotient(CalendarValue, CalendarValue, CalendarDestination),
    Compare(CalendarValue, CalendarValue, Device),
    Out(Device),
    Set(Device),
    Reset(Device),
    Timer(Device, u16),
    Counter(Device, u16),
    Move(CalendarValue, CalendarDestination),
    Add(CalendarValue, CalendarValue, CalendarDestination),
    Sub(CalendarValue, CalendarValue, CalendarDestination),
    Unary(WordUnary, CalendarDestination),
    Complement(CalendarValue, CalendarDestination),
    Logic(WordLogic, CalendarValue, CalendarValue, CalendarDestination),
    Pulse(Box<Action>),
}
impl Action {
    /// Device the action writes when it is fixed at compile time; indexed forms compute
    /// their target at run time and report none.
    pub fn written_device(&self) -> Option<Device> {
        let inner = if let Action::Pulse(inner) = self {
            inner.as_ref()
        } else {
            self
        };
        let indexed = matches!(
            inner,
            Action::IndexedScalar(_)
                | Action::IndexedComparison(_)
                | Action::IndexedBit(_)
                | Action::IndexedMultiply(_)
                | Action::IndexedFloat(_)
                | Action::IndexedRoot(_)
                | Action::IndexedFloatBinary(_)
                | Action::IndexedRealBinary(_)
                | Action::IndexedRotation(_)
                | Action::IndexedBlockExchange(_)
                | Action::IndexedShiftFifo(_)
                | Action::IndexedDataProcessing(_)
                | Action::IndexedCalendarTime(_)
                | Action::IndexedRtc(_)
                | Action::IndexedSegment(_)
                | Action::IndexedAsc(_)
                | Action::IndexedChecksum(_)
                | Action::IndexedText(_)
                | Action::IndexedAdditional(_)
                | Action::IndexedSearchToggle(_)
                | Action::IndexedAbsGray(_)
                | Action::IndexedBitNumber(_)
                | Action::IndexedRandom(_)
                | Action::IndexedSwapMask(_)
                | Action::IndexedLiteralMove(_)
                | Action::IndexedFloatComparison(_)
                | Action::IndexedTrig(_)
                | Action::IndexedDesqr(_)
                | Action::IndexedLogPow(_)
                | Action::IndexedExpLn(_)
                | Action::IndexedDecimalConversion(_)
                | Action::IndexedAngle(_)
                | Action::IndexedBcd(_)
                | Action::IndexedTruncated(_)
        );
        (!indexed).then(|| self.destination())
    }
    pub fn destination(&self) -> Device {
        match self {
            Self::IndexedScalar(a) => a.dest(),
            Self::IndexedMultiply(a) => a.dest(),
            Self::IndexedFloat(a) => a.dest(),
            Self::IndexedRoot(a) => a.dest(),
            Self::IndexedAngle(a) => a.dest(),
            Self::IndexedDecimalConversion(a) => a.dest(),
            Self::IndexedExpLn(a) => a.dest(),
            Self::IndexedLogPow(a) => a.dest(),
            Self::IndexedDesqr(a) => a.dest(),
            Self::IndexedTrig(a) => a.dest(),
            Self::IndexedFloatComparison(a) => a.dest(),
            Self::IndexedLiteralMove(a) => a.dest(),
            Self::IndexedSwapMask(a) => a.dest(),
            Self::IndexedRandom(a) => a.dest(),
            Self::IndexedBitNumber(a) => a.dest(),
            Self::IndexedAbsGray(a) => a.dest(),
            Self::IndexedSearchToggle(a) => a.dest(),
            Self::IndexedCalendarTime(a) => a.dest(),
            Self::IndexedRtc(a) => a.dest(),
            Self::IndexedSegment(a) => a.dest(),
            Self::IndexedAsc(a) => a.dest(),
            Self::IndexedChecksum(a) => a.dest(),
            Self::IndexedText(a) => a.dest(),
            Self::IndexedAdditional(a) => a.dest(),
            Self::IndexedDataProcessing(a) => a.dest(),
            Self::IndexedShiftFifo(a) => a.dest(),
            Self::IndexedBlockExchange(a) => a.dest(),
            Self::IndexedRotation(a) => a.dest(),
            Self::IndexedRealBinary(a) => a.dest(),
            Self::IndexedFloatBinary(a) => a.dest(),
            Self::IndexedBcd(a) => a.dest(),
            Self::IndexedTruncated(a) => a.dest(),
            Self::IndexedComparison(a) => a.dest(),
            Self::IndexedBit(a) => a.dest(),
            Self::WordCounter(a) => a.counter,
            Self::NumericReset(a) => a.dest(),
            Self::ExternalIo(a) => a.dest(),
            Self::MotionBasic(a) => a.dest(),
            Self::ModuleIo(a) => a.dest(),
            Self::SerialIo(a) => a.dest(),
            Self::ParallelRun(a) => a.dest(),
            Self::AnalogKnob(a) => a.dest(),
            Self::Drive(a) => a.dest(),
            Self::Servo(a) => a.dest(),
            Self::Dmv(a) => a.dest(),
            Self::Ethernet(a) => a.dest(),
            Self::RawEthernet(a) => a.dest(),
            Self::Canopen(a) => a.dest(),
            Self::CanExchange(a) => a.dest(),
            Self::CoordinatedMotion(a) => a.dest(),
            Self::Pu(a) => a.dest(),
            Self::ExtendedMotion(a) => a.dest(),
            Self::PulseOutput(a) => a.dest(),
            Self::Refresh(a) => a.dest(),
            Self::HighSpeed(a) => a.dest(),
            Self::DoubleCounter(a) => a.dest(),
            Self::Pid(a) => a.dest(),
            Self::Ftc(a) => a.dest(),
            Self::EthernetConfig(a) => a.dest(),
            Self::Station(a) => a.dest(),
            Self::StringMove(a) => a.dest(),
            Self::Sfc(a) => a.dest(),
            Self::Alarm(a) => a.dest(),
            Self::SpecialTimer(a) => a.dest(),
            Self::FileMemory(a) => a.dest(),
            Self::Drum(a) => a.dest(),
            Self::Matrix(a) => a.dest(),
            Self::Hour(a) => a.dest(),
            Self::Rtc(a) => a.dest(),
            Self::Time(a) => a.dest(),
            Self::Timing(a) => a.dest(),
            Self::Gps(a) => a.dest(),
            Self::Solar(a) => a.dest(),
            Self::Search(a) => a.dest(),
            Self::RampSort(a) => a.dest(),
            Self::Transform(a) => a.dest(),
            Self::Additional(a) => a.dest(),
            Self::Random(a) => a.dest(),
            Self::Float(a) => a.dest(),
            Self::Bits(a) => a.dest(),
            Self::Text(a) => a.dest(),
            Self::Data(a) => a.dest(),
            Self::Shift(a) => a.dest(),
            Self::EdgeOut(d, _) => *d,
            Self::Wide(a) => a.dest(),
            Self::Add(_, _, d)
            | Self::Sub(_, _, d)
            | Self::Unary(_, d)
            | Self::Move(_, d)
            | Self::Complement(_, d)
            | Self::Logic(_, _, _, d)
            | Self::Convert(_, _, d)
            | Self::MultiplyTruncated(_, _, d)
            | Self::DivideQuotient(_, _, d)
            | Self::Exchange(_, d) => d.base(),
            Self::Out(d)
            | Self::Set(d)
            | Self::Reset(d)
            | Self::Timer(d, _)
            | Self::Counter(d, _) => *d,
            Self::BlockMove(_, d, _) | Self::Fill(_, d, _) => d.base(),
            Self::Multiply(_, _, d) | Self::Divide(_, _, d) => d.base(),
            Self::Compare(_, _, d) => *d,
            Self::ZoneCompare(_, _, _, d) => *d,
            Self::Pulse(action) => action.destination(),
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::IndexedScalar(a) => a.label(),
            Self::IndexedMultiply(a) => a.label(),
            Self::IndexedFloat(a) => a.label(),
            Self::IndexedRoot(a) => a.label(),
            Self::IndexedAngle(a) => a.label(),
            Self::IndexedDecimalConversion(a) => a.label(),
            Self::IndexedExpLn(a) => a.label(),
            Self::IndexedLogPow(a) => a.label(),
            Self::IndexedDesqr(a) => a.label(),
            Self::IndexedTrig(a) => a.label(),
            Self::IndexedFloatComparison(a) => a.label(),
            Self::IndexedLiteralMove(a) => a.label(),
            Self::IndexedSwapMask(a) => a.label(),
            Self::IndexedRandom(a) => a.label(),
            Self::IndexedBitNumber(a) => a.label(),
            Self::IndexedAbsGray(a) => a.label(),
            Self::IndexedSearchToggle(a) => a.label(),
            Self::IndexedCalendarTime(a) => a.label(),
            Self::IndexedRtc(a) => a.label(),
            Self::IndexedSegment(a) => a.label(),
            Self::IndexedAsc(a) => a.label(),
            Self::IndexedChecksum(a) => a.label(),
            Self::IndexedText(a) => a.label(),
            Self::IndexedAdditional(a) => a.label(),
            Self::IndexedDataProcessing(a) => a.label(),
            Self::IndexedShiftFifo(a) => a.label(),
            Self::IndexedBlockExchange(a) => a.label(),
            Self::IndexedRotation(a) => a.label(),
            Self::IndexedRealBinary(a) => a.label(),
            Self::IndexedFloatBinary(a) => a.label(),
            Self::IndexedBcd(a) => a.label(),
            Self::IndexedTruncated(a) => a.label(),
            Self::IndexedComparison(a) => a.label(),
            Self::IndexedBit(a) => a.label(),
            Self::WordCounter(a) => a.label(),
            Self::NumericReset(a) => a.label(),
            Self::ExternalIo(a) => a.label(),
            Self::MotionBasic(a) => a.label(),
            Self::ModuleIo(a) => a.label(),
            Self::SerialIo(a) => a.label(),
            Self::ParallelRun(a) => a.label(),
            Self::AnalogKnob(a) => a.label(),
            Self::Drive(a) => a.label(),
            Self::Servo(a) => a.label(),
            Self::Dmv(a) => a.label(),
            Self::Ethernet(a) => a.label(),
            Self::RawEthernet(a) => a.label(),
            Self::Canopen(a) => a.label(),
            Self::CanExchange(a) => a.label(),
            Self::CoordinatedMotion(a) => a.label(),
            Self::Pu(a) => a.label(),
            Self::ExtendedMotion(a) => a.label(),
            Self::PulseOutput(a) => a.label(),
            Self::Refresh(a) => a.label(),
            Self::HighSpeed(a) => a.label(),
            Self::DoubleCounter(a) => a.label(),
            Self::Pid(a) => a.label(),
            Self::Ftc(a) => a.label(),
            Self::EthernetConfig(a) => a.label(),
            Self::Station(a) => a.label(),
            Self::StringMove(a) => a.label(),
            Self::Sfc(a) => a.label(),
            Self::Alarm(a) => a.label(),
            Self::SpecialTimer(a) => a.label(),
            Self::FileMemory(a) => a.label(),
            Self::Drum(a) => a.label(),
            Self::Matrix(a) => a.label(),
            Self::Hour(a) => a.label(),
            Self::Rtc(a) => a.label(),
            Self::Time(a) => a.label(),
            Self::Timing(a) => a.label(),
            Self::Gps(a) => a.label(),
            Self::Solar(a) => a.label(),
            Self::Search(a) => a.label(),
            Self::RampSort(a) => a.label(),
            Self::Transform(a) => a.label(),
            Self::Additional(a) => a.label(),
            Self::Random(a) => a.label(),
            Self::Float(a) => a.label(),
            Self::Bits(a) => a.label(),
            Self::Text(a) => a.label(),
            Self::Data(a) => a.label(),
            Self::Shift(a) => a.label(),
            Self::EdgeOut(d, rising) => format!("{} {d}", if *rising { "PLS" } else { "PLF" }),
            Self::Wide(a) => a.label(),
            Self::Out(d) => format!("OUT {d}"),
            Self::Set(d) => format!("SET {d}"),
            Self::Reset(d) => format!("RST {d}"),
            Self::Timer(d, k) => format!("TMR {d} K{k}"),
            Self::Counter(d, k) => format!("CNT {d} K{k}"),
            Self::Move(v, d) => format!("MOV {v} {d}"),
            Self::Convert(op, value, dest) => format!("{} {value} {dest}", op.name()),
            Self::BlockMove(source, dest, count) => format!("BMOV {source} {dest} {count}"),
            Self::Fill(value, dest, count) => format!("FMOV {value} {dest} K{count}"),
            Self::Exchange(source, dest) => format!("XCH {source} {dest}"),
            Self::Add(a, b, d) => format!("ADD {a} {b} {d}"),
            Self::Sub(a, b, d) => format!("SUB {a} {b} {d}"),
            Self::Compare(a, b, d) => format!("CMP {a} {b} {d}"),
            Self::ZoneCompare(low, high, value, d) => format!("ZCP {low} {high} {value} {d}"),
            Self::Multiply(a, b, d) => format!("MUL {a} {b} {d}"),
            Self::Divide(a, b, d) => format!("DIV {a} {b} {d}"),
            Self::MultiplyTruncated(a, b, d) => format!("MUL16 {a} {b} {d}"),
            Self::DivideQuotient(a, b, d) => format!("DIV16 {a} {b} {d}"),
            Self::Unary(op, d) => format!("{} {d}", op.name()),
            Self::Complement(a, d) => format!("CML {a} {d}"),
            Self::Logic(op, a, b, d) => format!("{} {a} {b} {d}", op.name()),
            Self::Pulse(action) => {
                let label = action.label();
                if let Some((op, args)) = label.split_once(' ') {
                    format!("{op}P {args}")
                } else {
                    format!("{label}P")
                }
            }
        }
    }
    /// Collect statically known devices referenced or implicitly used by this
    /// action. Dynamic/indexed addresses may require runtime observation too.
    pub fn source_devices(&self, out: &mut BTreeSet<Device>) {
        match self {
            Self::IndexedScalar(a) => a.devices(out),
            Self::IndexedMultiply(a) => a.devices(out),
            Self::IndexedFloat(a) => a.devices(out),
            Self::IndexedRoot(a) => a.devices(out),
            Self::IndexedAngle(a) => a.devices(out),
            Self::IndexedDecimalConversion(a) => a.devices(out),
            Self::IndexedExpLn(a) => a.devices(out),
            Self::IndexedLogPow(a) => a.devices(out),
            Self::IndexedDesqr(a) => a.devices(out),
            Self::IndexedTrig(a) => a.devices(out),
            Self::IndexedFloatComparison(a) => a.devices(out),
            Self::IndexedLiteralMove(a) => a.devices(out),
            Self::IndexedSwapMask(a) => a.devices(out),
            Self::IndexedRandom(a) => a.devices(out),
            Self::IndexedBitNumber(a) => a.devices(out),
            Self::IndexedAbsGray(a) => a.devices(out),
            Self::IndexedSearchToggle(a) => a.devices(out),
            Self::IndexedCalendarTime(a) => a.devices(out),
            Self::IndexedRtc(a) => a.devices(out),
            Self::IndexedSegment(a) => a.devices(out),
            Self::IndexedAsc(a) => a.devices(out),
            Self::IndexedChecksum(a) => a.devices(out),
            Self::IndexedText(a) => a.devices(out),
            Self::IndexedAdditional(a) => a.devices(out),
            Self::IndexedDataProcessing(a) => a.devices(out),
            Self::IndexedShiftFifo(a) => a.devices(out),
            Self::IndexedBlockExchange(a) => a.devices(out),
            Self::IndexedRotation(a) => a.devices(out),
            Self::IndexedRealBinary(a) => a.devices(out),
            Self::IndexedFloatBinary(a) => a.devices(out),
            Self::IndexedBcd(a) => a.devices(out),
            Self::IndexedTruncated(a) => a.devices(out),
            Self::IndexedComparison(a) => a.devices(out),
            Self::IndexedBit(a) => a.devices(out),
            Self::WordCounter(a) => a.devices(out),
            Self::NumericReset(a) => a.devices(out),
            Self::ExternalIo(a) => a.devices(out),
            Self::MotionBasic(a) => a.devices(out),
            Self::ModuleIo(a) => a.devices(out),
            Self::SerialIo(a) => a.devices(out),
            Self::ParallelRun(a) => a.devices(out),
            Self::AnalogKnob(a) => a.devices(out),
            Self::Drive(a) => a.devices(out),
            Self::Servo(a) => a.devices(out),
            Self::Dmv(a) => a.devices(out),
            Self::Ethernet(a) => a.devices(out),
            Self::RawEthernet(a) => a.devices(out),
            Self::Canopen(a) => a.devices(out),
            Self::CanExchange(a) => a.devices(out),
            Self::CoordinatedMotion(a) => a.devices(out),
            Self::Pu(a) => a.devices(out),
            Self::ExtendedMotion(a) => a.devices(out),
            Self::PulseOutput(a) => a.devices(out),
            Self::Refresh(a) => a.devices(out),
            Self::HighSpeed(a) => a.devices(out),
            Self::DoubleCounter(a) => a.devices(out),
            Self::Pid(a) => a.devices(out),
            Self::Ftc(a) => a.devices(out),
            Self::EthernetConfig(a) => a.devices(out),
            Self::Station(a) => a.devices(out),
            Self::StringMove(a) => a.devices(out),
            Self::Sfc(a) => a.devices(out),
            Self::Alarm(a) => a.devices(out),
            Self::SpecialTimer(a) => a.devices(out),
            Self::FileMemory(a) => a.devices(out),
            Self::Drum(a) => a.devices(out),
            Self::Matrix(a) => a.devices(out),
            Self::Hour(a) => a.devices(out),
            Self::Rtc(a) => a.devices(out),
            Self::Time(a) => a.devices(out),
            Self::Timing(a) => a.devices(out),
            Self::Gps(a) => a.devices(out),
            Self::Solar(a) => a.devices(out),
            Self::Search(a) => a.devices(out),
            Self::RampSort(a) => a.devices(out),
            Self::Transform(a) => a.devices(out),
            Self::Additional(a) => a.devices(out),
            Self::Random(a) => a.devices(out),
            Self::Float(a) => a.devices(out),
            Self::Bits(a) => a.devices(out),
            Self::Text(a) => a.devices(out),
            Self::Data(a) => a.devices(out),
            Self::MultiplyTruncated(a, b, d) | Self::DivideQuotient(a, b, d) => {
                d.devices(out);
                out.extend((1020..=1022).map(Memory::system_device));
                Memory::error_devices(out);
                for operand in [a, b] {
                    operand.devices(out);
                }
            }
            Self::Shift(a) => a.devices(out),
            Self::Wide(a) => a.devices(out),
            Self::BlockMove(source, dest, count) => {
                out.extend(source.devices(1).expect("validated BMOV source"));
                out.extend(dest.devices(1).expect("validated BMOV destination"));
                Memory::error_devices(out);
                if let Operand::Constant(count) = count {
                    out.extend(
                        source
                            .devices(*count as u16)
                            .expect("validated BMOV source"),
                    );
                    out.extend(
                        dest.devices(*count as u16)
                            .expect("validated BMOV destination"),
                    );
                } else if let Operand::Register(d) = count {
                    out.insert(*d);
                }
            }
            Self::Fill(value, dest, count) => {
                out.extend(dest.devices(*count).expect("validated FMOV destination"));
                value.devices(out);
            }
            Self::Exchange(source, dest) => {
                out.insert(Memory::system_device(1303));
                source.devices(out);
                dest.devices(out);
            }
            Self::ZoneCompare(low, high, value, d) => {
                out.extend(comparison_destinations(*d).expect("validated ZCP destination"));
                for operand in [low, high, value] {
                    operand.devices(out);
                }
            }
            Self::Multiply(a, b, d) | Self::Divide(a, b, d) => {
                if matches!(self, Self::Multiply(..)) {
                    out.insert(Memory::system_device(1022));
                    out.extend([Memory::system_device(1067), Memory::system_device(1068)]);
                } else {
                    Memory::error_devices(out);
                }
                d.devices(out);
                for operand in [a, b] {
                    operand.devices(out);
                }
            }
            Self::Compare(a, b, d) => {
                for device in comparison_destinations(*d).expect("validated CMP destination") {
                    out.insert(device);
                }
                for o in [a, b] {
                    o.devices(out);
                }
            }
            Self::Pulse(action) => action.source_devices(out),
            Self::Convert(_, source, dest) => {
                Memory::error_devices(out);
                source.devices(out);
                dest.devices(out);
            }
            Self::Move(source, dest) | Self::Complement(source, dest) => {
                source.devices(out);
                dest.devices(out);
            }
            Self::Add(a, b, dest) | Self::Sub(a, b, dest) => {
                out.extend((1020..=1022).map(Memory::system_device));
                a.devices(out);
                b.devices(out);
                dest.devices(out);
            }
            Self::Unary(_, dest) => dest.devices(out),
            Self::Logic(_, a, b, dest) => {
                a.devices(out);
                b.devices(out);
                dest.devices(out);
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone)]
pub struct Network {
    pub line: usize,
    pub end_line: usize,
    pub comment: String,
    pub condition: Expr,
    pub actions: Vec<Action>,
}
#[derive(Debug, Clone)]
pub enum Flow {
    Network(usize),
    Evaluate(Expr),
    Capture(usize),
    Restore(bool),
    Control {
        op: crate::program_control::ControlOp,
        line: usize,
    },
}
#[derive(Debug, Clone)]
pub struct Program {
    /// ST source remains canonical; graphical IL edits would discard its structure.
    pub source_is_st: bool,
    pub execution_plan: crate::execution_plan::ExecutionPlan,
    pub native_program_words: u16,
    pub native_action_pc: Vec<u16>,
    pub flow: Vec<Flow>,
    pub networks: Vec<Network>,
    pub devices: BTreeSet<Device>,
    pub instructions: usize,
}
impl Program {
    pub fn has_control(&self) -> bool {
        self.flow.iter().any(|s| matches!(s, Flow::Control { .. }))
    }
    pub fn requires_vm(&self) -> bool {
        self.has_control()
            || self.networks.iter().flat_map(|n| &n.actions).any(crate::rtc_clock::is_instruction)
            || self.networks.iter().flat_map(|n| &n.actions).any(crate::indexed_text::is_instruction)
            || self.networks.iter().flat_map(|n| &n.actions).any(crate::indexed_additional::is_instruction)
            || self.networks.iter().flat_map(|n| &n.actions).any(crate::calendar_tail::is_arithmetic)
            || self.execution_plan.nodes.iter().any(|n| matches!(n.op, crate::execution_plan::PlanOp::OperandTailControl { .. }))
            || self.networks.iter().any(|n| n.condition.has_indexed_operands())
            || self.flow.iter().any(|step| matches!(step, Flow::Evaluate(e) if e.has_indexed_operands()))
            || self.networks.iter().flat_map(|n| &n.actions).any(|a| {
                matches!(
                    a,
                    Action::WordCounter(_) | Action::IndexedBit(_) | Action::IndexedComparison(_) | Action::IndexedMultiply(_) | Action::IndexedFloat(_) | Action::IndexedRoot(_) | Action::IndexedFloatBinary(_) | Action::IndexedRealBinary(_) | Action::IndexedRotation(_) | Action::IndexedBlockExchange(_) | Action::IndexedShiftFifo(_) | Action::IndexedDataProcessing(_) | Action::IndexedCalendarTime(_) | Action::IndexedRtc(_) | Action::IndexedSegment(_) | Action::IndexedAsc(_) | Action::IndexedChecksum(_) | Action::IndexedText(_) | Action::IndexedAdditional(_) | Action::IndexedSearchToggle(_) | Action::IndexedAbsGray(_) | Action::IndexedBitNumber(_) | Action::IndexedRandom(_) | Action::IndexedSwapMask(_) | Action::IndexedLiteralMove(_) | Action::IndexedFloatComparison(_) | Action::IndexedTrig(_) | Action::IndexedDesqr(_) | Action::IndexedLogPow(_) | Action::IndexedExpLn(_) | Action::IndexedDecimalConversion(_) | Action::IndexedAngle(_) | Action::IndexedBcd(_) | Action::IndexedTruncated(_) | Action::SpecialTimer(_) | Action::Sfc(_) | Action::Alarm(_)
                        | Action::Pid(_) | Action::Ftc(_) | Action::EthernetConfig(_) | Action::Station(_) | Action::StringMove(_) | Action::DoubleCounter(_)
                        | Action::Refresh(_) | Action::HighSpeed(_) | Action::PulseOutput(_) | Action::ExternalIo(_) | Action::MotionBasic(_) | Action::ModuleIo(_) | Action::SerialIo(_) | Action::Drive(_) | Action::Servo(_) | Action::ExtendedMotion(_) | Action::Dmv(_) | Action::Ethernet(_) | Action::RawEthernet(_) | Action::Canopen(_) | Action::CanExchange(_) | Action::CoordinatedMotion(_) | Action::Pu(_)
                ) || matches!(a, Action::Pulse(inner) if matches!(inner.as_ref(), Action::IndexedComparison(_) | Action::IndexedMultiply(_) | Action::IndexedFloat(_) | Action::IndexedRoot(_) | Action::IndexedFloatBinary(_) | Action::IndexedRealBinary(_) | Action::IndexedRotation(_) | Action::IndexedBlockExchange(_) | Action::IndexedShiftFifo(_) | Action::IndexedDataProcessing(_) | Action::IndexedCalendarTime(_) | Action::IndexedRtc(_) | Action::IndexedSegment(_) | Action::IndexedAsc(_) | Action::IndexedChecksum(_) | Action::IndexedText(_) | Action::IndexedAdditional(_) | Action::IndexedSearchToggle(_) | Action::IndexedAbsGray(_) | Action::IndexedBitNumber(_) | Action::IndexedRandom(_) | Action::IndexedSwapMask(_) | Action::IndexedLiteralMove(_) | Action::IndexedFloatComparison(_) | Action::IndexedTrig(_) | Action::IndexedDesqr(_) | Action::IndexedLogPow(_) | Action::IndexedExpLn(_) | Action::IndexedDecimalConversion(_) | Action::IndexedAngle(_) | Action::IndexedBcd(_) | Action::IndexedTruncated(_) | Action::Alarm(_) | Action::StringMove(_) | Action::MotionBasic(_) | Action::ModuleIo(_) | Action::Pu(_)))
            })
    }
    /// Ladder edits are possible: IL source without flow control. MPS/MRD/MPP branches
    /// are allowed; only their shared (saved) part stays source-only.
    pub fn graphically_editable(&self) -> bool {
        !self.source_is_st && !self.has_control()
    }
    pub fn requires_source_editing(&self) -> bool {
        self.source_is_st || self.has_branches() || self.has_control()
    }
    pub fn has_branches(&self) -> bool {
        self.flow
            .iter()
            .any(|step| matches!(step, Flow::Capture(_)))
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub line: usize,
    pub message: String,
}
impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Satır {}: {}", self.line, self.message)
    }
}

/// Longest single network (rung). The old whole-program cap of 512 proved safe for the
/// recursive condition passes, so it now bounds one network instead.
pub const MAX_NETWORK_INSTRUCTIONS: usize = 512;

fn compile_inner(source: &str) -> Result<Program, Diagnostic> {
    let mut networks = Vec::new();
    let mut flow = Vec::new();
    let mut branch_stack = Vec::new();
    let mut expr = None;
    let mut stack = Vec::new();
    let mut actions = Vec::new();
    let mut comment = String::new();
    let mut current_comment = String::new();
    let mut last_comment_line = 0;
    let mut start = 1;
    let mut end_line = 1;
    let mut ended = false;
    let mut control_continuation = false;
    let mut sfc_bus = false;
    let mut instructions = 0;
    // Instructions of the network being built: its condition is a tree whose depth grows
    // with this count, and deep trees overflow the stack in later recursive passes.
    let mut network_instructions = 0usize;
    let mut networks_seen = 0usize;
    let mut timers = BTreeSet::new();
    for (idx, raw) in source.lines().enumerate() {
        let line = idx + 1;
        let fail = |s: String| Diagnostic { line, message: s };
        let raw = raw.trim();
        if let Some(c) = raw.strip_prefix(';').or_else(|| raw.strip_prefix("//")) {
            // Consecutive comment lines form one network comment: first line is the
            // title, the rest its description. A separated comment block replaces it.
            if last_comment_line + 1 == line && !comment.is_empty() {
                comment.push('\n');
                comment.push_str(c.trim());
            } else {
                comment = c.trim().to_owned();
            }
            last_comment_line = line;
            continue;
        }
        let raw_words = crate::il_lex::words(raw).map_err(fail)?;
        if raw_words.is_empty() {
            continue;
        }
        if ended {
            return Err(fail("END sonrasında komut olamaz".into()));
        }
        instructions += 1;
        // A completed network was pushed while handling the previous instruction.
        if networks.len() != networks_seen {
            networks_seen = networks.len();
            network_instructions = 0;
        }
        network_instructions += 1;
        if network_instructions > MAX_NETWORK_INSTRUCTIONS {
            return Err(fail(format!(
                "Bir ağ en fazla {MAX_NETWORK_INSTRUCTIONS} komut içerebilir; ağı bölün"
            )));
        }
        if instructions > crate::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS {
            return Err(fail(format!(
                "Program en fazla {} komut içerebilir (PLC program alanı)",
                crate::es2_encoder::MAX_EXECUTABLE_INSTRUCTIONS
            )));
        }
        let mut words: Vec<String> = raw_words
            .iter()
            .map(|word| {
                if word.starts_with(['\'', '"']) {
                    word.clone()
                } else {
                    word.to_ascii_uppercase()
                }
            })
            .collect();
        // ASC carries case-sensitive English letters. Only its mnemonic and
        // destination are case-insensitive; preserve the literal spelling.
        if words[0] == "ASC" {
            let last = words.len() - 1;
            for (i, raw) in raw_words.iter().enumerate().skip(1) {
                if i < last {
                    words[i] = raw.to_string();
                }
            }
        }
        let label_definition = words[0]
            .strip_prefix('P')
            .filter(|tail| !tail.is_empty() && tail.bytes().all(|v| v.is_ascii_digit()))
            .map(str::to_string);
        let interrupt_definition = words[0]
            .strip_prefix('I')
            .filter(|tail| !tail.is_empty() && tail.bytes().all(|v| v.is_ascii_digit()));
        let original_op = if label_definition.is_some() {
            "P"
        } else if interrupt_definition.is_some() {
            "I"
        } else {
            words[0].as_str()
        };
        let pulse = matches!(
            original_op,
            "MOVP"
                | "WDTP"
                | "$MOVP"
                | "ADDP"
                | "SUBP"
                | "INCP"
                | "DECP"
                | "NEGP"
                | "CMLP"
                | "WANDP"
                | "WORP"
                | "WXORP"
                | "CMPP"
                | "MULP"
                | "DIVP"
                | "MUL16P"
                | "DIV16P"
                | "MUL32P"
                | "DIV32P"
                | "ZCPP"
                | "BMOVP"
                | "FMOVP"
                | "XCHP"
                | "BCDP"
                | "BINP"
                | "DMOVP"
                | "DCMLP"
                | "DFMOVP"
                | "DXCHP"
                | "DBCDP"
                | "DBINP"
                | "DADDP"
                | "DSUBP"
                | "DMULP"
                | "DDIVP"
                | "DINCP"
                | "DDECP"
                | "DANDP"
                | "DORP"
                | "DXORP"
                | "DNEGP"
                | "DCMPP"
                | "DZCPP"
                | "RORP"
                | "ROLP"
                | "RCRP"
                | "RCLP"
                | "DRORP"
                | "DROLP"
                | "DRCRP"
                | "DRCLP"
                | "SFTRP"
                | "SFTLP"
                | "WSFRP"
                | "WSFLP"
                | "SFWRP"
                | "SFRDP"
                | "ZRSTP"
                | "SMOVP"
                | "DECOP"
                | "ENCOP"
                | "SUMP"
                | "DSUMP"
                | "BONP"
                | "DBONP"
                | "MEANP"
                | "DMEANP"
                | "SQRP"
                | "DSQRP"
                | "ABSP"
                | "DABSP"
                | "GRYP"
                | "DGRYP"
                | "GBINP"
                | "DGBINP"
                | "SEGDP"
                | "ASCIP"
                | "ASCIIP"
                | "HEXP"
                | "CCDP"
                | "LRCP"
                | "CRCP"
                | "FLTP"
                | "DFLTP"
                | "INTP"
                | "DINTP"
                | "DMOVRP"
                | "DECMPP"
                | "DEZCPP"
                | "DRADP"
                | "DDEGP"
                | "DEBCDP"
                | "DEBINP"
                | "DEADDP"
                | "DESUBP"
                | "DEMULP"
                | "DEDIVP"
                | "DEXPP"
                | "DLNP"
                | "DLOGP"
                | "DESQRP"
                | "DPOWP"
                | "DSINP"
                | "DCOSP"
                | "DTANP"
                | "DASINP"
                | "DACOSP"
                | "DATANP"
                | "DADDRP"
                | "DSUBRP"
                | "DMULRP"
                | "DDIVRP"
                | "SWAPP"
                | "DSWAPP"
                | "RANDP"
                | "DRANDP"
                | "MVMP"
                | "DMVMP"
                | "MMOVP"
                | "WSUMP"
                | "DWSUMP"
                | "SCALP"
                | "SCLPP"
                | "DSCLPP"
                | "CMPTP"
                | "DCMPTP"
                | "SERP"
                | "DSERP"
                | "ALTP"
                | "DTMP"
                | "DELAYP"
                | "REFP"
                | "REFFP"
                | "DICFP"
                | "TCMPP"
                | "TRDP"
                | "TWRP"
                | "TZCPP"
                | "TADDP"
                | "TSUBP"
                | "MEMRP"
                | "DMEMRP"
                | "MEMWP"
                | "CJP"
                | "CALLP"
                | "DMEMWP"
                | "MANDP"
                | "MORP"
                | "MXORP"
                | "MXNRP"
                | "MINVP"
                | "MCMPP"
                | "MBRDP"
                | "MBWRP"
                | "MBSP"
                | "MBRP"
                | "MBCP"
                | "ANRP"
                | "FROMP"
                | "DFROMP"
                | "TOP"
                | "DTOP"
                | "PRUNP"
                | "DPRUNP"
                | "VRRDP"
                | "VRSCP"
                | "PUXP"
                | "DPULSP"
        );
        let op = if pulse {
            &original_op[..original_op.len() - 1]
        } else {
            original_op
        };
        let args = &words[1..];
        let wide_comparison = ["DLD", "DAND", "DOR"]
            .into_iter()
            .any(|prefix| op.strip_prefix(prefix).and_then(Relation::parse).is_some());
        let comparison_op = if wide_comparison { &op[1..] } else { op };
        let comparison = ["LD", "AND", "OR"].into_iter().find_map(|prefix| {
            comparison_op
                .strip_prefix(prefix)
                .and_then(Relation::parse)
                .map(|relation| (prefix, relation))
        });
        let numeric_contact = crate::numeric_contacts::parse_spec(op);
        let float_contact = crate::float_ops::parse_spec(op);
        let arity = match op {
            "PID" | "DPID" | "YOUT" | "FTC" => 4,
            "XCMP" => 5,
            "$MOV" => 2,
            _ if float_contact.is_some() => 2,
            _ if numeric_contact.is_some() => numeric_contact.as_ref().unwrap().arity,
            _ if comparison.is_some() => 2,
            "LD" | "LDI" | "LDP" | "LDF" | "AND" | "ANI" | "ANDP" | "ANDF" | "OR" | "ORI"
            | "ORP" | "ORF" | "OUT" | "SET" | "RST" => 1,
            "INC" | "DEC" | "NEG" => 1,
            "SWAP" | "DSWAP" => 1,
            "ALT" => 1,
            "DELAY" => 1,
            "TRD" | "TWR" => 1,
            "HOUR" | "DHOUR" => 3,
            "ABSD" | "DABSD" | "INCD" => 4,
            "MEMR" | "DMEMR" | "MEMW" | "DMEMW" => 3,
            "FROM" | "DFROM" | "TO" | "DTO" => 4,
            "RS" => 4,
            "MODRD" | "MODWR" => 3,
            "MODRW" => 5,
            "PRUN" | "DPRUN" => 2,
            "MAND" | "MOR" | "MXOR" | "MXNR" | "MCMP" => 4,
            "MINV" | "MBRD" | "MBWR" | "MBS" | "MBR" | "MBC" => 3,
            "TCMP" => 5,
            "TZCP" => 4,
            "TADD" | "TSUB" => 3,
            "GPWM" | "CSFO" => 3,
            "GPS" | "DSPA" => 2,
            "SER" | "DSER" | "RAMP" | "DRAMP" | "DTM" => 4,
            "SORT" | "DSORT" => 5,
            "MMOV" => 2,
            "VRRD" | "VRSC" => 2,
            "FWD" | "REV" | "STOP" => 3,
            "RDST" | "RSTEF" => 2,
            "ASDRW" | "DPTPO" => 3,
            "DMVRW" | "ETHRW" => 4,
            "INITC" => 1,
            "RSTD" => 4,
            "COPRW" => 6,
            "ETHRS" => 6,
            "ETHCONF" => 12,
            "ETHMSG" => 13,
            "CANRS" => 5,
            "COPWL" | "DCOPWL" => 4,
            "ASDON" => 2,
            "CASD" | "DDRVIC" | "DDRVAC" | "ZRNC" | "DZRNC" => 3,
            "PLSVC" | "DPLSVC" => 2,
            "ZRNM" => 4,
            "EMER" => 5,
            "DPPMR" | "DPPMA" | "DCIMR" | "DCIMA" => 4,
            "DPUCONF" => 12,
            "PUSTAT" | "DPUZRN" | "DPUCNT" => 8,
            "DPUPLS" | "DPUDRI" | "DPUDRA" | "PUX" | "DPULS" => 7,
            "DPUJOG" => 6,
            "DPUMPG" => 10,
            "DCLLM" | "DVSPO" => 4,
            "RAND" | "DRAND" | "MVM" | "DMVM" | "WSUM" | "DWSUM" | "SCLP" | "DSCLP" => 3,
            "SCAL" | "CMPT" | "DCMPT" => 4,
            "TMR" | "ATMR" | "TTMR" | "CNT" | "MOV" | "CML" | "XCH" | "BCD" | "BIN" => 2,
            "STMR" => 3,
            "BMOV" | "FMOV" => 3,
            "ADD" | "SUB" | "WAND" | "WOR" | "WXOR" | "CMP" | "MUL" | "DIV" => 3,
            "MUL16" | "DIV16" | "MUL32" | "DIV32" => 3,
            "DECO" | "ENCO" | "BON" | "DBON" | "MEAN" | "DMEAN" => 3,
            "SUM" | "DSUM" | "SQR" | "DSQR" => 2,
            "ABS" | "DABS" => 1,
            "SEGD" => 2,
            "ASC" => {
                if args.len() == 9 {
                    9
                } else {
                    2
                }
            }
            "ASCI" | "ASCII" | "HEX" | "CCD" | "LRC" | "CRC" => 3,
            "FLT" | "DFLT" | "INT" | "DINT" | "DMOVR" => 2,
            "DECMP" => 3,
            "DRAD" | "DDEG" | "DEBCD" | "DEBIN" => 2,
            "DEXP" | "DLN" | "DESQR" | "DSIN" | "DCOS" | "DTAN" | "DASIN" | "DACOS" | "DATAN" => 2,
            "DLOG" | "DPOW" | "DADDR" | "DSUBR" | "DMULR" | "DDIVR" => 3,
            "DEADD" | "DESUB" | "DEMUL" | "DEDIV" => 3,
            "DEZCP" => 4,
            "GRY" | "DGRY" | "GBIN" | "DGBIN" | "BOUT" | "DBOUT" | "BSET" | "DBSET" | "BRST"
            | "DBRST" => 2,
            "ZCP" => 4,
            "DMOV" | "DCML" | "DXCH" | "DBCD" | "DBIN" => 2,
            "DADD" | "DSUB" | "DMUL" | "DDIV" | "DAND" | "DOR" | "DXOR" | "DCMP" | "DFMOV" => 3,
            "DINC" | "DDEC" | "DNEG" => 1,
            "DZCP" => 4,
            "PLS" | "PLF" => 1,
            "ROR" | "ROL" | "RCR" | "RCL" | "DROR" | "DROL" | "DRCR" | "DRCL" | "ZRST" => 2,
            "SFWR" | "SFRD" => 3,
            "SFTR" | "SFTL" | "WSFR" | "WSFL" => 4,
            "SMOV" => 5,
            "CJ" | "CALL" | "MC" | "MCR" | "FOR" | "STL" => 1,
            "IST" => 3,
            "DCNT" => 2,
            "PLSY" | "DPLSY" | "PWM" => 3,
            "DABSR" | "PLSV" | "DPLSV" | "DICF" => 3,
            "DZRN" | "DDRVI" | "DDRVA" => 4,
            "PLSR" | "DPLSR" => 4,
            "TKY" | "DTKY" | "SEGL" => 3,
            "HKY" | "DHKY" | "DSW" | "ARWS" => 4,
            "PR" => 2,
            "REF" => 2,
            "REFF" => 1,
            "MTR" | "DHSZ" => 4,
            "DHSCS" | "DHSCR" | "SPD" => 3,
            "ANS" => 3,
            "ANR" => 0,
            "P" | "I" | "IRET" | "EI" | "DI" | "WDT" | "SRET" | "FEND" | "NEXT" | "RET" => 0,
            "INV" => 0,
            "END" | "NOP" | "ANB" | "ORB" | "NP" | "PN" | "MPS" | "MRD" | "MPP" => 0,
            _ => return Err(fail(format!("Desteklenmeyen komut: {op}"))),
        };
        if args.len() != arity {
            return Err(fail(format!("{op}: {arity} operand gerekli")));
        }
        if matches!(
            op,
            "P" | "I"
                | "IRET"
                | "EI"
                | "DI"
                | "WDT"
                | "CJ"
                | "CALL"
                | "MC"
                | "MCR"
                | "FOR"
                | "SRET"
                | "FEND"
                | "NEXT"
                | "RET"
                | "STL"
                | "NOP"
        ) {
            use crate::program_control::{ControlOp, PointerProfile};
            let control = if let Some(label) = label_definition {
                ControlOp::Label(
                    label
                        .parse::<u8>()
                        .map_err(|_| fail("Etiket P0..P255 olmalı".into()))?,
                )
            } else if interrupt_definition.is_some() {
                ControlOp::InterruptLabel(words[0].parse().map_err(fail)?)
            } else {
                ControlOp::parse_with_profile(original_op, args, PointerProfile::InstalledIndexed)
                    .map_err(fail)?
            };
            if matches!(
                control,
                ControlOp::Jump { .. }
                    | ControlOp::Call { .. }
                    | ControlOp::MasterEnter { .. }
                    | ControlOp::Watchdog { .. }
            ) && expr.is_none()
            {
                return Err(fail(
                    "CJ/CALL/MC öncesinde bir mantık koşulu gerekli".into(),
                ));
            }
            if !stack.is_empty() {
                return Err(fail(
                    "Kontrol komutundan önce LD bloklarını ANB/ORB ile birleştirin".into(),
                ));
            }
            if let Some(condition) = expr.take() {
                if actions.is_empty() {
                    flow.push(Flow::Evaluate(condition));
                } else {
                    flow.push(Flow::Network(networks.len()));
                    networks.push(Network {
                        line: start,
                        end_line,
                        comment: current_comment.clone(),
                        condition,
                        actions: std::mem::take(&mut actions),
                    });
                }
            }
            flow.push(Flow::Control { op: control, line });
            if op == "STL" {
                sfc_bus = true;
            } else if op == "RET" {
                sfc_bus = false;
            }
            // Saved has no native instruction: following actions consume the
            // current restored WORD rather than reevaluating caller contacts.
            expr = Some(Expr::Saved(usize::MAX - networks.len()));
            control_continuation = true;
            start = line + 1;
            end_line = line;
            current_comment = String::new();
            continue;
        }
        if op == "END" {
            if !branch_stack.is_empty() {
                return Err(fail("MPS dalları MPP ile kapatılmalı".into()));
            }
            if !stack.is_empty() {
                return Err(fail(
                    "Birleştirilmemiş LD bloğu: ANB veya ORB gerekli".into(),
                ));
            }
            if let Some(condition) = expr.take() {
                if actions.is_empty() {
                    if control_continuation {
                        // The previous control already consumed this condition.
                    } else if flow.iter().any(|s| matches!(s, Flow::Capture(_))) {
                        flow.push(Flow::Evaluate(condition));
                    } else {
                        return Err(fail("Koşulun çıkış komutu yok".into()));
                    }
                } else {
                    flow.push(Flow::Network(networks.len()));
                    networks.push(Network {
                        line: start,
                        end_line,
                        comment: current_comment.clone(),
                        condition,
                        actions: std::mem::take(&mut actions),
                    });
                }
            }
            ended = true;
            continue;
        }
        if matches!(op, "LD" | "LDI" | "LDP" | "LDF")
            || float_contact
                .as_ref()
                .is_some_and(|(prefix, _)| *prefix == "LD")
            || numeric_contact
                .as_ref()
                .is_some_and(|spec| spec.prefix == "LD")
            || comparison
                .as_ref()
                .is_some_and(|(prefix, _)| *prefix == "LD")
        {
            sfc_bus = false;
            if control_continuation && actions.is_empty() {
                expr = None;
            }
            control_continuation = false;
            if !actions.is_empty() {
                if !branch_stack.is_empty() {
                    return Err(fail(
                        "MPS dalında çıkıştan sonra koşulu MRD/MPP ile geri alın".into(),
                    ));
                }
                if !stack.is_empty() {
                    return Err(fail("Birleştirilmemiş LD bloğu".into()));
                }
                flow.push(Flow::Network(networks.len()));
                networks.push(Network {
                    line: start,
                    end_line,
                    comment: current_comment.clone(),
                    condition: expr.take().unwrap(),
                    actions: std::mem::take(&mut actions),
                });
            }
            let condition = if crate::indexed_bit::IndexedBitContact::supports(op)
                && crate::indexed_bit::IndexedBitContact::has_modifiers(args)
            {
                Expr::IndexedBit(
                    crate::indexed_bit::IndexedBitContact::parse(op, args).map_err(fail)?,
                )
            } else if crate::indexed_bit_number::IndexedBitNumberPredicate::supports(op)
                && crate::indexed_bit_number::IndexedBitNumberPredicate::has_modifiers(args)
            {
                Expr::IndexedBitNumber(
                    crate::indexed_bit_number::IndexedBitNumberPredicate::parse(op, args)
                        .map_err(fail)?,
                )
            } else if crate::indexed_comparison::IndexedPredicate::supports(op)
                && crate::indexed_comparison::IndexedPredicate::has_modifiers(args)
            {
                Expr::IndexedComparison(
                    crate::indexed_comparison::IndexedPredicate::parse(op, args).map_err(fail)?,
                )
            } else if crate::indexed_float_comparison::IndexedFloatPredicate::supports(op)
                && crate::indexed_float_comparison::IndexedFloatPredicate::has_modifiers(args)
            {
                Expr::IndexedFloatComparison(
                    crate::indexed_float_comparison::IndexedFloatPredicate::parse(op, args)
                        .map_err(fail)?,
                )
            } else if float_contact.is_some() {
                Expr::Float(FloatPredicate::parse(op, args).map_err(fail)?)
            } else if numeric_contact.is_some() {
                Expr::Numeric(NumericPredicate::parse(op, args).map_err(fail)?)
            } else if let Some((_, relation)) = comparison {
                if wide_comparison {
                    Expr::CompareWide(
                        relation,
                        CalendarValue::parse(&args[0], true).map_err(fail)?,
                        CalendarValue::parse(&args[1], true).map_err(fail)?,
                    )
                } else {
                    Expr::Compare(
                        relation,
                        CalendarValue::parse(&args[0], false).map_err(fail)?,
                        CalendarValue::parse(&args[1], false).map_err(fail)?,
                    )
                }
            } else {
                let d: Device = args[0].parse().map_err(fail)?;
                if !d.bit() {
                    return Err(fail("Kontak D register olamaz".into()));
                }
                if matches!(op, "LDP" | "LDF") {
                    Expr::Edge(d, op == "LDP", instructions)
                } else {
                    Expr::Contact(d, op == "LDI")
                }
            };
            if let Some(old) = expr.take() {
                stack.push(old);
                if stack.len() > 8 {
                    return Err(fail("En fazla 8 iç içe blok desteklenir".into()));
                }
                if stack.len() + branch_stack.len() > 8 {
                    return Err(fail(
                        "MPS dalları ve iç içe LD blokları toplamı 8'i aşamaz".into(),
                    ));
                }
            } else {
                start = line;
                current_comment = std::mem::take(&mut comment);
            }
            expr = Some(condition);
            continue;
        }
        if expr.is_none() {
            return Err(fail("Önce LD veya LDI ile bir ağ başlatın".into()));
        }
        if matches!(op, "MPS" | "MRD" | "MPP") {
            if sfc_bus {
                return Err(fail(
                    "STL veriyolunda MPS/MRD/MPP öncesi LD/LDI gerekli".into(),
                ));
            }
            if !stack.is_empty() {
                return Err(fail(
                    "MPS/MRD/MPP öncesinde LD bloklarını ANB/ORB ile birleştirin".into(),
                ));
            }
            if op == "MPS" && branch_stack.len() >= 8 {
                return Err(fail("En fazla 8 iç içe MPS dalı desteklenir".into()));
            }
            if op != "MPS" && branch_stack.is_empty() {
                return Err(fail(format!("{op}: eşleşen MPS yok")));
            }
            let condition = expr.take().unwrap();
            if actions.is_empty() {
                flow.push(Flow::Evaluate(condition));
            } else {
                flow.push(Flow::Network(networks.len()));
                networks.push(Network {
                    line: start,
                    end_line,
                    comment: current_comment.clone(),
                    condition,
                    actions: std::mem::take(&mut actions),
                });
            }
            let id = if op == "MPS" {
                branch_stack.push(instructions);
                flow.push(Flow::Capture(instructions));
                instructions
            } else {
                let id = *branch_stack.last().unwrap();
                if op == "MPP" {
                    branch_stack.pop();
                }
                flow.push(Flow::Restore(op == "MPP"));
                id
            };
            expr = Some(Expr::Saved(id));
            start = line + 1;
            end_line = line;
            current_comment = format!("{op} · kaydedilmiş koşul");
            continue;
        }
        if matches!(op, "PN" | "NP" | "INV") {
            if !actions.is_empty() {
                return Err(fail("Çıkıştan sonra yeni koşul için LD kullanın".into()));
            }
            expr = if op == "INV" {
                Some(Expr::Invert(Box::new(expr.take().unwrap())))
            } else {
                Some(Expr::Trigger(
                    Box::new(expr.take().unwrap()),
                    op == "NP",
                    instructions,
                ))
            };
            continue;
        }
        if comparison.is_some()
            || float_contact.is_some()
            || numeric_contact.is_some()
            || matches!(
                op,
                "AND" | "ANI" | "ANDP" | "ANDF" | "OR" | "ORI" | "ORP" | "ORF" | "ANB" | "ORB"
            )
        {
            if !actions.is_empty() {
                return Err(fail(
                    "Çıkıştan sonra yeni koşul için LD veya MRD/MPP kullanın".into(),
                ));
            }
            let right = if crate::indexed_bit::IndexedBitContact::supports(op)
                && crate::indexed_bit::IndexedBitContact::has_modifiers(args)
            {
                Expr::IndexedBit(
                    crate::indexed_bit::IndexedBitContact::parse(op, args).map_err(fail)?,
                )
            } else if crate::indexed_bit_number::IndexedBitNumberPredicate::supports(op)
                && crate::indexed_bit_number::IndexedBitNumberPredicate::has_modifiers(args)
            {
                Expr::IndexedBitNumber(
                    crate::indexed_bit_number::IndexedBitNumberPredicate::parse(op, args)
                        .map_err(fail)?,
                )
            } else if crate::indexed_comparison::IndexedPredicate::supports(op)
                && crate::indexed_comparison::IndexedPredicate::has_modifiers(args)
            {
                Expr::IndexedComparison(
                    crate::indexed_comparison::IndexedPredicate::parse(op, args).map_err(fail)?,
                )
            } else if crate::indexed_float_comparison::IndexedFloatPredicate::supports(op)
                && crate::indexed_float_comparison::IndexedFloatPredicate::has_modifiers(args)
            {
                Expr::IndexedFloatComparison(
                    crate::indexed_float_comparison::IndexedFloatPredicate::parse(op, args)
                        .map_err(fail)?,
                )
            } else if float_contact.is_some() {
                Expr::Float(FloatPredicate::parse(op, args).map_err(fail)?)
            } else if numeric_contact.is_some() {
                Expr::Numeric(NumericPredicate::parse(op, args).map_err(fail)?)
            } else if let Some((_, relation)) = comparison.as_ref() {
                if wide_comparison {
                    Expr::CompareWide(
                        relation.clone(),
                        CalendarValue::parse(&args[0], true).map_err(fail)?,
                        CalendarValue::parse(&args[1], true).map_err(fail)?,
                    )
                } else {
                    Expr::Compare(
                        relation.clone(),
                        CalendarValue::parse(&args[0], false).map_err(fail)?,
                        CalendarValue::parse(&args[1], false).map_err(fail)?,
                    )
                }
            } else if matches!(op, "ANB" | "ORB") {
                expr.take().unwrap()
            } else {
                let d: Device = args[0].parse().map_err(fail)?;
                if !d.bit() {
                    return Err(fail("Kontak D register olamaz".into()));
                }
                let contact = Expr::Contact(d, matches!(op, "ANI" | "ORI"));
                if matches!(op, "ANDP" | "ANDF" | "ORP" | "ORF") {
                    Expr::Edge(d, matches!(op, "ANDP" | "ORP"), instructions)
                } else {
                    contact
                }
            };
            let left = if matches!(op, "ANB" | "ORB") {
                stack
                    .pop()
                    .ok_or_else(|| fail("Birleştirilecek LD bloğu yok".into()))?
            } else {
                expr.take().unwrap()
            };
            expr = Some(
                if comparison
                    .as_ref()
                    .is_some_and(|(prefix, _)| *prefix == "OR")
                    || numeric_contact
                        .as_ref()
                        .is_some_and(|spec| spec.prefix == "OR")
                    || float_contact
                        .as_ref()
                        .is_some_and(|(prefix, _)| *prefix == "OR")
                    || matches!(op, "OR" | "ORI" | "ORP" | "ORF" | "ORB")
                {
                    Expr::Or(Box::new(left), Box::new(right))
                } else {
                    Expr::And(Box::new(left), Box::new(right))
                },
            );
            continue;
        }
        if !stack.is_empty() {
            return Err(fail(
                "Çıkıştan önce LD bloklarını ANB/ORB ile birleştirin".into(),
            ));
        }
        if crate::indexed_bit::IndexedBitAction::supports(op)
            && crate::indexed_bit::IndexedBitAction::has_modifiers(args)
        {
            actions.push(Action::IndexedBit(
                crate::indexed_bit::IndexedBitAction::parse(op, args).map_err(fail)?,
            ));
            end_line = line;
            continue;
        }
        if op == "CNT" {
            let action = crate::word_counter::WordCounterAction::parse(args).map_err(fail)?;
            if !timers.insert(action.counter) {
                return Err(fail(format!(
                    "{} için birden fazla TMR/CNT tanımı var",
                    action.counter
                )));
            }
            actions.push(Action::WordCounter(action));
            end_line = line;
            continue;
        }
        if op == "RST" && args.first().is_some_and(|s| s.starts_with(['D', 'E', 'F'])) {
            actions.push(Action::NumericReset(
                crate::word_counter::NumericResetAction::parse(args).map_err(fail)?,
            ));
            end_line = line;
            continue;
        }
        if crate::indexed_comparison::IndexedComparisonAction::supports(op)
            && crate::indexed_comparison::IndexedComparisonAction::has_modifiers(args)
        {
            let action = Action::IndexedComparison(
                crate::indexed_comparison::IndexedComparisonAction::parse(op, args)
                    .map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_angle::IndexedAngleAction::supports(op)
            && crate::indexed_angle::IndexedAngleAction::has_modifiers(args)
        {
            let action = Action::IndexedAngle(
                crate::indexed_angle::IndexedAngleAction::parse(op, args).map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_decimal_conversion::IndexedDecimalConversionAction::supports(op)
            && crate::indexed_decimal_conversion::IndexedDecimalConversionAction::has_modifiers(
                args,
            )
        {
            let action = Action::IndexedDecimalConversion(
                crate::indexed_decimal_conversion::IndexedDecimalConversionAction::parse(op, args)
                    .map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_exp_ln::IndexedExpLnAction::supports(op)
            && crate::indexed_exp_ln::IndexedExpLnAction::has_modifiers(args)
        {
            let action = Action::IndexedExpLn(
                crate::indexed_exp_ln::IndexedExpLnAction::parse(op, args).map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_log_pow::IndexedLogPowAction::supports(op)
            && crate::indexed_log_pow::IndexedLogPowAction::has_modifiers(args)
        {
            let action = Action::IndexedLogPow(
                crate::indexed_log_pow::IndexedLogPowAction::parse(op, args).map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_desqr::IndexedDesqrAction::supports(op)
            && crate::indexed_desqr::IndexedDesqrAction::has_modifiers(args)
        {
            let action = Action::IndexedDesqr(
                crate::indexed_desqr::IndexedDesqrAction::parse(op, args).map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_trig::IndexedTrigAction::supports(op)
            && crate::indexed_trig::IndexedTrigAction::has_modifiers(args)
        {
            let action = Action::IndexedTrig(
                crate::indexed_trig::IndexedTrigAction::parse(op, args).map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_float_comparison::IndexedFloatComparisonAction::supports(op)
            && crate::indexed_float_comparison::IndexedFloatComparisonAction::has_modifiers(args)
        {
            let action = Action::IndexedFloatComparison(
                crate::indexed_float_comparison::IndexedFloatComparisonAction::parse(op, args)
                    .map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_literal_move::IndexedLiteralMoveAction::supports(op)
            && crate::indexed_literal_move::IndexedLiteralMoveAction::has_modifiers(args)
        {
            let action = Action::IndexedLiteralMove(
                crate::indexed_literal_move::IndexedLiteralMoveAction::parse(op, args)
                    .map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_swap_mask::IndexedSwapMaskAction::supports(op)
            && crate::indexed_swap_mask::IndexedSwapMaskAction::has_modifiers(args)
        {
            let action = Action::IndexedSwapMask(
                crate::indexed_swap_mask::IndexedSwapMaskAction::parse(op, args).map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_random::IndexedRandomAction::supports(op)
            && crate::indexed_random::IndexedRandomAction::has_modifiers(args)
        {
            let action = Action::IndexedRandom(
                crate::indexed_random::IndexedRandomAction::parse(op, args).map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_bit_number::IndexedBitNumberAction::supports(op)
            && crate::indexed_bit_number::IndexedBitNumberAction::has_modifiers(args)
        {
            actions.push(Action::IndexedBitNumber(
                crate::indexed_bit_number::IndexedBitNumberAction::parse(op, args).map_err(fail)?,
            ));
            end_line = line;
            continue;
        }
        if crate::indexed_abs_gray::IndexedAbsGrayAction::supports(op)
            && crate::indexed_abs_gray::IndexedAbsGrayAction::has_modifiers(args)
        {
            let action = Action::IndexedAbsGray(
                crate::indexed_abs_gray::IndexedAbsGrayAction::parse(op, args).map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_search_toggle::IndexedSearchToggleAction::supports(op)
            && (op == "DSER"
                || crate::indexed_search_toggle::IndexedSearchToggleAction::has_modifiers(args))
        {
            let action = Action::IndexedSearchToggle(
                crate::indexed_search_toggle::IndexedSearchToggleAction::parse(op, args)
                    .map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_additional::IndexedAdditionalAction::supports(op) {
            let action = Action::IndexedAdditional(
                crate::indexed_additional::IndexedAdditionalAction::parse(op, args)
                    .map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_text::IndexedTextAction::supports(op) {
            let action = Action::IndexedText(
                crate::indexed_text::IndexedTextAction::parse(op, args).map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_checksum::IndexedChecksumAction::supports(op) {
            let action = Action::IndexedChecksum(
                crate::indexed_checksum::IndexedChecksumAction::parse(op, args).map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_asc::IndexedAscAction::supports(op)
            && crate::indexed_asc::IndexedAscAction::has_modifiers(args)
        {
            actions.push(Action::IndexedAsc(
                crate::indexed_asc::IndexedAscAction::parse(op, args).map_err(fail)?,
            ));
            end_line = line;
            continue;
        }
        if crate::indexed_segment::IndexedSegmentAction::supports(op)
            && crate::indexed_segment::IndexedSegmentAction::has_modifiers(args)
        {
            let action = Action::IndexedSegment(
                crate::indexed_segment::IndexedSegmentAction::parse(op, args).map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_calendar_time::IndexedCalendarTimeAction::supports(op)
            && (matches!(op, "TADD" | "TSUB")
                || crate::indexed_calendar_time::IndexedCalendarTimeAction::has_modifiers(args))
        {
            let action = Action::IndexedCalendarTime(
                crate::indexed_calendar_time::IndexedCalendarTimeAction::parse(op, args)
                    .map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_data_processing::IndexedDataProcessingAction::supports(op)
            && crate::indexed_data_processing::IndexedDataProcessingAction::has_modifiers(args)
        {
            let action = Action::IndexedDataProcessing(
                crate::indexed_data_processing::IndexedDataProcessingAction::parse(op, args)
                    .map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_shift_fifo::IndexedShiftFifoAction::supports(op)
            && crate::indexed_shift_fifo::IndexedShiftFifoAction::has_modifiers(args)
        {
            let action = Action::IndexedShiftFifo(
                crate::indexed_shift_fifo::IndexedShiftFifoAction::parse(op, args).map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_block_exchange::IndexedBlockExchangeAction::supports(op)
            && (op == "DFMOV"
                || crate::indexed_block_exchange::IndexedBlockExchangeAction::has_modifiers(args))
        {
            let action = Action::IndexedBlockExchange(
                crate::indexed_block_exchange::IndexedBlockExchangeAction::parse(op, args)
                    .map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_rotation::IndexedRotationAction::supports(op)
            && crate::indexed_rotation::IndexedRotationAction::has_modifiers(args)
        {
            let action = Action::IndexedRotation(
                crate::indexed_rotation::IndexedRotationAction::parse(op, args).map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_real_binary::IndexedRealBinaryAction::supports(op)
            && crate::indexed_real_binary::IndexedRealBinaryAction::has_modifiers(args)
        {
            let action = Action::IndexedRealBinary(
                crate::indexed_real_binary::IndexedRealBinaryAction::parse(op, args)
                    .map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_float_binary::IndexedFloatBinaryAction::supports(op)
            && crate::indexed_float_binary::IndexedFloatBinaryAction::has_modifiers(args)
        {
            let action = Action::IndexedFloatBinary(
                crate::indexed_float_binary::IndexedFloatBinaryAction::parse(op, args)
                    .map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_root::IndexedRootAction::supports(op)
            && crate::indexed_root::IndexedRootAction::has_modifiers(args)
        {
            let action = Action::IndexedRoot(
                crate::indexed_root::IndexedRootAction::parse(op, args).map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_float::IndexedFloatAction::supports(op) {
            let action = Action::IndexedFloat(
                crate::indexed_float::IndexedFloatAction::parse(op, args).map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_bcd::IndexedBcdAction::supports(op)
            && crate::indexed_bcd::IndexedBcdAction::has_modifiers(args)
        {
            let action = Action::IndexedBcd(
                crate::indexed_bcd::IndexedBcdAction::parse(op, args).map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_truncated::IndexedTruncatedAction::supports(op)
            && crate::indexed_truncated::IndexedTruncatedAction::has_modifiers(args)
        {
            let action = Action::IndexedTruncated(
                crate::indexed_truncated::IndexedTruncatedAction::parse(op, args).map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_multiply::IndexedMultiplyAction::supports(op)
            && crate::indexed_multiply::IndexedMultiplyAction::has_modifiers(args)
        {
            let action = Action::IndexedMultiply(
                crate::indexed_multiply::IndexedMultiplyAction::parse(op, args).map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::indexed_scalar::IndexedScalarAction::supports(op)
            && crate::indexed_scalar::IndexedScalarAction::has_modifiers(args)
        {
            let action = Action::IndexedScalar(
                crate::indexed_scalar::IndexedScalarAction::parse(op, args).map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if matches!(
            op,
            "FLT"
                | "DFLT"
                | "INT"
                | "DINT"
                | "DMOVR"
                | "DECMP"
                | "DEZCP"
                | "DRAD"
                | "DDEG"
                | "DEBCD"
                | "DEBIN"
                | "DEADD"
                | "DESUB"
                | "DEMUL"
                | "DEDIV"
                | "DEXP"
                | "DLN"
                | "DLOG"
                | "DESQR"
                | "DPOW"
                | "DSIN"
                | "DCOS"
                | "DTAN"
                | "DASIN"
                | "DACOS"
                | "DATAN"
                | "DADDR"
                | "DSUBR"
                | "DMULR"
                | "DDIVR"
        ) {
            let action = Action::Float(FloatAction::parse(op, args).map_err(fail)?);
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            continue;
        }
        if matches!(
            op,
            "SEGD" | "ASC" | "ASCI" | "ASCII" | "HEX" | "CCD" | "LRC" | "CRC"
        ) {
            let action = Action::Text(TextAction::parse(op, args).map_err(fail)?);
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            continue;
        }
        if matches!(op, "MEMR" | "DMEMR" | "MEMW" | "DMEMW") {
            let action = Action::FileMemory(FileAction::parse(op, args).map_err(fail)?);
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if matches!(op, "ABSD" | "DABSD" | "INCD") {
            actions.push(Action::Drum(DrumAction::parse(op, args).map_err(fail)?));
            end_line = line;
            continue;
        }
        if matches!(
            op,
            "MAND"
                | "MOR"
                | "MXOR"
                | "MXNR"
                | "MINV"
                | "MCMP"
                | "MBRD"
                | "MBWR"
                | "MBS"
                | "MBR"
                | "MBC"
        ) {
            let action = Action::Matrix(MatrixAction::parse(op, args).map_err(fail)?);
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if matches!(
            op,
            "TKY" | "DTKY" | "HKY" | "DHKY" | "DSW" | "SEGL" | "ARWS" | "PR"
        ) {
            if pulse {
                return Err(fail(format!("{op}P belgelenmiş bir komut değil")));
            }
            if op == "PR"
                && networks
                    .iter()
                    .flat_map(|n: &Network| &n.actions)
                    .chain(&actions)
                    .filter(|a| {
                        matches!(
                            a,
                            Action::ExternalIo(crate::external_io::ExternalIoAction::Print { .. })
                        )
                    })
                    .count()
                    >= 2
            {
                return Err(fail(
                    "PR bir programda en fazla iki kez kullanılabilir".into(),
                ));
            }
            actions.push(Action::ExternalIo(
                crate::external_io::ExternalIoAction::parse(op, args).map_err(fail)?,
            ));
            end_line = line;
            continue;
        }
        if matches!(op, "VRRD" | "VRSC") {
            let action =
                Action::AnalogKnob(crate::analog_knob::KnobAction::parse(op, args).map_err(fail)?);
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if matches!(op, "PRUN" | "DPRUN") {
            let action = Action::ParallelRun(
                crate::parallel_run::ParallelRunAction::parse(op, args).map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if op == "FTC" {
            actions.push(Action::Ftc(
                crate::ftc::FtcAction::parse(args).map_err(fail)?,
            ));
            end_line = line;
            continue;
        }
        if op == "DMVRW" {
            actions.push(Action::Dmv(
                crate::dmv_comm::DmvAction::parse(op, args).map_err(fail)?,
            ));
            end_line = line;
            continue;
        }
        if matches!(
            op,
            "DPUCONF"
                | "PUSTAT"
                | "DPUPLS"
                | "DPUDRI"
                | "DPUDRA"
                | "DPUZRN"
                | "DPUJOG"
                | "DPUMPG"
                | "DPUCNT"
                | "PUX"
                | "DPULS"
        ) {
            let action = Action::Pu(crate::pu_module::PuAction::parse(op, args).map_err(fail)?);
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if matches!(
            op,
            "CASD"
                | "DDRVIC"
                | "DDRVAC"
                | "DPPMR"
                | "DPPMA"
                | "DCIMR"
                | "DCIMA"
                | "PLSVC"
                | "DPLSVC"
                | "ZRNC"
                | "DZRNC"
                | "ZRNM"
                | "EMER"
        ) {
            actions.push(Action::CoordinatedMotion(
                crate::coordinated_motion::CoordinatedAction::parse(op, args).map_err(fail)?,
            ));
            end_line = line;
            continue;
        }
        if matches!(op, "ETHCONF" | "ETHMSG") {
            actions.push(Action::EthernetConfig(
                crate::ethernet_config::EthernetConfigAction::parse(op, args).map_err(fail)?,
            ));
            end_line = line;
            continue;
        }
        if op == "ETHRS" {
            actions.push(Action::RawEthernet(
                crate::ethernet_raw::RawEthernetAction::parse(op, args).map_err(fail)?,
            ));
            end_line = line;
            continue;
        }
        if op == "ETHRW" {
            actions.push(Action::Ethernet(
                crate::ethernet::EthernetAction::parse(op, args).map_err(fail)?,
            ));
            end_line = line;
            continue;
        }
        if matches!(op, "COPRW" | "COPWL" | "DCOPWL" | "CANRS") {
            actions.push(Action::CanExchange(
                crate::canopen_exchange::ExchangeAction::parse(op, args).map_err(fail)?,
            ));
            end_line = line;
            continue;
        }
        if matches!(op, "INITC" | "ASDON" | "RSTD") {
            actions.push(Action::Canopen(
                crate::canopen::CanopenAction::parse(op, args).map_err(fail)?,
            ));
            end_line = line;
            continue;
        }
        if op == "ASDRW" {
            actions.push(Action::Servo(
                crate::servo_comm::ServoAction::parse(op, args).map_err(fail)?,
            ));
            end_line = line;
            continue;
        }
        if matches!(op, "DPTPO" | "DCLLM" | "DVSPO") {
            actions.push(Action::ExtendedMotion(
                crate::motion_extended::ExtendedMotionAction::parse(op, args).map_err(fail)?,
            ));
            end_line = line;
            continue;
        }
        if matches!(op, "FWD" | "REV" | "STOP" | "RDST" | "RSTEF") {
            actions.push(Action::Drive(
                crate::drive_comm::DriveAction::parse(op, args).map_err(fail)?,
            ));
            end_line = line;
            continue;
        }
        if matches!(op, "RS" | "MODRD" | "MODWR" | "MODRW") {
            if pulse {
                return Err(fail(format!("{op}P belgelenmiş bir komut değil")));
            }
            actions.push(Action::SerialIo(
                crate::serial_io::SerialIoAction::parse(op, args).map_err(fail)?,
            ));
            end_line = line;
            continue;
        }
        if matches!(op, "FROM" | "DFROM" | "TO" | "DTO") {
            let action =
                Action::ModuleIo(crate::module_io::ModuleIoAction::parse(op, args).map_err(fail)?);
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if matches!(
            op,
            "DABSR" | "DZRN" | "PLSV" | "DPLSV" | "DDRVI" | "DDRVA" | "DICF"
        ) {
            if pulse && op != "DICF" {
                return Err(fail(format!("{op}P belgelenmiş bir komut değil")));
            }
            if op == "DABSR"
                && networks
                    .iter()
                    .flat_map(|n: &Network| &n.actions)
                    .chain(&actions)
                    .any(|a| matches!(a, Action::MotionBasic(a) if a.name() == "DABSR"))
            {
                return Err(fail(
                    "DABSR bir programda yalnız bir kez kullanılabilir".into(),
                ));
            }
            let action = Action::MotionBasic(
                crate::motion_basic::MotionAction::parse(op, args).map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if matches!(op, "PLSY" | "DPLSY" | "PWM" | "PLSR" | "DPLSR") {
            if pulse {
                return Err(fail(format!("{op}P belgelenmiş bir komut değil")));
            }
            actions.push(Action::PulseOutput(
                crate::pulse_output::PulseAction::parse(op, args).map_err(fail)?,
            ));
            end_line = line;
            continue;
        }
        if op == "DCNT" {
            actions.push(Action::DoubleCounter(
                crate::counter::CounterAction::parse(op, args).map_err(fail)?,
            ));
            end_line = line;
            continue;
        }
        if matches!(
            op,
            "REF" | "REFF" | "MTR" | "DHSCS" | "DHSCR" | "DHSZ" | "SPD"
        ) {
            let action = if matches!(op, "REF" | "REFF" | "MTR") {
                Action::Refresh(crate::refresh_io::RefreshAction::parse(op, args).map_err(fail)?)
            } else {
                Action::HighSpeed(
                    crate::high_speed::HighSpeedAction::parse(op, args).map_err(fail)?,
                )
            };
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if matches!(op, "PID" | "DPID" | "XCMP" | "YOUT" | "$MOV") {
            let action = match op {
                "PID" | "DPID" => {
                    Action::Pid(crate::pid::PidAction::parse(op, args).map_err(fail)?)
                }
                "XCMP" | "YOUT" => Action::Station(
                    crate::station_compare::StationAction::parse(op, args).map_err(fail)?,
                ),
                _ => Action::StringMove(
                    crate::string_move::StringMoveAction::parse(op, args).map_err(fail)?,
                ),
            };
            if let Action::Pid(new) = &action {
                let mut pids: Vec<_> = networks
                    .iter()
                    .flat_map(|n: &Network| &n.actions)
                    .chain(&actions)
                    .filter_map(|a| {
                        if let Action::Pid(pid) = a {
                            Some(pid.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                pids.push(new.clone());
                crate::pid::PidAction::validate_blocks(&pids).map_err(fail)?;
            }
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if matches!(op, "ANS" | "ANR") {
            let action = Action::Alarm(crate::alarm::AlarmAction::parse(op, args).map_err(fail)?);
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if op == "IST" {
            if networks
                .iter()
                .flat_map(|n| &n.actions)
                .chain(actions.iter())
                .any(|a| matches!(a, Action::Sfc(_)))
            {
                return Err(fail("IST programda yalnızca bir kez kullanılabilir".into()));
            }
            actions.push(Action::Sfc(
                crate::sfc::SfcAction::parse(args).map_err(fail)?,
            ));
            end_line = line;
            continue;
        }
        if matches!(op, "TMR" | "ATMR" | "STMR" | "TTMR") {
            actions.push(Action::SpecialTimer(
                TimerAction::parse(op, &args.iter().map(String::as_str).collect::<Vec<_>>())
                    .map_err(fail)?,
            ));
            end_line = line;
            continue;
        }
        if matches!(op, "HOUR" | "DHOUR") {
            actions.push(Action::Hour(HourAction::parse(op, args).map_err(fail)?));
            end_line = line;
            continue;
        }
        if matches!(op, "TRD" | "TWR") {
            let action = Action::IndexedRtc(
                crate::indexed_rtc::IndexedRtcAction::parse(op, args).map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if matches!(op, "TCMP" | "TZCP" | "TADD" | "TSUB") {
            let action = Action::Time(TimeAction::parse(op, args).map_err(fail)?);
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if matches!(op, "DELAY" | "GPWM" | "CSFO" | "GPS" | "DSPA") {
            let action = match op {
                "DELAY" | "GPWM" | "CSFO" => {
                    Action::Timing(TimingAction::parse(op, args).map_err(fail)?)
                }
                "GPS" => Action::Gps(GpsAction::parse(op, args).map_err(fail)?),
                _ => Action::Solar(SolarAction::parse(op, args).map_err(fail)?),
            };
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if matches!(
            op,
            "SER" | "DSER" | "ALT" | "RAMP" | "DRAMP" | "SORT" | "DSORT" | "DTM"
        ) {
            let action = match op {
                "SER" | "DSER" | "ALT" => {
                    Action::Search(SearchAction::parse(op, args).map_err(fail)?)
                }
                "RAMP" | "DRAMP" | "SORT" | "DSORT" => {
                    Action::RampSort(RampSortAction::parse(op, args).map_err(fail)?)
                }
                _ => Action::Transform(TransformAction::parse(op, args).map_err(fail)?),
            };
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if matches!(
            op,
            "SWAP"
                | "DSWAP"
                | "MVM"
                | "DMVM"
                | "MMOV"
                | "WSUM"
                | "DWSUM"
                | "SCAL"
                | "SCLP"
                | "DSCLP"
                | "CMPT"
                | "DCMPT"
                | "RAND"
                | "DRAND"
        ) {
            let action = if matches!(op, "RAND" | "DRAND") {
                Action::Random(RandomAction::parse(op, args).map_err(fail)?)
            } else {
                Action::Additional(AdditionalAction::parse(op, args).map_err(fail)?)
            };
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if matches!(
            op,
            "DECO" | "ENCO" | "SUM" | "DSUM" | "BON" | "DBON" | "MEAN" | "DMEAN" | "SQR" | "DSQR"
        ) {
            let action = Action::Data(DataAction::parse(op, args).map_err(fail)?);
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if matches!(
            op,
            "ABS"
                | "DABS"
                | "GRY"
                | "DGRY"
                | "GBIN"
                | "DGBIN"
                | "BOUT"
                | "DBOUT"
                | "BSET"
                | "DBSET"
                | "BRST"
                | "DBRST"
        ) {
            let action = Action::Bits(BitAction::parse(op, args).map_err(fail)?);
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if matches!(op, "MUL" | "DIV") {
            let a = CalendarValue::parse(&args[0], false).map_err(fail)?;
            let b = CalendarValue::parse(&args[1], false).map_err(fail)?;
            let dest = MultiplyDestination::parse(&args[2], false).map_err(fail)?;
            let action = if op == "MUL" {
                Action::Multiply(a, b, dest)
            } else {
                Action::Divide(a, b, dest)
            };
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if matches!(op, "MUL16" | "DIV16") {
            let a = CalendarValue::parse(&args[0], false).map_err(fail)?;
            let b = CalendarValue::parse(&args[1], false).map_err(fail)?;
            let dest = CalendarDestination::parse(&args[2], false).map_err(fail)?;
            // The truncated-family matrices omit F in every operand role,
            // including 16-bit sources; do not inherit broad word F support.
            if [&a, &b, dest.value()].into_iter().any(|value| {
                matches!(
                    value,
                    CalendarValue::Index {
                        high_bank: true,
                        ..
                    }
                )
            }) {
                return Err(fail(format!("{op}: F operandı desteklenmez")));
            }
            let action = if op == "MUL16" {
                Action::MultiplyTruncated(a, b, dest)
            } else {
                Action::DivideQuotient(a, b, dest)
            };
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if matches!(op, "BMOV" | "FMOV") {
            let dest = BlockDestination::parse(&args[1], false).map_err(fail)?;
            let count = Operand::parse(&args[2]).map_err(fail)?;
            if let Operand::Constant(n) = count {
                if !(1..=512).contains(&n) {
                    return Err(fail(format!("{op} sayısı 1..512 olmalı")));
                }
                dest.devices(n as u16).map_err(fail)?;
            }
            let action = if op == "BMOV" {
                let source = BlockSource::parse(&args[0]).map_err(fail)?;
                source.validate_pair(&dest).map_err(fail)?;
                if let Operand::Constant(n) = count {
                    source.devices(n as u16).map_err(fail)?;
                }
                Action::BlockMove(source, dest, count)
            } else {
                let Operand::Constant(n) = count else {
                    return Err(fail("FMOV sayısı K/H sabiti olmalı".into()));
                };
                Action::Fill(
                    CalendarValue::parse(&args[0], false).map_err(fail)?,
                    dest,
                    n as u16,
                )
            };
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if op == "XCH" {
            let action = Action::Exchange(
                CalendarDestination::parse(&args[0], false).map_err(fail)?,
                CalendarDestination::parse(&args[1], false).map_err(fail)?,
            );
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if matches!(
            op,
            "ADD"
                | "SUB"
                | "INC"
                | "DEC"
                | "NEG"
                | "MOV"
                | "CML"
                | "WAND"
                | "WOR"
                | "WXOR"
                | "BCD"
                | "BIN"
        ) {
            let target_index = match op {
                "INC" | "DEC" | "NEG" => 0,
                "MOV" | "CML" | "BCD" | "BIN" => 1,
                _ => 2,
            };
            let dest = CalendarDestination::parse(&args[target_index], false).map_err(fail)?;
            let action = if matches!(op, "ADD" | "SUB" | "WAND" | "WOR" | "WXOR") {
                let a = CalendarValue::parse(&args[0], false).map_err(fail)?;
                let b = CalendarValue::parse(&args[1], false).map_err(fail)?;
                match op {
                    "ADD" => Action::Add(a, b, dest),
                    "SUB" => Action::Sub(a, b, dest),
                    _ => Action::Logic(
                        match op {
                            "WAND" => WordLogic::And,
                            "WOR" => WordLogic::Or,
                            _ => WordLogic::Xor,
                        },
                        a,
                        b,
                        dest,
                    ),
                }
            } else if matches!(op, "MOV" | "CML" | "BCD" | "BIN") {
                let source = CalendarValue::parse(&args[0], false).map_err(fail)?;
                match op {
                    "MOV" => Action::Move(source, dest),
                    "CML" => Action::Complement(source, dest),
                    _ => {
                        if source.constant().is_some() {
                            return Err(fail(
                                "BCD/BIN kaynağı Kn grubu, T/C/D veya E/F olmalı".into(),
                            ));
                        }
                        Action::Convert(
                            if op == "BCD" {
                                WordConversion::Bcd
                            } else {
                                WordConversion::Bin
                            },
                            source,
                            dest,
                        )
                    }
                }
            } else {
                Action::Unary(
                    match op {
                        "INC" => WordUnary::Inc,
                        "DEC" => WordUnary::Dec,
                        _ => WordUnary::Neg,
                    },
                    dest,
                )
            };
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if crate::wide::WideOp::parse(op).is_some() {
            let action = Action::Wide(WideAction::parse(op, args).map_err(fail)?);
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        if matches!(
            op,
            "ROR"
                | "ROL"
                | "RCR"
                | "RCL"
                | "DROR"
                | "DROL"
                | "DRCR"
                | "DRCL"
                | "SFTR"
                | "SFTL"
                | "WSFR"
                | "WSFL"
                | "SFWR"
                | "SFRD"
                | "ZRST"
                | "SMOV"
        ) {
            let action = Action::Shift(ShiftAction::parse(op, args).map_err(fail)?);
            actions.push(if pulse {
                Action::Pulse(Box::new(action))
            } else {
                action
            });
            end_line = line;
            continue;
        }
        let dest_idx = match op {
            "ZCP" => 3,
            "CMP" => 2,
            _ => 0,
        };
        let d: Device = args[dest_idx].parse().map_err(fail)?;
        let action = match op {
            "PLS" | "PLF" => {
                if !matches!(d.area, Area::Y | Area::M) {
                    return Err(fail("PLS/PLF hedefi yalnız Y/M olabilir".into()));
                }
                Action::EdgeOut(d, op == "PLS")
            }
            "ZCP" => {
                comparison_destinations(d).map_err(fail)?;
                Action::ZoneCompare(
                    CalendarValue::parse(&args[0], false).map_err(fail)?,
                    CalendarValue::parse(&args[1], false).map_err(fail)?,
                    CalendarValue::parse(&args[2], false).map_err(fail)?,
                    d,
                )
            }
            "CMP" => {
                comparison_destinations(d).map_err(fail)?;
                Action::Compare(
                    CalendarValue::parse(&args[0], false).map_err(fail)?,
                    CalendarValue::parse(&args[1], false).map_err(fail)?,
                    d,
                )
            }
            "OUT" | "SET" if !d.writable_bit() => {
                return Err(fail("Çıkış Y, M veya S olmalı".into()));
            }
            "RST" if !d.writable_bit() && !matches!(d.area, Area::T | Area::C) => {
                return Err(fail("RST hedefi Y/M/S/T/C olmalı".into()));
            }
            "OUT" => Action::Out(d),
            "SET" => Action::Set(d),
            "RST" => Action::Reset(d),
            "TMR" | "CNT" => {
                if (op == "TMR" && (d.area != Area::T || d.index > 127))
                    || (op == "CNT" && (d.area != Area::C || d.index > 111))
                {
                    return Err(fail(
                        "TMR: T0..T126 = 100 ms, T127 = 1 ms; CNT: C0..C111".into(),
                    ));
                }
                let k = args[1]
                    .strip_prefix('K')
                    .and_then(|s| s.parse::<u16>().ok())
                    .filter(|k| (1..=32767).contains(k))
                    .ok_or_else(|| fail("Preset K1..K32767 olmalı".into()))?;
                if !timers.insert(d) {
                    return Err(fail(format!("{d} için birden fazla TMR/CNT tanımı var")));
                }
                if op == "TMR" {
                    Action::Timer(d, k)
                } else {
                    Action::Counter(d, k)
                }
            }
            _ => unreachable!(),
        };
        actions.push(if pulse {
            Action::Pulse(Box::new(action))
        } else {
            action
        });
        end_line = line;
    }
    if !ended {
        return Err(Diagnostic {
            line: source.lines().count().max(1),
            message: "Program END ile bitmeli".into(),
        });
    }
    let mut devices = BTreeSet::new();
    for step in &flow {
        if let Flow::Evaluate(condition) = step {
            condition.devices(&mut devices);
        }
        if let Flow::Control { op, .. } = step {
            op.devices(&mut devices);
        }
    }
    for n in &networks {
        n.condition.devices(&mut devices);
        for a in &n.actions {
            if let Some(device) = a.written_device() {
                devices.insert(device);
            }
            a.source_devices(&mut devices);
        }
    }
    let mut program = Program {
        source_is_st: false,
        execution_plan: Default::default(),
        native_program_words: 0,
        native_action_pc: Vec::new(),
        flow,
        networks,
        devices,
        instructions,
    };
    match crate::es2_encoder::local_execution_layout(&program) {
        Ok(layout) => {
            program.native_action_pc = layout.action_pc;
            program.native_program_words = layout.word_count;
            program.execution_plan = layout.execution_plan;
        }
        Err(message) if program.requires_vm() => {
            return Err(Diagnostic {
                line: end_line,
                message,
            });
        }
        Err(_) => {}
    }
    Ok(program)
}

pub fn compile(source: &str) -> Result<Program, Diagnostic> {
    if crate::st::is_st_source(source) {
        compile_st(source)
    } else {
        compile_il(source)
    }
}

/// Compile explicit Instruction List without language detection.
pub fn compile_il(source: &str) -> Result<Program, Diagnostic> {
    if source.len() > 128 * 1024 {
        return Err(Diagnostic {
            line: 1,
            message: "Program 128 KiB sınırını aşıyor".into(),
        });
    }
    compile_inner(source)
}

/// Compile Structured Text through the same typed IL execution and native encoder.
pub fn compile_st(source: &str) -> Result<Program, Diagnostic> {
    compile_st_with(source, &crate::st::LowerOptions::default())
}
/// ST with the project's allocation ranges and global symbols.
pub fn compile_st_with(
    source: &str,
    options: &crate::st::LowerOptions,
) -> Result<Program, Diagnostic> {
    if source.len() > 128 * 1024 {
        return Err(Diagnostic {
            line: 1,
            message: "Program 128 KiB sınırını aşıyor".into(),
        });
    }
    let lowered = crate::st::lower_with(source, options)?;
    let mut program = compile_il(&lowered.il).map_err(|mut diagnostic| {
        diagnostic.line = lowered
            .line_map
            .get(diagnostic.line.saturating_sub(1))
            .copied()
            .unwrap_or(1);
        diagnostic
    })?;
    program.source_is_st = true;
    // Source diagnostics and highlighting refer to the original ST editor.
    for network in &mut program.networks {
        network.line = lowered
            .line_map
            .get(network.line.saturating_sub(1))
            .copied()
            .unwrap_or(1);
        network.end_line = lowered
            .line_map
            .get(network.end_line.saturating_sub(1))
            .copied()
            .unwrap_or(network.line);
    }
    for flow in &mut program.flow {
        if let Flow::Control { line, .. } = flow {
            *line = lowered
                .line_map
                .get(line.saturating_sub(1))
                .copied()
                .unwrap_or(1);
        }
    }
    for node in &mut program.execution_plan.nodes {
        if let crate::execution_plan::PlanOp::Control { line, .. } = &mut node.op {
            *line = lowered
                .line_map
                .get(line.saturating_sub(1))
                .copied()
                .unwrap_or(1);
        }
    }
    Ok(program)
}

#[derive(Debug, Clone, Default)]
pub struct Memory {
    /// Virtual physical Y levels, separate from the scan's logical output image.
    pub physical_outputs: BTreeMap<Device, bool>,
    pub refresh_io: crate::refresh_io::RefreshState,
    pub external_io: crate::external_io::ExternalIoState,
    pub motion_basic: crate::motion_basic::MotionBasicState,
    pub motion_extended: crate::motion_extended::MotionExtendedState,
    pub module_io: crate::module_io::VirtualModuleBus,
    pub serial_io: crate::serial_io::SerialBus,
    pub dmv_comm: crate::dmv_comm::DmvState,
    pub ethernet: crate::ethernet::EthernetState,
    pub ethernet_raw: crate::ethernet_raw::RawEthernetState,
    pub canopen: crate::canopen::CanopenNetwork,
    pub can_exchange: crate::canopen_exchange::CanExchangeState,
    pub coordinated_motion: crate::coordinated_motion::CoordinatedState,
    pub pu_module: crate::pu_module::PuState,
    pub analog_knob: crate::analog_knob::KnobState,
    pub pulse_output: crate::pulse_output::PulseState,
    pub high_speed: crate::high_speed::HighSpeedState,
    pub counters: crate::counter::CounterState,
    pub interrupts: crate::interrupt::InterruptState,
    pub watchdog: crate::interrupt::WatchdogState,
    pub pid: crate::pid::PidState,
    pub ftc: crate::ftc::FtcState,
    pub ethernet_config: crate::ethernet_config::EthernetConfigState,
    pub station_compare: crate::station_compare::StationState,
    /// Power-clock and END-armed high-speed timer domains, with explicit profiles.
    pub system_relays: crate::system_relays::ClockState,
    /// Explicit external supply classification and error LED; M1019 stays in bits.
    pub power_monitor: crate::power_monitor::PowerState,
    pub sfc: crate::sfc::SfcState,
    pub special_timers: crate::special_timers::TimerState,
    pub file_memory: crate::file_memory::FileMemoryState,
    pub drum: crate::drum::DrumState,
    pub hour_meter: crate::hour_meter::HourState,
    pub rtc_clock: crate::rtc_clock::RtcClockState,
    pub rtc_service_schedule: crate::rtc_clock::RtcServiceSchedule,
    /// Explicit virtual clock, physical edge capture and pulse-channel backend.
    pub timing_io: crate::timing_io::TimingState,
    /// Configured virtual COM1 receive state; no device or position is invented.
    pub gps: crate::gps::GpsState,
    pub native_numeric_scratch: Option<i16>,
    pub relay_limit: Option<u16>,
    pub ramp_sort: crate::ramp_sort::RampSortState,
    pub transform_context: crate::data_transform::TransformContext,
    /// Explicitly unavailable backend or unproven out-of-range execution.
    /// No PLC error flag is invented for these cases.
    pub execution_limitations: BTreeSet<String>,
    /// Reproducible local RAND state; the original also mutates its seed from
    /// wall-clock timing, so independent native random streams need not match.
    pub random_state: Option<u16>,
    /// Disabled DRAND operand effects are an explicit run policy. The bridge
    /// selects installed compatibility only for exact verified image hashes.
    pub random_tail_profile: crate::random_tail::RandomTailProfile,
    pub calendar_tail_profile: crate::calendar_tail::CalendarTailProfile,
    /// Exact installed-image comparison behavior, selected by the bridge's
    /// digest allowlist; standalone execution uses the current ES2 profile.
    pub compare_table_profile: crate::additional_processing::CompareTableProfile,
    /// DEBCD's hidden/visible scratch differs by exact executable, not model
    /// capacity alone. Standalone uses current ES2's private scratch behavior.
    pub decimal_scratch_profile: crate::float_math::DecimalScratchProfile,
    /// Inclusive D boundary for native model-specific partial source spans.
    /// Standalone execution defaults to the full D0..D9999 address space.
    pub data_register_limit: Option<u16>,
    native_pc: u16,
    wide_counters: BTreeMap<Device, i32>,
    branch_values: BTreeMap<usize, bool>,
    bits: BTreeMap<Device, bool>,
    /// Tracks writes, including same-value writes, to the completion flag shared
    /// by MTR and pulse commands. An old MTR pulse must not clear a newer writer.
    completion_flag_revision: u64,
    words: BTreeMap<Device, i16>,
    pub word_counters: crate::word_counter::WordCounterState,
    contact_edges: BTreeMap<usize, bool>,
    contact_pulses: BTreeMap<usize, bool>,
}
impl Memory {
    pub fn physical_output(&self, device: Device) -> bool {
        self.physical_outputs.get(&device).copied().unwrap_or(false)
    }
    pub(crate) fn set_physical_output(&mut self, device: Device, high: bool) {
        if device.area == Area::Y && device.index <= 255 {
            self.physical_outputs
                .insert(device, high && !self.system_bit(1034));
        }
    }
    pub fn native_instruction_pc(&self) -> u16 {
        self.native_pc
    }
    pub fn system_device(index: u16) -> Device {
        Device {
            area: Area::M,
            index,
        }
    }
    pub fn system_bit(&self, index: u16) -> bool {
        self.bit(Self::system_device(index))
    }
    pub fn set_system(&mut self, index: u16, value: bool) {
        self.write_bit_value(Self::system_device(index), value);
    }
    pub(crate) fn completion_flag_revision(&self) -> u64 {
        self.completion_flag_revision
    }
    fn write_bit_value(&mut self, device: Device, value: bool) {
        if device == Self::system_device(1034) && value {
            // The global output inhibit also covers pins owned by timed engines.
            // Logical image backup/restoration remains at the SFC service boundary.
            self.physical_outputs
                .values_mut()
                .for_each(|high| *high = false);
        }
        if device == Self::system_device(1029) {
            self.completion_flag_revision = self.completion_flag_revision.wrapping_add(1);
        }
        self.bits.insert(device, value);
        if device == Self::system_device(1019) {
            crate::power_monitor::PowerState::control_written(self);
        }
    }
    pub fn truncated_product_carry(&mut self, overflow: bool) {
        // The native MUL16 handler clears carry even with a latched
        // operation error, but only set it when both error flags are clear.
        self.set_system(1022, false);
        if overflow && !self.system_bit(1067) && !self.system_bit(1068) {
            self.set_system(1022, true);
        }
    }
    pub fn error_devices(out: &mut BTreeSet<Device>) {
        out.extend([
            Self::system_device(1067),
            Self::system_device(1068),
            Device {
                area: Area::D,
                index: 1067,
            },
            Device {
                area: Area::D,
                index: 1068,
            },
        ]);
    }
    pub fn operation_error(&mut self, code: i16, pc_offset: u16) {
        if !self.system_bit(1067) {
            self.set_system(1067, true);
            self.set_word(
                Device {
                    area: Area::D,
                    index: 1067,
                },
                code,
            );
        }
        if !self.system_bit(1068) {
            self.set_system(1068, true);
            self.set_word(
                Device {
                    area: Area::D,
                    index: 1068,
                },
                self.native_pc.wrapping_add(pc_offset) as i16,
            );
        }
    }
    pub fn arithmetic_flags(&mut self, a: i32, b: i32, raw: i32, subtract: bool, result: i32) {
        self.set_system(1020, result == 0);
        self.set_system(
            1021,
            a < 0 && (if subtract { b > 0 } else { b < 0 }) && raw >= 0,
        );
        self.set_system(
            1022,
            a > 0 && (if subtract { b < 0 } else { b > 0 }) && raw < 0,
        );
    }
    pub fn double_word(&self, d: Device) -> i32 {
        if d.area == Area::C && d.index == 199 {
            return i32::from(self.word(d) as u16);
        }
        if d.area == Area::C && d.index >= 200 {
            return self.wide_counters.get(&d).copied().unwrap_or(0);
        }
        i32::from(self.word(d) as u16)
            | (i32::from(self.word(Device {
                area: d.area,
                index: d.index + 1,
            })) << 16)
    }
    pub fn set_double_word(&mut self, d: Device, value: i32) {
        let before = (d.area == Area::C && d.index >= 200).then(|| self.double_word(d));
        self.set_double_word_raw(d, value);
        if let Some(before) = before
            && before != value
            && self.high_speed.compare_changes == crate::high_speed::CompareChanges::AllValueChanges
        {
            let active = self
                .counters
                .active_counters()
                .map(|index| Device {
                    area: Area::C,
                    index,
                })
                .collect::<Vec<_>>();
            let at_us = self.timing_io.now_us;
            let down = self.system_bit(1000 + d.index);
            crate::high_speed::HighSpeedState::counter_changed(
                self,
                d,
                before,
                value,
                at_us,
                down,
                crate::high_speed::CounterChange::Load,
                &active,
            );
        }
    }
    /// Backend commits emit their own Count/Reset event after value and contact
    /// are both committed; they must not emit a duplicate generic Load event.
    pub(crate) fn set_double_word_raw(&mut self, d: Device, value: i32) {
        if d.area == Area::C && d.index == 199 {
            self.set_word(d, value as i16);
            return;
        }
        if d.area == Area::C && d.index >= 200 {
            self.wide_counters.insert(d, value);
        } else {
            self.set_word(d, value as i16);
            self.set_word(
                Device {
                    area: d.area,
                    index: d.index + 1,
                },
                (value >> 16) as i16,
            );
        }
    }
    fn sample_edge(&mut self, id: usize, rising: bool, now: bool) -> bool {
        let was = self.contact_edges.insert(id, now).unwrap_or(false);
        let pulse = if rising { now && !was } else { was && !now };
        self.contact_pulses.insert(id, pulse);
        pulse
    }
    pub fn bit(&self, d: Device) -> bool {
        self.bits.get(&d).copied().unwrap_or(false)
    }
    pub fn word(&self, d: Device) -> i16 {
        self.words
            .get(&d)
            .copied()
            .unwrap_or(if d.area == Area::D && d.index == 1000 {
                200
            } else if d.area == Area::D && d.index == 1022 {
                4
            } else if d.area == Area::D && d.index == 1020 {
                10
            } else if d.area == Area::D
                && matches!(d.index, 1131 | 1132 | 1340 | 1343 | 1348..=1353 | 1379..=1382)
            {
                // PDF system D tables: pulse ratio, start/end Hz and ramp times.
                // Explicitly written zero remains zero in the map above.
                100
            } else {
                0
            })
    }
    /// A stored value, excluding context-specific firmware reset defaults.
    pub fn stored_word(&self, d: Device) -> Option<i16> {
        self.words.get(&d).copied()
    }
    pub fn set_word(&mut self, d: Device, v: i16) {
        if d.area == Area::D
            && matches!(d.index, 1178 | 1179)
            && self.analog_knob.profile != crate::analog_knob::KnobProfile::Unconfigured
        {
            return;
        }
        self.words.insert(d, v);
    }
    pub(crate) fn publish_analog_knob(&mut self, channel: u8, raw: u8) {
        if channel < 2 {
            self.words.insert(
                Device {
                    area: Area::D,
                    index: 1178 + u16::from(channel),
                },
                i16::from(raw),
            );
        }
    }
    pub fn set_input(&mut self, d: Device, v: bool) {
        if d.area == Area::X {
            let previous = self
                .timing_io
                .physical_inputs
                .get(&d)
                .copied()
                .unwrap_or_else(|| self.bit(d));
            self.timing_io.physical_inputs.insert(d, v);
            let now = self.timing_io.now_us;
            crate::high_speed::HighSpeedState::advance_to(self, now);
            crate::pulse_output::PulseState::advance(self, now);
            self.refresh_io.seed_input_image(d, v, now);
            if !self.motion_basic.suppresses_external_irq(d)
                && let Err(error) =
                    self.interrupts
                        .input_edge(d, previous, v, self.timing_io.now_us)
            {
                self.execution_limitations.insert(error);
            }
            crate::counter::CounterState::input_edge(self, d, previous, v, now);
            crate::high_speed::HighSpeedState::input_edge(self, d, previous, v, now);
            crate::pulse_output::PulseState::input_edge(self, d, previous, v, now);
            crate::external_io::ExternalIoState::input_edge(self, d, previous, v, now);
            crate::motion_basic::MotionBasicState::input_edge(self, d, previous, v, now);
            crate::motion_extended::MotionExtendedState::input_edge(self, d, previous, v, now);
            self.bits.insert(d, v);
        }
    }
    /// A process-image refresh is not a second physical input transition.
    pub(crate) fn set_input_image(&mut self, d: Device, v: bool) {
        if d.area == Area::X {
            self.bits.insert(d, v);
        }
    }
    /// Populate a captured contact image without PLC write permissions or
    /// simulated input edges. Clock phase/seed state is deliberately separate.
    pub fn set_snapshot_bit(&mut self, d: Device, v: bool) {
        if d.bit() {
            self.bits.insert(d, v);
        }
    }
    pub fn set_internal(&mut self, d: Device, v: bool) {
        if d.writable_bit() {
            self.write_bit_value(d, v);
        }
    }
    /// Operator write from a watch table: X goes through the input image, other bit
    /// devices are set directly.
    pub fn write_bit(&mut self, d: Device, v: bool) {
        if d.area == Area::X {
            self.set_input(d, v);
        } else {
            self.set_bit(d, v);
        }
    }
    pub(crate) fn set_bit(&mut self, d: Device, v: bool) {
        if d.bit() {
            self.write_bit_value(d, v);
        }
    }
    pub(crate) fn reset(&mut self, d: Device) {
        if d.area == Area::T {
            crate::special_timers::TimerState::visit_reset(self, d, true);
            return;
        }
        self.reset_value(d);
    }
    /// ZRST clears the visible value/contact while preserving a pending timer
    /// enable. RST additionally resets that timer's scheduler state.
    pub(crate) fn reset_value(&mut self, d: Device) {
        if d.area == Area::C {
            crate::counter::CounterState::note_reset(self, d);
        }
        if d.area == Area::T {
            self.timing_io.arm_plc_timer(d, false);
            if self.timing_io.ordered_plc_timers() {
                crate::special_timers::TimerState::reset_virtual_timer(self, d);
            }
        }
        self.wide_counters.remove(&d);
        self.write_bit_value(d, false);
        if matches!(d.area, Area::T | Area::C | Area::D) {
            // Clearing a register writes zero. Removing a cold-default entry
            // would incorrectly restore D1000=200/D1020=10/D1022=4.
            self.words.insert(d, 0);
            /* CNT edge history remains until its next evaluation. */
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Simulator {
    /// Last visited input power for each source action, for faithful serial
    /// Ladder projection. Unvisited actions remain false for this scan.
    pub action_power: Vec<Vec<bool>>,
    pub control: crate::program_control::ControlState,
    pub last_scan: ScanStatus,
    control_cursor: Option<scan_vm::ScanCursor>,
    pub memory: Memory,
    pub scans: u64,
    pub elapsed_ms: u64,
    pub power: Vec<bool>,
    // Pulse state belongs to an instruction occurrence, not its destination.
    // Compile/reset/project changes create a fresh Simulator.
    action_edges: BTreeMap<usize, bool>,
    // A fresh per-visit native control outcome; never inferred from sticky
    // error registers. Consumed by the source VM immediately after the visit.
    action_pc_override: Option<u16>,
    // Dynamic BMOV ranges are observed at execution time, before later
    // instructions can change the count. Keep earlier ranges for hold checks.
    pub observed_devices: BTreeSet<Device>,
}
impl Simulator {
    pub(super) fn check_clock_inputs(&mut self, program: &Program) -> Result<(), String> {
        if program.devices.contains(&Memory::system_device(1019)) {
            crate::power_monitor::PowerState::observe(&mut self.memory);
        }
        if program
            .devices
            .iter()
            .any(|d| d.area == Area::M && (1011..=1014).contains(&d.index))
        {
            crate::system_relays::ClockState::require_clock_observation(&mut self.memory)?;
        }
        Ok(())
    }
    /// A new RUN/stepped run exposes05 to the first body. The owned installed
    /// event-loop endpoint exposes09 after END; confirmed on both models.
    pub(crate) fn seed_running_flags(&mut self) {
        for (n, value) in [(1000, true), (1001, false), (1002, true), (1003, false)] {
            self.memory.set_system(n, value);
        }
    }
    pub(super) fn begin_running_scan(&mut self) {
        crate::system_relays::ClockState::set_running(&mut self.memory, true);
        crate::system_relays::ClockState::service(&mut self.memory);
        if self.scans == 0 {
            self.seed_running_flags();
        } else {
            self.finish_running_flags();
        }
    }
    pub(super) fn finish_running_flags(&mut self) {
        for (n, value) in [(1000, true), (1001, false), (1002, false), (1003, true)] {
            self.memory.set_system(n, value);
        }
    }
    /// CPU/RAM restart preserves storage, external modules and serial configuration.
    pub fn restart_preserving_file_store(&mut self) {
        let random_tail_profile = self.memory.random_tail_profile;
        let calendar_tail_profile = self.memory.calendar_tail_profile;
        let rtc_service_schedule = self.memory.rtc_service_schedule;
        let power_monitor = self.memory.power_monitor.restart_for_power_on();
        let mut file_memory = std::mem::take(&mut self.memory.file_memory);
        let mut module_io = std::mem::take(&mut self.memory.module_io);
        let mut serial_io = std::mem::take(&mut self.memory.serial_io);
        let mut dmv_comm = std::mem::take(&mut self.memory.dmv_comm);
        let mut ethernet = std::mem::take(&mut self.memory.ethernet);
        let mut ethernet_raw = std::mem::take(&mut self.memory.ethernet_raw);
        let mut canopen = std::mem::take(&mut self.memory.canopen);
        let mut can_exchange = std::mem::take(&mut self.memory.can_exchange);
        let mut coordinated_motion = std::mem::take(&mut self.memory.coordinated_motion);
        let mut pu_module = std::mem::take(&mut self.memory.pu_module);
        let mut ftc = std::mem::take(&mut self.memory.ftc);
        let mut ethernet_config = std::mem::take(&mut self.memory.ethernet_config);
        dmv_comm.reset_instruction_state();
        ethernet.reset_instruction_state();
        ethernet_raw.reset_instruction_state();
        canopen.reset_instruction_state();
        can_exchange.reset_instruction_state();
        coordinated_motion.reset_instruction_state();
        pu_module.reset_instruction_state();
        ftc.reset_instruction_state();
        ethernet_config.reset_instruction_state();
        let analog_knob = std::mem::take(&mut self.memory.analog_knob);
        let mut motion_extended = std::mem::take(&mut self.memory.motion_extended);
        motion_extended.reset_instruction_state();
        file_memory.reset_instruction_state();
        let module_clock_result = module_io.restart_cpu_clock(self.memory.timing_io.now_us);
        module_io.reset_instruction_state();
        let serial_clock_result = serial_io.restart_cpu_clock(self.memory.timing_io.now_us);
        serial_io.reset_instruction_state();
        *self = Self::default();
        self.memory.random_tail_profile = random_tail_profile;
        self.memory.calendar_tail_profile = calendar_tail_profile;
        self.memory.rtc_service_schedule = rtc_service_schedule;
        self.memory.power_monitor = power_monitor;
        crate::power_monitor::PowerState::service(&mut self.memory);
        self.memory.file_memory = file_memory;
        self.memory.module_io = module_io;
        self.memory.serial_io = serial_io;
        self.memory.dmv_comm = dmv_comm;
        self.memory.ethernet = ethernet;
        self.memory.ethernet_raw = ethernet_raw;
        self.memory.canopen = canopen;
        self.memory.can_exchange = can_exchange;
        self.memory.coordinated_motion = coordinated_motion;
        self.memory.pu_module = pu_module;
        self.memory.ftc = ftc;
        self.memory.ethernet_config = ethernet_config;
        crate::pu_module::PuState::apply_power_on_relays(&mut self.memory);
        self.memory.analog_knob = analog_knob;
        self.memory.motion_extended = motion_extended;
        if let Err(error) = module_clock_result {
            self.memory.execution_limitations.insert(error);
        }
        if let Err(error) = serial_clock_result {
            self.memory.execution_limitations.insert(error);
        }
    }
    fn sample_legacy_condition(&mut self, condition: &Expr) -> Option<bool> {
        match condition.sample(&mut self.memory) {
            Ok(value) => Some(value),
            Err(reason) => {
                self.last_scan = ScanStatus::Fault(reason.clone());
                self.memory.execution_limitations.insert(reason);
                self.stop_execution_io();
                None
            }
        }
    }
    pub fn scan(&mut self, program: &Program, scan_ms: u64) {
        if let Err(reason) = self.check_clock_inputs(program) {
            self.stop_execution_io();
            self.last_scan = ScanStatus::Fault(reason);
            return;
        }
        if program.requires_vm() || self.memory.interrupts.header_instruction_cost_us != 0 {
            self.run_plan_scan(program, scan_ms);
            return;
        }
        self.begin_running_scan();
        crate::sfc::SfcState::service_output_inhibit(&mut self.memory);
        if self.memory.native_numeric_scratch.is_none() && program.native_program_words != 0 {
            self.memory.native_numeric_scratch = Some(program.native_program_words as i16);
        }
        self.memory.execution_limitations.clear();
        self.memory.watchdog.begin_scan();
        if let Err(reason) = self.advance_watchdog(0, 0) {
            self.last_scan = ScanStatus::Fault(reason.clone());
            self.memory.execution_limitations.insert(reason);
            return;
        }
        crate::counter::CounterState::begin_scan(&mut self.memory);
        crate::counter::CounterState::set_running(&mut self.memory, true);
        crate::high_speed::HighSpeedState::set_running(&mut self.memory, true);
        crate::pulse_output::PulseState::set_running(&mut self.memory, true);
        crate::pulse_output::PulseState::begin_scan(&mut self.memory);
        crate::external_io::ExternalIoState::begin_scan(&mut self.memory);
        crate::motion_basic::MotionBasicState::begin_scan(&mut self.memory);
        crate::motion_extended::MotionExtendedState::begin_scan(&mut self.memory);
        crate::serial_io::SerialBus::begin_scan(&mut self.memory);
        crate::analog_knob::KnobState::service(&mut self.memory);
        let ordered_timers = program.networks.iter().flat_map(|n| &n.actions).any(|a| {
            let a = if let Action::Pulse(inner) = a {
                inner.as_ref()
            } else {
                a
            };
            matches!(a, Action::Timing(TimingAction::Delay { .. }))
        });
        self.memory.timing_io.configure_plc_timers(ordered_timers);
        crate::timing_io::begin_scan(&mut self.memory);
        crate::pid::PidState::begin_scan(&mut self.memory, scan_ms.saturating_mul(1000));
        let scan_start_us = self.memory.timing_io.now_us;
        self.power.clear();
        self.action_power = program
            .networks
            .iter()
            .map(|n| vec![false; n.actions.len()])
            .collect();
        let mut action_index = 0;
        let mut last_power = false;
        for step in &program.flow {
            let network = match step {
                Flow::Evaluate(condition) => {
                    let Some(value) = self.sample_legacy_condition(condition) else {
                        return;
                    };
                    last_power = value;
                    continue;
                }
                Flow::Capture(id) => {
                    self.memory.branch_values.insert(*id, last_power);
                    continue;
                }
                // The compiler resolves the correct saved frame into Expr::Saved.
                Flow::Restore(_) => continue,
                Flow::Control { .. } => unreachable!(
                    "Control programs dispatch through the resumable VM before this legacy loop"
                ),
                Flow::Network(index) => &program.networks[*index],
            };
            let Some(power) = self.sample_legacy_condition(&network.condition) else {
                return;
            };
            last_power = power;
            self.power.push(power);
            let network_index = if let Flow::Network(index) = step {
                *index
            } else {
                unreachable!()
            };
            for (network_action, action) in network.actions.iter().enumerate() {
                self.action_power[network_index][network_action] = power;
                let pc = program
                    .native_action_pc
                    .get(action_index)
                    .copied()
                    .unwrap_or(0);
                let before = self.memory.timing_io.now_us;
                self.execute_action_visit(action, power, action_index, pc);
                if let Err(reason) =
                    self.advance_watchdog(self.memory.timing_io.now_us.saturating_sub(before), pc)
                {
                    self.last_scan = ScanStatus::Fault(reason.clone());
                    self.memory.execution_limitations.insert(reason);
                    return;
                }
                crate::system_relays::ClockState::service(&mut self.memory);
                crate::sfc::SfcState::service_output_inhibit(&mut self.memory);
                action_index += 1;
            }
        }
        match self.finish_action_scan(scan_start_us, ordered_timers, scan_ms) {
            Ok(()) => self.last_scan = ScanStatus::Complete,
            Err(reason) => {
                self.last_scan = ScanStatus::Fault(reason.clone());
                self.memory.execution_limitations.insert(reason);
            }
        }
    }
}

// Recovered signed overflow behavior (original 0x410F88 / 0x411150), also
// exercised against the original. ES2 does not simply wrap at the sign bit.
fn es2_add(a: i16, b: i16) -> i16 {
    let value = a.wrapping_add(b);
    if a > 0 && b > 0 && value < 0 {
        ((value as u16) & 0x7fff) as i16
    } else if a < 0 && b < 0 && value >= 0 {
        value.wrapping_add(0x8001u16 as i16)
    } else {
        value
    }
}
fn es2_sub(a: i16, b: i16) -> i16 {
    let value = a.wrapping_sub(b);
    if a > 0 && b < 0 && value < 0 {
        ((value as u16) & 0x7fff) as i16
    } else if a < 0 && b > 0 && value >= 0 {
        value.wrapping_add(0x8001u16 as i16)
    } else {
        value
    }
}

#[cfg(test)]
#[path = "calendar_replay_tests.rs"]
mod calendar_replay_tests;
#[cfg(test)]
#[path = "program_control_integration_tests.rs"]
mod program_control_integration_tests;

#[cfg(test)]
mod tests {
    #[test]
    fn matrix_pulse_gates_hold_and_rearm_through_full_scan() {
        let p = super::compile("LD X0\nMBSP D0 D10 K1\nLD X1\nMOV D10 D0\nEND").unwrap();
        let d = |s: &str| s.parse::<super::Device>().unwrap();
        let mut sim = super::Simulator::default();
        sim.memory.set_word(d("D0"), 1);
        sim.memory.set_input(d("X0"), true);
        sim.scan(&p, 20);
        assert_eq!(sim.memory.word(d("D10")), 2);
        sim.memory.set_word(d("D0"), 4);
        sim.scan(&p, 20);
        assert_eq!(sim.memory.word(d("D10")), 2);
        sim.memory.set_input(d("X0"), false);
        sim.scan(&p, 20);
        assert_eq!(sim.memory.word(d("D10")), 2);
        sim.memory.set_input(d("X0"), true);
        sim.scan(&p, 20);
        assert_eq!(sim.memory.word(d("D10")), 8);
    }
    #[test]
    fn file_store_survives_cpu_restart_and_activation_is_rearmed() {
        use crate::file_memory::{FileMemoryProfile, FileModel, FileStore, FirmwareVersion};
        let mut s = super::Simulator::default();
        let store = FileStore::new(
            "restart-test",
            FileMemoryProfile::new(FileModel::Es2, FirmwareVersion::new(3, 44), false).unwrap(),
        )
        .unwrap();
        s.memory.file_memory.configure(store);
        let p = super::compile("LD X0\nMEMW D2000 K0 K1\nEND").unwrap();
        let x0 = "X0".parse().unwrap();
        let data = "D2000".parse().unwrap();
        s.memory.set_input(x0, true);
        s.memory.set_word(data, 77);
        s.scan(&p, 0);
        s.memory.set_word(data, 88);
        s.scan(&p, 0);
        assert_eq!(s.memory.file_memory.store().unwrap().words()[0], 77);
        assert_eq!(
            s.memory.file_memory.store().unwrap().write_transactions(),
            1
        );
        s.restart_preserving_file_store();
        assert_eq!(s.scans, 0);
        assert_eq!(s.memory.word(data), 0);
        assert_eq!(
            s.memory.file_memory.store().unwrap().identity(),
            "restart-test"
        );
        assert!(s.memory.file_memory.store().unwrap().dirty());
        s.memory.set_input(x0, true);
        s.memory.set_word(data, 99);
        s.scan(&p, 0);
        assert_eq!(s.memory.file_memory.store().unwrap().words()[0], 99);
        assert_eq!(
            s.memory.file_memory.store().unwrap().write_transactions(),
            2
        );
    }

    #[test]
    fn local_file_and_unproven_drum_layouts_preserve_later_owner_pcs() {
        let p=super::compile("LD X0\nABSD C200 D0 M0 K1\nINCD D0 C0 M10 K1\nMEMWP D2000 K1 K1\nDMEMRP K1 D2100 K1\nEND").unwrap();
        assert_eq!(p.native_action_pc, [1, 10, 19, 26]);
        assert_eq!(p.native_program_words, 40);
        assert!(crate::es2_encoder::encode(&p).is_err());
        for op in [
            "MEMR", "MEMRP", "DMEMR", "DMEMRP", "MEMW", "MEMWP", "DMEMW", "DMEMWP",
        ] {
            let args = if op.contains("MEMR") {
                "K0 D2000 K1"
            } else {
                "D2000 K0 K1"
            };
            let p = super::compile(&format!("LD X0\n{op} {args}\nEND")).unwrap();
            assert_eq!(
                p.native_program_words,
                if op.starts_with('D') { 15 } else { 9 }
            );
            assert!(
                crate::es2_encoder::encode(&p)
                    .unwrap_err()
                    .contains("yalnız PC")
            );
        }
        for op in ["ABSDP", "DABSDP", "DINCD", "INCDP"] {
            assert!(super::compile(&format!("LD X0\n{op} D0 C0 M0 K1\nEND")).is_err());
        }
    }

    #[test]
    fn delayed_timer_words_follow_activation_order_and_base_time_once() {
        for base_ms in [0, 100] {
            for (body, visible, final_count) in [
                ("TMR T127 K32767\nDELAY K10", 1, 1 + base_ms as i16),
                ("DELAY K10\nTMR T127 K32767", 0, base_ms as i16),
            ] {
                let p = super::compile(&format!("LD X0\n{body}\nMOV T127 D0\nEND")).unwrap();
                let mut s = super::Simulator::default();
                s.memory.set_input("X0".parse().unwrap(), true);
                s.scan(&p, base_ms);
                assert_eq!(s.memory.word("D0".parse().unwrap()), visible);
                assert_eq!(s.memory.word("T127".parse().unwrap()), final_count);
                assert!(!s.memory.bit("T127".parse().unwrap()));
            }
        }
        let p = super::compile("LD X0\nTMR T127 K1\nDELAY K10\nMOV T127 D0\nEND").unwrap();
        let mut s = super::Simulator::default();
        s.memory.set_input("X0".parse().unwrap(), true);
        s.scan(&p, 0);
        assert_eq!(s.memory.word("D0".parse().unwrap()), 1);
        assert!(!s.memory.bit("T127".parse().unwrap()));
        s.scan(&p, 0);
        assert!(s.memory.bit("T127".parse().unwrap()));
        assert_eq!(s.memory.word("T127".parse().unwrap()), 1);
    }

    #[test]
    fn delayed_timer_reset_disable_rearm_and_prior_scan_activity_are_ordered() {
        for reset in ["RST T127", "ZRST T127 T127"] {
            let p = super::compile(&format!("LD X0\nTMR T127 K100\nDELAY K10\nMOV T127 D0\n{reset}\nDELAY K10\nMOV T127 D1\nDELAY K10\nMOV T127 D2\nEND")).unwrap();
            let mut s = super::Simulator::default();
            s.memory.set_input("X0".parse().unwrap(), true);
            s.scan(&p, 0);
            assert_eq!(
                ["D0", "D1", "D2"].map(|d| s.memory.word(d.parse().unwrap())),
                [1, 0, 0]
            );
        }
        let p = super::compile("LD X0\nDELAY K10\nMOV T127 D0\nLD X1\nTMR T127 K100\nLD X0\nDELAY K10\nMOV T127 D1\nEND").unwrap();
        let mut s = super::Simulator::default();
        s.memory.set_input("X0".parse().unwrap(), true);
        s.memory.set_input("X1".parse().unwrap(), true);
        s.scan(&p, 0);
        assert_eq!(s.memory.word("D0".parse().unwrap()), 0);
        assert_eq!(s.memory.word("D1".parse().unwrap()), 1);
        s.memory.set_input("X1".parse().unwrap(), false);
        s.scan(&p, 0);
        assert_eq!(s.memory.word("D0".parse().unwrap()), 2);
        assert_eq!(s.memory.word("D1".parse().unwrap()), 0);
        // Switching to a legacy program clears the ordered armed set before time advances.
        s.memory.set_input("X1".parse().unwrap(), true);
        s.scan(&p, 0);
        let legacy = super::compile("LD X0\nTMR T127 K100\nEND").unwrap();
        s.scan(&legacy, 1);
        assert_eq!(s.memory.word("T127".parse().unwrap()), 2);
    }

    #[test]
    fn delayed_timers_share_prescaler_without_losing_fine_steps_or_event_segments() {
        let p = super::compile("LD X0\nTMR T127 K32767\nTMR T0 K32767\nSET M1148\nDELAY K1\nMOV T127 D0\nMOV T0 D1\nEND").unwrap();
        let mut s = super::Simulator::default();
        s.memory.set_input("X0".parse().unwrap(), true);
        for at in [997, 998, 999, 100001] {
            s.memory
                .timing_io
                .enqueue_input_edge(at, "X4".parse().unwrap(), true)
                .unwrap();
        }
        for _ in 0..20000 {
            s.scan(&p, 0);
        }
        assert_eq!(s.memory.timing_io.now_us, 100000);
        assert_eq!(s.memory.word("D0".parse().unwrap()), 100);
        assert_eq!(s.memory.word("D1".parse().unwrap()), 1);
        assert!(s.memory.bit("X4".parse().unwrap()));
        // A held pulse introduces no extra delay and the base quantum is counted once.
        let pulse = super::compile("LD X0\nTMR T127 K32767\nDELAYP K10\nMOV T127 D0\nEND").unwrap();
        let mut s = super::Simulator::default();
        s.memory.set_input("X0".parse().unwrap(), true);
        s.scan(&pulse, 1);
        s.scan(&pulse, 1);
        assert_eq!(s.memory.timing_io.now_us, 3000);
        assert_eq!(s.memory.word("D0".parse().unwrap()), 2);
        assert_eq!(s.memory.word("T127".parse().unwrap()), 3);
    }

    #[test]
    fn virtual_delay_orders_physical_events_without_refreshing_scan_input_image() {
        let p = super::compile("LD X0\nSET M1148\nDELAY K100\nLD X4\nOUT Y0\nEND").unwrap();
        let mut s = super::Simulator::default();
        s.memory.set_input("X0".parse().unwrap(), true);
        s.memory
            .timing_io
            .enqueue_input_edge(250, "X4".parse().unwrap(), true)
            .unwrap();
        s.scan(&p, 0);
        assert_eq!(s.memory.timing_io.now_us, 500);
        assert!(!s.memory.bit("Y0".parse().unwrap()));
        assert!(s.memory.timing_io.physical_inputs[&"X4".parse().unwrap()]);
        s.scan(&p, 0);
        assert!(s.memory.bit("Y0".parse().unwrap()));
        assert_eq!(s.memory.timing_io.now_us, 1000);
    }
    #[test]
    fn microsecond_delay_carries_into_plc_timer_ticks_and_pulse_occurrence_gating() {
        let p = super::compile("LD X0\nSET M1148\nDELAY K1\nTMR T127 K32767\nEND").unwrap();
        let mut s = super::Simulator::default();
        s.memory.set_input("X0".parse().unwrap(), true);
        for _ in 0..199 {
            s.scan(&p, 0);
        }
        assert_eq!(s.memory.timing_io.now_us, 995);
        assert_eq!(s.memory.word("T127".parse().unwrap()), 0);
        s.scan(&p, 0);
        assert_eq!(s.elapsed_ms, 1);
        assert_eq!(s.memory.word("T127".parse().unwrap()), 1);
        let p = super::compile("LD X0\nSET M1148\nDELAYP K1\nEND").unwrap();
        let mut s = super::Simulator::default();
        s.memory.set_input("X0".parse().unwrap(), true);
        s.scan(&p, 0);
        assert_eq!(s.memory.timing_io.now_us, 5);
        assert!(!s.memory.system_bit(1148));
        s.scan(&p, 0);
        assert_eq!(s.memory.timing_io.now_us, 5);
        assert!(s.memory.system_bit(1148)); // SET executes; held DELAYP does not consume it.
        s.memory.set_input("X0".parse().unwrap(), false);
        s.scan(&p, 0);
        s.memory.set_input("X0".parse().unwrap(), true);
        s.scan(&p, 0);
        assert_eq!(s.memory.timing_io.now_us, 10);
        assert!(!s.memory.system_bit(1148));
    }
    #[test]
    fn virtual_timing_gps_native_rejection_and_documented_forms_are_explicit() {
        for source in [
            "LD X0\nDELAY K1\nEND",
            "LD X0\nDELAYP K1\nEND",
            "LD X0\nGPWM D0 D10 M0\nEND",
            "LD X0\nCSFO X0 D0 D10\nEND",
            "LD X0\nGPS K0 D10\nEND",
        ] {
            let p = super::compile(source).unwrap();
            assert!(crate::es2_encoder::encode(&p).is_err());
            assert_ne!(p.native_program_words, 0);
        }
        for op in [
            "GPWMP D0 D10 M0",
            "CSFOP X0 D0 D10",
            "GPSP K0 D10",
            "DSPAP D0 D300",
        ] {
            assert!(super::compile(&format!("LD X0\n{op}\nEND")).is_err());
        }
        let p = super::compile("LD X0\nDSPA D2000 D3000\nMOV K1 D900\nEND").unwrap();
        assert_eq!(p.native_action_pc, [1, 10]);
        let e = crate::es2_encoder::encode(&p).unwrap();
        assert_eq!(e.words[1], 0x8301);
        assert_eq!(e.next_pc, [1, 10, 15, 0]);
    }
    #[test]
    fn local_only_commands_preserve_later_error_word_offsets() {
        let p = super::compile("LD X0\nMMOV K-1 D0\nASC \"aBcDeFgH\" D10\nRAND K-1 K2 D30\nEND")
            .unwrap();
        assert_eq!(p.native_action_pc, [1, 6, 17]);
        assert!(crate::es2_encoder::encode(&p).is_err());
        let mut sim = super::Simulator::default();
        sim.memory.set_input("X0".parse().unwrap(), true);
        sim.scan(&p, 100);
        assert_eq!(sim.memory.word("D1068".parse().unwrap()), 20);
        assert_eq!(sim.memory.double_word("D0".parse().unwrap()), -1);
        let p = super::compile("LD X0\nDTM D0 D10 K30 K1\nRAND K-1 K2 D30\nEND").unwrap();
        assert_eq!(p.native_action_pc, [1, 10]);
        assert!(crate::es2_encoder::encode(&p).is_err());
        let mut sim = super::Simulator::default();
        sim.memory.set_input("X0".parse().unwrap(), true);
        sim.scan(&p, 100);
        assert_eq!(sim.memory.word("D1068".parse().unwrap()), 13);
    }
    use super::*;
    #[test]
    fn truncated_word_product_writes_one_word_and_gates_carry_on_errors() {
        let p = compile("LD X0\nMUL16 D0 D1 D0\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        for (a, b, error, expected, carry) in [
            (32767, 1, None, 32767, false),
            (-32768, 1, None, -32768, false),
            (32767, 2, None, -2, true),
            (-32768, 2, None, 0, true),
            (-32768, -1, None, -32768, true),
            (32767, 2, Some(1067), -2, false),
            (32767, 2, Some(1068), -2, false),
        ] {
            s.memory.set_word(d("D0"), a);
            s.memory.set_word(d("D1"), b);
            s.memory.set_system(1067, error == Some(1067));
            s.memory.set_system(1068, error == Some(1068));
            for flag in 1020..=1022 {
                s.memory.set_system(flag, true);
            }
            s.scan(&p, 100);
            assert_eq!(s.memory.word(d("D0")), expected);
            assert_eq!(s.memory.word(d("D1")), b, "adjacent source/sentinel");
            assert_eq!(s.memory.system_bit(1022), carry);
            assert!(s.memory.system_bit(1020));
            assert!(s.memory.system_bit(1021));
        }
    }
    #[test]
    fn quotient_only_word_division_preserves_tail_and_reports_native_error_step() {
        let p = compile("LD X0\nDIV16 D0 D1 T255\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.memory.set_word(d("C0"), 901);
        for flag in 1020..=1022 {
            s.memory.set_system(flag, true);
        }
        for (a, b, expected) in [(-103, 5, -20), (103, -5, -20), (-32768, -1, -32768)] {
            s.memory.set_word(d("D0"), a);
            s.memory.set_word(d("D1"), b);
            s.scan(&p, 100);
            assert_eq!(s.memory.word(d("T255")), expected);
            assert_eq!(s.memory.word(d("C0")), 901);
            assert!((1020..=1022).all(|flag| s.memory.system_bit(flag)));
        }
        let old = s.memory.word(d("T255"));
        s.memory.set_word(d("D1"), 0);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("T255")), old);
        assert_eq!(s.memory.word(d("D1067")), 0xe19);
        assert_eq!(
            s.memory.word(d("D1068")),
            (p.native_action_pc[0] + 5) as i16
        );
    }
    #[test]
    fn truncated_word_pulses_rearm_and_accept_single_word_endpoints() {
        let p = compile("LD X0\nMUL16P D0 K2 D0\nDIV16P D2 K3 D2\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_word(d("D0"), 4);
        s.memory.set_word(d("D2"), 81);
        for (input, product, quotient) in [
            (false, 4, 81),
            (true, 8, 27),
            (true, 8, 27),
            (false, 8, 27),
            (true, 16, 9),
        ] {
            s.memory.set_input(d("X0"), input);
            s.scan(&p, 100);
            assert_eq!(s.memory.word(d("D0")), product);
            assert_eq!(s.memory.word(d("D2")), quotient);
        }
        for name in ["MUL16", "DIV16"] {
            for dest in ["D9999", "T255", "C199"] {
                assert!(compile(&format!("LD X0\n{name} K7 K2 {dest}\nEND")).is_ok());
            }
            for dest in ["C200", "X0", "D10000"] {
                assert!(compile(&format!("LD X0\n{name} K7 K2 {dest}\nEND")).is_err());
            }
        }
    }
    #[test]
    fn arithmetic_flags_and_full_width_multiply_preservation() {
        let p = compile("LD X0\nADD H7FFF K1 D0\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert!(s.memory.system_bit(1020));
        assert!(!s.memory.system_bit(1021));
        assert!(s.memory.system_bit(1022));
        let p = compile("LD X0\nMUL K7 K3 D0\nDMUL K7 K3 D2\nEND").unwrap();
        s.memory.set_system(1021, true);
        s.scan(&p, 100);
        assert!(s.memory.system_bit(1020));
        assert!(s.memory.system_bit(1021));
        assert!(s.memory.system_bit(1022));
    }
    #[test]
    fn error_code_and_error_pc_latch_independently() {
        let p=compile("LD X2\nRST M1067\nLD X3\nRST M1068\nLD X0\nDIV K1 K0 D0\nLD X1\nMOV K-1 D2\nBCD D2 D3\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        let first = s.memory.word(d("D1068"));
        assert_eq!(s.memory.word(d("D1067")), 0xe19);
        s.memory.set_input(d("X0"), false);
        s.memory.set_input(d("X1"), true);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D1067")), 0xe19);
        assert_eq!(s.memory.word(d("D1068")), first);
        s.memory.set_input(d("X2"), true);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D1067")), 0xe18);
        assert_eq!(s.memory.word(d("D1068")), first);
        s.memory.set_input(d("X3"), true);
        s.scan(&p, 100);
        assert_ne!(s.memory.word(d("D1068")), first);
    }
    #[test]
    fn pulse_coils_do_not_mutate_following_action_power() {
        let p = compile("LD X0\nINV\nINV\nPLS Y0\nPLF M0\nOUT Y1\nEND").unwrap();
        let mut s = Simulator::default();
        for (input, rise, fall) in [
            (false, false, false),
            (true, true, false),
            (true, false, false),
            (false, false, true),
            (false, false, false),
        ] {
            s.memory.set_input(d("X0"), input);
            s.scan(&p, 100);
            assert_eq!(s.memory.bit(d("Y0")), rise);
            assert_eq!(s.memory.bit(d("M0")), fall);
            assert_eq!(s.memory.bit(d("Y1")), input);
        }
    }
    #[test]
    fn block_move_overlap_is_sequential_and_dynamic_count_is_sampled_once() {
        let p = compile("LD X0\nBMOV D0 D1 D0\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        for (name, value) in [("D0", 3), ("D1", 8), ("D2", 9), ("D3", 10)] {
            s.memory.set_word(d(name), value);
        }
        s.scan(&p, 100);
        for name in ["D1", "D2", "D3"] {
            assert_eq!(s.memory.word(d(name)), 3);
            assert!(s.observed_devices.contains(&d(name)));
        }
        s.memory.set_input(d("X0"), false);
        s.memory.set_word(d("D0"), 1);
        s.scan(&p, 100);
        assert!(
            s.observed_devices.contains(&d("D3")),
            "prior range remains observable"
        );
        assert_eq!(s.memory.word(d("D3")), 3);
    }
    #[test]
    fn block_move_dynamic_invalid_count_and_partial_address_error() {
        let p = compile("LD X0\nBMOV D10 D9999 D0\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.memory.set_word(d("D10"), 17);
        for count in [-1, 0, 513] {
            s.memory.set_word(d("D0"), count);
            s.scan(&p, 100);
            assert_eq!(s.memory.word(d("D9999")), 0);
        }
        s.memory.set_word(d("D0"), 2);
        s.scan(&p, 100);
        assert_eq!(
            s.memory.word(d("D9999")),
            17,
            "native keeps first write before the next address fails"
        );
        assert!(
            s.observed_devices
                .iter()
                .all(|d| d.area != Area::D || d.index <= 9999)
        );
    }
    #[test]
    fn fill_exchange_and_pulse_preserve_read_before_write_semantics() {
        let p = compile("LD X0\nFMOVP D0 D0 K3\nXCHP D0 D10\nXCH D10 D10\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.memory.set_word(d("D0"), 12);
        s.memory.set_word(d("D10"), -7);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D0")), -7);
        assert_eq!(s.memory.word(d("D1")), 12);
        assert_eq!(s.memory.word(d("D2")), 12);
        assert_eq!(s.memory.word(d("D10")), 12);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D0")), -7);
        assert_eq!(s.memory.word(d("D1")), 12);
        s.memory.set_input(d("X0"), false);
        s.scan(&p, 100);
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D0")), 12);
        assert_eq!(s.memory.word(d("D1")), -7);
    }
    #[test]
    fn bcd_bin_roundtrip_and_invalid_values_preserve_destination() {
        for value in [0, 1, 9, 10, 99, 100, 999, 1000, 9999] {
            let packed = WordConversion::Bcd.apply(value).unwrap();
            assert_eq!(WordConversion::Bin.apply(packed), Some(value));
        }
        let p = compile("LD X0\nBCD D0 D1\nBIN D2 D3\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        for (name, value) in [("D0", -1), ("D1", 77), ("D2", 0x1a23), ("D3", 88)] {
            s.memory.set_word(d(name), value);
        }
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D1")), 77);
        assert_eq!(s.memory.word(d("D3")), 88);
        s.memory.set_word(d("D0"), 10000);
        s.memory.set_word(d("D2"), 0x9999u16 as i16);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D1")), 77);
        assert_eq!(s.memory.word(d("D3")), 9999);
    }
    #[test]
    fn bulk_range_validation_and_tracking() {
        let p =
            compile("LD X0\nBMOV D4997 D10 K3\nFMOV K0 T254 K2\nXCH C199 D0\nBCD C199 D20\nEND")
                .unwrap();
        for name in [
            "D4997", "D4998", "D4999", "D10", "D11", "D12", "T254", "T255", "C199",
        ] {
            assert!(p.devices.contains(&d(name)), "{name}");
        }
        for command in [
            "BMOV D0 D1 K0",
            "BMOV D0 D1 K513",
            "BMOV D9999 D0 K2",
            "BMOV C199 D0 K2",
            "FMOV K1 D9999 K2",
            "FMOV K1 D0 D1",
            "XCH K1 D0",
            "BCD K1 D0",
            "BIN C200 D0",
            "FMOV K1 X0 K1",
        ] {
            assert!(
                compile(&format!("LD X0\n{command}\nEND")).is_err(),
                "{command}"
            );
        }
    }
    fn d(s: &str) -> Device {
        s.parse().unwrap()
    }
    #[test]
    fn multiply_writes_exact_signed_32_bit_product_as_low_high_words() {
        let p =
            compile("LD X0\nMUL H8000 H8000 D0\nMUL H7FFF H7FFF D2\nMUL K-7 K3 D4\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert_eq!(
            (s.memory.word(d("D0")) as u16, s.memory.word(d("D1")) as u16),
            (0, 0x4000)
        );
        assert_eq!(
            (s.memory.word(d("D2")) as u16, s.memory.word(d("D3")) as u16),
            (1, 0x3fff)
        );
        assert_eq!((s.memory.word(d("D4")), s.memory.word(d("D5"))), (-21, -1));
    }
    #[test]
    fn divide_truncates_toward_zero_and_matches_native_overflow_result() {
        let p = compile("LD X0\nDIV K-7 K3 D0\nDIV K7 K-3 D2\nDIV H8000 K-1 D4\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert_eq!((s.memory.word(d("D0")), s.memory.word(d("D1"))), (-2, -1));
        assert_eq!((s.memory.word(d("D2")), s.memory.word(d("D3"))), (-2, 1));
        assert_eq!(
            (s.memory.word(d("D4")), s.memory.word(d("D5"))),
            (-32768, 0)
        );
    }
    #[test]
    fn paired_arithmetic_reads_aliases_before_writing_and_preserves_on_zero() {
        let p = compile("LD X0\nDIV D0 D1 D0\nDIV K7 D2 D10\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_word(d("D0"), -7);
        s.memory.set_word(d("D1"), 3);
        s.memory.set_word(d("D10"), 123);
        s.memory.set_word(d("D11"), -7);
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert_eq!((s.memory.word(d("D0")), s.memory.word(d("D1"))), (-2, -1));
        assert_eq!(
            (s.memory.word(d("D10")), s.memory.word(d("D11"))),
            (123, -7)
        );
        s.memory.set_input(d("X0"), false);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D0")), -2);
    }
    #[test]
    fn mul_div_pulse_pair_rearms_and_tracks_implicit_second_word() {
        let p = compile("LD X0\nMULP D0 K-3 D4998\nDIVP D0 K2 D4096\nEND").unwrap();
        for name in ["D4998", "D4999", "D4096", "D4097"] {
            assert!(p.devices.contains(&d(name)));
        }
        let mut s = Simulator::default();
        s.memory.set_word(d("D0"), 3);
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        s.memory.set_word(d("D0"), 4);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D4998")), -9);
        assert_eq!(s.memory.word(d("D4096")), 1);
        assert_eq!(s.memory.word(d("D4097")), 1);
        s.memory.set_input(d("X0"), false);
        s.scan(&p, 100);
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D4998")), -12);
        assert_eq!(s.memory.word(d("D4096")), 2);
        assert_eq!(s.memory.word(d("D4097")), 0);
    }
    #[test]
    fn paired_arithmetic_rejects_wrong_areas_overflow_and_wide_counters() {
        for body in [
            "MUL K1 K2 D9999",
            "DIV K1 K2 Y0",
            "MULP K1 K2 M0",
            "DIV K1 C200 D0",
            "MUL K1 D0",
        ] {
            assert!(compile(&format!("LD X0\n{body}\nEND")).is_err(), "{body}");
        }
    }
    #[test]
    fn cmp_replaces_three_results_but_disabled_and_held_cmpp_preserve_them() {
        let p = compile("LD X0\nCMP D0 K0 M10\nLD X1\nCMPP D0 K0 M20\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.memory.set_input(d("X1"), true);
        s.memory.set_word(d("D0"), -1);
        s.scan(&p, 100);
        assert!(s.memory.bit(d("M12")));
        assert!(s.memory.bit(d("M22")));
        s.memory.set_word(d("D0"), 0);
        s.scan(&p, 100);
        assert!(!s.memory.bit(d("M10")));
        assert!(s.memory.bit(d("M11")));
        assert!(!s.memory.bit(d("M12")));
        assert!(s.memory.bit(d("M22")));
        assert!(!s.memory.bit(d("M21")));
        s.memory.set_input(d("X0"), false);
        s.memory.set_input(d("X1"), false);
        s.memory.set_word(d("D0"), 1);
        s.scan(&p, 100);
        assert!(s.memory.bit(d("M11")));
        assert!(s.memory.bit(d("M22")));
        s.memory.set_input(d("X1"), true);
        s.scan(&p, 100);
        assert!(s.memory.bit(d("M20")));
        assert!(!s.memory.bit(d("M21")));
        assert!(!s.memory.bit(d("M22")));
    }
    #[test]
    fn comparison_contacts_are_signed_and_compose_with_logic() {
        for (suffix, expected) in [
            ("=", false),
            (">", false),
            ("<", true),
            ("<>", true),
            ("<=", true),
            (">=", false),
        ] {
            let p=compile(&format!("LD{suffix} H8000 H7FFF\nOUT Y0\nLD X0\nAND{suffix} H8000 H7FFF\nOUT Y1\nLD X0\nOR{suffix} H8000 H7FFF\nOUT Y2\nEND")).unwrap();
            let mut s = Simulator::default();
            s.scan(&p, 100);
            assert_eq!(s.memory.bit(d("Y0")), expected, "LD{suffix}");
            assert!(!s.memory.bit(d("Y1")));
            assert_eq!(s.memory.bit(d("Y2")), expected, "OR{suffix}");
            s.memory.set_input(d("X0"), true);
            s.scan(&p, 100);
            assert_eq!(s.memory.bit(d("Y1")), expected, "AND{suffix}");
            assert!(s.memory.bit(d("Y2")));
        }
    }
    #[test]
    fn zone_compare_signed_ranges_inclusive_and_reversed() {
        let p = compile("LD X0\nZCP D0 D1 D2 M0\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        for (low, high, value, expected) in [
            (-5, 5, -6, 0),
            (-5, 5, -5, 1),
            (-5, 5, 5, 1),
            (-5, 5, 6, 2),
            (-32768, 32767, -32768, 1),
            (-32768, 32767, 32767, 1),
            (7, -3, 0, 0),
            (7, -3, 7, 1),
            (7, -3, 8, 2),
            (-2, -2, -3, 0),
            (-2, -2, -2, 1),
            (-2, -2, -1, 2),
        ] {
            s.memory.set_word(d("D0"), low);
            s.memory.set_word(d("D1"), high);
            s.memory.set_word(d("D2"), value);
            s.scan(&p, 100);
            for bit in 0..3 {
                assert_eq!(
                    s.memory.bit(d(&format!("M{bit}"))),
                    bit == expected,
                    "bounds {low}..{high}, value {value}, bit {bit}"
                );
            }
        }
        s.memory.set_input(d("X0"), false);
        s.memory.set_word(d("D2"), -2);
        s.scan(&p, 100);
        assert!(
            s.memory.bit(d("M2")),
            "disabled instruction must retain results"
        );
    }
    #[test]
    fn zone_pulse_history_belongs_to_each_occurrence_and_rearms() {
        let p = compile("LD X0\nZCPP K0 K1 D0 M0\nLD X1\nZCPP K0 K1 D0 M10\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_word(d("D0"), -1);
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert!(s.memory.bit(d("M0")));
        s.memory.set_word(d("D0"), 1);
        s.memory.set_input(d("X1"), true);
        s.scan(&p, 100);
        assert!(s.memory.bit(d("M0")));
        assert!(s.memory.bit(d("M11")));
        s.memory.set_input(d("X0"), false);
        s.scan(&p, 100);
        s.memory.set_input(d("X0"), true);
        s.memory.set_word(d("D0"), 2);
        s.scan(&p, 100);
        assert!(s.memory.bit(d("M2")));
        assert!(!s.memory.bit(d("M0")));
        assert!(s.memory.bit(d("M11")));
    }
    #[test]
    fn zone_compiler_tracks_implicit_bits_and_all_three_sources() {
        let p =
            compile("LD X0\nZCPP D4999 T0 C199 Y375\nZCP K0 K0 K0 M4093\nZCP K0 K0 K0 S1021\nEND")
                .unwrap();
        for name in [
            "D4999", "T0", "C199", "Y375", "Y376", "Y377", "M4093", "M4094", "M4095", "S1021",
            "S1022", "S1023",
        ] {
            assert!(p.devices.contains(&d(name)), "{name}");
        }
        assert_eq!(p.networks[0].actions[0].label(), "ZCPP D4999 T0 C199 Y375");
        for command in [
            "ZCP K0 K1 K2 X0",
            "ZCP K0 K1 K2 D0",
            "ZCP K0 K1 K2 Y376",
            "ZCP K0 K1 K2 M998",
            "ZCP K0 K1 K2 M4094",
            "ZCP K0 K1 K2 S1022",
            "ZCPP C200 K0 K1 M0",
            "ZCP K0 C200 K1 M0",
            "ZCP K0 K1 C200 M0",
            "ZCP K0 K1 M0",
            "DZCP K0 K1 M0",
        ] {
            assert!(
                compile(&format!("LD X0\n{command}\nEND")).is_err(),
                "{command}"
            );
        }
    }
    #[test]
    fn cmp_result_ranges_and_comparison_sources_are_tracked() {
        let p =
            compile("LD= D4999 C199\nCMP T0 D4096 Y375\nCMPP K0 D4999 M4093\nCMP K0 K0 S1021\nEND")
                .unwrap();
        for name in [
            "D4999", "C199", "T0", "D4096", "Y375", "Y376", "Y377", "M4093", "M4094", "M4095",
            "S1021", "S1022", "S1023",
        ] {
            assert!(p.devices.contains(&d(name)), "{name}");
        }
        assert_eq!(p.networks[0].actions[0].label(), "CMP T0 D4096 Y375");
    }
    #[test]
    fn invalid_comparison_ranges_widths_and_arity_are_rejected() {
        for command in [
            "CMP K1 K2 X0",
            "CMP K1 K2 D0",
            "CMP K1 K2 Y376",
            "CMP K1 K2 M4094",
            "CMP K1 K2 S1022",
            "CMP K1 K2 M998",
            "CMPP K1 C200 M0",
            "CMP K1 M0",
        ] {
            assert!(
                compile(&format!("LD X0\n{command}\nEND")).is_err(),
                "{command}"
            );
        }
        for command in [
            "LD= C200 K0",
            "LD== K0 K0",
            "LD= K0",
            "LD=P K0 K0",
            "LD= X0 K0",
        ] {
            assert!(
                compile(&format!("{command}\nOUT Y0\nEND")).is_err(),
                "{command}"
            );
        }
    }
    #[test]
    fn branch_saved_value_survives_internal_writes_and_repeated_reads() {
        let p = compile(
            "LD X0\nOUT M0\nLD M0\nMPS\nRST M0\nMRD\nOUT Y0\nMRD\nOUT Y1\nMPP\nOUT Y2\nEND",
        )
        .unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert!(!s.memory.bit(d("M0")));
        for name in ["Y0", "Y1", "Y2"] {
            assert!(s.memory.bit(d(name)));
        }
        s.memory.set_input(d("X0"), false);
        s.scan(&p, 100);
        for name in ["Y0", "Y1", "Y2"] {
            assert!(!s.memory.bit(d(name)));
        }
    }
    #[test]
    fn branch_capture_after_output_does_not_resample_the_prefix() {
        let p = compile("LD X0\nOUT M0\nLD M0\nRST M0\nMPS\nOUT Y0\nMPP\nOUT Y1\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert!(!s.memory.bit(d("M0")));
        assert!(s.memory.bit(d("Y0")));
        assert!(s.memory.bit(d("Y1")));
    }
    #[test]
    fn nested_branch_restore_is_lifo_and_prefix_only_inputs_are_tracked() {
        let p = compile("LD X0\nMPS\nAND X1\nMPS\nANI X2\nOUT Y0\nMPP\nOUT Y1\nMPP\nOUT Y2\nEND")
            .unwrap();
        assert!(p.devices.contains(&d("X0")));
        assert!(p.has_branches());
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert!(!s.memory.bit(d("Y0")));
        assert!(!s.memory.bit(d("Y1")));
        assert!(s.memory.bit(d("Y2")));
        s.memory.set_input(d("X1"), true);
        s.memory.set_input(d("X2"), true);
        s.scan(&p, 100);
        assert!(!s.memory.bit(d("Y0")));
        assert!(s.memory.bit(d("Y1")));
        assert!(s.memory.bit(d("Y2")));
    }
    #[test]
    fn branch_stack_validation_is_bounded_and_balanced() {
        for source in [
            "MPS\nEND",
            "LD X0\nMRD\nOUT Y0\nEND",
            "LD X0\nMPP\nOUT Y0\nEND",
            "LD X0\nMPS\nOUT Y0\nEND",
            "LD X0\nMPS X1\nMPP\nEND",
            "LD X0\nLD X1\nMPS\nANB\nOUT Y0\nMPP\nEND",
        ] {
            assert!(compile(source).is_err(), "{source}");
        }
        let source = format!("LD X0\n{}{}END", "MPS\n".repeat(9), "MPP\n".repeat(9));
        assert!(compile(&source).unwrap_err().message.contains("8"));
        let valid = format!("LD X0\n{}{}END", "MPS\n".repeat(8), "MPP\n".repeat(8));
        assert!(compile(&valid).is_ok());
        let sequential = format!("{}END", "LD X0\nMPS\nOUT Y0\nMPP\nOUT Y1\n".repeat(10));
        assert!(compile(&sequential).is_ok());
    }
    #[test]
    fn edge_contacts_first_scan_hold_fall_and_rearm() {
        let p = compile("LDP X0\nOUT Y0\nLDF X0\nOUT Y1\nEND").unwrap();
        let mut s = Simulator::default();
        for (input, rise, fall) in [
            (false, false, false),
            (true, true, false),
            (true, false, false),
            (false, false, true),
            (false, false, false),
            (true, true, false),
        ] {
            s.memory.set_input(d("X0"), input);
            s.scan(&p, 100);
            assert_eq!((s.memory.bit(d("Y0")), s.memory.bit(d("Y1"))), (rise, fall));
        }
        let mut first = Simulator::default();
        first.memory.set_input(d("X0"), true);
        first.scan(&p, 100);
        assert!(first.memory.bit(d("Y0")));
    }
    #[test]
    fn edge_history_updates_behind_false_and_true_or_branches() {
        let p = compile("LD X0\nANDP X1\nOUT Y0\nLD X0\nORP X1\nOUT Y1\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X1"), true);
        s.scan(&p, 100);
        assert!(!s.memory.bit(d("Y0")));
        assert!(s.memory.bit(d("Y1")));
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert!(!s.memory.bit(d("Y0")));
        s.memory.set_input(d("X1"), false);
        s.scan(&p, 100);
        s.memory.set_input(d("X1"), true);
        s.scan(&p, 100);
        s.memory.set_input(d("X0"), false);
        s.scan(&p, 100);
        assert!(!s.memory.bit(d("Y1")));
    }
    #[test]
    fn result_edge_is_distinct_from_operand_edge() {
        let p = compile(
            "LD X0\nAND X1\nNP\nOUT Y0\nLD X0\nAND X1\nPN\nOUT Y1\nLD X0\nANDP X1\nOUT Y2\nEND",
        )
        .unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X1"), true);
        s.scan(&p, 100);
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert!(s.memory.bit(d("Y0")));
        assert!(!s.memory.bit(d("Y2")));
        s.memory.set_input(d("X0"), false);
        s.scan(&p, 100);
        assert!(s.memory.bit(d("Y1")));
    }
    #[test]
    fn edge_render_reads_do_not_consume_pulses_and_clone_keeps_history() {
        let p = compile("LDP X0\nOUT Y0\nLDP X0\nOUT Y1\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        for _ in 0..3 {
            assert!(p.networks[0].condition.evaluate(&s.memory));
            assert!(p.networks[1].condition.evaluate(&s.memory));
        }
        let mut s = s.clone();
        s.scan(&p, 100);
        assert!(!s.memory.bit(d("Y0")));
        assert!(!s.memory.bit(d("Y1")));
    }
    #[test]
    fn edge_syntax_rejects_numeric_contacts_and_bad_arity() {
        for body in [
            "LDP D0\nOUT Y0",
            "LDF K1\nOUT Y0",
            "LD X0\nANDP D0\nOUT Y0",
            "LD X0\nORF X1 X2\nOUT Y0",
            "LD X0\nNP X1\nOUT Y0",
            "NP\nOUT Y0",
            "LD X0\nOUT Y0\nPN",
        ] {
            assert!(compile(&format!("{body}\nEND")).is_err(), "{body}");
        }
    }
    #[test]
    fn manual_word_boundaries_and_disabled_writes() {
        let p = compile("LD X0\nINC D0\nDEC D1\nNEG D2\nNEG D3\nEND").unwrap();
        let mut s = Simulator::default();
        for (name, value) in [("D0", 32767), ("D1", -32768), ("D2", -32768), ("D3", -123)] {
            s.memory.set_word(d(name), value);
        }
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D0")), 32767);
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert_eq!(
            [
                s.memory.word(d("D0")),
                s.memory.word(d("D1")),
                s.memory.word(d("D2")),
                s.memory.word(d("D3"))
            ],
            [-32768, 32767, -32768, 123]
        );
    }
    #[test]
    fn word_logic_reads_sources_before_writing_aliases() {
        let p=compile("LD X0\nWAND HA55A H0FF0 D0\nWOR HA55A H0FF0 D1\nWXOR HA55A H0FF0 D2\nCML HA55A D3\nWXOR D3 HFFFF D3\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D0")) as u16, 0x0550);
        assert_eq!(s.memory.word(d("D1")) as u16, 0xaffa);
        assert_eq!(s.memory.word(d("D2")) as u16, 0xaaaa);
        assert_eq!(s.memory.word(d("D3")) as u16, 0xa55a);
    }
    #[test]
    fn pulses_are_per_occurrence_and_rearm_only_when_false() {
        let p = compile("LD X0\nINCP D0\nINCP D0\nLD X1\nINCP D0\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.memory.set_input(d("X1"), true);
        for _ in 0..3 {
            s.scan(&p, 100);
        }
        assert_eq!(s.memory.word(d("D0")), 3);
        s.memory.set_input(d("X0"), false);
        s.scan(&p, 100);
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D0")), 5);
        s.memory.set_input(d("X1"), false);
        s.scan(&p, 100);
        s.memory.set_input(d("X1"), true);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D0")), 6);
    }
    #[test]
    fn movp_samples_source_on_edge_and_clone_preserves_edge_state() {
        let p = compile("LD X0\nINC D0\nMOVP D0 D1\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        let mut s = s.clone();
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D0")), 2);
        assert_eq!(s.memory.word(d("D1")), 1);
        s.memory.set_input(d("X0"), false);
        s.scan(&p, 100);
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("D1")), 3);
    }
    #[test]
    fn numeric_parser_keeps_operand_widths_and_tracks_pulse_sources() {
        for body in [
            "INC X0",
            "INC D0 D1",
            "NEG K1",
            "CML K1 Y0",
            "WAND D0 K1",
            "WXOR C200 K1 D0",
            "CMLP C255 D0",
            "INCPP D0",
        ] {
            assert!(compile(&format!("LD X0\n{body}\nEND")).is_err(), "{body}");
        }
        let p = compile("LD X0\nWANDP D4096 T0 D4999\nCMLP C199 D2\nEND").unwrap();
        for name in ["X0", "D4096", "T0", "D4999", "C199", "D2"] {
            assert!(p.devices.contains(&d(name)), "{name}");
        }
        assert_eq!(p.networks[0].actions[0].label(), "WANDP D4096 T0 D4999");
    }
    #[test]
    fn reset_before_counter_inhibits_the_same_scan_edge() {
        let p = compile("LD X1\nRST C0\nLD X0\nCNT C0 K3\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.memory.set_input(d("X1"), true);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("C0")), 0);
        s.memory.set_input(d("X1"), false);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("C0")), 0);
        s.memory.set_input(d("X0"), false);
        s.scan(&p, 100);
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("C0")), 1);
    }
    #[test]
    fn t127_has_1ms_base_and_end_of_scan_counting() {
        let p = compile("LD X0\nTMR T127 K150\nLD T127\nOUT Y0\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("T127")), 100);
        assert!(!s.memory.bit(d("Y0")));
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("T127")), 200);
        assert!(!s.memory.bit(d("Y0")));
        s.scan(&p, 100);
        assert_eq!(s.memory.word(d("T127")), 150);
        assert!(s.memory.bit(d("Y0")));
    }
    #[test]
    fn octal_addresses_and_reserved_relays() {
        assert_eq!(d("X10").index, 8);
        assert_eq!(d("Y377").index, 255);
        assert!("X8".parse::<Device>().is_err());
        assert!("X400".parse::<Device>().is_err());
        assert!(!"M1000".parse::<Device>().unwrap().writable_bit());
        assert!("D10000".parse::<Device>().is_err());
        assert!("D+1".parse::<Device>().is_err());
    }
    #[test]
    fn motor_latches_and_stop_has_priority() {
        let p = compile("LD X0\nOR Y0\nANI X1\nOUT Y0\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 20);
        assert!(s.memory.bit(d("Y0")));
        s.memory.set_input(d("X0"), false);
        s.scan(&p, 20);
        assert!(s.memory.bit(d("Y0")));
        s.memory.set_input(d("X1"), true);
        s.scan(&p, 20);
        assert!(!s.memory.bit(d("Y0")));
    }
    #[test]
    fn timer_accumulates_subtick_time_and_resets() {
        let p = compile("LD X0\nTMR T0 K3\nLD T0\nOUT Y0\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        for _ in 0..14 {
            s.scan(&p, 20);
        }
        assert!(!s.memory.bit(d("Y0")));
        s.scan(&p, 20);
        assert_eq!(s.memory.word(d("T0")), 3);
        assert!(!s.memory.bit(d("Y0")));
        s.scan(&p, 20);
        assert!(s.memory.bit(d("Y0")));
        s.memory.set_input(d("X0"), false);
        s.scan(&p, 20);
        assert_eq!(s.memory.word(d("T0")), 0);
        assert!(!s.memory.bit(d("Y0")));
    }
    #[test]
    fn counter_counts_edges_and_rst_does_not_retrigger_held_input() {
        let p = compile("LD X0\nCNT C0 K2\nLD X1\nRST C0\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        for _ in 0..4 {
            s.scan(&p, 20);
        }
        assert_eq!(s.memory.word(d("C0")), 1);
        s.memory.set_input(d("X0"), false);
        s.scan(&p, 20);
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 20);
        assert!(s.memory.bit(d("C0")));
        s.memory.set_input(d("X1"), true);
        s.scan(&p, 20);
        s.memory.set_input(d("X1"), false);
        s.scan(&p, 20);
        assert_eq!(s.memory.word(d("C0")), 0);
    }
    #[test]
    fn es2_signed_overflow_and_sequential_writes() {
        let p = compile("LD X0\nMOV H7FFF D0\nADD D0 K1 D1\nSUB D1 K1 D2\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X0"), true);
        s.scan(&p, 20);
        assert_eq!(s.memory.word(d("D1")), 0);
        assert_eq!(s.memory.word(d("D2")), -1);
    }
    #[test]
    fn blocks_combine_in_order() {
        let p = compile("LD X0\nAND X1\nLD X2\nANI X3\nORB\nOUT Y0\nEND").unwrap();
        let mut s = Simulator::default();
        s.memory.set_input(d("X2"), true);
        s.scan(&p, 20);
        assert!(s.memory.bit(d("Y0")));
        s.memory.set_input(d("X3"), true);
        s.scan(&p, 20);
        assert!(!s.memory.bit(d("Y0")));
    }
    #[test]
    fn invalid_programs_never_execute() {
        for source in [
            "LD X0\nOUT X1\nEND",
            "LD X0\nOUT Y0",
            "LD X0\nEND",
            "LD X0\nTMR T256 K10\nEND",
            "LD X0\nCNT C200 K1\nEND",
            "LD X0\nOUT Y0\nEND\nSET M0",
            "LD X0\nLD X1\nOUT Y0\nEND",
            "LD X0\nOUT Y0\nAND X1\nEND",
            "LD X0\nTMR T0 H10\nEND",
        ] {
            assert!(compile(source).is_err(), "{source}");
        }
    }
}
